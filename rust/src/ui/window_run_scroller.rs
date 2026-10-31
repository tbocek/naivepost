//! §03-shell.md §1: the bottom row must not be the thing that holds the window wide.

use adw::prelude::*;
use gtk4 as gtk;

/// The run bar's controls, wrapped in a horizontal scroller that never asks for width of its own.
pub fn wrap_controls(controls: gtk::Widget) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_widget_name("run-controls");
    // `Never` makes the buttons this row's floor, so long readouts beside them cannot squeeze them to nothing.
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_vscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&controls));
    // EXPANDS, but only as the filler for whatever else in the row does not: `hand_the_free_width_to` gives
    // §1's free width to badge 13 first and this takes what is left.
    scroller.set_hexpand(true);
    scroller.set_vexpand(false);
    scroller
}

/// Give §1's free width to the progress bar alone: expand `run-progress` AND the controls' scroller (which
/// needs an expanding flag at all, see [`wrap_controls`]), and clear every other child of `row`, INCLUDING
/// their nested boxes.
pub fn hand_the_free_width_to(row: &gtk::Widget) {
    for child in children_of(row) {
        // The job buttons are gone from this row (§2's list is its whole content), so only the bar and the
        // controls' scroller expand; everything else keeps its natural ask.
        let keeps = matches!(
            child.widget_name().as_str(),
            "run-progress" | "run-controls"
        );
        child.set_hexpand(keeps);
        if !keeps {
            clear_below(&child);
        }
        if child.widget_name() == PROGRESS {
            share_the_free_width(row, &child);
        }
    }
}

/// Badge **13**'s width: what the row has LEFT once every other part of it has been served.
pub fn progress_px(row_w: i32, badges: i32, readouts: i32, gaps: i32) -> i32 {
    (row_w - badges - readouts - gaps).max(super::window_run_bar_fit::PROGRESS_MIN_PX)
}

/// The widths GTK actually gave the run-bar row's parts, as `(name, left edge, width)` in the window's own
/// coordinates — how the shell shot and §1's screen tests read badge 13 instead of guessing from the PNG.
pub fn drawn_row_widths(row: &gtk::Widget) -> Vec<(String, f64, f64)> {
    children_of(row)
        .iter()
        .filter(|child| !child.widget_name().is_empty())
        .filter_map(|child| {
            let b = child.compute_bounds(row)?;
            Some((
                child.widget_name().to_string(),
                b.x() as f64,
                b.width() as f64,
            ))
        })
        .collect()
}

/// The same numbers [`share_the_free_width`] charges the row, as a report: `[(name, px)]` — the controls
/// scroller's content floor (`run-controls-inner`) and what that scroller is charged for (its allocation,
/// or that floor when GTK gave it less).
pub fn charged_widths_for_test(row: &gtk::Widget) -> Vec<(String, i32)> {
    let floor_of = |name: &str| {
        find_in(row, name)
            .map(|w| w.measure(gtk::Orientation::Horizontal, -1).0.max(0))
            .unwrap_or_default()
    };
    let charge = |scroller: &str, floor: i32| {
        find_in(row, scroller)
            .map(|w| w.width().max(floor).max(0))
            .unwrap_or(floor)
    };
    let badge_floor = floor_of("run-controls-inner");
    vec![
        ("badges-floor".into(), badge_floor),
        ("run-controls".into(), charge("run-controls", badge_floor)),
        (
            "readouts".into(),
            children_of(row)
                .iter()
                .filter(|child| {
                    !matches!(
                        child.widget_name().as_str(),
                        "run-progress" | "run-controls"
                    )
                })
                .map(|child| child.measure(gtk::Orientation::Horizontal, -1).1.max(0))
                .sum(),
        ),
        ("row-width".into(), row.width()),
    ]
}

/// Size the progress bar from what the row has LEFT, which is what "the row's free width" means (§1 badge 13).
fn share_the_free_width(row: &gtk::Widget, bar: &gtk::Widget) {
    // THE ROW'S OWN WIDTH, whatever it happens to be when this runs — see [`window_run_scroller::hand_the_free_width_to`] for why the same ask is
    // re-made once the divider has been allocated.
    let badges_floor = find_in(row, "run-controls-inner")
        .map(|w| w.measure(gtk::Orientation::Horizontal, -1).0.max(0))
        .unwrap_or_default();
    let controls_w = badges_floor;
    // The readouts' own ask, and the row's gaps: the bar gets whatever is left once every other part of the row
    // has been served.
    let readouts: i32 = children_of(row)
        .iter()
        .filter(|child| {
            !matches!(
                child.widget_name().as_str(),
                "run-progress" | "run-controls"
            )
        })
        .map(|child| child.measure(gtk::Orientation::Horizontal, -1).1.max(0))
        .sum();
    let gaps = 6 * children_of(row).len() as i32;
    if let Some(scroller) = find_in(row, "run-controls") {
        if let Some(scroller) = scroller.downcast_ref::<gtk::ScrolledWindow>() {
            let others = controls_w + readouts + gaps;
            let serves_it = row.width() > 1 && row.width() >= others;
            scroller.set_min_content_width(if serves_it { controls_w.max(0) } else { 0 });
        }
    }
    // Badge 13 keeps only its FLOOR here (`PROGRESS_MIN_PX`).
    bar.set_size_request(super::window_run_bar_fit::PROGRESS_MIN_PX, -1);
}

fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
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
            .and_then(|w| find_in(w, name))
        {
            return Some(widget);
        }
    }
    None
}

fn children_of(row: &gtk::Widget) -> Vec<gtk::Widget> {
    row.downcast_ref::<gtk::Box>()
        .expect("the run bar row is a horizontal box")
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|o| o.downcast_ref::<gtk::Widget>().cloned())
        .collect()
}

/// The row child §1 gives the free width to (badge 13).
const PROGRESS: &str = "run-progress";

fn clear_below(node: &gtk::Widget) {
    node.set_hexpand(false);
    let Some(box_) = node.downcast_ref::<gtk::Box>() else {
        return;
    };
    for child in box_
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|o| o.downcast_ref::<gtk::Widget>().cloned())
    {
        clear_below(&child);
    }
}
