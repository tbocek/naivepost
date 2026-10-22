//! §03-shell.md §1: the bottom row must not be the thing that holds the window wide.
//!
//! §1 puts the run bar and the Inputs/Outputs readouts in ONE row, and "what no longer fits scrolls …
//! instead of holding the window wide". The run bar's row carries ▶ / 🎲 / ⏹ / ⚙ plus the three pass
//! buttons with words on them, so its natural minimum is ~800 px; add the readouts' own 378 px and the
//! row pins the shell wider than §1's 1240 px minimum (`tests/suite/window_shrinks.rs`
//! measures exactly that). So the controls scroll sideways inside the row while the readouts keep their
//! place at its right end — the row's minimum becomes the scroller (0) plus the readouts, not the sum.
//!
//! Split out of `window.rs`, which is at its size budget.

use adw::prelude::*;
use gtk4 as gtk;

/// The run bar's controls, wrapped in a horizontal scroller that never asks for width of its own.
///
/// `hscrollbar_policy = External` rather than `Automatic`: a scrollbar drawn along the bottom of a
/// 34 px-tall row would be a second strip nobody asked for, and GTK still lets the pointer wheel or
/// drag move the content inside. `vexpand(false)` keeps the row its own height.
///
/// The controls keep their own named box (`run-controls`) INSIDE this scroller: every door and test that
/// finds ▶ / 🎲 / ⏹ by name walks the tree, so they are reached through the scroller unchanged. The
/// The scroller is what carries the name `run-controls`: the row's direct children stay exactly
/// [scroller, progress bar, readouts], which is the order §1 draws. The box it holds keeps
/// `run-controls-inner`, so a door or test that wants ▶ / 🎲 / ⏹ descends into the scroller by that name.
pub fn wrap_controls(controls: gtk::Widget) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_widget_name("run-controls");
    scroller.set_hscrollbar_policy(gtk::PolicyType::External);
    scroller.set_vscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&controls));
    // EXPANDS, but only as the filler for whatever else in the row does not: `hand_the_free_width_to` gives
    // §1's free width to badge 13 first and this takes what is left. A `gtk::ScrolledWindow` measures MINIMUM
    // 0 (that is how it stops holding the window wide), so with no expanding flag of its own GTK starved it —
    // measured, `run-controls` came out a 0 px box while ▶ / 🎲 / ⏹ sat on top of the progress bar. The bar
    // still gets the wider share because the scroller never asks for more than its three badges.
    scroller.set_hexpand(true);
    scroller.set_vexpand(false);
    scroller
}

/// Give §1's free width to the progress bar alone: expand `run-progress` AND the controls' scroller (which
/// needs an expanding flag at all, see [`wrap_controls`]), and clear every other child of `row`, INCLUDING
/// their nested boxes. Called once from `window.rs`, after the row has all its children.
///
/// Two measured facts make this a rule rather than one `set_hexpand` call:
/// * a `gtk::Box` copies its children's expand flags into its own measure, so the readouts' box reported
///   "expands" through the `outputs-readout` label (which needs it to ellipsize) and the row split its
///   leftover three ways — measured, the bar got x=6..840 while the buttons needed ~800 px and ⏹ sat UNDER
///   the bar, invisible in the shot yet still findable by name;
/// * matching children BY NAME is deliberate: clones handed back by `observe_children()` do not compare
///   identical to an `upcast_ref()`, so a pointer test silently matched nothing and cleared everything;
/// * the leftover goes to the BAR, not to the two boxes sharing it with it: a `gtk::Box` splits evenly
///   between its expanding children (measured, 834/834 px), so [`share_the_free_width`] sizes the bar from
///   what is actually left once the natural-width children have been served.
pub fn hand_the_free_width_to(row: &gtk::Widget) {
    for child in children_of(row) {
        // The job buttons are NOT cleared: they are §1's job row sitting in the row's slack, and with the
        // scroller minimum-0 the row has room for them only if they take what the bar leaves.
        let keeps = matches!(
            child.widget_name().as_str(),
            "run-progress" | "run-controls" | "pass-controls" | "pass-scroller"
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

/// Size the progress bar from what the row has LEFT, which is what "the row's free width" means (§1 badge 13).
///
/// The bar cannot get that from `hexpand` alone: the controls' scroller must expand too (it measures minimum
/// 0 and would otherwise be given nothing), and a `gtk::Box` then divides the leftover EQUALLY between the two
/// — measured, both got 834 px and the readouts were pushed past the window's edge. So the bar asks for the
/// divider's width minus everything else in the row at its NATURAL size, and keeps expanding so a narrower
/// window squeezes IT and not the buttons.
fn share_the_free_width(row: &gtk::Widget, bar: &gtk::Widget) {
    let Some(divider_w) = row
        .parent()
        .and_then(|bottom| bottom.parent())
        .map(|paned| paned.width())
        .filter(|w| *w > 0)
    else {
        return;
    };
    // Every other child at its natural ask EXCEPT the scroller, whose natural width is §1's whole button row
    // (~800 px) and would leave the bar nothing; the scroller takes only what is left after the bar.
    let mut others = 0;
    for child in children_of(row) {
        match child.widget_name().as_str() {
            // The job scroller counts as an "other": it measures minimum 0 but holds four buttons the row
            // has to make room for, so the bar is sized from what is left after BOTH scrollers' natural asks.
            "run-progress" | "run-controls" => continue,
            _ => others += child.measure(gtk::Orientation::Horizontal, -1).1.max(0),
        }
    }
    let gaps = 6 * children_of(row).len() as i32;
    bar.set_size_request(
        (divider_w - others - gaps).max(super::window_run_bar_fit::PROGRESS_MIN_PX),
        -1,
    );
}

/// The row's direct children, as widgets.
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

/// Clear `hexpand` on `node` and everything under it, so no nested box re-declares the row's free width.
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
