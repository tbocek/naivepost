//! §03-shell.md §1, the two sentences a name lookup cannot answer: is ⏹ DRAWN, and is anything drawn in the
//! gap between the page and the run bar?
//!
//! The three other §1 files (`sec_03_shell_1_the_window`, its `_growth` and `_minimum` twins) pin the title,
//! the header's badge order, the collapsed band and the divider's arithmetic. None of them caught what the
//! rendered `shots/03-window.png` showed: every widget still resolved by name while `stop-button` had been
//! given no room at all, because `find_widget_by_name` walks the TREE and a zero-width box is in the tree.
//! So this file asks only about DRAWN boxes — `ui::window_shot::bounds_in_window_for_test`, the window's own
//! coordinates, which are what a screenshot shows — and prints every number it reads so a red run says where
//! the row went rather than merely that something is off.
//!
//! Shape: exactly ONE `#[test]` drives the widgets (a process gets one `adw::Application::run`,
//! `common::one_app_round`) and one pure-logic test stands on its own. Nothing here calls
//! `context.iteration(true)`: headless has no frame clock, so it would block this process forever.
//!
//! It also guards the deletion made for this item: the Prepare page carried a SECOND User Context paragraph
//! under the split (the prototype keeps one there, `gui/main.go:1569`, but §4 puts that box in the right
//! column and nothing else). A duplicate is invisible to a name lookup — the first match wins — so the check
//! here is geometric: nothing drawn may sit between the bottom of `prepare-split` and the top of `run-bar`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window};

const ITEM: &str = "sec_03_shell_1_the_window_screen";

/// Set once the widget round has run: `one_app_round` swallows a panicking closure, so the test needs its own
/// proof that the checks happened rather than that the loop exited.
static RAN: AtomicBool = AtomicBool::new(false);

/// How far outside a parent's y-range a child may reach before it counts as "not inside": GTK rounds each
/// allocation and a box's border can spill a pixel or two.
const SLACK_PX: f64 = 4.0;

/// §1's badges **10**-**13**, left to right: ▶, "I'm feeling lucky", ⏹, then the progress bar taking the
/// row's free width. Badge 14/15 (the Inputs/Outputs readouts) come after them in the same row.
const RUN_BUTTONS: [&str; 3] = ["play-button", "lucky-button", "stop-button"];

/// The row's children in §1's order: the scrolling controls, the bar that takes the free width, the job
/// buttons moved out of the scroller (`window_run_bar_fit`), and the readouts at the right end.
const ROW_CHILDREN: [&str; 4] = ["run-controls", "run-progress", "pass-controls", "readout-row"];

/// A window on `page` at §1's minimum with the band at rest — the geometry §1 says every shot is taken at.
fn minimum_window(app: &adw::Application, page: &str) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    let window = ui::build_window(app, &model, page);
    window.set_default_size(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    hold_last_window(window.clone());
    common::size_to_minimum(&window);
    common::rest_shell_band(&window);
    window
}

/// Where `named` was drawn, panicking with the name if it was given no box at all.
fn drawn(window: &adw::ApplicationWindow, named: &str) -> (f64, f64, f64, f64) {
    let bounds = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), named);
    assert!(
        bounds.is_some_and(|(_, _, w, h)| w > 0.0 && h > 0.0),
        "{ITEM}: {named} was given no room at §1's minimum size ({bounds:?}) — a name lookup would still find \
         it in the tree, which is exactly the bug this file exists to catch"
    );
    bounds.expect("just checked")
}

/// §1 badges **10**-**15**: the three run buttons and the progress bar are each DRAWN, inside the run-bar
/// row, in §1's left-to-right order, and the readouts start to their right. The picture review found ⏹
/// missing from the rendered row while every name still resolved, so "in the tree" is not the claim here.
fn s1_badges_10_to_15_are_drawn_in_the_run_bar_row(app: &adw::Application) {
    let window = minimum_window(app, "Prepare");
    let (bar_x, bar_y, bar_w, bar_h) = drawn(&window, "run-bar");
    println!("{ITEM} s1: run-bar box x={bar_x:.0} y={bar_y:.0} w={bar_w:.0} h={bar_h:.0}");
    // Every child of the row, printed: this is the picture the review could not get from a name lookup.
    let row = common::widget_in(&window, "run-bar").expect("§1 draws a run-bar row");
    for child in row
        .downcast_ref::<gtk::Box>()
        .expect("the run bar row is a horizontal box")
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|o| o.downcast_ref::<gtk::Widget>().cloned())
    {
        let b = child.compute_bounds(window.upcast_ref::<gtk::Widget>());
        println!(
            "{ITEM} s1: row child {} exp={} box {:?}",
            child.widget_name(),
            child.hexpands(),
            b.map(|r| (r.x() as i32, r.y() as i32, r.width() as i32, r.height() as i32))
        );
    }
    // The three badges §1 names, left to right inside the scroller.
    let mut previous_right = f64::MIN;
    let mut previous_name = String::new();
    for name in RUN_BUTTONS {
        // The handle may land somewhere else after a resize, so the row's own box is re-read each time.
        let (x, y, w, h) = drawn(&window, name);
        println!("{ITEM} s1: {name} box x={x:.0} y={y:.0} w={w:.0} h={h:.0}");
        assert!(w > 0.0 && h > 0.0, "{ITEM} §1 badge: {name} was given an EMPTY box — it is not visible");
        assert!(
            y >= bar_y - SLACK_PX && y + h <= bar_y + bar_h + SLACK_PX,
            "{ITEM} §1: {name} is drawn at y={y:.0}..{:.0}, outside the run-bar row's y={bar_y:.0}..{:.0}",
            y + h,
            bar_y + bar_h
        );
        assert!(
            x >= previous_right - SLACK_PX,
            "{ITEM} §1: {name} starts at x={x:.0}, at or left of {previous_name}'s right edge \
             ({previous_right:.0}) — §1's order is play, lucky, stop"
        );
        previous_right = x + w;
        previous_name = name.to_string();
    }
    // Then the row itself, left to right: the scroller, the bar, the job buttons, the readouts.
    let mut previous_right = f64::MIN;
    let mut previous_name = String::new();
    for name in ROW_CHILDREN {
        let (x, y, w, h) = drawn(&window, name);
        println!("{ITEM} s1: {name} box x={x:.0} y={y:.0} w={w:.0} h={h:.0}");
        assert!(w > 0.0 && h > 0.0, "{ITEM} §1: {name} was given an EMPTY box in the row");
        assert!(
            y >= bar_y - SLACK_PX && y + h <= bar_y + bar_h + SLACK_PX,
            "{ITEM} §1: {name} is drawn at y={y:.0}..{:.0}, outside the run-bar row's y={bar_y:.0}..{:.0}",
            y + h,
            bar_y + bar_h
        );
        assert!(
            x >= previous_right - SLACK_PX,
            "{ITEM} §1: {name} starts at x={x:.0}, at or left of {previous_name}'s right edge \
             ({previous_right:.0}) — §1's row order is controls, progress, jobs, readouts"
        );
        previous_right = x + w;
        previous_name = name.to_string();
    }
    window.close();
}

/// §1: the page sits directly above the run bar. A widget drawn in the strip between them is either a stray
/// control nobody specified or a duplicated block — the second User Context paragraph was exactly that, and
/// a name lookup could never see it because the first match always won.
fn s1_nothing_is_drawn_between_the_page_and_the_run_bar(app: &adw::Application) {
    let window = minimum_window(app, "Prepare");
    let (bar_x, bar_y, bar_w, bar_h) = drawn(&window, "run-bar");
    let split_bottom = {
        let split = common::widget_in(&window, "prepare-split")
            .expect("§4 draws Prepare as a split")
            .compute_bounds(window.upcast_ref::<gtk::Widget>())
            .expect("the Prepare split is drawn");
        println!(
            "{ITEM} s1: prepare-split box x={:.0} y={:.0} w={:.0} h={:.0}",
            split.x(),
            split.y(),
            split.width(),
            split.height()
        );
        split.y() as f64 + split.height() as f64
    };
    let bar_top = bar_y;
    println!("{ITEM} s1: prepare-split ends at y={split_bottom:.0}, run-bar starts at y={bar_top:.0} \
              (gap {:.0} px)", bar_top - split_bottom);
    assert!(
        bar_top >= split_bottom - SLACK_PX,
        "{ITEM} §1: the run bar starts ABOVE the bottom of the page split — the two overlap \
         (split bottom {split_bottom:.0}, run-bar top {bar_top:.0})"
    );
    // Walk every drawn widget and report any whose box falls inside the forbidden strip.
    let mut strays: Vec<String> = Vec::new();
    fn walk(node: &gtk::Widget, window: &adw::ApplicationWindow, from: f64, to: f64, out: &mut Vec<String>) {
        if node.widget_name() != "" && node.widget_name() != "main-divider" && node.widget_name() != "bottom-bar" {
            if let Some(b) = node.compute_bounds(window.upcast_ref::<gtk::Widget>()) {
                let y = b.y() as f64;
                let h = b.height() as f64;
                if h > 0.0 && y >= from + SLACK_PX && y + h <= to - SLACK_PX && to - from > 2.0 * SLACK_PX {
                    out.push(format!("{} y={y:.0} h={h:.0}", node.widget_name(),));
                }
            }
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                walk(&current, window, from, to, out);
                cursor = current.next_sibling();
            }
        }
    }
    walk(
        window.upcast_ref::<gtk::Widget>(),
        &window,
        split_bottom,
        bar_top,
        &mut strays,
    );
    println!("{ITEM} s1: widgets drawn in the strip between the page and the run bar: {:?}", strays);
    assert!(
        strays.is_empty(),
        "{ITEM} §1: something is drawn between the page and the run bar — §1 puts the page directly above \
         it, and a duplicate block hides here: {strays:?}"
    );
    println!("{ITEM} s1: run-bar box x={bar_x:.0} y={bar_y:.0} w={bar_w:.0} h={bar_h:.0}");
    window.close();
}

/// The four job buttons moved into `pass-controls` when the run-bar row was fixed, so prove none of them was
/// lost on the way: each resolves AND is drawn inside the run-bar row.
fn s1_the_pass_buttons_are_still_reachable(app: &adw::Application) {
    let window = minimum_window(app, "Cut");
    let (bar_x, bar_y, bar_w, bar_h) = drawn(&window, "run-bar");
    println!("{ITEM} s1: Cut run-bar box x={bar_x:.0} y={bar_y:.0} w={bar_w:.0} h={bar_h:.0}");
    for name in [
        "policy-button",
        "captions-pass-button",
        "speeds-pass-button",
        "decorations-pass-button",
    ] {
        assert!(
            common::widget_in(&window, name).is_some(),
            "{ITEM}: {name} is gone from the tree — the pass buttons must survive the move to pass-controls"
        );
        let (x, y, w, h) = drawn(&window, name);
        println!("{ITEM} s1: {name} box x={x:.0} y={y:.0} w={w:.0} h={h:.0}");
        assert!(w > 0.0 && h > 0.0, "{ITEM}: {name} resolves but has an EMPTY box");
        assert!(
            y >= bar_y - SLACK_PX && y + h <= bar_y + bar_h + SLACK_PX,
            "{ITEM} §1: {name} is drawn at y={y:.0}..{:.0}, outside the run-bar row y={bar_y:.0}..{:.0}",
            y + h,
            bar_y + bar_h
        );
    }
    window.close();
}

/// The widget round, once.
#[test]
fn sec_03_shell_1_the_window_screen_the_run_bar_row_is_drawn() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        s1_badges_10_to_15_are_drawn_in_the_run_bar_row(app);
        s1_nothing_is_drawn_between_the_page_and_the_run_bar(app);
        s1_the_pass_buttons_are_still_reachable(app);
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "{ITEM}: the widget round never ran");
}

/// The band rule's arithmetic with no window in sight, restated for this item's id so a failure names the
/// screen it belongs to: `rest_position` feeds `gtk::Paned::set_position`, whose own clamp panics when the
/// range comes out inverted, so both degenerate asks have to be answered here.
#[test]
fn sec_03_shell_1_the_window_s1_band_rule_arithmetic() {
    use naivepost::ui::window_shot::rest_position;
    assert_eq!(rest_position(844, 54), 790, "§1: an 844 px divider with a 54 px band gives the page 790");
    assert_eq!(rest_position(844, 0), 844, "no band: the page keeps the whole divider");
    assert_eq!(rest_position(5, 274), 0, "a band taller than the divider cannot eat below zero");
    assert_eq!(rest_position(844, 844), 0, "a band the divider's own height leaves the page nothing");
    for (total, band) in [(844, 1), (844, 843), (100, 100), (1, 1), (10, 0)] {
        let handle = rest_position(total, band);
        assert!(
            handle >= 0 && handle <= total,
            "§1: a resting handle must sit inside the divider: {total} - {band} = {handle}"
        );
    }
}
