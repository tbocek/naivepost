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
    /// The last voice this thread's page was told about, and whether saying it was missing has already
    /// been done. [`follow`] runs on EVERY repaint (a seek, a tick, a row select), so without this the
    /// refusal for a voice nobody can fix by waiting would print itself again each time and drown out
    /// whatever else just happened on the status line.
    static LAST_SPOKEN_VOICE: std::cell::RefCell<Option<(String, bool)>> =
        const { std::cell::RefCell::new(None) };
}

/// The option list behind the picker's rows, in the order they are drawn: resolved through
/// [`narrate_screen::voice_options`] so the wire and the paint can never disagree about which row is
/// which voice.
fn options(narrators: usize, files: &[String]) -> Vec<VoiceOption> {
    narrate_screen::voice_options(
        narrators,
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
    )
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

/// The picker at rest: no rows yet — `follow` fills them from the published voice on the first refresh,
/// so an untouched dropdown shows nothing rather than a speaker nobody chose.
pub(crate) fn build() -> gtk::DropDown {
    let picker = gtk::DropDown::from_strings(&[]);
    picker.set_widget_name("voice-picker");
    picker.set_tooltip_text(Some(&narrate_screen::sample_tip()));
    // The row shown is the chosen voice's, spelled by [`crate::narrate_rules::picker_row`] rather than
    // hardcoded: the spec's shot has the picker carrying `Narrator 1 — 2026-09-16 18-43-01.mkv` as its
    // own face. A voice that is not on the list is not shown at all: the dropdown carries no row (§5's
    // "leaves the picker empty"), and `follow` is what says why.
    match crate::narrate_rules::picker_row(
        &options(1, &[]),
        &crate::ui::narrate_page::read_state().voice,
    ) {
        Some(row) => seed(&picker, row as u32),
        None => picker.set_selected(gtk::INVALID_LIST_POSITION),
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
    // The row to show comes from [`crate::narrate_rules::picker_row`], the same door the logic tests
    // read: it is what makes a project that stores the legacy `own` land on the Narrator 1 row instead of
    // showing nothing for a voice that IS on offer. A voice that is NOT on the list counts as "nothing
    // chosen" — row 0 of the list IS captions-only, so a page whose state carries no voice at all must
    // leave the dropdown empty rather than show "captions only" and speak a different one.
    let chosen = crate::narrate_rules::picker_row(&options, voice);
    // §5's other half at the widget: a voice that is not on offer leaves the picker EMPTY. GTK clamps any
    // position to the model's rows, so a dropdown with rows cannot hold "nothing chosen" — the only thing
    // that spells it is an empty list, which also means nothing can be picked while the stored voice is
    // missing (row 0 would read "no audio" while the page still speaks the vanished voice).
    let offered: Vec<String> = match chosen {
        Some(_) => options.iter().map(|option| option.label.clone()).collect(),
        None => vec![],
    };
    // The rows are only replaced when they actually differ: `set_model` resets the dropdown's own
    // selection, and a refresh that changes nothing must not silently move the shown row.
    let same = picker.model().is_some_and(|model| {
        let Some(list) = model.downcast::<gtk::StringList>().ok() else {
            return false;
        };
        list.n_items() as usize == offered.len()
            && (0..list.n_items()).all(|i| {
                list.item(i)
                    .and_then(|item| item.downcast::<gtk::StringObject>().ok())
                    .is_some_and(|item| item.string() == offered[i as usize])
            })
    });
    if !same {
        let strings = gtk::StringList::new(&[]);
        for label in &offered {
            strings.append(label);
        }
        // Swapping the model moves the dropdown's row by itself, and that emission lands after this
        // call returns — too late for a flag set around it. So the flag stays up while the model is
        // replaced and is lowered only once the shown row has been re-seeded to the published voice.
        // Replacing the model re-selects row 0 by itself (§F4.6 S1 already had to re-seed over that), so an
        // unmet voice gets no rows at all and its button reads the placeholder.
        PICKER_SEEDING.with(|seeding| seeding.set(true));
        picker.set_model(Some(&strings));
        if let Some(row) = chosen {
            picker.set_selected(row as u32);
        }
        PICKER_SEEDING.with(|seeding| seeding.set(false));
    } else if let Some(row) = chosen {
        seed(&picker, row as u32);
    }
    say_why_missing(window, voice, chosen);
}

/// §F4.6 S1: when the voice this project speaks in is not on offer, the picker stays empty AND the status
/// line says which of the two ways it happened, because they are fixed in different places — a narrator
/// slot is tagged on Prepare, a file lives in the voices folder. The sentence is the rules module's,
/// word for word ([`crate::narrate_rules::missing_voice`]); the wire adds nothing.
///
/// Once per voice, not once per repaint: `refresh` runs on every seek and tick, and a refusal nobody can
/// clear by waiting would otherwise push every other status line off the row. A voice that IS on offer
/// clears the memory, so coming back to a missing one later tells its reason again — which is also what
/// keeps a page whose state carries no voice at all quiet: "" names no slot and matches no file, so it is
/// said once and never repeated while nothing has been picked.
fn say_why_missing(window: &adw::ApplicationWindow, voice: &str, chosen: Option<usize>) {
    // "no voice picked yet" is not a missing voice: an empty id is what a page that has never been given a
    // voice holds, and saying `voice "" is no longer in DIR` on every page open would shout over whatever
    // the arrival at this step actually reported. The picker stays empty for it all the same.
    if voice.is_empty() {
        return;
    }
    if chosen.is_some() {
        LAST_SPOKEN_VOICE.with(|told| *told.borrow_mut() = None);
        return;
    }
    let told = LAST_SPOKEN_VOICE.with(|told| told.borrow().clone());
    if told.is_some_and(|(told, _)| told == voice) {
        return;
    }
    LAST_SPOKEN_VOICE.with(|told| *told.borrow_mut() = Some((voice.to_string(), true)));
    crate::ui::narrate_page::say(
        window,
        &crate::narrate_rules::missing_voice(voice, &crate::ui::narrate_page::voices_dir_label()),
    );
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
        // A dropdown with no row selected IS the answer §5 wants for a voice that is not on offer, so a
        // negative selection is nobody picked anything: pressing the seam with the captions row would
        // silently switch the page to "no audio" the moment the picker was cleared.
        if picker.selected() == gtk::INVALID_LIST_POSITION {
            return;
        }
        let row = picker.selected() as usize;
        let offered = options(s.narrators, &s.voice_files);
        // Compared as ROWS, not ids: the stored "own" voice is narrator 1's row, so a row GTK settles on by
        // itself after the page loaded "own" is the same voice and no pick (an id compare pressed the seam
        // and said "voice: narrator 1's" over a page nobody had touched).
        if crate::narrate_rules::picker_row(&offered, &s.voice) == Some(row) {
            return;
        }
        if let Some(option) = offered.get(row) {
            crate::ui::narrate_page::press_voice(&win, &option.id.clone());
        }
    });
}
