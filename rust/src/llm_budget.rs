//! §09-llm-and-tools#5-context-budgets-new — every prompt is bounded.
//!
//! Spec: `spec/09-llm-and-tools.md` §5. The prototype sent whole sessions: a 64-minute lecture
//! made a 451 kB upload brief and a 778-line translation call, and the server lost its GPU over
//! it. The rewrite refuses instead. One number decides this for every chat request; the two ways a
//! job has of living inside it — batching, and letting the model read what was not sent through
//! `get_lines`/`get_events` — belong to the jobs themselves and are pinned in their own tests.
//!
//! Nothing here opens a socket or builds a body: [`check`] is the rule, and
//! [`crate::llm_request::body`] is the one place that applies it, because that is where every
//! request is built.

use crate::exchanges::{Message, Part};

/// P.machine.promptMaxChars — §10 §1's bound: the most one request may carry. Default 120 000.
/// This constant is the only holder of the number; the catalogue row in
/// [`crate::params::prepare`] spells it from here rather than repeating it.
pub const PROMPT_MAX_CHARS: usize = 120_000;

/// Characters of TEXT in a message list. A picture counts nothing: an image is not context the
/// model reads as prose, and counting its base64 would make the cap a limit on how many frames a
/// call may carry, which is [`P.machine.describeFramesPerReq`]'s business instead.
///
/// Chars, not bytes, on purpose: this is a bound on *context*, so a Cyrillic transcript must not
/// be penalised twice for its UTF-8 encoding. [`crate::exchanges::Call::sent`] deliberately still
/// counts BYTES, because its log line reports what went on the wire. The two counters measure
/// different things and are not interchangeable.
pub fn chars(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| {
            message
                .parts
                .iter()
                .map(|part| match part {
                    Part::Text(text) => text.chars().count(),
                    Part::Image(_) => 0,
                })
                .sum::<usize>()
        })
        .sum()
}

/// Whether a request of this many characters may go. The boundary is inclusive: exactly
/// [`PROMPT_MAX_CHARS`] is within the budget, one char more is refused.
pub fn fits(text_chars: usize) -> bool {
    text_chars <= PROMPT_MAX_CHARS
}

/// The bound as a decision. Step-agnostic wording on purpose: [`crate::llm_request::body`] has no
/// step name to put in front of the line, and every caller prefixes its own. The sentence says both
/// ways out, because "too big" with no remedy is a dead end for whoever reads the log.
pub fn check(text_chars: usize) -> Result<(), String> {
    if fits(text_chars) {
        return Ok(());
    }
    Err(format!(
        "prompt is over the budget: {text_chars} chars against {PROMPT_MAX_CHARS} -- \
         run the job in batches or let the model read the rest with get_lines/get_events"
    ))
}
