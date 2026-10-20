//! F3.4 S5 (`a press without travel toggles play/pause wherever it lands`) — the door the preview panel's one
//! button-1 gesture calls, and nothing else.
//!
//! §06 §B states the rule for the whole preview ("an unmoved press toggles play/pause") and F3.4 S5 repeats it for
//! the case this round draws a caption into: the box must not swallow a press meant for the picture under it
//! ([`crate::fx_text::press_without_travel_toggles_play`]'s own reason). So a click that lands inside an overlay's
//! box is answered here rather than falling through to the zoom flow's after-S5 answer, which would otherwise be the
//! only thing listening (§F3.1's paragraph after S5 owns a press clear of every box, and keeps it: this module only
//! claims presses that fall on a text or svg box, so `tests/suite/zoom_drag_widgets.rs` and
//! `tests/suite/f3_1_press_on_the_preview.rs` see exactly what they saw before).
//!
//! No rule is composed here. Which seconds an overlay covers and whether it is drawn come from [`crate::fx_lane`],
//! where its box is comes from [`crate::ui::cut_preview_caption`], and playing or pausing goes through the same door
//! the picture's own ▶ uses ([`window::press_play_recording`] with the page's filmed runs), so a click on a caption
//! and a click on the picture cannot end up in two different transports.

use crate::cut::{Cut, EffectKind};
use crate::fx_text;
use crate::fx_zoom;
use crate::timeline;
use crate::ui::{cut_preview_caption, cut_svg, cut_text_hand, window};

/// The overlay whose box covers `at_px`, as its index into `cut.fx`. Text and svg only: those are the kinds §06 gives
/// a box ON the output frame, and only ones the frame actually paints at this second can be pressed on — the filter
/// is the same `fx_lane::drawn_paused` the painter asks, so the box a press can hit and the box a refresh draws
/// cannot drift apart.
pub fn overlay_under_pointer(
    cut_: &Cut,
    at: f64,
    fit: (f64, f64),
    at_px: (f64, f64),
) -> Option<usize> {
    cut_.fx.iter().enumerate().find(|(_, effect)| {
        let Some(kind) = effect.effect_kind() else { return false };
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
        // The box GROWN by `EDGE_GRIP_PX` on every side: §F3.4 S5 lets a press near an edge resize the box, so the
        // record has to answer even when the finger is a few px outside what was drawn — otherwise a press just
        // off the caption's edge finds nothing to grab and falls through to play/pause.
        let grip = fx_text::EDGE_GRIP_PX;
        at_px.0 >= x - grip && at_px.0 <= x + w + grip && at_px.1 >= y - grip && at_px.1 <= y + h + grip
    }).map(|(index, _)| index)
}

/// Which TEXT record a press at `from` has grabbed, as its index into `cut.fx` and its box in the fitted px.
/// §F3.4 S5's own hit-test: only a caption counts (an svg box is F3.5's to move), and only one that draws at
/// this second — the same `fx_lane::drawn_paused` filter [`overlay_under_pointer`] uses, so a box cannot be
/// grabbed where it is not painted.
fn text_box_under(cut_: &Cut, at: f64, fit: (f64, f64), at_px: (f64, f64)) -> Option<(usize, (f64, f64, f64, f64))> {
    let index = overlay_under_pointer(cut_, at, fit, at_px)?;
    let effect = cut_.fx.get(index)?;
    (effect.kind == "text").then(|| (index, box_of(effect, EffectKind::Text, fit)))
}
/// §F3.4 S5 (`On the preview a box moves … and resizes`): the release that edits a PLACED caption. Returns `None`
/// for every press that is not this one's — anything armed (the arm owns the press), a held effect (F3.8's grip owns
/// it), or travel under `fx_zoom::DRAG_MIN_PX` on either axis, which is the click [`play_toggle_on_press`] already
/// answers. Otherwise the record whose box the press landed on is moved (interior) or resized (within
/// `fx_text::EDGE_GRIP_PX` of an edge) through `ui::nudge_text_box` / `ui::drag_text_edge`, which write the history
/// step, so one ↶ takes this gesture back.
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
    // The travel floor is `fx_zoom::DRAG_MIN_PX` — §F3.1's "below this a press is a click, not a box" — asked on
    // EACH axis, and deliberately the SAME door that flow uses: a caption edit travels on one axis only (drag an
    // edge, or slide a caption straight up), so requiring both axes to move would refuse the very gesture §S5 asks
    // for. A release under it on either axis was a press, and a press with nothing armed or held is §S5's play/pause
    // toggle (`play_toggle_on_press`, reached earlier in the same release) — one gesture cannot mean both.
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if !fx_zoom::travel_one_axis(dx.abs(), dy.abs()) {
        return None;
    }
    let cut_ = window::review_cut_of(win);
    let (index, box_) = text_box_under(&cut_, at, fit, from)?;
    match fx_text::grab_edge(box_, from) {
        Some(edge) => Some(match edge {
            // A vertical edge travels on x, a horizontal one on y: the axes are independent (§S5), so the other
            // coordinate of the release is not this edge's business.
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

/// Where the same gesture would land the box, without writing anything: what the drag-update shows while the hand
/// moves, so the person sees the snapped result (and the snap lines, see [`crate::ui::cut_preview_frame::note_edit`])
/// before releasing. Same three refusals as [`box_edit_on_release`]; answers the candidate box in the fitted px.
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
    // Same floor as [`box_edit_on_release`], so the box shown while the hand moves is the box the release performs.
    if !fx_zoom::travel_one_axis(dx.abs(), dy.abs()) {
        return None;
    }
    let cut_ = window::review_cut_of(win);
    let (_, box_) = text_box_under(&cut_, at, fit, from)?;
    // The candidate is measured against the FITTED picture, which is what every overlay on this panel is
    // (`set_measure_overlay` in `cut_preview_panel`) — not the panel's own larger allocation.
    let box_ = fx_text::Box_::from_px(box_.0, box_.1, box_.2, box_.3, fit.0, fit.1);
    let moved = match fx_text::grab_edge(box_.to_px(fit.0, fit.1), from) {
        Some(edge) => fx_text::resize_box(
            box_,
            edge,
            if matches!(edge, fx_text::Edge::Left | fx_text::Edge::Right) { to.0 } else { to.1 },
            fit,
        ),
        None => fx_text::move_box(box_, (dx, dy), fit),
    };
    Some(moved.to_px(fit.0, fit.1))
}

/// Test-only window onto the thread-local behind the §S5 snap lines. The PROGRAM never asks this question: the
/// painter reads [`crate::ui::cut_preview_frame::edit_snap_on`] directly at paint time (`paint_paused`), and the
/// drag's update leg writes it through `note_edit`. This door exists so a check can assert the switch follows
/// `note_edit` without reaching into the thread-local itself — hence the name, so a reader of either side knows which
/// path they are on.
pub fn snap_lines_are_on_for_test() -> bool {
    crate::ui::cut_preview_frame::edit_snap_on()
}

/// An overlay's box in the fitted picture's px. A caption's box is `cut_preview_caption`'s, because that is the box
/// its words were fitted into; a drawing's is the same four fractions read off the same output frame (§06#1).
fn box_of(effect: &crate::cut::Fx, kind: EffectKind, fit: (f64, f64)) -> (f64, f64, f64, f64) {
    if kind == EffectKind::Text {
        return cut_preview_caption::box_of(effect, fit);
    }
    let (cx, cy) = effect.centre();
    let default = fx_text::LOWER_THIRD;
    fx_text::Box_ { cx, cy, wf: effect.wf.unwrap_or(default.wf), hf: effect.hf.unwrap_or(default.hf) }
        .to_px(fit.0, fit.1)
}

/// §S5's toggle, performed: the same door the picture's own ▶ uses, so a click on a caption and a click on the
/// picture move ONE transport. Returns the sentence for the status row — which reads differently when the click
/// started play and when it paused, because "toggles" is the rule and a person needs to know which way it went.
///
/// `at` is the second the red line stands on (which overlay boxes are live is asked at that second), `fit` the
/// fitted picture the painter measures against, `at_px` the press in those same px. `Some` only when this press is
/// F3.4 S5's: nothing armed (`zoom_armed`/`text_armed`/`svg_armed` all down), nothing held, and the point inside an
/// overlay's box. Everything else returns `None` so the caller falls through to the framing flow unchanged.
pub fn play_toggle_on_press(
    win: &adw::ApplicationWindow,
    at: f64,
    fit: (f64, f64),
    at_px: (f64, f64),
) -> Option<String> {
    if window::zoom_armed() || cut_text_hand::text_armed() || cut_svg::svg_armed() {
        return None;
    }
    if window::held_effect().is_some() {
        return None;
    }
    let cut_ = window::review_cut_of(win);
    overlay_under_pointer(&cut_, at, fit, at_px)?;
    // The picture's own button's door, with the page's filmed runs: nothing here decides how playback starts.
    let runs = timeline::filmed_runs(&crate::ui::page_recordings())
        .iter()
        .map(|(start, end)| (*start, *end))
        .collect::<Vec<_>>();
    let said = window::press_play_recording(win, &runs);
    // Which way it went comes from the transport AFTER the press, not from a guess before it. The face is the
    // playhead's own reading (§D), which is what every other status on this page quotes.
    let playing = window::preview_player(win).transport.playing;
    let shown = crate::preview::clock(window::preview_player(win).playhead);
    let _ = said;
    Some(if playing {
        format!("{shown} \u{b7} the red line, session time \u{2014} a click on the picture plays it")
    } else {
        format!("paused at {shown} \u{2014} a click on the picture again plays it")
    })
}
