//! §07-narrate#1-screen **7** — the voice picker's own half of the Narrate page: the rows it shows,
//! the row it shows, and what a pick does. Split out of `narrate_page` because a dropdown's three doors
//! (build, repaint, press) each need the same seeding rule, and that rule is the whole reason this file
//! exists: GTK has no "no selection", so every programmatic move of the row looks exactly like a hand
//! on the dropdown unless something says otherwise.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_screen::{self, VoiceOption};
use crate::ui::narrate_page_state::{NarratePage, PAGE as NARRATE_PAGE};

fn options(narrators: usize, files: &[String]) -> Vec<VoiceOption> {
    narrate_screen::voice_options(
        narrators,
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

pub(crate) fn seed(picker: &gtk::DropDown, row: u32) {
    NARRATE_PAGE.with(
        |NarratePage {
             picker_seeding: seeding,
             ..
         }| seeding.set(true),
    );
    picker.set_selected(row);
    NARRATE_PAGE.with(
        |NarratePage {
             picker_seeding: seeding,
             ..
         }| seeding.set(false),
    );
}

/// Is a `selected-notify` emission the page's own seed rather than a hand?
fn seeding() -> bool {
    NARRATE_PAGE.with(
        |NarratePage {
             picker_seeding: seeding,
             ..
         }| seeding.get(),
    )
}

fn row_factory(ellipsize: gtk::pango::EllipsizeMode) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(ellipsize);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_then(|row| row.downcast::<gtk::StringObject>().ok());
        if let (Some(label), Some(text)) = (
            item.child()
                .and_then(|child| child.downcast::<gtk::Label>().ok()),
            text,
        ) {
            label.set_text(&text.string());
        }
    });
    factory
}

pub(crate) fn build() -> gtk::DropDown {
    let picker = gtk::DropDown::from_strings(&[]);
    picker.set_widget_name("voice-picker");
    // The closed face ellipsizes: a long take name must not hold the column past its 560 px. The open list keeps
    // the whole name.
    picker.set_list_factory(Some(&row_factory(gtk::pango::EllipsizeMode::None)));
    picker.set_factory(Some(&row_factory(gtk::pango::EllipsizeMode::End)));
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
        NARRATE_PAGE.with(
            |NarratePage {
                 picker_seeding: seeding,
                 ..
             }| seeding.set(true),
        );
        picker.set_model(Some(&strings));
        if let Some(row) = chosen {
            picker.set_selected(row as u32);
        }
        NARRATE_PAGE.with(
            |NarratePage {
                 picker_seeding: seeding,
                 ..
             }| seeding.set(false),
        );
    } else if let Some(row) = chosen {
        seed(&picker, row as u32);
    }
    say_why_missing(window, voice, chosen);
}

fn say_why_missing(window: &adw::ApplicationWindow, voice: &str, chosen: Option<usize>) {
    if voice.is_empty() {
        return;
    }
    if chosen.is_some() {
        NARRATE_PAGE.with(
            |NarratePage {
                 last_spoken_voice: told,
                 ..
             }| *told.borrow_mut() = None,
        );
        return;
    }
    let told = NARRATE_PAGE.with(
        |NarratePage {
             last_spoken_voice: told,
             ..
         }| told.borrow().clone(),
    );
    if told.is_some_and(|(told, _)| told == voice) {
        return;
    }
    NARRATE_PAGE.with(
        |NarratePage {
             last_spoken_voice: told,
             ..
         }| *told.borrow_mut() = Some((voice.to_string(), true)),
    );
    crate::ui::narrate_page::say(
        window,
        &crate::narrate_rules::missing_voice(voice, &crate::ui::narrate_page::voices_dir_label()),
    );
}

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
