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
        at_px.0 >= x && at_px.0 <= x + w && at_px.1 >= y && at_px.1 <= y + h
    }).map(|(index, _)| index)
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
