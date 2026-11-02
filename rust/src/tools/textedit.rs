//! Text edit — spec/02-services.md §3.5, one join at a time.

use crate::prepare_decisions;
use serde_json::json;

/// How many words each side of the join is shown at. P.machine.seamReachWords
pub const SEAM_REACH_WORDS: usize = 140;

/// Most words one join may remove. P.machine.seamMaxWords
pub const SEAM_MAX_WORDS: usize = 40;

/// Most of the words shown that one join may remove. P.machine.seamCeil
pub const SEAM_CEIL: f64 = 0.6;

pub const SEAM_SNAP_WORDS: usize = 3;

pub const SEAM_NOISE_WORDS: usize = 2;

pub const JOIN_REACH_WORDS: usize = 3;

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
    pub fn new(before: &[&str], after: &[&str]) -> Join {
        Join {
            before: before.iter().map(|word| word.to_string()).collect(),
            after: after.iter().map(|word| word.to_string()).collect(),
            dropped: Vec::new(),
            kept: false,
        }
    }

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

    pub fn sentence(&self) -> String {
        let before_all = self.all(Side::Before);
        let after_all = self.all(Side::After);
        let mut out: Vec<&str> = before_all
            .iter()
            .map(String::as_str)
            .take(
                before_all
                    .len()
                    .saturating_sub(self.dropped_count(Side::Before)),
            )
            .collect();
        out.extend(
            after_all
                .iter()
                .map(String::as_str)
                .skip(self.dropped_count(Side::After).min(after_all.len())),
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

pub fn joins_repaired_log(repaired: usize, cached: usize) -> String {
    format!(">>> text edit: {repaired} join(s) repaired, {cached} from the cache")
}

pub fn dedupe_note(seconds: f64, text: &str) -> String {
    format!("{seconds}: {text:?} said again straight after the cut -- the earlier one goes")
}

pub fn dedupe_log_head(note: &str) -> String {
    format!("{} {note}", crate::join_ask::JOIN_PREFIX)
}

pub fn dedupe_joins(words: &[String], times: &[(f64, f64)], kept: &mut [bool]) -> Vec<String> {
    let mut notes = Vec::new();
    // From 1: index 0 has nothing before it, so it can never be the head of a doubled saying.
    for j in 1..words.len() {
        if kept[j] || !kept[j - 1] {
            continue; // not the first dropped word of a run
        }
        let mut k = j;
        while k < words.len() && !kept[k] {
            k += 1;
        }
        if k >= words.len() {
            break; // the drop runs to the end: no far side to duplicate
        }

        // The kept words nearest the join on each side, at most the reach, ascending. The cap counts
        // KEPT words and stops at the first dropped one, so neither side offers more than this.
        let mut before: Vec<usize> = Vec::new();
        let mut i = j - 1;
        loop {
            if kept[i] {
                before.insert(0, i);
                if before.len() == JOIN_REACH_WORDS {
                    break;
                }
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        let mut after: Vec<usize> = Vec::new();
        let mut i = k;
        while i < words.len() && kept[i] && after.len() < JOIN_REACH_WORDS {
            after.push(i);
            i += 1;
        }

        // Longest first: the whole doubled tail is what went, and taking the longest match keeps the
        // later copy whole rather than leaving half of the earlier copy behind.
        for n in (1..=before.len().min(after.len())).rev() {
            let same = (0..n).all(|x| {
                prepare_decisions::same_word(&words[before[before.len() - n + x]], &words[after[x]])
            });
            if !same {
                continue;
            }
            let go = &before[before.len() - n..];
            for &idx in go {
                kept[idx] = false;
            }
            let text = go
                .iter()
                .map(|&idx| words[idx].as_str())
                .collect::<Vec<_>>()
                .join(" ");
            notes.push(dedupe_note(times[go[0]].0, &text));
            break; // one fix per join, the longest match only
        }
    }
    notes
}

/// The side's name as the model wrote it in the table.
fn name(side: Side) -> &'static str {
    match side {
        Side::Before => "before",
        Side::After => "after",
    }
}
