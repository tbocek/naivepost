//! §03-shell.md §1's run-bar row, made to FIT at the window's minimum size.

use gtk4 as gtk;

/// Keep §1's badges on screen: the row's parts are sized in `window_run_scroller`, and this door
/// exists so the caller's order (fit, then hand the free width to badge 13) stays in one place.
/// Badge 12 stays visible because the BAR's ask is matched to the leftovers, not because the
/// scroller refuses to shrink.
pub fn fit(_row: &gtk::Box, _controls: &gtk::Box, _readout_row: &gtk::Widget) {}

pub const PROGRESS_MIN_PX: i32 = 120;
