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

/// How far back before a cut a restart is looked for, in kept words. P.eng.restartReachWords
pub const RESTART_REACH_WORDS: usize = 60;

/// Fewest words in a row the take after a cut must open with, said already before it, to count as a restart.
/// P.eng.restartMinWords. Over the 22 lecture projects even 3 found only the one real restart; 5 keeps the margin.
pub const RESTART_MIN_WORDS: usize = 5;

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
        // Both ceilings count words that were said: an entry the fixed transcript left empty (a word
        // heard and then written out of the text) is not a word on either side (09-dao, a 34-word
        // first take with 12 empty entries after it counted 46 and was refused).
        let asked = already + count;
        // Asked past the end of the side (across calls) still counts every one asked for.
        let total =
            spoken(&self.get_words(side, asked)) + asked.saturating_sub(self.all(side).len());
        if total > SEAM_MAX_WORDS {
            return crate::tools::error(&format!(
                "that is {total} words from the {} side -- one join removes at most \
                 {SEAM_MAX_WORDS}",
                name(side)
            ));
        }
        let shown_total = spoken(self.shown(Side::Before)) + spoken(self.shown(Side::After));
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

/// How many of these entries are words that were said: an empty one (written out of the fixed text) is not.
fn spoken(words: &[String]) -> usize {
    words
        .iter()
        .filter(|word| !crate::word_list::bare(word).is_empty())
        .count()
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

pub fn restart_note(seconds: f64, text: &str) -> String {
    format!("{seconds}: {text:?} started over after the cut -- the first attempt goes")
}

/// A restart across a cut: the take after the cut opens with [`RESTART_MIN_WORDS`] or more words said already in the
/// last [`RESTART_REACH_WORDS`] kept words before it. The first attempt begins where those words were said first, and
/// everything from there to the cut goes; the take after the cut stays whole.
///
/// [`dedupe_joins`] only sees a repeat that touches the cut. When the join model trims the first attempt's tail but
/// not its start, the repeat sits a sentence back: 09-dao, "Und das dritte Problem: Stimmen kann man kaufen. Das beste
/// Beispiel sind die Curve Wars. Bei Curve entscheiden die Besitzer von" kept, then the new take said it all again.
pub fn restart_joins(words: &[String], times: &[(f64, f64)], kept: &mut [bool]) -> Vec<String> {
    let mut notes = Vec::new();
    for j in 1..words.len() {
        if kept[j] || !kept[j - 1] {
            continue; // not the first dropped word of a run
        }
        let mut k = j;
        while k < words.len() && !kept[k] {
            k += 1;
        }
        if k >= words.len() {
            break;
        }
        let mut before: Vec<usize> = (0..j)
            .rev()
            .filter(|&i| kept[i])
            .take(RESTART_REACH_WORDS)
            .collect();
        before.reverse();
        let after: Vec<usize> = (k..words.len())
            .take_while(|&i| kept[i])
            .take(RESTART_REACH_WORDS)
            .collect();
        // The earliest start of the longest opening said again: the whole first attempt goes, not half of it.
        let mut best: Option<(usize, usize)> = None;
        for from in 0..before.len() {
            let n = (0..after.len().min(before.len() - from))
                .take_while(|&x| {
                    prepare_decisions::same_word(&words[before[from + x]], &words[after[x]])
                })
                .count();
            if n >= RESTART_MIN_WORDS && best.is_none_or(|(_, longest)| n > longest) {
                best = Some((from, n));
            }
        }
        let Some((from, _)) = best else { continue };
        let go = &before[from..];
        for &idx in go {
            kept[idx] = false;
        }
        let text = go
            .iter()
            .map(|&idx| words[idx].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        notes.push(restart_note(times[go[0]].0, &text));
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
