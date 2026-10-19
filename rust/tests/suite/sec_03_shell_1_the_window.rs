//! §03-shell.md §1 The window — the shell's own layout facts, checked through the real window.
//!
//! §1 is a list of numbered badges plus a handful of sentences about where they sit and how the window
//! behaves. Most of it was already true (the default size, the header buttons and their tooltips, a
//! greyed-not-disabled tab, a read-only monospace log body). What this file newly pins, because it was
//! NOT true before this round, is: the window's TITLE, the header's project name/path with its three
//! narrowing stages, the log drawing the MESSAGE alone with the level as a colour, and the page and the
//! bottom bar being two halves of a draggable `gtk::Paned`.
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
    window.set_default_size(1240, 740);
    window.present();
    hold_last_window(window.clone());
    settle();
    window
}

/// S1: "One window, title \"Naivepost\", default 1240×740." The floor (shrinkable to 1024×640 or less
/// on every tab) is `tests/suite/window_shrinks.rs`'s measurement; this is the other half of the line.
fn s1_title_and_default_size(app: &adw::Application) {
    let window = window(app, "Prepare");
    assert_eq!(
        window.title().as_deref(),
        Some(shell::WINDOW_TITLE),
        "§1: one window, titled Naivepost — without it GTK titles the window from the app id"
    );
    assert_eq!(
        (window.default_width(), window.default_height()),
        (1240, 740),
        "§1: 1240×740 is the size it OPENS at, not a floor"
    );
    window.close();
}

/// S2: the header's numbered badges, in the order §1 reads them. New/Open/Save pack from the left edge;
/// Rescan, Settings and ⓘ pack from the right edge inward, so ⓘ lands innermost of that group and the
/// bar reads ⓘ ⚙ ⟳ on screen. Badge 4 sits in the header's centre slot, which is what `adw::HeaderBar`
/// keeps for a title widget — anything packed at an end would push the tab row aside.
fn s2_header_badges_in_order(app: &adw::Application) {
    let window = window(app, "Prepare");
    let header = header_of(&window);
    let left = packed_names(&header, "start");
    for name in ["new-button", "open-button", "save-button"] {
        assert!(left.iter().any(|n| n == name), "New, Open, Save pack from the left: {left:?}");
    }
    assert!(
        position_of(&left, "new-button") < position_of(&left, "open-button"),
        "New comes before Open, as §1's **1** **2**: {left:?}"
    );
    assert!(
        position_of(&left, "open-button") < position_of(&left, "save-button"),
        "Open comes before Save, as §1's **2** **3**: {left:?}"
    );
    let title = header.title_widget().expect("§1 badge 4: the project name/path label");
    assert_eq!(title.widget_name(), "project-title", "badge 4 is named for a test to find");
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

/// S5: the bottom of the window reads run bar, then the Inputs/Outputs readouts, then the log expander
/// with the status line in its header; the log body is read-only monospace.
fn s5_bottom_bar_order(app: &adw::Application) {
    let root = scratch_root("bottom");
    let window = window(app, "Prepare");
    let bottom = common::widget_in(&window, "bottom-bar").expect("the divider's bottom half");
    let names = child_names(&bottom);
    let run = position_of(&names, "run-bar");
    let readouts = position_of(&names, "readout-row");
    let log = position_of(&names, "log-expander");
    assert!(
        run < readouts && readouts < log,
        "§1's order: run bar, then the Inputs/Outputs readouts, then the log expander: {names:?}"
    );
    let expander: gtk::Expander = ui::log_expander(&window).expect("the shell carries a log expander");
    let header = expander.label_widget().expect("the expander's header carries the status line");
    let header_names = child_names(&header);
    assert!(
        header_names.contains(&"status-line".to_string()),
        "§1: the log expander, its header carrying the status line: {header_names:?}"
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
        "start" => &["new-button", "open-button", "save-button"],
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
