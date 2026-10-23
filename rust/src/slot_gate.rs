//! §02-services.md §1's first sentence, the half nothing else can do: "each model is given at most its
//! Settings slot count of requests at once ([09 §4](spec/09-llm-and-tools.md#4-liveness-and-the-gate-f63))".
//!
//! [`crate::llm_liveness::Gate`] already *is* that rule — slots, a first-come-first-served queue, an
//! automatic hand-off on release. What it never had was a caller on a dial path: a gate built by a test
//! stops nothing. This module is the wiring: one gate per (server, model), taken before the socket is
//! opened and released when the reply is in, so a second request to a busy model waits here rather than
//! joining a queue inside the server that this app cannot see or time.
//!
//! **One gate per model, not per server.** `llm_liveness`'s own header says two models on one server are
//! two slot counts: the Settings rows are per model (`AUDIOCPP_ASR_SLOTS`, `AUDIOCPP_TTS_SLOTS`, …) and
//! the server may hold more slots than this app is allowed to use, or serve someone else entirely. So
//! the key carries both halves — same model on two servers is two gates too, because the box that names
//! each is a different machine.
//!
//! The wait is reported, not swallowed: `requests.tsv` has a "how long it waited for a slot" column
//! (§09 §10) precisely because that number is how slot counts get chosen, so a row written after a wait
//! must carry it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::llm_liveness::{Acquisition, Gate};

/// How often a blocked asker re-reads the gate. Short enough that a release is picked up promptly, long
/// enough that a queue of waiting steps costs nothing: this is a poll of a tiny `Vec`, not a syscall.
const POLL: Duration = Duration::from_millis(20);

/// A held slot. Dropping it gives the slot back, which is what makes a leak impossible on the happy path
/// and visible on the unhappy one: the holder lives in the caller's scope, so an early `?` releases just
/// as surely as a normal return.
#[derive(Debug)]
pub struct Held {
    /// The gate this slot belongs to, plus who is holding it. Kept behind a box so the guard itself is
    /// one pointer wide: it sits on the caller's stack between taking the slot and sending, and an
    /// inline key (two `String`s) was enough to overflow an already-deep UI stack before a request was
    /// ever dialled.
    inner: Box<Guard>,
}

/// What a held slot has to remember in order to give itself back.
#[derive(Debug)]
struct Guard {
    url: String,
    model: String,
    step: String,
}

impl Drop for Held {
    fn drop(&mut self) {
        release(
            &(self.inner.url.clone(), self.inner.model.clone()),
            &self.inner.step,
        );
    }
}

/// Which machine and which model: the gate's identity, as §1's table plus the Settings row name it.
type Key = (String, String);

/// Why a request did not get its slot: ⏹ landed while it waited (§4's "the wait is cancellable by ⏹").
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

/// Ask for a slot on `(url, model)`'s gate and wait until it is ours.
///
/// `slots` is the Settings value for this model ([`crate::settings::Slots`]'s field for the task being
/// dialled), clamped by the gate itself to §4's range. `holder` is the step name: it is who the gate
/// queues and hands over to, and what its log line would name.
///
/// Returns the guard and the seconds spent waiting — `0.0` when a slot was free on arrival, which is the
/// common case and the reason the wait is a return value rather than something only a log sees.
///
/// While every slot is busy the ask retries, and §4's once-only line is the GATE's to write: `acquire`
/// hands back [`Acquisition::Waiting`] with the sentence (empty when this step is only queued behind its
/// own earlier call, and empty again after the first telling), and this puts a non-empty one on the log.
/// Re-implementing "once" here would give the rule two homes.
pub fn hold(
    url: &str,
    model: &str,
    slots: u32,
    holder: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<(Held, f64), Wait> {
    let started = Instant::now();
    // The queue is keyed by step name, so two requests from one step must not share an identity: the
    // second would be told it already holds a slot (it does — the first one's) and go out over count.
    // A per-ask suffix keeps each ask its own holder while leaving the name the gate logs readable.
    let holder = format!(
        "{holder}#{}",
        ASK.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    );
    loop {
        match acquire(&(url.to_string(), model.to_string()), slots, &holder) {
            Acquisition::Taken => {
                return Ok((
                    Held {
                        inner: Box::new(Guard {
                            url: url.to_string(),
                            model: model.to_string(),
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
        // Every slot is busy: a wait, not a failure — §4's whole point is that a queued request is a
        // normal state of a run. ⏹ ends it, and the asker drops its own place in the queue on the way out,
        // so a step that quits leaves nothing behind for the next release to hand a slot to.
        if cancelled() {
            cancel_waiting(&(url.to_string(), model.to_string()), &holder);
            return Err(Wait::Cancelled);
        }
        std::thread::sleep(POLL);
    }
}

/// Counter behind the per-ask holder suffix; see [`hold`].
static ASK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// Gates by (server, model). Each gate carries its own lock so a step waiting on one model never blocks
/// a step asking about another; see [`acquire`] for the order the two locks are taken in.
/// Gates by (server, model), each behind its own mutex.
///
/// This is deliberately NOT one lock over the whole registry: an asker polls while every slot is busy, and
/// if that poll held the registry's only lock, nothing could release a slot meanwhile — a request that has
/// to be answered before it can finish would never be answered, and the run would sit there forever with
/// one thread spinning on a lock its own server is waiting on. So the outer map is only ever locked for
/// the moment it takes to find or create a gate, and the wait happens on that gate's own lock alone.
fn registry() -> &'static Mutex<HashMap<Key, Slot>> {
    static REGISTRY: OnceLock<Mutex<HashMap<Key, Slot>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One gate plus the slot count it was built with: a Settings change mid-run rebuilds it, because a
/// user who raises a slot count while a run is on means it.
#[derive(Clone)]
struct Slot {
    slots: u32,
    gate: Arc<Mutex<Gate>>,
}

impl Slot {
    fn new(model: &str, slots: u32) -> Self {
        Slot {
            slots,
            gate: Arc::new(Mutex::new(Gate::new(model, slots))),
        }
    }
}

/// The slot answer for one ask: `Taken`, or `Waiting` carrying the gate's once-only sentence.
///
/// Lock order, always in this direction: the map first to find the gate, then the gate — and the map's
/// lock is dropped before the gate is waited on. A blocked asker therefore holds nothing a release needs,
/// which is what keeps "wait for a free slot" from being a wait that can never end.
fn acquire(key: &Key, slots: u32, holder: &str) -> Acquisition {
    let gate = {
        let mut map = registry().lock().expect("the gate map");
        let slot = map
            .entry(key.clone())
            .or_insert_with(|| Slot::new(&key.1, slots));
        if slot.slots != slots {
            // Rebuilt rather than resized: `Gate` has no resize, and rebuilding keeps its own rules
            // (clamping included) as the single source of truth.
            *slot = Slot::new(&key.1, slots);
        }
        slot.gate.clone()
    };
    let mut gate = gate.lock().expect("the gate");
    gate.acquire(holder)
}

/// ⏹ during a wait: drop this asker from the queue so a later release does not hand a slot to a step that
/// has already given up. No-op when the gate does not exist yet.
fn cancel_waiting(key: &Key, holder: &str) {
    let found = registry()
        .lock()
        .expect("the gate map")
        .get(key)
        .map(|slot| slot.gate.clone());
    if let Some(gate) = found {
        gate.lock().expect("the gate").cancel_waiting(holder);
    }
}

/// Give a slot back. The gate is found under the map's lock and released under its own, in that order and
/// never the other way round.
fn release(key: &Key, holder: &str) {
    let found = registry()
        .lock()
        .expect("the gate map")
        .get(key)
        .map(|slot| slot.gate.clone());
    if let Some(gate) = found {
        gate.lock().expect("the gate").release(holder);
    }
}

/// Who is on the wire for this (server, model), if anything has ever asked about it.
fn held_by(map: &HashMap<Key, Slot>, key: &Key) -> Vec<String> {
    map.get(key)
        .map(|slot| slot.gate.lock().expect("the gate").holders())
        .unwrap_or_default()
}

/// Every (server, model) the registry holds a gate for, with who is on it — the diagnostic that tells
/// "no such gate" from "gate with nothing held", which `holders` alone cannot distinguish.
fn keys(map: &HashMap<Key, Slot>) -> Vec<(String, String, Vec<String>)> {
    map.iter()
        .map(|((url, model), _)| {
            (
                url.clone(),
                model.clone(),
                held_by(map, &(url.clone(), model.clone())),
            )
        })
        .collect()
}

/// The slot count for one audio task, read from the settings the caller already resolved.
///
/// The mapping is §6's Settings rows: ASR, diarization, alignment, TTS and separation each have their
/// own count because each is a different model with different memory, and the LLM has its own box.
/// The slot count for one audio task, read from the settings the caller already resolved.
///
/// The mapping is §6's Settings rows: ASR, diarization, alignment, TTS and separation each have their
/// own count because each is a different model with different memory, and the LLM has its own box.
/// Only the WORD here is this function's: the key -> field table is [`crate::services::slots_for`]'s,
/// so the dialog's spin and the gate that holds a request read one answer rather than two copies.
pub fn slots_for_task(conf: &crate::settings::Conf, task: &str) -> u32 {
    let wanted = match task {
        "diarize" => crate::services::AudioTask::Diar,
        "align" => crate::services::AudioTask::Align,
        "tts" => crate::services::AudioTask::Tts,
        "separate" => crate::services::AudioTask::Sep,
        _ => crate::services::AudioTask::Asr,
    };
    crate::services::slots_for(conf, wanted)
}

/// How many requests to `(url, model)` may be on the wire at once, as this build currently reads it.
/// Exposed for the tests that need to know whether a gate exists yet — a released gate keeps its queue
/// empty, so an unheld gate answers "nothing held", which is what proves the release landed.
pub fn holders(url: &str, model: &str) -> Vec<String> {
    let key = (url.to_string(), model.to_string());
    held_by(&registry().lock().expect("the gate map"), &key)
}

/// Every gate the registry holds, as (server, model, who is on it). For a test that needs to know what
/// the door actually keyed its gate on rather than what the caller thinks it typed.
pub fn gates() -> Vec<(String, String, Vec<String>)> {
    keys(&registry().lock().expect("the gate map"))
}
