//! P.machine.seamRetries: how many times a refused join is asked again (`spec/10-parameters.md`, default 1), used by the text-edit pass ([`crate::tools::textedit`], F1.10/F1.11).
//! A join is asked up to [`SEAM_RETRIES`] more times, each in its own cache slot; [`attempts`] is `retries + 1`. What makes an answer a refusal lives in [`crate::tools::textedit::Join::drop_words`].
//! Not in [`crate::params`] because `rust/tests/suite/prepare_parameters.rs` pins that list by order; [`crate::params::family`] still answers `Family::Machine` from the prefix.

pub const SEAM_RETRIES: u32 = 1;

const ASKING_ONCE_MORE: &str = "asking once more";
const NOTHING_REMOVED: &str = "nothing removed there";

/// Asks one join makes in all: the first plus the retries. A refusal on the last leaves the join untouched.
pub fn attempts(retries: u32) -> u32 {
    retries + 1
}

pub fn more_coming(attempt: u32, retries: u32) -> bool {
    attempt < retries
}

/// The tail the refusal line ends with: `asking once more` while a retry remains, else `nothing removed there`.
pub fn log_tail(attempt: u32, retries: u32) -> &'static str {
    if more_coming(attempt, retries) {
        ASKING_ONCE_MORE
    } else {
        NOTHING_REMOVED
    }
}

/// Cache slot for one ask: the first is the base request, each retry carries `
/// try N`, else `crate::llm_cache` would serve back the refused reply.
pub fn ask_slot(base: &str, attempt: u32) -> String {
    if attempt == 0 {
        base.to_string()
    } else {
        format!("{base}|try {}", attempt + 1)
    }
}

/// One join's ask/retry counter, kept out of any callback so it is testable.
/// - Count is per join, so one join's refusals never spend another's retries.
/// - A transport error is not a refusal; the caller retries the same slot instead of calling [`Asking::refused`].
/// - After the last refusal the join stays as it was (`spec/04-prepare.md`: "A refusal still keeps the stumble").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    pub attempt: u32,
    pub retries: u32,
}

impl Asking {
    pub fn start(retries: u32) -> Asking {
        Asking {
            attempt: 0,
            retries,
        }
    }

    pub fn slot(&self, base: &str) -> String {
        ask_slot(base, self.attempt)
    }

    pub fn spent(&self) -> bool {
        !more_coming(self.attempt, self.retries)
    }

    /// Record a refusal against the ask just made; returns the tail for the refusal line, or `None` once every ask is used up.
    pub fn refused(&mut self) -> Option<&'static str> {
        if self.attempt >= attempts(self.retries) {
            return None;
        }
        let tail = log_tail(self.attempt, self.retries);
        self.attempt += 1;
        Some(tail)
    }
}
