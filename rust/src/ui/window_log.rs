//! The shell's log expander: the two rules that decide what it CLAIMS, kept out of `window.rs`.
//!
//! Nothing here chooses a height — §10's open/closed numbers and the divider's split live elsewhere. This is
//! only about which widgets an expander charges to its parent box.

use adw::prelude::*;
use gtk4 as gtk;

/// Keep a collapsed expander from being charged its undrawn body.
///
/// GTK keeps an expander's child in its allocation whatever the toggle says, so a closed log measured ~240 px
/// tall and pushed the run-bar row onto the window's last pixel, while §1 wants the collapsed band "exactly as
/// tall as the run bar row plus the log expander's header".
///
/// THE BODY IS HIDDEN, NOT UNPARENTED. Detaching it (`unparent()`) made the widget unreachable from the window
/// tree, so every door that finds the log by name (`ui::log_view`, hence `snapshot_face::show_the_log` and every
/// `paint_progress` caller) got `None`, and the shell drew the log CLOSED on top of lines it had just been
/// handed. §1's badge 18 is the log itself and its picture has it OPEN with its lines: collapsed hides the
/// body, it does not remove it. The band's height is therefore what the divider asks for
/// (see [`super::window_shot::rest_px`]), not what the expander happens to measure.
pub fn hide_body_while_collapsed(log: &gtk::Expander, body: &gtk::Widget) {
    log.connect_expanded_notify({
        let open_body = body.clone();
        move |l| {
            // Hidden while collapsed, shown when opened — the same signal a person's click on the header fires.
            open_body.set_visible(l.is_expanded());
        }
    });
    body.set_visible(false);
}

/// Make an `AdwViewStack` measure each page on its own instead of homogeneously.
///
/// §1: "Above the minimum the layout grows with the window and no part has a fixed pixel size", and the page
/// half of `main-divider` is what should absorb that growth. Homogeneous (the default), the stack reports the
/// tallest tab's minimum — the Cut page's ~760 px tape — as the start half's floor even while Prepare is shown,
/// and a paned clamps any position below its start child's minimum: every ask for the page came back as
/// "page = everything" and the run bar row fell off the bottom of the shell shot.
pub fn per_tab_stack(stack: &adw::ViewStack) {
    stack.set_vhomogeneous(false);
    stack.set_vexpand(true);
}
