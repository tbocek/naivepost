//! §03-shell.md §1's tab row: the four tab words keep their whole label at the window's minimum size.
//!
//! §1 makes 1240×900 the size at which "every control and lane a page names is on screen at once", and
//! the tabs are named controls (Prepare, Cut, Narrate, Produce). An `adw::ViewSwitcher` in Wide policy
//! ellipsizes its button labels when the centre slot runs short — the shot showed "Prep…", "Narr…",
//! "Prod…" — while badge **4** next to it is spec'd to give way by ellipsizing first ("as the window
//! narrows: path + tab words → file name + tab words"). So the switcher asks for what its words need and
//! never trades them for an ellipsis; the project path, already ellipsizing with a one-character
//! minimum (`header_title`), absorbs whatever width is missing.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::window_bench::tab_buttons;

/// Make every label inside this tab row show its whole word.
pub fn keep_labels_whole(switcher: &adw::ViewSwitcher) {
    for button in tab_buttons(switcher.upcast_ref()) {
        for label in labels_of(&button) {
            // No ellipsis, so the button's natural width IS its word; no shrink below that either.
            label.set_ellipsize(gtk::pango::EllipsizeMode::None);
            label.set_max_width_chars(-1);
            label.set_width_chars(-1);
            label.set_hexpand(false);
        }
        // A button that shrinks below its own label would clip it anyway.
        button.set_hexpand(false);
    }
}

/// Every label under `root` — a switcher button holds its label inside a box, not directly.
fn labels_of(root: &gtk::Widget) -> Vec<gtk::Label> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
            out.push(label);
        }
        out.extend(labels_of(&widget));
    }
    out
}
