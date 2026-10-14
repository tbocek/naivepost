//! §07-narrate#1-screen **7** — the voice picker's own half of the Narrate page: the rows it shows,
//! the row it shows, and what a pick does. Split out of `narrate_page` because a dropdown's three doors
//! (build, repaint, press) each need the same seeding rule, and that rule is the whole reason this file
//! exists: GTK has no "no selection", so every programmatic move of the row looks exactly like a hand
//! on the dropdown unless something says otherwise.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_screen::{self, VoiceOption};

thread_local! {
    /// A dropdown row the PAGE selected itself is not a hand picking a voice. `refresh` seeds the picker
    /// from the published state, and that selection would otherwise reach the pick seam — which writes
    /// `narrate/voice.txt` and drops the built voice reference — on every repaint.
    static PICKER_SEEDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The option list behind the picker's rows, in the order they are drawn: resolved through
/// [`narrate_screen::voice_options`] so the wire and the paint can never disagree about which row is
/// which voice.
fn options(narrators: usize, files: &[String]) -> Vec<VoiceOption> {
    narrate_screen::voice_options(narrators, &files.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Show `row` in the voice picker as the page's own choice, not the person's: the flag keeps the
/// `selected-notify` wire from reading this seed as a pick.
pub(crate) fn seed(picker: &gtk::DropDown, row: u32) {
    PICKER_SEEDING.with(|seeding| seeding.set(true));
    picker.set_selected(row);
    PICKER_SEEDING.with(|seeding| seeding.set(false));
}

/// Is a `selected-notify` emission the page's own seed rather than a hand?
fn seeding() -> bool {
    PICKER_SEEDING.with(|seeding| seeding.get())
}

/// The picker at rest: two placeholder rows so the widget has a face before the first refresh, seeded
/// to the chosen voice's row when the published state already names one.
pub(crate) fn build() -> gtk::DropDown {
    let picker = gtk::DropDown::from_strings(&[
        "No audio \u{2014} captions only",
        "Narrator 1 \u{2014} cut from the recording",
    ]);
    picker.set_widget_name("voice-picker");
    picker.set_tooltip_text(Some(&narrate_screen::sample_tip()));
    // The row shown is the chosen voice's, spelled by `voice_options` rather than hardcoded: the spec's
    // shot has the picker carrying `Narrator 1 — 2026-09-16 18-43-01.mkv` as its own face. A number
    // outside the list leaves the dropdown on its first row, so this cannot crash on an empty state.
    if let Some(row) = options(1, &[])
        .iter()
        .position(|option| option.id == crate::ui::narrate_page::read_state().voice)
    {
        seed(&picker, row as u32);
    }
    picker
}

/// Follow the published voice: rebuild the rows when the folder or the narrator count changed, then put
/// the chosen voice's row on show. Rebuilt whole because "Add file…" adds a row and a fixed two-row list
/// would never show it — the same reason the lines grid is rebuilt rather than patched.
pub(crate) fn follow(
    window: &adw::ApplicationWindow,
    narrators: usize,
    files: &[String],
    voice: &str,
) {
    let Some(picker) = crate::ui::narrate_page::widget_in(window, "voice-picker")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
    else {
        return;
    };
    let options = options(narrators, files);
    let labels: Vec<String> = options.iter().map(|option| option.label.clone()).collect();
    // Only a voice that is NOT on the list counts as "nothing chosen": row 0 of the list IS
    // captions-only, so a page whose state carries no voice at all must leave the dropdown with
    // nothing selected rather than showing "captions only" and speaking a different one.
    let chosen = options.iter().position(|option| option.id == voice);
    // The rows are only replaced when they actually differ: `set_model` resets the dropdown's own
    // selection, and a refresh that changes nothing must not silently move the shown row.
    let same = picker.model().is_some_and(|model| {
        let Some(list) = model.downcast::<gtk::StringList>().ok() else {
            return false;
        };
        list.n_items() as usize == labels.len()
            && (0..list.n_items()).all(|i| {
                list.item(i)
                    .and_then(|item| item.downcast::<gtk::StringObject>().ok())
                    .is_some_and(|item| item.string() == labels[i as usize])
            })
    });
    if !same {
        let strings = gtk::StringList::new(&[]);
        for label in &labels {
            strings.append(label);
        }
        // Swapping the model moves the dropdown's row by itself, and that emission lands after this
        // call returns — too late for a flag set around it. So the flag stays up while the model is
        // replaced and is lowered only once the shown row has been re-seeded to the published voice.
        PICKER_SEEDING.with(|seeding| seeding.set(true));
        picker.set_model(Some(&strings));
        match chosen {
            Some(row) => picker.set_selected(row as u32),
            // Nothing on the list matches: show an empty row rather than a wrong voice. A dropdown
            // has no "no selection", so the first row is shown only when the voice really is the
            // first row; otherwise the page's own refusal is what tells the person.
            None => picker.set_selected(0),
        }
        PICKER_SEEDING.with(|seeding| seeding.set(false));
    } else if let Some(row) = chosen {
        seed(&picker, row as u32);
    }
}

/// Attach the pick. The picker answers by row, not by id: the row is resolved through the same option
/// list the page drew, so a wire cannot pick an id that was never offered.
pub(crate) fn wire(window: &adw::ApplicationWindow) {
    let Some(picker) = crate::ui::narrate_page::widget_in(window, "voice-picker")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
    else {
        return;
    };
    let win = window.clone();
    picker.connect_selected_notify(move |picker| {
        // A selection the page made while painting itself is not a hand on the dropdown: pressing the
        // voice seam for it would write the voice file and drop the reference F4.6 already built.
        // `set_model` also lands here AFTER this handler returns, so the flag is still up when GTK
        // settles the row: re-seed from the published state and let this emission go.
        if seeding() {
            seed(picker, picker.selected());
            return;
        }
        let s = crate::ui::narrate_page::read_state();
        let row = picker.selected() as usize;
        if let Some(option) = options(s.narrators, &s.voice_files).get(row) {
            let id = option.id.clone();
            if id != s.voice {
                crate::ui::narrate_page::press_voice(&win, &id);
            }
        }
    });
}
