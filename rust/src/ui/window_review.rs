//! F3.12's review-cut doors, reached through their old paths: `ui::clamp_effects_to_cut` for the verb report
//! and the tests, `ui::set_review_segs` for the verb doors that rewrite the segment list.

use adw::prelude::*;

use crate::cut;

/// F3.12 S1–S5: hold this window's effects to the cut as it will actually play, and say what went.
///
/// The rule lives in [`crate::cut_clamp`]; this runs it over the newest review cut (the scenes AFTER snapping,
/// dead-air removal, mark removal and coalescing) and writes back what survives. Reached through
/// [`crate::ui::cut_verb_press::report_verb`], so every verb that rewrites the segments clamps in the same
/// step; `suggest::apply` runs it on the model's answer.
///
/// A no-op writes NOTHING, not even a history entry: it must not consume an Undo step or repaint the lane.
pub fn clamp_effects_to_cut(window: &adw::ApplicationWindow) -> String {
    let before = super::window_preview::newest_review_cut();
    let clamped = crate::cut_clamp::clamp_to_cut(&before);
    // Nothing dropped or trimmed: the effects already sit inside the footage the cut keeps.
    if clamped.dropped == 0 && clamped.kept == before.fx {
        return "nothing to clamp \u{2014} every effect sits inside the footage the cut keeps"
            .to_string();
    }
    let mut cut_ = before.clone();
    cut_.fx = clamped.kept.clone();
    // Amended onto the step the segments just took (§F3.12: "same Undo step as the segments").
    super::window_history_zoom::amend_edit(window, &cut_);
    super::cut_lane_rows::refresh_effects_lane(window);
    let said = if clamped.dropped > 0 {
        // S5's line goes to the log; `dropped_log` names which effects went (capped so a caption run cannot bury the log, §12 entry 19).
        if let Some(line) = crate::cut_clamp_line::dropped_log(&clamped) {
            super::window_logs::log_line(&line);
        }
        // §12 entry 19: the count says how many; this row says WHICH, in the effect dropdown's words.
        if let Some(names) = crate::cut_clamp_line::dropped_names(&clamped) {
            super::window_logs::log_line(&names);
        }
        // §9 (`A stop at a dropped second SHOULD be refused when placed`): a stop among the dropped owes its own
        // reason, as there was no frame to hold.
        for unholdable in crate::cut_clamp::refused_stops(&clamped) {
            super::window_logs::log_line(&crate::effect_overlap::no_frame(unholdable));
        }
        format!(
            "effects clamped to the cut \u{2014} {} dropped",
            clamped.dropped
        )
    } else {
        format!("effects trimmed to the cut \u{2014} {} kept", cut_.fx.len())
    };
    if let Some(status_line) = super::window_doors::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    said
}

/// Write this window's cut's segment list, keeping lanes, shifts, rows and folds as they were; the write side
/// of [`newest_review_cut`](super::window_preview::newest_review_cut), used by the verb doors.
pub(crate) fn set_review_segs(window: &adw::ApplicationWindow, segs: &[cut::Seg]) {
    let _ = window;
    super::window::REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            slot.borrow_mut().segs = segs.to_vec();
        }
    });
}
