//! §07-narrate#1-screen badge **13** — the pitch control as the spec draws it.
//!
//! `spec/img/07-narrate.png` shows `pitch: ─── 0.0` on the sample line: a label, the slider, and the number
//! the shift currently stands at. A bare `gtk::Scale` carries neither the label nor the value (its tooltip is
//! not on screen), so this module builds the row the picture shows and keeps the readout in step with whatever
//! moved the slider — the drag, or the keyboard step in [`crate::ui::narrate_page::press_pitch`].
//!
//! Its own file because `narrate_page.rs` is past its size budget; the page calls [`build_row`] where it used
//! to build the scale itself and [`show_value`] wherever the state changes.

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::narrate_screen;

/// The labelled row: `pitch:` · the slider · the current shift. `value` is the page's stored shift, so the
/// control and `narrate/pitch.txt` agree from the first paint rather than after the first move.
pub fn build_row(value: f64) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_widget_name("pitch-row");
    row.set_halign(gtk::Align::Start);

    let label = gtk::Label::new(Some("pitch:"));
    label.set_widget_name("pitch-label");
    row.append(&label);

    let pitch = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        narrate_screen::PITCH_MIN_SEMITONES,
        narrate_screen::PITCH_MAX_SEMITONES,
        narrate_screen::PITCH_STEP,
    );
    pitch.set_round_digits(1);
    pitch.set_widget_name("pitch-slider");
    pitch.set_tooltip_text(Some(&narrate_screen::pitch_tip()));
    pitch.set_value(narrate_screen::clamp_pitch(value));
    row.append(&pitch);

    // The face of the same number the slider holds, so the shift is readable at a glance.
    let shown = gtk::Label::new(Some(&narrate_screen::pitch_readout(value)));
    shown.set_widget_name("pitch-value");
    row.append(&shown);

    row
}

/// Print `semitones` beside the slider. Tolerates no window and no row yet: [`crate::ui::narrate_page::press_pitch`]
/// answers before a page exists in a headless run, and a status sentence must not depend on the readout landing.
pub fn show_value(window: &adw::ApplicationWindow, semitones: f64) {
    let Some(found) = crate::ui::narrate_page::widget_in(window, "pitch-value") else {
        return;
    };
    if let Ok(label) = found.downcast::<gtk::Label>() {
        label.set_text(&narrate_screen::pitch_readout(semitones));
    }
}
