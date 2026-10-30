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
//! under the split (the prototype keeps one there, `go/main.go:1569`, but §4 puts that box in the right
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

/// §1's badges **10**, **12** and **13**, left to right: ▶, ⏹, then the progress bar taking the row's free
/// width (badge 11, the prototype's "I'm feeling lucky", is gone). Badge 14/15 (the Inputs/Outputs readouts)
/// come after them in the same row.
const RUN_BUTTONS: [&str; 2] = ["play-button", "stop-button"];

/// The row's children in §1's order and NOTHING else: the scrolling controls (▶ / ⏹), the bar that
/// takes the free width, and the readouts at the right end. §2's list is the row's whole content — no gear,
/// no pass buttons; those flows start from elsewhere.
const ROW_CHILDREN: [&str; 3] = ["run-controls", "run-progress", "readout-row"];
/// A window on `page` at §1's minimum with the band at rest — the geometry §1 says every shot is taken at.
fn minimum_window(app: &adw::Application, page: &str) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    let window = ui::build_window(app, &model, page);
    window.set_default_size(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    hold_last_window(window.clone());
    common::size_to_minimum(&window);
    window
}

/// Where `named` was drawn, panicking with the name if it has no box at all.
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
            b.map(|r| (
                r.x() as i32,
                r.y() as i32,
                r.width() as i32,
                r.height() as i32
            ))
        );
    }
    // The three badges §1 names, left to right inside the scroller.
    let mut previous_right = f64::MIN;
    let mut previous_name = String::new();
    // ⏹'s right edge, kept for the check after the loop: badge 13 may take the row's free width but must leave
    // the badges theirs, and a bar whose track starts LEFT of that is drawn over them (what the shot showed).
    let mut last_badge_right = f64::MIN;
    for name in RUN_BUTTONS {
        // The handle may land somewhere else after a resize, so the row's own box is re-read each time.
        let (x, y, w, h) = drawn(&window, name);
        println!("{ITEM} s1: {name} box x={x:.0} y={y:.0} w={w:.0} h={h:.0}");
        assert!(
            w > 0.0 && h > 0.0,
            "{ITEM} §1 badge: {name} was given an EMPTY box — it is not visible"
        );
        assert!(
            y >= bar_y - SLACK_PX && y + h <= bar_y + bar_h + SLACK_PX,
            "{ITEM} §1: {name} is drawn at y={y:.0}..{:.0}, outside the run-bar row's y={bar_y:.0}..{:.0}",
            y + h,
            bar_y + bar_h
        );
        assert!(
            x >= previous_right - SLACK_PX,
            "{ITEM} §1: {name} starts at x={x:.0}, at or left of {previous_name}'s right edge \
             ({previous_right:.0}) — §1's order is play, stop"
        );
        inside_window(&window, name, x, w);
        previous_right = x + w;
        last_badge_right = x + w;
        previous_name = name.to_string();
    }
    // Badge 13 against those badges: the progress bar's box must begin at or right of ⏹'s right edge.
    let (bar_x, _by, bw, _bh) = drawn(&window, "run-progress");
    // What the row charged each part with, printed beside the boxes: when the bar lands on a badge, the number
    // that was wrong is in this line and not in the picture.
    let row = common::widget_in(&window, "run-bar").expect("§1 draws a run-bar row");
    let charged = naivepost::ui::window_run_scroller::charged_widths_for_test(row.upcast_ref());
    println!("{ITEM} s1: row charges {charged:?}");
    println!("{ITEM} s1: run-progress box x={bar_x:.0} w={bw:.0}, stop-button right edge {last_badge_right:.0}");
    assert!(
        bar_x >= last_badge_right - SLACK_PX,
        "{ITEM} §1 badge 13: the progress bar starts at x={bar_x:.0}, LEFT of ⏹'s right edge \n         ({last_badge_right:.0}) — it takes more than the row's free width and is drawn over the buttons"
    );
    // Then the row itself, left to right: the scroller, the bar, the job buttons, the readouts.
    let mut previous_right = f64::MIN;
    let mut previous_name = String::new();
    for name in ROW_CHILDREN {
        let (x, y, w, h) = drawn(&window, name);
        println!("{ITEM} s1: {name} box x={x:.0} y={y:.0} w={w:.0} h={h:.0}");
        assert!(
            w > 0.0 && h > 0.0,
            "{ITEM} §1: {name} was given an EMPTY box in the row"
        );
        assert!(
            y >= bar_y - SLACK_PX && y + h <= bar_y + bar_h + SLACK_PX,
            "{ITEM} §1: {name} is drawn at y={y:.0}..{:.0}, outside the run-bar row's y={bar_y:.0}..{:.0}",
            y + h,
            bar_y + bar_h
        );
        assert!(
            x >= previous_right - SLACK_PX,
            "{ITEM} §1: {name} starts at x={x:.0}, at or left of {previous_name}'s right edge \
             ({previous_right:.0}) — §1's row order is controls, progress, readouts"
        );
        inside_window(&window, name, x, w);
        previous_right = x + w;
        previous_name = name.to_string();
    }
    // …and the three parts of badges **14**/**15**, individually: a row whose last child ends inside the window
    // can still have one of them squeezed to nothing beside it. The row is allocated ~362 px at §1's minimum while
    // this fixture's two full sentences ask ~528 px, so GTK shrinks somebody; §1's precedent for what gives way is
    // badge **4**'s project path — it ellipsizes, the tabs beside it keep their words — so here the count label
    // takes a share and ellipsizes into it, and the fact worth pinning is that all three are on screen and in
    // order, not the share's width. `prepare_screen_widgets.rs` pins the sentence itself from the text.
    let (row_x, _ry, row_w, _rh) = drawn(&window, "readout-row");
    let mut previous_right = row_x;
    for name in [
        "inputs-readout",
        "outputs-label",
        "outputs-folder-button",
        "outputs-readout",
    ] {
        let (x, _y, w, h) = drawn_or_share(&window, name);
        println!("{ITEM} s1: {name} box x={x:.0} w={w:.0} h={h:.0}");
        assert!(
            h > 0.0 && x >= -SLACK_PX && x + w <= window.width() as f64 + SLACK_PX,
            "{ITEM} §1 badge: {name} is not on screen at §1's minimum ({x:.0},{w:.0},{h:.0})"
        );
        assert!(
            x >= previous_right - SLACK_PX,
            "{ITEM} §1: {name} starts at x={x:.0}, left of the previous badge's right edge {previous_right:.0}"
        );
        previous_right = x + w;
    }
    assert!(
        previous_right <= row_x + row_w + SLACK_PX,
        "{ITEM} §1: the readouts run to {:.0}, past their row's right edge {:.0}",
        previous_right,
        row_x + row_w
    );
    window.close();
}

/// The box of a label §1 lets give way by ellipsizing (badge **4**'s project path, badge **15**'s count): it must
/// have a height and sit inside the window, but its width may be as narrow as the ellipsis GTK falls back to, so
/// an empty box is not by itself the failure here — the failure is a part of the row that is NOT on screen, and
/// the sentence it should print, which `prepare_screen_widgets.rs` pins from the label's own text.
fn drawn_or_share(window: &adw::ApplicationWindow, named: &str) -> (f64, f64, f64, f64) {
    ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), named)
        .unwrap_or_else(|| panic!("{ITEM}: {named} has no drawn box at all at §1's minimum size"))
}

/// Is `named`'s box wholly inside the window's width? This is the fact a clipped ⏹ broke and a name lookup
/// cannot see: `find_widget_by_name` walks the tree, and a widget pushed past the window's right edge, or
/// squeezed into a zero-width corner of its scroller, is still in the tree.
fn inside_window(window: &adw::ApplicationWindow, named: &str, x: f64, w: f64) {
    assert!(
        x >= -SLACK_PX && x + w <= window.width() as f64 + SLACK_PX,
        "{ITEM} §1: {named} is drawn from x={x:.0} to {:.0}, which reaches outside the {} px window — \
         it is not on screen at §1's minimum size",
        x + w,
        window.width()
    );
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
    println!(
        "{ITEM} s1: prepare-split ends at y={split_bottom:.0}, run-bar starts at y={bar_top:.0} \
              (gap {:.0} px)",
        bar_top - split_bottom
    );
    assert!(
        bar_top >= split_bottom - SLACK_PX,
        "{ITEM} §1: the run bar starts ABOVE the bottom of the page split — the two overlap \
         (split bottom {split_bottom:.0}, run-bar top {bar_top:.0})"
    );
    // Walk every drawn widget and report any whose box falls inside the forbidden strip.
    let mut strays: Vec<String> = Vec::new();
    fn walk(
        node: &gtk::Widget,
        window: &adw::ApplicationWindow,
        from: f64,
        to: f64,
        out: &mut Vec<String>,
    ) {
        // The divider's own handle is §1's divider, not a stray: with the bar 8 px under it (§1's margins)
        // it now sits inside the strip.
        if node.widget_name() != ""
            && node.widget_name() != "main-divider"
            && node.widget_name() != "bottom-bar"
            && node.widget_name() != "GtkPanedHandle"
        {
            if let Some(b) = node.compute_bounds(window.upcast_ref::<gtk::Widget>()) {
                let y = b.y() as f64;
                let h = b.height() as f64;
                if h > 0.0
                    && y >= from + SLACK_PX
                    && y + h <= to - SLACK_PX
                    && to - from > 2.0 * SLACK_PX
                {
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
    println!(
        "{ITEM} s1: widgets drawn in the strip between the page and the run bar: {:?}",
        strays
    );
    assert!(
        strays.is_empty(),
        "{ITEM} §1: something is drawn between the page and the run bar — §1 puts the page directly above \
         it, and a duplicate block hides here: {strays:?}"
    );
    println!("{ITEM} s1: run-bar box x={bar_x:.0} y={bar_y:.0} w={bar_w:.0} h={bar_h:.0}");
    window.close();
}

/// §1 badge **5**, both halves: the tab row is the header bar's title widget, so it reads as being in the
/// header's CENTRE only if it is actually drawn there (a name lookup cannot see a shifted title widget —
/// libadwaita's default LOOSE centring slid the row right of centre beside the expanding project path), and its
/// four WORDS are drawn whole at this size (libadwaita ellipsizes a switcher's labels when its centre slot runs
/// short, "Prep…"), which `ui::window_tabs::keep_labels_whole` is what holds. So this measures the drawn box —
/// its horizontal centre against the window's own — and then each tab button's width against its label's.
fn s2_the_tab_row_sits_in_the_header_centre(app: &adw::Application) {
    let window = minimum_window(app, "Prepare");
    let (x, _y, w, h) = drawn(&window, "page-tabs");
    let tabs_centre = x + w / 2.0;
    let window_centre = window.width() as f64 / 2.0;
    println!(
        "{ITEM} s2: page-tabs box x={x:.0} y={_y:.0} w={w:.0} h={h:.0} centre={tabs_centre:.0}, window centre={window_centre:.0}"
    );
    assert!(
        (tabs_centre - window_centre).abs() <= CENTRE_SLACK_PX,
        "{ITEM} §1: the tab row is drawn with its centre at {tabs_centre:.0}, the window's centre is \
         {window_centre:.0} — §1 wants the tab row in the header bar's centre"
    );
    // …and the words: §1 makes this the size where "every control and lane a page names is on screen at
    // once", so a tab may neither be allowed to ellipsize nor be squeezed under its own text.
    for page in shell::Page::all() {
        let word = page.label();
        let button = ui::tab_button(&window, page)
            .unwrap_or_else(|| panic!("{ITEM}: the tab row has no {word} button"));
        // The tab's own label — the one whose text IS the word. A switcher button holds its label inside a
        // box of its own, so the walk goes down through every child rather than only the first.
        fn find_label(node: &gtk::Widget, word: &str) -> Option<gtk::Label> {
            for child in node.observe_children().iter::<glib::Object>() {
                let Ok(child) = child else { continue };
                let Ok(widget) = child.downcast::<gtk::Widget>() else {
                    continue;
                };
                if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
                    if label.text() == word {
                        return Some(label);
                    }
                }
                if let Some(found) = find_label(&widget, word) {
                    return Some(found);
                }
            }
            None
        }
        let label = find_label(button.upcast_ref(), word)
            .unwrap_or_else(|| panic!("{ITEM}: the {word} tab holds no label saying {word:?}"));
        assert_eq!(
            label.ellipsize(),
            gtk::pango::EllipsizeMode::None,
            "{ITEM} §1: the {word} tab may be shown as an ellipsis"
        );
        let natural = label.measure(gtk::Orientation::Horizontal, -1).1;
        let button_w = button
            .compute_bounds(window.upcast_ref::<gtk::Widget>())
            .map(|b| b.width() as f64)
            .unwrap_or_default();
        println!("{ITEM} s2: tab {word} natural={natural} drawn w={button_w:.1} (row w={w:.0})");
        assert!(
            button_w >= f64::from(natural),
            "{ITEM} §1: the {word} tab was given {button_w:.1} px for a {natural} px word, so it prints truncated"
        );
    }
    window.close();
}

/// How far off the window's centre the tab row may sit before it counts as not centred: GTK rounds each
/// allocation and Adwaita pads the header's ends, so a strictly centred row is a pixel or two off.
const CENTRE_SLACK_PX: f64 = 8.0;

/// The box's children by widget name, in the order it holds them — how "exactly these and no others" is said.
fn child_names(widget: &gtk::Widget) -> Vec<String> {
    widget
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect()
}

/// §2's run bar holds EXACTLY what its list says: ▶ , ⏹ , the progress bar, then the
/// Inputs readout, the "Outputs:" label, the folder button and the count. Nothing else may sit in that row — a
/// control no section places there is what this check exists to catch (the editing-policy ⚙ and the three F3.x
/// pass buttons once did, clipped to "Cap…" in the rendered shot). Checked on the Cut page, where those four
/// controls used to be added.
fn s1_the_run_bar_holds_only_what_section_two_lists(app: &adw::Application) {
    let window = minimum_window(app, "Cut");
    let (bar_x, bar_y, bar_w, bar_h) = drawn(&window, "run-bar");
    println!("{ITEM} s1: Cut run-bar box x={bar_x:.0} y={bar_y:.0} w={bar_w:.0} h={bar_h:.0}");
    // (a) the row itself, by child name, in order.
    assert_eq!(
        child_names(&common::widget_in(&window, "run-bar").expect("§1 draws a run-bar row")),
        vec!["run-controls", "run-progress", "readout-row"],
        "{ITEM} §1/§2: the run-bar row holds something §2 does not list"
    );
    // (b) inside the scroller: the two transport badges and the reset, alone.
    assert_eq!(
        child_names(
            &common::widget_in(&window, "run-controls-inner")
                .expect("the controls sit in their scrolling box")
        ),
        vec!["play-button", "stop-button", "reset-button"],
        "{ITEM} §2: the run bar's controls are not exactly \u{25b6} , \u{23f9} , \u{21ba}"
    );
    // (c) the readouts at the row's right end, in §1's order.
    assert_eq!(
        child_names(
            &common::widget_in(&window, "readout-row").expect("the readouts share the run row")
        ),
        vec![
            "inputs-readout",
            "outputs-label",
            "outputs-folder-button",
            "outputs-readout"
        ],
        "{ITEM} §1 badge 14/15: the readouts are not exactly Inputs, Outputs label, folder button, count"
    );
    // (d) none of the four controls that do not belong here is anywhere in the window.
    for name in [
        "policy-button",
        "captions-pass-button",
        "speeds-pass-button",
        "decorations-pass-button",
    ] {
        assert!(
            common::widget_in(&window, name).is_none(),
            "{ITEM} §1: {name} is in the window tree — no spec section puts a control for it on the run bar"
        );
    }
    window.close();
}

/// §1 badge **12** against its own scroller: ⏹ must be drawn INSIDE the `run-controls` viewport, not merely
/// present in the tree. Every order check above compares one badge's box with the next badge's box, so a badge
/// pushed sideways OUT of its scroller by content too wide for it passes all of them — GTK reports the box it
/// gave the button inside the scrollable's own (wider) canvas, and `find_widget_by_name` walks the tree, so the
/// rendered row showed ▶, the prototype's gears and the progress track with no ⏹ at all while the whole suite
/// stayed green. This is the check that measures the clip itself.
fn s12_the_stop_button_is_drawn_inside_its_scroller(app: &adw::Application) {
    let window = minimum_window(app, "Prepare");
    let (stop_x, _sy, stop_w, stop_h) = drawn(&window, "stop-button");
    let (view_x, view_y, view_w, view_h) = drawn(&window, "run-controls");
    println!(
        "{ITEM} s12: stop-button box x={stop_x:.0} w={stop_w:.0} h={stop_h:.0}; run-controls viewport x={view_x:.0} w={view_w:.0} y={view_y:.0} h={view_h:.0}"
    );
    assert!(
        stop_x + stop_w <= view_x + view_w + SLACK_PX,
        "{ITEM} §1 badge 12: ⏹ runs from x={stop_x:.0} to {:.0}, past its scroller's right edge {:.0} — GTK \
         scrolled it out of the viewport, so the rendered run bar has no stop button even though the widget is \
         in the tree",
        stop_x + stop_w,
        view_x + view_w
    );
    assert!(
        stop_x >= view_x - SLACK_PX,
        "{ITEM} §1 badge 12: ⏹ starts at x={stop_x:.0}, left of its scroller's left edge {view_x:.0}"
    );
    window.close();
}

/// The widget round, once.
#[test]
fn sec_03_shell_1_the_window_screen_the_run_bar_row_is_drawn() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        s1_badges_10_to_15_are_drawn_in_the_run_bar_row(app);
        s12_the_stop_button_is_drawn_inside_its_scroller(app);
        s1_nothing_is_drawn_between_the_page_and_the_run_bar(app);
        s2_the_tab_row_sits_in_the_header_centre(app);
        s1_the_run_bar_holds_only_what_section_two_lists(app);
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(
        RAN.load(Ordering::SeqCst),
        "{ITEM}: the widget round never ran"
    );
}

/// §1 badge **13**'s rule, in words a test can check without a window: the bar takes what is left of the row
/// once every other part of it has been served, and never squeezes another part out of the row.
#[test]
fn sec_03_shell_1_the_window_s13_the_progress_bar_leaves_the_badges_their_room() {
    use naivepost::ui::window_run_bar_fit::PROGRESS_MIN_PX;
    use naivepost::ui::window_run_scroller::progress_px;
    // §1's numbers for the row at the minimum size: a ~1220 px row, ▶ / 🎲 / ⏹ needing 171 px, the two
    // readouts 531 px, and three gaps of 6 px (measured off `just snapshot 03-window`).
    let (row_w, badges, readouts, gaps) = (1220, 171, 531, 18);
    let bar = progress_px(row_w, badges, readouts, gaps);
    assert_eq!(
        bar + badges + readouts + gaps,
        row_w,
        "{ITEM} §1 badge 13: the bar must take EXACTLY the row's free width — no more (that is what drew ⏹ \
         under the track) and no less (that leaves blank plate in the row)"
    );
    // A row narrower than everything in it still gets a VISIBLE bar: the floor is `PROGRESS_MIN_PX`, never 0.
    let squeezed = progress_px(400, badges, readouts, gaps);
    assert_eq!(
        squeezed, PROGRESS_MIN_PX,
        "{ITEM} §1 badge 13: a squeezed row must fall back to the bar's floor, not to nothing"
    );
    assert!(
        squeezed > 0,
        "{ITEM}: a zero-width progress bar is invisible"
    );
}

/// The band rule's arithmetic with no window in sight, restated for this item's id so a failure names the
/// screen it belongs to: `rest_position` feeds `gtk::Paned::set_position`, whose own clamp panics when the
/// range comes out inverted, so both degenerate asks have to be answered here.
#[test]
fn sec_03_shell_1_the_window_s1_band_rule_arithmetic() {
    use naivepost::ui::window_shot::rest_position;
    assert_eq!(
        rest_position(844, 54),
        790,
        "§1: an 844 px divider with a 54 px band gives the page 790"
    );
    assert_eq!(
        rest_position(844, 0),
        844,
        "no band: the page keeps the whole divider"
    );
    assert_eq!(
        rest_position(5, 274),
        0,
        "a band taller than the divider cannot eat below zero"
    );
    assert_eq!(
        rest_position(844, 844),
        0,
        "a band the divider's own height leaves the page nothing"
    );
    for (total, band) in [(844, 1), (844, 843), (100, 100), (1, 1), (10, 0)] {
        let handle = rest_position(total, band);
        assert!(
            handle >= 0 && handle <= total,
            "§1: a resting handle must sit inside the divider: {total} - {band} = {handle}"
        );
    }
}
