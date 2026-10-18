//! §F2.4 S1 / §F2.6 S1–S3 — the drag surface and the line-placing click: the two gestures attached to
//! `select-surface` and to the painted band, their storage, and the ✕ that clears the band. Split out of
//! `window.rs`, which is past its size budget; its doors are re-exported through `window`, so no caller's
//! path changed.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_screen;
use crate::cut_select;
use crate::ui::window_line_step::now_ms;
use crate::ui::window_doors::find_widget_by_name;
use crate::ui::window_selection::{
    draw_selection, placeholder_surface_bands, press_fx_lane, pressed_surface, select_surface,
    set_pressed_surface, SELECT_GESTURES,
};

/// The placeholder surface's pixels-per-second (F2.6). Named here rather than inlined so the widget
/// test converts its drag with the SAME number the handler uses -- two copies of 4.0 in two files are
/// how a drag assertion starts disagreeing with the code it tests.
pub const SELECT_SURFACE_PPS: f64 = cut_screen::ZOOM_AT_OPEN;

/// F2.4 S1: the click gesture that places the red line, newest window winning like [`SELECT_GESTURES`].
/// A test emits `released` on this and reads back what the rule stored.
pub fn line_click_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    LINE_CLICKS.with(|cell| cell.borrow().clone())
}

thread_local! {
    static LINE_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}

/// Attach a left-button click to this window's `select-surface` so a real press places the line.
pub(crate) fn wire_line_click_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else { return };
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

/// Attach the same press to the PAINTED band, where the geometry the rule reads is.
pub fn wire_band_line_press(window: &adw::ApplicationWindow) {
    let Some(content) = window.content() else { return };
    for name in ["track-band", "track-playhead"] {
        if let Some(band) = find_widget_by_name(&content, name) {
            crate::ui::cut_line_press::attach(window, &band);
        }
    }
}

thread_local! {
    /// The gesture attached to the painted band, kept so a widget test can fire it the toolkit's way.
    static BAND_LINE_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}
/// Store the band's line gesture (called by [`crate::ui::cut_line_press::attach`]).
pub(crate) fn store_band_line_gesture(gesture: gtk::GestureClick) {
    BAND_LINE_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// The millisecond clock the line throttle reads, for the page half that places a line.
pub fn line_now_ms() -> u64 {
    now_ms()
}

/// This window's line-placing gesture on the painted band. Test-only reader: the page attaches the gesture
/// itself (`cut_line_press::attach`) and never asks for it back, so a name saying so keeps the program path
/// honest about what calls this.
pub fn band_line_gesture_for_test(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    BAND_LINE_CLICKS.with(|cell| cell.borrow().clone())
}

/// Attach the drag gesture to this window's `select-surface` and remember it. S1's scope is chosen once at
/// the press and the band is drawn FROM THE SECOND THE HAND LANDED ON to the second under it (never from 0:
/// a drag that always started at the head of the tape could not select a stretch in the middle of a
/// session, which is the only thing a selection is for). Scope resolution stays `cut_select::draw`'s.
pub(crate) fn wire_select_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else { return };
    let gesture = gtk::GestureDrag::new();
    let win = window.clone();
    // F2.6 S1: the press decides the scope, once. Which band the y fell in (ruler / picture row / wave
    // strip / lane / selection band / effects lane) is answered by `cut_select::surface_at` -- a pure
    // function over the drawn bands -- and remembered, so the drag below never re-decides it. The begin x
    // goes with it: that second is the band's start.
    gesture.connect_drag_begin(move |_g, sx, sy| {
        let (bands, rows) = placeholder_surface_bands();
        let surface = cut_select::surface_at(sy, &bands, rows);
        set_pressed_surface(surface);
        crate::ui::cut_select_press::set_drag_begin(sx / SELECT_SURFACE_PPS);
        // The effects lane refuses a selection and puts a held effect down instead — at the press, once,
        // so dragging across it cannot put the same effect down on every update.
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
        // Pixels to seconds at the page's current zoom; until the tracks exist the placeholder runs at
        // the open-at zoom so a drag still lands on real session seconds.
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

/// Wire the ✕ to [`clear_selection`] and nothing else: the rule for what clearing means (and that it
/// clears only the selection, never the cut) lives in `cut_select::clear`.
/// Wire the ✕ to [`cut_verb_press::press_badge`] with the `selection` badge and nothing else: the rule for
/// what clearing means (and that it clears only the selection, never the cut) lives in `cut_select::clear`,
/// and the dispatch of a ✕ lives in the one badge door every ✕ on this page fires through.
pub(crate) fn wire_clear_selection(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        crate::ui::cut_verb_press::press_badge(&window, crate::cut_verbs::Badge::Selection, 0);
    });
}
