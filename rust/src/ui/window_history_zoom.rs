//! §05-cut#1-screen — the history buttons and the zoom pair: the door from a press to the rule, the
//! repaint from the answer. Split out of `window.rs`, which is past its size budget; its doors are
//! re-exported through `window`.
//!
//! The four history buttons and the −/+ pair live in the toolbar; the rules they consult are
//! `cut::History` (the stack, its depth bound `P.layout.undoDepth`) and `cut_screen`'s zoom ladder.
//! What this section adds is the door from a press to those rules and nothing else: each function takes
//! the window, calls one rule, prints what came back, and repaints from the answer.

use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_screen;
use crate::fx_aspect;
use crate::fx_zoom;
use crate::ui::cut_folds::page_recordings;
use crate::ui::cut_svg::{svg_armed, svg_drag_ended};
use crate::ui::cut_text_hand::{press_aspect, text_armed, text_drag_ended};
use crate::ui::cut_zoom_hand::{
    preview_panel, set_camera_layer_down, set_zoom_armed, set_zoom_form, zoom_armed,
    zoom_drag_ended, zoom_entry, zoom_form_box, zoom_form_open, LAST_ZOOM_WINDOW,
};
use crate::ui::window as ui;
use crate::ui::window_bench::refresh_cut_readouts;

// --- §05-cut#1-screen: the history and zoom groups -----------------------------------------------
//
// The four history buttons and the −/+ pair live in the toolbar; the rules they consult are
// `cut::History` (the stack, its depth bound `P.layout.undoDepth`) and `cut_screen`'s zoom ladder.
// What this section adds is the door from a press to those rules and nothing else: each function takes
// the window, calls one rule, prints what came back, and repaints from the answer.

thread_local! {
    /// This window's edit history (§2), newest slot last like [`REVIEW_CUTS`] so the newest window is
    /// the live one. Opened on whatever cut the page was built with, which §2 makes both the first
    /// state and the base Revert returns to.
    static CUT_HISTORIES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut::History>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The TEST-FACING door for "the page has been edited". Every mutating press in the app records through
/// [`record_edit`], which publishes, records and refreshes in one step; this is the narrower seam a test
/// uses when it wants to say "treat what is on screen as an edit" without going through a widget -- which
/// is why `tests/suite/cut_screen_toolbar_widgets.rs` calls it after a Split. It pushes unconditionally
/// (`History::push`), where the app-side door asks first whether anything actually changed.
///
/// A real ＋ Add lights Undo WITHOUT any call to this function: `report_verb`'s Applied branch calls
/// `record_edit`, so the button's state comes from the app's own recording rather than from a caller
/// remembering to say so. Do not use this to make a button light in production code.
pub fn note_edit(window: &adw::ApplicationWindow) {
    let history = cut_history(window);
    history.borrow_mut().push(&ui::newest_review_cut());
    refresh_history_buttons(window);
}

/// This window's history, opened on the cut it currently shows if none was opened yet.
pub(crate) fn cut_history(_window: &adw::ApplicationWindow) -> Rc<std::cell::RefCell<cut::History>> {
    let held = CUT_HISTORIES.with(|slot| slot.borrow().last().cloned());
    if let Some(held) = held {
        return held;
    }
    let fresh = Rc::new(std::cell::RefCell::new(cut::History::open(&ui::newest_review_cut())));
    CUT_HISTORIES.with(|slot| slot.borrow_mut().push(fresh.clone()));
    fresh
}

/// F2.13: re-open this window's edit history on the cut a caller is about to show, so the base Revert
/// returns to is that cut and not whatever was newest when the history happened to be touched first.
/// Used by `build_window` at project open (against the cut read from `cut/cut.json`) and by tests that
/// seed a cut into an already-built window — both are "this is what the page opens with".
/// F2.13: re-open this window's edit history on the cut a caller is about to show, so the base Revert
/// returns to is that cut and not whatever was newest when the history happened to be touched first.
/// Used by `build_window` at project open (against the cut read from `cut/cut.json`) and by tests that
/// seed a cut into an already-built window — both are "this is what the page opens with".
pub fn reopen_history_on(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    let _ = window;
    CUT_HISTORIES.with(|slot| {
        let fresh = Rc::new(std::cell::RefCell::new(cut::History::open(cut_)));
        let mut slot = slot.borrow_mut();
        if let Some(latest) = slot.last_mut() {
            *latest = fresh;
        } else {
            slot.push(fresh);
        }
    });
}

/// Undo (§1 item 18): take the page back one state. Returns the line for the status bar — either what
/// the page went back to, or the fact that there was nothing behind it.
pub fn press_undo(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let back = history.borrow_mut().undo();
    match back {
        Some(snapshot) => {
            let mut cut_ = ui::newest_review_cut();
            snapshot.restore(&mut cut_);
            publish_cut(&cut_);
            // The lane and the preview refresh with it: a bar left on screen for an effect Undo took away
            // is a lie about what this cut holds.
            ui::refresh_effects_lane(window);
            // ...and so do the timeline readings: Undo changes the segment count, so a `Segments:` row that
            // still shows the undone answer would be the same kind of lie.
            refresh_cut_readouts(window);
            // S1: the sentence is `cut::undone`'s, not composed here.
            let said = cut::undone(cut_.segs.len());
            ui::log_line(&said);
            said
        }
        None => {
            let said = "nothing to undo \u{2014} you are at the state this page opened with".to_string();
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
            publish_cut(&cut_);
            ui::refresh_effects_lane(window);
            refresh_cut_readouts(window);
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

/// Revert (§1 item 20): drop everything added or removed by hand and go back to the base — the last
/// suggestion, or what the page opened with when there has been none.
pub fn press_revert(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let already_base = history.borrow().base_is_the_screen(&ui::newest_review_cut());
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
    publish_cut(&cut_);
    ui::refresh_effects_lane(window);
    refresh_cut_readouts(window);
    let said = cut::reverted(snapshot.segs.len(), had.segs.len());
    ui::log_line(&said);
    said
}

/// Clear (§1 item 21): every kept stretch and every effect off the timeline, recordings as loaded.
/// It is itself an edit, so Undo brings them back — which is why the result is pushed rather than
/// simply written.
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
    publish_cut(&cleared);
    ui::refresh_effects_lane(window);
    let said = cut::cleared_message(had.segs.len(), had.fx.len());
    ui::log_line(&said);
    said
}

/// Write a cut back as the page's current one, so the readouts, the verbs and every later flow see it.
pub(crate) fn publish_cut(cut_: &cut::Cut) {
    ui::REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_.clone();
        }
    });
}

/// F2.13 S4: the ONE door every mutating press writes through. It publishes the new cut, records it on
/// the history, and refreshes the four history buttons from the stack — so a button lighting up always
/// means a snapshot is behind it, never that some caller remembered to say so.
///
/// Recording goes through [`cut::History::note_change`] rather than `push`: several doors write things
/// that are not edits along with the cut (a fold toggle persists `folds`, which `Snapshot` deliberately
/// omits so Undo can never unfold the page under someone; the watched row is not a cut field at all),
/// and comparing the snapshot against the state on screen is what keeps those off the stack. A door whose
/// write changed none of the seven editable things therefore leaves Undo exactly as greyed as it was.
///
/// The history's OWN writes (`press_undo`, `press_redo`, `press_revert`) use plain [`publish_cut`]: they
/// move the pointer or reset the base, and recording them would put the undo itself on the undo stack.
pub(crate) fn record_edit(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    publish_cut(cut_);
    // Whether the write was an edit at all is the history's own question (`note_change`), not the door's:
    // a drag released at the second it was already at changes nothing and therefore takes no step.
    cut_history(window).borrow_mut().note_change(cut_);
    refresh_history_buttons(window);
    // Every edit changes the timeline, and four of the form column's readings answer from it. That column is
    // built ONCE by `cut_form_column` for an empty cut, so without this the page kept saying `Segments: 0`
    // beside landed scenes until something else happened to repaint it.
    refresh_cut_readouts(window);
}

/// − / + (§1 item 22): one step of the zoom ladder over THIS page's view, its floor where the whole
/// session fits (`cut_view::floor_pps`) rather than the placeholder strip's constant, so a session of any
/// length stops at “all of it on screen”.
pub fn press_zoom(window: &adw::ApplicationWindow, inward: bool) -> String {
    // The ladder's floor is the strip's DECLARED fit (§A), not the full-page band's and not the width GTK
    // happened to allocate it: `strip_pps` clamps over `view_px(STRIP_WIDTH)`, so a floor worked out over
    // the realized allocation (which the Cut page's paned stretches across the column) would be a number
    // nothing else agrees with — − would stop there while the strip still showed the tape from its first
    // second. Same surface both ways, asked of the constant the strip was built with.
    let _ = window;
    let width = f64::from(crate::ui::window::STRIP_WIDTH);
    let (next, _) = crate::ui::cut_view::zoom_step(
        inward,
        &crate::timeline::filmed_runs(&page_recordings()),
        crate::ui::cut_view::extents(width).0,
        crate::ui::cut_view::view_px(width),
    );
    set_preview_pps(next);
    repaint_scrollbar(window);
    let said = format!("zoom {:.1} px a second", next);
    ui::log_line(&said);
    said
}

/// Repaint badge **31** and the band beside it from the live view. Lives in `cut_view`, which owns the bar.
pub(crate) fn repaint_scrollbar(window: &adw::ApplicationWindow) {
    crate::ui::cut_view::refresh_page_scrollbar(window);
}

/// §1's three wheel gestures, attached to the widgets they belong to. The ground names live here because
/// this is where those widgets are named; `cut_view::attach_wheel` does the attaching.
pub(crate) fn wire_wheel_gestures(window: &adw::ApplicationWindow) {
    crate::ui::cut_view::attach_wheel(
        window,
        &[
            ("group-transport", cut_screen::WheelOver::Transport),
            ("preview-panel", cut_screen::WheelOver::Preview),
            ("track-strip", cut_screen::WheelOver::Tracks),
        ],
        |win, frames| {
            let _ = ui::step_frames_of(win, frames);
        },
    );
}
thread_local! {
    /// The page's pixels-per-second. Seeded at the placeholder strip's zoom so the toolbar's −/+ start
    /// from where the page actually draws; F2.10/F2.11 move the real tracks onto this same number.
    static PREVIEW_PPS: std::cell::RefCell<f64> = const { std::cell::RefCell::new(0.0) };
}

pub(crate) fn set_preview_pps(pps: f64) {
    PREVIEW_PPS.with(|cell| *cell.borrow_mut() = pps);
}

/// Wire the four history buttons and the two zoom buttons. Same shape as the verb wiring: find by name,
//  click forwards to the seam, the status line gets the sentence, and the sensitivity is refreshed
/// from the rule rather than remembered.
pub(crate) fn wire_history_and_zoom(window: &adw::ApplicationWindow) {
    for (name, seam) in [
        ("undo-button", press_undo as fn(&adw::ApplicationWindow) -> String),
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
    // The thumbnail ladder (§A's "Thumbnails [🖼− 🖼+]") sits in the FORM column beside the number it
    // drives, so its handler repaints that one label rather than a status line.
    for (name, bigger) in [("thumb-minus", false), ("thumb-plus", true)] {
        if let Some(button) = ui::line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let px = press_thumb(&win, bigger);
                if let Some(status_line) = ui::find_status(win.upcast_ref()) {
                    status_line.set_text(&format!("thumbnails {px} px"));
                }
            });
        }
    }
}

thread_local! {
    /// The page's thumbnail height in px. Seeded at `THUMB_AT_OPEN` because that is what the form
    /// column drew, so the first click steps from the number on screen rather than from zero.
    static THUMB_PX: std::cell::RefCell<u32> = const { std::cell::RefCell::new(0) };
}

/// 🖼− / 🖼+ (§A): one third of a step, clamped 40..160 by `cut_screen::thumb_down`/`thumb_up`, and
/// the `cut-readout-thumbnails` label is repainted from the new number so the readout can never drift
/// from the control that owns it. Returns the new height.
pub fn press_thumb(_window: &adw::ApplicationWindow, bigger: bool) -> u32 {
    let now = THUMB_PX.with(|cell| {
        let held = *cell.borrow();
        let next = if bigger {
            cut_screen::thumb_up(held.max(cut_screen::THUMB_AT_OPEN))
        } else {
            cut_screen::thumb_down(held.max(cut_screen::THUMB_AT_OPEN))
        };
        *cell.borrow_mut() = next;
        next
    });
    if let Some(label) = ui::find_widget_by_name(
        _window.upcast_ref(),
        &cut_screen::readout_widget("Thumbnails"),
    )
    .and_then(|w| w.downcast::<gtk::Label>().ok())
    {
        label.set_text(&format!("{now} px"));
    }
    now
}

/// Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y (§1 items 18-19's chords). One controller on the window, following
/// [`wire_line_keys`]' shape: the key decides, the shift decides which of the two directions, and an
/// unheld letter is left to whatever widget has focus.
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
    HISTORY_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}

thread_local! {
    static HISTORY_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// This window's history key controller, newest window winning like [`line_key_controller`].
pub fn history_key_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    HISTORY_KEYS.with(|cell| cell.borrow().clone())
}

/// F3.1 S2: this window's zoom drag, newest-window rule as everywhere else here (`SELECT_GESTURES`), so a
/// widget test can fire `drag-begin`/`drag-end` on the real gesture instead of calling the seam directly.
pub fn zoom_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    ZOOM_GESTURES.with(|cell| cell.borrow().clone())
}

thread_local! {
    static ZOOM_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
}

/// F3.1 S2: the drag on the preview that draws the box. The gesture carries no rule: it converts its own
/// press position and offset into two corners and hands them to [`zoom_drag_ended`], which is the same seam
/// the widget test drives. Left button only — the right button belongs to trim/move (F2.8).
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
        let at = ui::paste_line(&win_update);
        let fit = crate::ui::cut_preview_frame::fit_size_of(
            &win_update,
            ui::PREVIEW_PANEL_W,
            ui::PREVIEW_PANEL_H,
        );
        if let Some(box_) = crate::ui::cut_preview_press::box_edit_preview(
            &win_update,
            at,
            fit,
            (sx, sy),
            (sx + dx, sy + dy),
        ) {
            crate::ui::cut_preview_frame::note_edit(true);
            crate::ui::cut_preview_frame::note_box(Some(box_), &win_update);
            return;
        }
        crate::ui::cut_preview_frame::note_box(Some((sx, sy, dx, dy)), &win_update);
    });
    let win_end = window.clone();
    gesture.connect_drag_end(move |_g, dx, dy| {
        let (sx, sy) = start.get();
        // DISPATCH BY ARMED ENTRY -- the one structural choice here that the spec does not dictate. This panel
        // carries ONE left-button drag gesture (F3.1's); adding a second button-1 gesture for F3.4 would put
        // two grab handlers on one widget fighting over the same sequence, and which one wins depends on GTK's
        // internal capture order rather than on what the user armed. So the single gesture asks who is waiting:
        // a caption arm takes the press first (its box is drawn on the output frame), and only an unarmed press
        // falls through to the zoom flow. A no-travel press arrives here too and is read as a CLICK inside
        // `text_drag_ended_with_source`, which is also where a press with nothing to place at falls back to the
        // play/pause toggle (`fx_text::press_without_travel_toggles_play`).
        // ...and a DRAWING arm is asked before the caption one. Both draw against the output frame, but the
        // svg arm is the more specific state: it only exists once a file has actually been chosen, so an svg
        // press that fell through to the text flow would drop the ink someone just picked. Order: svg, text,
        // then the zoom flow for an unarmed press.
        let say = if svg_armed() {
            svg_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy))
        } else if text_armed() {
            text_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy))
        } else {
            // §F3.4 S5 (`a box moves … and resizes`) ahead of the framing fallback: a release that started ON a
            // placed caption's box edits that caption, and only a release clear of every box is F3.1's framing
            // question. The door refuses anything armed or held, and any press without travel (that is the
            // play/pause toggle), so nothing above or below it changes its answer.
            let at = ui::paste_line(&win_end);
            let fit = crate::ui::cut_preview_frame::fit_size_of(
                &win_end,
                ui::PREVIEW_PANEL_W,
                ui::PREVIEW_PANEL_H,
            );
            match crate::ui::cut_preview_press::box_edit_on_release(
                &win_end,
                at,
                fit,
                (sx, sy),
                (sx + dx, sy + dy),
            ) {
                Some(said) => said,
                None => zoom_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy)),
            }
        };
        if let Some(status_line) = ui::find_status(win_end.upcast_ref()) {
            status_line.set_text(&say);
        }
        crate::ui::cut_preview_frame::note_edit(false);
        crate::ui::cut_preview_frame::note_box(None, &win_end);
    });
    panel.add_controller(gesture.clone());
    ZOOM_GESTURES.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// F3.2: the Aspect ratio dropdown's wire. `connect_selected_notify` is the `notify::selected` handler GTK
/// raises when the popover's own choice lands (and the one a test raises with `set_selected`), so this fires
/// on the user's pick and on nothing else. The callback converts the index to the string it stands for and
/// prints what [`press_aspect`] returns — no rule, no sentence of its own.
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

/// F3.1 S1: Esc releases the arm without drawing a box ("same entry again disarms" has a keyboard twin, and
/// §cut.md's key list drops the arm rather than leaving someone stuck with a lowered camera layer). Claims
/// ONLY Escape while armed; every other key, and Escape with nothing armed, returns `Proceed`, so typing in an
/// entry field is untouched exactly as `wire_copy_esc` leaves it.
/// Is this the window whose zoom form is live? Every zoom write goes through here so a closed window's leftover
/// handler cannot draw into, or clear, the form another window is showing -- the failure shape behind
/// `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed` across checks.
pub(crate) fn zoom_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_ZOOM_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
            .unwrap_or(false)
    })
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
    if let Some((trimmed, _switched)) = crate::effect_overlap::trim_at_lens_switch(&effect, &cut_.segs) {
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
    fx_zoom::placed_status(effect.t, effect.stay)
}

/// Read one of the ending checks back from THIS window's tree.
pub(crate) fn zoom_check(name: &str) -> Option<gtk::CheckButton> {
    LAST_ZOOM_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| ui::find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    })
}

/// S4: Cancel drops the form and changes nothing on the cut — no record pushed, no history written, so ↶
/// still points where it did before the box was drawn.
pub fn press_zoom_cancel(window: &adw::ApplicationWindow) -> String {
    set_zoom_form(None);
    if let Some(holder) = zoom_form_box(window) {
        holder.set_visible(false);
    }
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
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match press_zoom_esc(&win) {
            Some(said) => {
                if let Some(status_line) = ui::find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    window.add_controller(controller);
}

/// Which of the four history buttons may be pressed right now, from the stack's own counts. Called
/// from [`refresh_selection_readout`] so a greyed history button is today's answer along with every
/// other greyed control on the page, never a leftover from the last draw.
pub(crate) fn refresh_history_buttons(window: &adw::ApplicationWindow) {
    let history = cut_history(window);
    let held = history.borrow();
    let depth = held.depth();
    // `depth - at - 1` is what lies ahead of the pointer; `at` is private, so the two counts come from
    // the questions the stack answers rather than from its insides.
    let undo_left = usize::from(held.can_undo());
    let redo_left = usize::from(held.can_redo());
    let ever_edited = depth > 1 || !held.base_is_the_screen(&ui::newest_review_cut());
    let live = cut_screen::history_buttons_enabled(undo_left, redo_left, ever_edited);
    for (tool, is_live) in cut_screen::HISTORY_BUTTONS.iter().zip(live) {
        if let Some(button) = ui::line_step_button(window, tool.name) {
            button.set_sensitive(is_live);
        }
    }
}
