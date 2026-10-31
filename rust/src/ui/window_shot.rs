//! §03-shell.md §1's bottom band — the run-bar row and the log expander under it, measured where they are
//! actually DRAWN, and the one rule that decides how tall that band is.

use adw::prelude::*;
use gtk4 as gtk;

/// The band's resting height: the run-bar row plus the log expander's header, nothing else.
pub fn height_px(run_row: &gtk::Widget, log: &gtk::Expander) -> i32 {
    let row = run_row.measure(gtk::Orientation::Vertical, -1).0;
    // An expander reports its OPEN height through `measure`; the collapsed band wants only its header, and
    // GTK exposes that as the label widget's own height (that is literally what §1 counts).
    let header = log
        .label_widget()
        .map(|label| label.measure(gtk::Orientation::Vertical, -1).0)
        .unwrap_or_default();
    (row + header + crate::ui::margins::BAR_SPACING_PX).max(1)
}

pub fn height_ask(run_row: &gtk::Widget, log: &gtk::Expander) -> i32 {
    if log.is_expanded() {
        return band_px(run_row, log);
    }
    height_px(run_row, log)
}

pub fn band_px(run_row: &gtk::Widget, log: &gtk::Expander) -> i32 {
    let collapsed = height_px(run_row, log);
    if !log.is_expanded() {
        return collapsed;
    }
    // §10's open height is what the log asks for, not what it needs: its scroller's own minimum is the smaller
    // `LOG_MIN_BODY_PX`, so an open log gives way when the page needs the room.
    collapsed + super::window_bench::LOG_OPEN_PX
}

/// THE OPEN LOG'S HEIGHT WHEN NOTHING DRAGS THE DIVIDER — the headless answer to §1's "Expanding the log
/// moves the divider up so the log gets room of its own".
pub fn opened_px(divider: &gtk::Paned) -> Option<i32> {
    let bottom = divider.end_child()?;
    let log = find_in_bottom(&bottom, "log-expander")?
        .downcast::<gtk::Expander>()
        .ok()?;
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
    let body = band_px(&run_row, &log).clamp(1, total - 1);
    bottom.set_size_request(-1, body - crate::ui::margins::BAR_MARGINS_PX);
    let band = body;
    let reposition = |divider: &gtk::Paned| {
        divider.set_position(rest_position(divider.height(), band));
    };
    reposition(divider);
    for _ in 0..3 {
        let context = gtk::glib::MainContext::default();
        for _ in 0..16 {
            while context.pending() {
                context.iteration(false);
            }
        }
        reposition(divider);
    }
    Some(band)
}

/// The first widget named `name` at or below `node`, within the band.
fn find_in_bottom(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    for child in node
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
    {
        if let Some(widget) = child
            .downcast_ref::<gtk::Widget>()
            .and_then(|w| find_in_bottom(w, name))
        {
            return Some(widget);
        }
    }
    None
}

pub fn rest_px(run_row: &gtk::Widget, log: &gtk::Expander) -> i32 {
    height_ask(run_row, log)
}

/// Start the divider so the band is exactly [`height_px`] tall: the handle goes to `total − band`, leaving
/// the page everything above it. Clamped inside the paned because a tiny window (a test that allocates a
/// postage stamp) has no room for both halves.
pub fn rest_position(total: i32, band: i32) -> i32 {
    (total - band).clamp(0, total)
}

/// Where a widget is drawn, in the window's own coordinates: `(x, y, width, height)`.
pub fn bounds_in_window_for_test(
    window: &gtk::Widget,
    named: &str,
) -> Option<(f64, f64, f64, f64)> {
    let widget = find_named(window, named)?;
    let rect = widget.compute_bounds(window)?;
    Some((
        rect.x() as f64,
        rect.y() as f64,
        rect.width() as f64,
        rect.height() as f64,
    ))
}

/// Is `named` drawn at all — non-empty box, top-left inside the window? This is what "on screen at once"
/// means for a control: not that it exists in the tree, but that it got pixels. Test-read for the same
/// reason as [`bounds_in_window_for_test`]: a running app has no use for the answer.
pub fn is_on_screen_for_test(window: &gtk::Widget, named: &str) -> bool {
    let Some((x, y, w, h)) = bounds_in_window_for_test(window, named) else {
        return false;
    };
    let (win_w, win_h) = (window.width() as f64, window.height() as f64);
    w > 0.0 && h > 0.0 && x >= -0.5 && y >= -0.5 && x < win_w && y < win_h
}

/// Walk the tree for a widget by the name its builder gave it — the same depth-first walk every door in
/// `window_doors` uses, kept here because this module is what the bottom-band tests read through.
fn find_named(window: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if window.widget_name() == name {
        return Some(window.clone());
    }
    for child in window
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
    {
        if let Some(found) = child
            .downcast_ref::<gtk::Widget>()
            .and_then(|w| find_named(w, name))
        {
            return Some(found);
        }
    }
    None
}
