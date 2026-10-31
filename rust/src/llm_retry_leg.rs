//! F6.2 §3's live half: the patience every chat ask rides.
//!
//! [`crate::llm_retry`] holds §3's rules as pure functions; this puts them on the wire through [`chat`]: a
//! call that died in transit waits 5 s → 20 s → 1 min → 2 min → 4 min and asks the IDENTICAL request again;
//! anything that got an answer gets one short retry after 2 s, first failure only.
//!
//! # What counts as which
//!
//! `server_leg` returns a 4xx/5xx as `Ok(Sent)`, so the door turns `!sent.ok()` into a failure and classifies
//! it: a 5xx whose body names a device or memory failure backs off like a death (§3), every other status takes
//! the short retry. A 2xx is an answer whatever its content; rejected ANSWERS have their own counters
//! (`roles::LLM_ATTEMPTS`, `seam_retries`, `cut_captions::retries`) which this door does not touch.
//!
//! A connection refused or unreachable at connect time (`server_leg::down`'s "is not answering") rides the
//! ladder too: the restart policy leaves the port closed while weights load (§3 lists "refused" outright).
//!
//! # Cancellable
//!
//! The wait is [`nap`], which polls the session's cancel switch, so ⏹ ends a four-minute wait. A stopped run
//! reports itself through `Run::stopped`; the caller sees the last failure as the error.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::cancel_leg;
use crate::exchange_leg;
use crate::layout::Tree;
use crate::llm_retry::{self};
use crate::requests::Request;
use crate::server_leg::{self, Sent};
use crate::services::{Endpoint, Kind};

/// How long [`nap`] sleeps per slice. `cancel_leg::POLL` is private; 50 ms is plenty against a five-second floor.
const NAP_POLL: Duration = Duration::from_millis(50);

/// Wait `seconds`, giving up the moment ⏹ lands. True = the whole wait happened; false = the run was stopped.
pub fn nap(seconds: u64) -> bool {
    let left = nap_duration(seconds);
    // One deadline read fresh each slice, so a starved thread cannot make the wait longer or shorter.
    let started = Instant::now();
    while started.elapsed() < left {
        if cancel_leg::cancelled() {
            return false;
        }
        std::thread::sleep(NAP_POLL.min(left - started.elapsed()));
    }
    // A stop landing in the last slice still counts as stopped: do not report a server problem for a run the user ended.
    !cancel_leg::cancelled()
}

/// How long a §3 wait actually takes. `NAIVEPOST_RETRY_SCALE` divides it so a live test can sit through the
/// ladder in seconds; the log still names the SECONDS §3 spells. Unset in every real run.
fn nap_duration(seconds: u64) -> Duration {
    let scale = std::env::var("NAIVEPOST_RETRY_SCALE")
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|factor| *factor > 0)
        .unwrap_or(1);
    Duration::from_millis(seconds * 1000 / scale)
}

/// Either leg's failure, unified so one loop can hold both. The status and body ride along (not a string) so
/// §3's device rule is applied to the real status.
enum Failure {
    Gone(String),
    Status {
        status: u16,
        body: String,
    },
    /// ⏹ landed before this attempt was dialled: not a server problem, and no patience applies.
    Stopped,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Gone(why) => out.write_str(why),
            Failure::Stopped => {
                out.write_str("cancelled while waiting for the server to come back")
            }
            Failure::Status { status, body } => write!(out, "answered {status}: {}", body.trim()),
        }
    }
}

impl Failure {
    /// §3's two patiences applied to this failure: a 5xx naming a device, or a socket death on an endpoint that
    /// had already answered, walks the ladder. Everything else takes the one short retry.
    fn went_away(&self, endpoint: &str) -> bool {
        match self {
            Failure::Gone(why) => llm_retry::transport_death(why) && has_answered(endpoint),
            Failure::Stopped => false,
            Failure::Status { status, body } => llm_retry::device_failure(Some(*status), body),
        }
    }
}

/// Endpoints that have answered at least once since the app started.
///
/// §3's ladder is for a server that "went away mid-call". A connection REFUSED by an endpoint that never
/// replied (§02-services#1's "a service that is down gives an error") would otherwise hold a run for eight
/// minutes over a box nobody started, so the ladder is gated on having been answered before; refused with
/// no history takes the short patience and then reports the server as down.
fn answered_once() -> &'static Mutex<std::collections::HashSet<String>> {
    static ANSWERED: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    ANSWERED.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Has this endpoint ever answered? A poisoned lock only means a check panicked mid-call; the table is still ours.
fn has_answered(endpoint: &str) -> bool {
    answered_once()
        .lock()
        .map(|seen| seen.contains(endpoint))
        .unwrap_or(false)
}

/// Note that this endpoint answered, so a later death of it counts as mid-call.
///
/// Public because the table is process-wide, keyed by URL: a wire test wanting mid-call history on its own
/// port would otherwise need an answered call on the very fake scripted to die. Only tests should call it.
pub fn note_answer(endpoint: &str) {
    if let Ok(mut seen) = answered_once().lock() {
        seen.insert(endpoint.to_string());
    }
}

/// Forget that this endpoint ever answered; the mirror of [`note_answer`].
///
/// The table is process-wide and a test's dead server sits on an EPHEMERAL port that an earlier scenario in
/// the same binary may have been answered on, which would spend §3's ladder where it should cost nothing.
///
/// Named `_for_test` because only a proof needs it: an endpoint that answered keeps its history for the run.
pub fn forget_answer_for_test(endpoint: &str) {
    if let Ok(mut seen) = answered_once().lock() {
        seen.remove(endpoint);
    }
}

/// Ask the LLM for one chat turn and keep asking until it answers, the patience runs out, or ⏹ says otherwise.
///
/// The door every chat ask goes through: it writes a timed row per dial (§09 §10), logs each wait as §3
/// spells it, and returns the `Sent` unchanged for `llm_request::reply_from`. `step` names the job in the
/// log line (§3's `>>> <step>` / `!!! <step>`). The waits are [`llm_retry::BACKOFF_SECONDS`], long by design:
/// the container has to stop, come back and read its weights.
pub fn chat(
    step: &str,
    tree: &Tree,
    base: &Request,
    endpoint: &Endpoint,
    body: &str,
    cancelled: &dyn Fn() -> bool,
    log: &dyn Fn(&str),
) -> Result<Sent, String> {
    chat_on(step, tree, base, endpoint, body, cancelled, log, &nap)
}

/// A holder that is NOT this step. Holder names carry a per-ask `#N` suffix (`slot_gate::hold`), so the
/// comparison is on the step prefix; only the first such holder is named, as the log lists every holder.
pub fn other_holder(step: &str, holders: &[String]) -> Option<String> {
    holders
        .iter()
        .map(|holder| holder.split('#').next().unwrap_or(holder.as_str()))
        .find(|holder| *holder != step)
        .map(str::to_string)
}

/// [`chat`] with the wait supplied, so a test can watch the patience without living through eight minutes.
/// The program calls [`chat`], which passes §3's own ladder and [`nap`].
///
/// Every request is filed in the run's exchange page on its way past [`dial`], so no chat ask records
/// anything itself (§7: "recording never fails the call").
#[allow(clippy::too_many_arguments)]
pub fn chat_on(
    step: &str,
    tree: &Tree,
    base: &Request,
    endpoint: &Endpoint,
    body: &str,
    cancelled: &dyn Fn() -> bool,
    log: &dyn Fn(&str),
    sleep: &dyn Fn(u64) -> bool,
) -> Result<Sent, String> {
    // A plain ask treats every failure as retryable.
    dial(step, tree, base, endpoint, body, cancelled, log, sleep).answer
}

/// File one dialled chat call in the run's exchange page (§7), and hand its log lines to the sink the
/// caller writes its §3 patience lines to.
///
/// Called from all three endings of a dial (answered, refused, never answered); the last still belongs on the
/// page, with the failure's wording as its error and no reply.
///
/// `streamed` is decided by the response's `content-type` (§9 §1: only `text/event-stream`). Chat asks read
/// the body to its end first, so the section is closed straight away rather than left open to grow.
#[allow(clippy::too_many_arguments)]
fn record_dial(
    step: &str,
    tree: &Tree,
    model: &str,
    body: &str,
    reply: Result<&Sent, &str>,
    started: Instant,
    log: &dyn Fn(&str),
    live: &exchange_leg::Live,
) {
    // Decided by the body, not by what arrived: the page's live section was opened (or not) when the request
    // went out and the two must agree. `asked_for_live` is that same decision on the request side.
    let streamed = live.asked_for_live();
    let took = started.elapsed().as_secs();
    let (reply_body, content_type, failure) = match reply {
        // A refused status is the failure this dial ended with, in the words `Failure::Status` prints.
        Ok(sent) => (
            sent.body.as_str(),
            sent.content_type.as_str(),
            sent.ok()
                .then(String::new)
                .unwrap_or_else(|| format!("answered {}: {}", sent.status, sent.body.trim())),
        ),
        // A call with no reply still goes on the page, with the failure's wording.
        Err(why) => ("", "", why.to_string()),
    };
    let problem = (!failure.is_empty()).then_some(failure);
    // A whole stream is read back through `call_of`'s stream parser, which keeps thinking and tool calls on the
    // finished section. A call that ended without one shows the words the hook decoded, or else the raw body.
    let streamed_words = live.streamed_words();
    let partial = streamed
        && !streamed_words.is_empty()
        && !crate::llm_request::is_event_stream(content_type);
    let shown_reply = if partial {
        streamed_words.as_str()
    } else {
        reply_body
    };
    let shown_type = if partial {
        exchange_leg::WORDS_AS_RECEIVED
    } else {
        content_type
    };
    let call = exchange_leg::call_of(
        step,
        model,
        body,
        shown_reply,
        shown_type,
        took,
        problem.clone(),
    );
    // A streamed call is closed via `live.finish` (§7's other ending); anything else files the whole exchange.
    if streamed {
        live.finish(shown_reply, shown_type, took, problem, log);
    } else {
        exchange_leg::record(tree, step, &call, log);
    }
}

/// What one dialled chat call came to: the answer or the reason there is none, plus how many times the wire
/// was touched, so "asked once and refused" differs from "waited through the ladder".
pub struct Dialled {
    pub answer: Result<Sent, String>,
    /// Requests that actually went out; §09 §10 counts one timed row per one.
    pub attempts: u32,
}

/// [`chat_on`], the door itself.
///
/// Two things stop here before §3's patience: a 4xx refusing the `tools` field is not transient (F6.1 resends
/// without the field), so it is handed back after a single dial ([`crate::degraded::refused_answer`] decides);
/// and a socket death only walks the ladder when this endpoint has answered before ([`has_answered`]).
#[allow(clippy::too_many_arguments)]
pub fn dial(
    step: &str,
    tree: &Tree,
    base: &Request,
    endpoint: &Endpoint,
    body: &str,
    cancelled: &dyn Fn() -> bool,
    log: &dyn Fn(&str),
    sleep: &dyn Fn(u64) -> bool,
) -> Dialled {
    let mut attempts = 0u32;
    // One dial, the answer read as it arrived, so a caller with its own next move (F6.1's resend-without-tools) sees it untouched.
    let mut one = || -> Result<Sent, Failure> {
        attempts += 1;
        let started = Instant::now();
        // ⏹ between rungs: the dial never goes out and the loop must not spend another wait. Same check `server_leg` applies.
        if cancelled() {
            return Err(Failure::Stopped);
        }
        // §02 §1: one request at a time per server; the wait goes in the row's wait column (§09 §10).
        let (slot, waited) = match crate::slot_gate::hold(&endpoint.url, step, cancelled) {
            Ok(got) => got,
            // ⏹ landed while every slot was busy: nothing went out, so no patience applies.
            Err(crate::slot_gate::Wait::Cancelled) => return Err(Failure::Stopped),
        };
        if waited > 0.0 {
            if let Some(other) = other_holder(step, &crate::slot_gate::holders(&endpoint.url)) {
                log(&crate::slot_gate::waited_log(step, &other));
            }
        }
        // §7's live half: `chat_on` is the one door every chat ask goes through, so the page fills up while you
        // wait with no flow changed. The recorder reads the body to decide whether to stream (§9 §1).
        let live = exchange_leg::Live::new(tree, step, &base.model, body);
        let hook = live.opened_then_hook(log);
        let (_row, reply) = server_leg::call_dialed(
            tree,
            base,
            &server_leg::Dial {
                server: crate::services::Server::Llm,
                endpoint,
                kind: Kind::Chat,
                job: None,
                body: Some(body.as_bytes()),
                filename: None,
                cancelled,
                // §4: every chat call is streamed, so the stall rule holds for all (same decision as `"stream": true` in `llm_request::body`).
                liveness: Some(crate::llm_watch_leg::Liveness::streamed(step)),
                waited_ms: (waited * 1000.0).round() as i64,
                delta: hook,
                budget: None,
                // §10: "a retry is a line of its own", with its attempt number; this loop knows which rung it is on.
                attempt: attempts,
            },
        );
        // This attempt is over, so its hook writes nothing more: a §4 give-up leaves the reader alive and its late
        // words must not land in the NEXT attempt's section.
        live.close_attempt();
        // The slot is given back before the reply is read (§4 holds it across the HTTP call only). Dropping the
        // guard is the release, so an early `?` below releases too.
        drop(slot);
        match reply {
            Ok(sent) if sent.ok() => {
                note_answer(&endpoint.url);
                record_dial(
                    step,
                    tree,
                    &base.model,
                    body,
                    Ok(&sent),
                    started,
                    log,
                    &live,
                );
                Ok(sent)
            }
            Ok(sent) => {
                record_dial(
                    step,
                    tree,
                    &base.model,
                    body,
                    Ok(&sent),
                    started,
                    log,
                    &live,
                );
                Err(Failure::Status {
                    status: sent.status,
                    body: sent.body,
                })
            }
            Err(why) => {
                record_dial(
                    step,
                    tree,
                    &base.model,
                    body,
                    Err(why.as_str()),
                    started,
                    log,
                    &live,
                );
                Err(Failure::Gone(why))
            }
        }
    };
    let first = one();
    let refused_field = match &first {
        Err(Failure::Status { status, body }) => crate::degraded::refused_answer(*status, body),
        _ => false,
    };
    if refused_field {
        // A `tools` refusal is not transient and the caller's resend is a DIFFERENT request: hand the verdict back, having dialled once.
        let why = match &first {
            Err(Failure::Status { status, body }) => crate::degraded::refused_reason(*status, body),
            _ => unreachable!("only a refused answer reaches here"),
        };
        return Dialled {
            answer: Err(why),
            attempts,
        };
    }
    // Everything else rides §3's patience. `attempts` already counts the dial above, so the loop's first call
    // classifies that same result and must NOT dial again.
    let mut first_attempt = Some(first);
    let mut attempt = || match first_attempt.take() {
        Some(carried) => carried,
        None => one(),
    };
    let outcome = llm_retry::run(
        step,
        // The first dial is already behind us (needed to see whether this was a `tools` refusal), so the loop starts
        // its patience from there; re-running `one` would turn §3's one short retry into two.
        &mut attempt,
        sleep,
        // The run's OWN switch, not the process-wide one: a never-cancelled test closure must not be stopped by another run's ⏹.
        cancelled,
        &|problem: &Failure| problem.went_away(&endpoint.url),
    );
    // A stopped run reports itself through `Run::stopped`; nothing here may hand a stop back as a server failure.
    // `server_leg`'s wording for a call with no reply names the server and address; keep it to the step's line
    // (§Failure-is-specific), so the last failure's text is what comes back.
    let last_reason = match &outcome.last {
        Some(Failure::Gone(why)) => why.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    // §3 gives the death branch a line and the short retry none; `llm_retry::run` writes both to the sink
    // users and tests read.
    for line in &outcome.logs {
        log(line);
    }
    let answer = match outcome.answer {
        Some(sent) => Ok(sent),
        // Nothing arrived to report. A declined refusal comes back as the server's own wording (the tools-refusal
        // check reads it); anything else gets a reason naming the patience that ran out.
        None => Err(match (outcome.stopped, outcome.last) {
            (false, Some(Failure::Status { status, body })) => {
                format!("at {} answered {}: {}", endpoint.url, status, body.trim())
            }
            (true, _) => "cancelled while waiting for the server to come back".to_string(),
            _ => format!(
                "{last_reason} -- no answer after {} failed attempt(s)",
                outcome.failures
            ),
        }),
    };
    Dialled { answer, attempts }
}
