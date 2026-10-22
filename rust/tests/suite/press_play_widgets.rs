//! §03-shell.md F0.2 Press ▶ — through the widgets. The button on screen decides nothing: it draws
//! [`naivepost::run::controls`] and forwards its click to [`naivepost::run::RunBar::press`], so what
//! these checks assert is that the forwarding happened, not what a press means (spec/00-principles.md §5).


use adw::prelude::*;
use naivepost::run;
use naivepost::ui;
use crate::common;
use common::{fake_audio_server, fixture_dir};

/// Point the settings this process reads at that server. One conf for the whole binary: every check
/// here presses ▶, so they all need the same answer, and `XDG_CONFIG_HOME` belongs to the process.
fn serve_audio_for_the_presses(url: &str) {
    unsafe {
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-f02w-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    let paths = naivepost::settings::Paths {
        config_dir: config.join("naivepost"),
        data_dir: config.join("data"),
    };
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the fake server");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
}

fn window_round() {
    common::one_app_round(|app| {
        // §S5's preflight runs before any stage, so every press below needs an audio.cpp that answers.
        serve_audio_for_the_presses(&fake_audio_server());
        check_press_forwards(app);
        RAN_PRESS.store(true, std::sync::atomic::Ordering::SeqCst);
        check_the_run_button_toggles_a_started_preview(app);
        RAN_TRANSPORT.store(true, std::sync::atomic::Ordering::SeqCst);
        check_the_snapshot_names_the_live_sources(app);
        RAN_SNAPSHOT.store(true, std::sync::atomic::Ordering::SeqCst);
        check_a_failed_stage_names_itself(app);
        RAN_FAILURE.store(true, std::sync::atomic::Ordering::SeqCst);
        check_the_pause_toggle_says_both_readings(app);
        RAN_BOTH_READINGS.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN_PRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_TRANSPORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_SNAPSHOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_FAILURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BOTH_READINGS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// S2 through the widgets: once Cut's ▶✂ has started the preview, the run bar's ▶ belongs to that
/// preview and toggles it -- it does NOT start a run. Asserted on the state both buttons read
/// (`preview_player(...).transport`) plus `RunBar::running`, which must stay empty (§F0.2: S2 stops).
fn check_the_run_button_toggles_a_started_preview(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    // The Cut page owns a transport; Prepare and Produce ignore one (`run::transport_for` is None).
    ui::tab_button(&window, naivepost::shell::Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, naivepost::shell::Page::Cut, "the Cut tab bounced");
    ui::seed_review_cut(&window, &naivepost::cut::Cut {
        segs: vec![
            naivepost::cut::Seg { s: 0.0, e: 10.0, ..Default::default() },
            naivepost::cut::Seg { s: 20.0, e: 30.0, ..Default::default() },
        ],
        ..Default::default()
    });

    // ▶✂ is the program's own way in, and `cut_play::pressed`'s S2 SWITCHES from the recording preview
    // (it keeps `playing`, it does not start one), so the page starts cued -- what
    // tests/suite/cut_play_the_cut_widgets.rs seeds for the same reason. The press itself is the wire.
    ui::set_preview_state(&window, false, false, naivepost::run::Transport { playing: true, started: true });
    ui::play_cut_button(&window).expect("the Cut page carries play-cut-button")
        .emit_by_name::<()>("clicked", &[]);
    let started = ui::preview_player(&window).transport;
    assert!(started.playing && started.started, "▶✂ carries a playing preview onto the cut: {started:?}");
    assert!(ui::preview_player(&window).cut_only, "and the preview is now the plain cut");

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    play.emit_by_name::<()>("clicked", &[]);
    let paused = ui::preview_player(&window).transport;
    assert!(!paused.playing && paused.started, "S2 paused the preview instead of starting a run: {paused:?}");
    assert!(ui::run_bar(&window).expect("the window carries a run bar").borrow().running.is_none(),
        "S2 stops before the snapshot and the step, so no run opened");

    play.emit_by_name::<()>("clicked", &[]);
    let resumed = ui::preview_player(&window).transport;
    assert!(resumed.playing, "a second press resumes the parked preview: {resumed:?}");
    assert!(ui::run_bar(&window).expect("still no run").borrow().running.is_none());

    // ⏹ (F0.3) ends it, and with nothing playing ▶ is the step's button again.
    ui::stop_button(&window).expect("the run bar has a ⏹").emit_by_name::<()>("clicked", &[]);
    assert!(!ui::preview_player(&window).transport.playing, "⏹ stopped the preview");
    assert_eq!(ui::play_tooltip(&window).as_deref(), Some(run::PLAY_TOOLTIP),
        "with nothing playing the run bar's ▶ shows its step face again");
    window.close();
}

#[test]
fn f0_2_pressing_the_button_starts_the_visible_pages_step() {
    // A working directory of its own: the app keeps `session.naivepost` there, and test files run side by side.
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_PRESS.load(std::sync::atomic::Ordering::SeqCst),
        "the ▶ check never ran"
    );
}

/// The button opens with §2's tooltip; one press starts Prepare's step, which makes the run busy and
/// so turns the same button into ⏸ "Pause"; the next press pauses that run, and the bar's sentence —
/// "pausing after the current stage…" — reaches the window's status line. Nothing is decided here
/// that [`run`] does not decide.
fn check_press_forwards(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PLAY_TOOLTIP),
        "an idle ▶ says what §2 makes it say"
    );

    // Press 1: S3 + S4 — nothing was busy, so Prepare's step started. F0.5 S4 closes what a single-step
    // press opens (running off, the audio unload handed over), so the ⏸ face is pinned while that run
    // stands: the same open run ▶ leaves behind mid-step, stood up here through `start_run` because
    // headless the whole step walks inside one click.
    // Press 1: S3 + S4 — nothing was busy, so Prepare's step started. F0.5 S4 makes a finished
    // single-step press close its own bookkeeping, and headless the whole step walks inside one click,
    // so the ⏸ face is pinned over the run that is still standing: `start_run`, the call ▶ itself makes
    // before it walks the step.
    play.emit_clicked();
    stand_a_run_open(&window, &model);
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PAUSE_TOOLTIP),
        "a run under way turns ▶ into ⏸"
    );
    assert_eq!(
        ui::state(&window).status,
        "frames 1/2: frame",
        "F0.5 S2/§2: the queue's joined track line IS the status line while the run owns it"
    );

    // Press 2: S1 — the run is paused rather than restarted, and the bar has something to say.
    play.emit_clicked();
    assert_eq!(
        ui::state(&window).status,
        run::PAUSING,
        "the status line carries the bar's sentence"
    );
    // Paused means nothing to pause, so the button shows ▶ again — while ⏹ (F0.3) still has the run.
    assert_eq!(ui::play_tooltip(&window).as_deref(), Some(run::PLAY_TOOLTIP));

    // Press 3 resumes it.
    play.emit_clicked();
    assert_eq!(ui::state(&window).status, run::RESUMED);
}

/// Stand an open run on this window's bar, the way a step in progress has one: `runqueue::start_run`,
/// the same call ▶ makes before it walks the step. Needed since F0.5 S4 made a finished single-step
/// press close its own bookkeeping -- headless there is no half-finished step to observe otherwise.
fn stand_a_run_open(window: &adw::ApplicationWindow, model: &naivepost::project::Project) {
    let bar = ui::run_bar(window).expect("the window carries a run bar");
    let queue = ui::run_queue(window).expect("the window carries a queue");
    naivepost::runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        naivepost::run::Step::Prepare,
        naivepost::run::snapshot_sources(model),
    );
    // Repaint the buttons from the bar the way ▶'s own tail does, so the face under test is the one the
    // shell draws rather than a widget nobody refreshed.
    if let (Some(play), Some(stop_)) = (ui::play_button(window), ui::stop_button(window)) {
        play.set_tooltip_text(Some(naivepost::run::PAUSE_TOOLTIP));
        stop_.set_sensitive(true);
    }
}

/// The flag above: a second `#[test]` on the same shared app, so a check that never ran cannot pass.
#[test]
fn f0_2_s2_a_started_preview_owns_the_run_button() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_TRANSPORT.load(std::sync::atomic::Ordering::SeqCst),
        "the transport-toggle check never ran"
    );
}

/// S3 through the widgets: what ▶ records is the session as it stands at the press, not the
/// project the window was built with. A file added afterwards (F0.12's own copy path) must be in
/// the snapshot.
fn check_the_snapshot_names_the_live_sources(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let footage = model
        .sources
        .iter()
        .find(|source| source.footage)
        .expect("the fixture carries a footage row")
        .path
        .clone();
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    // `add_files` takes its project as `startup::session_dir(cwd)`, so cwd moves to a throwaway root
    // first -- the arrangement tests/suite/add_sources_widgets.rs uses to keep copies out of `rust/`.
    let root = std::env::temp_dir().join(format!("naivepost-f02s3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let late = root.join("late.mkv");
    std::fs::write(&late, vec![b'x'; 5]).unwrap();
    std::env::set_current_dir(&root).unwrap();
    assert!(ui::add_files(&window, &[late]), "the late file becomes a row");

    ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
    // F0.5 S4 closes a single-step press's bookkeeping, so the snapshot lives where the shell keeps the
    // last run's -- written by ▶ itself at `start_run`, which is what this checks.
    let running = naivepost::ui::run_close::last_snapshot_for_test().expect("a run opened");
    assert!(
        running.footage.contains(&footage),
        "the fixture's recording is in the snapshot: {:?}",
        running.footage
    );
    assert!(
        running.footage.iter().any(|path| path.ends_with("late.mkv")),
        "a source added AFTER the window was built is in the snapshot: {:?}",
        running.footage
    );
    ui::stop_button(&window).expect("the run bar has a ⏹").emit_by_name::<()>("clicked", &[]);
    window.close();
}

#[test]
fn f0_2_s3_the_snapshot_names_the_live_sources() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_SNAPSHOT.load(std::sync::atomic::Ordering::SeqCst),
        "the live-snapshot check never ran"
    );
}

/// S4's reporting through the widgets: the step's own subprocess failure reaches the log naming the
/// stage, and one step ending says no "all done" of its own (§F0.2 S4, §Failure-is-specific).
///
/// The failure is REAL, not staged: Prepare's `frames` stage spawns ffmpeg over each footage row, and
/// the fixture's `project:sources/lecture.mkv` is 18 bytes of placeholder text, so ffmpeg exits
/// non-zero on it (observed by dumping `ui::window_logs()` after the press -- the drive reported
/// `!!! frames failed — 'scening frames' exited 256`).
fn check_a_failed_stage_names_itself(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    // A run left open by an earlier check would make this press S2's toggle instead of S4's start.
    ui::stop_button(&window).expect("the run bar has a ⏹").emit_by_name::<()>("clicked", &[]);

    ui::play_button(&window).expect("the run bar has a ▶").emit_by_name::<()>("clicked", &[]);
    let logs = ui::window_logs();
    println!("LOGS after one Prepare press:\n{}", logs.join("\n"));
    assert!(
        logs.iter().any(|line| line.contains("frames") && line.contains("failed")),
        "the failing stage must name itself in the log, got: {logs:?}"
    );
    assert!(
        !logs.iter().any(|line| line.to_lowercase().contains("all done")),
        "one step finishing says nothing about being done: {logs:?}"
    );
    ui::stop_button(&window).expect("the run bar has a ⏹").emit_by_name::<()>("clicked", &[]);
    window.close();
}

#[test]
fn f0_2_s4_a_failed_stage_names_itself_in_the_log() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_FAILURE.load(std::sync::atomic::Ordering::SeqCst),
        "the failure-reporting check never ran"
    );
}

/// S1's two readings through the real button on the page whose own ▶ used to answer a busy bar with
/// "pausing…" by itself: Narrate refuses without a cut, so the bar is stood up first and both presses
/// then belong to it. The label text is read off `status-line` (through `ui::state`, the same widget
/// the person reads) AND off the bar's own `status`, so the wire and the rule are pinned together.
fn check_the_pause_toggle_says_both_readings(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Narrate");
    window.present();
    assert_eq!(ui::state(&window).page, naivepost::shell::Page::Narrate, "the Narrate tab bounced");
    // A run under way is what makes ▶ the pause button. `runqueue::start_run` needs a live audio.cpp
    // (F0.5 S1's preflight asks it), which this binary already serves for its other presses.
    let queue = ui::run_queue(&window).expect("the window carries a queue");
    naivepost::runqueue::start_run(
        &mut ui::run_bar(&window).expect("run bar").borrow_mut(),
        &mut queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        naivepost::run::Step::Narrate,
        naivepost::run::snapshot_sources(&model),
    );
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    assert!(bar.borrow().running.is_some(), "a run stands on the bar before the presses");

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    // The transport is parked, not playing: `tab_states::play` gives a *playing or cued* preview the
    // button ahead of the step (S2 beats S4), and on Narrate that would take this press away from S1.
    // A run under way still outranks it — that is what `RunBar::press`'s own order pins — but the shell
    // asks the player first, so the check starts from a quiet player, as a person who never pressed ▶✂
    // would.
    ui::set_preview_state(&window, false, false, naivepost::run::Transport::default());
    play.emit_by_name::<()>("clicked", &[]);
    assert!(
        bar.borrow().running.as_ref().is_some_and(|run| run.paused),
        "the press paused the run rather than refusing it"
    );
    assert_eq!(bar.borrow().status, run::PAUSING, "the bar's sentence is the pause reading");
    assert_eq!(
        ui::state(&window).status,
        run::PAUSING,
        "S1's first reading reaches the status line on the Narrate tab"
    );

    play.emit_by_name::<()>("clicked", &[]);
    assert!(
        bar.borrow().running.as_ref().is_some_and(|run| !run.paused),
        "the second press resumed the same run"
    );
    assert_eq!(bar.borrow().status, run::RESUMED, "the bar's sentence is the resume reading");
    assert_eq!(
        ui::state(&window).status,
        run::RESUMED,
        "S1's second reading reaches the status line, which no press could say before"
    );
    ui::stop_button(&window).expect("the run bar has a ⏹").emit_by_name::<()>("clicked", &[]);
    // ⏹ sets the stop flag (F0.3 S3); the bookkeeping comes down through the shell's own close door,
    // which is what a step ending calls, so the bar is empty again for whichever check runs next.
    naivepost::ui::run_close::close_step_run(
        &bar, &queue,
        &std::rc::Rc::new(std::cell::RefCell::new(naivepost::exchanges::RunLog::new())),
        &ui::find_status(window.upcast_ref()).expect("the window carries a status line"),
        &ui::progress_bar(&window).expect("the run bar has a progress widget"),
        &ui::log_expander(&window).expect("the shell has a log expander"),
        &ui::log_view(&window).expect("the expander's child is the log body"),
        None,
    );
    assert!(
        bar.borrow().running.is_none(),
        "the run this check stood up is closed again"
    );
    window.close();
}

#[test]
fn f0_2_s1_the_button_press_lands_both_pause_readings() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN_BOTH_READINGS.load(std::sync::atomic::Ordering::SeqCst),
        "the two-readings check never ran"
    );
}
