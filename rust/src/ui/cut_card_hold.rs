//! F2.12 S6 (`right-click or double left-click a card → holds it, Insert becomes Edit`) — the press that
//! picks a card UP instead of placing the line.
//!
//! Split out of `window.rs`, which is past its size budget, and kept separate from
//! [`crate::ui::cut_line_press`] on purpose: that gesture is `set_button(1)`, so a right-click never reaches
//! it, and a second left press there runs F2.4 S1b's pick-up reach (held edge, then border, then the whole
//! clip), which is a footage rule. S6's hold needs both buttons and must ignore that reach, so it rides its
//! own `set_button(0)` gesture on the same surface and answers only when [`holds_card`] says the press means
//! a hold.
//!
//! The seconds a pixel is come from [`cut_tracks::band_span`]'s own [`timeline::Span::t_at`], the same cell
//! walk the bars were painted from, so the card the hold picks up is the card under the ink — not the card a
//! second arithmetic placed there.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_insert;
use crate::timeline;
use crate::ui::{self, cut_card_preview, cut_tracks, cut_view};

/// A card's own name, the way every other sentence here names a file: its last path element.
fn card_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The status line's answer when the press landed on footage: the hand is left EXACTLY as it was, because a
/// card nobody aimed at must not throw away a card someone already holds.
pub const NOTHING: &str = "no card there \u{2014} a right-click or a double click holds a card, not the footage";

/// Which presses mean "hold that card": the right button, on its first press, or a left press that is the
/// second or later click of a multi-click.
///
/// A synthetic `released` emit carries `n_press` and nothing else, and the button comes off the gesture
/// itself, so both halves are readable at the handler and no state has to be kept between presses. A
/// double RIGHT click answers true on both releases and therefore holds the same card twice, which is
/// harmless: S6 says a held card makes Insert `Edit`, and saying it twice says the same thing (§05 says
/// nothing else about the right button's click count).
pub fn holds_card(button: u32, clicks: u32) -> bool {
    button == 3 || clicks >= 2
}

/// The band's second under `x_px`, through the span the bars were painted from. Scroll and zoom come from
/// `cut_view`, so a hold aimed after a scroll reads the scrolled tape. `x_px` is the press's own x inside
/// the surface: [`timeline::Span`] already carries `cut_view::gutter_px()` in its own `gutter_px`, so only the
/// scroll has to come off — and it is subtracted HERE rather than at each caller, so every door in this module
/// converts a surface x the same way.
fn second_at(window: &adw::ApplicationWindow, x_px: f64) -> f64 {
    let scene = cut_view::band_scene_for(window);
    cut_tracks::band_span(&scene).t_at(x_px - cut_view::origin())
}

/// The hold itself: aim at a card, and it comes into the hand with `insert-button` re-worded underneath.
/// Returns the sentence it printed, so a test reads what the user read rather than a painted pixel.
pub fn press(window: &adw::ApplicationWindow, seconds: f64) -> String {
    let cut_ = ui::window::newest_review_cut();
    let Some(card) = cut_card_preview::card_under(&cut_, seconds).cloned() else {
        // Footage: say so and change nothing — not the hand, not the button's word.
        if let Some(status) = ui::window::find_status(window.upcast_ref()) {
            status.set_text(NOTHING);
        }
        return NOTHING.to_string();
    };
    let name = card_name(&card.ins).to_string();
    ui::set_held_clip(Some(card));
    ui::refresh_insert_button(window);
    let said = format!("{name} is held \u{2014} {} now re-opens it", cut_insert::edit_verb(true));
    if let Some(status) = ui::window::find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    said
}

thread_local! {
    /// The hold gesture of the band built most recently: a test fires it the toolkit's way. The program never
    /// reads it back — [`wire`] attaches and forgets — so the name says who the only reader is.
    static CARD_HOLD_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}

/// The band's card-hold gesture. Test-only reader, following `ui::zoom_gesture`'s convention: the page
/// attaches the gesture itself and never asks for it back.
pub fn card_hold_gesture_for_test(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    CARD_HOLD_CLICKS.with(|cell| cell.borrow().clone())
}

/// Attach the hold to a painted band. Every button, because the rule below decides which presses mean
/// something; the line-placing gesture stays `set_button(1)` and never sees a right-click.
pub fn wire(window: &adw::ApplicationWindow, surface: &gtk::DrawingArea) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(0);
    let win = window.clone();
    gesture.connect_released(move |_g, n_press, x, _y| {
        // The band is one column of pictures, so only the x carries a second; the y decides nothing here.
        // The button is read off the gesture that fired (`current_button`, which GTK fills from the real
        // button event), never recomputed here.
        if !holds_card(_g.current_button(), n_press.max(0) as u32) {
            return;
        }
        let seconds = second_at(&win, x);
        press(&win, seconds);
    });
    surface.add_controller(gesture.clone());
    CARD_HOLD_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// Which second a press x is. Test-only reader: the program's own conversion is the private [`second_at`],
/// which [`wire`]'s handler calls and never asks back, so a name saying so keeps the program path honest about
/// what calls this (the shape of `window::band_line_gesture_for_test`).
pub fn seconds_at_for_test(window: &adw::ApplicationWindow, x_px: f64) -> f64 {
    second_at(window, x_px)
}

/// The span a scene draws its bars through. Test-only reader, same convention: a check asks the SAME span for
/// `x_of(31.0)` so the x it presses with is the x the ink was painted at; the program never asks.
pub fn span_of_for_test(window: &adw::ApplicationWindow) -> timeline::Span {
    cut_tracks::band_span(&cut_view::band_scene_for(window))
}
