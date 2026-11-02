//! F3.1: the two zoom rules that had no caller, out of `window.rs`.
//! [`preview_press`]: with nothing armed or held and the preview paused, a DRAG on the picture is about framing; from inside a zoom's box it takes that zoom, clear of every box it draws a new rectangle (`fx_zoom::placed_status`, §S5, promising "↶ Undo takes it back"). A click is never a framing question (`cut_preview_press::play_toggle_on_click`).
//! [`settle_plate`] is §2's "rect stroked white WITH A PLATE": the plate from `fx_zoom::zoom_plate`, drawn where the box is.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::fx_text;
use crate::fx_zoom;
use crate::ui::cut_preview_frame;
use crate::ui::window::{self, record_edit, selection, set_held_effect};

#[derive(Debug, Clone, PartialEq)]
pub enum Pressed {
    /// The press took the zoom whose box was pressed: nothing written, the hand holds it, the answer is `fx_zoom::re_frame_status` of that second (§S6).
    Held {
        status: String,
    },
    /// A fresh box opened as its form (§S2 after S5); this is the status sentence.
    Framed {
        status: String,
    },
    Silent,
}

/// F3.1, after S5: the drag on a paused preview, through the page's own doors. `box_under_pointer` is which zoom's box the drag began inside (from [`framed_boxes`]); `drawn` the covered rectangle, `None` under `fx_zoom::DRAG_MIN_PX`. While ⊕ Zoom is armed the gesture is the arm's (§S2).
pub fn preview_press(
    win: &adw::ApplicationWindow,
    at: f64,
    box_under_pointer: Option<usize>,
    drawn: Option<fx_zoom::Rect>,
) -> Pressed {
    if zoom_armed() {
        return Pressed::Silent;
    }
    let cut_ = crate::ui::window_preview::newest_review_cut();
    // "a drag takes the zoom in force": the hand goes on the existing record so §F3.8's hold, grip and nudge work it. No write, no Undo step.
    if let Some(index) = box_under_pointer {
        if let Some(effect) = cut_.fx.get(index) {
            let said = fx_zoom::re_frame_status(effect.t);
            // Nothing written: the record is already in history from the step that placed it (§S6).
            set_held_effect(Some(effect.clone()));
            window::refresh_effects_lane(win);
            settle_plate(win, effect.t);
            return Pressed::Held { status: said };
        }
    }
    let paused = !window::preview_player().transport.playing;
    match fx_zoom::preview_press(true, paused, true, None) {
        Some(fx_zoom::PressPreview::DrawsNew) => {}
        _ => return Pressed::Silent,
    }
    let Some(drawn) = drawn else {
        return Pressed::Silent;
    };
    let marked = marked_band_at(at);
    let form = fx_zoom::form_from_drag(
        drawn,
        PREVIEW_PANEL_W,
        PREVIEW_PANEL_H,
        at,
        marked,
        &cut_.fx,
        fx_zoom::aspect_is_set(&cut_),
        ui::press_row(win),
    );
    // The page's own door claims the form's ownership, draws the holder and shows it, in that order.
    window::show_zoom_form(win, &form);
    settle_plate(win, at);
    Pressed::Framed {
        status: fx_zoom::placed_status(at, form.stay),
    }
}

/// F3.1, after S5: the release of a preview drag, the page's caller for [`preview_press`]; `at` is the second a zoom drawn there would run from. Returns the status sentence, or empty when this is not a framing question (playing, or under the floor), so the page falls back to its §S2 refusal.
pub fn press_on_release(win: &adw::ApplicationWindow, from: (f64, f64), to: (f64, f64)) -> String {
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let at = crate::ui::cut_lane::paste_line();
    let fit = cut_preview_frame::fit_size_of(PREVIEW_PANEL_W, PREVIEW_PANEL_H);
    let drawn = fx_zoom::free_rectangle(from, to);
    match preview_press(win, at, box_under_pointer(&cut_, at, fit, from), drawn) {
        Pressed::Silent => String::new(),
        Pressed::Held { status } | Pressed::Framed { status } => status,
    }
}

/// F3.1: the zoom's box in panel px, whose interior counts as "on the zoom"; same two doors as the painter (fitted footage, [`crate::ui::window::PREVIEW_PANEL_W`] x [`crate::ui::window::PREVIEW_PANEL_H`]).
pub fn framed_boxes(cut_: &Cut, at: f64, fit: (f64, f64)) -> Vec<(usize, (f64, f64, f64, f64))> {
    cut_.fx
        .iter()
        .enumerate()
        .filter(|(_, effect)| effect.effect_kind() == Some(crate::cut::EffectKind::Zoom))
        .filter(|(_, effect)| effect.t <= at && at < effect.t + effect.dur)
        .map(|(index, _)| {
            (
                index,
                cut_preview_frame::camera_rect_px(&fx_zoom::camera_at(&cut_.fx, at), fit),
            )
        })
        .collect()
}

/// F3.1: is this press inside a zoom's box? The index into `cut.fx`, or `None` (which draws a new rectangle).
pub fn box_under_pointer(cut_: &Cut, at: f64, fit: (f64, f64), at_px: (f64, f64)) -> Option<usize> {
    framed_boxes(cut_, at, fit)
        .into_iter()
        .find(|(_, box_)| {
            at_px.0 >= box_.0
                && at_px.0 <= box_.0 + box_.2
                && at_px.1 >= box_.1
                && at_px.1 <= box_.1 + box_.3
        })
        .map(|(index, _)| index)
}

/// F3.8 S3 (`A box or camera rect dragged on the picture ends with "<label> moved — ↶ Undo takes it back" or "<label> resized — …"`): the release of a drag that started ON a placed zoom's rectangle (captions have [`crate::ui::cut_preview_press::box_edit_on_release`]). A camera is `cx`, `cy`, `hf`, so it needs its own door.
/// `None` when anything is armed, a lane effect is held (F3.8), or travel is under [`fx_zoom::DRAG_MIN_PX`] on both axes.
/// [`fx_text::grab_edge`] (`EDGE_GRIP_PX`, 8 px) decides MOVE vs RESIZE: near a line sizes it, interior slides the frame.
/// - MOVE shifts the centre by the travel;
/// - TOP/BOTTOM sizes `hf` (a full-width slice: width IS height) and keeps the opposite edge, moving the centre by half the change;
/// - LEFT/RIGHT has nothing to size; a horizontal pull slides the centre sideways.
/// Fractions go through [`fx_zoom::clamp_rect`], like the form. The write is [`ui::record_edit`] on a copy of the cut with ONE record replaced (Apply's door): one ↶ takes it back alone.
pub fn zoom_box_edit_on_release(
    window: &adw::ApplicationWindow,
    at: f64,
    fit: (f64, f64),
    from: (f64, f64),
    to: (f64, f64),
) -> Option<String> {
    if zoom_armed() || crate::ui::cut_hand::held_effect().is_some() {
        return None;
    }
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if !fx_zoom::travel_one_axis(dx.abs(), dy.abs()) {
        return None;
    }
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let index = box_under_pointer(&cut_, at, fit, from)?;
    let record = cut_.fx.get(index)?;
    let box_px = cut_preview_frame::camera_rect_px(&fx_zoom::rect_of(record), fit);
    let camera = fx_zoom::rect_of(record);
    let edge = fx_text::grab_edge(box_px, from);
    let moved = match edge {
        Some(fx_text::Edge::Top) | Some(fx_text::Edge::Bottom) => {
            let was_y = box_px.1;
            let was_bottom = box_px.1 + box_px.3;
            let target = match edge {
                Some(fx_text::Edge::Top) => to.1,
                _ => to.1,
            };
            let kept = match edge {
                // A slice flipped past its opposite edge is clamped back to its starting frame.
                Some(fx_text::Edge::Top) => target.max(0.0).min(was_bottom),
                _ => target.max(was_y).min(fit.1),
            };
            let height = (kept
                - match edge {
                    Some(fx_text::Edge::Top) => was_bottom,
                    _ => was_y,
                })
            .abs();
            let hf = (height / fit.1).max(0.0);
            // The pinned edge stays put, so the centre is half the new height from it.
            let cy = (kept
                + match edge {
                    Some(fx_text::Edge::Top) => was_bottom,
                    _ => was_y,
                })
                / (2.0 * fit.1);
            fx_zoom::clamp_rect(hf, camera.cx, cy)
        }
        // A horizontal edge of a full-width slice has no width to take.
        Some(_) => fx_zoom::clamp_rect(camera.hf, camera.cx + dx / fit.0, camera.cy),
        None => fx_zoom::clamp_rect(camera.hf, camera.cx + dx / fit.0, camera.cy + dy / fit.1),
    };
    let (hf, cx, cy) = moved;
    let mut edited = cut_.clone();
    let mut changed = record.clone();
    changed.hf = Some(hf);
    changed.cx = Some(cx);
    changed.cy = Some(cy);
    edited.fx[index] = changed;
    record_edit(window, &edited);
    crate::ui::refresh_effects_lane(window);
    let label = crate::fx_record::label_of(crate::cut::EffectKind::Zoom);
    let said = match edge {
        Some(fx_text::Edge::Top) | Some(fx_text::Edge::Bottom) => crate::fx_band::resized(label),
        _ => crate::fx_band::moved(label),
    };
    crate::ui::window_logs::log_line(&said);
    Some(said)
}

/// §F3.1's plate is the ZOOM's; the sound-debt plate is `fx_lane::plate_for`'s (§2 lists both).
pub const PLATE_NAME: &str = "preview-zoom-plate";

/// F3.1 §2 (`rect stroked white WITH A PLATE`): the zoom's plate beside the box, worded by `fx_zoom::zoom_plate`. Built once into the panel's overlay stack and reused (a plate per refresh trips `gtk_box_append`'s parent assertion). Returns whether a plate is showing.
pub fn settle_plate(win: &adw::ApplicationWindow, at: f64) -> bool {
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let Some(effect) = cut_
        .fx
        .iter()
        .find(|effect| effect.effect_kind() == Some(crate::cut::EffectKind::Zoom))
        .filter(|effect| effect.t <= at && at < effect.t + effect.dur)
    else {
        hide_plate(win);
        return false;
    };
    let (trans, tout) = if effect.stay {
        (0.0, 0.0)
    } else {
        (effect.trans, effect.tout)
    };
    let text = fx_zoom::zoom_plate(effect.t, effect.dur, trans, tout);
    let Some(stack) = overlay_stack(win) else {
        return false;
    };
    let plate = ensure_plate(&stack);
    plate.set_label(&text);
    // Beside the box, clamped onto the panel so a full-frame zoom cannot push the plate off the right edge.
    let fit = cut_preview_frame::fit_size_of(
        crate::ui::window::PREVIEW_PANEL_W,
        crate::ui::window::PREVIEW_PANEL_H,
    );
    let (_x, _y, w, _h) = cut_preview_frame::camera_rect_px(&fx_zoom::camera_at(&cut_.fx, at), fit);
    plate.set_margin_start(w.clamp(0.0, PREVIEW_PANEL_W - 8.0) as i32);
    plate.set_visible(true);
    true
}

const OVERLAYS_NAME: &str = "preview-overlays";

/// This window's overlay stack (the plate's holder), resolved through its own content so a closed window's tree cannot answer.
fn overlay_stack(win: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = win.content()?;
    window::find_widget_by_name(&content, OVERLAYS_NAME)?
        .downcast::<gtk::Box>()
        .ok()
}

/// The plate this module draws, `preview-zoom-plate`, appended to the overlay stack once; separate from the sound-debt plate in `preview-plate` (§2).
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
    // No CSS class: `.preview-plate` is a widget NAME, not a class (see `cut_preview_panel`).
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

/// The marked stretch covering `at` (as §S3's default length): a marked band outranks the line. Asked through `window::selection`, as §F3.3's speed press does.
fn marked_band_at(at: f64) -> Option<(f64, f64)> {
    let band = selection()?;
    if band.start <= at && at < band.end {
        return Some((band.start, band.end));
    }
    None
}

// The F3.1 zoom-by-hand arm group, out of `window.rs`. Rules are in `src/fx_zoom.rs` (`arm`, `NO_LINE`, `ARM_WORDS`/`arm_tail`, `whole_source_shown`, `disarm`); this holds the page's state and wording. `press_zoom_item` stays in `window.rs`.

use crate::fx_lane;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::cut_preview_press;
use crate::ui::window as ui;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// Tell the page whether a line exists to zoom at (mirrors `note_place`).
pub fn note_zoom_place(placed: bool) {
    CUT_PAGE.with(
        |CutPage {
             zoom_line_known: cell,
             ..
         }| *cell.borrow_mut() = Some(placed),
    );
}

/// F3.1 S1: what pressing the dropdown's ⊕ Zoom did, as the status sentence. The line is read through the Paste/Insert seam (`paste_line`) gated by `ZOOM_LINE_KNOWN`. Answers are `fx_zoom::arm`'s. Arming records NOTHING on the cut (the box someone draws is the effect); the camera goes down via `fx_zoom::whole_source_shown(true)`.
pub fn press_zoom_item(window: &adw::ApplicationWindow) -> String {
    // The line is known because the session was told (`note_zoom_place`) or the F2.4/F2.12 seam recorded one (`INSERT_PLACE_KNOWN`): one notion of "a line exists". With neither, S1's refusal.
    let known = CUT_PAGE.with(
        |CutPage {
             zoom_line_known: cell,
             ..
         }| *cell.borrow(),
    ) == Some(true)
        || WINDOW_PAGE.with(
            |WindowPage {
                 window_insert_place_known: cell,
                 ..
             }| *cell.borrow(),
        ) == Some(true);
    let line = if known {
        Some(crate::ui::cut_lane::paste_line())
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
            // §S1: the camera layer goes down while the box is drawn; the Disarmed branch takes the class off. `set_camera_layer_down` touches only its two widgets.
            set_camera_layer_down(window, true);
            // No marked stretch reaches the Cut page yet, so the tail names the default length (`fx_zoom::arm_tail(Some((a, b)))` is logic-side only).
            format!("{}{}", fx_zoom::ARM_WORDS, fx_zoom::arm_tail(None))
        }
        fx_zoom::Press::Disarmed => {
            set_zoom_armed(false);
            set_camera_layer_down(window, false);
            // §A.1 gives no disarm sentence, so this one is ours, in the shape of the other cancellations.
            "zoom disarmed \u{2014} the picture stops waiting for a box".to_string()
        }
    }
}

pub fn zoom_armed() -> bool {
    CUT_PAGE.with(
        |CutPage {
             zoom_armed: cell, ..
         }| cell.get(),
    )
}

pub fn set_zoom_armed(armed: bool) {
    CUT_PAGE.with(
        |CutPage {
             zoom_armed: cell, ..
         }| cell.set(armed),
    );
}

/// F3.1 S2 → S4: the form a finished drag left on the page, for widget tests ([`fx_zoom::Form`]).
pub fn zoom_form_open() -> Option<fx_zoom::Form> {
    CUT_PAGE.with(
        |CutPage {
             zoom_form: cell, ..
         }| cell.borrow().clone(),
    )
}

/// Store or drop the pending form; dropping is Apply and Cancel, and visibility follows in `ui::show_zoom_form`.
pub fn set_zoom_form(form: Option<fx_zoom::Form>) {
    CUT_PAGE.with(
        |CutPage {
             zoom_form: cell, ..
         }| *cell.borrow_mut() = form,
    );
}

/// F3.1 S2: the panel's picture size in px, pinned at 240 x 135 (see `page_box`); both the `set_size_request` and the drag fallback read it.
pub const PREVIEW_PANEL_W: f64 = 240.0;
pub const PREVIEW_PANEL_H: f64 = 135.0;

/// This window's preview panel, resolved through its own content (as `effects_lane_box`).
pub fn preview_panel(window: &adw::ApplicationWindow) -> Option<gtk::Widget> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, "preview-panel")
}

/// F3.1 S2: the box came off the preview, now what? The seam for the gesture AND the widget test, like F2.12's `insert_chosen_with_length`; the source size is handed in (no ffprobe headless); every answer is `fx_zoom`'s. `marked` is `None` as no live marking seam reaches the Cut page (see `tests/suite/cut_zoom_by_hand.rs::f3_1_s3_defaults`).
pub fn zoom_drag_ended_with_source(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
    source: (f64, f64),
) -> String {
    if !zoom_armed() {
        if crate::ui::cut_hand::held_effect().is_some() {
            return "no zoom armed \u{2014} pick \u{2295} Zoom first".to_string();
        }
        let threshold = f64::from(window.settings().gtk_dnd_drag_threshold());
        let travelled = (to.0 - from.0).abs() > threshold || (to.1 - from.1).abs() > threshold;
        if !travelled {
            return cut_preview_press::play_toggle_on_click(window).unwrap_or_default();
        }
        let said = press_on_release(window, from, to);
        if !said.is_empty() {
            return said;
        }
        return "no zoom armed \u{2014} pick \u{2295} Zoom first".to_string();
    }
    let width = (to.0 - from.0).abs();
    let height = (to.1 - from.1).abs();
    if !fx_zoom::drag_counts(width, height) {
        // Under the floor this was a click, not a framing; the arm goes too (§S1 makes the second press a disarm).
        set_zoom_armed(false);
        set_camera_layer_down(window, false);
        return format!(
            "too small to zoom \u{2014} a box has to be at least {:.0} px",
            fx_zoom::DRAG_MIN_PX
        );
    }
    let at = crate::ui::cut_lane::paste_line();
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let box_ = match fx_zoom::free_rectangle(from, to) {
        Some(box_) => box_,
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
        ui::press_row(window),
    );
    set_zoom_armed(false);
    // The arm is off, so the camera comes back up. The record's bar arrives with `press_zoom_apply`'s refresh.
    set_camera_layer_down(window, false);
    // S4: the form opens in the same turn. Ownership is claimed INSIDE `ui::show_zoom_form` before it resolves the holder.
    show_zoom_form(window, &form);
    format!("{} \u{2014} the form says how it ends", fx_zoom::label(at))
}

/// The holder without the ownership gate, ONLY to hide-and-empty before a redraw.
fn zoom_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    ui::find_widget_by_name(&content, &cut_zoom_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

fn cut_zoom_form_name() -> String {
    format!("zoom-form-{}", crate::shell::Page::Cut.label())
}

/// This window's zoom-form holder, resolved through its own content (as `insert_form_box`, `effects_lane_box`).
pub fn zoom_form_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    // Only the window owning the live form gets its holder back: `page_box` builds a `zoom-form` on every tab of every window and a closed window's tree survives parented, so resolving by name alone appends into another window's page (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`).
    if !ui::zoom_form_owner(window) {
        return None;
    }
    let content = window.content()?;
    ui::find_widget_by_name(&content, &cut_zoom_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// F3.1 S4: draw `Zoom at m:ss`. Every word comes from `fx_zoom` so the form cannot drift. Children are cleared FROM THE HOLDER, never by a tree-wide name search (a closed GTK window keeps its widgets parented).
pub fn show_zoom_form(window: &adw::ApplicationWindow, form: &fx_zoom::Form) {
    // Hidden FIRST so its children cannot read as "already parented here" in the clear below.
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
    // Everything is built into locals and appended LAST in one pass (GTK asserts on appending a parented widget); the holder is resolved through THIS window's content.
    CUT_PAGE.with(
        |CutPage {
             last_zoom_window: cell,
             ..
         }| *cell.borrow_mut() = Some(window.clone()),
    );
    let Some(holder) = zoom_form_box(window) else {
        return;
    };
    // Drop whatever THIS holder showed; never a name search, which cleared one form from under another window.
    let stale: Vec<gtk::Widget> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .collect();
    for old in stale {
        holder.remove(&old);
    }

    let heading = gtk::Label::new(Some(&fx_zoom::form_title(form.at)));
    heading.set_widget_name("zoom-heading");
    heading.set_xalign(0.0);

    let length = zoom_field_row(
        fx_zoom::FORM_FIELDS[0],
        fx_zoom::LENGTH_HELP,
        "zoom-field-length",
        &trim_seconds(form.dur),
    );

    // At the end: Pull back / Stay on it, one grouped answer (`end_choices` puts the chosen one first, §A.1), each with its reason.
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
    // Fade out is greyed when staying, with the reason swapped into its tooltip (§A.1).
    let fade_out = zoom_field_row(
        fx_zoom::FORM_FIELDS[3],
        fx_zoom::FADE_OUT_HELP,
        "zoom-field-fade-out",
        &trim_seconds(form.tout),
    );
    if let Some(entry) = find_named(fade_out.upcast_ref(), "zoom-field-fade-out")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        let greyed = fx_zoom::fade_out_greyed(form.stay);
        entry.set_sensitive(!greyed);
        if greyed {
            entry.set_tooltip_text(Some(fx_zoom::NO_WAY_BACK));
        }
    }

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

    // Handlers go on BEFORE the widgets are parented: GTK fires `toggled` from `set_active(true)`, which would otherwise write `stay` onto the form before anyone chose.
    let owner = window.clone();
    for (name, button) in [(first_name, &first_button), (second_name, &second_button)] {
        let wants_stay = name.ends_with("stay-on-it");
        let owner = owner.clone();
        button.connect_toggled(move |check| {
            // Only turning THIS control on answers, and only for the window owning the live form.
            if CUT_PAGE.with(
                |CutPage {
                     zoom_form_refresh: cell,
                     ..
                 }| cell.get(),
            ) || !check.is_active()
            {
                return;
            }
            if !ui::zoom_form_owner(&owner) {
                return;
            }
            CUT_PAGE.with(
                |CutPage {
                     zoom_form_refresh: cell,
                     ..
                 }| cell.set(true),
            );
            store_zoom_stay(wants_stay);
            CUT_PAGE.with(
                |CutPage {
                     zoom_form_refresh: cell,
                     ..
                 }| cell.set(false),
            );
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

    // ONE append pass in the order `spec/img/06-zoom.png` reads them; each widget is parented once.
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
    // Stored only now that the widgets exist, so `zoom_form_open()` means a form is on screen.
    set_zoom_form(Some(form.clone()));
    // §A's pinned heading names the open form and carries its ✕, as for the insert form.
    crate::ui::cut_form_frame::set_pinned(
        window,
        crate::ui::cut_form_frame::open_form_title().as_deref(),
    );
}

/// One labelled numeric row: name, help as tooltip, entry named for the field. Built detached.
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

/// `1.0` reads as `1`, `0.5` as `0.5`: no trailing `.0`.
pub fn trim_seconds(seconds: f64) -> String {
    if (seconds - seconds.round()).abs() < 1e-9 {
        format!("{}", seconds.round() as i64)
    } else {
        format!("{seconds}")
    }
}

/// Find a named widget inside a detached subtree, so the fade-out greying is set before its row is parented.
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

fn store_zoom_stay(stay: bool) {
    let Some(mut form) = zoom_form_open() else {
        return;
    };
    form.stay = stay;
    // A staying camera has no way back, so its fades go to nought, via `fx_zoom::fades`.
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

/// Read one numeric field of the open form; unparseable answers `None`, which Apply turns into the stored value.
pub fn zoom_entry(name: &str) -> Option<gtk::Entry> {
    CUT_PAGE.with(
        |CutPage {
             last_zoom_window: cell,
             ..
         }| {
            cell.borrow()
                .as_ref()
                .and_then(|w| ui::find_widget_by_name(w.upcast_ref(), name))
                .and_then(|w| w.downcast::<gtk::Entry>().ok())
        },
    )
}

/// F3.1 S2: the box's size in the picture's pixels. The panel is pinned at [`PREVIEW_PANEL_W`] x [`PREVIEW_PANEL_H`] but only `set_halign(Start)`, so GTK reports a natural size far larger (1815 x 1809 measured) and an unrealized width of 0. The pin IS the answer; never `natural.max(...)`, which would hand back an hf at the clamp floor.
pub fn preview_picture_px() -> (f64, f64) {
    // Always the PINNED frame, never the realized allocation: it differs between an isolated test (135 px) and a loaded suite (~2000 px), putting hf on the clamp depending on scheduler luck.
    (PREVIEW_PANEL_W, PREVIEW_PANEL_H)
}

/// F3.1 S1: lower or raise the camera layer on the preview, nothing else (§S1 "the camera layer goes down so the whole source shows"): the panel wears the arm's class and the camera rect stops drawing. Deliberately NOT `refresh_effects_lane` (11 `gtk_box_append` criticals measured); the same two writes live there off `zoom_armed()`.
pub fn set_camera_layer_down(window: &adw::ApplicationWindow, down: bool) {
    if let Some(panel) = ui::preview_widget(window, "preview-panel") {
        ui::set_class(&panel, "zoom-camera-down", down);
    }
    if let Some(rect) = ui::preview_widget(window, "preview-camera-rect") {
        // Up means the paused frame's stroke, down means not drawn; whether there IS a stroke comes from `fx_lane`.
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
    let source = preview_picture_px();
    zoom_drag_ended_with_source(window, from, to, source)
}
