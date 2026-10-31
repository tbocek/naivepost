//! §03-shell.md §1's run-bar row, made to fit — the ONE door `build_window` calls.

use adw::prelude::*;
use gtk4 as gtk;

/// Fit §1's run-bar row: floor the controls scroller, then give badge 13 the row's free width.
pub fn fit_into_row(row: &gtk::Box, controls: &gtk::Box, readout_row: &gtk::Widget) {
    // §1's four badges must be ON SCREEN at the minimum size: the controls scroller gets their own minimum
    // (`window_run_bar_fit`).
    super::window_run_bar_fit::fit(row, controls, readout_row);
    // §1 badge 13: the progress bar alone takes the row's free width.
    super::window_run_scroller::hand_the_free_width_to(row.upcast_ref());
}
