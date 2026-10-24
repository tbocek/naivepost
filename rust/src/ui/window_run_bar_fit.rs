//! §03-shell.md §1's run-bar row, made to FIT at the window's minimum size.
//!
//! §1 counts four badges in that row — **10** ▶, **11** "I'm feeling lucky", **12** ⏹, **13** the progress
//! bar — and then the readouts at its right end, and says that at 1240×900 "every control a page names is on
//! screen at once". Nothing else belongs in the row: F0.7 has no policy form for a gear to open (badge 6's ⓘ
//! is a tooltip) and F3.9-F3.11 are jobs of the run after the cut, so `run-controls-inner` holds exactly ▶ / 🎲 / ⏹.
//!
//! What is left of fitting is one number, and where that number is ASKED. A `gtk::ScrolledWindow` measures
//! MINIMUM 0 whatever it holds, so GTK happily allocates `run-controls` less than its own buttons and scrolls
//! the rest away unseen: measured at §1's minimum size it was given 209 px while ▶ / 🎲 / ⏹ need 224, badge **12**
//! was drawn under the progress bar, absent from `shots/03-window.png`, and still found by name — which is why
//! every tree-based test passed. The ask is therefore made on the ALLOCATION path (`window_run_scroller::
//! share_the_free_width`), never as a `set_min_content_width` here: a floor written before anything is allocated
//! travels into the SHELL's own minimum and moved it from §1's 1240 to 1311 once the row held only the badges.
//!
//! Split out of `window.rs`, which is at its size budget.

use gtk4 as gtk;

/// Keep §1's badges on screen: the row's parts are sized in `window_run_scroller`, and this door exists so
/// the caller's order (fit, then hand the free width to badge 13) stays in one place.
///
/// Nothing is asked of the controls scroller here on purpose. Its contents' minimum IS used — as the number
/// `share_the_free_width` subtracts when it sizes badge 13 — but writing it as a `set_min_content_width` here
/// was measured to move the SHELL's own minimum from §1's 1240 to 1311: this call runs while the row has no
/// allocation yet, and a floor set before the first measure travels all the way up through the divider into the
/// window. Badge 12 stays visible because the BAR's ask is matched to the leftovers, not because the scroller
/// refuses to shrink.
pub fn fit(_row: &gtk::Box, _controls: &gtk::Box, _readout_row: &gtk::Widget) {}

/// Badge **13**: the floor the progress bar keeps even when everything else in the row has been served —
/// "the progress bar (taking the row's free width)" with no room at all would be an invisible bar. Used by
/// [`super::window_run_scroller::share_the_free_width`].
pub const PROGRESS_MIN_PX: i32 = 120;
