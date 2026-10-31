//! §09-llm-and-tools#5-context-budgets-new — every prompt is bounded.

use crate::exchanges::{Message, Part};

/// P.machine.promptMaxChars — §10 §1's bound: the most one request may carry. Default 120 000.
pub const PROMPT_MAX_CHARS: usize = 120_000;

/// Characters of TEXT in a message list. A picture counts nothing: an image is not context the
/// model reads as prose, and counting its base64 would make the cap a limit on how many frames a
/// call may carry, which is `P.machine.describeFramesPerReq`'s business instead.
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

pub fn check(step: &str, text_chars: usize) -> Result<(), String> {
    if fits(text_chars) {
        return Ok(());
    }
    Err(format!(
        ">>> {step}: prompt is over the budget: {text_chars} chars against {PROMPT_MAX_CHARS} -- \
         run the job in batches or let the model read the rest with get_lines/get_events"
    ))
}
