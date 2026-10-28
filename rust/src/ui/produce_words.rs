//! §08-produce#1-screen item **7** — the ✎ dialog of the thumbnail's text-overlay editor.
//!
//! Thin by design: it builds the box, prefills it, and forwards to the page seams that own the rules
//! (`produce_page::press_words_reword` / `press_words_remove` / `press_words_place`, which in turn
//! call `produce_screen::{set_words, remove_words, place_words}`). No rule is duplicated here, so a
//! reworded box and a freshly placed one cannot drift apart.
//!
//! WHY no modal `run()`: calling `gtk::Dialog::run()` inside a widget callback nests a second main
//! loop, and this codebase answers dialogs by wiring their buttons to seams instead (the way
//! `new_project_confirm` presents and its caller closes). Each button below calls its seam and closes
//! the window, so the answer travels through the same path a test drives.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::produce_page;

/// The dialog's title, spelled as `spec/inventory/produce.md` §A gives it. Public because §A states the
/// shape a test pins (heading, explanation, box, Remove/Cancel/Save) through the real dialog: the check
/// reads this constant rather than retyping the wording and drifting from it.
pub const DIALOG_TITLE: &str = "Words on the thumbnail";

// The dialog this module has open, if any. An `adw::MessageDialog` is its OWN top-level window rather
// than a child of the page, so it is not reachable by walking the page's content tree; holding it
// here is how a caller finds the box it just opened, through [`open_dialog`].
thread_local! {
    static OPEN_DIALOG: std::cell::RefCell<Option<adw::MessageDialog>> =
        const { std::cell::RefCell::new(None) };
}

/// The dialog currently open, if any.
pub fn open_dialog() -> Option<adw::MessageDialog> {
    OPEN_DIALOG.with(|held| held.borrow().clone())
}

/// Record (or clear) the open dialog. Called by the two openers.
fn remember(dialog: Option<&adw::MessageDialog>) {
    OPEN_DIALOG.with(|held| *held.borrow_mut() = dialog.cloned());
}

/// A small dialog over the Produce page: the words box, the buttons this box needs, nothing else.
/// Built as an `adw::MessageDialog` because that is the dialog shape this app already uses and whose
/// response ids stay readable from a test; `with_remove` is false for a NEW box (§A: the Remove button
/// belongs to a box that already exists, not to one being drawn for the first time).
fn build(parent: &adw::ApplicationWindow, current: &str, with_remove: bool) -> adw::MessageDialog {
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some(DIALOG_TITLE),
        Some(crate::produce_details::TEXT_CHIP_HINT),
    );
    dialog.set_widget_name("words-dialog");
    // × answers Cancel, the same convention `new_project_confirm` sets: closing without saying is not a save.
    dialog.set_property("close-response", "cancel");
    // The order the spec's picture shows: Remove first (it is the destructive answer to a box that exists),
    // then Cancel, then Save as the suggested action on the right.
    if with_remove {
        dialog.add_response("remove", "Remove");
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    }
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("save", "Save");
    dialog.set_default_response(Some("save"));

    // The words themselves go in the body, prefilled: a reword dialog that opened blank would lose the
    // words it was opened to change. Multi-line on purpose — §A's sentence says "Enter starts a new line",
    // and the box is sized for about three lines so a two-line title fits without scrolling.
    let view = gtk::TextView::new();
    crate::ui::margins::text_box(&view);
    view.set_widget_name("words-dialog-entry");
    view.set_wrap_mode(gtk::WrapMode::Word);
    view.buffer().set_text(current);
    let box_height = 3 * LINE_HEIGHT;
    view.set_size_request(-1, box_height);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&view));
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_min_content_height(box_height);
    // The frame is what gives the box its visible edge in the spec's picture; without it a `TextView`
    // inside a dialog body reads as loose text on the background.
    let frame = gtk::Frame::new(None);
    frame.set_child(Some(&scroller));
    frame.set_hexpand(true);
    dialog.set_extra_child(Some(&frame));
    dialog
}

/// Rough height of one line in the dialog's font, so the editor asks for about three of them.
const LINE_HEIGHT: i32 = 24;

/// Read the words back out of the dialog before its response handler runs anything away. The editor is
/// named on the `TextView`, so the walk finds the view through the scroller that holds it.
fn entry_of(dialog: &adw::MessageDialog) -> String {
    fn walk(node: &gtk::Widget) -> Option<String> {
        if node.widget_name() == "words-dialog-entry" {
            return node.downcast_ref::<gtk::TextView>().map(|view| {
                let buffer = view.buffer();
                let (start, end) = buffer.bounds();
                buffer.text(&start, &end, true).to_string()
            });
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(dialog.upcast_ref()).unwrap_or_default()
}

/// ✎ on an existing box: reword it, or take it away. Emptying the box and saving removes it — that
/// rule lives in `press_words_reword`, not here.
pub fn open_reword(parent: &adw::ApplicationWindow, index: usize, current: &str) {
    let dialog = build(parent, current, true);
    remember(Some(&dialog));
    let w = parent.clone();
    dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
        let text = entry_of(dialog);
        match response {
            "save" => {
                produce_page::press_words_reword(&w, index, &text);
            }
            "remove" => {
                produce_page::press_words_remove(&w, index);
            }
            // Cancel and × both leave the record alone.
            _ => (),
        }
        remember(None);
        dialog.close();
    });
    dialog.present();
}

/// A box dragged over the picture: ask for the words that fill it. The geometry arrives from the gesture
/// as fractions of the plate and goes straight to `press_words_place`, which owns how a mark is stored.
pub fn open_place(parent: &adw::ApplicationWindow, cx: f64, cy: f64, w: f64, h: f64) {
    let dialog = build(parent, "", false);
    remember(Some(&dialog));
    let win = parent.clone();
    dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
        if response == "save" {
            let text = entry_of(dialog);
            produce_page::press_words_place(&win, cx, cy, w, h, &text);
        }
        remember(None);
        dialog.close();
    });
    dialog.present();
}

/// ✎ on the title band: the words printed ON THE PICTURE, which start as the video's title and are the
/// picture's own from then on. Rewording them leaves the video's title alone; Remove takes the line off the
/// picture and it stays off (the prototype's `editTitle`).
pub fn open_title(parent: &adw::ApplicationWindow, current: &str) {
    let dialog = build(parent, current, true);
    remember(Some(&dialog));
    let w = parent.clone();
    dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
        match response {
            "save" => {
                produce_page::press_title_words(&w, &entry_of(dialog));
            }
            "remove" => {
                produce_page::press_title_words(&w, "");
            }
            _ => (),
        }
        remember(None);
        dialog.close();
    });
    dialog.present();
}
