//! §08-produce#1-screen items **4**, **5**, **8**, **9** — the four boxes a person TYPES into: Edit
//! instruction, Negative prompt, Title and YouTube description.

use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

/// Reads the page's repaint guard: a programmatic write during `refresh` emits the same signal a keystroke
/// does, and must not come back as an edit (the `SETTINGS_GUARD` rule the settings rows already follow).
type Guard = Rc<dyn Fn() -> bool>;
/// Records one field: `(widget name, text)`.
type Changed = Rc<dyn Fn(&str, &str)>;

/// Attach the four typing wires.
pub fn wire_text(window: &adw::ApplicationWindow, guarded: Guard, changed: Changed) {
    // The three multi-line boxes (§A: 4 / 2 / 8 lines) are `TextView`s, whose text lives in a buffer and
    // whose `changed` signal comes from THAT buffer, not from the view.
    for name in ["edit-instruction", "negative-prompt", "description-box"] {
        let Some(view) = crate::ui::produce_page::widget_in(window, name)
            .and_then(|w| w.downcast::<gtk::TextView>().ok())
        else {
            continue;
        };
        let buffer = view.buffer();
        let guarded = Rc::clone(&guarded);
        let changed = Rc::clone(&changed);
        let field = name.to_string();
        buffer.connect_changed(move |buffer| {
            if guarded() {
                return;
            }
            let (start, end) = buffer.bounds();
            let said = buffer.text(&start, &end, true);
            changed(&field, &said);
        });
    }
    // The title is a single-line `Entry`: one line, because it is also a picture's caption.
    if let Some(entry) = crate::ui::produce_page::widget_in(window, "title-entry")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
    {
        let guarded = Rc::clone(&guarded);
        let changed = Rc::clone(&changed);
        entry.connect_changed(move |entry| {
            if guarded() {
                return;
            }
            changed("title-entry", &entry.text());
        });
    }
}
