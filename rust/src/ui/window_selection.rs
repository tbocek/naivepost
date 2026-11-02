//! §05-cut#1-screen: the selection band, the held effect and its live form, and the preview's volume. Re-exported through `window`. Rules are `cut_select`'s, `fx_band`'s and `cut_hear`'s; this holds per-window state, the repainted widgets and the sentences.

use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;

use crate::cut_hear;
use crate::cut_line;
use crate::cut_select;
use crate::cut_verbs;
use crate::fx_band;
use crate::fx_record;
use crate::fx_zoom;
use crate::narration::Narration;
use crate::preview;
use crate::ui::cut_cameras::refresh_camera_rows;
use crate::ui::cut_form_frame;
use crate::ui::cut_label_hand::{close_label_form, label_form_open, show_label_form};
use crate::ui::cut_speed_hand::{close_speed_form, show_speed_form, speed_form_open};
use crate::ui::cut_svg::{
    close_svg_form, press_svg_esc, release_svg_arm_only, show_svg_form, svg_armed, svg_form_open,
};
use crate::ui::cut_text_hand::{
    close_text_form, press_text_esc, refresh_aspect_readout, release_text_arm_only, show_text_form,
    text_armed, text_form_open,
};
use crate::ui::cut_volume_hand::{close_volume_form, show_volume_form, volume_form_open};
use crate::ui::window as ui;
use crate::ui::window::refresh_effects_lane;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// The narration this window holds, newest slot; crate-visible so only flows that re-read the folder replace the lines.
pub(crate) fn held_narration() -> Option<Rc<std::cell::RefCell<Narration>>> {
    WINDOW_PAGE.with(
        |WindowPage {
             held_narration: held,
             ..
         }| held.borrow().last().cloned(),
    )
}
/// F2.6 S1: a left-drag from `from` to `to` on `surface` selected what it was drawn on, by [`cut_select::draw`] alone (the effects lane answers `None`, leaving no band). Returns what it stored.
pub fn draw_selection(
    window: &adw::ApplicationWindow,
    surface: cut_select::Surface,
    recording: Option<&str>,
    from: f64,
    to: f64,
) -> Option<cut_select::Selection> {
    let band = cut_select::draw(surface, recording, from, to);
    WINDOW_PAGE.with(
        |WindowPage {
             selected: slots, ..
         }| {
            if let Some(slot) = slots.borrow().last() {
                *slot.borrow_mut() = band.clone();
            }
        },
    );
    WINDOW_PAGE.with(
        |WindowPage {
             select_surfaces: slots,
             ..
         }| {
            if let Some(slot) = slots.borrow().last() {
                *slot.borrow_mut() = Some(surface);
            }
        },
    );
    refresh_selection_readout(window);
    band
}

/// This window's band, or `None`; the one value all selection readers use.
pub fn selection() -> Option<cut_select::Selection> {
    WINDOW_PAGE.with(
        |WindowPage {
             selected: slots, ..
         }| slots.borrow().last().and_then(|s| s.borrow().clone()),
    )
}

pub fn selection_surface() -> cut_select::Surface {
    WINDOW_PAGE
        .with(
            |WindowPage {
                 select_surfaces: slots,
                 ..
             }| slots.borrow().last().and_then(|s| *s.borrow()),
        )
        .unwrap_or(cut_select::Surface::Ruler)
}

/// F2.6 S2: the cross; routes through [`cut_select::clear`] and returns whether anything was cleared (which also greys the button).
pub fn clear_selection(window: &adw::ApplicationWindow) -> bool {
    let had = selection().is_some();
    WINDOW_PAGE.with(
        |WindowPage {
             selected: slots, ..
         }| {
            if let Some(slot) = slots.borrow().last() {
                *slot.borrow_mut() = cut_select::clear(selection());
            }
        },
    );
    refresh_selection_readout(window);
    had
}

/// F2.6 S2: drag one part of the band to `to` (session seconds) at `pps`. Snap marks are rebuilt each call from the cut, lanes and playhead. `Part::Outside` moves nothing (a new selection is S1's job).
pub fn nudge_selection(window: &adw::ApplicationWindow, part: cut_select::Part, to: f64, pps: f64) {
    let Some(band) = selection() else {
        return;
    };
    let (cut_, lanes) = selection_snap_inputs(window);
    let marks = cut_select::snap_marks(&cut_, &lanes, preview_playhead());
    let moved = match part {
        cut_select::Part::Start => cut_select::resize(&band, false, to, &marks, pps),
        cut_select::Part::End => cut_select::resize(&band, true, to, &marks, pps),
        cut_select::Part::Middle => cut_select::move_band(&band, to, &marks, pps),
        cut_select::Part::Outside => return,
    };
    WINDOW_PAGE.with(
        |WindowPage {
             selected: slots, ..
         }| {
            if let Some(slot) = slots.borrow().last() {
                *slot.borrow_mut() = Some(moved);
            }
        },
    );
    refresh_selection_readout(window);
}

/// F2.6 S4: the verbs' state for the current band: Add/Split/Remove greyed for a sound selection, Copy and Insert re-aimed; `refresh_verb_buttons` reads this.
pub fn selection_verbs(window: &adw::ApplicationWindow) -> cut_select::Verbs {
    let (cut_, _) = selection_snap_inputs(window);
    // "is there a recording to cut": a cut with no segment has nothing to split.
    let has_footage = !cut_.segs.is_empty();
    cut_select::verbs(selection().as_ref(), has_footage)
}

/// What this window can offer the snap-mark builder: the review-cut slot until the cut model round supplies a real `Cut`.
fn selection_snap_inputs(_window: &adw::ApplicationWindow) -> (cut::Cut, Vec<cut::Lane>) {
    let cut_ = WINDOW_PAGE.with(
        |WindowPage {
             review_cuts: slots, ..
         }| {
            slots
                .borrow()
                .last()
                .map(|slot| slot.borrow().clone())
                .unwrap_or_default()
        },
    );
    let lanes = cut_.lanes.clone();
    (cut_, lanes)
}

/// F2.7: which of the three buttons is live, as (add, split, remove). Split stays live with no band (F2.6's `verbs()` says false): one border at the red line, right half in hand. With a band up, `verbs()` decides all three.
pub fn verb_buttons_state(window: &adw::ApplicationWindow) -> (bool, bool, bool) {
    let verbs = selection_verbs(window);
    let split_live = match selection() {
        Some(_) => verbs.split,
        None => true,
    };
    (verbs.add, split_live, verbs.remove)
}

/// F2.7: set the three verb buttons from live state; runs from `refresh_selection_readout`, the one refresh path for the selection row.
pub(crate) fn refresh_verb_buttons(window: &adw::ApplicationWindow) {
    let (add, split, remove) = verb_buttons_state(window);
    let sensitivity = [add, split, remove];
    for ((name, _, _), live) in cut_verbs::BUTTONS.iter().zip(sensitivity) {
        if let Some(button) = ui::line_step_button(window, name) {
            button.set_sensitive(live);
        }
    }
    if let Some(button) = ui::line_step_button(window, "add-button") {
        let tip = cut_verbs::add_tip_now(selection().as_ref())
            .unwrap_or_else(|| cut_verbs::BUTTONS[0].2.to_string());
        button.set_tooltip_text(Some(&tip));
    }
    crate::ui::cut_cut_play::paint(window);
    crate::ui::cut_review_play::paint(window);
}

/// F2.7 S3: write the band into the readout and set the cross's sensitivity. The readout IS the band's marks (`cut_select::marks`); the form comes from `preview::clock`.
pub(crate) fn refresh_selection_readout(window: &adw::ApplicationWindow) {
    let band = selection();
    let text = match cut_select::marks(band.as_ref()) {
        Some((start, end)) => {
            let runs = crate::timeline::filmed_runs(&crate::ui::cut_folds::page_recordings());
            format!(
                "{} {} \u{2013} {}",
                cut_select::READOUT_PREFIX,
                preview::clock(Some(crate::timeline::filmed_seconds(&runs, start))),
                preview::clock(Some(crate::timeline::filmed_seconds(&runs, end)))
            )
        }
        None => cut_select::READOUT_NONE.to_string(),
    };
    if let Some(label) = ui::find_widget_by_name(window.upcast_ref(), "selection-readout") {
        if let Ok(label) = label.downcast::<gtk::Label>() {
            label.set_text(&text);
        }
    }
    if let Some(button) = ui::find_widget_by_name(window.upcast_ref(), "clear-selection") {
        if let Ok(button) = button.downcast::<gtk::Button>() {
            button.set_sensitive(band.is_some());
        }
    }
    // F3.2: the Aspect ratio row rides this path so it moves with the cut.
    refresh_aspect_readout(window);
    refresh_verb_buttons(window);
    // F2.12: Insert is greyed with nowhere to go and wears "Edit" while a card is held.
    ui::refresh_insert_button(window);
    // F2.9: the copy buttons.
    ui::refresh_copy_buttons(window);
    // §05-cut#1-screen: the four history buttons.
    ui::refresh_history_buttons(window);
    // F2.10: the camera rows (badges, plates, gutter switches).
    refresh_camera_rows(window);
    // F2.6 S3: the in/out marks are painted on the band, so every change queues a repaint; the painter reads the band via `cut_view::band_scene_for`.
    crate::ui::cut_view::repaint_page(window);
    // §06-effects#2: the lane and preview read the live cut on every change.
    refresh_effects_lane(window);
}

pub fn preview_playhead() -> f64 {
    ui::live_player()
        .map(|player| player.borrow().playhead.unwrap_or(0.0))
        .unwrap_or(0.0)
}

pub fn selection_gesture() -> Option<gtk::GestureDrag> {
    WINDOW_PAGE.with(
        |WindowPage {
             select_gestures: cell,
             ..
         }| cell.borrow().clone(),
    )
}
pub fn pressed_surface() -> Option<cut_select::Surface> {
    WINDOW_PAGE.with(
        |WindowPage {
             select_surfaces: slots,
             ..
         }| slots.borrow().last().and_then(|s| *s.borrow()),
    )
}

pub(crate) fn set_pressed_surface(surface: cut_select::Surface) {
    WINDOW_PAGE.with(
        |WindowPage {
             select_surfaces: slots,
             ..
         }| {
            if let Some(slot) = slots.borrow().last() {
                *slot.borrow_mut() = Some(surface);
            }
        },
    );
}

/// F3.8 S5 (`Forms are live`): close whichever per-kind form is open so ✎ Edit cannot leave two showing. Each kind's Cancel keeps its own wording.
pub(crate) fn close_any_effect_form(window: &adw::ApplicationWindow) {
    if ui::zoom_form_open().is_some() {
        ui::set_zoom_form(None);
        if let Some(holder) = ui::zoom_form_box(window) {
            holder.set_visible(false);
        }
    }
    if speed_form_open().is_some() {
        close_speed_form(window);
    }
    if text_form_open().is_some() {
        close_text_form(window);
    }
    if svg_form_open().is_some() {
        close_svg_form(window);
    }
    if volume_form_open().is_some() {
        close_volume_form(window);
    }
    if label_form_open().is_some() {
        close_label_form(window);
    }
    cut_form_frame::set_pinned(window, None);
}

/// F3.8 S1 (`a click: the form opens`): the ONE door from a band to its kind's form; a kind with no form returns `None`. A dispatch, so a new kind without a form shows as a missing arm.
pub fn open_effect_form(window: &adw::ApplicationWindow, fx: &cut::Fx) -> Option<&'static str> {
    let kind = fx.effect_kind()?;
    match kind {
        cut::EffectKind::Zoom => {
            // The zoom form takes a `fx_zoom::Form`, so rebuild its six fields from the record.
            let form = fx_zoom::Form {
                at: fx.t,
                dur: fx.dur,
                stay: fx.stay,
                trans: fx.trans,
                tout: fx.tout,
                curve: fx.ease.clone(),
                // No box shows the centre, as `zoom_drag_ended` leaves.
                cx: fx.cx.unwrap_or(0.5),
                cy: fx.cy.unwrap_or(0.5),
                hf: fx.hf.unwrap_or(0.5),
                // The camera row is the page's answer (`ui::press_row`), not a record field.
                row: ui::press_row(window),
            };
            ui::show_zoom_form(window, &form);
            Some("zoom")
        }
        cut::EffectKind::Speed => {
            show_speed_form(window, fx);
            Some("speed")
        }
        cut::EffectKind::Text => {
            show_text_form(window, fx);
            Some("text")
        }
        cut::EffectKind::Svg => {
            show_svg_form(window, fx);
            Some("svg")
        }
        cut::EffectKind::Volume => {
            show_volume_form(window, fx);
            Some("volume")
        }
        cut::EffectKind::Label => {
            show_label_form(window, fx);
            Some("label")
        }
    }
}

/// F3.8 S1/S2/S3: a press on one bar of the effects lane, answered by [`fx_band`] alone. `index` is the record's place in the cut's `fx` list (`fx-bar-<index>`), `x_px` the press within the bar, `width_px` the bar's width, `travel_px` the pointer travel, `over_kill` whether on the ✕.
/// Named `press_effect_bar`, NOT `press_band` (which picks which RECORDING a press refers to, §05).
pub fn press_effect_bar(
    window: &adw::ApplicationWindow,
    index: usize,
    x_px: f64,
    width_px: f64,
    travel_px: f64,
    over_kill: bool,
) -> fx_band::Press {
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let Some(fx) = cut_.fx.get(index).cloned() else {
        return fx_band::Press::Dropped;
    };
    // Where the hand grabbed decides slide or resize, from the lane's own pixel reach.
    let grab = fx_band::grab_at(x_px, width_px);
    let answer = fx_band::press_band(fx.t, width_px, travel_px, over_kill, true, grab);
    match answer {
        fx_band::Press::ClickForm => {
            open_effect_form(window, &fx);
            cut_form_frame::set_pinned(
                window,
                Some(&format!(
                    "edit {}",
                    fx_record::label_of(fx.effect_kind().unwrap_or(cut::EffectKind::Zoom))
                )),
            );
        }
        fx_band::Press::Hold { t, from_end: _ } => {
            ui::set_held_effect(Some(fx.clone()));
            // S1: `picked up` puts the line on the band's start; `ui::set_line_position` marks that a line exists.
            ui::set_line_position(window, cut_line::LinePos { t });
            ui::refresh_insert_button(window);
            refresh_effects_lane(window);
            let said = fx_band::picked_up(fx_record::label_of(
                fx.effect_kind().unwrap_or(cut::EffectKind::Zoom),
            ));
            if let Some(status_line) = ui::find_status(window.upcast_ref()) {
                status_line.set_text(&said);
            }
            ui::log_line(&said);
        }
        fx_band::Press::Kill => {
            let said = remove_effect_at(window, index);
            if let Some(status_line) = ui::find_status(window.upcast_ref()) {
                status_line.set_text(&said);
            }
            ui::log_line(&said);
        }
        fx_band::Press::Dropped => {
            ui::set_held_effect(None);
            refresh_effects_lane(window);
        }
    }
    answer
}

/// F3.8 S3: take ONE record out by index via `ui::record_edit`, NEVER `ui::seed_review_cut`, so F2.13's ↶ restores it.
pub fn remove_effect_at(window: &adw::ApplicationWindow, index: usize) -> String {
    let mut cut_ = crate::ui::window_preview::newest_review_cut();
    if index >= cut_.fx.len() {
        return fx_band::GONE.to_string();
    }
    let taken = cut_.fx.remove(index);
    ui::save_insert_cut(&cut_);
    ui::record_edit(window, &cut_);
    // The held thing was this record; holding a removed effect would leave an overlay of nothing.
    if crate::ui::cut_hand::held_effect()
        .is_some_and(|held| held.kind == taken.kind && held.t == taken.t)
    {
        ui::set_held_effect(None);
    }
    refresh_effects_lane(window);
    ui::refresh_insert_button(window);
    let label = fx_record::label_of(taken.effect_kind().unwrap_or(cut::EffectKind::Zoom));
    let said = fx_band::removed(label);
    ui::log_line(&said);
    said
}

/// F3.8 S3 (`⌦ with an effect held removes it`): the held record is found in the LIVE cut by kind + start, never by a remembered index.
pub fn remove_held_effect(window: &adw::ApplicationWindow) -> String {
    let Some(held) = crate::ui::cut_hand::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let Some(index) = crate::effect_rules::find_by_value(&cut_.fx, &held.kind, held.t) else {
        return fx_band::GONE.to_string();
    };
    remove_effect_at(window, index)
}

/// F3.8 S2 (`Frame steps and ←/→ nudge a held effect unsnapped`): `fx_band::nudged` IS `effect_rules::nudge`, so no snap; the band moves, never stretches.
pub fn nudge_held_effect(window: &adw::ApplicationWindow, delta: f64) -> String {
    let Some(held) = crate::ui::cut_hand::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let mut cut_ = crate::ui::window_preview::newest_review_cut();
    let Some(index) = crate::effect_rules::find_by_value(&cut_.fx, &held.kind, held.t) else {
        return fx_band::GONE.to_string();
    };
    let (t, dur) = fx_band::nudged(&held, delta);
    cut_.fx[index].t = t;
    cut_.fx[index].dur = dur;
    // §7 mid-drag rule: `cut_save` refuses a write while a gesture is in flight; a keyboard nudge is a whole act and writes.
    ui::save_insert_cut(&cut_);
    ui::record_edit(window, &cut_);
    ui::set_held_effect(Some(cut_.fx[index].clone()));
    // As in `drag_held_band`: the line rides with the band so the hold is not given up.
    ui::set_line_position(window, cut_line::LinePos { t });
    refresh_effects_lane(window);
    let said = fx_band::moved(fx_record::label_of(
        held.effect_kind().unwrap_or(cut::EffectKind::Zoom),
    ));
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    ui::log_line(&said);
    said
}

/// F3.8 S2: a drag of the record in hand. `from_end == None` slides the WHOLE band snapped against `marks`; `Some(end)` moves that end. `marks` MUST arrive without the moved effect's own two ends (§A.9). The page builds them, keeping the rule testable.
pub fn drag_held_band(
    window: &adw::ApplicationWindow,
    to: f64,
    marks: &[f64],
    pps: f64,
    session_end: f64,
    from_end: Option<bool>,
) -> String {
    let Some(held) = crate::ui::cut_hand::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let mut cut_ = crate::ui::window_preview::newest_review_cut();
    let Some(index) = crate::effect_rules::find_by_value(&cut_.fx, &held.kind, held.t) else {
        return fx_band::GONE.to_string();
    };
    let (t, dur) = match from_end {
        None => fx_band::move_whole(&held, to, marks, pps, session_end),
        Some(end) => fx_band::resize_end(&held, end, to),
    };
    cut_.fx[index].t = t;
    cut_.fx[index].dur = dur;
    // §7: the write is refused while a gesture is in flight ([`crate::cut_save::gesture_in_flight`]); the release writes.
    ui::save_insert_cut(&cut_);
    // §7 one Undo per drag: the FIRST tick in a bar gesture spends the history entry and the rest ride on it. A direct call with no gesture keeps recording (`band_hold_widgets`).
    if !crate::ui::cut_band_press::band_drag_in_flight()
        || crate::ui::cut_band_press::take_history_step()
    {
        ui::record_edit(window, &cut_);
    } else {
        crate::ui::window_history_zoom::amend_edit(window, &cut_);
    }
    ui::set_held_effect(Some(cut_.fx[index].clone()));
    // A drag carries the line with the band, else the refresh reads the OLD line against the NEW position and drops the hold.
    ui::set_line_position(window, cut_line::LinePos { t });
    refresh_effects_lane(window);
    let label = fx_record::label_of(held.effect_kind().unwrap_or(cut::EffectKind::Zoom));
    let said = match from_end {
        None => fx_band::moved(label),
        Some(_) => fx_band::resized(label),
    };
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    ui::log_line(&said);
    said
}

/// F3.8 S4 (`✎ Edit: found by kind and start · the box from the live effect`): reopen the form of the record in hand. A miss says so and changes NOTHING. The box goes through [`fx_band::live_box`] so a stale form copy cannot overwrite a moved box.
pub fn press_edit_effect(window: &adw::ApplicationWindow) -> String {
    let Some(held) = crate::ui::cut_hand::held_effect() else {
        return fx_band::GONE.to_string();
    };
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let Some(index) = crate::effect_rules::find_by_value(&cut_.fx, &held.kind, held.t) else {
        ui::log_line(fx_band::GONE);
        return fx_band::GONE.to_string();
    };
    let live = &cut_.fx[index];
    // `held` went into the hand, so its fractions are the snapshot and may be older than `live`'s.
    let stale = (
        held.cx.unwrap_or(0.0),
        held.cy.unwrap_or(0.0),
        held.wf.unwrap_or(0.0),
        held.hf.unwrap_or(0.0),
    );
    let (cx, cy, wf, hf) = fx_band::live_box(live, Some(stale));
    let mut shown = live.clone();
    shown.cx = cx;
    shown.cy = cy;
    shown.wf = wf;
    shown.hf = hf;
    ui::set_held_effect(Some(shown.clone()));
    // One form on the page at a time.
    close_any_effect_form(window);
    open_effect_form(window, &shown);
    cut_form_frame::set_pinned(
        window,
        Some(&format!(
            "edit {}",
            fx_record::label_of(shown.effect_kind().unwrap_or(cut::EffectKind::Zoom))
        )),
    );
    let said = format!(
        "editing {} \u{2014} {}",
        fx_record::label_of(shown.effect_kind().unwrap_or(cut::EffectKind::Zoom)),
        fx_band::KEPT_AS_YOU_TYPE
    );
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    ui::log_line(&said);
    said
}

/// F3.8 S3 (`Esc drops the hold and disarms ("cancelled" for a bare disarm)`): `None` when nothing is held, so the key travels on to the per-kind controllers (this one is wired FIRST among the Esc controllers). Claiming the key drops the hold and every hand-armed drag; `fx_band::CANCELLED` is the one sentence (§A.9).
pub fn press_band_esc(window: &adw::ApplicationWindow) -> Option<String> {
    let _ = crate::ui::cut_hand::held_effect()?;
    // The arms come down BEFORE the hold clears, so a refusing door leaves nothing half-done.
    if ui::zoom_armed() {
        let _ = ui::press_zoom_esc(window);
    }
    if text_armed() && press_text_esc(window).is_none() {
        // This window did not draw the caption form, so its door left the arm up; S3 needs it gone.
        release_text_arm_only();
    }
    if svg_armed() && press_svg_esc(window).is_none() {
        release_svg_arm_only();
    }
    ui::set_held_effect(None);
    // A held effect plus an OPEN form: this press drops the hold and arms; the form takes the next Esc.
    refresh_effects_lane(window);
    ui::refresh_insert_button(window);
    Some(fx_band::CANCELLED.to_string())
}

/// F3.8 S5: does THIS accepted answer take the visit's single Undo? True the first time only.
pub fn live_form_answer_is_undo(_window: &adw::ApplicationWindow) -> bool {
    WINDOW_PAGE.with(
        |WindowPage {
             live_form: cell, ..
         }| cell.borrow_mut().first_answer(),
    )
}

/// F3.8 S5: a refused answer wrote nothing, so it gives the Undo entry back.
pub fn live_form_refused() {
    WINDOW_PAGE.with(
        |WindowPage {
             live_form: cell, ..
         }| cell.borrow_mut().refusal_resets(),
    );
}

/// F2.6 S1: a press on the effects lane, the only ground that refuses a selection. With an effect in hand it PUTS IT DOWN; otherwise nothing. No band is drawn, so the selection stays.
pub fn press_fx_lane(window: &adw::ApplicationWindow) -> cut_select::FxLanePress {
    let answer = cut_select::fx_lane_press(crate::ui::cut_hand::held_effect().is_some());
    match answer {
        cut_select::FxLanePress::PutsHeldEffectDown => {
            ui::set_held_effect(None);
            ui::log_line("effect put down on the effects lane");
            // The preview's held overlay goes too, else a dashed outline claims an effect is in hand.
            refresh_effects_lane(window);
        }
        cut_select::FxLanePress::Nothing => {}
    }
    answer
}

/// The surface the placeholder track area draws, and its rows (§7 `a selection is of what it was drawn on`): a 48 px box with a ruler, ONE picture row with its wave strip and the effects lane, read by y. Both [`crate::ui::cut_line_press::press_strip`] and `wire_select_surface` go through here.
pub(crate) fn placeholder_surface_bands() -> (cut_select::SurfaceBands, usize) {
    (
        cut_select::PLACEHOLDER_BANDS,
        crate::ui::cut_line_press::STRIP_PICTURE_ROWS,
    )
}

pub fn clear_selection_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    ui::find_widget_by_name(window.upcast_ref(), "clear-selection")?
        .downcast()
        .ok()
}

pub fn selection_readout(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    ui::find_widget_by_name(window.upcast_ref(), "selection-readout")?
        .downcast()
        .ok()
}

pub fn select_surface(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    ui::find_widget_by_name(window.upcast_ref(), "select-surface")?
        .downcast()
        .ok()
}

/// F2.5 S6: set this window's one preview volume from a slider percent (0..100) and push it to the live player. Returns the gain written (`0..=1`); the clamp is [`cut_hear::clamp_volume`].
pub fn set_preview_volume(percent: f64) -> f64 {
    let volume =
        WINDOW_PAGE.with(
            |WindowPage {
                 preview_volumes: slots,
                 ..
             }| {
                slots.borrow().last().cloned().unwrap_or_else(|| {
                    Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::new()))
                })
            },
        );
    volume.borrow_mut().set_percent(percent);
    let gain = volume.borrow().value();
    if let Some(player) = ui::live_player() {
        player.borrow_mut().volume = Some(gain);
    }
    gain
}

/// F2.5 S6: the one number as a gain, which [`cut_hear::mix_at`] is handed; default full if no slider was shown.
pub fn preview_volume() -> f64 {
    WINDOW_PAGE.with(
        |WindowPage {
             preview_volumes: slots,
             ..
         }| {
            slots
                .borrow()
                .last()
                .map(|v| v.borrow().value())
                .unwrap_or(cut_hear::VOLUME_DEFAULT)
        },
    )
}

/// F2.5: ask the hearing rules what this tick means and store it on this window's player. `cut_hear::mix_at` decides lanes, footage mute, loudness and speed; this lands `footage_muted` for the pipeline.
pub fn apply_mix(segs: &[cut::Seg], lanes: &[cut::Lane], fx: &[cut::Fx], t: f64) -> cut_hear::Mix {
    let mut mix = cut_hear::mix_at(segs, lanes, fx, t, preview_volume());
    // F2.12 S7 (`card sound is its own pipeline`): while a SPLICED card plays the footage is held on the wall clock (`card_hold_advance`), so the footage is muted here, never the card.
    let held_by_card = crate::ui::cut_card_preview::card_hearing(
        &cut::Cut {
            segs: segs.to_vec(),
            ..Default::default()
        },
        t,
    )
    .is_some_and(|hearing| hearing.footage_held);
    if held_by_card {
        mix.footage_muted = true;
    }
    // §05-cut#8 bullet 2: ONE line per CHANGE of the footage mute, naming the reason (wording `preview::sound_change`'s); asked before the answer is overwritten so a 100 ms tick does not repeat it.
    crate::ui::preview_sound::report_mute_change(
        crate::ui::preview_sound::heard_before(),
        crate::ui::preview_sound::heard_now(&mix),
        held_by_card,
        &|line| ui::log_line(line),
    );
    // §06-effects#2 (§9 `it SHOULD say "sound at 1× in the render only"`): a speed asking its sound at 1× needs a clock this preview lacks, so it plays with the picture and says so. Asked of the MIX so any door (form, loaded project, F3.10) gets the sentence; one line per entry.
    crate::ui::preview_sound::report_sound_clock_change(
        crate::ui::preview_sound::sound_clock_noticed(),
        mix.sound_needs_two_clocks,
        crate::ui::preview_sound::unpreviewable_snd_under(fx, t).unwrap_or(""),
        &|line| ui::log_line(line),
    );
    if let Some(player) = ui::live_player() {
        player.borrow_mut().footage_muted = mix.footage_muted;
    }
    mix
}
