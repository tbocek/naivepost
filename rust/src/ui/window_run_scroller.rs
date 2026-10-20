//! §03-shell.md §1: the bottom row must not be the thing that holds the window wide.
//!
//! §1 puts the run bar and the Inputs/Outputs readouts in ONE row, and "what no longer fits scrolls …
//! instead of holding the window wide". The run bar's row carries ▶ / 🎲 / ⏹ / ⚙ plus the three pass
//! buttons with words on them, so its natural minimum is ~800 px; add the readouts' own 378 px and the
//! row pins the shell wider than §1's 1240 px minimum (`tests/suite/window_shrinks.rs`
//! measures exactly that). So the controls scroll sideways inside the row while the readouts keep their
//! place at its right end — the row's minimum becomes the scroller (0) plus the readouts, not the sum.
//!
//! Split out of `window.rs`, which is at its size budget.

use adw::prelude::*;
use gtk4 as gtk;

/// The run bar's controls, wrapped in a horizontal scroller that never asks for width of its own.
///
/// `hscrollbar_policy = External` rather than `Automatic`: a scrollbar drawn along the bottom of a
/// 34 px-tall row would be a second strip nobody asked for, and GTK still lets the pointer wheel or
/// drag move the content inside. `vexpand(false)` keeps the row its own height.
///
/// The controls keep their own named box (`run-controls`) INSIDE this scroller: every door and test that
/// finds ▶ / 🎲 / ⏹ by name walks the tree, so they are reached through the scroller unchanged. The
/// The scroller is what carries the name `run-controls`: the row's direct children stay exactly
/// [scroller, progress bar, readouts], which is the order §1 draws. The box it holds keeps
/// `run-controls-inner`, so a door or test that wants ▶ / 🎲 / ⏹ descends into the scroller by that name.
pub fn wrap_controls(controls: gtk::Widget) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_widget_name("run-controls");
    scroller.set_hscrollbar_policy(gtk::PolicyType::External);
    scroller.set_vscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&controls));
    scroller.set_hexpand(true);
    scroller.set_vexpand(false);
    scroller
}
