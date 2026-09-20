//! Hand-edit the text (spec/04-prepare.md F1.12): the user deletes words in `prepare/transcript/final.txt`
//! in any text editor — there is no editor in the app — and the next Cut ▶ remakes the marks from that
//! text without asking a model.
//!
//! The match runs backwards, which is what makes an edit cheap: the text's tail lines up with the
//! speech's tail, so deleting a word in the middle cannot shift everything after it out of place. A word
//! in the file that nothing spoken matches was never said and is left out — there is no sound for it —
//! and an edit that removes more than the retake ceiling is refused outright rather than quietly cutting
//! half the video away.

use std::time::SystemTime;

use crate::edges::{self, Edges};
use crate::layout::Tree;
use crate::textfmt::{self, read_final, Retake};
use crate::tools::mm_ss;
use crate::tools::retakes::RETAKE_CEIL;

/// How many words back the match may look for the text's next word: past that, the word was not said —
/// it is a typing, or a word from a sentence the user rewrote, and there is no sound for it.
pub const KEEP_REACH: usize = 200;

/// What the run says when it takes the edit over. The model is not asked, which is the point: the text
/// is the edit, and asking again would let a model undo what the user just typed.
pub const EDITED_NOTE: &str = ">>> final.txt was edited after Prepare -- the marks are remade from it, no model asked";

/// S2: is the text newer than the marks? A missing `final.txt` is nothing to read; a missing
/// `retakes.tsv` means nothing has been marked yet, so an edit there is one waiting to be used. Equal
/// times are not newer — the model's marks stand and no work is redone.
pub fn edited(final_mtime: Option<SystemTime>, marks_mtime: Option<SystemTime>) -> bool {
    match final_mtime {
        Some(text) => !marks_mtime.is_some_and(|marks| text <= marks),
        None => false,
    }
}

/// The words of the edited text, join marks read out by [`read_final`] and any pause marker the file was
/// written with dropped along with them. A hand edit can break the lines anywhere it likes; a pipe never
/// occurs in a transcript, so even a mangled mark goes rather than becoming a word.
fn text_words(text: &str) -> Vec<String> {
    let bare = PAUSE_MARK.replace_all(text, " ");
    read_final(bare.as_ref())
        .0
        .into_iter()
        .filter(|word| word != "SEAM")
        .collect()
}

/// The pause marker `final.txt` can hold — `(pause 1.2s)` — which is a mark, never a word.
static PAUSE_MARK: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"\(pause [0-9.]+s\)").unwrap());

/// S2: which spoken words the edited text still holds, and how many of its words were never said.
///
/// Walked from the end in both: the edit is a deletion, so the tails agree, and matching forwards would
/// let one mismatch near the start misalign every word after it — turning an edit of two words into an
/// apparent edit of two hundred.
pub fn keep_mask(words: &[String], out: &[String]) -> (Vec<bool>, usize) {
    let mut kept = vec![false; words.len()];
    let mut extra = 0usize;
    let mut cursor = words.len();
    for wanted in out.iter().rev() {
        // Search back from the cursor, no further than the reach: a word that only matches across half
        // the recording is not this word. The window is `KEEP_REACH` *candidates*, counted off the
        // cursor itself — which is what makes it independent of how much was deleted above.
        // The window is KEEP_REACH *candidates*, counted off the cursor rather than off the word's own
        // index — so how far the match may look does not depend on how much was deleted above it.
        let found = (1..=cursor.min(KEEP_REACH)).find(|&back| words[cursor - back] == *wanted);
        match found {
            Some(back) => {
                let k = cursor - back;
                kept[k] = true;
                cursor = k;
            }
            None => extra += 1,
        }
    }
    (kept, extra)
}

/// S2: every maximal run of dropped words becomes one mark.
///
/// The seconds come in beside the words rather than inside them, which is what lets a test hand over a
/// synthetic session and keeps this function about runs. Hand-editing has no notion of "said again":
/// there is no second take to read instead, so `again` stays nought and `to` equals the run's own end —
/// the words are all that goes.
pub fn marks_from(times: &[(f64, f64)], kept: &[bool]) -> Vec<Retake> {
    let mut marks = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    for (index, keep) in kept.iter().enumerate().take(times.len()) {
        match (keep, run) {
            (false, None) => run = Some((index, index)),
            (false, Some((first, _))) => run = Some((first, index)),
            (true, Some((first, last))) => {
                marks.push(mark(times, first, last));
                run = None;
            }
            (true, None) => {}
        }
    }
    if let Some((first, last)) = run {
        marks.push(mark(times, first, last));
    }
    marks
}

fn mark(times: &[(f64, f64)], first: usize, last: usize) -> Retake {
    Retake {
        s: times[first].0,
        e: times[last].1,
        again: 0.0,
        to: times[last].1,
        text: String::new(),
        whole: String::new(),
    }
}

/// What remaking the marks says and produces.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub marks: Vec<Retake>,
    pub logs: Vec<String>,
    pub dropped: usize,
    pub extra: usize,
    /// The edit took more than the ceiling away: nothing is marked, and what was there stands.
    pub refused: bool,
}

/// S2: the marks remade from the edited text, no model asked.
///
/// `edge_of` answers with the envelope of the recording a session second belongs to — the same lookup
/// [`crate::edges`] takes — so an edge can move onto the sound; `None` leaves it on the word times, which
/// is what a run with no audio to ask has.
///
/// Not here on purpose: `dedupeJoins` (the "said again straight after the cut" note) and the join-repair
/// pass belong to the passes that produce those marks (F1.9, F1.10). This remakes marks from the text it
/// was handed, and [`textfmt::read_final`] already tolerates the join marks staying or going.
pub fn remake<'a>(
    text: &str,
    words: &[String],
    times: &[(f64, f64)],
    mut edge_of: impl FnMut(f64) -> Option<&'a Edges>,
) -> Outcome {
    let (kept, extra) = keep_mask(words, &text_words(text));
    let mut out = Outcome { extra, ..Default::default() };
    out.logs.push(EDITED_NOTE.to_string());
    if extra > 0 {
        out.logs.push(format!(
            "!!! text edit: {extra} word(s) in the text were never said -- left out, there is no sound for them"
        ));
    }

    out.dropped = kept.iter().filter(|keep| !**keep).count();
    if times.is_empty() {
        // Nothing was ever spoken, so there is nothing to mark and nothing to refuse.
        return out;
    }
    if out.dropped as f64 > RETAKE_CEIL * times.len() as f64 {
        out.logs.push(format!(
            "!!! text edit: {} of {} words removed -- refused, that is not an edit, nothing is marked",
            out.dropped,
            times.len()
        ));
        out.refused = true;
        return out;
    }

    let mut marks = marks_from(times, &kept);
    // The text names the words, so the marks can: it is what a person reads in `retakes.tsv`. Named off
    // the spoken list and clipped to the run's own seconds, which is what keeps the name identical whether
    // or not an envelope went on to move the edges.
    for mark in marks.iter_mut() {
        mark.text = words
            .iter()
            .zip(times)
            .filter(|(_, (start, _))| *start >= mark.s && *start < mark.to)
            .map(|(word, _)| word.as_str())
            .collect::<Vec<_>>()
            .join(" ");
    }

    let (marks, notes) = edges::place_edges(marks, &[], &mut edge_of, &[]);
    out.logs.extend(notes);
    for mark in &marks {
        out.logs.push(format!(
            ">>> text edit: {}-{} goes ({:?})",
            mm_ss(mark.s),
            mm_ss(mark.to),
            mark.text
        ));
    }
    out.logs.push(format!(
        ">>> text edit: {} of {} words removed in {} stretch(es)",
        out.dropped,
        times.len(),
        marks.len()
    ));
    out.marks = marks;
    out
}

/// Write the text where Prepare keeps it — through the same 0644 writer, so a hand edit lands on a file
/// the app made and the folder that holds it.
pub fn write_final(tree: &Tree, text: &str) -> Result<(), String> {
    let path = tree.final_txt();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    std::fs::write(&path, text).map_err(|err| format!("{}: {err}", path.display()))
}

/// Write the remade marks where Cut reads them. The fresh file's own mtime is what stops the next Cut ▶
/// remaking the same edit — "remade once" is structural rather than remembered.
pub fn write_marks(tree: &Tree, marks: &[Retake]) -> Result<(), String> {
    textfmt::write_retakes(marks, &tree.retakes_tsv())
}
