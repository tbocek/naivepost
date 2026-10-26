//! §02-services.md §1: "each server is given one of this app's requests at a time; requests to different
//! servers run side by side" ([09 §4](spec/09-llm-and-tools.md#4-liveness-and-the-gate-f63)).
//!
//! [`crate::llm_liveness::Gate`] is the queue: first come first served, an automatic hand-off on release,
//! a once-only line for a step that has to wait. This module is the wiring: one gate per server, taken
//! before the socket is opened and released when the reply is in, so a second request to a busy server
//! waits here rather than joining a queue inside the server that this app cannot see or time. A narration
//! speaking on audio.cpp and a title being written on the LLM run side by side; two LLM jobs take turns.
//!
//! Keyed by the server's address only. A per-model count in Settings was tried and removed: two models on
//! one GPU share it whatever a count says, and the number was one more thing to tune for no gain.
//!
//! The wait is reported, not swallowed: `requests.tsv` has a "how long it waited for its server" column
//! (§09 §10), so a row written after a wait carries it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::llm_liveness::{Acquisition, Gate};

/// How often a blocked asker re-reads the gate. Short enough that a release is picked up promptly, long
/// enough that a queue of waiting steps costs nothing: this is a poll of a tiny `Vec`, not a syscall.
const POLL: Duration = Duration::from_millis(20);

/// A held server. Dropping it gives the server back, which is what makes a leak impossible on the happy
/// path and visible on the unhappy one: the holder lives in the caller's scope, so an early `?` releases
/// just as surely as a normal return.
#[derive(Debug)]
pub struct Held {
    /// The server this turn belongs to, plus who is holding it. Kept behind a box so the guard itself is
    /// one pointer wide: it sits on the caller's stack between taking the server and sending.
    inner: Box<Guard>,
}

/// What a held turn has to remember in order to give itself back.
#[derive(Debug)]
struct Guard {
    url: String,
    step: String,
}

impl Drop for Held {
    fn drop(&mut self) {
        release(&self.inner.url, &self.inner.step);
    }
}

/// Why a request did not get its turn: ⏹ landed while it waited (§4's "the wait is cancellable by ⏹").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    Cancelled,
}

/// §4's prototype sentence for a step that queued behind ANOTHER step's call. Only a different step's
/// holder earns it: a step queued behind its own earlier call waits silently, which `Gate` already decides
/// for the waiting line and this repeats for the after-the-fact one — "publish waited for publish" says
/// nothing the two requests in flight do not already say.
pub fn waited_log(step: &str, other: &str) -> String {
    format!(">>> {step}: waited for the LLM -- it was busy with {other}; one request at a time")
}

/// Ask for `url`'s server and wait until it is ours. `holder` is the step name: it is who the gate queues
/// and hands over to, and what its log line would name.
///
/// Returns the guard and the seconds spent waiting — `0.0` when the server was free on arrival, which is
/// the common case and the reason the wait is a return value rather than something only a log sees.
///
/// While the server is busy the ask retries, and §4's once-only line is the GATE's to write: `acquire`
/// hands back [`Acquisition::Waiting`] with the sentence (empty when this step is only queued behind its
/// own earlier call, and empty again after the first telling), and this puts a non-empty one on the log.
pub fn hold(url: &str, holder: &str, cancelled: &dyn Fn() -> bool) -> Result<(Held, f64), Wait> {
    let started = Instant::now();
    // The queue is keyed by step name, so two requests from one step must not share an identity: the
    // second would be told it already holds the server (it does — the first one's) and go out alongside it.
    // A per-ask suffix keeps each ask its own holder while leaving the name the gate logs readable.
    let holder = format!(
        "{holder}#{}",
        ASK.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    );
    loop {
        match acquire(url, &holder) {
            Acquisition::Taken => {
                return Ok((
                    Held {
                        inner: Box::new(Guard {
                            url: url.to_string(),
                            step: holder,
                        }),
                    },
                    started.elapsed().as_secs_f64(),
                ))
            }
            Acquisition::Waiting { log } => {
                if !log.is_empty() {
                    crate::ui::window::log_line(&log);
                }
            }
            Acquisition::Cancelled => unreachable!("only Gate::cancel_waiting returns this"),
        }
        // The server is busy: a wait, not a failure — §4's whole point is that a queued request is a normal
        // state of a run. ⏹ ends it, and the asker drops its own place in the queue on the way out, so a
        // step that quits leaves nothing behind for the next release to hand the server to.
        if cancelled() {
            cancel_waiting(url, &holder);
            return Err(Wait::Cancelled);
        }
        std::thread::sleep(POLL);
    }
}

/// Counter behind the per-ask holder suffix; see [`hold`].
static ASK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// Gates by server, each behind its own mutex.
///
/// This is deliberately NOT one lock over the whole registry: an asker polls while the server is busy,
/// and if that poll held the registry's only lock, nothing could release the server meanwhile. So the
/// outer map is only ever locked for the moment it takes to find or create a gate, and the wait happens
/// on that gate's own lock alone.
fn registry() -> &'static Mutex<HashMap<String, Arc<Mutex<Gate>>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Mutex<Gate>>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The gate for one server: found, or made with its one turn.
fn gate_of(url: &str) -> Arc<Mutex<Gate>> {
    registry()
        .lock()
        .expect("the gate map")
        .entry(url.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(Gate::new(url, 1))))
        .clone()
}

/// The turn answer for one ask: `Taken`, or `Waiting` carrying the gate's once-only sentence. The map's
/// lock is dropped before the gate is locked, so a blocked asker holds nothing a release needs.
fn acquire(url: &str, holder: &str) -> Acquisition {
    let gate = gate_of(url);
    let mut gate = gate.lock().expect("the gate");
    gate.acquire(holder)
}

/// ⏹ during a wait: drop this asker from the queue so a later release does not hand the server to a step
/// that has already given up.
fn cancel_waiting(url: &str, holder: &str) {
    gate_of(url)
        .lock()
        .expect("the gate")
        .cancel_waiting(holder);
}

/// Give the server back.
fn release(url: &str, holder: &str) {
    gate_of(url).lock().expect("the gate").release(holder);
}

/// Who is on the wire to `url` now, in arrival order. Empty for a server nothing has asked about or one
/// whose last request is in — which is what proves a release landed.
pub fn holders(url: &str) -> Vec<String> {
    let found = registry().lock().expect("the gate map").get(url).cloned();
    found
        .map(|gate| gate.lock().expect("the gate").holders())
        .unwrap_or_default()
}

/// Every server the registry holds a gate for, with who is on it: the diagnostic that tells "no such gate"
/// from "gate with nothing held", which `holders` alone cannot distinguish.
pub fn gates() -> Vec<(String, Vec<String>)> {
    let map: Vec<(String, Arc<Mutex<Gate>>)> = registry()
        .lock()
        .expect("the gate map")
        .iter()
        .map(|(url, gate)| (url.clone(), gate.clone()))
        .collect();
    map.into_iter()
        .map(|(url, gate)| {
            let held = gate.lock().expect("the gate").holders();
            (url, held)
        })
        .collect()
}
