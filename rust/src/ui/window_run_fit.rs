//! §03-shell.md §1's run-bar row, made to fit — the ONE door `build_window` calls.
//!
//! Two rules decide that row and they have an order: first the controls scroller is floored at its buttons'
//! own minimum (`window_run_bar_fit`), then the progress bar is sized from what
//! the row has left (`window_run_scroller`). Kept as one call because the second must see the first's
//! children: `window_divider::install` adds the readouts last, so sizing badge 13 before the fit ran left
//! it sharing the row's slack with siblings that were about to move, and ⏹ came out clipped in the shot.
//!
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

/// Fit §1's run-bar row: floor the controls scroller, then give badge 13 the row's free width.
///
/// `row` is the `run-bar` box, `controls` its `run-controls-inner` child (exactly ▶ / 🎲 / ⏹), `readout_row`
/// the readouts box the divider appends last.
pub fn fit_into_row(row: &gtk::Box, controls: &gtk::Box, readout_row: &gtk::Widget) {
    // §1's four badges must be ON SCREEN at the minimum size: the controls scroller gets their own minimum
    // (`window_run_bar_fit`).
    super::window_run_bar_fit::fit(row, controls, readout_row);
    // §1 badge 13: the progress bar alone takes the row's free width. Last, because `install` and `fit` above
    // are what add the row's last two children — running earlier left them out of the rule.
    super::window_run_scroller::hand_the_free_width_to(row.upcast_ref());
}
