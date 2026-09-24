//! P.machine.seamRetries — how many times a refused join is asked again.
//! `spec/10-parameters.md` row `P.machine.seamRetries` (default 1, added 2026-09-18), used by the
//! text-edit pass of Prepare ([`crate::tools::textedit`], `spec/04-prepare.md` F1.10/F1.11).
//!
//! One join gets one ask; when the answer is refused the same join is asked again up to
//! [`SEAM_RETRIES`] times, each ask in its own cache slot so the refusal cannot be served back from
//! `cache/llm`. The prototype's loop was `for try := 0; try <= seamRetries; try++`, which is why
//! [`attempts`] is `retries + 1`: the first ask plus the retries.
//!
//! This module owns only the counting and the wording around it. What makes an answer a refusal lives
//! in [`crate::tools::textedit::Join::drop_words`] (two stretches instead of one at the join, too
//! many words, a stretch that misses the join) — here a refusal is simply an event that costs another ask.
//!
//! Deliberately not in [`crate::params`]: §04#4 does not list this id among the eighteen it names, and
//! `rust/tests/prepare_parameters.rs` pins that list by order, so a row inserted there would break
//! that assertion. The number lives with the rule that uses it, as every `P.*` value must; a reader who
//! asks which family it belongs to gets the right answer anyway, since
//! [`crate::params::family`] answers `Family::Machine` from the `P.machine.` prefix.

/// How many times a join whose answer was refused is asked again. `P.machine.seamRetries`
pub const SEAM_RETRIES: u32 = 1;

/// The two log tails the pass prints after a refusal, verbatim as they reach the run's page.
const ASKING_ONCE_MORE: &str = "asking once more";
const NOTHING_REMOVED: &str = "nothing removed there";

/// How many asks one join makes in all: the first, plus the retries. A refusal on the last of
/// these ends the join with nothing removed.
pub fn attempts(retries: u32) -> u32 {
    retries + 1
}

/// Whether another ask is still owed after `attempt` (0-based) refusals.
pub fn more_coming(attempt: u32, retries: u32) -> bool {
    attempt < retries
}

/// The tail the refusal line ends with: `asking once more` while a retry remains, `nothing removed
/// there` on the last ask, where the join is left exactly as it was.
pub fn log_tail(attempt: u32, retries: u32) -> &'static str {
    if more_coming(attempt, retries) {
        ASKING_ONCE_MORE
    } else {
        NOTHING_REMOVED
    }
}

/// The cache slot for one ask about a join. The first ask is the base request unchanged; every retry
/// carries `|try N` so it keys elsewhere. `crate::llm_cache` keys on the exact request, so without
/// a distinct slot per attempt the second ask would be answered out of cache by the very reply that
/// was just refused, and the retry would buy nothing.
pub fn ask_slot(base: &str, attempt: u32) -> String {
    if attempt == 0 {
        base.to_string()
    } else {
        format!("{base}|try {}", attempt + 1)
    }
}

/// One join's ask/retry counter, kept out of any callback so the sequence is testable.
///
/// Three things the spec leaves open, decided here:
/// - The count is per join. Building one `Asking` per seam means one join's refusals never spend
///   another join's retries.
/// - A transport error is not a refusal. It is the ask itself failing, not an answer being refused,
///   so the caller retries the *same* slot rather than calling [`Asking::refused`].
/// - After the last refusal the join stays as it was: the words either side were said, and keeping a
///   stumble is a smaller fault than cutting something said once (`spec/04-prepare.md`: "A refusal
///   still keeps the stumble").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    /// Which ask this is, 0-based: 0 is the first, 1 the first retry.
    pub attempt: u32,
    /// The bound this join is working to — normally [`SEAM_RETRIES`].
    pub retries: u32,
}

impl Asking {
    /// Begin a join's asks: the first one has not been refused yet.
    pub fn start(retries: u32) -> Asking {
        Asking {
            attempt: 0,
            retries,
        }
    }

    /// The cache slot to use for this ask.
    pub fn slot(&self, base: &str) -> String {
        ask_slot(base, self.attempt)
    }

    /// Whether every allowed ask has been used up.
    pub fn spent(&self) -> bool {
        !more_coming(self.attempt, self.retries)
    }

    /// Record a refusal against the ask just made: the attempt is spent either way, and the returned
    /// tail is what the refusal line ends with. `None` only once every allowed ask has been used up —
    /// a caller that ignores that keeps asking past the bound.
    pub fn refused(&mut self) -> Option<&'static str> {
        // The last allowed ask was already made, so there is nothing left to refuse.
        if self.attempt >= attempts(self.retries) {
            return None;
        }
        let tail = log_tail(self.attempt, self.retries);
        self.attempt += 1;
        Some(tail)
    }
}
