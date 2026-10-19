//! §03-shell.md §1: "Page and bottom bar are two halves of a draggable divider, not a fixed strip;
//! collapsing the log expander returns its height to the page."
//!
//! The window's own geometry, split out of `window.rs` (which is past its size budget): this builds the
//! vertical [`gtk::Paned`] whose start child is the page stack and whose end child is the box holding the
//! run bar, the Inputs/Outputs readouts and the log expander. Dragging the handle moves height between
//! the two halves, so the log is a half of the window rather than a strip bolted under it.
//!
//! Both halves shrink (`shrink_start_child` / `shrink_end_child`), which is what keeps the window's floor
//! the pages' own minimum instead of the sum of the two — `tests/suite/window_shrinks.rs` measures that
//! at 1024×640 or less on every tab. The page's own scroller is what stops the top half becoming
//! unreadable when the handle is dragged down.

use adw::prelude::*;
use gtk4 as gtk;

/// The starting split, in pixels of page height. At rest the log is collapsed, so the handle sits where
/// the page keeps its height and the thin bottom row keeps its own; this number only ever decides the
/// STARTING position, never a floor — the user's drag replaces it and nothing reads it back.
const START_PAGE_PX: i32 = 480;

/// Build §1's divider into `into`: the page stack above, the bottom bar (run bar, readouts, log) below.
pub fn install(into: &gtk::Box, page: &gtk::Widget, run_row: &gtk::Widget, readout_row: &gtk::Widget, log: &gtk::Widget) {
    let divider = gtk::Paned::new(gtk::Orientation::Vertical);
    divider.set_widget_name("main-divider");
    divider.set_wide_handle(false);
    divider.set_shrink_start_child(true);
    divider.set_shrink_end_child(true);
    divider.set_start_child(Some(page));
    page.set_vexpand(true);
    let bottom = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom.set_widget_name("bottom-bar");
    bottom.append(run_row);
    bottom.append(readout_row);
    bottom.append(log);
    divider.set_end_child(Some(&bottom));
    into.append(&divider);
    divider.set_position(START_PAGE_PX);
}
