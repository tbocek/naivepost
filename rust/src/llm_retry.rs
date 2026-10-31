//! F6.2 — `spec/09-llm-and-tools.md` §3, retries.

use std::fmt::Display;

use crate::degraded;
use crate::exchanges::{Message, Part};

pub const BACKOFF_SECONDS: [u64; 5] = [5, 20, 60, 120, 240];

/// The one short retry for anything else — a 4xx/5xx or an unusable answer, first failure only (§3's second
/// bullet). Two seconds is not patience about the server; it covers a request that tripped over something local.
pub const RETRY_SECONDS: u64 = 2;

const DEATH_WORDS: [&str; 7] = [
    "eof",
    "connection reset",
    "connection refused",
    "broken pipe",
    "server closed",
    "no such host",
    "transport is closing",
];

pub const GONE_REPLY: &str = "is not answering";

/// The two ways `server_leg` says the USER ended a call rather than the server dying.
const CANCELLED_CASES: [&str; 2] = ["cancelled before sending", "cancelled mid-reply"];

/// Whether a `server_leg` failure string is a death rather than a stop dressed in the same words.
fn leg_says_gone(lower: &str) -> bool {
    lower.contains(GONE_REPLY) && !CANCELLED_CASES.iter().any(|case| lower.contains(case))
}

/// §3's condition words — reset, refused, unreachable. `reset` and `refused` only count beside the noun that
/// says what was reset or refused: net/http, Go's h2 transport and Rust's client all write `connection reset` /
/// `connection refused`, so requiring the pair loses nothing real and keeps "the model refused to answer" out.
const DEATH_CONDITIONS: [(&str, bool); 3] =
    [("reset", true), ("refused", true), ("unreachable", false)];

pub const DEVICE_FAILURE_PHRASES: [&str; 8] = [
    "out of memory",
    "oom",
    "cannot allocate",
    "allocate",
    "alloc",
    "cuda",
    "vram",
    "gpu",
];

pub fn transport_death(error: &str) -> bool {
    if status_of(error).is_some() {
        return false;
    }
    let lower = error.to_lowercase();
    leg_says_gone(&lower)
        || DEATH_WORDS.iter().any(|word| contains_word(&lower, word))
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
    DEVICE_FAILURE_PHRASES
        .iter()
        .any(|phrase| contains_word(&lower, phrase))
}

/// The HTTP status a message names, if it names one.
pub fn status_of(text: &str) -> Option<u16> {
    const LABELS: [&str; 5] = ["status", "http", "http/", "code", "response"];
    let lower = without_urls(text);
    if !LABELS.iter().any(|label| lower.contains(label)) {
        return None;
    }
    // Padded so a run at either end has the same non-digit neighbour as one in the middle.
    let padded = format!(" {lower} ");
    let digits = |char: char| char.is_ascii_digit();
    let mut at = 0;
    while let Some(offset) = padded[at..].find(digits) {
        let start = at + offset;
        let end = start
            + padded[start..]
                .find(|char: char| !digits(char))
                .unwrap_or(padded.len() - start);
        // Exactly three digits, bounded by non-digits on both sides: a port or a byte count is not a status.
        if end - start == 3 {
            return padded[start..end].parse().ok();
        }
        at = end + 1;
    }
    None
}

/// The message with every URL blanked, lower-cased.
fn without_urls(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .map(|word| if word.contains("://") { " " } else { word })
        .collect::<Vec<&str>>()
        .join(" ")
}

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

/// The flowchart's two branches as one question: given what failed and how many failures this call has already
/// seen, how long to wait before asking again — or `None`, which means the step fails.
pub fn wait_seconds(died: bool, failure: u32) -> Option<u64> {
    if died {
        return BACKOFF_SECONDS.get(failure as usize).copied();
    }
    (failure == 0).then_some(RETRY_SECONDS)
}

pub fn wait_spelling(seconds: u64) -> String {
    if seconds >= 60 && seconds.is_multiple_of(60) {
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
    format!(
        ">>> {step}: asked again after {} ({reason})",
        wait_spelling(RETRY_SECONDS)
    )
}

/// What a retrying call ended with. `answer` is `None` when the step fails; `stopped` says ⏹ ended it, which the
/// caller must not report as a server problem.
#[derive(Debug, Clone)]
pub struct Run<T, E> {
    pub answer: Option<T>,
    /// Every line this call logged, in order — including the ones from attempts that did not fail.
    pub logs: Vec<String>,
    /// How many failures it took (0 when the first attempt answered).
    pub failures: u32,
    pub stopped: bool,
    /// The last failure seen, so a caller can act on WHICH kind it was rather than only on there having been
    /// one. `llm_retry_leg` uses this to hand a refusal the caller declined to retry back as the server's own
    /// wording instead of as "the patience ran out".
    pub last: Option<E>,
}

impl<T, E> Run<T, E> {
    /// The lines joined for an assertion message.
    pub fn joined(&self) -> String {
        self.logs.join("\n")
    }
}

/// Ask until the call answers, the patience runs out, or ⏹ says otherwise.
pub fn run<T, E: Display>(
    step: &str,
    call: &mut dyn FnMut() -> Result<T, E>,
    sleep: &dyn Fn(u64) -> bool,
    stopped: &dyn Fn() -> bool,
    died: &dyn Fn(&E) -> bool,
) -> Run<T, E> {
    let mut out = Run {
        answer: None,
        logs: Vec::new(),
        failures: 0,
        stopped: false,
        last: None,
    };
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
                let reason = problem.to_string();
                out.last = Some(problem);
                let Some(wait) = wait_seconds(died, attempt) else {
                    // No patience left: the step fails, and says nothing about waiting because it is not.
                    return out;
                };
                // Only the death branch has a line in §3. The short retry gets one too: a run that pauses two
                // seconds between two identical requests should say that it did.
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

pub fn think_again(thinking: bool, reply: &str) -> bool {
    thinking && no_answer(reply).is_none()
}

/// Put a rejected answer and its correction back into the conversation so the next attempt sees what it got wrong.
pub fn retry_turn(
    messages: Vec<Message>,
    reply: &str,
    problem: &str,
    tools_active: bool,
) -> Vec<Message> {
    let mut messages = messages;
    if !reply.trim().is_empty() {
        messages.push(Message {
            role: "assistant".to_string(),
            parts: vec![Part::Text(reply.to_string())],
        });
    }
    let correction =
        format!("Your answer failed validation: {problem}. Return corrected strict JSON only.");
    let role = if tools_active { "tool" } else { "user" };
    messages.push(Message {
        role: role.to_string(),
        parts: vec![Part::Text(correction)],
    });
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
