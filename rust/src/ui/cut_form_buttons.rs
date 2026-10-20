//! F3.8 S5: every effect form ends with the same row — "Apply" and "Cancel", left to right — and under it the
//! note "Kept as you type — ↶ Undo takes the whole edit back."
//!
//! The six forms built that row six times, in three different orders, and one of them (Label's and Volume's
//! picture) carried only a corner ✕ until the row was added. The row is built once here so the NAMES and the
//! LEFT-TO-RIGHT ORDER are one fact: `<kind>-apply-button`, `<kind>-cancel-button`, inside one box named
//! `form-buttons-<kind>` whose children are [Apply, Cancel]. Each form keeps its own click handlers: the row's
//! shape is shared, the doors behind the two buttons are not.

use adw::prelude::*;
use gtk4 as gtk;

/// The two labels §F3.8 S5 spells, in the order it puts them on the row.
pub const FORM_BUTTONS: [&str; 2] = ["Apply", "Cancel"];

/// The row's box name for one form kind (`form-buttons-zoom`). A check finds the buttons through this box
/// rather than by a per-kind name, which is what proves they are ONE row and not two loose buttons.
pub fn form_buttons_name(kind: &str) -> String {
    format!("form-buttons-{kind}")
}

/// Build the shared row. The two buttons keep the per-kind names every existing wire and test already uses, so
/// nothing that presses them by name has to change.
pub fn form_buttons_row(kind: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_widget_name(&form_buttons_name(kind));
    let apply = gtk::Button::with_label(FORM_BUTTONS[0]);
    apply.set_widget_name(&format!("{kind}-apply-button"));
    let cancel = gtk::Button::with_label(FORM_BUTTONS[1]);
    cancel.set_widget_name(&format!("{kind}-cancel-button"));
    // S5 spells the order: Apply first, then Cancel.
    row.append(&apply);
    row.append(&cancel);
    row
}

/// The row's two buttons, in S5's order — `[Apply, Cancel]`. A wire attaches its own doors to them by name;
/// this hands the pair back to a form that wires them from the locals rather than by a tree walk.
pub fn row_buttons(row: &gtk::Box) -> (gtk::Button, gtk::Button) {
    let children = row.observe_children();
    let mut seen = children
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Button>().ok());
    let apply = seen.next().expect("the row is built with Apply first");
    let cancel = seen.next().expect("the row is built with Cancel second");
    (apply, cancel)
}

/// The note S5 puts UNDER the row, as one label with the form's own name.
pub fn form_note(kind: &str) -> gtk::Label {
    let note = gtk::Label::new(Some(crate::fx_band::KEPT_AS_YOU_TYPE));
    note.set_xalign(0.0);
    note.add_css_class("dim-label");
    note.set_widget_name(&format!("{kind}-form-footer"));
    note
}
