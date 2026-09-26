//! §04-prepare.md F1.1 ▶ Prepare — through the widgets. The button decides nothing: it forwards its
//! click to `ui::wire_play`, which asks [`naivepost::prepare_run`] what the press means before the
//! run bar is opened. What these checks assert is that a real click lands on the same answers the
//! logic tests in `tests/prepare_run_flow.rs` set for each state (spec/00-principles.md §5).
//!
//! A separate binary from `tests/press_play_widgets.rs` on purpose: GTK's main loop may only be run
//! once per thread, so one application drives every check here and none of them can share a session
//! folder with the F0.2 round's fixture.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::run;
use naivepost::ui;

/// Each check gets its own root so the press-time `startup::session_dir(current_dir())` resolves into
/// it rather than into the repo — which is how a stray `rust/session.naivepost/` got written last
/// time this flow was driven through the button. Returns the root AND the tree inside it, since S3
/// needs paths under the project folder while the handler reaches it through cwd.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f11w-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

fn source(path: &str, footage: bool) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }
}

fn with_sources(paths: &[&str]) -> Project {
    Project {
        sources: paths.iter().map(|p| source(p, true)).collect(),
        ..Project::default()
    }
}

/// One running GTK application for this test binary, taken in turn by the checks below — the same
/// single-main-loop arrangement `tests/smoke.rs` needs for the same reason (`g_application_run`
/// refuses a second claimant of the default main context). Every check runs inside one `activate`,
/// in order, because ▶'s meaning depends on what the press before it did.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_no_sources_refuses(app);
            RAN_NO_SOURCES.store(true, Ordering::SeqCst);
            check_name_clash_refuses(app);
            RAN_CLASH.store(true, Ordering::SeqCst);
            check_busy_prepare_pauses_first(app);
            RAN_BUSY.store(true, Ordering::SeqCst);
            check_start_clears_and_logs(app);
            RAN_START.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_NO_SOURCES: AtomicBool = AtomicBool::new(false);
static RAN_CLASH: AtomicBool = AtomicBool::new(false);
static RAN_BUSY: AtomicBool = AtomicBool::new(false);
static RAN_START: AtomicBool = AtomicBool::new(false);

/// The window log from the first line containing `marker` onward. Every check in this binary shares
/// one window-wide log stream, so a check that cares about ORDER inside its own press has to cut the
/// earlier checks off the front rather than search the whole session.
fn tail_after(logs: &[String], marker: &str) -> Vec<String> {
    let at = logs
        .iter()
        .position(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("no line containing {marker:?} in {logs:?}"));
    logs[at..].to_vec()
}

#[test]
fn f1_1_pressing_play_runs_prepare_s_own_start_flow() {
    window_round();
    assert!(
        RAN_NO_SOURCES.load(Ordering::SeqCst),
        "the no-sources check never ran"
    );
    assert!(RAN_CLASH.load(Ordering::SeqCst), "the clash check never ran");
    assert!(RAN_BUSY.load(Ordering::SeqCst), "the busy-precedence check never ran");
    assert!(
        RAN_START.load(Ordering::SeqCst),
        "the save/clear/log check never ran"
    );
}

/// S1: an empty list refuses at the start, before the bar opens — no run, no ⏸, the status line says
/// where to add a source. Same words as `f1_1_s1_no_sources_refuses`.
fn check_no_sources_refuses(app: &adw::Application) {
    // No `naivepost.json` in the session folder either: a refusal returns before S2, so nothing is
    // saved anywhere, and leaving the folder empty keeps that provable afterwards.
    let (_keep, _tree) = pin_session("empty");
    let model = Project::default();
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PLAY_TOOLTIP),
        "an idle ▶ says what §2 makes it say"
    );
    play.emit_clicked();

    assert_eq!(
        ui::state(&window).status,
        naivepost::prepare_run::NO_SOURCES,
        "the status line carries S1's sentence"
    );
    let logs = ui::window_logs();
    assert!(
        logs.contains(&naivepost::prepare_run::no_sources_log()),
        "the log says why the press was answered at all: {logs:?}"
    );
    assert_eq!(
        ui::running_step(&window),
        None,
        "a refused press never opened a run, so ⏸ was never shown"
    );
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PLAY_TOOLTIP),
        "and the button stayed ▶"
    );
}

/// S1: two inputs that would land in one `inputs/<base>` folder refuse, naming both rows. Same pair
/// and same strings as `f1_1_s1_a_name_clash_refuses_and_names_the_shared_folder`.
fn check_name_clash_refuses(app: &adw::Application) {
    let (_keep, _tree) = pin_session("clash");
    let first = "project:sources/clip.mkv";
    let second = "/media/clip.mkv";
    let model = with_sources(&[first, second]);
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    assert_eq!(
        ui::state(&window).status,
        naivepost::sources::clash_status(first, second),
        "the status line tells the person to rename one"
    );
    let logs = ui::window_logs();
    assert!(
        logs.contains(&naivepost::sources::clash_log(first, second)),
        "the log names the folder the two rows would share: {logs:?}"
    );
    assert_eq!(ui::running_step(&window), None, "no run was opened");
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PLAY_TOOLTIP),
        "S1 short-circuits before the bar turns busy"
    );
}

/// S1 vs F0.2: while a run is open, the pause wins over Prepare's start. The session folder holds no
/// project file, so press 1's S2 cannot rewrite anything that press 2 would then read — the only
/// thing that changes between presses is the bar's own state.
fn check_busy_prepare_pauses_first(app: &adw::Application) {
    let (_keep, _tree) = pin_session("busy");
    let model = with_sources(&["project:sources/lecture.mkv"]);
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    let play = ui::play_button(&window).expect("the run bar has a ▶");

    // Press 1: nothing busy, so the step started and the same button became ⏸.
    play.emit_clicked();
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "press 1 opened Prepare's run"
    );
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PAUSE_TOOLTIP),
        "a run under way turns ▶ into ⏸"
    );

    // Press 2: S1 defers — a session that would refuse cold is merely paused, not told off twice.
    play.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        run::PAUSING,
        "F0.2's precedence holds over Prepare's own start rule: the line says \u{201c}pausing\u{201d}, \
         not a refusal and not a fresh start"
    );
    assert_ne!(
        ui::state(&window).status,
        naivepost::prepare_run::NO_SOURCES,
        "a busy Prepare is never told to add a source it already has"
    );
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "pausing keeps the run rather than clearing it"
    );

    // Press 3 resumes it.
    play.emit_clicked();
    assert_eq!(ui::state(&window).status, run::RESUMED);
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "still the same run"
    );
}

/// S2 → S3 → S4 through the button: the project is saved first, the dead Describe middle is
/// cleared while the scaled frames stay, and the log prints the restart notice BEFORE the opening
/// lines — that order is the pinned reason, not an accident of concatenation.
fn check_start_clears_and_logs(app: &adw::Application) {
    let (_keep, tree) = pin_session("start");
    let lane = "lecture";
    let describe = tree.describe_dir(lane);
    std::fs::create_dir_all(describe.join(".llmframes")).expect("describe + scaled frames");
    std::fs::write(tree.events_tsv(lane), b"frame event\n").unwrap();
    std::fs::write(tree.describe_state(lane), b"halfway\n").unwrap();
    std::fs::write(describe.join(".llmframes/f.jpg"), b"scaled").unwrap();
    std::fs::create_dir_all(tree.frames_dir(lane)).expect("frames dir");
    std::fs::write(tree.frames_dir(lane).join("raw.jpg"), b"raw").unwrap();

    let model = with_sources(&["project:sources/lecture.mkv"]);
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_clicked();

    assert!(
        tree.dir().join("naivepost.json").exists(),
        "S2 saved the project into the session folder before anything else ran"
    );
    assert!(
        !tree.events_tsv(lane).exists(),
        "S3 removed events.tsv — the half-written description cannot be trusted"
    );
    assert!(
        !tree.describe_state(lane).exists(),
        "S3 removed state.txt too"
    );
    assert!(
        describe.join(".llmframes/f.jpg").exists(),
        "the scaled frames are KEPT: re-extracting them costs a minute for nothing"
    );
    assert!(
        tree.frames_dir(lane).join("raw.jpg").exists(),
        "and so are the extracted frames themselves"
    );
    assert_eq!(
        ui::running_step(&window),
        Some(run::Step::Prepare),
        "a clean start opens the run like any other page's ▶"
    );

    let logs = ui::window_logs();
    // The log is one window-wide stream (F0.5), so the earlier checks' lines are still in it. Slice to
    // this press's own tail rather than searching the whole session, which would find check 3's
    // opening line and read the order backwards.
    let mine = tail_after(&logs, ">>> stopped last time");
    let notice = naivepost::prepare_run::restart_line(&[
        "events.tsv".to_string(),
        "state.txt".to_string(),
    ]);
    let opening = ">>> prepare: 1 input file(s)";
    let at_notice = mine
        .iter()
        .position(|line| *line == notice)
        .unwrap_or_else(|| panic!("S3's restart notice is missing from this press: {mine:?}"));
    let at_opening = mine
        .iter()
        .position(|line| line.starts_with(opening))
        .unwrap_or_else(|| panic!("S4's opening line is missing from this press: {mine:?}"));
    assert!(
        at_notice < at_opening,
        "the clear is logged before the run opens — S2 saves, then S3 clears, then S4 speaks: \
         notice at {at_notice}, opening at {at_opening}, log {logs:?}"
    );
}
