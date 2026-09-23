//! F6.2 — `spec/09-llm-and-tools.md` §3, retries.
//!
//! A call fails in one of two ways and they deserve opposite patience. A server that went away mid-call knows
//! nothing wrong with the request, so the same request is asked again after a wait long enough to land past a
//! container restart and a weight load. A server that answered — with a 4xx/5xx or with an answer this app
//! cannot use — has already told us what it thinks, so asking again at once gets the same reply; one short retry
//! covers the transient, and after that the step fails.
//!
//! Nothing here sleeps, spawns a thread or reads a clock: [`run`] takes the wait as a closure the caller passes,
//! the way [`crate::tool_loop::run`] takes `ask` and `call`. That is what makes "the fifth death asks for nothing"
//! and "⏹ ends a four-minute wait" facts a test can pin rather than facts a reviewer hopes about.
//!
//! # What this is not
//!
//! `P.eng.llmAttempts` (3) counts *validation* attempts at one job — the JSON was rejected twice and asked for
//! again — and lives in [`crate::roles`] with its own tests. This module counts failures of the HTTP call itself.
//! The two numbers are unrelated: a job may spend three validation attempts inside one successful call, and one
//! call may die five times without ever rejecting an answer.
//!
//! # Ids used
//!
//! §10 lists these only in its by-area line ("backoff 5 s–4 min"), with no `P.` row, so they carry bare prefixes:
//! `llm.backoffSeconds` and `llm.retrySeconds`, catalogued in [`crate::params`] from [`BACKOFF_SECONDS`] and
//! [`RETRY_SECONDS`].

use std::fmt::Display;

use crate::degraded;
use crate::exchanges::{Message, Part};

// ---- The two patiences ---------------------------------------------------------------

/// How long to wait before each attempt at a call that died in transit: five waits, roughly eight minutes of
/// patience in total (§3's bullet; §10's by-area line "backoff 5 s–4 min"). Past the restart, not inside it —
/// the container has to stop, come back and read tens of gigabytes of weights before it answers anything, and an
/// attempt made while that is happening is a connection refused that costs one of the tries.
pub const BACKOFF_SECONDS: [u64; 5] = [5, 20, 60, 120, 240];

/// The one short retry for anything else — a 4xx/5xx or an unusable answer, first failure only (§3's second
/// bullet). Two seconds is not patience about the server; it covers a request that tripped over something local.
pub const RETRY_SECONDS: u64 = 2;

// ---- Which kind of failure -----------------------------------------------------------

/// The words that mean the connection died rather than answered. §3 names them — eof, connection reset,
/// connection refused, broken pipe, server closed, no such host, transport is closing — and adds the conditions
/// those strings come from (reset, refused, unreachable). `transport is closing` is net/http's and h2's way of
/// reporting a server that closed on us.
///
/// Matched as whole words in their own space, so a model's own prose inside an error message cannot read as a
/// dead connection: "the model refused to answer" must not cost five minutes of waiting for a server that is
/// perfectly well. That is why §3's loose condition words are matched only alongside their noun (see
/// [`DEATH_CONDITIONS`]) — net/http, Go's h2 transport and Rust's client all say `connection reset` /
/// `connection refused`, so nothing real is lost, and a sentence about the model refusing is left alone.
const DEATH_WORDS: [&str; 7] = [
    "eof",
    "connection reset",
    "connection refused",
    "broken pipe",
    "server closed",
    "no such host",
    "transport is closing",
];

/// §3's condition words — reset, refused, unreachable. `reset` and `refused` only count beside the noun that
/// says what was reset or refused: net/http, Go's h2 transport and Rust's client all write `connection reset` /
/// `connection refused`, so requiring the pair loses nothing real and keeps "the model refused to answer" out.
/// `unreachable` is already specific to a socket, so it stands alone.
const DEATH_CONDITIONS: [(&str, bool); 3] = [("reset", true), ("refused", true), ("unreachable", false)];

/// §3's NEW rule: a 5xx whose body names a device or memory failure backs off like a transport death rather than
/// being resent at once. A 500 normally means "the server answered, and will answer the same way in four minutes"
/// — but an out-of-memory is not an answer about the request, it is the box having no room yet, which is exactly
/// what a wait fixes.
///
/// §3 names the two kinds and not the words, so these are the phrases llama.cpp-family servers actually print:
/// `out of memory` / `OOM` / `alloc` for memory, `CUDA` / `device` / `VRAM` / `GPU` for the device.
pub const DEVICE_FAILURE_PHRASES: [&str; 8] = [
    "out of memory",
    "oom",
    "cannot allocate",
    "alloc",
    "cuda",
    "device",
    "vram",
    "gpu",
];

/// Whether an error string is a transport death. Whole-word matching, so `refused` inside "the model refused to
/// answer" is prose and `refused` inside "connection refused" is a dead socket: the words are matched as pairs
/// (`connection refused`, not bare `refused`) precisely so a sentence about the model's behaviour cannot be read
/// as a socket's. A message naming an HTTP status is never one — see [`status_of`] and §3's rule.
pub fn transport_death(error: &str) -> bool {
    if status_of(error).is_some() {
        return false;
    }
    let lower = error.to_lowercase();
    DEATH_WORDS.iter().any(|word| contains_word(&lower, word))
        || DEATH_CONDITIONS.iter().any(|(word, needs_noun)| {
            contains_word(&lower, word) && (!needs_noun || contains_word(&lower, "connection"))
        })
}

/// Whether the failure should back off like a dead connection: the transport died, or the body says the box ran
/// out of device memory. Everything else takes the one short retry.
pub fn went_away(error: &str, status: Option<u16>, body: &str) -> bool {
    transport_death(error) || device_failure(status, body)
}

/// A 5xx naming a device or memory failure. A 4xx never qualifies: that is the request's fault, and no wait
/// changes what the server objects to.
pub fn device_failure(status: Option<u16>, body: &str) -> bool {
    if !matches!(status, Some(500..=599)) {
        return false;
    }
    let lower = body.to_lowercase();
    DEVICE_FAILURE_PHRASES.iter().any(|phrase| lower.contains(phrase))
}

/// The HTTP status a message names, if it names one.
///
/// Only a *labelled* status counts: `400 bad request`, `HTTP 500`, `status: 503`. A bare three-digit run does
/// not, because an error string is full of numbers that are not statuses — `127.0.0.1:8731` would otherwise read
/// as one and turn every connection failure into a status the server supposedly sent, which §3 forbids for the
/// opposite reason ("a status code is never the server went away").
pub fn status_of(text: &str) -> Option<u16> {
    const LABELS: [&str; 5] = ["status", "http", "http/", "code", "response"];
    let lower = text.to_lowercase();
    if !LABELS.iter().any(|label| lower.contains(label)) {
        return None;
    }
    // Padded so a run at either end has the same non-digit neighbour as one in the middle.
    let padded = format!(" {lower} ");
    let digits = |char: char| char.is_ascii_digit();
    let mut at = 0;
    while let Some(offset) = padded[at..].find(digits) {
        let start = at + offset;
        let end = start + padded[start..].find(|char: char| !digits(char)).unwrap_or(padded.len() - start);
        // Exactly three digits, bounded by non-digits on both sides: a port or a byte count is not a status.
        if end - start == 3 {
            return padded[start..end].parse().ok();
        }
        at = end + 1;
    }
    None
}

/// Is `word` present in `lower` as a whole word? The boundaries are anything that is not a letter or digit, plus
/// the ends of the string — which is why each candidate is padded with spaces before searching: it turns "the
/// socket reset" into a hit and "resettable" into a miss without needing a regex. Punctuation counts as a
/// boundary, so `eof` inside "unexpected eof while reading" is found.
fn contains_word(lower: &str, word: &str) -> bool {
    let haystack = format!(" {lower} ");
    let needle = format!(" {word}");
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(&needle) {
        let end = from + offset + needle.len();
        // The word must end at a boundary too: "reset" must not match inside "resettable".
        if haystack[end..].starts_with(|next: char| !(next.is_alphanumeric() || next == '_')) {
            return true;
        }
        from = from + offset + 1;
    }
    false
}

// ---- How long, and whether at all ----------------------------------------------------

/// The flowchart's two branches as one question: given what failed and how many failures this call has already
/// seen, how long to wait before asking again — or `None`, which means the step fails.
///
/// A death walks [`BACKOFF_SECONDS`] and stops when the ladder runs out: five waits is the patience §3 allows,
/// and a sixth death is a server that is not coming back. Anything else gets [`RETRY_SECONDS`] once, on the
/// first failure only.
pub fn wait_seconds(died: bool, failure: u32) -> Option<u64> {
    if died {
        return BACKOFF_SECONDS.get(failure as usize).copied();
    }
    (failure == 0).then_some(RETRY_SECONDS)
}

/// A wait as §3 spells it in its own prose — "5 s, 20 s, 1 min, 2 min, 4 min". The prototype prints Go's
/// `durOf` instead (`1m0s`), which is a duration formatted for a programmer; the spec's words are what a person
/// reads in the log, so they win. A number the ladder does not hold keeps the unit that fits it, so a shortened
/// backoff still reads sensibly rather than falling through to seconds.
pub fn wait_spelling(seconds: u64) -> String {
    if seconds >= 60 && seconds % 60 == 0 {
        let minutes = seconds / 60;
        return format!("{minutes} min");
    }
    format!("{seconds} s")
}

/// §3's line, byte for byte: `!!! <step>: the server went away mid-call (…) -- waiting T and asking again (i of
/// 5)`. ASCII `--` because that is what the spec's bytes hold. `attempt` counts from zero, as the loop sees it;
/// the log shows one-based, and the total is the ladder's length rather than a literal 5 that could drift.
pub fn gone_log(step: &str, reason: &str, seconds: u64, attempt: u32) -> String {
    format!(
        "!!! {step}: the server went away mid-call ({reason}) -- waiting {} and asking again ({} of {})",
        wait_spelling(seconds),
        attempt + 1,
        BACKOFF_SECONDS.len()
    )
}

/// The short retry's line. §3 gives no wording for it — only the death branch has a log in the spec — but a retry
/// nobody can see is indistinguishable from a call that hung for two seconds, and the difference matters to
/// whoever reads a stalled run. Shape borrowed from the prototype's `>>>` lines.
pub fn waited_log(step: &str, reason: &str) -> String {
    format!(">>> {step}: asked again after {} ({reason})", wait_spelling(RETRY_SECONDS))
}

// ---- The loop -----------------------------------------------------------------------

/// What a retrying call ended with. `answer` is `None` when the step fails; `stopped` says ⏹ ended it, which the
/// caller must not report as a server problem.
#[derive(Debug, Clone)]
pub struct Run<T> {
    pub answer: Option<T>,
    /// Every line this call logged, in order — including the ones from attempts that did not fail.
    pub logs: Vec<String>,
    /// How many failures it took (0 when the first attempt answered).
    pub failures: u32,
    pub stopped: bool,
}

impl<T> Run<T> {
    /// The lines joined for an assertion message.
    pub fn joined(&self) -> String {
        self.logs.join("\n")
    }
}

/// Ask until the call answers, the patience runs out, or ⏹ says otherwise.
///
/// `call` is the request — retried with the same arguments every time, which is what "the same request again"
/// means and why a transport retry of a tool loop re-runs from the original messages rather than continuing a
/// half-finished conversation. `sleep` waits and answers `false` when the run was stopped instead: eight minutes
/// of blocking sleep is eight minutes of a ⏹ button that does nothing, so the wait belongs to the caller and this
/// loop stops the moment either one says the run is over. `died` classifies the error — the real one is
/// [`went_away`] against the response's status and body; taking it as an argument keeps the classification
/// testable without a server.
pub fn run<T, E: Display>(
    step: &str,
    call: &mut dyn FnMut() -> Result<T, E>,
    sleep: &dyn Fn(u64) -> bool,
    stopped: &dyn Fn() -> bool,
    died: &dyn Fn(&E) -> bool,
) -> Run<T> {
    let mut out = Run { answer: None, logs: Vec::new(), failures: 0, stopped: false };
    loop {
        match call() {
            Ok(answer) => {
                out.answer = Some(answer);
                return out;
            }
            Err(problem) => {
                let attempt = out.failures;
                out.failures += 1;
                let died = died(&problem);
                let Some(wait) = wait_seconds(died, attempt) else {
                    // No patience left: the step fails, and says nothing about waiting because it is not.
                    return out;
                };
                // Only the death branch has a line in §3. The short retry gets one too: a run that pauses two
                // seconds between two identical requests should say that it did.
                let reason = problem.to_string();
                out.logs.push(if died {
                    gone_log(step, &reason, wait, attempt)
                } else {
                    waited_log(step, &reason)
                });
                if !sleep(wait) || stopped() {
                    out.stopped = true;
                    return out;
                }
            }
        }
    }
}

// ---- S: content repair in degraded (JSON) mode --------------------------------------

/// Why an answer could not be used, and what the retry does about it. §3 names four repairs — `noAnswer`,
/// `cutOff`, `thinkAgain` (see [`think_again`]), `retryTurn` (see [`retry_turn`]) — and with tools they become
/// tool results rather than user turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repair {
    /// The whole reply was reasoning: nothing to parse.
    NoAnswer,
    /// The answer stopped at the token ceiling — "unexpected end of JSON", which wants a *shorter* answer.
    CutOff,
    /// Anything else the parser objected to, which wants care rather than brevity.
    Other,
}

/// The correction for a reply with no answer in it at all: the thinking budget spent and not a word written.
/// A JSON parser cannot name this — it reports "unexpected end of JSON input", the same words it uses for an
/// answer truncated at the ceiling, and those two want opposite fixes. Kept as the prototype's sentence so the
/// model sees the words it was tuned on: what happened, then what to write.
pub const NO_ANSWER: &str = "you returned no answer at all -- the whole reply was reasoning. \
                             Think briefly, then write the complete JSON: every segment and every effect, \
                             not a sample";

/// The correction for an answer that stopped in the middle — [`degraded::CUTOFF_RETRY`], reused rather than
/// retyped so one sentence has one home: "Answer again with far fewer items".
pub fn cut_off_problem(reply: &str, parse_error: &str) -> Option<&'static str> {
    // An empty reply is no answer, not a truncated one; and an error that is not about a missing tail is a
    // different complaint.
    if reply.trim().is_empty() || !parse_error.contains("unexpected end of JSON") {
        return None;
    }
    Some(degraded::CUTOFF_RETRY)
}

/// The whole reply was reasoning: everything up to `</think>` and nothing after it, or simply no text.
fn answer_of(reply: &str) -> &str {
    match reply.rfind("</think>") {
        Some(end) => &reply[end + "</think>".len()..],
        None => reply,
    }
}

/// Whether the reply holds an answer at all. Empty and all-reasoning are the same failure: there is nothing to
/// parse, and the model has to be told the difference between that and a malformed answer.
pub fn no_answer(reply: &str) -> Option<&'static str> {
    if answer_of(reply).trim().is_empty() {
        return Some(NO_ANSWER);
    }
    None
}

/// Whether the next attempt may still think. An empty answer means the thinking budget was spent with no words,
/// and the same question asked the same way will answer the same way — so that retry sends thinking off with the
/// shorter ceiling. Bad JSON or the wrong shape keeps thinking: that model is writing and getting it wrong, and
/// turning its reasoning off makes the next answer worse, not better.
pub fn think_again(thinking: bool, reply: &str) -> bool {
    thinking && no_answer(reply).is_none()
}

/// Put a rejected answer and its correction back into the conversation so the next attempt sees what it got wrong.
///
/// An empty reply is not appended: there is no message to send, and an assistant turn holding nothing makes the
/// history read as though the model had answered with a blank. With tools the correction arrives as a `tool`
/// result — §3's "with tools these become tool results" — because on that path the model is mid-round and expects
/// its calls answered, not a new instruction from the user.
pub fn retry_turn(messages: Vec<Message>, reply: &str, problem: &str, tools_active: bool) -> Vec<Message> {
    let mut messages = messages;
    if !reply.trim().is_empty() {
        messages.push(Message { role: "assistant".to_string(), parts: vec![Part::Text(reply.to_string())] });
    }
    let correction =
        format!("Your answer failed validation: {problem}. Return corrected strict JSON only.");
    let role = if tools_active { "tool" } else { "user" };
    messages.push(Message { role: role.to_string(), parts: vec![Part::Text(correction)] });
    messages
}

/// The prototype's `jsonReply` order as a decision: is there an answer at all, did it stop in the middle, or is
/// it simply wrong? `parse_error` is what the JSON parser said (`""` when it parsed), and `None` means the reply
/// is usable.
pub fn repair(reply: &str, parse_error: &str) -> Option<(Repair, String)> {
    if let Some(problem) = no_answer(reply) {
        return Some((Repair::NoAnswer, problem.to_string()));
    }
    if parse_error.is_empty() {
        return None;
    }
    if let Some(problem) = cut_off_problem(reply, parse_error) {
        return Some((Repair::CutOff, problem.to_string()));
    }
    Some((Repair::Other, format!("not valid JSON: {parse_error}")))
}
