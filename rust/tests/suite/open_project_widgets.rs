//! §03-shell.md F0.9 Open a project — through the widget.
//!
//! The header bar's Open decides nothing: it asks for a folder and forwards the answer to
//! [`naivepost::open_project`], so what these checks assert is that the button is there with §1's
//! tooltip, that a press reaches the chooser without changing anything when nobody answers, and —
//! through the same seam the chooser callback uses — that each outcome lands on the state the logic
//! tests check (spec/00-principles.md §5).

use std::fs;
use std::path::{Path, PathBuf};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::open_project::{CANNOT_OPEN, NOT_A_PROJECT};
use naivepost::project::PROJECT_FILE;
use naivepost::shell::Page;
use naivepost::ui;
use crate::common;
use common::{fixture_dir, temp_dir_created as temp_dir};

/// One running GTK application for this test binary, taken in turn by the checks below.
fn window_round() {
    common::one_app_round(|app| {
        open_button_is_in_the_header(app);
        a_press_nobody_answers_changes_nothing(app);
        an_old_single_file_refuses_through_the_window(app);
        a_folder_that_will_not_open_refuses_through_the_window(app);
        a_valid_project_switches_the_session(app);
        s4_refreshes_every_page_through_the_doors_a_tab_switch_uses(app);
        s4_the_opened_project_lands_in_the_conf_on_disk(app);
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
        the_dropped_source_is_said_by_the_real_open(app);
        RAN_DROP.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_DROP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A project folder holding `body` as its `naivepost.json`, with real files for `present`.
/// A project folder written with `body` as its `naivepost.json`, plus real files for each of the
/// `present` source names under `sources/`, and one frame file in each footage lane's counted folder
/// (`prepare/inputs/frames/<lane>/`) so the `Inputs:` readout has a number to show.
fn project_folder(dir: &Path, body: &str, present: &[&str]) -> PathBuf {
    fs::create_dir_all(dir.join("sources")).unwrap();
    fs::write(dir.join(PROJECT_FILE), body).unwrap();
    for name in present {
        fs::write(dir.join("sources").join(name), "video bytes").unwrap();
        let lane = name.trim_end_matches(".mkv");
        let frames = dir.join("prepare").join("inputs").join("frames").join(lane);
        fs::create_dir_all(&frames).unwrap();
        fs::write(frames.join("000001.jpg"), "frame").unwrap();
    }
    dir.to_path_buf()
}

/// A project whose second source is not on disk, so the open has something to drop.
const TWO_SOURCES_ONE_GONE: &str = r#"{
  "sources": [
    {"path": "project:sources/keep.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": [0]},
    {"path": "project:sources/gone.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": [0]}
  ],
  "interval": 2.5,
  "language": "de"
}"#;

/// S1: the button exists, carries §1's tooltip, and sits between New and Save — read off the
/// header's child order rather than off pixels.
fn open_button_is_in_the_header(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let open_ = ui::open_button(&window).expect("F0.9 puts Open in the header bar");
    assert_eq!(
        open_.tooltip_text().map(|text| text.to_string()),
        Some("Load a project \u{2014} sources, prompts and settings".to_string()),
        "badge 2 of §1's header bar, em dash included"
    );

    // Only the header's own buttons: stop at the header bar, whose subtree holds New/Open/Save on
    // the left and Rescan at the end. The run bar and page widgets are below it, not in it.
    let names = header_names(&window);
    let new_at = names.iter().position(|name| name == "new-button");
    let open_at = names.iter().position(|name| name == "open-button");
    let save_at = names.iter().position(|name| name == "save-button");
    assert_eq!(
        (new_at, open_at, save_at),
        (Some(0), Some(1), Some(2)),
        "§1 reads New · Open · Save from the left: {names:?}"
    );
    window.close();
}

/// The packed-left children of the window's header bar, by widget name. `adw::HeaderBar` nests its
/// boxes (window handle → title box → buttons), so the tree is walked rather than one level read.
fn header_names(window: &adw::ApplicationWindow) -> Vec<String> {
    let content = window.content().expect("the window has content");
    // build_window's top box: header, switcher, stack, run bar, log expander.
    let header = content
        .first_child()
        .expect("the top box has a first child (the header bar)");
    let mut out = Vec::new();
    collect_button_names(&header, &mut out);
    out
}

fn collect_button_names(widget: &gtk::Widget, out: &mut Vec<String>) {
    for child in widget.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(child) = child.downcast::<gtk::Widget>() else { continue };
        let name = child.widget_name().to_string();
        if name.ends_with("-button") {
            out.push(name);
        }
        collect_button_names(&child, out);
    }
}

/// S1 through the button: the press opens the chooser, which nobody answers headless, so the
/// session is untouched. The rules behind the answer live in `open_project`; what is asserted here
/// is that the click got that far and changed nothing on its own.
fn a_press_nobody_answers_changes_nothing(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let before = ui::session_sources(&window);
    assert!(!before.is_empty(), "the fixture's session has sources to compare against");

    let open_ = ui::open_button(&window).expect("Open button");
    open_.emit_clicked();

    assert_eq!(
        ui::session_sources(&window),
        before,
        "an unanswered chooser leaves the session exactly where it was"
    );
    window.close();
}

/// S2 through the window: a single-file project is refused by name, on the status line and in the
/// log, and the session did not move.
fn an_old_single_file_refuses_through_the_window(app: &adw::Application) {
    let root = temp_dir("s2");
    let file = root.join("old.json");
    fs::write(&file, "{\"sources\": []}").unwrap();

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let before = ui::session_sources(&window);

    let opened = ui::open_from(&window, &file);

    assert!(!opened, "F0.9 S2: the open did not happen");
    assert_eq!(ui::state(&window).status, NOT_A_PROJECT);
    assert_eq!(ui::state(&window).status, "not a project folder");
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| line.ends_with("-- not supported") && line.contains("old single-file")),
        "the refusal is in the log: {:?}",
        ui::window_logs()
    );
    assert_eq!(
        ui::session_sources(&window),
        before,
        "a refusal must not have touched the session"
    );
    window.close();
}

/// S4 through the window: a folder that will not open takes the generic failure, never the
/// single-file message, and again leaves the session alone.
fn a_folder_that_will_not_open_refuses_through_the_window(app: &adw::Application) {
    let root = temp_dir("s4-bare");
    let bare = root.join("somewhere");
    fs::create_dir_all(bare.join("photos")).unwrap();

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let before = ui::session_sources(&window);

    let opened = ui::open_from(&window, &bare);

    assert!(!opened, "F0.9 S4: nothing opened");
    assert_eq!(ui::state(&window).status, CANNOT_OPEN);
    // `window_logs()` is one thread-local shared by every window in this binary, so only the line
    // this attempt added is looked at — earlier checks' refusals are still in there.
    let mine = last_log();
    assert!(mine.starts_with("!!! "), "{mine}");
    assert!(mine.contains("no naivepost.json"), "{mine}");
    assert!(
        !mine.contains("single-file"),
        "an ordinary folder is not called an old single-file project: {mine}"
    );
    assert_eq!(ui::session_sources(&window), before, "nothing half-applied");
    window.close();
}

/// The most recent line the window logged. Shared thread-local, so callers take the tail rather than
/// scanning everything earlier checks put there.
fn last_log() -> String {
    common::last_log().expect("something was logged")
}

/// S3 + S4 through the window: a valid project switches the session in place, drops the source
/// whose file is gone with one line, and those lines are visible in the log area.
fn a_valid_project_switches_the_session(app: &adw::Application) {
    let root = temp_dir("s3-switch");
    let folder = project_folder(
        &root.join("eth.naivepost"),
        TWO_SOURCES_ONE_GONE,
        &["keep.mkv"],
    );

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let before = ui::session_sources(&window);

    let opened = ui::open_from(&window, &folder);

    assert!(opened, "F0.9: a real project opens");
    let after = ui::session_sources(&window);
    assert_ne!(after, before, "the session switched to the opened project");
    assert_eq!(
        after,
        vec!["project:sources/keep.mkv".to_string()],
        "only the source that exists survived"
    );
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| line == "!!! project:sources/gone.mkv is not there any more -- dropped from the session"),
        "the drop is said once in the log: {:?}",
        ui::window_logs()
    );
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| line.starts_with(">>> opened ")),
        "and the open itself is logged: {:?}",
        ui::window_logs()
    );
    // The status line now says what happened, in the same shape as "new project — <base>".
    assert_eq!(
        ui::state(&window).status,
        "opened \u{2014} eth.naivepost",
        "the status line reports the open, not just the folder name"
    );
    // The log expander is where those lines are drawn; opening a project keeps it open.
    assert!(
        ui::log_expander(&window)
            .expect("log expander")
            .is_expanded()
            || !ui::window_logs().is_empty(),
        "the log has somewhere to be read"
    );
    window.close();
}

#[test]
fn f0_9_s1_to_s4_through_the_real_open_button() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the F0.9 widget checks never ran"
    );
}

/// S4's first two steps, on the same state the pages read: the tabs are re-greyed from the OPENED
/// project, and the visible tab's `Inputs:` row counts the opened folder rather than the old session.
fn s4_refreshes_every_page_through_the_doors_a_tab_switch_uses(app: &adw::Application) {
    let root = temp_dir("s4-refresh");
    // A project with footage, opened over a session that has none: Cut goes from greyed to live, which
    // is something only a repaint can do, and the Inputs row changes wording entirely.
    let folder = project_folder(&root.join("fresh.naivepost"), TWO_SOURCES_ONE_GONE, &["keep.mkv"]);

    let empty = naivepost::project::Project::default();
    let window = ui::build_window(app, &empty, "Prepare");
    window.present();
    assert!(
        ui::tab_dimmed(&window, Page::Cut),
        "a session with no footage opens with Cut greyed"
    );
    let before = ui::inputs_readout(&window).map(|l| l.text().to_string()).unwrap_or_default();

    assert!(ui::open_from(&window, &folder), "the project opened");

    assert!(
        !ui::tab_dimmed(&window, Page::Cut),
        "S4: the tabs were repainted from the opened project — Cut is live now"
    );
    let after = ui::inputs_readout(&window).map(|l| l.text().to_string()).unwrap_or_default();
    assert_ne!(after, before, "S4: the Inputs row was redrawn ({before:?} -> {after:?})");
    assert!(after.starts_with("Inputs: "), "{after:?}");
    assert_eq!(
        ui::state(&window).status,
        "opened \u{2014} fresh.naivepost",
        "and the status says which project it switched to"
    );
    window.close();
}

/// S4's third step, in its own `Once` round because `XDG_CONFIG_HOME` is process-wide and this check
/// owns it. The pair has to be in the FILE: `open_project::remember` only fills a map, so a test that
/// read the map would pass even with no conf write at all — which is the gap this round closed.
fn s4_the_opened_project_lands_in_the_conf_on_disk(app: &adw::Application) {
    let root = temp_dir("s4-conf");
    let config = root.join("config");
    fs::create_dir_all(&config).unwrap();
    let folder = project_folder(
        &root.join("conf-eth.naivepost"),
        TWO_SOURCES_ONE_GONE,
        &["keep.mkv"],
    );
    std::env::set_var("XDG_CONFIG_HOME", &config);

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    // The conf already holds the project an earlier check in this round opened, because `XDG_CONFIG_HOME`
    // is process-wide and every open writes through the same file. That is the point: a second open ADDS
    // its own pair rather than replacing the first, so the numbering here is not PROJECT_1.
    let opened = ui::open_from(&window, &folder);
    let status = ui::state(&window).status;
    let inputs = ui::inputs_readout(&window).map(|l| l.text().to_string()).unwrap_or_default();
    window.close();

    assert!(opened, "F0.9 S4: the project opened");
    assert_eq!(status, "opened \u{2014} conf-eth.naivepost", "S4's status sentence");
    assert!(
        inputs.starts_with("Inputs: "),
        "the readout row was repainted, not left blank: {inputs:?}"
    );

    let conf = config.join("naivepost").join("llm.conf");
    let body = fs::read_to_string(&conf).expect("the open wrote llm.conf");
    // The writer quotes every value (`settings::quote`), so the file holds `"‹path›"`, not a bare path.
    assert!(
        body.contains(&format!("ROOT=\"{}\"", folder.display())),
        "the ROOT half is in the file: {body}"
    );
    assert!(
        body.contains(&format!(
            "FILE=\"{}\"",
            folder.join(PROJECT_FILE).display()
        )),
        "and the FILE half: {body}"
    );
}

/// S3's dropped-source sentence through the button's own path. The literal is pinned as bytes and
/// counted, not just searched for: `open_folder` logs every line the apply produced, so a second door
/// saying the same thing (or the same line logged twice) must fail here rather than read as a detail.
fn the_dropped_source_is_said_by_the_real_open(app: &adw::Application) {
    let root = temp_dir("s3-drop-wire");
    let folder = project_folder(
        &root.join("drop-eth.naivepost"),
        TWO_SOURCES_ONE_GONE,
        &["keep.mkv"],
    );
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    // The seam the folder chooser's response calls — same body, no dialog in the way.
    assert!(ui::open_from(&window, &folder), "the project opened");

    assert_eq!(
        ui::session_sources(&window),
        vec!["project:sources/keep.mkv".to_string()],
        "only the source whose file is there survives"
    );
    let literal = "!!! project:sources/gone.mkv is not there any more -- dropped from the session";
    // Counted over THIS press only: take the log from the `>>> opened ‹this folder›` line that ends
    // it, since three earlier checks in this round open a project with the same missing file and the
    // sink is shared process-wide. The drop line comes BEFORE that line (the repaint's ffprobe rows
    // follow it), so the window starts one marker further back.
    let logs = ui::window_logs();
    let end = logs
        .iter()
        .rposition(|line| line.starts_with(">>> opened") && line.contains(&folder.display().to_string()))
        .expect("this open logged its own >>> opened line");
    let start = logs[..end]
        .iter()
        .rposition(|line| line.starts_with(">>> opened"))
        .map(|i| i + 1)
        .unwrap_or(0);
    let mine = &logs[start..=end];
    assert!(
        mine.iter().any(|line| line.as_str() == literal),
        "this open must say the drop: {mine:?}"
    );
    assert_eq!(
        mine.iter().filter(|line| line.as_str() == literal).count(),
        1,
        "said once by this press, not twice: {mine:?}"
    );
    assert_eq!(
        ui::state(&window).status,
        naivepost::ui::open_apply::opened_status("drop-eth.naivepost"),
        "the open succeeded, so the status carries the opened wording, not a failure"
    );
    window.close();
}

#[test]
fn f0_9_s3_the_real_open_button_logs_the_dropped_source() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_DROP.load(std::sync::atomic::Ordering::SeqCst),
        "the dropped-source check never ran"
    );
}
