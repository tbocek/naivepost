//! F2.14 §2/§5 — the WORD GATE the ▶ press passes before anything else.
//!
//! §1 refuses the press and leaves the page alone when the base cut's recordings have no usable
//! words: no `words.json`, or an aligned file that does not say `ok`. That decision needs the
//! session's recordings, the project tree and the base cut, so it belongs here rather than in
//! [`crate::suggest::why_refused`], which answers the press-shaped reason from flags only.
//!
//! This module is also where §2's four kinds of snap point are accounted for: they live ONCE as
//! [`crate::cut_effect_decisions::Candidate`], the enum `walk_of` stamps candidates with and
//! [`crate::suggest::snap_edge`] scores, so no second list of words for the same four places can drift.
//!
//! The wording lives here, in a file of its own, because `src/suggest.rs` is at 2500 lines and
//! §12's growth budget leaves no room in it for a paragraph.

use crate::cut::Cut;
use crate::layout::Tree;
use crate::timeline::Recording;

/// "a, b, c" with the last separator replaced by `joiner`: "a, b or c". `List`s are joined this way in
/// two sentences here and nowhere else, so the helper stays local and private.
fn list_with(items: &[&str], joiner: &str) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{}{joiner}{last}", rest.join(", ")),
        Some((last, _)) => (*last).to_string(),
        None => String::new(),
    }
}

/// §1's gate: the base cut's recordings and the project tree decide whether words exist at all.
///
/// Recordings come as `(source, camera)` pairs — the camera slot a strip carries is the same slot a
/// `words.json` is filed under (§2), so this takes the pair rather than borrowing an `Alignment`.
/// Readings come in as `Option<Option<String>>`, in the same order as `recordings`: the outer layer
/// is "was the file readable at all" (an unreadable file is a failure of reading, not of alignment),
/// the inner is "did `words.aligned.json` say `ok`". A test supplies them; the program reads them
/// from disk through [`read_word_states`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words {
    /// §1 S2: the base cut's recordings have no words (no file, an unreadable file, or an aligner
    /// that did not answer `ok`). Nothing runs; the caller prints [`WORDS_NEEDED`] and, for the
    /// second and third causes, the more specific sentence §5 asks for.
    Needed,
    /// Words exist for every recording the cut depends on.
    Present,
}

/// §1's refusal sentence, printed when words are needed. `spec/05-cut.md` §1:
/// "▶ says 'Words needed – add them with ✐ Captions first' and runs nothing". The glyph is the one
/// the run bar's own button carries (`ui::window::CAPTIONS_PASS_LABEL`), so the sentence points at the
/// button it names rather than a look-alike.
pub const WORDS_NEEDED: &str = "Words needed \u{2013} add them with \u{2710} Captions first";

/// §5's more specific reason for the two causes that have one: no file at all is the same refusal;
/// an unreadable or not-ok file names which. Each string lands in `cut.log.json` next to the
/// press's time, alongside the decision §1 made.
pub fn reason_for(states: &[WordState]) -> Option<String> {
    let causes = reason_causes(states);
    // Every recording with no word file at all is §1's plain cause: nothing was ever transcribed, so
    // there is nothing more specific to say than the sentence the spec gives.
    if causes.len() == 1 && causes[0] == "no words.json" {
        return None;
    }
    if causes.is_empty() {
        return None;
    }
    let named = list_with(&causes, " and ");
    // Every recording that failed is counted, whatever its cause: a person reading this has to know
    // how many rows are affected.
    let failed = states.len() - states.iter().filter(|s| matches!(s, WordState::Usable)).count();
    Some(format!(
        "{failed} of {} recording(s) have words we cannot use: {named}",
        states.len()
    ))
}

/// The causes worth naming for these readings — the three §5 spells, deduplicated, in the order the
/// recordings show them. Empty means there is nothing beyond the plain refusal to say.
fn reason_causes(states: &[WordState]) -> Vec<&'static str> {
    let mut causes: Vec<&'static str> = Vec::new();
    for state in states {
        let cause = match state {
            WordState::Missing => "no words.json",
            WordState::Unreadable => "words.json unreadable",
            WordState::NotAligned => "not aligned",
            WordState::Usable => continue,
        };
        if !causes.contains(&cause) {
            causes.push(cause);
        }
    }
    causes
}

/// One recording's word state, as §5's three causes and the usable case name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordState {
    /// No `words.json` for that recording at all.
    Missing,
    /// A `words.json` is there but could not be read.
    Unreadable,
    /// `words.aligned.json` did not answer `ok` — the words are not trustworthy yet.
    NotAligned,
    /// Words exist and are aligned: the press may run.
    Usable,
}

/// §1's answer for one set of readings: any recording whose words are not usable and the cut
/// actually depends on, and the whole gate is `Needed`.
pub fn gate(states: &[WordState]) -> Words {
    if states.iter().any(|state| !matches!(state, WordState::Usable)) {
        Words::Needed
    } else {
        Words::Present
    }
}

/// The recordings the base cut depends on, as `(source, camera)` pairs: the camera rows the cut
/// actually shows, in the order the session lists them. A cut with no segments depends on nothing,
/// which is why an empty cut answers `Present` — there are no words that could be missing.
pub fn depended_recordings(cut: &Cut, recordings: &[Recording]) -> Vec<String> {
    let cams = depended_cam_slots(cut);
    recordings
        .iter()
        .filter(|rec| cams.iter().any(|cam| format!("cam{cam}") == rec.base))
        .map(|rec| rec.base.clone())
        .collect()
}

/// The camera slots the cut shows — the slots whose recordings the word gate reads files for.
/// A lane segment carries its own camera too, so a lane's words count like any other row's.
pub fn depended_cam_slots(cut: &Cut) -> Vec<i32> {
    let mut cams: Vec<i32> = cut.segs.iter().map(|seg| seg.cam).collect();
    cams.sort_unstable();
    cams.dedup();
    cams
}

/// Read the two word files of each recording the cut depends on and answer the gate plus the reason.
/// `Err` carries the failure of READING (a tree that cannot be opened), which is not the same thing
/// as words being missing: §5 keeps those two causes apart, and only the first is a log-worthy
/// "we could not look".
/// The `Vec` lines up with [`depended_recordings`], one entry per recording the cut shows.
pub fn read_word_states(
    tree: &Tree,
    cut: &Cut,
    recordings: &[Recording],
) -> Result<Vec<WordState>, String> {
    let mut states = Vec::new();
    for source in depended_recordings(cut, recordings) {
        states.push(one_state(tree, &source)?);
    }
    Ok(states)
}

/// One recording's state, read from the two files §5 names. `read_aligned` answers `Ok(None)` for
/// "no `words.aligned.json`" (§04 F1.9), and both files hold their words, not a status field: the
/// aligner's answer IS its word list, so a non-empty `words.aligned.json` is what "the aligner
/// answered" means on disk, and an empty one is an aligner that did not.
fn one_state(tree: &Tree, source: &str) -> Result<WordState, String> {
    match crate::requests::read_words(tree, source) {
        Err(_why) => Ok(WordState::Unreadable),
        Ok(None) => Ok(WordState::Missing),
        Ok(Some(words)) if words.words.is_empty() => Ok(WordState::Missing),
        Ok(Some(_)) => match crate::requests::read_aligned(tree, source)? {
            Some(doc) if !doc.words.is_empty() => Ok(WordState::Usable),
            _ => Ok(WordState::NotAligned),
        },
    }
}

/// The §1 press seam: given the window's session and base cut, answer whether the press may run and
/// what to print. `states` is what [`read_word_states`] returns, or the error of reading; a failed
/// read is reported as a refusal whose reason names the read itself, because §5 says a cause must be
/// nameable and "we could not open the project" is a cause.
pub fn press_answer(states: &Result<Vec<WordState>, String>) -> (bool, String) {
    match states {
        Err(why) => (
            false,
            format!("{WORDS_NEEDED} \u{2014} the words could not be read: {why}"),
        ),
        Ok(states) if states.is_empty() => (true, String::new()),
        Ok(states) => match gate(states) {
            Words::Present => (true, String::new()),
            Words::Needed => match reason_for(states) {
                Some(more) => (false, format!("{WORDS_NEEDED} \u{2014} {more}")),
                None => (false, WORDS_NEEDED.to_string()),
            },
        },
    }
}
