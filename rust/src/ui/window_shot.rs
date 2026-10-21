//! §03-shell.md §1's bottom band — the run-bar row and the log expander under it, measured where they are
//! actually DRAWN, and the one rule that decides how tall that band is.
//!
//! §1 puts eight badges under the page (**10**-**18**: ▶ run/pause, "I'm feeling lucky", ⏹, the progress
//! bar, then in the same row the `Inputs:` readout, the Outputs folder button and the `Outputs:` count,
//! and UNDER that row the log expander carrying the status line at its right end) and says the whole thing
//! twice over: while the log is collapsed "the bottom half is exactly as tall as the run bar row plus the
//! log expander's header, with no empty space under them: the page takes all the rest of the height".
//! A name lookup can never catch a window that fails that sentence — every widget is still in the tree with
//! a zero box — so this module owns both halves of the fix: [`height_px`] answers how tall the band has to
//! be (which `window_divider` asks for the divider's resting position), while [`bounds_in_window_for_test`]
//! and [`is_on_screen_for_test`] report what room a control was actually given. Those two are read only by
//! the shell's screen-coverage tests (`tests/suite/sec_03_shell_1_the_window`); no UI handler, flow or
//! `main` ever asks them, which is what the `_for_test` suffix says.
//!
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

/// The band's resting height: the run-bar row plus the log expander's header, nothing else.
///
/// This is §1's "exactly as tall as … with no empty space under them" turned into the number the divider is
/// started at, so the page gets `window − header − band` instead of a remembered constant that drifts the
/// moment a control grows. Minimum sizes, not allocations: the caller asks before anything is laid out, and
/// a minimum is what GTK will honour whatever the window's own height turns out to be.
pub fn height_px(run_row: &gtk::Widget, log: &gtk::Expander) -> i32 {
    let row = run_row.measure(gtk::Orientation::Vertical, -1).0;
    // An expander reports its OPEN height through `measure`; the collapsed band wants only its header, and
    // GTK exposes that as the label widget's own height (that is literally what §1 counts).
    let header = log
        .label_widget()
        .map(|label| label.measure(gtk::Orientation::Vertical, -1).0)
        .unwrap_or_default();
    row.max(header).max(1) + header
}

/// Start the divider so the band is exactly [`height_px`] tall: the handle goes to `total − band`, leaving
/// the page everything above it. Clamped inside the paned because a tiny window (a test that allocates a
/// postage stamp) has no room for both halves.
pub fn rest_position(total: i32, band: i32) -> i32 {
    (total - band).clamp(0, total)
}

/// Where a widget is drawn, in the window's own coordinates: `(x, y, width, height)`.
///
/// `compute_bounds` walks the widget's surface transform back to the window, so these are the numbers a
/// screenshot shows — not `width()`/`height()`, which are relative to the parent and say nothing about
/// whether the thing is on screen. `None` when the widget has no window yet (never mapped), which is a
/// different fact from "mapped with an empty box" and worth telling apart in a failure message.
///
/// `_for_test` because asking it needs a laid-out window, and the app itself never does: no handler, flow or
/// `main` path reads where its own controls landed. The question belongs to §1's "at the minimum size every
/// control a page names is on screen at once", which only a test over a realized window can answer.
pub fn bounds_in_window_for_test(window: &gtk::Widget, named: &str) -> Option<(f64, f64, f64, f64)> {
    let widget = find_named(window, named)?;
    let rect = widget.compute_bounds(window)?;
    Some((rect.x() as f64, rect.y() as f64, rect.width() as f64, rect.height() as f64))
}

/// Is `named` drawn at all — non-empty box, top-left inside the window? This is what "on screen at once"
/// means for a control: not that it exists in the tree, but that it got pixels. Test-read for the same
/// reason as [`bounds_in_window_for_test`]: a running app has no use for the answer.
pub fn is_on_screen_for_test(window: &gtk::Widget, named: &str) -> bool {
    let Some((x, y, w, h)) = bounds_in_window_for_test(window, named) else { return false };
    let (win_w, win_h) = (window.width() as f64, window.height() as f64);
    w > 0.0 && h > 0.0 && x >= -0.5 && y >= -0.5 && x < win_w && y < win_h
}

/// Walk the tree for a widget by the name its builder gave it — the same depth-first walk every door in
/// `window_doors` uses, kept here because this module is what the bottom-band tests read through.
fn find_named(window: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if window.widget_name() == name {
        return Some(window.clone());
    }
    for child in window.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
        if let Some(found) = child.downcast_ref::<gtk::Widget>().and_then(|w| find_named(w, name)) {
            return Some(found);
        }
    }
    None
}
