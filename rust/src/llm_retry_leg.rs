//! F6.2 §3's live half: the patience every chat ask rides.
//!
//! [`crate::llm_retry`] holds §3's rules as pure functions and `tests/llm_retries_f6_2.rs` pins them; this is
//! the door that puts them on the wire. Before it, every chat ask dialled once and failed on the first error, so
//! a container restart cost the step even though the ladder exists to sit through exactly that. Now an LLM ask
//! goes through [`chat`]: a call that died in transit waits 5 s → 20 s → 1 min → 2 min → 4 min and asks the
//! IDENTICAL request again; anything that got an answer — a 4xx, a plain 5xx, an unusable reply — gets the one
//! short retry after 2 s, first failure only.
//!
//! # What counts as which
//!
//! `server_leg` returns a 4xx/5xx as `Ok(Sent)` (it sets `http_status_as_error(false)` so the caller can read
//! the server's own message beside the status), so the door turns `!sent.ok()` into a failure of the loop and
//! classifies it there: a 5xx whose body names a device or memory failure backs off like a death (§3's NEW
//! rule — an out-of-memory is the box having no room yet, which is what a wait fixes), and every other status
//! takes the short retry. A `Sent` with a 2xx is an answer whatever its content says: whether the JSON parses is
//! the caller's question, and a rejected ANSWER has its own counters (`roles::LLM_ATTEMPTS`,
//! `seam_retries`, `cut_captions::retries`) which this door does not touch — those count refusals of answers,
//! this counts failures of the call.
//!
//! One decision worth stating, because §3 and §02-services pull opposite ways: a failure where the request never
//! reached a server that had ever answered — connection refused or unreachable at connect time, which is what
//! `server_leg::down`'s "is not answering" wording carries — rides the ladder. §3 lists "refused" among the
//! death words outright, and the reason it gives is the reason that fits: the restart policy leaves the port
//! closed while weights load, so refusing to wait would spend the step on a server that is fine by the time
//! anyone looks. The prototype did the same (`llmGone` matches `ECONNREFUSED`).
//!
//! # Cancellable
//!
//! The wait is [`nap`], which polls the session's cancel switch, so ⏹ ends a four-minute wait instead of
//! turning the button into decoration for eight minutes. A stopped run reports itself through `Run::stopped`;
//! the caller sees the last failure as the error, because nothing arrived to say otherwise.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::cancel_leg;
use crate::layout::Tree;
use crate::llm_retry::{self};
use crate::requests::Request;
use crate::server_leg::{self, Sent};
use crate::services::{Endpoint, Kind};

/// How long [`nap`] sleeps per slice. `cancel_leg::POLL` is private to that module; the number is repeated here
/// rather than made public because a wait this coarse gains nothing from a tighter poll, and 50 ms is well inside
/// a blink against a five-second floor.
const NAP_POLL: Duration = Duration::from_millis(50);

/// Wait `seconds`, giving up the moment ⏹ lands. True = the whole wait happened; false = the run was stopped.
pub fn nap(seconds: u64) -> bool {
    let left = Duration::from_secs(seconds);
    // One deadline read fresh each slice, so a thread that got no timeslice for a second cannot make the wait
    // longer or shorter than it was asked.
    let started = Instant::now();
    while started.elapsed() < left {
        if cancel_leg::cancelled() {
            return false;
        }
        std::thread::sleep(NAP_POLL.min(left - started.elapsed()));
    }
    // A stop landing in the last slice still counts as stopped: the caller must not report a server problem for
    // a run the user ended.
    !cancel_leg::cancelled()
}

/// Either leg's failure, unified so one loop can hold both. `run` needs `Display` on it and a closure that says
/// which patience applies; the status and body ride along rather than being folded into a string, so §3's device
/// rule is applied to the real status instead of to a guess about one (`llm_retry::status_of` exists for text
/// that only *names* a status, which a `Sent` no longer is).
enum Failure {
    /// The socket died: `server_leg`'s own wording, naming the server and the address.
    Gone(String),
    /// The server answered with a status this app cannot use.
    Status { status: u16, body: String },
    /// ⏹ landed before this attempt was dialled: not a server problem at all, and no patience applies to it.
    Stopped,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Gone(why) => out.write_str(why),
            Failure::Stopped => out.write_str("cancelled while waiting for the server to come back"),
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
/// §3's ladder is for a server that "went away mid-call", and its own log line says exactly that: a container
/// restart caught mid-reply, coming back through its weights load. A connection REFUSED by an endpoint this app
/// has never once got a reply from is a different thing — §02-services#1's "a service that is down gives an
/// error" — and sitting 5 s + 20 s + 1 min + 2 min + 4 min in front of it would hold a run for eight minutes over
/// a box nobody started. So the ladder is gated on having been answered before: refused-with-no-history takes §3's
/// short patience instead, which still asks the identical request once more and then reports the server as down.
/// The word list in [`llm_retry::transport_death`] is unchanged by this — it classifies what the words say; this
/// table knows whether there was ever a call to go away from.
fn answered_once() -> &'static Mutex<std::collections::HashSet<String>> {
    static ANSWERED: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    ANSWERED.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Has this endpoint ever answered? A poisoned lock means a check panicked mid-call; the table is still ours to read.
fn has_answered(endpoint: &str) -> bool {
    answered_once()
        .lock()
        .map(|seen| seen.contains(endpoint))
        .unwrap_or(false)
}

/// Note that this endpoint answered, so a later death of it counts as mid-call.
///
/// Public because the table is process-wide and keyed by URL: a wire test that wants its OWN fresh port to have
/// mid-call history would otherwise have to earn it with an answered call on the very fake that is scripted to die,
/// which changes what that fake is testing. Nothing outside a test should call it.
pub fn note_answer(endpoint: &str) {
    if let Ok(mut seen) = answered_once().lock() {
        seen.insert(endpoint.to_string());
    }
}

/// Ask the LLM for one chat turn and keep asking until it answers, the patience runs out, or ⏹ says otherwise.
///
/// This is the door every chat ask should go through: it writes its timed row per dial (so a retried call leaves
/// one row per request, §09 §10), logs each wait as §3 spells it, and hands back the `Sent` unchanged so the
/// caller keeps reading its own answer through `llm_request::reply_from`. `step` names the job in the log line
/// (§3's `>>> <step>` / `!!! <step>` prefix). The waits are [`llm_retry::BACKOFF_SECONDS`] — 5 s to 4 min,
/// cancellable — which is long by design: the container has to stop, come back and read its weights before it
/// can answer again.
///
/// This is the door every chat ask should go through: it writes its timed row per dial (so a retried call leaves
/// one row per request, §09 §10), logs each wait as §3 spells it, and hands back the `Sent` unchanged so the
/// caller keeps reading its own answer through `llm_request::reply_from`. `step` names the job in the log line
/// (§3's `>>> <step>` / `!!! <step>` prefix). The waits are [`llm_retry::BACKOFF_SECONDS`] — 5 s to 4 min,
/// cancellable — which is long by design: the container has to stop, come back and read its weights before it
/// answers anything.
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

/// [`chat`] with the wait supplied — the seam that lets a test watch the patience without living through eight
/// minutes of it. The program calls [`chat`], which passes §3's own ladder and [`nap`]; the argument exists so
/// the waits are observable, not so a caller can pick its own patience.
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
    // A plain ask treats every failure as retryable: §3's patience is exactly about these.
    dial(step, tree, base, endpoint, body, cancelled, log, sleep).answer
}

/// What one dialled chat call came to: the answer, or the reason there is none, plus how many times the wire was
/// touched — so a caller (and a test) can tell "asked once and refused" from "waited through the ladder".
pub struct Dialled {
    pub answer: Result<Sent, String>,
    /// Requests that actually went out. §09 §10 counts one timed row per one of these.
    pub attempts: u32,
}

/// [`chat_on`], the door itself.
///
/// Two things stop here before §3's patience is applied. A 4xx whose message refuses the `tools` field is not a
/// transient — F6.1's own next move is a DIFFERENT request (the same messages minus the field) — so that answer
/// is handed back after a single dial rather than re-sent first; [`crate::degraded::refused_answer`] makes that
/// call, because §1 keeps the server's message beside the status and "a 400" alone cannot tell a schema it
/// dislikes from a broken server. And a socket death only walks the ladder when this endpoint has answered at
/// least once (§3 says "went away MID-CALL"), see [`has_answered`].
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
    // One dial, and the answer read exactly as it arrived: no patience applied yet, so a caller that has its own
    // next move for an answered-but-refused response (F6.1's resend-without-tools) can see it untouched.
    let mut one = || -> Result<Sent, Failure> {
        attempts += 1;
        // ⏹ between rungs: the dial never goes out, and the loop must not spend another wait on a run already over.
        // This is the same check `server_leg` applies before sending; the ladder would otherwise keep walking after
        // the stop with nothing left to cancel.
        if cancelled() {
            return Err(Failure::Stopped);
        }
        let (_row, reply) = server_leg::call(
            tree,
            base,
            crate::services::Server::Llm,
            endpoint,
            Kind::Chat,
            None,
            Some(body),
            cancelled,
        );
        match reply {
            Ok(sent) if sent.ok() => {
                note_answer(&endpoint.url);
                Ok(sent)
            }
            Ok(sent) => Err(Failure::Status { status: sent.status, body: sent.body }),
            Err(why) => Err(Failure::Gone(why)),
        }
    };
    let first = one();
    let refused_field = match &first {
        Err(Failure::Status { status, body }) => crate::degraded::refused_answer(*status, body),
        _ => false,
    };
    if refused_field {
        // §3's patience is for a transient; a refusal of the `tools` field is not one, and the caller's own
        // resend is a DIFFERENT request. Hand the verdict back in the shape its check recognises, having dials
        // once.
        let why = match &first {
            Err(Failure::Status { status, body }) => crate::degraded::refused_reason(*status, body),
            _ => unreachable!("only a refused answer reaches here"),
        };
        return Dialled { answer: Err(why), attempts };
    }
    // Everything else rides §3's patience: the identical body re-sent after each wait, the ladder for a death,
    // the one short retry for anything the server answered but could not use. `attempts` already counts the dial
    // above, so the loop's first call is that same result being classified — it must NOT dial again.
    let mut first_attempt = Some(first);
    let mut attempt = || match first_attempt.take() {
        Some(carried) => carried,
        None => one(),
    };
    let outcome = llm_retry::run(
        step,
        // The FIRST dial is already behind us (it had to be, to see whether this was a `tools` refusal the caller
        // must answer with a different request), so the loop starts its patience from there: re-running `one`
        // here would send the request a second time before any wait and turn §3's one short retry into two.
        &mut attempt,
        sleep,
        // The run's OWN switch, not the process-wide one: a leg that belongs to a run carries `cancelled`, and a
        // test (or a settings probe) that passes a never-cancelled closure must not be stopped by some other
        // run's ⏹ sitting on the shared cell.
        cancelled,
        &|problem: &Failure| problem.went_away(&endpoint.url),
    );
    // §3's patience is for a server that is coming back; ⏹ is not. A stopped run reports itself through
    // `Run::stopped` and its caller says so — so nothing after this point may hand a stop back as a server
    // failure, which is what the refused-field return below would otherwise do.
    // `server_leg`'s own wording for a call that got no reply names the server and the address it tried, and
    // §Failure-is-specific wants that kept all the way to the step's line -- "no answer after N attempts" on its
    // own would not say which of the four boxes to look at. So the last failure's text is what comes back.
    let last_reason = match &outcome.last {
        Some(Failure::Gone(why)) => why.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    // §3 gives the death branch a line and the short retry none; `llm_retry::run` writes both, and every one of
    // them goes to the sink the user and the tests read (`ui::window::log_line` from every caller).
    for line in &outcome.logs {
        log(line);
    }
    let answer = match outcome.answer {
        Some(sent) => Ok(sent),
        // Nothing arrived to report. A refusal the caller declined to retry comes back as the server's own
        // wording, because that is what its tools-refusal check reads; anything else gets a reason naming the
        // patience that ran out. The lines already logged say what was waited for and why it stopped.
        None => Err(match (outcome.stopped, outcome.last) {
            (false, Some(Failure::Status { status, body })) => format!(
                "at {} answered {}: {}",
                endpoint.url,
                status,
                body.trim()
            ),
            (true, _) => "cancelled while waiting for the server to come back".to_string(),
            _ => format!("{last_reason} -- no answer after {} failed attempt(s)", outcome.failures),
        }),
    };
    Dialled { answer, attempts }
}
