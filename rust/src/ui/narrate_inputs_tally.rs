//! §07-narrate#1-screen (**22**): the two counts the Inputs row brackets, computed from the
//! session.

use crate::cut::Seg;
use crate::layout::Tree;
use crate::narrate_rewrite;
use crate::narrate_run;
use crate::narration::{Entry, Narration};

/// The four numbers §1's Inputs line prints after the clip count and the length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    pub unwritten: usize,
    pub off_cut: usize,
    /// The page's cut has clips at all — without them the row says "no cut yet" instead of
    /// counting.
    pub has_cut: bool,
    pub has_timeline: bool,
}

pub fn of(clips: &[(f64, f64)], entries: &[Entry], silent: bool, has_timeline: bool) -> Tally {
    if clips.is_empty() {
        return Tally {
            unwritten: 0,
            off_cut: 0,
            has_cut: false,
            has_timeline,
        };
    }
    let narration = Narration {
        entries: entries.to_vec(),
        silent: if silent {
            clips
                .iter()
                .map(|(start, end)| crate::narration::Silent { s: *start, e: *end })
                .collect()
        } else {
            Vec::new()
        },
    };
    Tally {
        unwritten: narrate_rewrite::indexes_without_line(clips, &narration)
            .iter()
            .filter(|index| !narration.is_silent(clips[**index].0, clips[**index].1))
            .count(),
        off_cut: narrate_run::lines_off_cut(clips, &narration),
        has_cut: true,
        has_timeline,
    }
}

pub fn from_session(tree: Option<&Tree>, segs: &[Seg], entries: &[Entry]) -> Tally {
    let clips: Vec<(f64, f64)> = segs.iter().map(|seg| (seg.s, seg.e)).collect();
    let silent = tree
        .and_then(|tree| crate::narration::load(tree).ok())
        .is_some_and(|saved| !saved.silent.is_empty());
    of(
        &clips,
        entries,
        silent,
        tree.is_some_and(|tree| tree.session_tsv().is_file()),
    )
}

pub fn apply(state: &mut crate::ui::narrate_page::NarrateState, tree: Option<&Tree>) {
    let tally = from_session(tree, &state.segs, &state.entries);
    state.unwritten = tally.unwritten;
    state.off_cut = tally.off_cut;
    state.has_timeline = tally.has_timeline;
}

pub fn fill_defaults(state: &mut crate::ui::narrate_page::NarrateState, tree: Option<&Tree>) {
    if state.unwritten != 0 || state.off_cut != 0 || state.has_timeline {
        return;
    }
    apply(state, tree);
}
