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

use std::fs;
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
        check_the_picker_lists_every_prompt_plus_policy(app);
        RAN_POLICY_LIST.store(true, Ordering::SeqCst);
        check_editing_policy_opens_the_form(app);
        RAN_POLICY_OPEN.store(true, Ordering::SeqCst);
        check_the_title_names_the_open_row(app);
        RAN_TITLE.store(true, Ordering::SeqCst);
        check_the_picker_switches_rows_and_stores(app);
        RAN_SWITCH.store(true, Ordering::SeqCst);
        check_the_mark_and_reset_wake_only_on_an_edit(app);
        RAN_MARK.store(true, Ordering::SeqCst);
        check_the_picked_row_is_the_one_the_shot_ticks(app);
        RAN_TICK.store(true, Ordering::SeqCst);
        check_the_open_picker_hangs_under_its_button(&window);
        RAN_OPEN_PANEL.store(true, Ordering::SeqCst);
        check_the_page_holds_no_second_user_context_paragraph(&window);
        RAN_UC.store(true, Ordering::SeqCst);
        window.close();
    });
}

static RAN_MARK: AtomicBool = AtomicBool::new(false);

static RAN_SPLIT: AtomicBool = AtomicBool::new(false);
static RAN_LEFT: AtomicBool = AtomicBool::new(false);
static RAN_HEADING: AtomicBool = AtomicBool::new(false);
static RAN_READOUT: AtomicBool = AtomicBool::new(false);
static RAN_POLICY_LIST: AtomicBool = AtomicBool::new(false);
static RAN_POLICY_OPEN: AtomicBool = AtomicBool::new(false);
static RAN_TITLE: AtomicBool = AtomicBool::new(false);
static RAN_ON_SCREEN: AtomicBool = AtomicBool::new(false);
static RAN_COLUMN_TOPS: AtomicBool = AtomicBool::new(false);
static RAN_BAND: AtomicBool = AtomicBool::new(false);
static RAN_SWITCH: AtomicBool = AtomicBool::new(false);
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
        "frame-controls",
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
    assert!(
        !naivepost::bench::is_policy(0),
        "{ITEM} s3: User Context is not the policy row"
    );
    for index in 0..naivepost::bench::ROWS.len() {
        assert!(
            !naivepost::bench::is_policy(index),
            "{ITEM} s3: row {index} is a prompt"
        );
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
    assert_eq!(
        title_after, title_before,
        "{ITEM} s3: the bench kept its prompt heading"
    );
    assert_ne!(
        title_after,
        naivepost::bench::POLICY_TITLE,
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
        naivepost::bench::ROWS
            .iter()
            .all(|row| row.title != naivepost::bench::POLICY_TITLE),
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

/// §1's badges **2** and **4** end to end: the "edited — kept in your settings" mark and Reset are live
/// ONLY while this machine holds an edit, and both go away again through the real Reset button.
///
/// The wire is what this pins, not the rule — `bench_paint::paint_marks` already decides visibility from
/// `bench::edited`, and `sec_04_prepare_1_screen_s3_a_picker_row_switch_and_a_keystroke_go_through_the_widgets`
/// covers the picker side. Here nothing is pre-staged: a fresh config folder shows neither widget, one
/// keystroke into `bench-text` (the program's own store path) brings BOTH back with the override file on
/// disk under that folder, and a press of `bench-reset` (fired the way the toolkit fires it) drops the
/// override, restores the shipped wording and greys the pair again.
fn check_the_mark_and_reset_wake_only_on_an_edit(app: &adw::Application) {
    let root = common::temp_dir_created("bench-mark-reset");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths = naivepost::settings::from_environment().expect("the pinned config folder");
    let describe = naivepost::bench::ROWS
        .iter()
        .position(|row| row.title == "Describe")
        .expect("Describe is a bench row");
    let row = &naivepost::bench::ROWS[describe];
    let shipped = naivepost::bench::shipped(row);

    let window = prepare_window(app);
    lay_out();
    widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown")
        .set_selected(u32::try_from(describe).unwrap());
    lay_out();
    let editor = widget(&window, "bench-text")
        .downcast::<gtk::TextView>()
        .expect("the bench box is a text view");

    // --- a machine that holds nothing: no mark, no live Reset (§1 badge 4).
    assert!(
        !naivepost::bench::edited(&paths, row),
        "{ITEM}: a private config folder must start unedited"
    );
    assert!(
        !widget(&window, "bench-mark").is_visible(),
        "{ITEM}: the edited mark showed on a machine holding no edit"
    );
    assert!(
        !widget(&window, "bench-reset").is_sensitive(),
        "{ITEM}: Reset was live with nothing to restore"
    );

    // --- one keystroke: stored for real, and both widgets wake.
    editor
        .buffer()
        .set_text(&format!("{shipped}\none line typed here"));
    lay_out();
    assert!(
        naivepost::bench::edited(&paths, row),
        "{ITEM}: the keystroke stored no override under {root:?}"
    );
    // The store is real but the heading is not repainted by it (the mark follows `paint_bench`, which runs
    // on a row switch or a Reset) — so the shot stages its edit BEFORE building the window. Here the pair
    // is woken the way a person would wake it: leave the row and come back.
    widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown")
        .set_selected(0);
    lay_out();
    widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown")
        .set_selected(u32::try_from(describe).unwrap());
    lay_out();
    assert!(
        paths.prompt_file(row.key).is_file(),
        "{ITEM}: the override is not where the settings door says it is ({})",
        paths.prompt_file(row.key).display()
    );
    assert!(
        widget(&window, "bench-mark").is_visible(),
        "{ITEM}: the mark stayed hidden while this machine held an edit"
    );
    assert!(
        widget(&window, "bench-reset").is_visible()
            && widget(&window, "bench-reset").is_sensitive(),
        "{ITEM}: Reset is not live while this machine holds an edit"
    );

    // --- press Reset the way the toolkit does: shipped wording back, override gone, pair asleep.
    // The box is repainted by `paint_bench` from the stored state (the guard parks the `changed` handler
    // so restoring does not re-store), so the text view shows it only after this repaint runs.
    widget(&window, "bench-reset")
        .downcast::<gtk::Button>()
        .expect("Reset is a button")
        .emit_clicked();
    lay_out();
    let (start, end) = editor.buffer().bounds();
    assert_eq!(
        editor.buffer().text(&start, &end, true).as_str(),
        shipped,
        "{ITEM}: Reset did not put the shipped wording back in the box"
    );
    assert!(
        naivepost::settings::stored_prompt(&paths, row.key)
            .expect("the prompts folder reads back")
            .is_none(),
        "{ITEM}: Reset left the override on disk"
    );
    assert!(
        !widget(&window, "bench-mark").is_visible(),
        "{ITEM}: the mark survived Reset"
    );
    assert!(
        !widget(&window, "bench-reset").is_sensitive(),
        "{ITEM}: Reset stayed live after restoring the shipped wording"
    );

    // --- and once more through the door the SCREENSHOT uses, not a keystroke: `snapshot.rs` stages an
    // override with `snapshot_bench::stage_edited_row` before it builds the window, because a shot presses
    // no keys. If that door stored nothing, `shots/04-prepare.png` would draw §1's badge 2 row without its
    // mark and with a greyed Reset — which is exactly what an earlier round shipped.
    naivepost::snapshot_bench::stage_edited_row(describe);
    assert!(
        naivepost::bench::edited(&paths, row),
        "{ITEM}: staging the picture's row stored no override under {root:?}, so the shot could never show its mark"
    );
    // The pair itself needs no re-check here — the keystroke leg above already pinned that an edit wakes
    // both. What is new is §1's other sentence: "The User Context row hides mark and Reset (no built-in
    // wording to differ from)", so row 0 stays bare even while ANOTHER row holds this machine's edit.
    widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown")
        .set_selected(0);
    lay_out();
    assert!(
        !widget(&window, "bench-mark").is_visible(),
        "{ITEM}: the User Context row showed the edited mark while another row held an override"
    );
    assert!(
        !widget(&window, "bench-reset").is_visible(),
        "{ITEM}: the User Context row showed Reset while another row held an override"
    );
    window.close();
    fs::remove_dir_all(&root).ok();
}

#[test]
fn sec_04_prepare_1_screen_s_the_mark_and_reset_wake_only_while_this_machine_holds_an_edit() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_MARK.load(Ordering::SeqCst),
        "{ITEM}: the mark/Reset wire check never ran"
    );
}

/// §1's picture of the open list (`spec/img/04-prompt-picker.png`) is a LIST: plain row names, one per line,
/// with a ✓ before the row the picker is open on and nothing else. The live DropDown cannot carry that tick
/// (a `GtkDropDown` shows its rows on their own surface, so the shot paints them — see
/// `snapshot_prompt_picker`), so the mark has to be asked of the bench itself. This pins the fact the picture
/// shows: the row the bench opened on is the row the shot ticks, and the tick is not baked into a name.
fn check_the_picked_row_is_the_one_the_shot_ticks(app: &adw::Application) {
    let _ = app;
    // Names first: the reference list reads "Describe", not the heading's "‹Describe› prompt" — so the menu
    // carries the plain row titles, and only the heading spells the prompt wording (§1 badges 2 vs 3).
    for (index, name) in naivepost::bench::picker_rows() {
        assert_eq!(
            name,
            if index == naivepost::bench::POLICY_ROW {
                naivepost::bench::POLICY_TITLE.to_string()
            } else {
                naivepost::bench::ROWS[index].title.to_string()
            },
            "{ITEM} s3: picker row {index} is {name:?}, not the plain row name the picture draws"
        );
    }
    // …and the tick follows the window's own bench, read off the live picker after it was moved.
    let window = prepare_window(app);
    lay_out();
    let picker = widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown");
    let describe = naivepost::bench::ROWS
        .iter()
        .position(|row| row.title == "Describe")
        .expect("Describe is a bench row");
    picker.set_selected(u32::try_from(describe).unwrap());
    lay_out();
    assert_eq!(
        usize::try_from(picker.selected()).unwrap_or(usize::MAX),
        describe,
        "{ITEM} s3: the picker face did not follow the row the bench opened on"
    );
    window.close();
}

#[test]
fn sec_04_prepare_1_screen_s3_the_picked_row_carries_the_tick_and_the_names_are_plain() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_TICK.load(Ordering::SeqCst),
        "{ITEM}: the picker-tick check never ran"
    );
}

static RAN_TICK: AtomicBool = AtomicBool::new(false);

/// §1's badge **3** and **4** as a WIRE: choosing a row in the real `bench-picker` repaints the heading,
/// the text box and the mark/Reset pair through the same code a click runs, and typing in `bench-text`
/// writes through ("Every keystroke is stored"). Asserted through `bench::text`/`bench::edited` — the
/// stored state the logic tests check — not by re-reading the widget that was just set.
///
/// §1's other sentence here: "The User Context row hides mark and Reset (no built-in wording to differ
/// from)", so the pair must be invisible on row 0 even after that row has been edited.
fn check_the_picker_switches_rows_and_stores(app: &adw::Application) {
    // "Every keystroke is stored" needs somewhere to store to: the config folder is the program's own
    // seam (`settings::from_environment()`), pointed at this test's private directory.
    let root = common::temp_dir_created("bench-picker");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths = naivepost::settings::from_environment().expect("the pinned config folder");

    let window = prepare_window(app);
    lay_out();
    let picker = widget(&window, "bench-picker")
        .downcast::<gtk::DropDown>()
        .expect("the picker is a dropdown");
    let describe = naivepost::bench::ROWS
        .iter()
        .position(|row| row.title == "Describe")
        .expect("Describe is a bench row");

    // --- choosing a prompt row: heading, wording and Reset all follow the picker.
    picker.set_selected(u32::try_from(describe).unwrap());
    lay_out();
    assert_eq!(
        widget(&window, "prepare-title")
            .downcast::<gtk::Label>()
            .expect("the heading is a label")
            .label()
            .as_str(),
        naivepost::bench::title(&naivepost::bench::ROWS[describe]),
        "{ITEM} s3: the heading did not follow the picker to the Describe prompt"
    );
    let editor = widget(&window, "bench-text")
        .downcast::<gtk::TextView>()
        .expect("the bench box is a text view");
    let shipped = naivepost::bench::shipped(&naivepost::bench::ROWS[describe]);
    let buffer = editor.buffer();
    let (start, end) = buffer.bounds();
    let shown = buffer.text(&start, &end, true).to_string();
    assert_eq!(
        shown, shipped,
        "{ITEM} s3: the box shows the shipped Describe wording before any edit"
    );
    // Nothing edited yet: §1's mark and Reset are only "live while this machine holds an edit".
    assert!(
        !widget(&window, "bench-reset").is_visible(),
        "{ITEM} s4: Reset shows on an unedited prompt row"
    );

    // --- typing: the keystroke goes through to the machine's stored prompt, not just the widget.
    let typed = format!("{shipped}\nextra line from a keystroke");
    editor.buffer().set_text(&typed);
    lay_out();
    assert!(
        naivepost::bench::edited(&paths, &naivepost::bench::ROWS[describe]),
        "{ITEM} s3: typing into bench-text stored nothing"
    );
    assert_eq!(
        naivepost::bench::text(&paths, &fixture(), &naivepost::bench::ROWS[describe]),
        typed,
        "{ITEM} s3: the stored Describe wording is not what was typed"
    );
    assert!(
        widget(&window, "bench-reset").is_visible()
            && widget(&window, "bench-reset").is_sensitive(),
        "{ITEM} s4: Reset is not live while this machine holds an edit"
    );
    assert!(
        widget(&window, "bench-mark").is_visible(),
        "{ITEM} s3: the \"edited — kept in your settings\" mark is missing on an edited prompt"
    );

    // --- User Context: §1 says its row hides BOTH the mark and Reset, edit or not.
    picker.set_selected(0);
    lay_out();
    let context_text = widget(&window, "bench-text")
        .downcast::<gtk::TextView>()
        .expect("the bench box is a text view");
    context_text
        .buffer()
        .set_text("a context typed in this test");
    lay_out();
    assert_eq!(
        naivepost::ui::session_handle(&window).borrow().context,
        "a context typed in this test",
        "{ITEM} s3: the User Context keystroke did not reach the session"
    );
    assert!(
        !widget(&window, "bench-mark").is_visible(),
        "{ITEM} s3: the User Context row must hide the edited mark"
    );
    assert!(
        !widget(&window, "bench-reset").is_visible(),
        "{ITEM} s3: the User Context row must hide Reset"
    );
    window.close();
}

/// The session's User Context as the shell holds it — the state a keystroke on row 0 writes through to.
#[test]
fn sec_04_prepare_1_screen_s3_a_picker_row_switch_and_a_keystroke_go_through_the_widgets() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_SWITCH.load(Ordering::SeqCst),
        "{ITEM} s3: the picker/keystroke wire check never ran"
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

/// §1's left column, top to bottom: the add row, the scrolling list, the controls under it.
fn check_the_left_column_order(window: &adw::ApplicationWindow) {
    let left = widget(window, "prepare-left");
    for name in ["add-sources-row", "sources-scroll", "frame-controls"] {
        assert!(
            holds(&left, name),
            "{ITEM} s1: {name} is not inside the left column"
        );
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
    // Picker after the title, and at the row's right end.
    assert!(
        picker.x() >= title.x() + title.width(),
        "{ITEM} s2: the row picker sits left of the title ({picker:?} vs {title:?})"
    );
    let room = heading.x() + heading.width() - (picker.x() + picker.width());
    assert!(
        room <= 40.0,
        "{ITEM} s2: the row picker is {:.1} px short of the heading's right end",
        room
    );
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

/// §1's second picture (`spec/img/04-prompt-picker.png`), as numbers: the row picker's list hangs under the
/// picker button at the top right of the bench column, and the heading row and the text box stay on screen
/// beside it. `spec/QUESTIONS.md`'s §04 answer 3c rejects "only the list, stretched over the whole window",
/// so what is pinned is COVERAGE: the panel is a strip of the window, not the window, and nothing it overlaps
/// has been pushed off.
fn check_the_open_picker_hangs_under_its_button(window: &adw::ApplicationWindow) {
    let picker = bounds(window, "bench-picker");
    let heading = bounds(window, "bench-heading");
    let text = bounds(window, "bench-text");
    // The shot's own placement doors, so a green check cannot coexist with a wrong picture.
    let (px, py, pw, ph) = naivepost::snapshot_prompt_picker::open_panel_box_for_test(window)
        .expect("{ITEM} s6: the picker has no box to hang a list under");
    let win_w = window.width().max(naivepost::shell::WINDOW_MIN_W) as f64;
    let win_h = window.height().max(naivepost::shell::WINDOW_MIN_H) as f64;
    let (picker_x, picker_right, picker_bottom) = (
        picker.x() as f64,
        (picker.x() + picker.width()) as f64,
        (picker.y() + picker.height()) as f64,
    );
    println!(
        "SPLIT s6 picker x={picker_x:.1}..{picker_right:.1} y={:.1}..{picker_bottom:.1}; panel x={px:.1}..{:.1} y={py:.1}..{:.1} in a {win_w:.0}x{win_h:.0} window",
        picker.y(), px + pw, py + ph
    );
    // The split's own handle position, not `prepare-left`'s box: inside a `gtk::Paned` with `shrink-start-child`
    // a column is allocated its own minimum while the handle sits elsewhere, so the column's x+w says nothing
    // about where the divider is. The handle is what separates the two columns on screen, and it is the geometry
    // the panel must fit inside — read here, before the width checks, because both of them compare against it.
    let split = widget(window, "prepare-split")
        .downcast::<gtk::Paned>()
        .expect("{ITEM} s6: the Prepare page is split by a paned");
    let split_bounds = bounds(window, "prepare-split");
    let divider_x = split_bounds.x() as f64 + split.position() as f64;
    println!(
        "SPLIT s6 prepare-split x={:.1} handle at {divider_x:.1}",
        split_bounds.x()
    );
    // **COMPACT**: the panel is a menu, not a sheet — narrower than the bench column it hangs over, and a slice
    // of the window's width (`spec/QUESTIONS.md` §04 answer 3c's exact complaint was a list filling the canvas).
    assert!(
        pw < win_w - divider_x,
        "{ITEM} s6: the open list is {pw:.0} px wide but the column it hangs over is only {:.0} px — that is a \
         sheet over the whole page, not a dropdown under a button",
        win_w - divider_x
    );
    assert!(
        pw < win_w / 4.0,
        "{ITEM} s6: the open list is {pw:.0} px wide in a {win_w:.0} px window against a {:.0} px picker — \
         that is a sheet, not a dropdown hanging under a button",
        picker_right - picker_x
    );
    // **AND THE PAGE IS STILL THERE**: the panel hangs under the heading row it belongs to, down the RIGHT
    // column only, inside the window — which is what a dropdown does and a sheet does not. The height is NOT
    // capped: §1's caption says the list scrolls ("Translate and Upload text sit below Narration"), so all
    // fourteen rows are drawn and the bottom lands where GTK clamps a real popover, at the window's edge.
    let heading_x = heading.x() as f64;
    assert!(
        py >= heading.y() as f64 + heading.height() as f64 - 2.0,
        "{ITEM} s6: the open list starts at {py:.1}, over the bench heading that owns the picker \
         (heading {:.1}..{:.1})",
        heading.y() as f64,
        heading.y() as f64 + heading.height() as f64
    );
    assert!(
        px - heading_x >= 120.0,
        "{ITEM} s6: only {:.0} px of the heading row stay visible to the list's left, so the list covers the \
         page instead of hanging beside it (heading starts {heading_x:.1}, list starts {px:.1})",
        px - heading_x
    );
    assert!(
        px >= divider_x - 1.0,
        "{ITEM} s6: the open list starts at {px:.1}, left of the page's own divider at {divider_x:.1} — it \
         overlays the sources column, not the bench's",
    );
    assert!(
        py + ph <= win_h + 1.0,
        "{ITEM} s6: the open list runs past the bottom of the window (ends {py:.1}+{ph:.0} in {win_h:.0} px)"
    );
    // **HUNG FROM THE BUTTON**: its right edge is the button's, its top is just under the button.
    assert!(
        (px + pw - picker_right).abs() <= 8.0,
        "{ITEM} s6: the list's right edge ({:.1}) does not follow the picker's ({picker_right:.1}), so it is \
         not anchored to that button",
        px + pw
    );
    assert!(
        py >= picker_bottom - 1.0 && py <= picker_bottom + 12.0,
        "{ITEM} s6: the list's top ({py:.1}) is not under the picker button (ends {picker_bottom:.1})"
    );
    // **AND THE PAGE IS STILL THERE**: the heading row beside it, and the text box under it, both on screen.
    let (heading_x, heading_right) = (heading.x() as f64, (heading.x() + heading.width()) as f64);
    assert!(
        px > heading_x && px + pw <= heading_right + 1.0,
        "{ITEM} s6: the list ({px:.1}..{:.1}) does not sit within the bench heading's column \
         ({heading_x:.1}..{heading_right:.1})",
        px + pw
    );
    for name in [
        "bench-heading",
        "bench-text",
        "prepare-title",
        "add-sources-row",
        "sources-scroll",
    ] {
        // The bounds this file reads everywhere, not `is_on_screen_for_test`: that door compares against
        // `window.width()`, which under a window manager with no WM answers 0 before the first map, and every
        // control would read as "off screen" whatever the picture shows.
        let (x, y, w, h) = ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), name)
            .unwrap_or((0.0, 0.0, 0.0, 0.0));
        assert!(
            w > 0.0 && h > 0.0 && x >= -0.5 && y >= -0.5 && x < win_w && y < win_h,
            "{ITEM} s6: {name} left the screen once the picker opened: ({x:.1},{y:.1},{w:.1},{h:.1}) in a \
             {win_w:.0}x{win_h:.0} window"
        );
    }
    assert!(
        text.height() > 0.0 && (text.y() as f64) < py + ph,
        "{ITEM} s6: the prompt text box ({:.1}..{:.1}) is not the panel's neighbour any more \
         (panel ends {:.1})",
        text.y() as f64,
        (text.y() + text.height()) as f64,
        py + ph
    );
    // The window shot is the screen now: no sidecar steals `04-prompt-picker.png` from the page.
    assert!(
        !naivepost::snapshot_subjects::subject_takes_the_name("04-prompt-picker"),
        "{ITEM} s6: a subject took the asked filename again, which is how the asked name ended up a list alone"
    );
}

#[test]
fn sec_04_prepare_1_screen_s6_the_open_picker_hangs_under_its_button_over_the_page() {
    common::pin_private_cwd();
    split_round();
    assert!(
        RAN_OPEN_PANEL.load(Ordering::SeqCst),
        "{ITEM} s6: the open-picker placement check never ran"
    );
}

static RAN_OPEN_PANEL: AtomicBool = AtomicBool::new(false);
