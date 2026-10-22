//! §03-shell.md §1's run-bar row, made to FIT at the window's minimum size.
//!
//! §1 counts four badges in that row — **10** ▶, **11** "I'm feeling lucky", **12** ⏹, **13** the progress
//! bar — and then the readouts at its right end, and says that at 1240×900 "every control a page names is on
//! screen at once". The row as built could not show them: `run-controls-inner` also carries the policy gear
//! and the three F3.x pass buttons (✐ Captions, ⏩ Speeds, ✨ Decorations), whose labels together ask ~600 px,
//! and the row hands its sideways scroller whatever is left after the progress bar and the readouts — measured
//! ~205 px — so GTK clipped ⏹ and everything after it inside the scroller. A shot of the shell then showed ▶,
//! "I'm feeling lucky" and the bar, with badge 12 nowhere on screen.
//!
//! The fix separates what §1 puts IN THE ROW from what the app happens to start from the same place. The three
//! pass buttons are jobs, not §1's badges (§05 #1-screen pins the Cut toolbar to its six declared groups;
//! these three name no §1 item), so they get a box of their own AFTER the progress bar and before the
//! readouts, where the row has slack, while `run-controls-inner` keeps exactly ▶ / 🎲 / ⏹ and fits. Every
//! widget keeps its name, so every door and test still finds them by walking the window.
//!
//! Split out of `window.rs`, which is at its size budget.

use adw::prelude::*;
use gtk4 as gtk;

/// Move the run bar's JOB buttons out of the scrolling controls box into a row of their own.
///
/// `row` is the `run-bar` box, `controls` its `run-controls-inner` child, `progress` the progress bar that
/// already sits in `row`, `readout_row` the readouts box the divider appends last. The pass buttons are
/// re-seated into the scroller below, in the order they were appended: ⚙ policy, ✐ Captions, ⏩ Speeds,
/// ✨ Decorations. The gear goes with them for the same reason: it is a way INTO a form, not one of §1's four
/// badges, and leaving it in the controls scroller would clip it exactly as ⏹ was clipped.
pub fn fit(row: &gtk::Box, controls: &gtk::Box, readout_row: &gtk::Widget) {
    let jobs = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    jobs.set_widget_name("pass-controls");
    // Everything this box takes belongs to it now: `GtkBox::append` re-parents, so each button leaves
    // `run-controls-inner` as it arrives here, and the scroller's natural width drops to the three badges.
    for name in ["policy-button", "captions-pass-button", "speeds-pass-button", "decorations-pass-button"] {
        if let Some(button) = find_in(controls.upcast_ref(), name) {
            jobs.append(&button);
        }
    }
    // The box goes into a scroller of its own, not straight on the row. §1/§2 count FOUR badges in this row
    // (▶, the lucky gears, ⏹, the progress bar) and then the readouts at its right end; these four buttons are
    // other items' jobs sharing the row, and at §1's 1240 px minimum their natural width is what squeezed ⏹
    // out of the shot. Inside a scroller (minimum width 0, see `window_run_scroller::wrap_controls`) they give
    // way and §1's badges keep their pixels. The INNER box keeps the name `pass-controls`, so every door and
    // test that walks for it by name still finds the buttons' own row.
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_widget_name("pass-scroller");
    scroller.set_hscrollbar_policy(gtk::PolicyType::External);
    scroller.set_vscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&jobs));
    scroller.set_hexpand(true);
    scroller.set_vexpand(false);
    // Nothing here puts a width on the row or its boxes. A `set_size_request` anywhere in this row becomes part
    // of the shell's minimum WIDTH (measured: 500 px there truncated every tab label in the header, and giving
    // the two scrollers their buttons' natural ask pushed the shot to 1341 px against §1's 1240), and §1 fixes
    // 1240 as the narrowest the window can ever be. The clipped ⏹ that this file was written for is instead
    // solved by what `fit` already does — moving the four job buttons out of `run-controls-inner` into their own
    // scroller — which drops the controls' natural ask from ~800 px to ▶ / 🎲 / ⏹ (~228 px) and leaves the row
    // room for the bar and the readouts at §1's width without any fixed number.
    // The scroller goes AFTER the progress bar (§1: the bar takes the row's free width) and BEFORE the
    // readouts, which stay at the row's right end. `GtkBox::insert_child_after` is gtk4's way of saying
    // "goes after this sibling"; with no previous sibling found it simply appends.
    let before = row
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|o| o.downcast_ref::<gtk::Widget>().cloned())
        .find(|w| w.widget_name() == readout_row.widget_name())
        .and_then(|readouts| readouts.prev_sibling());
    match before {
        Some(anchor) => row.insert_child_after(&scroller, Some(&anchor)),
        None => row.append(&scroller),
    }
}

/// Badge **13**: the floor the progress bar keeps even when everything else in the row has been served —
/// "the progress bar (taking the row's free width)" with no room at all would be an invisible bar. Used by
/// [`super::window_run_scroller::share_the_free_width`].
pub const PROGRESS_MIN_PX: i32 = 120;

/// The first widget named `name` at or below `node`.
fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    for child in node.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
        if let Some(widget) = child.downcast_ref::<gtk::Widget>().and_then(|w| find_in(w, name)) {
            return Some(widget);
        }
    }
    None
}
