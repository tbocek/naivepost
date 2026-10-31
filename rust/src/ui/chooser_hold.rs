//! A `FileChooserNative` lives only as long as someone holds it: dropped after `show()`, GTK's built-in chooser (the one
//! used where no desktop portal answers) is destroyed mid-load and leaves "Operation was cancelled" behind.

#![allow(deprecated)] // the same GtkFileChooser API every chooser in the app keeps

use adw::prelude::*;
use gtk4 as gtk;

thread_local! {
    static HELD: std::cell::RefCell<Vec<gtk::FileChooserNative>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Show `chooser` and hold it until it answers. Connect its own response handler first.
pub fn show(chooser: &gtk::FileChooserNative) {
    HELD.with(|held| held.borrow_mut().push(chooser.clone()));
    chooser.connect_response(|answered, _| {
        HELD.with(|held| held.borrow_mut().retain(|kept| kept != answered));
    });
    chooser.show();
}
