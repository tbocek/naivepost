//! Print where the shell's bottom band actually landed in a snapshot, so a shot that shows no run bar can be
//! traced to a number instead of guessed at.
//!
//! §03-shell.md §1 counts eight badges UNDER the page (**10**-**18**: ▶, the step selector, ⏹, the progress
//! bar, then in the same row the `Inputs:` readout, the Outputs folder button and its count, and under that
//! row the log expander carrying the status line), and says the bottom half is "exactly as tall as the run bar
//! row plus the log expander's header, with no empty space under them: the page takes all the rest of the
//! height". A window whose content asks for more than §1's 900 px leaves that band nothing — every widget is
//! still in the tree with a zero or off-screen box, so the PNG simply has no bottom bar and the exit code says
//! nothing about it. This prints the numbers the shot was taken at, the way `05-rows` prints its row tops.
//!
//! Nothing here decides a rule: it reads bounds and the divider's position and hands back one line of text.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui;

/// The widgets §1 counts under the page, by the names their builders gave them.
const BAND: [&str; 4] = ["main-divider", "run-bar", "inputs-readout", "log-expander"];

/// §05-cut's track region, printed ONLY when the page has them (every other screen answers nothing).
/// §1 draws the tracks FILLING the pane down to badge **31**, with the bar flush under the last row, so a
/// shot that leaves a blank band has to say which of these three did not take the pane's leftover.
const CUT_TRACKS: [&str; 6] = [
    "page-scroll-Cut",
    "cut-page-split",
    "cut-bottom",
    "track-layers",
    "track-scrollbar",
    "camera-rows",
];

/// THE SHELL'S DIVIDER ASKS ITS RESTING SPLIT — the door a person's drag fires, asked for a shot.
///
/// The divider is the one thing a headless window never drags, and a band whose height ask moved (a shot that
/// opened §1's log, a page grown for its own subject) keeps the handle where the OLD ask put it — measured on
/// `03-window`, `pos=570` left the band 571 px of an 844 px divider, so the run-bar row floated in the middle
/// of the window with the log body past its edge. The clamp only answers correctly once the window holds the
/// size the PNG is taken at, so callers run this AFTER their own size ask and BEFORE the pixels.
pub fn rest_band(window: &adw::ApplicationWindow) {
    if let Some(divider) =
        ui::find_source_widget(window, "main-divider").and_then(|w| w.downcast::<gtk::Paned>().ok())
    {
        ui::window_shot::opened_px(&divider);
    }
}

/// The run-bar row's own children, printed as `name x=.. w=..`.
///
/// §1 counts five things in that row (▶ , ⏹ , the progress bar , then the two readouts and
/// the folder button between them) and says all of them are on screen at the minimum size. A shot can hold
/// every one of them in the tree and still draw ⏹ UNDER the progress bar — which is what the shell shot showed
/// while a name lookup found it happily — so the numbers of each direct child are printed here and read off the
/// recipe rather than guessed from the PNG. Report only: this decides no rule.
fn row_children(row: &gtk::Widget) -> String {
    let mut said = String::new();
    for (name, x, w) in crate::ui::window_run_scroller::drawn_row_widths(row) {
        said.push_str(&format!(", {name} x={x:.0} w={w:.0}"));
    }
    said
}

/// One line describing where the band is. The handle itself is moved by [`rest_band`], which `snapshot::run`
/// asks just before this — headless nothing drags this divider, so a shot that left it at GTK's default shows
/// the split nobody would ever leave it at (measured on `03-window`: the page half sat at its 342 px minimum
/// while the bottom box took 501 px of an 844 px divider and painted its plate over the bottom of the page).
///
/// Returns `None` when this window has no shell band to measure (a dialog shot).
pub fn report(window: &adw::ApplicationWindow) -> Option<String> {
    let found = |name: &str| ui::find_source_widget(window, name);
    // No divider means no shell: a form or dialog top level has its own layout and nothing to report.
    let paned_handle = found("main-divider")?;
    if let Some(paned) = paned_handle.downcast_ref::<gtk::Paned>() {
        // §1's rule read two ways: with the log COLLAPSED the band is its two rows, with it OPEN the log has
        // room of its own. A shot that opened its log already had [`rest_band`] ask for the open height; with the
        // log still closed this asks the collapsed pair — what the shell rests at — through the same door a
        // person's drag fires, so a caller that forgot to does not print a band floating mid-window.
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
    // §05-cut's tape: where the bottom pane, the band stack and badge 31 actually landed, with the bar's own
    // mapped state (a `set_visible(false)` bar is IN the tree with a zero box, which a name lookup cannot see).
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
        // The bottom pane's OWN children, in order: when the pane is taller than the space its children
        // allocate, GTK draws it at its natural size and the LAST child (badge 31) lands past the window's
        // edge — invisible in the PNG while the tree says it is there. Only this list says so.
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
    // The divider's own position is the rule's answer: page above it, band below. Printed because a wrong
    // resting split and a page minimum that swallows the band look identical in the PNG.
    if let Some(paned) = found("main-divider").and_then(|w| w.downcast::<gtk::Paned>().ok()) {
        line.push_str(&format!(
            ", divider pos={} of {}",
            paned.position(),
            paned.height()
        ));
        // …and what the two halves were actually GIVEN. `pos` alone cannot tell a correct split from a clamped
        // one: GTK keeps both children at their own asks unless one expands, so an end child taller than
        // `total - position` means the band is painting over the page no matter what the position says.
        for (which, child) in [("page", paned.start_child()), ("band", paned.end_child())] {
            if let Some(child) = child {
                line.push_str(&format!(
                    ", {which} h={} min={}",
                    child.height(),
                    child.measure(gtk::Orientation::Vertical, -1).0
                ));
            }
        }
        // The log's state belongs on this line too: §1 gives the band two different heights depending on it, and
        // a shot that opened the log looks like a band that grew for no reason without it.
        if let Some(log) = found("log-expander").and_then(|w| w.downcast::<gtk::Expander>().ok()) {
            line.push_str(&format!(", log open={}", log.is_expanded()));
        }
    }
    Some(line)
}
