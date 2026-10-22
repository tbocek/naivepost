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

/// Badge **13**'s width: what the row has LEFT once every other part of it has been served.
///
/// This is the whole rule, kept apart from any widget so a test can pin it: the bar may take the row's free
/// width (§1 badge 13) but never the room §1's badges and readouts need. `badges` is ▶ / 🎲 / ⏹ and `jobs`
/// the four job buttons — asked off the boxes INSIDE their scrollers, because a `gtk::ScrolledWindow`
/// measures natural 0 and would otherwise report none of its content, which is exactly how ⏹ ended up drawn
/// UNDER the progress bar track in the shell shot. A row narrower than all of them still gets a visible bar:
/// the floor is [`PROGRESS_MIN_PX`], never 0.
pub fn progress_px(total: i32, badges: i32, jobs: i32, readouts: i32, gaps: i32) -> i32 {
    (total - badges - jobs - readouts - gaps).max(super::window_run_bar_fit::PROGRESS_MIN_PX)
}

/// Re-ask §1's progress-bar width for a row that has just been resized. Called by `window_divider` after it
/// moves the band's handle: the bar's ask was first made while the row had no width at all (see
/// [`share_the_free_width`]), so without this the shell keeps a bar sized from nothing.
pub fn resize_row(row: &gtk::Widget) {
    let Some(bar) = find_in(row, "run-progress") else {
        return;
    };
    share_the_free_width(row, &bar);
}

/// The widths GTK actually gave the run-bar row's parts, as `(name, left edge, width)` in the window's own
/// coordinates — how the shell shot and §1's screen tests read badge 13 instead of guessing from the PNG.
/// Report only: decides no rule.
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

/// Size the progress bar from what the row has LEFT, which is what "the row's free width" means (§1 badge 13).
///
/// The bar cannot get that from `hexpand` alone: the controls' scroller must expand too (it measures minimum
/// 0 and would otherwise be given nothing), and a `gtk::Box` then divides the leftover EQUALLY between the two
/// — measured, both got 834 px and the readouts were pushed past the window's edge. So the bar asks for the
/// divider's width minus everything else in the row at its NATURAL size, and keeps expanding so a narrower
/// window squeezes IT and not the buttons.
fn share_the_free_width(row: &gtk::Widget, bar: &gtk::Widget) {
    // THE ROW'S OWN WIDTH, whatever it happens to be when this runs — see [`resize_row`] for why the same ask is
    // re-made once the divider has been allocated.
    // The two scrollers measure natural 0, so their CONTENTS are asked for by name instead: `run-controls-inner`
    // is §1's ▶ / 🎲 / ⏹ and `pass-controls` the four job buttons (`window_run_bar_fit` moved them there).
    // Everything else in the row contributes its own natural ask.
    let natural = |name: &str| {
        find_in(row, name)
            .map(|widget| widget.measure(gtk::Orientation::Horizontal, -1).1.max(0))
            .unwrap_or_default()
    };
    let badges = natural("run-controls-inner");
    let jobs = natural("pass-controls");
    // THE SCROLLER'S ALLOCATION, not its content's natural ask: GTK splits the row from what each child
    // answers for itself, and a `gtk::ScrolledWindow` keeps whatever extra it was given rather than shrinking
    // to its buttons. Measured at §1's minimum, `run-controls` answered 211 px for three badges needing 171,
    // and the four spare pixels landed on top of ⏹ — the bar started at x=217 while ⏹ ran to 224.
    // THE SCROLLERS' OWN ALLOCATIONS, which is what GTK splits the row from — not their content's natural ask.
    // A `gtk::ScrolledWindow` keeps whatever extra it was given instead of shrinking to its buttons: measured at
    // §1's minimum, `run-controls` answered 211 px for three badges needing 171, and those four spare pixels
    // landed on top of ⏹ — the bar started at x=217 while ⏹ ran to 224. Measuring with the allocations is what
    // makes this arithmetic agree with the allocation GTK then makes.
    let allocated = |name: &str, content: i32| {
        find_in(row, name)
            .map(|widget| widget.width().max(content).max(0))
            .unwrap_or(content)
    };
    let controls_w = allocated("run-controls", badges);
    let pass_w = allocated("pass-scroller", jobs);
    // The readouts' own ask, and the row's gaps: the bar gets whatever is left once every other part of the row
    // has been served. `row.width()` answers 1 px before the window is allocated, which clamps the answer to
    // the floor — harmless, because [`resize_row`] asks again with the real width.
    let readouts: i32 = children_of(row)
        .iter()
        .filter(|child| {
            !matches!(
                child.widget_name().as_str(),
                "run-progress" | "run-controls" | "pass-scroller"
            )
        })
        .map(|child| child.measure(gtk::Orientation::Horizontal, -1).1.max(0))
        .sum();
    let gaps = 6 * children_of(row).len() as i32;
    bar.set_size_request(
        progress_px(row.width(), controls_w, pass_w, readouts, gaps),
        -1,
    );
}

/// The first widget named `name` at or below `node`.
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
