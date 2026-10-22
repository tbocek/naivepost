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

/// One line describing where the band is, and MOVING THE HANDLE WHERE §1 WANTS IT. Headless nothing drags this
/// divider, so a snapshot that leaves it at GTK's default shows the split nobody would ever leave it at: measured
/// on `03-window`, the page half sat at its 342 px minimum while the bottom box took 501 px of an 844 px divider
/// and painted its plate over the bottom of the page. The rule lives in `ui::window_divider`; this asks it once,
/// after the window has its real size, through the same door a person's drag fires.
///
/// Returns `None` when this window has no shell band to measure (a dialog shot).
pub fn report(window: &adw::ApplicationWindow) -> Option<String> {
    let found = |name: &str| ui::find_source_widget(window, name);
    // No divider means no shell: a form or dialog top level has its own layout and nothing to report.
    let paned_handle = found("main-divider")?;
    if let Some(paned) = paned_handle.downcast_ref::<gtk::Paned>() {
        // §1's rule read two ways: with the log COLLAPSED the band is its two rows, with it OPEN the log has
        // room of its own. `rest_split` asks the collapsed pair (what the shell rests at); when the shot has
        // opened the log, `opened_px` re-asks through the same door a person's drag fires — otherwise the shot
        // prints a 571 px band inside an 844 px divider, a grey box under the run bar no window shows.
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
