//! Text edit — spec/02-services.md §3.5, one join at a time.
//!
//! Two takes that should have been one, with the words either side shown so the model can say how
//! many to drop and from which side. That is all the tool needs from it, and it is the whole point of
//! the rewrite: the prototype had the model return the joined text and derived the counts by matching
//! backwards, so a respelled word anywhere in the seam lost the join entirely. Now the count is stated
//! and the app re-derives the *stretch* only to check it is one stretch at the join.

use serde_json::json;

/// How many words each side of the join is shown at. P.machine.seamReachWords
pub const SEAM_REACH_WORDS: usize = 140;

/// Most words one join may remove. P.machine.seamMaxWords
pub const SEAM_MAX_WORDS: usize = 40;

/// Most of the words shown that one join may remove. P.machine.seamCeil
pub const SEAM_CEIL: f64 = 0.6;

/// How far from the join a named stretch may sit and still count as at the join — measured: refusals
/// went from four to two when this dropped to three. The counting itself is F1.10's round; the number
/// is §10's and lives here with the rest of the seam family. P.machine.seamSnapWords
pub const SEAM_SNAP_WORDS: usize = 3;

/// A stretch this short somewhere else in the session is a respelling of the seam rather than a second
/// copy, so it does not disqualify the join. Also F1.10's to apply; §10's number stated once.
/// P.machine.seamNoiseWords
pub const SEAM_NOISE_WORDS: usize = 2;

/// Which side of the join a stretch is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The end of the first take.
    Before,
    /// The start of the second.
    After,
}

/// One join and what has been taken from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    before: Vec<String>,
    after: Vec<String>,
    /// Words dropped so far, by side — in order, because "which words" is answered from these.
    dropped: Vec<(Side, usize)>,
    /// `keep_join`: a whole answer that removes nothing.
    kept: bool,
}

impl Join {
    /// A join between the end of one take and the start of the next. The two sides are shown capped
    /// at [`SEAM_REACH_WORDS`] words each — what a model may count from is what it was given, so the
    /// cap is stored rather than applied per call and the ceilings below read against it.
    pub fn new(before: &[&str], after: &[&str]) -> Join {
        Join {
            before: before.iter().map(|word| word.to_string()).collect(),
            after: after.iter().map(|word| word.to_string()).collect(),
            dropped: Vec::new(),
            kept: false,
        }
    }

    /// The words shown on this side, capped at [`SEAM_REACH_WORDS`] — nearest the join last for
    /// `Before` and first for `After`.
    pub fn shown(&self, side: Side) -> &[String] {
        let words = self.all(side);
        let kept = SEAM_REACH_WORDS.min(words.len());
        match side {
            // The tail is what touches the join, so the cap keeps the end.
            Side::Before => &words[words.len() - kept..],
            Side::After => &words[..kept],
        }
    }

    /// How many words were shown on this side: the denominator of [`SEAM_CEIL`].
    pub fn shown_count(&self, side: Side) -> usize {
        self.shown(side).len()
    }

    /// `drop_words(side, count)` — ok carrying the exact words that would go and the sentence left
    /// reading across the join.
    pub fn drop_words(&mut self, side: Side, count: usize) -> String {
        // "The stretch does not touch the join" is what a count of zero says: no words at all are a
        // stretch away from the seam rather than at it. Refused, because a model that meant
        // keep_join should say so and get the whole answer for it.
        if count == 0 {
            return crate::tools::error(
                "no words were named -- an empty stretch is not at the join; send keep_join \
                 when the two takes belong together as they stand",
            );
        }
        let shown = self.shown_count(side);
        if count > shown {
            return crate::tools::error(&format!(
                "{count} words were asked from the {} side and only {shown} were shown",
                name(side)
            ));
        }
        // Both ceilings are checked against what has already gone, not just this call: two drops of
        // 30 words each are 60 words, and a per-call check would let that through.
        let already = self.dropped_count(side);
        let total = already + count;
        if total > SEAM_MAX_WORDS {
            return crate::tools::error(&format!(
                "that is {total} words from the {} side -- one join removes at most \
                 {SEAM_MAX_WORDS}",
                name(side)
            ));
        }
        let shown_total = self.shown_count(Side::Before) + self.shown_count(Side::After);
        if shown_total > 0 && total as f64 > SEAM_CEIL * shown_total as f64 {
            return crate::tools::error(&format!(
                "{total} of {shown_total} words shown is over the {:.0}% a join may remove -- \
                 these are two takes, not one",
                SEAM_CEIL * 100.0
            ));
        }

        self.dropped.push((side, count));
        let going = self.words_taken(side);
        crate::tools::ok(&json!({
            "side": name(side),
            "count": total,
            // The exact words, in the order they were spoken: a model that sees them cannot have
            // dropped something it did not mean to.
            "words": going,
            "across_the_join": self.sentence(),
        }))
    }

    /// `keep_join` — nothing removed here, and a whole answer: the pass is over.
    pub fn keep_join(&mut self) -> String {
        self.kept = true;
        self.dropped.clear();
        crate::tools::ok(&json!({"kept": true, "removed": 0}))
    }

    /// Whether `keep_join` was the answer.
    pub fn kept(&self) -> bool {
        self.kept
    }

    /// `get_words(side, count)` — more words than the brief's [`SEAM_REACH_WORDS`] on either side.
    ///
    /// Not capped by it: the cap is what a model is *shown*, and asking for more is how it checks a
    /// seam that looked longer than the extract. What comes back is bounded by what exists, so a
    /// count past the end of the take returns the whole side rather than padding.
    pub fn get_words(&self, side: Side, count: usize) -> Vec<String> {
        let words = self.all(side);
        let kept = count.min(words.len());
        match side {
            Side::Before => words[words.len() - kept..].to_vec(),
            Side::After => words[..kept].to_vec(),
        }
    }

    /// Words dropped so far from one side.
    pub fn dropped_count(&self, side: Side) -> usize {
        self.dropped
            .iter()
            .filter(|(at, _)| *at == side)
            .map(|(_, count)| count)
            .sum()
    }

    /// The words that will actually go, in the order they were spoken.
    pub fn words_taken(&self, side: Side) -> Vec<String> {
        let count = self.dropped_count(side);
        if count == 0 {
            return Vec::new();
        }
        self.get_words(side, count)
    }

    /// The sentence left reading across the join, which is what the model is really being asked to
    /// approve.
    pub fn sentence(&self) -> String {
        let mut out: Vec<&str> = self
            .all(Side::Before)
            .iter()
            .map(String::as_str)
            .take(self.all(Side::Before).len() - self.dropped_count(Side::Before))
            .collect();
        out.extend(
            self.all(Side::After)
                .iter()
                .map(String::as_str)
                .skip(self.dropped_count(Side::After)),
        );
        out.join(" ")
    }

    /// The whole of one side, before any cap.
    fn all(&self, side: Side) -> &[String] {
        match side {
            Side::Before => &self.before,
            Side::After => &self.after,
        }
    }
}

/// Whether the words named for one side come to exactly one stretch at the join.
///
/// The app re-derives this (§3.5) because a count is not a location: two drops on opposite sides are
/// two stretches and a join in the middle of them, which no amount of trimming fixes. `stretches`
/// holds, per side, how many separate runs of words were named — more than one run on either side is
/// not one stretch, and neither is a run that does not start at the seam (a gap before it means the
/// model skipped words in the middle).
pub fn one_stretch_at_join(before_runs: &[usize], after_runs: &[usize]) -> bool {
    // One stretch total: exactly one side gives one run, and the other gives none.
    let runs = before_runs.len() + after_runs.len();
    if runs != 1 {
        return false;
    }
    // A zero-length run is not a stretch at all — it is the gap §3.5 refuses.
    before_runs
        .iter()
        .chain(after_runs.iter())
        .all(|length| *length > 0)
}

/// The side's name as the model wrote it in the table.
fn name(side: Side) -> &'static str {
    match side {
        Side::Before => "before",
        Side::After => "after",
    }
}
