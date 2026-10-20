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
use crate::ui::window::{self, review_cut_of, selection, set_held_effect};

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
        ui::press_row(win),
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

// The F3.1 zoom-by-hand arm group, moved out of `window.rs`, which is past its size budget. The rule half
// lives in `src/fx_zoom.rs` (`arm`, `NO_LINE`, `ARM_WORDS`/`arm_tail`, `whole_source_shown`, `disarm`); what
// came with it is only the page's own state and wording, so the callback holds no decision of its own.
//
// `press_zoom_item` stays in `window.rs`: it belongs to the effect-menu group that remained there.

use crate::fx_lane;
use crate::ui::cut_preview_press;
use crate::ui::window as ui;

thread_local! {
    /// Whether this session has ever been told where its red line stands. Shaped exactly like F2.12's
    /// `ui::INSERT_PLACE_KNOWN`: `None` means no line was EVER placed, which is the state that makes "click a track
    /// first — the effect needs a moment to happen at" true rather than arming a zoom silently at t = 0.
    static ZOOM_LINE_KNOWN: std::cell::RefCell<Option<bool>> = const { std::cell::RefCell::new(None) };

    /// Whether ⊕ Zoom is currently armed — i.e. waiting for a box on the preview. Read by the drag area's tooltip
    /// and by `zoom_drag_ended`, and set false again when the box lands or the entry is pressed a second time.
    static ZOOM_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// The form a finished drag left waiting on the page, before Apply. `None` is no form open — which is
    /// also what hides `zoom-form`, so one slot answers both "is there a box to settle" and "what does the
    /// form show". Held by value (a copy), because the cut keeps moving under it while someone types.
    static ZOOM_FORM: std::cell::RefCell<Option<fx_zoom::Form>> =
        const { std::cell::RefCell::new(None) };
}

/// Tell the page whether a line exists to zoom at. Mirrors [`note_place`]: placing a line makes a zoom possible,
/// taking it away (a cleared page) makes the next ⊕ Zoom refuse again.
pub fn note_zoom_place(placed: bool) {
    ZOOM_LINE_KNOWN.with(|cell| *cell.borrow_mut() = Some(placed));
}

/// Is a zoom waiting for its box right now?
/// F3.1 S1: what pressing the dropdown's ⊕ Zoom did, as the sentence for the status line.
///
/// The line is read through the same seam Paste and Insert use (`paste_line`) and gated by whether one was ever
/// placed (`ZOOM_LINE_KNOWN`), so a session that has never placed a line refuses instead of arming at midnight.
/// The three answers are `fx_zoom::arm`'s; nothing is decided here. Arming records NOTHING on the cut — the box
/// someone draws is the effect, not this click — and the camera going down follows from
/// `fx_zoom::whole_source_shown(true)`, asserted by tests rather than painted here.
pub fn press_zoom_item(window: &adw::ApplicationWindow) -> String {
    // The line is known either because this session was explicitly told (`note_zoom_place`) or because the F2.4/F2.12
    // seam already recorded one (`INSERT_PLACE_KNOWN` via `note_place` / `set_line_position`) -- the page has one
    // notion of "a line exists", not two, so arming a zoom must agree with what Insert already believes. With
    // neither, S1's refusal is the answer rather than arming at t = 0.
    let known = ZOOM_LINE_KNOWN.with(|cell| *cell.borrow()) == Some(true)
        || crate::ui::window::INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true);
    let line = if known {
        Some(ui::paste_line(window))
    } else {
        None
    };
    match fx_zoom::arm(line, zoom_armed()) {
        fx_zoom::Press::Refused(reason) => {
            set_zoom_armed(false);
            reason.to_string()
        }
        fx_zoom::Press::Armed => {
            set_zoom_armed(true);
            // §S1: the camera layer goes down so the whole source shows while the box is drawn. The class is
            // Added here and taken off again by the Disarmed branch. `set_camera_layer_down` touches the two
            // widgets it owns and nothing else -- no lane rebuild for a change no lane widget reads.
            set_camera_layer_down(window, true);
            // No marked stretch reaches the Cut page yet (no live marking seam exists in this file), so the tail
            // names the default length. `fx_zoom::arm_tail(Some((a, b)))` is proven logic-side only.
            format!("{}{}", fx_zoom::ARM_WORDS, fx_zoom::arm_tail(None))
        }
        fx_zoom::Press::Disarmed => {
            set_zoom_armed(false);
            // And the camera comes back up with the arm taken off.
            set_camera_layer_down(window, false);
            // §A.1 spells the refusal and the arm words but gives no exact disarm sentence, so this one is ours,
            // in the shape this tree's other cancellations use: what stopped, and what happens now.
            "zoom disarmed \u{2014} the picture stops waiting for a box".to_string()
        }
    }
}

pub fn zoom_armed() -> bool {
    ZOOM_ARMED.with(|cell| cell.get())
}

/// Set the arm from outside — the drag landing takes it down, and a test sets it to prove the disarm branch.
pub fn set_zoom_armed(armed: bool) {
    ZOOM_ARMED.with(|cell| cell.set(armed));
}

/// F3.1 S2 → S4: the form a finished drag left on the page, if any. What a widget test reads to assert the
/// same state the logic test asserts ([`fx_zoom::Form`]) rather than a painted field.
pub fn zoom_form_open() -> Option<fx_zoom::Form> {
    ZOOM_FORM.with(|cell| cell.borrow().clone())
}

/// Store or drop the pending form. Dropping is what Apply and Cancel do; the widget's visibility follows in
/// `ui::show_zoom_form`, so nothing can be open with no form behind it.
pub fn set_zoom_form(form: Option<fx_zoom::Form>) {
    ZOOM_FORM.with(|cell| *cell.borrow_mut() = form);
}


/// F3.1 S2: the panel's own picture size, in px. The panel pins itself to 240 x 135 (240 wide as drawn,
/// 9/16 of that tall — see `page_box`), and this pair is what BOTH that `set_size_request` and the drag's
/// fallback read, so a change to the frame cannot leave the two halves disagreeing about what a box covers.
pub const PREVIEW_PANEL_W: f64 = 240.0;
pub const PREVIEW_PANEL_H: f64 = 135.0;

/// This window's preview panel, resolved through its own content so a closed window's surviving tree cannot
/// answer for a live one — the same scoping `effects_lane_box` uses.
pub fn preview_panel(window: &adw::ApplicationWindow) -> Option<gtk::Widget> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, "preview-panel")
}

/// F3.1 S2: the box came off the preview, now what? The seam the gesture AND the widget test drive, shaped
/// exactly like F2.12's `insert_chosen_with_length`: the source's own pixel size is handed in because no
/// ffprobe runs headless, and nothing here composes a rule — every answer comes from `fx_zoom`.
///
/// `marked` is `None` on purpose: no live marking seam reaches the Cut page yet, which is the same reason
/// `press_zoom_item` prints `arm_tail(None)`. A marked stretch outranking the line is proven logic-side in
/// `tests/suite/cut_zoom_by_hand.rs::f3_1_s3_defaults`.
pub fn zoom_drag_ended_with_source(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
    source: (f64, f64),
) -> String {
    // §F3.1's paragraph after S5: with nothing armed or held, a release on the preview is a PRESS about framing,
    // not a box nobody asked for — a press inside a zoom's box takes that zoom, a press clear of every box draws
    // a new rectangle for the zoom in force. A release that covered real pixels is still a hand-drawn box and
    // stays refused (only ⊕ Zoom frames by hand); one under the floor was a click, which is what the rule reads.
    if !zoom_armed() {
        let width = (to.0 - from.0).abs();
        let height = (to.1 - from.1).abs();
        if fx_zoom::drag_counts(width, height) {
            return "no zoom armed \u{2014} pick \u{2295} Zoom first".to_string();
        }
        if ui::held_effect().is_none() {
            // F3.4 S5 (`a press without travel toggles play/pause wherever it lands`): a click that landed inside an
            // overlay's own box is the picture's toggle, not a framing question — the box must not swallow it.
            // Claimed here because this is where the panel's ONE button-1 release arrives; `cut_preview_press`
            // performs it through the picture's own door and answers `None` for every other click, so §F3.1's
            // after-S5 rule below is untouched.
            let at = ui::paste_line(window);
            let fit = cut_preview_frame::fit_size_of(
                window,
                PREVIEW_PANEL_W,
                PREVIEW_PANEL_H,
            );
            if let Some(said) = cut_preview_press::play_toggle_on_press(window, at, fit, from) {
                return said;
            }
            let said = super::cut_zoom_hand::press_on_release(window, from);
            if !said.is_empty() {
                return said;
            }
        }
        return "no zoom armed \u{2014} pick \u{2295} Zoom first".to_string();
    }
    let width = (to.0 - from.0).abs();
    let height = (to.1 - from.1).abs();
    if !fx_zoom::drag_counts(width, height) {
        // Under the floor this was a click at the preview, not a framing. The arm goes with it: §S1 makes
        // the second press a disarm, and leaving it armed would draw a box on the next stray click.
        set_zoom_armed(false);
        set_camera_layer_down(window, false);
        return format!(
            "too small to zoom \u{2014} a box has to be at least {:.0} px",
            fx_zoom::DRAG_MIN_PX
        );
    }
    let at = ui::paste_line(window);
    let cut_ = review_cut_of(window);
    let box_ = match fx_zoom::free_rectangle(from, to) {
        Some(box_) => box_,
        // `drag_counts` already answered above; this is the type's own guard, not a second rule.
        None => return "too small to zoom \u{2014} a box has to be at least {:.0} px".to_string(),
    };
    let form = fx_zoom::form_from_drag(
        box_,
        source.0,
        source.1,
        at,
        None,
        &cut_.fx,
        fx_zoom::aspect_is_set(&cut_),
        // §1's `cam`: the row this zoom was framed on is the row the page is watching, not a literal 0.
        ui::press_row(window),
    );
    set_zoom_armed(false);
    // The arm is off, so the camera comes back up. The record is not on the cut yet -- its bar arrives with
    // `press_zoom_apply`'s own refresh once someone applies the form.
    set_camera_layer_down(window, false);
    // S4: the form opens in the same turn the box landed, so nobody has to ask for it. Ownership of the form
    // is claimed INSIDE `ui::show_zoom_form`, before it resolves the holder -- storing the form first would let a
    // later window's refusal-looking lookup find this one's half-drawn form under the wrong tree.
    show_zoom_form(window, &form);
    format!("{} \u{2014} the form says how it ends", fx_zoom::label(at))
}

/// The holder without the ownership gate -- used ONLY to hide-and-empty before a redraw, so a leftover form in
/// whichever box the walk reaches cannot collide with the one about to be drawn.
fn zoom_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, &cut_zoom_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// The Cut page's zoom-form holder name. Only the Cut tab's instance is ever drawn into or read from.
fn cut_zoom_form_name() -> String {
    format!("zoom-form-{}", crate::shell::Page::Cut.label())
}

/// This window's zoom-form holder, resolved through its own content so a closed window's surviving tree
/// cannot answer for a live one — the scoping `insert_form_box` and `effects_lane_box` use.
pub fn zoom_form_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    // Only the window that owns the live form gets its holder back. `page_box` builds a `zoom-form` on every
    // tab of every window, and a closed window's tree survives with its widgets parented -- so resolving by
    // name alone hands back whichever box a walk reaches first, and appending into it lands in another window's
    // page (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`).
    if !ui::zoom_form_owner(window) {
        return None;
    }
    let content = window.content()?;
    ui::find_widget_by_name(&content, &cut_zoom_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// F3.1 S4: draw `Zoom at m:ss`. Every word on it comes from `fx_zoom` — the field names, the help
/// sentences, the two endings, the footer — so the form cannot drift from what the rules describe.
///
/// Children are cleared FROM THE HOLDER, never by a tree-wide name search: a closed GTK window keeps its
/// widgets parented, and searching globally can wipe another window's form (the fold-badge lesson).
pub fn show_zoom_form(window: &adw::ApplicationWindow, form: &fx_zoom::Form) {
    // Hidden FIRST, before anything is touched: while the holder is invisible its children cannot be seen as
    // "already parented here" by a later pass, and the clear below then removes exactly what this holder owns.
    if let Some(holder) = zoom_form_box_raw(window) {
        holder.set_visible(false);
        let stale: Vec<gtk::Widget> = holder
            .observe_children()
            .iter::<glib::Object>()
            .flatten()
            .filter_map(|child| child.downcast::<gtk::Widget>().ok())
            .collect();
        for old in stale {
            holder.remove(&old);
        }
    }
    // Everything is built into locals first and appended LAST, in one pass. A widget that already has a parent
    // cannot be appended again -- GTK asserts it -- so nothing here may travel between calls or be attached
    // early and moved. The holder itself is resolved through THIS window's content (the `fold-badges` lesson:
    // `page_box` runs once per tab, and a closed window keeps its widgets parented somewhere).
    LAST_ZOOM_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    let Some(holder) = zoom_form_box(window) else {
        return;
    };
    // Drop whatever THIS holder was showing. Only children of THIS holder are touched -- never a name search,
    // which is how one form got cleared out from under another window.
    let stale: Vec<gtk::Widget> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .collect();
    for old in stale {
        holder.remove(&old);
    }
    // Nothing may be appended twice, so every widget below is created here and parented exactly once.

    let heading = gtk::Label::new(Some(&fx_zoom::form_title(form.at)));
    heading.set_widget_name("zoom-heading");
    heading.set_xalign(0.0);

    let length = zoom_field_row(
        fx_zoom::FORM_FIELDS[0],
        fx_zoom::LENGTH_HELP,
        "zoom-field-length",
        &trim_seconds(form.dur),
    );

    // At the end: Pull back / Stay on it -- one grouped answer (`end_choices` puts the chosen one first, as
    // §A.1 shows it), each carrying the sentence that says why someone would pick it.
    let end_row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let [first_label, second_label] = fx_zoom::end_choices(form.stay);
    let (first_name, second_name) = if form.stay {
        ("zoom-end-stay-on-it", "zoom-end-pull-back")
    } else {
        ("zoom-end-pull-back", "zoom-end-stay-on-it")
    };
    let first_button = gtk::CheckButton::with_label(first_label);
    first_button.set_widget_name(first_name);
    first_button.set_tooltip_text(Some(if form.stay {
        fx_zoom::STAY_ON_IT_HELP
    } else {
        fx_zoom::PULL_BACK_HELP
    }));
    let second_button = gtk::CheckButton::with_label(second_label);
    second_button.set_widget_name(second_name);
    second_button.set_tooltip_text(Some(if form.stay {
        fx_zoom::PULL_BACK_HELP
    } else {
        fx_zoom::STAY_ON_IT_HELP
    }));
    first_button.set_group(Some(&second_button));
    first_button.set_active(true);

    let fade_in = zoom_field_row(
        fx_zoom::FORM_FIELDS[2],
        fx_zoom::FADE_IN_HELP,
        "zoom-field-fade-in",
        &trim_seconds(form.trans),
    );
    // Fade out: greyed when staying, with the reason swapped into its tooltip -- §A.1 pairs the greying with a
    // sentence, so a field is never grey without something to say about it.
    let fade_out = zoom_field_row(
        fx_zoom::FORM_FIELDS[3],
        fx_zoom::FADE_OUT_HELP,
        "zoom-field-fade-out",
        &trim_seconds(form.tout),
    );
    if let Some(entry) = find_named(fade_out.upcast_ref(), "zoom-field-fade-out").and_then(|w| w.downcast::<gtk::Entry>().ok()) {
        let greyed = fx_zoom::fade_out_greyed(form.stay);
        entry.set_sensitive(!greyed);
        if greyed {
            entry.set_tooltip_text(Some(fx_zoom::NO_WAY_BACK));
        }
    }

    // Curve: one choice today, shown as a label rather than a dropdown with nothing in it.
    let curve_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let curve_key = gtk::Label::new(Some(fx_zoom::FORM_FIELDS[4]));
    curve_key.set_xalign(0.0);
    let curve_value = gtk::Label::new(Some(fx_zoom::CURVE_CHOICES[0]));
    curve_value.set_widget_name("zoom-field-curve");
    curve_value.set_tooltip_text(Some(fx_zoom::CURVE_HELP));
    curve_row.append(&curve_key);
    curve_row.append(&curve_value);

    let buttons = super::cut_form_buttons::form_buttons_row("zoom");
    let (apply, cancel) = super::cut_form_buttons::row_buttons(&buttons);
    let footer = super::cut_form_buttons::form_note("zoom");

    // Handlers go on BEFORE the widgets are parented. GTK fires `toggled` from its own `set_active(true)`
    // above, so wiring after that call but before appending means the construction-time signal finds no
    // handler and cannot write `stay` onto the form before anyone chose anything.
    let owner = window.clone();
    for (name, button) in [(first_name, &first_button), (second_name, &second_button)] {
        let wants_stay = name.ends_with("stay-on-it");
        let owner = owner.clone();
        button.connect_toggled(move |check| {
            // A refresh-driven activation is not a user choice; only turning THIS control on answers a thing,
            // and only for the window that owns the live form.
            if ZOOM_FORM_REFRESH.with(|cell| cell.get()) || !check.is_active() {
                return;
            }
            if !ui::zoom_form_owner(&owner) {
                return;
            }
            ZOOM_FORM_REFRESH.with(|cell| cell.set(true));
            store_zoom_stay(wants_stay);
            ZOOM_FORM_REFRESH.with(|cell| cell.set(false));
        });
    }
    let win_apply = window.clone();
    apply.connect_clicked(move |_| {
        let said = ui::press_zoom_apply(&win_apply);
        if let Some(status_line) = ui::find_status(win_apply.upcast_ref()) {
            status_line.set_text(&said);
        }
    });
    let win_cancel = window.clone();
    cancel.connect_clicked(move |_| {
        let said = ui::press_zoom_cancel(&win_cancel);
        if let Some(status_line) = ui::find_status(win_cancel.upcast_ref()) {
            status_line.set_text(&said);
        }
    });

    // ONE append pass, in the order `spec/img/06-zoom.png` reads them. Every widget above was created here and
    // is parented exactly once, which is what keeps GTK's "child must be unparented" assertion out of this path.
    holder.append(&heading);
    holder.append(&length);
    end_row.append(&first_button);
    end_row.append(&second_button);
    holder.append(&end_row);
    holder.append(&fade_in);
    holder.append(&fade_out);
    holder.append(&curve_row);
    holder.append(&buttons);
    holder.append(&footer);
    holder.set_visible(true);
    // Stored only now that the widgets exist and belong to THIS window: `zoom_form_open()` then always means
    // "there is a form on screen behind it", never "a value was parked somewhere".
    set_zoom_form(Some(form.clone()));
}

/// One labelled numeric row: the field's name, its help as the tooltip, and an entry named for the field.
/// Built detached; the caller appends it exactly once.
pub fn zoom_field_row(label: &str, help: &str, name: &str, value: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let key = gtk::Label::new(Some(label));
    key.set_xalign(0.0);
    row.append(&key);
    let entry = gtk::Entry::new();
    entry.set_widget_name(name);
    entry.set_text(value);
    entry.set_tooltip_text(Some(help));
    entry.set_width_chars(6);
    row.append(&entry);
    row
}

/// `1.0` reads as `1`, `0.5` as `0.5`: the form shows the seconds without a trailing `.0` that would make
/// someone think the field holds fractions it does not.
pub fn trim_seconds(seconds: f64) -> String {
    if (seconds - seconds.round()).abs() < 1e-9 {
        format!("{}", seconds.round() as i64)
    } else {
        format!("{seconds}")
    }
}

/// Find a named widget inside a detached subtree we already hold, so the fade-out's greying can be set on
/// the entry before its row is ever parented.
fn find_named(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if let Some(found) = find_named(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

// S4: the two endings are ONE answer. Clicking either re-greys the fade-out through
// `fx_zoom::fade_out_greyed` and stores the new `stay` on the open form, behind a re-entrancy guard so
// the sensitivity write cannot fire the toggle handler again (the `CAMERA_REFRESH` shape).
thread_local! {
    /// Guard against the ending toggle re-entering itself through the sensitivity write below.
    static ZOOM_FORM_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Move `stay` onto the open form and re-grey the fade-out for it, through the module's own rule.
fn store_zoom_stay(stay: bool) {
    let Some(mut form) = zoom_form_open() else { return };
    form.stay = stay;
    // A camera that stays has no way back, so its fades go to nought the moment `stay` is chosen — the same
    // `fx_zoom::fades` the defaults came from, not a number typed here.
    let (trans, tout) = fx_zoom::fades(stay);
    form.trans = trans;
    form.tout = tout;
    set_zoom_form(Some(form));
    if let Some(entry) = zoom_entry("zoom-field-fade-out") {
        entry.set_sensitive(!fx_zoom::fade_out_greyed(stay));
        entry.set_tooltip_text(Some(if stay {
            fx_zoom::NO_WAY_BACK
        } else {
            fx_zoom::FADE_OUT_HELP
        }));
        entry.set_text(&trim_seconds(tout));
    }
    if let Some(entry) = zoom_entry("zoom-field-fade-in") {
        entry.set_text(&trim_seconds(trans));
    }
}

/// Read one numeric field of the open form back from its entry. An unparseable field answers `None`, which
/// Apply turns into the stored value rather than silently becoming zero.
pub fn zoom_entry(name: &str) -> Option<gtk::Entry> {
    LAST_ZOOM_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| ui::find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
    })
}

thread_local! {
    /// The window whose form was last shown, so the field readers have a tree of THIS window to look in
    /// rather than a global name search that could land on a closed window's copy.
    pub static LAST_ZOOM_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

/// F3.1 S2: the box's size in the picture's own pixels, read off the panel it was drawn on.
///
/// The panel pins itself to [`PREVIEW_PANEL_W`] x [`PREVIEW_PANEL_H`] but is only `set_halign(Start)`, so GTK
/// reports a natural size far LARGER than that (measured headless: 1815 x 1809 for a box pinned at 240 x 135)
/// and an unrealized `width()` of 0. Neither is the picture: the pin IS the answer while nothing has been laid
/// out, and once something has, the allocation is. Hence "realized allocation first, else the pin" — never
/// `natural.max(...)`, which would divide a 40 px box by ~1800 and hand back a hf at the clamp floor.
pub fn preview_picture_px(window: &adw::ApplicationWindow) -> (f64, f64) {
    // The PINNED frame, always -- never the realized allocation. GTK lays this panel out at whatever height
    // its column happens to give it, and that differs between an isolated test run and a loaded full-suite run
    // (measured: 135 px alone, ~2000 px under the gate), so reading the allocation makes a 40 px box divide
    // to hf 0.29 or land on the `HF_MIN` clamp depending on scheduler luck. The panel declares its own shape
    // through PREVIEW_PANEL_W/H; that declaration is the answer.
    let _ = window;
    (PREVIEW_PANEL_W, PREVIEW_PANEL_H)
}

/// F3.1 S2: the drag as the page sees it, sized by its own panel.
/// F3.1 S1: lower or raise the camera layer on the preview, and nothing else.
///
/// This is the whole of §S1's "the camera layer goes down so the whole source shows": the panel wears the
/// arm's class and the camera rect stops being drawn. It deliberately does NOT call `refresh_effects_lane` —
/// a bar in the lane, an overlay's alpha and the sound-debt plate are untouched by arming, and pulling the
/// whole lane rebuild in for a two-property change multiplies that path's existing re-parenting noise once per
/// arm/disarm (measured: 11 `gtk_box_append` criticals from one test file before this existed as its own call).
/// The same two writes live inside `refresh_effects_lane` too, off the same `zoom_armed()`, so a full refresh
/// still paints the arm correctly — one rule, two callers, neither dragging the other along.
pub fn set_camera_layer_down(window: &adw::ApplicationWindow, down: bool) {
    if let Some(panel) = ui::preview_widget(window, "preview-panel") {
        ui::set_class(&panel, "zoom-camera-down", down);
    }
    if let Some(rect) = ui::preview_widget(window, "preview-camera-rect") {
        // Up means back to the paused frame's stroke; down means not drawn at all. Whether there IS a stroke
        // comes from `fx_lane`, so this cannot invent a number the render disagrees with.
        rect.set_opacity(if down || fx_lane::RECT_STROKE_PX <= 0.0 {
            0.0
        } else {
            1.0
        });
    }
}

pub fn zoom_drag_ended(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
) -> String {
    let source = preview_picture_px(window);
    zoom_drag_ended_with_source(window, from, to, source)
}
