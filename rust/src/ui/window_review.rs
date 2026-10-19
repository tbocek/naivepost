//! F3.12's review-cut doors, split out of `window.rs` (which is at its size budget).
//!
//! These are the same names, reached through their old paths: `ui::clamp_effects_to_cut` for the verb
//! report and the tests, `ui::set_review_segs` for the verb doors that rewrite the segment list.

use adw::prelude::*;

use crate::cut;

/// F3.12 S1–S5: hold this window's effects to the cut as it will actually play, and say what went.
///
/// The rule lives in [`crate::cut_clamp`]; this seam only reads the page, runs the pass over the newest
/// review cut (the scenes AFTER snapping, dead-air removal, mark removal and coalescing — which is the whole
/// reason the pass exists), and writes back what survives. It is reached from the Cut page through
/// [`crate::ui::cut_verb_press::report_verb`], so every verb that rewrites the segment list clamps the
/// effect list against it in the same step; `suggest::apply` runs the same function on the model's own
/// answer.
///
/// A no-op writes NOTHING at all — not even a status-free history entry: `record_edit` feeds
/// `History::note_change`, but a clamp that changed nothing must not consume an Undo step or repaint the lane.
pub fn clamp_effects_to_cut(window: &adw::ApplicationWindow) -> String {
    let before = super::window_preview::newest_review_cut();
    let clamped = crate::cut_clamp::clamp_to_cut(&before);
    // Nothing dropped and nothing trimmed: the effects already sit inside the footage the cut keeps.
    if clamped.dropped == 0 && clamped.kept == before.fx {
        return "nothing to clamp \u{2014} every effect sits inside the footage the cut keeps".to_string();
    }
    let mut cut_ = before.clone();
    cut_.fx = clamped.kept.clone();
    // Through `record_edit`, NEVER `publish_cut` alone: one ↶ takes the clamp back exactly as it takes any
    // other edit back (§F3.12's "same Undo step as the segments").
    super::window_history_zoom::record_edit(window, &cut_);
    super::cut_lane_rows::refresh_effects_lane(window);
    let said = if clamped.dropped > 0 {
        // S5's line goes to the log, where the count is read with the rest of the run; the status line says
        // the same thing shorter. §12 entry 19: the count alone was not something the user could act on, so
        // `dropped_log` names which effects went after it (capped, so a caption run cannot bury the log).
        if let Some(line) = crate::cut_clamp_line::dropped_log(&clamped) {
            super::window_logs::log_line(&line);
        }
        // §12 entry 19's half: the count says how many; this row says WHICH, in the words the effect
        // dropdown uses, so a person can find them on the lane and move them inside the cut.
        if let Some(names) = crate::cut_clamp_line::dropped_names(&clamped) {
            super::window_logs::log_line(&names);
        }
        // §9 (`A stop at a dropped second SHOULD be refused when placed`): the count says how
        // many went; a stop among them also owes its own reason, because "dropped" is not what
        // happened to it — there was no frame to hold, so it could not be placed at all.
        for unholdable in crate::cut_clamp::refused_stops(&clamped) {
            super::window_logs::log_line(&crate::effect_overlap::no_frame(unholdable));
        }
        format!(
            "effects clamped to the cut \u{2014} {} dropped",
            clamped.dropped
        )
    } else {
        format!(
            "effects trimmed to the cut \u{2014} {} kept",
            cut_.fx.len()
        )
    };
    if let Some(status_line) = super::window_doors::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    said
}

/// Write this window's cut's segment list, keeping everything else about the cut (lanes, shifts, rows,
/// folds) exactly as it was — the write side of [`newest_review_cut`](super::window_preview::newest_review_cut),
/// used by the verb doors.
pub(crate) fn set_review_segs(window: &adw::ApplicationWindow, segs: &[cut::Seg]) {
    let _ = window;
    super::window::REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            slot.borrow_mut().segs = segs.to_vec();
        }
    });
}
