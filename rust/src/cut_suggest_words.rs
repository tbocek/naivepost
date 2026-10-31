//! F2.14 §2/§5 — the WORD GATE the ▶ press passes before anything else.

use crate::cut::Cut;
use crate::layout::Tree;
use crate::timeline::Recording;

fn list_with(items: &[&str], joiner: &str) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{}{joiner}{last}", rest.join(", ")),
        Some((last, _)) => (*last).to_string(),
        None => String::new(),
    }
}

/// §1's gate: the base cut's recordings and the project tree decide whether words exist at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words {
    /// §1 S2: the base cut's recordings have no words (no file, an unreadable file, or an aligner
    /// that did not answer `ok`).
    Needed,
    Present,
}

/// §1's refusal sentence, printed when words are needed.
pub const WORDS_NEEDED: &str = "Words needed \u{2013} add them with \u{2710} Captions first";

/// §5's more specific reason for the two causes that have one: no file at all is the same refusal.
pub fn reason_for(states: &[WordState]) -> Option<String> {
    let causes = reason_causes(states);
    // Every recording with no word file at all is §1's plain cause: nothing was ever transcribed,
    // so there is nothing more specific to say than the sentence the spec gives.
    if causes.len() == 1 && causes[0] == "no words.json" {
        return None;
    }
    if causes.is_empty() {
        return None;
    }
    let named = list_with(&causes, " and ");
    let failed = states.len()
        - states
            .iter()
            .filter(|s| matches!(s, WordState::Usable))
            .count();
    Some(format!(
        "{failed} of {} recording(s) have words we cannot use: {named}",
        states.len()
    ))
}

/// The causes worth naming for these readings — the three §5 spells, deduplicated, in the order the
/// recordings show them.
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
    Missing,
    Unreadable,
    NotAligned,
    Usable,
}

/// §1's answer for one set of readings: any recording whose words are not usable and the cut
/// actually depends on, and the whole gate is `Needed`.
pub fn gate(states: &[WordState]) -> Words {
    if states
        .iter()
        .any(|state| !matches!(state, WordState::Usable))
    {
        Words::Needed
    } else {
        Words::Present
    }
}

/// The recordings the base cut depends on, as `(source, camera)` pairs: the camera rows the cut
/// actually shows, in the order the session lists them.
pub fn depended_recordings(cut: &Cut, recordings: &[Recording]) -> Vec<String> {
    let cams = depended_cam_slots(cut);
    recordings
        .iter()
        .filter(|rec| cams.iter().any(|cam| format!("cam{cam}") == rec.base))
        .map(|rec| rec.base.clone())
        .collect()
}

pub fn depended_cam_slots(cut: &Cut) -> Vec<i32> {
    let mut cams: Vec<i32> = cut.segs.iter().map(|seg| seg.cam).collect();
    cams.sort_unstable();
    cams.dedup();
    cams
}

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

/// One recording's state, read from the two files §5 names.
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
/// what to print.
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
