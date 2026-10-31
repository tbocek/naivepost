//! §F2.4 S1 / §F2.6 S1–S3 — the drag surface and the line-placing click: the two gestures attached
//! to `select-surface` and to the painted band, their storage, and the ✕ that clears the band.
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_screen;
use crate::cut_select;
use crate::ui::window_doors::find_widget_by_name;
use crate::ui::window_line_step::now_ms;
use crate::ui::window_selection::{
    draw_selection, placeholder_surface_bands, press_fx_lane, pressed_surface, select_surface,
    set_pressed_surface, SELECT_GESTURES,
};

/// The placeholder surface's pixels-per-second (F2.6).
pub const SELECT_SURFACE_PPS: f64 = cut_screen::ZOOM_AT_OPEN;

/// F2.4 S1: the click gesture that places the red line, newest window winning like
/// [`SELECT_GESTURES`].
pub fn line_click_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    LINE_CLICKS.with(|cell| cell.borrow().clone())
}

thread_local! {
    static LINE_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn wire_line_click_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else {
        return;
    };
    let gesture = gtk::GestureClick::new();
    // Left button only: the right button belongs to trim/move (F2.8) and must not move the line.
    gesture.set_button(1);
    let win = window.clone();
    gesture.connect_released(move |_g, _n_press, x, y| {
        crate::ui::cut_line_press::press_strip(&win, x, y, now_ms());
    });
    area.add_controller(gesture.clone());
    LINE_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

pub fn wire_band_line_press(window: &adw::ApplicationWindow) {
    let Some(content) = window.content() else {
        return;
    };
    for name in ["track-band", "track-playhead"] {
        if let Some(band) = find_widget_by_name(&content, name) {
            crate::ui::cut_line_press::attach(window, &band);
        }
    }
}

thread_local! {
    static BAND_LINE_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}
pub(crate) fn store_band_line_gesture(gesture: gtk::GestureClick) {
    BAND_LINE_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// The millisecond clock the line throttle reads, for the page half that places a line.
pub fn line_now_ms() -> u64 {
    now_ms()
}

pub fn band_line_gesture_for_test(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    BAND_LINE_CLICKS.with(|cell| cell.borrow().clone())
}

pub(crate) fn wire_select_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else {
        return;
    };
    let gesture = gtk::GestureDrag::new();
    let win = window.clone();
    // F2.6 S1: the press decides the scope, once.
    gesture.connect_drag_begin(move |_g, sx, sy| {
        let (bands, rows) = placeholder_surface_bands();
        let surface = cut_select::surface_at(sy, &bands, rows);
        set_pressed_surface(surface);
        crate::ui::cut_select_press::set_drag_begin(sx / SELECT_SURFACE_PPS);
        // The effects lane refuses a selection and puts a held effect down instead.
        if surface == cut_select::Surface::FxLane {
            press_fx_lane(&win);
        }
    });
    let fx_win = window.clone();
    gesture.connect_drag_update(move |_g, x, y| {
        let surface = pressed_surface(&fx_win).unwrap_or_else(|| {
            let (bands, rows) = placeholder_surface_bands();
            cut_select::surface_at(y, &bands, rows)
        });
        // The effects lane takes nothing back: it puts a held effect down and draws no band, so the
        // selection that was there stays exactly as it was.
        if surface == cut_select::Surface::FxLane {
            return;
        }
        // Pixels to seconds at the page's current zoom.
        let from = crate::ui::cut_select_press::drag_begin().unwrap_or(0.0);
        draw_selection(
            &fx_win,
            surface,
            crate::ui::cut_select_press::recording_for_surface(&fx_win, surface, y).as_deref(),
            from,
            x / SELECT_SURFACE_PPS,
        );
    });
    area.add_controller(gesture.clone());
    SELECT_GESTURES.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// Wire the ✕ to [`clear_selection`] and nothing else: the rule for what clearing means (and that
/// it clears only the selection, never the cut) lives in `cut_select::clear`.
pub(crate) fn wire_clear_selection(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        crate::ui::cut_verb_press::press_badge(&window, crate::cut_verbs::Badge::Selection, 0);
    });
}
