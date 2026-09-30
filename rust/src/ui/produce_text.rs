//! §08-produce#1-screen items **4**, **5**, **8**, **9** — the four boxes a person TYPES into: Edit
//! instruction, Negative prompt, Title and YouTube description.
//!
//! Split out of `produce_page`, which is at its size budget. The page already draws these widgets and
//! paints the held state INTO them (`refresh`); what lives here is the other half of the wire — reading
//! what was typed back out and handing it to the page's mutator, so the flows see the words rather than
//! a stale copy. No rule lives here: no trimming, no case change, no length bound. §1 gives none, and an
//! instruction or a description is prose whose spacing belongs to whoever wrote it.
//!
//! WHY the title is stored like the rest: it is the only one of the four whose printed-on-the-thumbnail
//! life is separate — §1 prints it across the thumbnail the FIRST time only, after which title and
//! picture are independent. Storing every keystroke is what lets `title_seeded` be the thing that decides
//! whether a retitle repaints the picture, instead of the page simply never having known the title changed.

use adw::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

/// Reads the page's repaint guard: a programmatic write during `refresh` emits the same signal a keystroke
/// does, and must not come back as an edit (the `SETTINGS_GUARD` rule the settings rows already follow).
type Guard = Rc<dyn Fn() -> bool>;
/// Records one field: `(widget name, text)`. The widget's own name is the field name, so this module needs
/// no vocabulary of its own and cannot drift from what the page drew.
type Changed = Rc<dyn Fn(&str, &str)>;

/// Attach the four typing wires. Each handler is three steps: bail when the guard says this emission is
/// the page's own repaint, read what the widget now holds, hand `(name, text)` to the mutator.
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
