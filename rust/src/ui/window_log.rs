//! The shell's log expander: the two rules that decide what it CLAIMS, kept out of `window.rs`.

use adw::prelude::*;
use gtk4 as gtk;

/// Keep a collapsed expander from being charged its undrawn body.
pub fn hide_body_while_collapsed(log: &gtk::Expander, body: &gtk::Widget) {
    log.connect_expanded_notify({
        let open_body = body.clone();
        move |l| {
            open_body.set_visible(l.is_expanded());
        }
    });
    body.set_visible(false);
}

/// Make an `AdwViewStack` measure each page on its own instead of homogeneously.
pub fn per_tab_stack(stack: &adw::ViewStack) {
    stack.set_vhomogeneous(false);
    stack.set_vexpand(true);
}
