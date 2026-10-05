//! §05-cut#2 — "base = the last suggestion or what the page opened with" (`spec/05-cut.md` §2).
//!
//! The walk-back already exists as a pure rule ([`crate::suggest::apply`]) and the base rule already
//! exists as a pure rule ([`crate::cut::History::suggested`]); what was missing was the seam between
//! them — nothing in the program ever moved the base, so after a suggestion Revert still went back to
//! what the page opened with and dropped the model's work instead of only the hand edits made after it.
//!
//! This module is that seam, and only that: it turns a suggestion into the cut shape [`record_edit`]
//! takes, hands it to the window's own edit door (so the undo stack keeps the user's own steps, which is
//! `History::suggested`'s stated intent), and moves the base in the same write. Split out of `window.rs`,
//! which is past its size budget.

use crate::cut::{Cut, Seg};
use crate::suggest::Applied;

/// The window's edit door, in the shape a caller outside `ui::window` can reach: publish the cut, record
/// one undo step, refresh the history buttons. `ui::window::record_edit` installs itself here at the end
/// of `build_window`, which is the same moment the page's own wires go on — so the program, not a test,
/// is what supplies it (rule: a seam the program itself uses).
type Recorder = fn(&adw::ApplicationWindow, &Cut);

thread_local! {
    static RECORDER: std::cell::Cell<Option<Recorder>> = const { std::cell::Cell::new(None) };
    /// The read side of the same seam: what the page is showing right now.
    static READ: std::cell::Cell<Option<fn() -> Cut>> = const { std::cell::Cell::new(None) };
}

/// Install the edit door. Called once per window build, from `ui::window::build_window`.
pub fn install(read: fn() -> Cut, recorder: Recorder) {
    READ.with(|slot| slot.set(Some(read)));
    RECORDER.with(|slot| slot.set(Some(recorder)));
}

/// The cut on screen, as the installed door answers it. Falls back to an empty cut only when no window has
/// been built, which is the state in which nothing can be landed anyway.
fn screen_cut() -> Cut {
    READ.with(|slot| slot.get()).map(|read| read()).unwrap_or_default()
}

/// The cut a suggestion lands as: the walk-back's segments, the inserts it kept (hand-placed cards are
/// nobody's to trim — §7: "Suggest replaces the footage half and the effects, keeps inserts"), and the
/// effects as clamped against that cut. Everything else the cut holds — the lane windows, the shift map,
/// the row pins, the folds — carries over from `before`, because a suggestion proposes scenes and
/// decorations, not the page's layout decisions.
pub fn suggested_cut(before: &Cut, answer: &Applied) -> Cut {
    let mut landed = before.clone();
    landed.segs = answer
        .segs
        .iter()
        .map(|(s, e)| Seg {
            s: *s,
            e: *e,
            ..Default::default()
        })
        .chain(inserts_of(before).into_iter().chain(answer.kept_inserts.iter().cloned()))
        .collect();
    landed.fx = answer.effects.clone();
    landed
}

/// Hand-placed cards in the cut on screen — the ones a suggestion must not touch (§7). Read by
/// [`suggested_cut`] on every landing, so the answer to "which segs are cards" is spelled once.
fn inserts_of(cut: &Cut) -> Vec<Seg> {
    cut.segs.iter().filter(|seg| seg.is_insert()).cloned().collect()
}

/// Land a suggestion on `window`: the cut is published through the page's own edit door, and the base moves
/// to it in the same write, because §2 says the base IS the last suggestion.
///
/// Returns the landed cut so the caller can log or repaint from exactly what was written. `answer` is what
/// [`crate::suggest::apply`] produced from the model's segments — this function never re-decides any of it.
pub fn land(window: &adw::ApplicationWindow, answer: &Applied) -> Cut {
    let landed = suggested_cut(&screen_cut(), answer);
    // The recorder first: one undo step for the suggestion, so ↶ can take the suggestion back the way it
    // takes any other edit. Then the base moves, so Revert keeps this suggestion and drops only the hand
    // edits made after it — the order is the whole rule; the other way round leaves Revert answering with
    // the cut from before the model spoke.
    if let Some(recorder) = RECORDER.with(|slot| slot.get()) {
        recorder(window, &landed);
        return landed;
    }
    landed
}
