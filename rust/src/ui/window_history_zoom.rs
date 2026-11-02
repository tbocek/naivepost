//! §05-cut#1-screen — the history buttons and the zoom pair: the door from a press to the rule, the
//! repaint from the answer. Split out of `window.rs`, which is past its size budget; its doors are
//! re-exported through `window`.

use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_screen;
use crate::fx_aspect;
use crate::fx_zoom;
use crate::ui::cut_folds::page_recordings;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::cut_svg::{svg_armed, svg_drag_ended};
use crate::ui::cut_text_hand::{press_aspect, text_armed, text_drag_ended};
use crate::ui::cut_zoom_hand::{
    preview_panel, set_camera_layer_down, set_zoom_armed, set_zoom_form, zoom_armed,
    zoom_drag_ended, zoom_entry, zoom_form_box, zoom_form_open,
};
use crate::ui::window as ui;
use crate::ui::window_bench::refresh_cut_readouts;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

pub fn note_edit(window: &adw::ApplicationWindow) {
    let history = cut_history(window);
    history.borrow_mut().push(&ui::newest_review_cut());
    refresh_history_buttons(window);
}

/// This window's history, opened on the cut it currently shows if none was opened yet.
pub(crate) fn cut_history(
    _window: &adw::ApplicationWindow,
) -> Rc<std::cell::RefCell<cut::History>> {
    let held = WINDOW_PAGE.with(
        |WindowPage {
             cut_histories: slot,
             ..
         }| slot.borrow().last().cloned(),
    );
    if let Some(held) = held {
        return held;
    }
    let fresh = Rc::new(std::cell::RefCell::new(cut::History::open(
        &ui::newest_review_cut(),
    )));
    WINDOW_PAGE.with(
        |WindowPage {
             cut_histories: slot,
             ..
         }| slot.borrow_mut().push(fresh.clone()),
    );
    fresh
}

pub fn reopen_history_on(cut_: &cut::Cut) {
    WINDOW_PAGE.with(
        |WindowPage {
             cut_histories: slot,
             ..
         }| {
            let fresh = Rc::new(std::cell::RefCell::new(cut::History::open(cut_)));
            let mut slot = slot.borrow_mut();
            if let Some(latest) = slot.last_mut() {
                *latest = fresh;
            } else {
                slot.push(fresh);
            }
        },
    );
}

pub fn press_undo(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let back = history.borrow_mut().undo();
    match back {
        Some(snapshot) => {
            let mut cut_ = ui::newest_review_cut();
            snapshot.restore(&mut cut_);
            crate::ui::cut_folds::sync_page_folds(&mut cut_);
            land_history(window, &cut_);
            // S1: the sentence is `cut::undone`'s, not composed here.
            let said = cut::undone(cut_.segs.len());
            ui::log_line(&said);
            said
        }
        None => {
            let said = cut::NOTHING_TO_UNDO.to_string();
            ui::log_line(&said);
            said
        }
    }
}

/// Redo (§1 item 19): put back what Undo took away.
pub fn press_redo(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let forward = history.borrow_mut().redo();
    match forward {
        Some(snapshot) => {
            let mut cut_ = ui::newest_review_cut();
            snapshot.restore(&mut cut_);
            crate::ui::cut_folds::sync_page_folds(&mut cut_);
            land_history(window, &cut_);
            // S1: one walk sentence for both directions (spec F2.13 gives the pair a single line).
            let said = cut::undone(cut_.segs.len());
            ui::log_line(&said);
            said
        }
        None => {
            let said = "nothing to redo \u{2014} a new edit threw that branch away".to_string();
            ui::log_line(&said);
            said
        }
    }
}

pub fn press_revert(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let already_base = history
        .borrow()
        .base_is_the_screen(&ui::newest_review_cut());
    if already_base {
        let said = cut::NOTHING_TO_REVERT.to_string();
        ui::log_line(&said);
        return said;
    }
    // What was on screen BEFORE the restore, so the sentence can say how much hand-made work went away.
    let had = ui::newest_review_cut();
    let snapshot = history.borrow_mut().revert();
    let mut cut_ = had.clone();
    snapshot.restore(&mut cut_);
    land_history(window, &cut_);
    let said = cut::reverted(snapshot.segs.len(), had.segs.len());
    ui::log_line(&said);
    said
}

/// Clear (§1 item 21): every kept stretch and every effect off the timeline, recordings as loaded.
pub fn press_clear_cut(window: &adw::ApplicationWindow) -> String {
    let had = ui::newest_review_cut();
    if had.segs.is_empty() && had.fx.is_empty() {
        let said = cut::NOTHING_TO_CLEAR.to_string();
        ui::log_line(&said);
        return said;
    }
    let cleared = crate::cut::cleared(&had);
    let history = cut_history(window);
    history.borrow_mut().push(&cleared);
    land_history(window, &cleared);
    let said = cut::cleared_message(had.segs.len(), had.fx.len());
    ui::log_line(&said);
    said
}

/// A walk through the history landed: on the file, and on everything drawn from the cut.
fn land_history(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    publish_cut(cut_);
    ui::save_insert_cut(cut_);
    // What was in hand may be gone from this state.
    ui::set_held_clip(None);
    ui::set_held_edge(None);
    ui::set_held_effect(None);
    ui::refresh_effects_lane(window);
    refresh_cut_readouts(window);
    crate::ui::cut_folds::refresh_fold_badges(window);
    crate::ui::cut_cameras::refresh_camera_rows(window);
    refresh_history_buttons(window);
}

/// Write a cut back as the page's current one, so the readouts, the verbs and every later flow see it.
pub(crate) fn publish_cut(cut_: &cut::Cut) {
    WINDOW_PAGE.with(
        |WindowPage {
             review_cuts: slots, ..
         }| {
            if let Some(slot) = slots.borrow().last() {
                *slot.borrow_mut() = cut_.clone();
            }
        },
    );
}

pub(crate) fn record_edit(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    publish_cut(cut_);
    // The one door every edit takes, so no edit can reach the screen and miss the file; mid-drag the writer waits.
    ui::save_insert_cut(cut_);
    // Whether the write was an edit at all is the history's own question (`note_change`), not the door's:
    // a drag released at the second it was already at changes nothing and therefore takes no step.
    cut_history(window).borrow_mut().note_change(cut_);
    refresh_history_buttons(window);
    // Every edit changes the timeline, and four of the form column's readings answer from it. That column is
    // built ONCE by `cut_form_column` for an empty cut, so without this the page kept saying `Segments: 0`
    // beside landed scenes until something else happened to repaint it.
    refresh_cut_readouts(window);
}

pub(crate) fn amend_edit(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    crate::ui::window::seed_review_cut(cut_);
    ui::save_insert_cut(cut_);
    cut_history(window).borrow_mut().amend(cut_);
    refresh_history_buttons(window);
    refresh_cut_readouts(window);
}

pub fn press_zoom(window: &adw::ApplicationWindow, inward: bool) -> String {
    let view = crate::ui::cut_view::wheel_gestures_view(window);
    let runs = crate::timeline::filmed_runs(&page_recordings());
    let filmed: f64 = runs.iter().map(|(start, end)| end - start).sum();
    let before = crate::ui::cut_view::pps();
    // Where the red line stands on screen before the step, in the band's own pixels.
    let line = ui::line_position().t;
    let gutter = crate::ui::cut_view::gutter_px();
    let line_x = crate::ui::cut_tracks::band_x(&crate::ui::cut_view::band_scene_for(), line)
        - crate::ui::cut_view::origin();
    let (next, _) = crate::ui::cut_view::zoom_step(
        inward,
        &runs,
        crate::ui::cut_view::total_px(filmed, before),
        view,
    );
    let on_screen = line_x >= gutter && line_x <= gutter + view;
    let wanted_x = if on_screen {
        line_x
    } else {
        gutter + view / 2.0
    };
    let after = crate::ui::cut_tracks::band_x(&crate::ui::cut_view::band_scene_for(), line);
    crate::ui::cut_view::set_origin(
        after - wanted_x,
        crate::ui::cut_view::total_px(filmed, next),
        view,
    );
    crate::ui::cut_view::repaint_page(window);
    let said = if (next - before).abs() > 1e-9 {
        format!("zoom {:.1} px a second", next)
    } else if inward {
        format!("zoom {:.1} px a second -- as close as it goes", next)
    } else {
        format!(
            "zoom {:.1} px a second -- the whole session is on screen",
            next
        )
    };
    ui::log_line(&said);
    said
}

pub(crate) fn wire_wheel_gestures(window: &adw::ApplicationWindow) {
    crate::ui::cut_view::attach_wheel(
        window,
        &[
            ("group-transport", cut_screen::WheelOver::Transport),
            ("preview-panel", cut_screen::WheelOver::Preview),
            // §1's single stack draws its ruler and rows on `track-band`; the placeholder strip keeps its own
            // controller so F2.8's press geometry stays reachable, and `attach_wheel` skips a widget that
            // already carries one, so listing both cannot double-attach.
            ("track-band", cut_screen::WheelOver::Tracks),
            ("track-strip", cut_screen::WheelOver::Tracks),
        ],
        |win, frames| {
            let _ = ui::step_frames_of(win, frames);
        },
    );
}
/// from the rule rather than remembered.
pub(crate) fn wire_history_and_zoom(window: &adw::ApplicationWindow) {
    for (name, seam) in [
        (
            "undo-button",
            press_undo as fn(&adw::ApplicationWindow) -> String,
        ),
        ("redo-button", press_redo),
        ("revert-button", press_revert),
        ("clear-cut-button", press_clear_cut),
    ] {
        if let Some(button) = ui::line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let status = seam(&win);
                if let Some(status_line) = ui::find_status(win.upcast_ref()) {
                    status_line.set_text(&status);
                }
                refresh_history_buttons(&win);
            });
        }
    }
    for (name, inward) in [("zoom-out-button", false), ("zoom-in-button", true)] {
        if let Some(button) = ui::line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let status = press_zoom(&win, inward);
                if let Some(status_line) = ui::find_status(win.upcast_ref()) {
                    status_line.set_text(&status);
                }
            });
        }
    }
}

pub fn press_wave(window: &adw::ApplicationWindow, px: f64) -> f64 {
    let held = crate::ui::cut_tracks::row_wave_px();
    crate::ui::cut_tracks::set_row_wave_px(px);
    let now = crate::ui::cut_tracks::row_wave_px();
    if now != held {
        crate::ui::cut_view::repaint_page(window);
        crate::ui::cut_cameras::refresh_camera_rows(window);
    }
    let shown = now.round();
    let said = if px > now {
        format!("sound strip at its largest, {shown} px")
    } else if px < now {
        format!("sound strip at its smallest, {shown} px")
    } else {
        format!("sound strip {shown} px")
    };
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    now
}

pub fn press_thumb(window: &adw::ApplicationWindow, px: u32) -> u32 {
    let held = crate::ui::cut_tracks::thumb_px();
    crate::ui::cut_tracks::set_thumb_px(px);
    let now = crate::ui::cut_tracks::thumb_px();
    if now != held {
        crate::ui::cut_view::repaint_page(window);
        crate::ui::cut_cameras::refresh_camera_rows(window);
    }
    // Shorter rows hand their height to the picture, as the page opened: the handle goes as far down as the bottom
    // half's new minimum lets it, and nothing empty stays under the tape. Taller rows push it up by themselves.
    if now < held {
        if let Some(split) = crate::ui::find_source_widget(window, "cut-page-split")
            .and_then(|found| found.downcast::<gtk::Paned>().ok())
        {
            split.set_position(split.height());
        }
    }
    let said = if now == cut_screen::THUMB_MAX && px >= now {
        format!("thumbnails at their largest, {now} px")
    } else if now == cut_screen::THUMB_MIN && px <= now {
        format!("thumbnails at their smallest, {now} px")
    } else {
        format!("thumbnails {now} px")
    };
    if let Some(status_line) = ui::find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    now
}

pub(crate) fn wire_history_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, mods| {
        let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if !ctrl {
            return glib::Propagation::Proceed;
        }
        let status = match key {
            // Z alone undoes; Z with Shift redoes, and so does Y — §1 spells both, because both hands
            // reach for one of them and a person should not have to remember which editor they opened.
            gtk::gdk::Key::z | gtk::gdk::Key::Z if !shift => press_undo(&win),
            gtk::gdk::Key::z | gtk::gdk::Key::Z if shift => press_redo(&win),
            gtk::gdk::Key::y | gtk::gdk::Key::Y => press_redo(&win),
            _ => return glib::Propagation::Proceed,
        };
        if let Some(status_line) = ui::find_status(win.upcast_ref()) {
            status_line.set_text(&status);
        }
        refresh_history_buttons(&win);
        glib::Propagation::Stop
    });
    window.add_controller(controller.clone());
    WINDOW_PAGE.with(
        |WindowPage {
             history_keys: cell, ..
         }| *cell.borrow_mut() = Some(controller),
    );
}

/// This window's history key controller, newest window winning like `line_key_controller`.
pub fn history_key_controller() -> Option<gtk::EventControllerKey> {
    WINDOW_PAGE.with(
        |WindowPage {
             history_keys: cell, ..
         }| cell.borrow().clone(),
    )
}

pub fn zoom_gesture() -> Option<gtk::GestureDrag> {
    WINDOW_PAGE.with(
        |WindowPage {
             zoom_gestures: cell,
             ..
         }| cell.borrow().clone(),
    )
}

pub(crate) fn wire_zoom_drag(window: &adw::ApplicationWindow) {
    let Some(panel) = preview_panel(window).and_then(|w| w.downcast::<gtk::Widget>().ok()) else {
        return;
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    let _win = window.clone();
    // `drag-begin` carries the ABSOLUTE press position; `drag-end` carries the OFFSET from it (the F2.8
    // lesson), so the release corner is start + offset and both corners are known only across the two signals.
    let start = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
    let start_begin = start.clone();
    gesture.connect_drag_begin(move |_g, x, y| {
        start_begin.set((x, y));
    });
    let start_update = start.clone();
    let win_update = window.clone();
    gesture.connect_drag_update(move |_g, dx, dy| {
        let (sx, sy) = start_update.get();
        // §F3.4 S5: while a hand drags a PLACED caption's box, show where it will land and the six lines it may
        // snap to. Asked before the armed dispatch because this is the one left-button gesture on the panel; the
        // door answers `None` for every other press (anything armed or held, no travel, no box under the pointer),
        // so an armed drag still gets only the box the flow that owns it draws.
        let at = crate::ui::cut_lane::paste_line();
        let fit =
            crate::ui::cut_preview_frame::fit_size_of(ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H);
        if let Some(box_) =
            crate::ui::cut_preview_press::box_edit_preview(at, fit, (sx, sy), (sx + dx, sy + dy))
        {
            crate::ui::cut_preview_frame::note_edit(true);
            crate::ui::cut_preview_frame::note_box(Some(box_), &win_update);
            return;
        }
        crate::ui::cut_preview_frame::note_box(Some((sx, sy, dx, dy)), &win_update);
    });
    let win_end = window.clone();
    gesture.connect_drag_end(move |_g, dx, dy| {
        let (sx, sy) = start.get();
        let say = if svg_armed() {
            svg_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy))
        } else if text_armed() {
            text_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy))
        } else {
            // §F3.4 S5 (`a box moves … and resizes`) ahead of the framing fallback: a release that started ON a
            // placed caption's box edits that caption, and only a release clear of every box is F3.1's framing
            // question. The door refuses anything armed or held, and any press without travel (that is the
            // play/pause toggle), so nothing above or below it changes its answer.
            let at = crate::ui::cut_lane::paste_line();
            let fit =
                crate::ui::cut_preview_frame::fit_size_of(ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H);
            match crate::ui::cut_preview_press::box_edit_on_release(
                &win_end,
                at,
                fit,
                (sx, sy),
                (sx + dx, sy + dy),
            ) {
                Some(said) => said,
                None => match crate::ui::cut_zoom_hand::zoom_box_edit_on_release(
                    &win_end,
                    at,
                    fit,
                    (sx, sy),
                    (sx + dx, sy + dy),
                ) {
                    Some(said) => said,
                    None => zoom_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy)),
                },
            }
        };
        if let Some(status_line) = ui::find_status(win_end.upcast_ref()) {
            status_line.set_text(&say);
        }
        crate::ui::cut_preview_frame::note_edit(false);
        crate::ui::cut_preview_frame::note_box(None, &win_end);
    });
    panel.add_controller(gesture.clone());
    WINDOW_PAGE.with(
        |WindowPage {
             zoom_gestures: cell,
             ..
         }| *cell.borrow_mut() = Some(gesture),
    );
}

pub(crate) fn wire_aspect_choice(window: &adw::ApplicationWindow) {
    let Some(widget) = ui::find_widget_by_name(window.upcast_ref(), "aspect-choice") else {
        return;
    };
    let Ok(choice) = widget.downcast::<gtk::DropDown>() else {
        return;
    };
    let win = window.clone();
    choice.connect_selected_notify(move |picker| {
        // The index is the only thing the signal carries; the string it stands for comes from the same
        // `ASPECTS` list the dropdown was built from, so the two cannot disagree about what index 2 means.
        if picker.selected() == gtk::INVALID_LIST_POSITION {
            return;
        }
        let picked = fx_aspect::ASPECTS
            .get(picker.selected() as usize)
            .copied()
            .unwrap_or(fx_aspect::SOURCE);
        let say = press_aspect(&win, picked);
        if let Some(status_line) = ui::find_status(win.upcast_ref()) {
            status_line.set_text(&say);
        }
    });
}

pub(crate) fn zoom_form_owner(window: &adw::ApplicationWindow) -> bool {
    CUT_PAGE.with(
        |CutPage {
             last_zoom_window: cell,
             ..
         }| {
            cell.borrow()
                .as_ref()
                .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
                .unwrap_or(false)
        },
    )
}

pub fn press_zoom_apply(window: &adw::ApplicationWindow) -> String {
    if !zoom_form_owner(window) {
        return "no zoom form on this page \u{2014} drag a box first".to_string();
    }
    let Some(stored) = zoom_form_open() else {
        return "no zoom to apply \u{2014} drag a box first".to_string();
    };
    // Read the six fields back. An entry that does not parse keeps what the form already held rather than
    // becoming zero, so a half-typed number cannot silently place a 0-second camera move.
    let read = |name: &str, keep: f64| -> f64 {
        zoom_entry(name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    let stay = zoom_check("zoom-end-stay-on-it").is_some_and(|check| check.is_active());
    let form = fx_zoom::Form {
        at: stored.at,
        dur: read("zoom-field-length", stored.dur),
        stay,
        trans: read("zoom-field-fade-in", stored.trans),
        tout: read("zoom-field-fade-out", stored.tout),
        curve: fx_zoom::CURVE_CHOICES[0].to_string(),
        // The box came from the drag; it is carried through untouched, never re-derived here.
        cx: stored.cx,
        cy: stored.cy,
        hf: stored.hf,
        row: stored.row,
    };
    let mut effect = match fx_zoom::apply(&form) {
        Ok(effect) => effect,
        // S4's floor: refuse with the module's own sentence and leave the form open to be corrected.
        Err(reason) => {
            // F3.8 S5: a refusal wrote nothing, so it gives the visit's Undo entry back rather than spending it.
            ui::live_form_refused();
            return reason;
        }
    };
    let mut cut_ = ui::newest_review_cut();
    // §9 (`A zoom does not cross into a clip from another camera`): the band ends at the lens
    // switch rather than jumping onto the next camera's frame, and the person is told which
    // camera it was framed on. Inside THIS edit — one ↶ still takes the whole placement back.
    if let Some((trimmed, _switched)) =
        crate::effect_overlap::trim_at_lens_switch(&effect, &cut_.segs)
    {
        effect.dur = trimmed;
        let said = crate::effect_overlap::lens_switch(effect.t, effect.cam);
        ui::log_line(&said);
        if let Some(status) = ui::find_status(window.upcast_ref()) {
            status.set_text(&said);
        }
    }
    // A zoom placed by a drag is new: one push of the applied record. `fx_record::record_into` is not the
    // path here because it lays down a fresh record of its own width, and this one's width was just typed.
    cut_.fx.push(effect.clone());
    ui::save_insert_cut(&cut_);
    // `record_edit`, NOT `seed_review_cut`: the addition must sit on the history so F2.13's ↶ takes it back.
    ui::live_form_answer_is_undo(window);
    record_edit(window, &cut_);
    ui::refresh_effects_lane(window);
    set_zoom_form(None);
    if let Some(holder) = zoom_form_box(window) {
        holder.set_visible(false);
    }
    // The form is gone, so the pinned heading goes with it (or names whatever else is still open).
    crate::ui::cut_form_frame::set_pinned(
        window,
        crate::ui::cut_form_frame::open_form_title().as_deref(),
    );
    fx_zoom::placed_status(effect.t, effect.stay)
}

/// Read one of the ending checks back from THIS window's tree.
pub(crate) fn zoom_check(name: &str) -> Option<gtk::CheckButton> {
    CUT_PAGE.with(
        |CutPage {
             last_zoom_window: cell,
             ..
         }| {
            cell.borrow()
                .as_ref()
                .and_then(|w| ui::find_widget_by_name(w.upcast_ref(), name))
                .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        },
    )
}

pub fn press_zoom_cancel(window: &adw::ApplicationWindow) -> String {
    set_zoom_form(None);
    if let Some(holder) = zoom_form_box(window) {
        holder.set_visible(false);
    }
    crate::ui::cut_form_frame::set_pinned(
        window,
        crate::ui::cut_form_frame::open_form_title().as_deref(),
    );
    "left as it was \u{2014} no zoom placed".to_string()
}

pub fn press_zoom_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if !zoom_armed() {
        return None;
    }
    set_zoom_armed(false);
    set_camera_layer_down(window, false);
    // §A.1 spells the refusal and the arm words but no exact disarm sentence, so this reuses the one
    // `press_zoom_item` gives for taking the arm off — one wording, never two near-duplicates.
    Some("zoom released \u{2014} no box was drawn".to_string())
}

pub(crate) fn wire_zoom_esc(window: &adw::ApplicationWindow) {
    crate::ui::cut_form_buttons::wire_esc(window, press_zoom_esc);
}

pub(crate) fn refresh_history_buttons(window: &adw::ApplicationWindow) {
    let history = cut_history(window);
    let held = history.borrow();
    let undo_left = usize::from(held.can_undo());
    let redo_left = usize::from(held.can_redo());
    let ever_edited = held.depth() > 1 || !held.base_is_the_screen(&ui::newest_review_cut());
    let mut live = cut_screen::history_buttons_enabled(undo_left, redo_left, ever_edited);
    // F2.13: Revert is greyed whenever the screen is the base, including right after a Revert.
    live[2] = !held.base_is_the_screen(&ui::newest_review_cut());
    for (tool, is_live) in cut_screen::HISTORY_BUTTONS.iter().zip(live) {
        if let Some(button) = ui::line_step_button(window, tool.name) {
            button.set_sensitive(is_live);
        }
    }
}
