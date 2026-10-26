//! F6.2 §3's live half: the patience every chat ask rides.
//!
//! [`crate::llm_retry`] holds §3's rules as pure functions and `tests/suite/llm_retries_f6_2.rs` pins them; this is
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
use crate::exchange_leg;
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
    let left = nap_duration(seconds);
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

/// How long a §3 wait actually takes. `NAIVEPOST_RETRY_SCALE` divides it, which is how a live test sits through the
/// whole 5 s → 4 min ladder in seconds; the log keeps naming the SECONDS §3 spells, because that is the truth about
/// the patience, and only the wall time shrinks. Unset in every real run, where a wait is a wait.
fn nap_duration(seconds: u64) -> Duration {
    let scale = std::env::var("NAIVEPOST_RETRY_SCALE")
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|factor| *factor > 0)
        .unwrap_or(1);
    Duration::from_millis(seconds * 1000 / scale)
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

/// Forget that this endpoint ever answered, so a later death of it reads as "a box nobody started" again.
///
/// The mirror of [`note_answer`], for the other half of the same race. The table is process-wide and keyed by
/// URL, and a test's dead server sits on an EPHEMERAL port: an earlier scenario in the same binary may have been
/// ANSWERED on that number before its listener was released, so `has_answered` can come back true for a wire
/// this scenario has never dialled — and §3's ladder, which should cost nothing here, is spent instead (three
/// rows where two were promised). A scenario that means "a server this app has never heard from" says so here
/// instead of depending on which port number the OS happened to hand out.
///
/// Named `_for_test` because nothing in the program calls it: an endpoint that answered keeps its mid-call history
/// for the life of the run, which is the point of the table, so only a proof needs it wiped.
pub fn forget_answer_for_test(endpoint: &str) {
    if let Ok(mut seen) = answered_once().lock() {
        seen.remove(endpoint);
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

/// The prototype's queue line needs a holder that is NOT this step. Holder names carry the per-ask `#N`
/// suffix `slot_gate::hold` adds, so the comparison is on the step prefix; the first such holder is named,
/// because "it was busy with <other>" reads as one cause and the log already lists every holder in §4's
/// waiting line.
pub fn other_holder(step: &str, holders: &[String]) -> Option<String> {
    holders
        .iter()
        .map(|holder| holder.split('#').next().unwrap_or(holder.as_str()))
        .find(|holder| *holder != step)
        .map(str::to_string)
}

/// [`chat`] with the wait supplied — the seam that lets a test watch the patience without living through eight
/// minutes of it. The program calls [`chat`], which passes §3's own ladder and [`nap`]; the argument exists so
/// the waits are observable, not so a caller can pick its own patience.
///
/// Every request this door sends is filed in the run's exchange page on its way past [`dial`], which is why no
/// chat ask has to record anything itself (§7: "recording never fails the call" — the page is written before
/// the answer is even looked at, and a failed page only ever costs a log line).
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

/// File one dialled chat call in the run's exchange page (§7), and hand its log lines to the sink the
/// caller already writes its §3 patience lines to, so the record of the call sits beside the record of
/// the waiting.
///
/// Called from all three endings of a dial: answered, refused, and never answered. The last of these
/// still belongs on the page — an exchange where the server said nothing is the one a user opens the
/// page to find — so the call carries the failure's wording as its error and no reply at all.
///
/// Only a `text/event-stream` reply arrives in chunks, and every chat ask has already read them by the
/// time this runs, so the section is filed whole rather than left open to grow.
///
/// `streamed` is decided by the response's own `content-type` (§9 §1: only `text/event-stream` is a
/// stream). A stream read here has still been read to its end by this function's caller before anyone
/// looks at the reply, so the section is closed straight away — the open-`<pre>` form §8 describes is
/// for a caller that appends as chunks land, and none of the chat asks do that; they wait for the body.
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
    // Decided by the body, not by what arrived: §9 §1 recognises a stream by the response's `content-type`, but
    // the page's live section was opened (or not) when the request went out, and the two must agree about which
    // calls this one. `asked_for_live` is that same decision on the request side.
    let streamed = live.asked_for_live();
    let took = started.elapsed().as_secs();
    let (reply_body, content_type, failure) = match reply {
        // A refused status is the failure this dial ended with, in the same words `Failure::Status`
        // prints so the page and the §3 patience lines agree about what went wrong. A 2xx has none.
        Ok(sent) => (
            sent.body.as_str(),
            sent.content_type.as_str(),
            sent.ok()
                .then(String::new)
                .unwrap_or_else(|| format!("answered {}: {}", sent.status, sent.body.trim())),
        ),
        // A call that got no reply at all still belongs on the page — it is the one a user opens the page
        // to find — so it goes with the failure's wording and no reply.
        Err(why) => ("", "", why.to_string()),
    };
    let problem = (!failure.is_empty()).then_some(failure);
    // A streamed call's own reply is the words that reached the page. The leg hands `reply_body` to `call_of`
    // as raw SSE framing (`data: {…}` lines), which belongs on an exchange page only once it has been read
    // back into prose — otherwise the closed section shows the wire, not the answer. The hook did that reading
    // as it grew the live section, so its copy is what the finished section prints; a call that streamed nothing
    // (refused, dead) keeps the body as it arrived, which for those endings is the server's own message and
    // worth seeing.
    let streamed_words = live.streamed_words();
    let shown_reply = if streamed && !streamed_words.is_empty() {
        streamed_words.as_str()
    } else {
        reply_body
    };
    // And the page's `reply:` line has to describe that same text: `call_of` reads an answer through the
    // stream parser when the content type says event-stream, and prose handed to it then parses as nothing.
    let shown_type = if streamed && !streamed_words.is_empty() {
        "text/plain"
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
    // A call nobody streamed ends exactly as it always did: one call files the whole exchange. For a streamed
    // one, `live.finish` closes the section the hook was growing instead, which is §7's other ending.
    if streamed {
        live.finish(shown_reply, shown_type, took, problem, log);
    } else {
        exchange_leg::record(tree, step, &call, log);
    }
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
        let started = Instant::now();
        // ⏹ between rungs: the dial never goes out, and the loop must not spend another wait on a run already over.
        // This is the same check `server_leg` applies before sending; the ladder would otherwise keep walking after
        // the stop with nothing left to cancel.
        if cancelled() {
            return Err(Failure::Stopped);
        }
        // §02 §1, on the LLM's own door: one request at a time per server. The wait is written into the
        // row's wait column (§09 §10).
        let (slot, waited) = match crate::slot_gate::hold(&endpoint.url, step, cancelled) {
            Ok(got) => got,
            // ⏹ landed while every slot was busy. Nothing went out, so no patience applies — the same answer
            // as the check above, reached from inside the wait instead of before it.
            Err(crate::slot_gate::Wait::Cancelled) => return Err(Failure::Stopped),
        };
        // The prototype's queue line, kept: a step that queued behind ANOTHER step's call says who had it.
        // Its own earlier call is silence — `slot_gate::waited_log`'s caller decides that, because only the
        // holder list can tell the two apart.
        if waited > 0.0 {
            if let Some(other) = other_holder(step, &crate::slot_gate::holders(&endpoint.url)) {
                log(&crate::slot_gate::waited_log(step, &other));
            }
        }
        // §7's live half of a chat call. `chat_on` is the one door every chat ask in the app goes through, so
        // this is where "the page fills up while you wait" happens — no flow had to be changed, and none can
        // forget it. The recorder reads the body to decide whether there is anything to stream (§9 §1: only a
        // request that asked for a stream gets chunks), so a one-shot ask still files its exchange at the end.
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
                // §4: every chat call is streamed, so the stall rule holds for all of them — the same decision that
                // puts `"stream": true` in the body (`llm_request::body`), carried here rather than re-derived.
                liveness: Some(crate::llm_watch_leg::Liveness::streamed(step)),
                waited_ms: (waited * 1000.0).round() as i64,
                delta: hook,
                budget: None,
                // §10: "a retry is a line of its own", with its attempt number. This loop is the only
                // thing that knows which rung it is on, so it says so rather than letting the leg guess.
                attempt: attempts,
            },
        );
        // §7's live half: this attempt is over, so its hook writes nothing more — a §4 give-up leaves the
        // reader thread alive, and its late words must not land in the NEXT attempt's section.
        live.close_attempt();
        // The slot is given back here, before the reply is read: §4 holds it across the HTTP call only, so a
        // step thinking between tool rounds holds nothing. Dropping the guard is the release, which also makes
        // an early `?` below release just as surely as this line does.
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
        // §3's patience is for a transient; a refusal of the `tools` field is not one, and the caller's own
        // resend is a DIFFERENT request. Hand the verdict back in the shape its check recognises, having dials
        // once.
        let why = match &first {
            Err(Failure::Status { status, body }) => crate::degraded::refused_reason(*status, body),
            _ => unreachable!("only a refused answer reaches here"),
        };
        return Dialled {
            answer: Err(why),
            attempts,
        };
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
