//! §07-narrate#1-screen (**22**): the two counts the Inputs row brackets, computed from the session.
//!
//! [`NarrateState`] carries `unwritten` / `off_cut` / `has_cut` / `has_timeline`, but a flow publishes the
//! rows and the cut, not the arithmetic: nothing used to fill those four, so the page drew its Inputs line
//! with the bracketed halves missing even in a session that was plainly stale. This module derives them
//! from what the page already knows — the folder, the cut on disk and the narration — at the two doors that
//! paint the page ([`narrate_page::build`]'s seed and [`narrate_page::refresh`]).
//!
//! Nothing here re-derives a rule. "No line and not deliberately silent" is
//! [`crate::narrate_rewrite::indexes_without_line`] beside [`crate::narration::Narration::is_silent`], the
//! same pair [`crate::ui::narrate_inputs::tooltip`] uses for the row's hover copy, and "on video the cut no
//! longer has" is [`crate::narrate_run::lines_off_cut`], the count ▶ refuses with. The timeline question is
//! the one the shell answers for the tab's own lock (`session.tsv` on disk).

use crate::cut::Seg;
use crate::layout::Tree;
use crate::narrate_run;
use crate::narrate_rewrite;
use crate::narration::{Entry, Narration};

/// The four numbers §1's Inputs line prints after the clip count and the length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    /// Clips of the cut carrying no entry and not deliberately silent.
    pub unwritten: usize,
    /// Entries sitting on video the cut no longer has.
    pub off_cut: usize,
    /// The page's cut has clips at all — without them the row says "no cut yet" instead of counting.
    pub has_cut: bool,
    /// The session transcript timeline exists (`session.tsv`).
    pub has_timeline: bool,
}

/// The page's own cut (its `segs` mirror what a flow last published), counted against `entries`.
///
/// With no clips there is nothing to count, so `has_cut` says so and the two counts come back zero:
/// [`crate::narrate_screen::inputs_readout`] replaces the whole line with its no-cut sentence, and a
/// half-counted stale mark beside it would contradict that.
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

/// The tally as the page sees it: the cut on disk, the narration the page shows, and the project's own
/// silence markers (from `narration.json`, so a clip someone silenced with 🗑 stays counted as silent even
/// though its line is gone). No folder open leaves `has_timeline` false — nothing was transcribed.
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

/// The tally for this session, written into `state` in place. It leaves `has_cut` alone: whether the page has
/// a cut at all is the shell's answer (its Narrate tab is locked without one), so the state's own word stands,
/// and only the two counts and the timeline question are the page's arithmetic.
pub fn apply(state: &mut crate::ui::narrate_page::NarrateState, tree: Option<&Tree>) {
    let tally = from_session(tree, &state.segs, &state.entries);
    state.unwritten = tally.unwritten;
    state.off_cut = tally.off_cut;
    state.has_timeline = tally.has_timeline;
}

/// Fill the tally ONLY where the state still carries what a fresh page starts with. A flow that published
/// counts of its own keeps them — the page may not overwrite an answer a caller computed for a moment that
/// has already passed (the same reason `narrate_off::skips` refuses to recompute the run's counts per step).
pub fn fill_defaults(state: &mut crate::ui::narrate_page::NarrateState, tree: Option<&Tree>) {
    if state.unwritten != 0 || state.off_cut != 0 || state.has_timeline {
        return;
    }
    apply(state, tree);
}
