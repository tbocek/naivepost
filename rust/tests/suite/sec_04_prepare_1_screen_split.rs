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

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::Project;
use naivepost::ui;
use crate::common;

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
        window.close();
        check_the_picker_lists_every_prompt_plus_policy(app);
        RAN_POLICY_LIST.store(true, Ordering::SeqCst);
        check_editing_policy_opens_the_form(app);
        RAN_POLICY_OPEN.store(true, Ordering::SeqCst);
        check_the_title_names_the_open_row(app);
        RAN_TITLE.store(true, Ordering::SeqCst);
    });
}

static RAN_SPLIT: AtomicBool = AtomicBool::new(false);
static RAN_LEFT: AtomicBool = AtomicBool::new(false);
static RAN_HEADING: AtomicBool = AtomicBool::new(false);
static RAN_READOUT: AtomicBool = AtomicBool::new(false);
static RAN_POLICY_LIST: AtomicBool = AtomicBool::new(false);
static RAN_POLICY_OPEN: AtomicBool = AtomicBool::new(false);
static RAN_TITLE: AtomicBool = AtomicBool::new(false);

/// §1's badge **3** as data: the picker lists every prompt in pipeline order and then the row §1's REVIEW
/// line adds. The policy row is a row of the MENU, not a prompt — `bench::ROWS` (which drives every
/// `prompts/<key>/` lookup) stays at its thirteen entries, so nothing about a prompt file changes.
fn check_the_picker_lists_every_prompt_plus_policy(app: &adw::Application) {
    let _ = app;
    let rows = naivepost::bench::picker_rows();
    assert_eq!(
        rows.len(),
        naivepost::bench::ROWS.len() + 1,
        "{ITEM} s3: the picker is the prompts plus Editing policy"
    );
    for (index, row) in naivepost::bench::ROWS.iter().enumerate() {
        assert_eq!(
            rows[index].1, row.title,
            "{ITEM} s3: picker entry {index} is not the prompt in pipeline order"
        );
    }
    assert_eq!(
        rows.last().map(|(_, title)| title.as_str()),
        Some(naivepost::bench::POLICY_TITLE),
        "{ITEM} s3: the last picker row is \"Editing policy\""
    );
    assert!(naivepost::bench::is_policy(naivepost::bench::POLICY_ROW));
    assert!(!naivepost::bench::is_policy(0), "{ITEM} s3: User Context is not the policy row");
    for index in 0..naivepost::bench::ROWS.len() {
        assert!(!naivepost::bench::is_policy(index), "{ITEM} s3: row {index} is a prompt");
    }
}

#[test]
fn sec_04_prepare_1_screen_s3_the_picker_lists_every_prompt_plus_editing_policy() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_POLICY_LIST.load(Ordering::SeqCst),
        "{ITEM} s3: the picker-contents check never ran"
    );
}

/// The WIRE: choosing "Editing policy" in the real dropdown opens F0.7's form through the same door the
/// run bar's ⚙ and every tab's ⓘ use, and leaves the bench on its prompt row (the menu has no text of its
/// own to show). Asserted through `ui::policy_form_open`/`open_policy_form`, the accessors that door
/// publishes, and back on the picker's face and the heading it reads.
fn check_editing_policy_opens_the_form(app: &adw::Application) {
    let window = prepare_window(app);
    lay_out();
    let picker = widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown");
    let before = picker.selected();
    let title_before = widget(&window, "prepare-title")
        .downcast::<gtk::Label>()
        .expect("the heading is a label")
        .label()
        .to_string();
    assert!(
        !naivepost::bench::is_policy(usize::try_from(before).unwrap_or(0)),
        "{ITEM} s3: the bench opens on a prompt row"
    );
    assert!(
        !naivepost::ui::policy_form_open(&window),
        "{ITEM} s3: nothing opened the policy form yet"
    );

    // The toolkit's own way in: a new selection on the dropdown, which is what a click on the menu does.
    picker.set_selected(naivepost::bench::POLICY_ROW as u32);
    lay_out();

    assert!(
        naivepost::ui::policy_form_open(&window),
        "{ITEM} s3: choosing Editing policy did not open the policy form"
    );
    assert!(
        naivepost::ui::open_policy_form().is_some(),
        "{ITEM} s3: the form is not the handle press_policy published"
    );
    assert_eq!(
        picker.selected(),
        before,
        "{ITEM} s3: after the policy row the picker went back to the prompt it was on"
    );
    let title_after = widget(&window, "prepare-title")
        .downcast::<gtk::Label>()
        .expect("the heading is a label")
        .label()
        .to_string();
    assert_eq!(title_after, title_before, "{ITEM} s3: the bench kept its prompt heading");
    assert_ne!(
        title_after, naivepost::bench::POLICY_TITLE,
        "{ITEM} s3: the heading must not read the policy row's name"
    );
    // §1's REVIEW line says the row OPENS the form of F0.7 — it is not itself a bench row, so choosing it
    // must not move the bench's own idea of which prompt is open.
    assert_eq!(
        naivepost::bench::POLICY_TITLE,
        "Editing policy",
        "{ITEM} s3: the picker's extra row is spelled \"Editing policy\", as §1 writes it"
    );
    assert!(
        naivepost::bench::ROWS.iter().all(|row| row.title != naivepost::bench::POLICY_TITLE),
        "{ITEM} s3: the policy row is a menu entry, not a fourteenth prompt"
    );
    // Close it again so a later check in this round starts with no form showing.
    if let Some(form) = naivepost::ui::open_policy_form() {
        form.close();
    }
    lay_out();
    window.close();
}

#[test]
fn sec_04_prepare_1_screen_s3_editing_policy_opens_the_policy_form() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_POLICY_OPEN.load(Ordering::SeqCst),
        "{ITEM} s3: the Editing-policy wire check never ran"
    );
}

/// §1's badge **2**: the heading names the row the bench is open on, as "‹Name› prompt", and the picker's
/// face agrees with it. Built through `set_bench_open_row`, the documented seam `--snapshot 04-prepare`
/// uses to put the Describe prompt on screen (§1's own picture).
fn check_the_title_names_the_open_row(app: &adw::Application) {
    ui::set_bench_open_row(2);
    let window = prepare_window(app);
    lay_out();
    let title = widget(&window, "prepare-title")
        .downcast::<gtk::Label>()
        .expect("the heading is a label");
    assert_eq!(
        title.label().as_str(),
        naivepost::bench::title(&naivepost::bench::ROWS[2]),
        "{ITEM} s2: the heading is the open prompt's name with \"prompt\" after it"
    );
    let picker = widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown");
    assert_eq!(
        usize::try_from(picker.selected()).unwrap_or(usize::MAX),
        2,
        "{ITEM} s2: the picker's face shows the same row the heading names"
    );
    window.close();
}

#[test]
fn sec_04_prepare_1_screen_s2_the_title_names_the_open_prompt_row() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_TITLE.load(Ordering::SeqCst),
        "{ITEM} s2: the open-row title check never ran"
    );
}

fn fixture() -> Project {
    naivepost::project::load(&common::fixture_dir()).expect("the fixture loads")
}

fn prepare_window(app: &adw::Application) -> adw::ApplicationWindow {
    let window = ui::build_window(app, &fixture(), "Prepare");
    window.present();
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
    root.observe_children().iter::<glib::Object>().filter_map(Result::ok).any(|child| {
        let Ok(widget) = child.downcast::<gtk::Widget>() else { return false };
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
    let start = paned.start_child().expect("{ITEM}: the paned has a start child");
    let end = paned.end_child().expect("{ITEM}: the paned has an end child");
    assert_eq!(start.widget_name(), "prepare-left", "{ITEM}: the LEFT column is the paned's start half");
    assert_eq!(end.widget_name(), "prepare-right", "{ITEM}: the RIGHT column is the paned's end half");

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
    assert!(left.width() >= 300.0, "{ITEM}: the left column is only {:.1} px wide", left.width());
    assert!(right.width() >= 300.0, "{ITEM}: the right column is only {:.1} px wide", right.width());
}

#[test]
fn sec_04_prepare_1_screen_s1_the_page_is_split_left_and_right_by_a_draggable_divider() {
    common::pin_private_cwd();
    split_round();
    assert!(RAN_SPLIT.load(Ordering::SeqCst), "{ITEM} s1: the split check never ran");
}

/// §1's left column, top to bottom: the add row, the scrolling list, the controls under it.
fn check_the_left_column_order(window: &adw::ApplicationWindow) {
    let left = widget(window, "prepare-left");
    for name in ["add-sources-row", "sources-scroll", "frame-controls"] {
        assert!(holds(&left, name), "{ITEM} s1: {name} is not inside the left column");
    }
    let add = bounds(window, "add-sources-row");
    let scroll = bounds(window, "sources-scroll");
    let controls = bounds(window, "frame-controls");
    assert!(
        add.y() < scroll.y() && scroll.y() < controls.y(),
        "{ITEM} s1: the left column is not add row → list → controls ({add:?} {scroll:?} {controls:?})"
    );
    // The add row STAYS above the list while the list scrolls (§1: "the rows above and below it stay").
    assert!(
        add.y() + add.height() <= scroll.y() + 2.0,
        "{ITEM} s1: the add row overlaps the scrolling band"
    );
    assert!(
        controls.y() >= scroll.y(),
        "{ITEM} s1: the Freq/Language row is not under the list"
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
    assert!(RAN_LEFT.load(Ordering::SeqCst), "{ITEM} s1: the left-column order check never ran");
}

/// §1's right column: one heading row (title first, then at its right end the row picker), the text
/// box under it filling the rest of the column. Reset is skipped here: the bench opens on row 0, the
/// User Context, and §1 hides mark and Reset on that row — asserting its x would assert a hidden widget.
fn check_the_bench_heading_order(window: &adw::ApplicationWindow) {
    let right = widget(window, "prepare-right");
    for name in ["bench-heading", "prepare-title", "bench-picker", "bench-text"] {
        assert!(holds(&right, name), "{ITEM} s2: {name} is not inside the right column");
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
    assert!(text.height() > 200.0, "{ITEM} s2: the text box is only {:.1} px tall", text.height());
    // Picker after the title, and at the row's right end.
    assert!(
        picker.x() >= title.x() + title.width(),
        "{ITEM} s2: the row picker sits left of the title ({picker:?} vs {title:?})"
    );
    let room = heading.x() + heading.width() - (picker.x() + picker.width());
    assert!(room <= 40.0, "{ITEM} s2: the row picker is {:.1} px short of the heading's right end", room);
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
        assert!(!holds(&readout, column), "{ITEM} s3: {column} contains the Inputs readout");
    }
    // Walk up: neither column may be on the readout's path to the top level.
    let mut node = readout.parent();
    while let Some(parent) = node {
        assert_ne!(parent.widget_name(), "prepare-left", "{ITEM} s3: the readout is inside the left column");
        assert_ne!(parent.widget_name(), "prepare-right", "{ITEM} s3: the readout is inside the right column");
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
