//! F2.12 S6 (`right-click or double left-click a card → holds it, Insert becomes Edit`) — the press that
//! picks a card UP instead of placing the line.
//!
//! Kept apart from [`crate::ui::cut_line_press`]: that gesture is `set_button(1)`, so a right-click never
//! reaches it, and its second left press runs F2.4 S1b's footage pick-up reach, which S6 must ignore. This
//! rides its own `set_button(0)` gesture and answers only when [`holds_card`] says the press means a hold.
//! Seconds come from [`cut_tracks::band_span`]'s [`timeline::Span::t_at`], the walk the bars were painted from.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_insert;
use crate::timeline;
use crate::ui::{self, cut_card_preview, cut_tracks, cut_view};

/// A card's own name, as every other sentence here names a file: its last path element.
fn card_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The status line's answer when the press landed on footage: the hand is left EXACTLY as it was, so a card
/// nobody aimed at cannot throw away one already held.
pub const NOTHING: &str =
    "no card there \u{2014} a right-click or a double click holds a card, not the footage";

/// Which presses mean "hold that card": the right button on its first press, or a left press that is the
/// second or later click of a multi-click.
///
/// A synthetic `released` emit carries `n_press` and the button comes off the gesture, so no state is kept
/// between presses. A double RIGHT click holds the same card twice, which is harmless (S6: a held card makes
/// Insert `Edit`).
pub fn holds_card(button: u32, clicks: u32) -> bool {
    button == 3 || clicks >= 2
}

/// The band's second under `x_px`, through the span the bars were painted from. [`timeline::Span`] already
/// carries `cut_view::gutter_px()`, so only the scroll comes off, here rather than at each caller.
fn second_at(window: &adw::ApplicationWindow, x_px: f64) -> f64 {
    let scene = cut_view::band_scene_for(window);
    cut_tracks::band_t_at(&scene, x_px)
}

/// The hold itself: aim at a card, and it comes into the hand with `insert-button` re-worded underneath.
/// Returns the sentence it printed.
pub fn press(window: &adw::ApplicationWindow, seconds: f64) -> String {
    let cut_ = ui::window::newest_review_cut();
    let Some(card) = cut_card_preview::card_under(&cut_, seconds).cloned() else {
        // Footage: say so and change nothing, neither the hand nor the button's word.
        if let Some(status) = ui::window::find_status(window.upcast_ref()) {
            status.set_text(NOTHING);
        }
        return NOTHING.to_string();
    };
    let name = card_name(&card.ins).to_string();
    ui::set_held_clip(Some(card));
    ui::refresh_insert_button(window);
    let said = format!(
        "{name} is held \u{2014} {} now re-opens it",
        cut_insert::edit_verb(true)
    );
    if let Some(status) = ui::window::find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    said
}

thread_local! {
    /// The hold gesture of the band built most recently, for a test; [`wire`] attaches and forgets it.
    static CARD_HOLD_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}

/// The band's card-hold gesture. Test-only reader (as `ui::zoom_gesture`).
pub fn card_hold_gesture_for_test(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    CARD_HOLD_CLICKS.with(|cell| cell.borrow().clone())
}

/// Attach the hold to a painted band. Every button, because [`holds_card`] decides which presses count;
/// the line-placing gesture stays `set_button(1)`.
pub fn wire(window: &adw::ApplicationWindow, surface: &gtk::DrawingArea) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(0);
    let win = window.clone();
    gesture.connect_released(move |_g, n_press, x, _y| {
        // The band is one column of pictures, so only the x carries a second.
        // The button is read off the gesture that fired (`current_button`), never recomputed.
        if !holds_card(_g.current_button(), n_press.max(0) as u32) {
            return;
        }
        let seconds = second_at(&win, x);
        press(&win, seconds);
    });
    surface.add_controller(gesture.clone());
    CARD_HOLD_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// Which second a press x is. Test-only reader of the private [`second_at`] (as `window::band_line_gesture_for_test`).
pub fn seconds_at_for_test(window: &adw::ApplicationWindow, x_px: f64) -> f64 {
    second_at(window, x_px)
}

/// The span a scene draws its bars through. Test-only reader: a check presses at the x the ink was painted at.
pub fn span_of_for_test(window: &adw::ApplicationWindow) -> timeline::Span {
    cut_tracks::band_span(&cut_view::band_scene_for(window))
}
