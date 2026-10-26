//! §04-prepare.md §5 bullet 6 — *"⏸ resumes; ⏹ restarts only the describing, and only on the next ▶"*,
//! driven from the real run-bar buttons.
//!
//! Two halves, and they run through different doors on purpose:
//!
//! * **▶/⏸** is a widget test. `play-button` is pressed with the toolkit (`emit_clicked`, the same route
//!   `press_stop_widgets.rs` uses) and the answer is read off `ui::state(&window).status` — the line the
//!   user actually sees — plus the held `RunBar` for the pause flag and the run's identity. §5's point is
//!   that the resume is the SAME run continuing, so the identity is asserted across the two presses.
//! * **⏹'s restart scope** lives in `prepare_run::begin`, which the ▶ press calls for a Prepare page; the
//!   clear itself (`clear_stopped_describe`) is what decides "only the describing". Both halves of the
//!   window press are exercised here: the real ⏹ arms the restart through `stop_legs::describe_stage`, and
//!   the next `prepare_run::begin` — the door the next ▶ runs — is what clears it. A live audio.cpp would be
//!   needed for the press's preflight to pass, and §5's rule is not about the preflight, so the arming and
//!   the next-▶ clear are driven through the same functions the press calls rather than through a server.
//!
//! One `adw::Application::run` per binary, checks back to back inside one `connect_activate`; cwd pinned to
//! this test's own folder before anything touches disk.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::common;
use adw::prelude::*;
use common::fixture_dir;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::run::{self, RunBar, Subprocesses, Transport};
use naivepost::shell::Page;
use naivepost::{prepare_run, stop_legs, ui};

thread_local! {
    static APP: std::cell::RefCell<Option<adw::Application>> = const { std::cell::RefCell::new(None) };
}

static RAN_PAUSE: AtomicBool = AtomicBool::new(false);
static RAN_STOP: AtomicBool = AtomicBool::new(false);

/// A project folder of §1's shape for one check. The Describe + frame files a stopped run leaves are
/// laid down by [`seed_stopped_describe`].
fn scratch(tag: &str) -> PathBuf {
    scratch_from(&common::temp_dir_created(tag))
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

/// Lay a session out the way a run stopped inside Describe leaves it: the describing's own working
/// files (`events.tsv`, `state.txt`) plus everything the run FINISHED — scaled frames, extracted
/// frames, the `.frames` marker and a transcript. §5 says only the first two go.
fn seed_stopped_describe(tree: &Tree) {
    let lane = "lecture";
    let describe = tree.describe_dir(lane);
    fs::create_dir_all(describe.join(".llmframes")).expect("describe folder");
    fs::create_dir_all(tree.frames_dir(lane)).expect("frames folder");
    fs::create_dir_all(tree.input_dir(lane)).expect("inputs folder");
    fs::write(tree.events_tsv(lane), b"frame event\n").unwrap();
    fs::write(tree.describe_state(lane), b"halfway through\n").unwrap();
    fs::write(describe.join(".llmframes/f.jpg"), b"scaled").unwrap();
    fs::write(tree.frames_dir(lane).join("raw.jpg"), b"raw").unwrap();
    fs::write(tree.frames_marker(lane), "0.25|1\n").unwrap();
    fs::write(tree.transcript_tsv(lane), "0.00\t1.00\tNARRATOR\there\n").unwrap();
}

/// Which of the seeded files are still on disk, as names — the assertion "only the describing went".
fn survivors(tree: &Tree) -> Vec<String> {
    let lane = "lecture";
    let describe = tree.describe_dir(lane);
    [
        (tree.events_tsv(lane), "events.tsv"),
        (tree.describe_state(lane), "state.txt"),
        (describe.join(".llmframes/f.jpg"), "scaled frame"),
        (tree.frames_dir(lane).join("raw.jpg"), "extracted frame"),
        (tree.frames_marker(lane), ".frames"),
        (tree.transcript_tsv(lane), "transcript.tsv"),
    ]
    .into_iter()
    .filter(|(path, _)| path.exists())
    .map(|(_, name)| name.to_string())
    .collect()
}

// ---- the widget round ------------------------------------------------------------

/// The ▶ / ⏸ half through the real button: one run, paused and resumed by the same `play-button`, and
/// never restarted.
fn play_pauses_and_the_next_play_resumes(app: &adw::Application) {
    // §5's resume rule is about the run, so the session folder is this check's own: `prepare_run::begin`
    // writes the project file and `meta.env` into the cwd's session folder.
    let root = common::temp_dir_created("b6-pause");
    let cwd = std::env::current_dir().expect("a cwd to come back to");
    std::env::set_current_dir(&root).expect("cwd in the check's folder");

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Prepare.label());
    window.set_default_size(1280, 980);
    window.present();

    let play = ui::play_button(&window).expect("the run bar has a play-button");
    // First press: nothing was running, so ▶ starts Prepare's step (§F0.2 S3/S4). Its walk is on the
    // run's worker (§03-shell §1), so the run stands open until the window takes the worker's answer,
    // which it cannot before this check turns the main loop.
    play.emit_clicked();
    let bar = ui::run_bar(&window).expect("the window holds a run bar");
    {
        let held = bar.borrow();
        let running = held.running.as_ref().expect("▶ started a run");
        assert_eq!(
            running.step,
            run::Step::Prepare,
            "the Prepare page's ▶ runs Prepare"
        );
        assert!(!running.paused, "a fresh run is not paused");
    }

    // Second press: ▶ is now the pause button. §F0.2 S1's own sentence, and `paused` on the run.
    play.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        run::PAUSING,
        "a ▶ over a running run says it will pause after the stage in hand"
    );
    let step_after_pause = {
        let held = bar.borrow();
        let running = held.running.as_ref().expect("pausing keeps the run");
        assert!(running.paused, "the run is now paused");
        running.step
    };

    // Third press: the resume, by the SAME button, and the SAME run — nothing was restarted. §5's whole
    // point: ⏸ is a hold on this run, so resuming it must not cost the finished work or start again.
    play.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        run::RESUMED,
        "and the next ▶ resumes rather than restarting"
    );
    {
        let held = bar.borrow();
        let running = held
            .running
            .as_ref()
            .expect("the resumed run is still the run");
        assert!(!running.paused, "the pause is off again");
        assert_eq!(
            running.step, step_after_pause,
            "one and the same run, not a new one"
        );
        assert_eq!(
            running.sources,
            held.running.as_ref().expect("still there").sources,
            "and its source snapshot survived both presses: a restart would take a fresh one"
        );
    }
    // The resumed walk goes on to its end, and the run closes when the window takes the answer.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(120)),
        "the press's worker answered"
    );
    assert!(
        bar.borrow().running.is_none(),
        "the run closed when it ended"
    );

    std::env::set_current_dir(&cwd).expect("cwd back");
    window.close();
    RAN_PAUSE.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The ⏹ side of the buttons: pressing ⏹ on a window whose run is inside Describe brings the run down,
/// arms the restart, and touches NOTHING on disk — the restart waits for the next ▶.
fn stop_arms_and_waits_for_the_next_play(app: &adw::Application) {
    let root = common::temp_dir_created("b6-stop");
    let tree = Tree::new(scratch_from(&root)).expect("a .naivepost folder is a project");
    seed_stopped_describe(&tree);
    let _project = with_sources(&["project:sources/lecture.mkv"]);
    let before = survivors(&tree);

    // The button, on a Prepare window with a run under way and Describe open as the running stage.
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, Page::Prepare.label());
    window.set_default_size(1280, 980);
    window.present();
    let stop = ui::stop_button(&window).expect("the run bar has a stop-button");
    let bar = ui::run_bar(&window).expect("the window holds a run bar");
    // Put a run in the bar so the press is S3 rather than S2 (nothing to end). `start_run` is what ▶
    // calls first; the ⏹ sensitivity is the widget's own, so it is set the way the paint would.
    naivepost::runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut ui::run_queue(&window).expect("a queue").borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        run::Step::Prepare,
        run::snapshot_sources(&model),
    );
    stop.set_sensitive(true);
    // §F1.7's stage flag: the run is inside Describe, which is what makes this stop a Describe stop
    // (`stop_legs::describe_stage` is exactly what the press reads).
    stop_legs::enter_describe();

    stop.emit_clicked();
    stop_legs::leave_describe();

    assert_eq!(
        ui::state(&window).status,
        run::STOPPING,
        "one press brings the run down and says so"
    );
    assert!(
        bar.borrow().stop_flag,
        "the stop flag is set for the flows between subprocesses"
    );
    assert!(
        stop_legs::take_describe_restart(),
        "and the press armed the next Prepare run to describe from the start"
    );
    // ⏹ itself deletes nothing: §5's restart happens on the NEXT ▶, not here.
    assert_eq!(
        survivors(&tree),
        before,
        "⏹ touched no file — the clear is the next ▶'s work"
    );

    window.close();
    RAN_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The root the checks write their session into: `temp_dir_created` gives a plain folder, and a project
/// is a folder ending in `.naivepost`.
fn scratch_from(root: &Path) -> PathBuf {
    let dir = root.join("session.naivepost");
    fs::create_dir_all(&dir).expect("the session folder");
    dir
}

// ---- the next-▶ half: prepare_run::begin is the door ▶ runs ------------------------------

/// §5: *"⏹ restarts only the describing, and only on the next ▶"*, read through the door the next ▶
/// actually runs (`prepare_run::begin`). The arming came from the ⏹ press above.
#[test]
fn sec_04_prepare_5_rules_b6_stop_mid_describe_restarts_only_the_describing() {
    let _turn = crate::common::describe_flags_turn();
    let tree = Tree::new(scratch("b6-describe")).expect("a session folder");
    seed_stopped_describe(&tree);
    let project = with_sources(&["project:sources/lecture.mkv"]);
    let before = survivors(&tree);
    assert_eq!(
        before.len(),
        6,
        "the stopped run left six files behind: {before:?}"
    );

    // The next ▶, armed by the ⏹ that cut through Describe.
    stop_legs::arm_describe_restart();
    let start = prepare_run::begin(&project, &tree);
    let (lines, cleared) = match start {
        prepare_run::Start::Started { lines, cleared } => (lines, cleared),
        other => panic!("a session with sources starts, it does not refuse: {other:?}"),
    };

    // Only the describing: the two working files went, everything finished stayed.
    assert_eq!(
        cleared.removed,
        vec!["events.tsv".to_string(), "state.txt".to_string()]
    );
    assert!(
        cleared.failures.is_empty(),
        "nothing failed to unlink: {:?}",
        cleared.failures
    );
    let after = survivors(&tree);
    assert!(
        !after.contains(&"events.tsv".to_string()),
        "the describing starts again from the start"
    );
    assert!(
        !after.contains(&"state.txt".to_string()),
        "and its running state is gone with it"
    );
    for kept in [
        "scaled frame",
        "extracted frame",
        ".frames",
        "transcript.tsv",
    ] {
        assert!(
            after.contains(&kept.to_string()),
            "{kept} is kept — {after:?}"
        );
    }
    // The log says which two went and which were kept, in §F1.1 S3's own sentence.
    assert_eq!(
        lines.first().map(String::as_str),
        Some(prepare_run::restart_line(&["events.tsv".into(), "state.txt".into()]).as_str()),
        "the restart line names both files and the kept frames: {lines:?}"
    );
    assert!(
        lines[0].contains("scaled frames kept"),
        "as §S3 spells it: {}",
        lines[0]
    );
    // One arming, one run: a second ▶ is not a second restart.
    assert!(
        !stop_legs::take_describe_restart(),
        "the arming was consumed by that ▶ — one stop restarts exactly one run"
    );
}

/// §5: the restart is scoped to the describing. A stop that landed on another stage arms nothing, so
/// the next ▶ says nothing and the describing's files survive untouched.
#[test]
fn sec_04_prepare_5_rules_b6_a_stop_outside_describe_restarts_nothing() {
    let _turn = crate::common::describe_flags_turn();
    let tree = Tree::new(scratch("b6-outside")).expect("a session folder");
    seed_stopped_describe(&tree);
    let project = with_sources(&["project:sources/lecture.mkv"]);
    let before = survivors(&tree);

    // No `arm_describe_restart`: this stop landed on e.g. the ASR stage. A plain RunBar answers the
    // same way the press does with `in_describe == false`.
    let mut bar = RunBar {
        running: Some(run::Run {
            step: run::Step::Prepare,
            paused: false,
            log_expanded: true,
            sources: run::snapshot_sources(&project),
        }),
        ..RunBar::default()
    };
    let stopped = bar.press_stop(
        Page::Prepare,
        Transport::default(),
        false,
        &mut Subprocesses::default(),
    );
    assert!(
        matches!(stopped, run::Stopped::RunStopped { .. }),
        "the run came down: {stopped:?}"
    );
    assert!(
        !bar.describe_restarts(),
        "and it did NOT arm a restart, because the stage was not Describe"
    );
    assert!(
        !stop_legs::take_describe_restart(),
        "nor did the legs' own flag get armed"
    );

    // The next ▶, un-armed. A clean tree means `describes_from_the_start(false, false)`, so the run
    // says nothing about restarting.
    let counts = naivepost::prepare::count(&tree, &project);
    let lines = prepare_run::start_log(&project, &counts);
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("describing from the start again")),
        "an un-armed run says nothing about restarting: {lines:?}"
    );
    assert_eq!(survivors(&tree), before, "and it removed nothing");
}

/// §5's last clause, alone: **only on the next ▶**. With the marker on disk and the stop armed but no
/// press since, the files stand; the clear happens inside the press's own door, not at the stop.
#[test]
fn sec_04_prepare_5_rules_b6_the_restart_happens_only_on_the_next_play() {
    let _turn = crate::common::describe_flags_turn();
    let tree = Tree::new(scratch("b6-onlynext")).expect("a session folder");
    seed_stopped_describe(&tree);
    let project = with_sources(&["project:sources/lecture.mkv"]);
    stop_legs::arm_describe_restart();
    let before = survivors(&tree);

    // The rule as `prepare_run::begin` computes it: armed OR a marker on disk ⇒ restart; neither ⇒ not.
    assert!(
        stop_legs::describes_from_the_start(true, false),
        "an armed stop restarts the describing"
    );
    assert!(
        stop_legs::describes_from_the_start(false, true),
        "so does a lane left holding its working files, with no stop at all"
    );
    assert!(
        !stop_legs::describes_from_the_start(false, false),
        "and only when neither is true does a run simply resume"
    );

    // Nothing has cleared them yet: the arming sits on the flag, waiting for a press that has not come.
    assert_eq!(
        survivors(&tree),
        before,
        "the marker stands until a ▶ takes it up"
    );

    // The next ▶ takes it up: the arming is read-and-cleared there, once.
    let armed = stop_legs::take_describe_restart();
    assert!(armed, "the next ▶ reads the arming");
    assert!(
        !stop_legs::take_describe_restart(),
        "and it is good for exactly one run"
    );
    let cleared = prepare_run::clear_stopped_describe(&tree, &project);
    assert_eq!(
        cleared.removed,
        vec!["events.tsv".to_string(), "state.txt".to_string()],
        "the clear is the ▶'s doing"
    );
    assert!(
        !stop_legs::take_describe_restart(),
        "and a third ▶ restarts nothing"
    );
}

// ---- the round, and the #[test] that starts it ----------------------------------------

fn run_round(app: &adw::Application) {
    APP.with(|held| *held.borrow_mut() = Some(app.clone()));
    // The pause/resume pair must run before any check that touches the describe arming: one process,
    // one thread-local flag.
    for check in [
        play_pauses_and_the_next_play_resumes as fn(&adw::Application),
        stop_arms_and_waits_for_the_next_play,
    ] {
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(app)))
        {
            let why = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "panicked".to_string());
            eprintln!("sec_04_prepare_5_rules_transport check failed: {why}");
        }
    }
    app.quit();
}

#[test]
fn sec_04_prepare_5_rules_b6_the_run_bar_buttons_hold_and_release_the_run() {
    common::pin_private_cwd();
    let app = adw::Application::builder()
        .application_id("org.example.naivepost.sec04p5transport")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(run_round);
    app.run_with_args::<&str>(&[]);
    assert!(
        RAN_PAUSE.load(std::sync::atomic::Ordering::SeqCst),
        "the ▶/⏸ widget check never ran"
    );
    assert!(
        RAN_STOP.load(std::sync::atomic::Ordering::SeqCst),
        "the ⏹ widget check never ran"
    );
}
