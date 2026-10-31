//! Print where the shell's bottom band landed in a snapshot, so a shot with no run bar can be traced to a
//! number instead of guessed at.
//!
//! §03-shell.md §1 counts eight badges UNDER the page (**10**-**18**) and says the bottom half is "exactly as
//! tall as the run bar row plus the log expander's header ... the page takes all the rest". Content asking
//! for more than §1's 900 px leaves the band nothing: every widget stays in the tree with a zero or
//! off-screen box, the PNG has no bottom bar and the exit code says nothing. Reads bounds and the divider's
//! position and decides no rule.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui;

const BAND: [&str; 4] = ["main-divider", "run-bar", "inputs-readout", "log-expander"];

/// §05-cut's track region, printed ONLY when the page has them. §1 draws the tracks FILLING the pane down to
/// badge **31**, so a blank band has to say which of these did not take the pane's leftover.
const CUT_TRACKS: [&str; 6] = [
    "page-scroll-Cut",
    "cut-page-split",
    "cut-bottom",
    "track-layers",
    "track-scrollbar",
    "camera-rows",
];

/// THE SHELL'S DIVIDER ASKS ITS RESTING SPLIT, the door a person's drag fires, asked for a shot.
///
/// A headless window never drags the divider, and a band whose ask moved keeps the handle where the OLD ask
/// put it (`03-window`: `pos=570` left the band 571 px of 844). The clamp is right only once the window holds
/// the PNG's size, so callers run this AFTER their own size ask and BEFORE the pixels.
pub fn rest_band(window: &adw::ApplicationWindow) {
    if let Some(divider) =
        ui::find_source_widget(window, "main-divider").and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        ui::window_shot::opened_px(&divider);
    }
}

/// The run-bar row's own children, printed as `name x=.. w=..`.
///
/// §1 says all five fit on screen at the minimum size; a shot can hold every one in the tree and still draw
/// ⏹ UNDER the progress bar, so each direct child's numbers are printed. Report only.
fn row_children(row: &gtk::Widget) -> String {
    let mut said = String::new();
    for (name, x, w) in crate::ui::window_run_scroller::drawn_row_widths(row) {
        said.push_str(&format!(", {name} x={x:.0} w={w:.0}"));
    }
    said
}

/// One line describing where the band is. The handle is moved by [`rest_band`], which `snapshot::run` asks
/// just before this (a headless shot left at GTK's default had the page at its 342 px minimum and the bottom
/// box at 501 px of 844).
///
pub fn report(window: &adw::ApplicationWindow) -> Option<String> {
    let found = |name: &str| ui::find_source_widget(window, name);
    // No divider means no shell: a dialog's own layout has nothing to report.
    let paned_handle = found("main-divider")?;
    if let Some(paned) = paned_handle.downcast_ref::<gtk::Paned>() {
        // §1's rule: with the log COLLAPSED the band is its two rows, OPEN the log has room of its own. A log still
        // closed gets the collapsed pair asked here, so a caller that forgot does not print a band floating mid-window.
        if ui::window_shot::opened_px(paned).is_none() {
            crate::ui::window_divider::rest_split(paned);
        }
    }
    let mut line = format!(
        "band | window {}x{}, asked shot {}x{}",
        window.width(),
        window.height(),
        crate::snapshot_face::asked_shot_wh().0,
        crate::snapshot_face::asked_shot_wh().1
    );
    for name in BAND {
        let Some(widget) = found(name) else {
            line.push_str(&format!(", {name} MISSING"));
            continue;
        };
        match widget.compute_bounds(window) {
            Some(b) => line.push_str(&format!(
                ", {name} y={:.0}..{:.0} h={:.0}",
                b.y(),
                b.y() + b.height(),
                b.height()
            )),
            None => line.push_str(&format!(", {name} unmapped")),
        }
    }
    // …and the run-bar row's own children, since "in the tree" is not the claim §1 makes about that row.
    if let Some(row) = found("run-bar") {
        line.push_str(&row_children(&row));
    }
    // §05-cut's tape: where the bottom pane, band stack and badge 31 landed, with the bar's mapped state (a
    // `set_visible(false)` bar is IN the tree with a zero box).
    for name in CUT_TRACKS {
        let Some(widget) = found(name) else { continue };
        match widget.compute_bounds(window) {
            Some(b) => line.push_str(&format!(
                ", {name} y={:.0}..{:.0} h={:.0} shown={}",
                b.y(),
                b.y() + b.height(),
                b.height(),
                widget.is_visible()
            )),
            None => line.push_str(&format!(", {name} unmapped shown={}", widget.is_visible())),
        }
        // The bottom pane's OWN children, in order: when the pane is taller than its children's allocation GTK draws
        // it at natural size and the LAST child (badge 31) lands past the window's edge, invisible in the PNG.
        if name == "cut-bottom" {
            if let Some(parent) = widget.parent() {
                for child in parent
                    .observe_children()
                    .iter::<glib::Object>()
                    .filter_map(Result::ok)
                    .filter_map(|c| c.downcast::<gtk::Widget>().ok())
                {
                    let tail = match child.compute_bounds(window) {
                        Some(b) => format!(
                            "\n           PAGE {} y={:.0}..{:.0} h={:.0} shown={}",
                            child.widget_name(),
                            b.y(),
                            b.y() + b.height(),
                            b.height(),
                            child.is_visible()
                        ),
                        None => format!("\n           PAGE {} unmapped", child.widget_name()),
                    };
                    line.push_str(&tail);
                }
            }
        }
        if name == "cut-bottom" {
            for child in widget
                .observe_children()
                .iter::<glib::Object>()
                .filter_map(Result::ok)
                .filter_map(|c| c.downcast::<gtk::Widget>().ok())
            {
                let tail = match child.compute_bounds(window) {
                    Some(b) => format!(
                        "\n           {} y={:.0}..{:.0} h={:.0} shown={}",
                        child.widget_name(),
                        b.y(),
                        b.y() + b.height(),
                        b.height(),
                        child.is_visible()
                    ),
                    None => format!(
                        "\n           {} unmapped shown={}",
                        child.widget_name(),
                        child.is_visible()
                    ),
                };
                line.push_str(&tail);
            }
        }
    }
    // The divider's own position is the rule's answer: page above, band below. Printed because a wrong split
    // and a page minimum that swallows the band look identical in the PNG.
    if let Some(paned) = found("main-divider").and_then(|w| w.downcast::<gtk::Paned>().ok()) {
        line.push_str(&format!(
            ", divider pos={} of {}",
            paned.position(),
            paned.height()
        ));
        // What the two halves were actually GIVEN: `pos` alone cannot tell a correct split from a clamped one, since
        // GTK keeps children at their own asks unless one expands.
        for (which, child) in [("page", paned.start_child()), ("band", paned.end_child())] {
            if let Some(child) = child {
                line.push_str(&format!(
                    ", {which} h={} min={}",
                    child.height(),
                    child.measure(gtk::Orientation::Vertical, -1).0
                ));
            }
        }
        // The log's state belongs on this line too: §1 gives the band two heights depending on it.
        if let Some(log) = found("log-expander").and_then(|w| w.downcast::<gtk::Expander>().ok()) {
            line.push_str(&format!(", log open={}", log.is_expanded()));
        }
    }
    Some(line)
}
