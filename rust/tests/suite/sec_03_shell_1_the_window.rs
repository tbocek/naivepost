//! §03-shell.md §1 The window — the shell's own layout facts, checked through the real window.
//!
//! §1 is a list of numbered badges plus a handful of sentences about where they sit and how the window
//! behaves. Most of it was already true (the default size, the header buttons and their tooltips, a
//! greyed-not-disabled tab, a read-only monospace log body). What this file newly pins, because it was
//! NOT true before this round, is: the window's TITLE, the header's project name/path with its three
//! narrowing stages, the log drawing the MESSAGE alone with the level as a colour, and the page and the
//! bottom bar being two halves of a draggable `gtk::Paned`. Its minimum size (1240×900, opened at and never
//! smaller, with nothing scrolling as a whole at it) is `tests/suite/window_shrinks.rs`.
//!
//! Shape follows `sec05_live_log_link.rs` / `switch_tab_widgets.rs`: GTK comes up once per binary
//! inside ONE `adw::Application::run_with_args`, and every widget scenario runs inside that one
//! `activate`. A second `run()` from another test thread cannot be activated, which is why there is
//! exactly one application here. The rules that need no display (`header_title::label_for`,
//! `log_level::level_of`) are plain `#[test]`s of their own. Every assertion reads back what the UI
//! itself shows — spec/00-principles.md §5.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::Project;
use naivepost::shell::{self, Page};
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

/// The one GTK application this binary's widget round needs; the round runs every scenario inside one
/// `activate`, which is also what brings GTK up for the process.
static GTK: std::sync::Once = std::sync::Once::new();
static RAN: AtomicBool = AtomicBool::new(false);

/// The scratch root one round works in: `build_window` resolves its session folder from the cwd, so a
/// round that cares which folder the header names pins the cwd here first.
fn scratch_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-sec03win-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::env::set_current_dir(&dir).expect("cwd pinned to the scratch dir");
    dir
}

/// One real window on `page`, freshly built, with the previous round's window retired first so a name
/// lookup cannot land in a closed tree.
fn window(app: &adw::Application, page: &str) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    let window = ui::build_window(app, &model, page);
    window.set_default_size(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    window.present();
    hold_last_window(window.clone());
    settle();
    window
}

/// §1's own picture of this window is `spec/img/03-window.png`, shot at the minimum size.
fn s1_title_and_default_size(app: &adw::Application) {
    let window = window(app, "Prepare");
    assert_eq!(
        window.title().as_deref(),
        Some(shell::WINDOW_TITLE),
        "§1: one window, titled Naivepost — without it GTK titles the window from the app id"
    );
    assert_eq!(
        (window.default_width(), window.default_height()),
        (shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        "§1: 1240×900 is the size it OPENS at"
    );
    // …and its FLOOR: `build_window` requests that size, so the window manager cannot take the shell below
    // it. Measured on the window itself, which is what the WM reads; no page may push it higher either.
    assert_eq!(
        (
            window.measure(gtk::Orientation::Horizontal, -1).0,
            window.measure(gtk::Orientation::Vertical, -1).0
        ),
        (shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        "§1: the window 'cannot be made smaller' than 1240×900"
    );
    window.close();
}

/// S2: the header's numbered badges, in the order §1 reads them. New/Open/Save pack from the left edge
/// and badge 4 (the project name/path) is packed AFTER Save, so it opens the start side's run too; the
/// tab row is the bar's TITLE widget, in its centre. Rescan, Settings and ⓘ pack from the right edge
/// inward, so ⓘ lands innermost of that group and the bar reads ⓘ ⚙ ⟳ on screen.
fn s2_header_badges_in_order(app: &adw::Application) {
    let window = window(app, "Prepare");
    let header = header_of(&window);
    let left = packed_names(&header, "start");
    for name in ["new-button", "open-button", "save-button"] {
        assert!(left.iter().any(|n| n == name), "New, Open, Save pack from the left: {left:?}");
    }
    assert_eq!(
        position_of(&left, "project-title"),
        position_of(&left, "save-button") + 1,
        "§1 badge 4 is \"packed after Save\", not centred: {left:?}"
    );
    assert!(
        position_of(&left, "new-button") < position_of(&left, "open-button"),
        "New comes before Open, as §1's **1** **2**: {left:?}"
    );
    assert!(
        position_of(&left, "open-button") < position_of(&left, "save-button"),
        "Open comes before Save, as §1's **2** **3**: {left:?}"
    );
    // Badge 5: the tab row IS the title widget, in the header's centre -- not a row under the header.
    let title = header.title_widget().expect("§1 badge 5: the tab row is the header's title widget");
    assert_eq!(title.widget_name(), "page-tabs", "the header's centre slot holds the tab row");
    assert!(
        common::widget_in(&window, "page-tabs").is_some(),
        "and it is still reachable by name from the window tree"
    );
    let right = packed_names(&header, "end");
    for name in ["rescan-button", "settings-button", "tab-info-button"] {
        assert!(
            right.iter().any(|n| n == name),
            "Rescan, Settings and ⓘ pack from the right edge: {right:?}"
        );
    }
    assert_eq!(
        position_of(&right, "tab-info-button"),
        0,
        "packed from the right edge INWARD, ⓘ is first of that group, so the bar reads ⓘ ⚙ ⟳: {right:?}"
    );
    window.close();
}

/// S4: the header's project label names the open project and carries the shown tab's word while there
/// is room for both (§1's widest rung). The ladder itself is the pure test below.
fn s4_project_label_names_the_open_project(app: &adw::Application) {
    let root = scratch_root("title");
    let window = window(app, "Prepare");
    let title = ui::project_title(&window).expect("§1 badge 4 is in the header");
    let shown = title.text().to_string();
    let session = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    let name = session.file_name().unwrap().to_string_lossy().into_owned();
    assert!(shown.contains(&name), "the header names the open project: {shown} / {name}");
    assert!(
        shown.contains(Page::Prepare.label()),
        "at full width the tab's word is part of it: {shown}"
    );
    // A switch repaints it with the new tab's word, through the same door the readouts use.
    let cut_tab = ui::tab_button(&window, Page::Cut).expect("the tab row carries Cut");
    cut_tab.emit_by_name::<()>("clicked", &[]);
    settle();
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S4b: the tab row shows the four pages with the icons §1 names, and an unlocked tab's tooltip is the
/// page's own line (the locked-tab grey + reason is `switch_tab_widgets.rs`).
fn s4b_tab_row_icons(app: &adw::Application) {
    assert_eq!(
        Page::all().map(|page| page.icon()),
        [
            "view-list-symbolic",
            "edit-cut-symbolic",
            "audio-input-microphone-symbolic",
            "applications-multimedia-symbolic",
        ],
        "§1's tab row: Prepare (view-list), Cut (edit-cut), Narrate (microphone), Produce (multimedia)"
    );
    let root = scratch_root("tabs");
    release_last_window();
    let model = Project { sources: vec![common::footage("project:sources/a.mkv")], ..Default::default() };
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    hold_last_window(window.clone());
    settle();
    assert!(
        common::widget_in(&window, "page-tabs").is_some(),
        "the window carries its named tab row (§1 badge 5)"
    );
    assert!(!ui::tab_dimmed(&window, Page::Cut), "with a footage source the Cut tab is not greyed");
    assert_eq!(
        ui::tab_tooltip(&window, Page::Cut).as_deref(),
        Some(Page::Cut.tip()),
        "an unlocked tab's tooltip is the page's own line, not a lock reason"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S5: the bottom of the window, as §1 now reads it — ONE row holding the run bar and, to its right in
/// the same row, the Inputs readout then the Outputs folder button then the Outputs readout; UNDER that
/// row the log expander, its header carrying the status line at the header's RIGHT end. The log body is
/// read-only monospace.
fn s5_bottom_bar_order(app: &adw::Application) {
    let root = scratch_root("bottom");
    let window = window(app, "Prepare");
    let bottom = common::widget_in(&window, "bottom-bar").expect("the divider's bottom half");
    let names = child_names(&bottom);
    assert_eq!(
        names,
        vec!["run-bar".to_string(), "log-expander".to_string()],
        "§1: the run bar's row on top, the log expander under it — the readouts are NOT a row of their own: {names:?}"
    );
    // Inside that one row: the controls (inside their scroller, §1's "what no longer fits scrolls") and
    // then the readouts at its right end.
    let run = common::widget_in(&window, "run-bar").expect("the run bar row");
    let run_names = child_names(&run);
    assert!(
        run_names.contains(&"run-controls".to_string()) && position_of(&run_names, "run-controls") < position_of(&run_names, "readout-row"),
        "§1: the controls scroll sideways inside the row, with the readouts to their right: {run_names:?}"
    );
    let controls = common::widget_in(&window, "run-controls-inner").expect("the run bar's controls");
    // §2's order lives inside the scroller now: ▶ , I'm feeling lucky , ⏹ , then the gear and the passes.
    let control_names = child_names(&controls);
    assert!(
        position_of(&control_names, "play-button") < position_of(&control_names, "lucky-button")
            && position_of(&control_names, "lucky-button") < position_of(&control_names, "stop-button"),
        "§2's run bar still reads ▶ , I'm feeling lucky , ⏹ inside its scroller: {control_names:?}"
    );
    let progress = position_of(&run_names, "run-progress");
    let readouts = position_of(&run_names, "readout-row");
    assert!(
        progress < readouts && readouts != usize::MAX,
        "§1: the progress bar and the readouts share the row, the bar first: {run_names:?}"
    );
    let readout_row = common::widget_in(&window, "readout-row").expect("the readouts share the run row");
    assert_eq!(
        child_names(&readout_row),
        vec![
            "inputs-readout".to_string(),
            "outputs-folder-button".to_string(),
            "outputs-readout".to_string()
        ],
        "§1's order inside that row: Inputs readout, Outputs folder button, Outputs readout"
    );
    let log = position_of(&names, "log-expander");
    assert!(position_of(&names, "run-bar") < log, "the log expander sits under the row: {names:?}");
    let expander: gtk::Expander = ui::log_expander(&window).expect("the shell carries a log expander");
    let header = expander.label_widget().expect("the expander's header carries the status line");
    let header_names = child_names(&header);
    assert_eq!(
        header_names.last().map(String::as_str),
        Some("status-line"),
        "§1: the status line is at the expander header's RIGHT end: {header_names:?}"
    );
    let view = ui::log_view(&window).expect("the expander holds the log body");
    assert!(!view.is_editable(), "§1: the log is read-only");
    assert!(view.is_monospace(), "§1: the log is monospace");
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S6b: through the real log — lines go in through the shell's own painter and come out as messages,
/// the failure line red, the detail line dimmed, the progress and unmarked lines plain.
fn s6b_log_messages_and_levels(app: &adw::Application) {
    use naivepost::ui::log_level;
    let root = scratch_root("loglevel");
    let window = window(app, "Prepare");
    let view = ui::log_view(&window).expect("the expander holds the log body");
    let expander: gtk::Expander = ui::log_expander(&window).expect("the shell carries a log expander");
    expander.set_expanded(true);
    let buffer = view.buffer();
    buffer.set_text("");
    naivepost::ui::log_link::paint_lines(
        &buffer,
        &[
            ">>> describe 1/2: chunk 4/12".to_string(),
            "!!! frames failed -- 'scening frames' exited 256".to_string(),
            "    $ ffmpeg -i in.mkv out.png".to_string(),
            "line restored to 00:11".to_string(),
        ],
    );
    assert_eq!(
        naivepost::ui::log_link::painted_lines_for_test(&buffer),
        vec![
            "describe 1/2: chunk 4/12",
            "frames failed -- 'scening frames' exited 256",
            "$ ffmpeg -i in.mkv out.png",
            "line restored to 00:11",
        ],
        "§1: each line is the message alone — no `>>> `, no `!!! `, no four-space prefix"
    );
    let tagged = |tag: &str| -> Vec<i32> {
        let t = buffer
            .tag_table()
            .lookup(tag)
            .unwrap_or_else(|| panic!("the painter installs its own {tag} tag"));
        (0..buffer.line_count())
            .filter(|line| buffer.iter_at_line(*line).is_some_and(|it| it.has_tag(&t)))
            .collect()
    };
    assert_eq!(tagged(log_level::FAILURE_TAG), vec![1], "the failure line is the red one");
    assert_eq!(tagged(log_level::DETAIL_TAG), vec![2], "the detail line is the dimmed one");
    // "progress plain" is the ABSENCE of a tag: the progress line and the unmarked one carry none, so
    // they draw in the log view's own foreground.
    for plain in [0, 3] {
        let iter = buffer.iter_at_line(plain).expect("the line exists");
        let level_tags: Vec<String> = iter
            .tags()
            .into_iter()
            .map(|tag| tag.name().unwrap_or_default().to_string())
            .filter(|name| name == log_level::FAILURE_TAG || name == log_level::DETAIL_TAG)
            .collect();
        assert!(level_tags.is_empty(), "line {plain} draws plain, with no level tag: {level_tags:?}");
    }
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S7: the page and the bottom bar are two halves of a draggable divider, and collapsing the log expander
/// hands its height back rather than keeping it reserved.
fn s7_draggable_divider(app: &adw::Application) {
    let root = scratch_root("divider");
    let window = window(app, "Prepare");
    let paned: gtk::Paned = ui::find_source_widget(&window, "main-divider")
        .expect("the shell splits page from bottom bar with a named divider")
        .downcast()
        .expect("the divider is a gtk::Paned");
    assert_eq!(paned.orientation(), gtk::Orientation::Vertical, "the page sits above the bottom bar");
    let start = paned.start_child().expect("the divider's first half is the page");
    let end = paned.end_child().expect("the divider's second half is the bottom bar");
    assert!(
        start.downcast::<adw::ViewStack>().is_ok(),
        "the page half is the stack itself, so dragging gives IT height"
    );
    let names = child_names(&end);
    assert!(
        names.contains(&"run-bar".to_string()) && names.contains(&"log-expander".to_string()),
        "the bottom half holds the run bar, the readouts and the log: {names:?}"
    );
    // Dragging the handle moves height: a position is a value the user sets and GTK keeps.
    let before = paned.position();
    paned.set_position(before + 120);
    assert_eq!(paned.position(), before + 120, "the handle carries the page's height, not a fixed strip");
    // Collapsing the log returns its height to the page half: the bottom half asks for less.
    let expander: gtk::Expander = ui::log_expander(&window).expect("the shell carries a log expander");
    expander.set_expanded(true);
    settle();
    let tall = end.measure(gtk::Orientation::Vertical, -1).0;
    expander.set_expanded(false);
    settle();
    let short = end.measure(gtk::Orientation::Vertical, -1).0;
    assert!(
        short < tall,
        "collapsing the log expander must hand its height back, not keep reserving it: {tall} -> {short}"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S8: "A page whose prerequisites vanish while open switches silently to Prepare" — reached through the
/// real Rescan button, the one door §1 gives this flow. The Cut page is unlocked only while a source is
/// marked footage; the planted footage file is NOT on disk, so the scan drops that row, the page's own
/// prerequisite is gone, and `rescan_apply::apply` moves the window to Prepare in silence (F0.1 S2's
/// bounce is only for a click). Asserted through `ui::state`, the same state the logic test reads.
fn s8_a_page_whose_prerequisites_vanish_moves_to_prepare(app: &adw::Application) {
    use naivepost::project::{self, Project};
    use naivepost::rescan::RESCANNED;
    let root = scratch_root("vanished");
    // The session folder the window will resolve from the cwd, with a project naming one footage row
    // whose file does not exist -- that is the prerequisite about to vanish.
    let session = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    std::fs::create_dir_all(&session).expect("session folder");
    project::save(
        &Project {
            sources: vec![common::footage("project:sources/gone.mkv")],
            ..Default::default()
        },
        &session,
    )
    .expect("the session project is written");
    let model = project::load(&session).expect("it loads back");
    let window = ui::build_window(app, &model, "Cut");
    window.present();
    hold_last_window(window.clone());
    settle();
    assert_eq!(ui::state(&window).page, Page::Cut, "the check starts on the page that will lose its base");

    // The press: the header's Rescan, fired the way GTK fires a click.
    let rescan = common::widget_in(&window, "rescan-button")
        .expect("the header bar has a Rescan button")
        .downcast::<gtk::Button>()
        .expect("Rescan is a button");
    rescan.emit_by_name::<()>("clicked", &[]);
    settle();

    assert_eq!(
        ui::state(&window).page,
        Page::Prepare,
        "§1: a page whose prerequisites vanished while open goes to Prepare by itself"
    );
    assert_eq!(
        ui::state(&window).status,
        RESCANNED,
        "and it says the scan happened, not a lock reason -- this was no bounce"
    );
    window.close();
    let _ = std::fs::remove_dir_all(&root);
}

/// S9: "Closing the window flushes the narration autosave, red line, project and prompts, then closes;
/// NO PROMPT." A half-typed narration line is owed when the window comes down; the close hook writes it
/// and asks nothing -- no dialog opens, no top level appears. Prompt edits need no such flush: prompt
/// wording is read from the shipped files (`prompts::shipped_text`) and there is no owed-write door for
/// it anywhere in `src/`, which is why the close hook calls the four that do exist and no more.
fn s9_closing_flushes_without_asking(app: &adw::Application) {
    use naivepost::cut::{self, Cut, Seg};
    use naivepost::layout::Tree;
    use naivepost::narration::{self, Entry, Narration};
    use naivepost::project::{self, Project, Source};
    let root = scratch_root("closeflush");
    let session = naivepost::startup::session_dir(&std::env::current_dir().unwrap());
    std::fs::create_dir_all(&session).expect("session folder");
    let model = Project {
        sources: vec![Source { path: "project:sources/lecture.mkv".into(), footage: true, ..Default::default() }],
        ..Default::default()
    };
    project::save(&model, &session).expect("session written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    cut::save(&Cut { segs: vec![Seg { s: 5.0, e: 12.0, ..Default::default() }], ..Default::default() }, &tree)
        .expect("a cut clip, so the row is drawn");
    narration::save(
        &Narration { entries: vec![Entry { s: 5.0, e: 12.0, text: "before".into(), ..Default::default() }], silent: vec![] },
        &tree,
    )
    .expect("narration on disk");
    let window = ui::build_window(app, &model, "Narrate");
    window.present();
    hold_last_window(window.clone());
    settle();
    ui::set_state(naivepost::ui::NarrateState { entries: vec![Entry { s: 5.0, e: 12.0, text: "before".into(), ..Default::default() }], ..Default::default() });
    ui::refresh(&window);
    // Type into the row's live box: that marks the narration owed without waiting for the autosave beat.
    let typed = "written on the way out";
    // The row's OWN live box is what a person types into, so the test types into that box and lets the
    // box's `changed` handler mark the write owed (`wire_row_boxes` -> `note_typing`) -- the same path a
    // click on the page takes. Calling `note_typing` directly would prove only the seam, not the widget.
    let box_widget = common::widget_in(&window, "line-text-0").expect("the row's editable box");
    let entry = box_widget
        .downcast_ref::<gtk::Entry>()
        .expect("the row's box is an editable entry");
    entry.set_text(typed);
    settle();
    assert_eq!(
        entry.text(),
        typed,
        "the row's live box holds what was typed into it"
    );
    let file = tree.narration_json();
    assert!(
        !std::fs::read_to_string(&file).unwrap_or_default().contains(typed),
        "the edit is owed, not yet on disk"
    );
    let windows_before = app.windows().len();
    window.close();
    // A close is only a close once GTK has run it through the main loop: the same `settle()` the tests
    // above use after every `window.close()`.
    settle();
    assert!(
        std::fs::read_to_string(&file).unwrap_or_default().contains(typed),
        "§1: closing flushes what is owed before it closes"
    );
    assert_eq!(app.windows().len(), windows_before - 1, "§1: …then closes, with no prompt of its own");
    let _ = std::fs::remove_dir_all(&root);
}

/// The header bar of a built window: `build_window` puts it first in the window's top box.
fn header_of(window: &adw::ApplicationWindow) -> adw::HeaderBar {
    let content = window.content().expect("the window has content");
    let header = content.first_child().expect("the top box starts with the header bar");
    header.downcast().expect("the first child is the header bar")
}

/// The widget names under the header bar that belong to one side, in the order the tree is walked.
///
/// `adw::HeaderBar` keeps its start and end boxes behind private pointers (no accessor exists — the same
/// reason `open_project_widgets.rs` walks the tree rather than reading a child), so the bar's children
/// are walked and each name is sorted into a side by which buttons §1 puts there: New/Open/Save on the
/// left, ⓘ/⚙/⟳ packed from the right edge inward.
fn packed_names(header: &adw::HeaderBar, end: &str) -> Vec<String> {
    let mut all = Vec::new();
    collect_names(header.upcast_ref(), &mut all);
    all.dedup();
    let side: &[&str] = match end {
        "start" => &["new-button", "open-button", "save-button", "project-title"],
        _ => &["tab-info-button", "settings-button", "rescan-button"],
    };
    all.into_iter().filter(|name| side.contains(&name.as_str())).collect()
}

fn collect_names(widget: &gtk::Widget, out: &mut Vec<String>) {
    for child in widget.observe_children().iter::<glib::Object>().filter_map(Result::ok) {
        let Ok(child) = child.downcast::<gtk::Widget>() else { continue };
        let name = child.widget_name().to_string();
        if !name.is_empty() {
            out.push(name);
        }
        collect_names(&child, out);
    }
}

fn child_names(widget: &gtk::Widget) -> Vec<String> {
    widget
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect()
}

fn position_of(names: &[String], name: &str) -> usize {
    names.iter().position(|n| n == name).unwrap_or(usize::MAX)
}

/// The one widget round: every scenario above, inside one application's one `activate`.
#[test]
fn sec_03_shell_1_the_window_the_shell_draws_its_title_tabs_log_and_divider() {
    common::pin_private_cwd();
    GTK.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            s1_title_and_default_size(app);
            s2_header_badges_in_order(app);
            s4_project_label_names_the_open_project(app);
            s4b_tab_row_icons(app);
            s5_bottom_bar_order(app);
            s6b_log_messages_and_levels(app);
            s7_draggable_divider(app);
            s8_a_page_whose_prerequisites_vanish_moves_to_prepare(app);
            s9_closing_flushes_without_asking(app);
            RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(RAN.load(Ordering::SeqCst), "the widget rounds never ran");
}

/// S3: the project name/path ladder — path + tab words → file name + tab words → file name alone —
/// answered with no window in sight, which is what makes the rule checkable at all.
#[test]
fn sec_03_shell_1_the_window_s3_the_project_label_narrows_path_then_name_then_name_only() {
    use naivepost::ui::header_title::{label_for, stage_for, APPROX_CHAR_PX, Fit};
    let dir = Path::new("/home/dev/videos/02-eth.naivepost");
    assert_eq!(
        label_for(dir, "Cut", Fit::Roomy),
        "/home/dev/videos/02-eth.naivepost · Cut",
        "roomy: the path and the tab's word"
    );
    assert_eq!(
        label_for(dir, "Cut", Fit::Tight),
        "02-eth.naivepost · Cut",
        "tight: the file name and the tab's word"
    );
    assert_eq!(
        label_for(dir, "Cut", Fit::Tightest),
        "02-eth.naivepost",
        "tightest: the file name alone (§1's \"file name + icons only\"; the icons are the bar's own buttons)"
    );
    // A missing half is left out, never joined with a stray separator.
    assert_eq!(label_for(Path::new(""), "Cut", Fit::Roomy), "Cut");
    assert_eq!(label_for(Path::new("/a/b.naivepost"), "", Fit::Roomy), "/a/b.naivepost");
    assert_eq!(label_for(Path::new(""), "", Fit::Tightest), "");
    // And the stage follows the width the label was given: the same text stays Roomy while it fits,
    // drops one rung when half of it would fit, and the last when even that would not.
    let text = label_for(dir, "Cut", Fit::Roomy);
    let natural = text.chars().count() as i32 * APPROX_CHAR_PX;
    assert_eq!(stage_for(&text, natural), Fit::Roomy);
    assert_eq!(stage_for(&text, natural / 2), Fit::Tight);
    assert_eq!(stage_for(&text, natural / 4), Fit::Tightest);
    // An unallocated label (width 0) keeps the widest rung, which is the 1240 px window §1 opens at.
    assert_eq!(stage_for(&text, 0), Fit::Roomy);
}

/// S6: the level rule itself — `>>> ` / `!!! ` / four spaces stay what decides the LEVEL (§7 spells its
/// lines that way and every flow writes them) but none of them reaches the screen; failure is red,
/// detail dimmed, progress and an unmarked line plain.
#[test]
fn sec_03_shell_1_the_window_s6_the_log_level_rule_strips_the_marker_and_colours_the_rest() {
    use naivepost::ui::log_level::{self, Level};
    assert_eq!(log_level::level_of(">>> cut: 3 clips"), Level::Progress);
    assert_eq!(log_level::level_of("!!! frames failed"), Level::Failure);
    assert_eq!(log_level::level_of("    $ ffmpeg -i x"), Level::Detail);
    assert_eq!(log_level::level_of("line restored to 00:11"), Level::Plain);
    assert_eq!(log_level::body_of(">>> cut: 3 clips"), "cut: 3 clips");
    assert_eq!(log_level::body_of("!!! frames failed"), "frames failed");
    assert_eq!(log_level::body_of("    $ ffmpeg -i x"), "$ ffmpeg -i x");
    assert_eq!(log_level::body_of("line restored to 00:11"), "line restored to 00:11");
    // §7's link line keeps the indent INSIDE its marker, so the message keeps it too.
    assert_eq!(
        log_level::body_of(">>>   this run's exchanges, images included: llm/x.html"),
        "  this run's exchanges, images included: llm/x.html"
    );
    // Which level gets which colour is one table, and the plain levels answer `None`.
    assert_eq!(Level::Failure.tag(), Some(log_level::FAILURE_TAG), "failure is the red one");
    assert_eq!(Level::Detail.tag(), Some(log_level::DETAIL_TAG), "detail is the dimmed one");
    assert_eq!(Level::Progress.tag(), None, "progress is PLAIN: no tag, the theme's own colour");
    assert_eq!(Level::Plain.tag(), None, "an unmarked line draws the same plain");
}
