//! §02-services.md §1: "each server is given one of this app's requests at a time; requests to different
//! servers run side by side" ([09 §4](spec/09-llm-and-tools.md#4-liveness-and-the-gate-f63)).
//!
//! [`crate::llm_liveness::Gate`] is the queue; this module is the wiring: one gate per server, taken before the socket opens and released when the reply is in, so a second request to a busy server waits here
//! rather than in a queue inside the server this app cannot see or time. Different servers run side by side; two LLM jobs take turns.
//!
//! Keyed by the server's address only: a per-model count in Settings was removed, since models on one GPU share it. The wait is reported in `requests.tsv`'s "how long it waited" column (§09 §10).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::llm_liveness::{Acquisition, Gate};

/// How often a blocked asker re-reads the gate: a poll of a tiny `Vec`, short enough to pick up a release promptly.
const POLL: Duration = Duration::from_millis(20);

/// A held server. Dropping it gives the server back, so an early `?` releases as surely as a normal return.
#[derive(Debug)]
pub struct Held {
    /// The server this turn belongs to, plus who holds it; boxed so the guard is one pointer wide on the caller's stack.
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

/// §4's prototype sentence for a step that queued behind ANOTHER step's call; a step behind its own earlier call waits silently (as `Gate` already decides for the waiting line).
pub fn waited_log(step: &str, other: &str) -> String {
    format!(">>> {step}: waited for the LLM -- it was busy with {other}; one request at a time")
}

/// Ask for `url`'s server and wait until it is ours. `holder` is the step name: it is who the gate queues
/// and hands over to, and what its log line would name.
///
/// Returns the guard and the seconds spent waiting, `0.0` when the server was free on arrival.
///
/// While busy the ask retries, and §4's once-only line is the GATE's to write: `acquire` returns [`Acquisition::Waiting`] with the sentence (empty when queued behind the step's own earlier call, or after the first telling).
pub fn hold(url: &str, holder: &str, cancelled: &dyn Fn() -> bool) -> Result<(Held, f64), Wait> {
    let started = Instant::now();
    // The queue is keyed by step name, so each ask gets a per-ask suffix: two requests from one step must not share an identity, or the second would be told it already holds the server and go out alongside it.
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
        // The server is busy: a wait, not a failure (§4). ⏹ ends it, and the asker drops its own queue place so the next release does not hand the server to a step that quit.
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
/// Deliberately NOT one lock over the whole registry: an asker polls while the server is busy, so the outer map is locked only to find or create a gate, and the wait happens on that gate's own lock.
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

/// The turn answer for one ask: `Taken`, or `Waiting` carrying the gate's once-only sentence. The map's lock is dropped before the gate is locked.
fn acquire(url: &str, holder: &str) -> Acquisition {
    let gate = gate_of(url);
    let mut gate = gate.lock().expect("the gate");
    gate.acquire(holder)
}

/// ⏹ during a wait: drop this asker from the queue so a release does not hand the server to a step that gave up.
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

/// Who is on the wire to `url` now, in arrival order; empty for an unknown server or one whose last request is in.
pub fn holders(url: &str) -> Vec<String> {
    let found = registry().lock().expect("the gate map").get(url).cloned();
    found
        .map(|gate| gate.lock().expect("the gate").holders())
        .unwrap_or_default()
}

/// Every server the registry holds a gate for, with who is on it: tells "no such gate" from "gate with nothing held".
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
