//! F3.1 — the two zoom rules that had no caller, taken out of `window.rs` so the page keeps its call site and
//! this file keeps the rule.
//!
//! [`preview_press`] is the sentence after §S5: with nothing armed or held and the preview paused, a press on
//! the picture is about framing, not about drawing. A press inside a zoom's box takes that zoom (the page then
//! drags it, which is §S5's "drag its overlay"); a press clear of every box draws a new rectangle for the zoom
//! in force, whose status sentence is `fx_zoom::placed_status` — the §S5 wording that promises
//! "↶ Undo takes it back", because the form it opens writes on Apply.
//!
//! [`settle_plate`] is §2's "rect stroked white WITH A PLATE": the plate beside the box names the zoom the
//! panel is framing with, from `fx_zoom::zoom_plate`, and is drawn where the box is — which is the one part of
//! that sentence no caller existed for.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::fx_zoom::{self, Camera};
use crate::ui::cut_preview_frame;
use crate::ui::window::{
    self, press_row, preview_picture_px, review_cut_of, selection, set_held_effect, zoom_armed,
    PREVIEW_PANEL_H, PREVIEW_PANEL_W,
};

/// What a press on the paused preview did, as the page is supposed to report it.
#[derive(Debug, Clone, PartialEq)]
pub enum Pressed {
    /// The press took the zoom whose box was pressed: nothing was written, the hand holds it now, and the
    /// answer is `fx_zoom::re_frame_status` of that second (§S6's wording).
    Held { status: String },
    /// A fresh box for the zoom in force, opened as its form (§S2 after S5: "a press clear of every box draws
    /// it a new rectangle"). The form is on the page already; this is the sentence for the status row.
    Framed { status: String },
    /// Nothing to say: §S1's arm owns the press, or there was no zoom to take and no box to draw.
    Silent,
}

/// F3.1, the paragraph after S5: the press on a paused preview, answered through the page's own doors.
///
/// `box_under_pointer` is which zoom's box the press fell inside — the panel can only be asked of a frame the
/// page has painted, so the caller answers it from [`framed_boxes`]. While ⊕ Zoom is armed the gesture is the
/// arm's (§S2), and a press there is not a framing question at all.
pub fn preview_press(
    win: &adw::ApplicationWindow,
    at: f64,
    box_under_pointer: Option<usize>,
) -> Pressed {
    if zoom_armed() {
        return Pressed::Silent;
    }
    let cut_ = review_cut_of(win);
    // "a drag takes the zoom in force": the hand goes on the existing record, so §F3.8's hold, grip and nudge
    // doors work it from there. No write, so no Undo step — the status says what the hand is on.
    if let Some(index) = box_under_pointer {
        if let Some(effect) = cut_.fx.get(index) {
            let said = fx_zoom::re_frame_status(effect.t);
            // The hand is on an existing record, so this press wrote nothing: the promise the sentence makes is §S6's,
            // and the record is already in the cut's history from the step that placed it.
            set_held_effect(Some(effect.clone()));
            window::refresh_effects_lane(win);
            settle_plate(win, effect.t);
            return Pressed::Held { status: said };
        }
    }
    // Clear of every box: a new rectangle for the zoom in force. The seconds of the rectangle come from the
    // camera the panel is showing, so what the form offers is what was on the screen a moment ago rather than
    // the whole source. `fx_zoom::preview_press` is the rule-side door (nothing armed or held, paused, camera
    // settled); the panel only gets here with the first two true, and the third is what the camera answers.
    let camera = fx_zoom::camera_at(&cut_.fx, at);
    match fx_zoom::preview_press(true, true, camera == fx_zoom::SETTLED, None) {
        Some(fx_zoom::PressPreview::DrawsNew) => {}
        _ => return Pressed::Silent,
    }
    let marked = marked_band_at(win, at);
    let form = fx_zoom::form_from_drag(
        box_of(camera, preview_picture_px(win)),
        PREVIEW_PANEL_W,
        PREVIEW_PANEL_H,
        at,
        marked,
        &cut_.fx,
        fx_zoom::aspect_is_set(&cut_),
        press_row(win),
    );
    // The page's own door: it claims the form's ownership, draws the holder and shows it, in that order.
    window::show_zoom_form(win, &form);
    settle_plate(win, at);
    Pressed::Framed {
        status: fx_zoom::placed_status(at, form.stay),
    }
}

/// F3.1, the paragraph after S5: the release of a click on the preview — the page's own caller for
/// [`preview_press`]. The corner the hand came up on is where the press was, so that is the point asked about;
/// `at` is the second the red line stands on, which is what a zoom drawn there would run from.
///
/// Returns the sentence for the status row, or the empty string when this press is not a framing question at all
/// (no zoom in force and nothing to take), which is what lets the page fall back to its §S2 refusal.
pub fn press_on_release(win: &adw::ApplicationWindow, at_px: (f64, f64)) -> String {
    let cut_ = review_cut_of(win);
    let at = window::paste_line(win);
    let fit = cut_preview_frame::fit_size_of(win, PREVIEW_PANEL_W, PREVIEW_PANEL_H);
    match preview_press(win, at, box_under_pointer(&cut_, at, fit, at_px)) {
        Pressed::Silent => String::new(),
        Pressed::Held { status } | Pressed::Framed { status } => status,
    }
}

/// F3.1: the zoom's own box, in the panel's px — the rectangle whose interior counts as "on the zoom" for a
/// press. The panel pins itself to [`crate::ui::window::PREVIEW_PANEL_W`] x
/// [`crate::ui::window::PREVIEW_PANEL_H`] and the painter measures the camera against the fitted footage, so
/// the same two doors are asked here: a press cannot fall inside a box the frame does not draw.
pub fn framed_boxes(cut_: &Cut, at: f64, fit: (f64, f64)) -> Vec<(usize, (f64, f64, f64, f64))> {
    cut_
        .fx
        .iter()
        .enumerate()
        .filter(|(_, effect)| effect.effect_kind() == Some(crate::cut::EffectKind::Zoom))
        .filter(|(_, effect)| effect.t <= at && at < effect.t + effect.dur)
        .map(|(index, _)| (index, cut_preview_frame::camera_rect_px(&fx_zoom::camera_at(&cut_.fx, at), fit)))
        .collect()
}

/// F3.1: is this press inside a zoom's box? Which one, as the index into `cut.fx`, or `None` when the press
/// fell clear of every box — which is the case that draws a new rectangle instead.
pub fn box_under_pointer(cut_: &Cut, at: f64, fit: (f64, f64), at_px: (f64, f64)) -> Option<usize> {
    framed_boxes(cut_, at, fit).into_iter().find(|(_, box_)| {
        at_px.0 >= box_.0 && at_px.0 <= box_.0 + box_.2 && at_px.1 >= box_.1 && at_px.1 <= box_.1 + box_.3
    }).map(|(index, _)| index)
}

/// §F3.1's plate is the ZOOM's; the sound-debt plate is `fx_lane::plate_for`'s, and the two are kept apart
/// because they name different things and appear on different effects (§2 lists both as separate rules).
pub const PLATE_NAME: &str = "preview-zoom-plate";

/// F3.1 §2 (`rect stroked white WITH A PLATE`): the zoom's plate, beside the box the frame strokes. The
/// wording is `fx_zoom::zoom_plate`'s ("zoom at 1:21 for 3.0s (1.0s in, 1.0s out)"); the widget is built once,
/// into the panel's own overlay stack, and reused — a plate added per refresh is what trips `gtk_box_append`'s
/// parent assertion on this page.
///
/// Returns whether a plate is showing, so a check can tell "no zoom at this second" from "plate not drawn".
pub fn settle_plate(win: &adw::ApplicationWindow, at: f64) -> bool {
    let cut_ = crate::ui::review_cut_of(win);
    let Some(effect) = cut_
        .fx
        .iter()
        .find(|effect| effect.effect_kind() == Some(crate::cut::EffectKind::Zoom))
        .filter(|effect| effect.t <= at && at < effect.t + effect.dur)
    else {
        hide_plate(win);
        return false;
    };
    let (trans, tout) = if effect.stay { (0.0, 0.0) } else { (effect.trans, effect.tout) };
    let text = fx_zoom::zoom_plate(effect.t, effect.dur, trans, tout);
    let Some(stack) = overlay_stack(win) else {
        return false;
    };
    let plate = ensure_plate(&stack);
    plate.set_label(&text);
    // Beside the box: the box's own width is what the plate is measured against, clamped onto the panel so a
    // full-frame zoom cannot push the plate off the right edge.
    let fit = cut_preview_frame::fit_size_of(
        win,
        crate::ui::window::PREVIEW_PANEL_W,
        crate::ui::window::PREVIEW_PANEL_H,
    );
    let (_x, _y, w, _h) = cut_preview_frame::camera_rect_px(&fx_zoom::camera_at(&cut_.fx, at), fit);
    plate.set_margin_start(w.clamp(0.0, PREVIEW_PANEL_W - 8.0) as i32);
    plate.set_visible(true);
    true
}

/// The plate's label, as the panel shows it — `None` when it is hidden. TEST-ONLY: a `gtk::Label` inside a
/// fixed overlay stack is not something a check can find by walking blindly, and nothing in the program reads
/// it back.
pub fn plate_shown_for_test(win: &adw::ApplicationWindow) -> Option<String> {
    plate_widget(win).filter(|plate| plate.is_visible()).map(|plate| plate.label().to_string())
}

/// The overlay stack's widget name, shared with the page that builds it.
const OVERLAYS_NAME: &str = "preview-overlays";

/// This window's overlay stack — the plate's holder. Resolved through the window's own content, the way every
/// lookup on this page is resolved, so a closed window's surviving tree cannot answer for a live one.
fn overlay_stack(win: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = win.content()?;
    window::find_widget_by_name(&content, OVERLAYS_NAME)?
        .downcast::<gtk::Box>()
        .ok()
}

/// The plate this module draws: its own `preview-zoom-plate`, appended to the panel's overlay stack once. The
/// sound-debt plate `refresh_effects_lane` writes into `preview-plate` is a different rule about a different
/// thing (§2 lists both), so the two never share a widget.
fn plate_widget(win: &adw::ApplicationWindow) -> Option<gtk::Label> {
    let stack = overlay_stack(win)?;
    plate_in(&stack)
}

fn ensure_plate(stack: &gtk::Box) -> gtk::Label {
    if let Some(existing) = plate_in(stack) {
        return existing;
    }
    let plate = gtk::Label::new(None);
    plate.set_widget_name(PLATE_NAME);
    // No CSS class: neither this tree nor the panel's own plate declares one (`.preview-plate` is a widget NAME,
    // not a class — see `cut_preview_panel`), and inventing a class here would style nothing while looking styled.
    plate.set_wrap(true);
    plate.set_max_width_chars(18);
    plate.set_valign(gtk::Align::Start);
    plate.set_margin_top(4);
    stack.append(&plate);
    plate
}

fn plate_in(stack: &gtk::Box) -> Option<gtk::Label> {
    window::find_widget_by_name(stack.upcast_ref(), PLATE_NAME)
        .and_then(|w| w.downcast::<gtk::Label>().ok())
}

fn hide_plate(win: &adw::ApplicationWindow) {
    if let Some(plate) = plate_widget(win) {
        plate.set_visible(false);
    }
}

/// The marked stretch covering `at`, the same question §S3 asks for a default length: a band the hand marked
/// outranks the line, and a zoom over a marked stretch is as long as that stretch. The page holds the marking;
/// this asks it through `window::selection`, the door §F3.3's speed press asks it through.
fn marked_band_at(win: &adw::ApplicationWindow, at: f64) -> Option<(f64, f64)> {
    let band = selection(win)?;
    if band.start <= at && at < band.end {
        return Some((band.start, band.end));
    }
    None
}

/// The camera as the rectangle it was framed from — the inverse of `fx_zoom::to_fractions`, so a box drawn from
/// the camera in force round-trips to the same three numbers. Measured on the panel the press happened on.
fn box_of(camera: Camera, panel: (f64, f64)) -> fx_zoom::Rect {
    fx_zoom::Rect {
        x: (camera.cx - camera.hf / 2.0) * panel.0,
        y: (camera.cy - camera.hf / 2.0) * panel.1,
        w: camera.hf * panel.0,
        h: camera.hf * panel.1,
    }
}
