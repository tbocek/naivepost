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
    // ONE sum, not `row.max(header) + header`: §1 counts the run-bar row AND the header, so a header taller
    // than the row used to be charged twice and left a blank strip under the band.
    (row + header).max(1)
}

/// The band's height for the log's CURRENT state: [`height_px`] while the log is collapsed, and that plus the
/// room the log body asks for once it is open. §1's two sentences are one rule read two ways — collapsed the
/// bottom half is "exactly as tall as the run bar row plus the log expander's header", expanded the log has
/// "room of its own" — and the divider's handle has to move between exactly those two numbers.
///
/// The body's ask is read off the scroller ONLY when the expander actually holds it: `window.rs` keeps a
/// collapsed log's body out of the tree altogether (an expander otherwise keeps its child in its allocation and
/// charges the band 240 px of undrawn body), so asking here can only ever see an open log.
pub fn band_px(run_row: &gtk::Widget, log: &gtk::Expander) -> i32 {
    let collapsed = height_px(run_row, log);
    if !log.is_expanded() {
        return collapsed;
    }
    // The body's own ask, read back off the widget that holds it ([`open_body_px`]): the shell sizes the log's
    // scroller with `min_content_height` (see `window.rs`, where §10's prose 220 px goes on), so asking that
    // scroller is how this stays right if that number ever moves.
    collapsed + open_body_px(log)
}

/// THE OPEN LOG'S HEIGHT WHEN NOTHING DRAGS THE DIVIDER — the headless answer to §1's "Expanding the log
/// moves the divider up so the log gets room of its own".
///
/// §1's two sentences assume a person moves the handle, and the shell follows it: `connect_band_rest` watches
/// the expander's `expanded` notify and re-asks the rule. Headless that never fires for a shot —
/// `snapshot_face::show_the_log` expands the expander through `paint_progress`, but the divider's clamp was
/// already computed for the COLLAPSED band, and `gtk::Paned::set_position` clamps to that floor, so the shot
/// printed `band h=571 min=274` inside an 844 px divider: the log's 220 px body plus ~297 px of leftover page,
/// a grey box under the run bar no window shows. This is the same door a person's drag fires, asked once more
/// AFTER the expander was opened: the band is read live, and the handle goes back to `total − band`.
///
/// Returns the band it asked for, or `None` when this window has no shell band (a dialog shot).
pub fn opened_px(divider: &gtk::Paned) -> Option<i32> {
    let bottom = divider.end_child()?;
    let log = find_in_bottom(&bottom, "log-expander")?.downcast::<gtk::Expander>().ok()?;
    // THE COLLAPSED CASE IS THE RESTING CASE — leave it to `rest_split`, which is what the shell already
    // asked and what §1's collapsed sentence counts. Asking here would move a shot that never opened its log.
    if !log.is_expanded() {
        return None;
    }
    let run_row = bottom.first_child()?;
    let total = divider.height();
    if total <= 1 {
        return None;
    }
    let band = band_px(&run_row, &log).clamp(1, total - 1);
    // THE BOTTOM HALF'S FLOOR MOVES WITH THE LOG. `window_divider::install` puts `rest_px` (the collapsed two
    // rows) on the band as its size request, and a `gtk::Paned` cannot be asked to sit below its end child's
    // minimum — measured: asking 274 px into an 844 px divider left the handle at 570 and the band at 571 px,
    // because the clamp had nothing else to give. That floor is a PROGRAM floor: §1 says the collapsed band is
    // exactly its two rows, and the page's own minimum is what stops the handle coming down further, so the
    // request has to be lifted here for the open state and handed back by `connect_band_rest` on the close.
    bottom.set_size_request(-1, band);
    divider.set_position(rest_position(total, band));
    // …and the halves' minimums are re-read only on the next allocation, which headless nothing causes: the
    // divider clamps `set_position` against the end child's CURRENT minimum (still the collapsed 274 px), so
    // the same pair of asks has to be made again after the snapshot's clamp re-allocates the window. Two more
    // asks, same rule; a third is what lands the handle at 844 − 274 = 570 with the page above it.
    for _ in 0..2 {
        divider.set_position(rest_position(total, band));
    }
    Some(band)
}

/// The first widget named `name` at or below `node`, within the band.
fn find_in_bottom(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
        if let Some(widget) = child.downcast_ref::<gtk::Widget>().and_then(|w| find_in_bottom(w, name)) {
            return Some(widget);
        }
    }
    None
}

/// The band's RESTING height — what the divider starts at and re-asks for at every size. §1 names two states
/// and only one of them is a resting position: "The log STARTS collapsed, and while it is collapsed the bottom
/// half is exactly as tall as the run bar row plus the log expander's header, with no empty space under them";
/// OPENING the log is what moves the handle up. So the resting ask is [`height_px`] — the two widgets §1 counts,
/// nothing else — never the expander's own `measure`, which headless GTK answers as though the body were open
/// and thereby asked for ~280 px of band in a window whose page needs every pixel above it.
pub fn rest_px(run_row: &gtk::Widget, log: &gtk::Expander) -> i32 {
    height_px(run_row, log)
}

/// The open log's own height, read back off the widget that holds it: `window.rs` sizes the log's scroller
/// with `min_content_height` (§10's prose "log heights 220/110" — no `P.` id, so the number lives on the
/// widget, `window_bench::LOG_OPEN_PX`). §1's open band is the collapsed two rows PLUS this, which is what
/// [`band_px`] adds to get the height the divider has to leave the band. Walked off the expander rather than
/// passed in because the divider owns neither widget; falls back to §10's number when there is no scroller to
/// ask (a window still being built).
pub fn open_body_px(log: &gtk::Expander) -> i32 {
    log.child()
        .and_then(|child| find_scroller(&child))
        .map(|scroller| scroller.min_content_height())
        .unwrap_or(super::window_bench::LOG_OPEN_PX)
        .max(0)
}

/// The first `gtk::ScrolledWindow` at or below `node` — the log body's own scroller.
fn find_scroller(node: &gtk::Widget) -> Option<gtk::ScrolledWindow> {
    if let Some(scroller) = node.downcast_ref::<gtk::ScrolledWindow>() {
        return Some(scroller.clone());
    }
    for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
        if let Some(found) = child.downcast_ref::<gtk::Widget>().and_then(find_scroller) {
            return Some(found);
        }
    }
    None
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
