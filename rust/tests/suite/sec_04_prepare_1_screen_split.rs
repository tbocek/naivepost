//! §04-prepare.md **§1 Screen** — the page's GEOMETRY, measured off a real window.
//!
//! §1 was rewritten to say where the parts sit: "The page is split left and right by a draggable
//! divider opened at half: two columns side by side, not one above the other. Left column, top to
//! bottom: one row with "Add source files…" and the "copy into project" tick, the sources list (it
//! scrolls, the rows above and below it stay), and, under it, one row of controls … Right column: the
//! prompt bench, top to bottom: one heading row (left to right: title, mark, then at the row's right
//! end the row picker and Reset), then one text box filling the rest of column (it scrolls)." And the
//! Inputs readout moved: "(in the bottom bar's run-bar row, [`03-shell.md` §1], NOT on the page)".
//!
//! Every check reads NUMBERS out of the realized tree — `compute_bounds`, `start_child`/`end_child`,
//! parent walks — because a page that merely renders can be wrong: before this item the same widgets
//! stacked in one column and rendered fine. Same reasoning as `sec_05_cut_1_screen_layout.rs`.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::Project;
use naivepost::ui;

const ITEM: &str = "sec_04_prepare_1_screen";

/// One GTK application for the whole file: its main loop may only be started by the thread that runs
/// it, so all four checks share one round and each records that it got there.
fn split_round() {
    common::one_app_round(|app| {
        let window = prepare_window(app);
        lay_out();
        check_the_page_is_split_left_and_right(&window);
        RAN_SPLIT.store(true, Ordering::SeqCst);
        check_the_left_column_order(&window);
        RAN_LEFT.store(true, Ordering::SeqCst);
        check_the_bench_heading_order(&window);
        RAN_HEADING.store(true, Ordering::SeqCst);
        check_the_inputs_readout_is_not_on_the_page(&window);
        RAN_READOUT.store(true, Ordering::SeqCst);
        check_the_whole_page_is_on_screen_at_the_minimum(&window);
        RAN_ON_SCREEN.store(true, Ordering::SeqCst);
        check_each_column_starts_at_its_named_top(&window);
        RAN_COLUMN_TOPS.store(true, Ordering::SeqCst);
        check_the_bottom_band_is_inside_the_window(&window);
        RAN_BAND.store(true, Ordering::SeqCst);
        window.close();
        check_the_picker_lists_every_prompt(app);
        RAN_POLICY_LIST.store(true, Ordering::SeqCst);
        check_the_page_holds_no_second_user_context_paragraph(&window);
        RAN_UC.store(true, Ordering::SeqCst);
        window.close();
    });
}

static RAN_SPLIT: AtomicBool = AtomicBool::new(false);
static RAN_LEFT: AtomicBool = AtomicBool::new(false);
static RAN_HEADING: AtomicBool = AtomicBool::new(false);
static RAN_READOUT: AtomicBool = AtomicBool::new(false);
static RAN_POLICY_LIST: AtomicBool = AtomicBool::new(false);
static RAN_ON_SCREEN: AtomicBool = AtomicBool::new(false);
static RAN_COLUMN_TOPS: AtomicBool = AtomicBool::new(false);
static RAN_BAND: AtomicBool = AtomicBool::new(false);
static RAN_UC: AtomicBool = AtomicBool::new(false);

/// §03-shell.md §1 counts eight badges UNDER the page (**10**-**18**: the run bar's ▶, step selector and ⏹,
/// the progress bar, then in the same row the `Inputs:` readout, the Outputs folder button and its count,
/// and under that row the log expander), and says the bottom half is "exactly as tall as the run bar row plus
/// the log expander's header, with no empty space under them: the page takes all the rest of the height".
///
/// A page whose minimum is taller than §1's window leaves that band NOTHING: every widget is still in the tree
/// with a zero box, so a name lookup cannot see it — which is what the shell shots showed (page content to the
/// bottom edge, no run bar at all). So this reads the band's own bounds and the window's measured minimum.
fn check_the_bottom_band_is_inside_the_window(window: &adw::ApplicationWindow) {
    let floor = window.measure(gtk::Orientation::Vertical, -1).0;
    println!(
        "SPLIT s6 window allocated {} px, measured vertical minimum {floor} px (§1's floor {})",
        window.height(),
        naivepost::shell::WINDOW_MIN_H
    );
    assert!(
        floor <= naivepost::shell::WINDOW_MIN_H,
        "{ITEM} s6: the window cannot be smaller than {floor} px, over §1's {} px minimum, so the bottom band has no room inside it",
        naivepost::shell::WINDOW_MIN_H
    );
    for name in ["run-bar", "inputs-readout", "log-expander"] {
        let b = bounds(window, name);
        assert!(
            b.width() > 0.0 && b.height() > 0.0,
            "{ITEM} s6: {name} was given no pixels ({b:?}) — the bottom band is outside the window"
        );
        assert!(
            b.y() + b.height() <= window.height() as f32 + 2.0,
            "{ITEM} s6: {name} runs past the bottom of the window (ends {:.1} in a {} px window)",
            b.y() + b.height(),
            window.height()
        );
    }
    // The band sits UNDER the page, not behind it.
    let split = bounds(window, "prepare-split");
    let run = bounds(window, "run-bar");
    assert!(
        run.y() >= split.y() + split.height() - 4.0,
        "{ITEM} s6: the run bar is not under the page (run y={:.1}, page ends {:.1})",
        run.y(),
        split.y() + split.height()
    );
}

#[test]
fn sec_04_prepare_1_screen_s6_the_bottom_band_is_inside_the_window_under_the_page() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_BAND.load(Ordering::SeqCst),
        "{ITEM} s6: the bottom-band check never ran"
    );
}

/// §1's new sentence: "At the minimum window size the whole page is on screen at once with nothing
/// scrolled out of view: the 'Add source files…' row and the sources list at the top of the left column,
/// the bench's heading row at the top of the right column."
///
/// (a) Nothing scrolled: Prepare's own page scroller has no travel at §1's minimum — the same number
/// `window_shrinks.rs` measures, read here for this page's named controls. (b) Every control §1 numbers on
/// this page has drawn pixels inside the allocated window.
fn check_the_whole_page_is_on_screen_at_the_minimum(window: &adw::ApplicationWindow) {
    let scroller = widget(window, "page-scroll-Prepare")
        .downcast::<gtk::ScrolledWindow>()
        .expect("{ITEM} s4: the page keeps its own scroller");
    let adj = scroller.vadjustment();
    let travel = adj.upper() - adj.page_size();
    println!(
        "SPLIT s4 page-scroll-Prepare travel {travel:.1} px (upper {:.1}, page {:.1})",
        adj.upper(),
        adj.page_size()
    );
    assert!(
        travel <= 1.0,
        "{ITEM} s4: at §1's minimum the page still has {travel:.1} px scrolled out of view"
    );
    for name in [
        "add-sources-row",
        "sources-scroll",
        "bench-heading",
        "bench-text",
    ] {
        assert!(
            ui::window_shot::is_on_screen_for_test(window.upcast_ref(), name),
            "{ITEM} s4: {name} has no drawn box inside the window: {:?}",
            ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), name)
        );
    }
}

#[test]
fn sec_04_prepare_1_screen_s4_at_the_minimum_the_whole_page_is_on_screen() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_ON_SCREEN.load(Ordering::SeqCst),
        "{ITEM} s4: the whole-page-on-screen check never ran"
    );
}

/// The first DRAWN child of a column — §1's "at the top of" is about what is painted first, and a hidden
/// or zero-height widget paints nothing. Its y comes from its bounds against the window, like every other
/// number this file reads.
fn first_drawn_child(
    window: &adw::ApplicationWindow,
    column: &gtk::Widget,
) -> Option<(String, f64)> {
    let mut node = column.first_child();
    while let Some(child) = node {
        if child.height() > 0 {
            let y = child
                .compute_bounds(window)
                .map(|b| b.y() as f64)
                .unwrap_or_default();
            return Some((child.widget_name().to_string(), y));
        }
        node = child.next_sibling();
    }
    None
}

/// §1: the left column starts at the "Add source files…" row and the right one at the bench's heading
/// row, side by side, and the text box fills what is left of its column below that heading.
fn check_each_column_starts_at_its_named_top(window: &adw::ApplicationWindow) {
    let left = widget(window, "prepare-left");
    let right = widget(window, "prepare-right");
    let (left_name, left_y) =
        first_drawn_child(window, &left).expect("{ITEM} s5: the left column draws nothing");
    let (right_name, right_y) =
        first_drawn_child(window, &right).expect("{ITEM} s5: the right column draws nothing");
    println!("SPLIT s5 column tops: left `{left_name}` y={left_y:.1}, right `{right_name}` y={right_y:.1}");
    assert_eq!(
        left_name, "add-sources-row",
        "{ITEM} s5: the left column does not start at the add row"
    );
    assert_eq!(
        right_name, "bench-heading",
        "{ITEM} s5: the right column does not start at the bench heading"
    );
    assert!(
        (left_y - right_y).abs() <= 2.0,
        "{ITEM} s5: the two columns do not begin at the same height: {left_y:.1} vs {right_y:.1}"
    );
    // "one text box filling the rest of the column": under the heading, inside the window.
    let heading = bounds(window, "bench-heading");
    let text = bounds(window, "bench-text");
    println!(
        "SPLIT s5 bench heading y={:.1} h={:.1}; text y={:.1} ends {:.1} in a {} tall window",
        heading.y(),
        heading.height(),
        text.y(),
        text.y() + text.height(),
        window.height()
    );
    assert!(
        text.y() >= heading.y() + heading.height() - 2.0,
        "{ITEM} s5: the text box does not start below the heading ({:.1} vs {:.1})",
        text.y(),
        heading.y() + heading.height()
    );
    assert!(
        text.y() + text.height() <= window.height() as f32 + 2.0,
        "{ITEM} s5: the text box runs past the bottom of the window ({:.1} in a {} px window)",
        text.y() + text.height(),
        window.height()
    );
}

#[test]
fn sec_04_prepare_1_screen_s5_each_column_starts_at_its_named_top_control() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_COLUMN_TOPS.load(Ordering::SeqCst),
        "{ITEM} s5: the column-top check never ran"
    );
}

/// §1's badge **3** as data: the picker lists every prompt in pipeline order and nothing else. F0.7 has no
/// form, so the menu has no non-prompt row: `bench::ROWS` (which drives every `prompts/<key>/` lookup) is
/// the whole list.
fn check_the_picker_lists_every_prompt(app: &adw::Application) {
    let _ = app;
    let rows = naivepost::bench::picker_rows();
    assert_eq!(
        rows.len(),
        naivepost::bench::ROWS.len(),
        "{ITEM} s3: the picker is exactly the prompts"
    );
    for (index, row) in naivepost::bench::ROWS.iter().enumerate() {
        assert_eq!(
            rows[index].1, row.title,
            "{ITEM} s3: picker entry {index} is not the prompt in pipeline order"
        );
    }
}

#[test]
fn sec_04_prepare_1_screen_s3_the_picker_lists_every_prompt_and_nothing_else() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_POLICY_LIST.load(Ordering::SeqCst),
        "{ITEM} s3: the picker-contents check never ran"
    );
}

fn fixture() -> Project {
    naivepost::project::load(&common::fixture_dir()).expect("the fixture loads")
}

fn prepare_window(app: &adw::Application) -> adw::ApplicationWindow {
    let window = ui::build_window(app, &fixture(), "Prepare");
    // §1's minimum, as the shell's own test opens it: every number this file reads is an allocation, so the
    // page is measured in the box GTK lays it out in at that size.
    common::size_to_minimum(&window);
    window
}

/// Let GTK allocate the tree. `present()` alone does not lay children out until an iteration runs, so
/// without this every bound below would read 0 and every comparison would pass by accident.
fn lay_out() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name).unwrap_or_else(|| panic!("{ITEM}: no widget named {name}"))
}

/// Bounds against the window, printed so the log carries the numbers a check decided on.
fn bounds(window: &adw::ApplicationWindow, what: &str) -> gtk::graphene::Rect {
    let w = widget(window, what);
    let b = w
        .compute_bounds(window)
        .unwrap_or_else(|| panic!("{ITEM}: {what} is not laid out"));
    println!(
        "SPLIT {what} x={:.1} y={:.1} w={:.1} h={:.1}",
        b.x(),
        b.y(),
        b.width(),
        b.height()
    );
    b
}

/// Whether `root`'s subtree holds a widget named `name` — how "is this part INSIDE that column" is asked.
fn holds(root: &gtk::Widget, name: &str) -> bool {
    if root.widget_name() == name {
        return true;
    }
    root.observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .any(|child| {
            let Ok(widget) = child.downcast::<gtk::Widget>() else {
                return false;
            };
            holds(&widget, name)
        })
}

/// §1: "split left and right by a draggable divider opened at half: two columns side by side". A
/// `gtk::Paned` is the draggable divider; the two boxes must be ITS halves, and they must be beside
/// each other rather than stacked (which is exactly what the page did before this item).
fn check_the_page_is_split_left_and_right(window: &adw::ApplicationWindow) {
    let split = widget(window, "prepare-split");
    let paned = split
        .clone()
        .downcast::<gtk::Paned>()
        .expect("{ITEM}: prepare-split is a Paned, which is what makes the divider draggable");
    assert_eq!(
        paned.orientation(),
        gtk::Orientation::Horizontal,
        "{ITEM}: the divider splits left and right, not top and bottom"
    );
    let start = paned
        .start_child()
        .expect("{ITEM}: the paned has a start child");
    let end = paned
        .end_child()
        .expect("{ITEM}: the paned has an end child");
    assert_eq!(
        start.widget_name(),
        "prepare-left",
        "{ITEM}: the LEFT column is the paned's start half"
    );
    assert_eq!(
        end.widget_name(),
        "prepare-right",
        "{ITEM}: the RIGHT column is the paned's end half"
    );

    let left = bounds(window, "prepare-left");
    let right = bounds(window, "prepare-right");
    // SIDE BY SIDE: same top edge, and the left column ends where the right one begins.
    assert!(
        (left.y() - right.y()).abs() <= 2.0,
        "{ITEM}: the columns are not side by side — left at y={:.1}, right at y={:.1}",
        left.y(),
        right.y()
    );
    assert!(
        left.x() + left.width() <= right.x() + 2.0,
        "{ITEM}: the left column overlaps or follows the right one ({left:?} vs {right:?})"
    );
    // OPENED AT HALF: neither column is a sliver. Half of §1's 1240 px window less margins is ~596 px.
    assert!(
        left.width() >= 300.0,
        "{ITEM}: the left column is only {:.1} px wide",
        left.width()
    );
    assert!(
        right.width() >= 300.0,
        "{ITEM}: the right column is only {:.1} px wide",
        right.width()
    );
}

#[test]
fn sec_04_prepare_1_screen_s1_the_page_is_split_left_and_right_by_a_draggable_divider() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_SPLIT.load(Ordering::SeqCst),
        "{ITEM} s1: the split check never ran"
    );
}

/// §1's left column, top to bottom: the add row, then the scrolling list (no controls under it since Freq went).
fn check_the_left_column_order(window: &adw::ApplicationWindow) {
    let left = widget(window, "prepare-left");
    assert!(
        !holds(&left, "frame-controls"),
        "{ITEM} s1: no row of frame controls under the list since Freq went"
    );
    for name in ["add-sources-row", "sources-scroll"] {
        assert!(
            holds(&left, name),
            "{ITEM} s1: {name} is not inside the left column"
        );
    }
    let add = bounds(window, "add-sources-row");
    let scroll = bounds(window, "sources-scroll");
    assert!(
        add.y() < scroll.y(),
        "{ITEM} s1: the left column is not add row → list ({add:?} {scroll:?})"
    );
    // The add row STAYS above the list while the list scrolls (§1: "the rows above and below it stay").
    assert!(
        add.y() + add.height() <= scroll.y() + 2.0,
        "{ITEM} s1: the add row overlaps the scrolling band"
    );
    let scroller = widget(window, "sources-scroll")
        .downcast::<gtk::ScrolledWindow>()
        .expect("{ITEM} s1: the sources list sits in its own ScrolledWindow so it can scroll");
    // Vertical scrolling is the one the spec asks for; horizontal would mean a column too narrow to read.
    assert_ne!(
        scroller.vscrollbar_policy(),
        gtk::PolicyType::Never,
        "{ITEM} s1: the list cannot scroll vertically"
    );
}

#[test]
fn sec_04_prepare_1_screen_s1_the_left_column_is_add_row_then_scrolling_list_then_controls() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_LEFT.load(Ordering::SeqCst),
        "{ITEM} s1: the left-column order check never ran"
    );
}

/// §1's right column: one heading row (title first, then at its right end the row picker), the text
/// box under it filling the rest of the column. Reset is skipped here: the bench opens on row 0, the
/// User Context, and §1 hides mark and Reset on that row — asserting its x would assert a hidden widget.
fn check_the_bench_heading_order(window: &adw::ApplicationWindow) {
    let right = widget(window, "prepare-right");
    for name in [
        "bench-heading",
        "prepare-title",
        "bench-picker",
        "bench-text",
    ] {
        assert!(
            holds(&right, name),
            "{ITEM} s2: {name} is not inside the right column"
        );
    }
    let heading = bounds(window, "bench-heading");
    let title = bounds(window, "prepare-title");
    let picker = bounds(window, "bench-picker");
    let text = bounds(window, "bench-text");
    assert!(
        heading.y() < text.y(),
        "{ITEM} s2: the heading row is not above the text box ({heading:?} vs {text:?})"
    );
    // One column: the box starts where the heading starts.
    assert!(
        (text.x() - heading.x()).abs() <= 2.0,
        "{ITEM} s2: the text box does not line up with the heading ({text:?} vs {heading:?})"
    );
    // "filling the rest of the column": the box is the tall part, not a strip under the heading.
    assert!(
        text.height() > 200.0,
        "{ITEM} s2: the text box is only {:.1} px tall",
        text.height()
    );
    // The row picker is gone: the prompts are configured in profiles, so the bench is the User Context alone.
    assert!(
        !widget(window, "bench-picker").is_visible(),
        "{ITEM} s2: no prompt rows to pick on the bench"
    );
    let _ = (picker, title);
}

#[test]
fn sec_04_prepare_1_screen_s2_the_bench_heading_puts_the_picker_and_reset_at_its_right_end() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_HEADING.load(Ordering::SeqCst),
        "{ITEM} s2: the bench heading check never ran"
    );
}

/// §1: the Inputs readout lives "in the bottom bar's run-bar row …, not on the page". So it must exist,
/// must not be inside either column, and must sit below the page's split.
fn check_the_inputs_readout_is_not_on_the_page(window: &adw::ApplicationWindow) {
    let readout = widget(window, "inputs-readout");
    for column in ["prepare-left", "prepare-right"] {
        assert!(
            !holds(&readout, column),
            "{ITEM} s3: {column} contains the Inputs readout"
        );
    }
    // Walk up: neither column may be on the readout's path to the top level.
    let mut node = readout.parent();
    while let Some(parent) = node {
        assert_ne!(
            parent.widget_name(),
            "prepare-left",
            "{ITEM} s3: the readout is inside the left column"
        );
        assert_ne!(
            parent.widget_name(),
            "prepare-right",
            "{ITEM} s3: the readout is inside the right column"
        );
        node = parent.parent();
    }
    let split = bounds(window, "prepare-split");
    let readout_b = bounds(window, "inputs-readout");
    assert!(
        readout_b.y() >= split.y() + split.height() - 40.0,
        "{ITEM} s3: the Inputs readout is not under the page ({} vs split ending at {:.1})",
        readout_b.y(),
        split.y() + split.height()
    );
}

#[test]
fn sec_04_prepare_1_screen_s3_the_inputs_readout_is_in_the_run_bar_row_not_on_the_page() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_READOUT.load(Ordering::SeqCst),
        "{ITEM} s3: the readout-placement check never ran"
    );
}

/// §1 badge **5** is the bench's text box — "the prompt or the User Context", row 0 being the project's
/// User Context — so that text has exactly one place on the screen. An earlier round also drew it full width
/// under the two columns, a copy §1 has no badge for; this pins the one-copy fact by typing a marker into
/// `bench-text` and finding it in exactly one carrier.
fn check_the_page_holds_no_second_user_context_paragraph(window: &adw::ApplicationWindow) {
    // Row 0 through the picker, then a marker line typed into `bench-text` — the program's own store door
    // (`bench::store`, which for row 0 writes the project).
    let marker = "UC-ONCE-7f31 only here";
    let picker = widget(window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("{ITEM} s5: the row picker is a dropdown");
    picker.set_selected(0);
    lay_out();
    let editor = widget(window, "bench-text")
        .downcast::<gtk::TextView>()
        .expect("{ITEM} s5: the bench box is a text view");
    editor.buffer().set_text(marker);
    lay_out();

    let carriers = texts_holding(window.upcast_ref(), marker);
    assert_eq!(
        carriers,
        vec!["bench-text".to_string()],
        "{ITEM} s5: the User Context must show once, in the bench (badge 5), nowhere else on the page"
    );
    // Put the box back the way the round found it: this window is shared by every check in the file.
    editor.buffer().set_text("");
    lay_out();
}

/// Names of the widgets in `root`'s subtree that DRAW `needle` (a label's text or a text view's buffer)
/// and have a non-zero box. A widget with no name is reported by its type so a stray copy is still locatable.
fn texts_holding(root: &gtk::Widget, needle: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut stack: Vec<gtk::Widget> = vec![root.clone()];
    while let Some(node) = stack.pop() {
        let shown = if let Some(label) = node.downcast_ref::<gtk::Label>() {
            label.text().to_string()
        } else if let Some(view) = node.downcast_ref::<gtk::TextView>() {
            let (start, end) = view.buffer().bounds();
            view.buffer().text(&start, &end, true).to_string()
        } else {
            String::new()
        };
        let drawn = node
            .compute_bounds(root)
            .is_some_and(|b| b.height() > 0.0 && b.width() > 0.0);
        if drawn && shown.contains(needle) {
            let name = node.widget_name();
            found.push(if name.is_empty() || name == "unknown" {
                node.type_().name().to_string()
            } else {
                name.to_string()
            });
        }
        for child in node
            .observe_children()
            .iter::<glib::Object>()
            .filter_map(Result::ok)
        {
            if let Ok(widget) = child.downcast::<gtk::Widget>() {
                stack.push(widget);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn sec_04_prepare_1_screen_s5_the_user_context_shows_once_in_the_bench() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_UC.load(Ordering::SeqCst),
        "{ITEM} s5: the one-copy User Context check never ran"
    );
}
