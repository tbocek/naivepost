//! F3.4 S5 (`a press without travel toggles play/pause wherever it lands`) — the door the preview panel's one
//! button-1 gesture calls, and nothing else.
//!
//! A click inside an overlay's box is answered here rather than falling through to the zoom flow's after-S5
//! answer (the box must not swallow a press meant for the picture, [`crate::fx_text::press_without_travel_toggles_play`]).
//! Only presses on a text or svg box are claimed, so `tests/suite/zoom_drag_widgets.rs` and
//! `tests/suite/f3_1_press_on_the_preview.rs` see what they saw before. No rule lives here: coverage comes
//! from [`crate::fx_lane`], the box from [`crate::ui::cut_preview_caption`], playback from [`window::press_play_recording`].

use crate::cut::{Cut, EffectKind};
use crate::fx_text;
use crate::fx_zoom;
use crate::timeline;
use crate::ui::{cut_preview_caption, cut_svg, cut_text_hand, window};

/// The overlay whose box covers `at_px`, as its index into `cut.fx`. Text and svg only (§06 gives those a box
/// ON the output frame), filtered by the painter's own `fx_lane::drawn_paused` so a hit box is a drawn box.
pub fn overlay_under_pointer(
    cut_: &Cut,
    at: f64,
    fit: (f64, f64),
    at_px: (f64, f64),
) -> Option<usize> {
    cut_.fx
        .iter()
        .enumerate()
        .find(|(_, effect)| {
            let Some(kind) = effect.effect_kind() else {
                return false;
            };
            if kind != EffectKind::Text && kind != EffectKind::Svg {
                return false;
            }
            if effect.t > at || at >= effect.t + effect.dur {
                return false;
            }
            // A record with no words draws nothing, so nothing can be pressed on it.
            if kind == EffectKind::Text && effect.text.trim().is_empty() {
                return false;
            }
            let (x, y, w, h) = box_of(effect, kind, fit);
            // The box GROWN by `EDGE_GRIP_PX` on every side: §F3.4 S5 lets a press near an edge resize, so a press a few px
            // outside the drawn edge must still find it rather than fall through to play/pause.
            let grip = fx_text::EDGE_GRIP_PX;
            at_px.0 >= x - grip
                && at_px.0 <= x + w + grip
                && at_px.1 >= y - grip
                && at_px.1 <= y + h + grip
        })
        .map(|(index, _)| index)
}

/// Which TEXT record a press at `from` has grabbed, as its index into `cut.fx` and its box in the fitted px
/// (§F3.4 S5; an svg box is F3.5's to move), filtered by `fx_lane::drawn_paused` like [`overlay_under_pointer`].
fn text_box_under(
    cut_: &Cut,
    at: f64,
    fit: (f64, f64),
    at_px: (f64, f64),
) -> Option<(usize, (f64, f64, f64, f64))> {
    let index = overlay_under_pointer(cut_, at, fit, at_px)?;
    let effect = cut_.fx.get(index)?;
    (effect.kind == "text").then(|| (index, box_of(effect, EffectKind::Text, fit)))
}
/// §F3.4 S5 (`On the preview a box moves … and resizes`): the release that edits a PLACED caption. `None` for
/// every press that is not this one's: anything armed (the arm owns the press), a held effect (F3.8's grip),
/// or travel under `fx_zoom::DRAG_MIN_PX` on either axis (the click [`play_toggle_on_click`] answers).
/// Otherwise the box is moved (interior) or resized (within `fx_text::EDGE_GRIP_PX` of an edge) through
/// `ui::nudge_text_box` / `ui::drag_text_edge`, which write the history step, so one ↶ takes it back.
pub fn box_edit_on_release(
    win: &adw::ApplicationWindow,
    at: f64,
    fit: (f64, f64),
    from: (f64, f64),
    to: (f64, f64),
) -> Option<String> {
    if window::zoom_armed() || cut_text_hand::text_armed() || cut_svg::svg_armed() {
        return None;
    }
    if window::held_effect().is_some() {
        return None;
    }
    // The travel floor `fx_zoom::DRAG_MIN_PX` (§F3.1) is asked on EACH axis, as that flow does: a caption edit
    // travels on one axis only (an edge drag, or a straight slide), so requiring both would refuse §S5's gesture.
    // Under it on either axis moves nothing; one also inside GTK's drag threshold is a click (`play_toggle_on_click`).
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if !fx_zoom::travel_one_axis(dx.abs(), dy.abs()) {
        return None;
    }
    let cut_ = window::review_cut_of(win);
    let (index, box_) = text_box_under(&cut_, at, fit, from)?;
    match fx_text::grab_edge(box_, from) {
        Some(edge) => Some(match edge {
            // A vertical edge travels on x, a horizontal one on y; the axes are independent (§S5).
            fx_text::Edge::Left | fx_text::Edge::Right => {
                cut_text_hand::drag_text_edge(win, Some(index), edge, to.0)
            }
            fx_text::Edge::Top | fx_text::Edge::Bottom => {
                cut_text_hand::drag_text_edge(win, Some(index), edge, to.1)
            }
        }),
        None => Some(cut_text_hand::nudge_text_box(win, Some(index), (dx, dy))),
    }
}

/// Where the same gesture would land the box, without writing anything: what the drag-update shows (snapped,
/// with snap lines, see [`crate::ui::cut_preview_frame::note_edit`]). Same three refusals as [`box_edit_on_release`].
pub fn box_edit_preview(
    win: &adw::ApplicationWindow,
    at: f64,
    fit: (f64, f64),
    from: (f64, f64),
    to: (f64, f64),
) -> Option<(f64, f64, f64, f64)> {
    if window::zoom_armed() || cut_text_hand::text_armed() || cut_svg::svg_armed() {
        return None;
    }
    if window::held_effect().is_some() {
        return None;
    }
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    // Same floor as [`box_edit_on_release`], so the shown box is the box the release performs.
    if !fx_zoom::travel_one_axis(dx.abs(), dy.abs()) {
        return None;
    }
    let cut_ = window::review_cut_of(win);
    let (_, box_) = text_box_under(&cut_, at, fit, from)?;
    // Measured against the FITTED picture, which every overlay on this panel is (`set_measure_overlay` in `cut_preview_panel`).
    let box_ = fx_text::Box_::from_px(box_.0, box_.1, box_.2, box_.3, fit.0, fit.1);
    let moved = match fx_text::grab_edge(box_.to_px(fit.0, fit.1), from) {
        Some(edge) => fx_text::resize_box(
            box_,
            edge,
            if matches!(edge, fx_text::Edge::Left | fx_text::Edge::Right) {
                to.0
            } else {
                to.1
            },
            fit,
        ),
        None => fx_text::move_box(box_, (dx, dy), fit),
    };
    Some(moved.to_px(fit.0, fit.1))
}

/// Test-only window onto the thread-local behind the §S5 snap lines: the painter reads
/// [`crate::ui::cut_preview_frame::edit_snap_on`] directly and the drag's update writes it through `note_edit`.
pub fn snap_lines_are_on_for_test() -> bool {
    crate::ui::cut_preview_frame::edit_snap_on()
}

/// An overlay's box in the fitted picture's px: a caption's is `cut_preview_caption`'s (where its words were
/// fitted); a drawing's is the same four fractions off the output frame (§06#1).
fn box_of(effect: &crate::cut::Fx, kind: EffectKind, fit: (f64, f64)) -> (f64, f64, f64, f64) {
    if kind == EffectKind::Text {
        return cut_preview_caption::box_of(effect, fit);
    }
    let (cx, cy) = effect.centre();
    let default = fx_text::LOWER_THIRD;
    fx_text::Box_ {
        cx,
        cy,
        wf: effect.wf.unwrap_or(default.wf),
        hf: effect.hf.unwrap_or(default.hf),
    }
    .to_px(fit.0, fit.1)
}

/// The click on the picture (§05-cut F2.1 S2, §06 F3.1 after S5, F3.4 S5): a press and release that never
/// passed GTK's drag threshold plays or pauses WHATEVER is placed under it, through the picture's own button's
/// door so every click moves ONE transport. Returns the status sentence, which differs for play and pause.
///
/// `None` while anything is armed or held: the arm owns the press, as does F3.8's grip.
///
/// Answering only inside an overlay's box once let a click on a placed zoom start play AND take the zoom.
pub fn play_toggle_on_click(win: &adw::ApplicationWindow) -> Option<String> {
    if window::zoom_armed() || cut_text_hand::text_armed() || cut_svg::svg_armed() {
        return None;
    }
    if window::held_effect().is_some() {
        return None;
    }
    // The picture's own button's door, with the page's filmed runs.
    let runs = timeline::filmed_runs(&crate::ui::page_recordings())
        .iter()
        .map(|(start, end)| (*start, *end))
        .collect::<Vec<_>>();
    let _said = window::press_play_recording(win, &runs);
    crate::ui::cut_picture::refresh(win);
    // Which way it went comes from the transport AFTER the press, the playhead's own reading (§D).
    let playing = window::preview_player(win).transport.playing;
    let shown = crate::preview::clock(window::preview_player(win).playhead);
    Some(if playing {
        format!(
            "{shown} \u{b7} the red line, session time \u{2014} a click on the picture plays it"
        )
    } else {
        format!("paused at {shown} \u{2014} a click on the picture again plays it")
    })
}
