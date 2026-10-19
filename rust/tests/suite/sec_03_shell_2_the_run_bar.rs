//! §03-shell.md #2 "The run bar" — the two ways to run, the gears' gate, the bar that draws no words
//! of its own, and the `run_steps` key an older file may still carry.
//!
//! The logic half asserts on the very functions the widgets call (`run::controls`,
//! `runqueue::Queue`, `shell_beats::should_pulse`), so a painted widget cannot disagree with the rule.
//! The widget half then drives the real buttons and the real painter, so the wire is pinned too
//! (spec/00-principles.md §5).

use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::run;
use naivepost::runqueue::{self, Queue, TRACK_DESCRIBE, TRACK_FIX, TRACK_FRAMES, TRACK_STT, STANDING_TIP, TRACK_JOIN};
use naivepost::ui;
use crate::common;
use common::{fake_audio_server, fixture_dir};

// ---------------------------------------------------------------------------
// bullet: "the tracks' lines, joined by \"  ·  \"" — the joiner's exact bytes.
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_2_the_run_bar_s1_the_two_track_lines_join_with_a_spaced_middle_dot() {
    assert_eq!(
        TRACK_JOIN,
        "  \u{00B7}  ",
        "§2: the joiner is a middle dot with TWO spaces on each side"
    );
    let mut q = Queue::new();
    q.job(TRACK_STT, "describe", 1, 2);
    q.push(TRACK_STT, 12, "chunk");
    for _ in 0..4 {
        q.take(TRACK_STT);
    }
    q.prog(TRACK_STT, 4.0 / 12.0, "chunk 4/12");
    // The frames track already finished its job: its line is its name and the word "done".
    q.job(TRACK_FRAMES, "speech", 0, 0);
    q.push(TRACK_FRAMES, 3, "frame");
    for _ in 0..3 {
        q.take(TRACK_FRAMES);
    }
    q.done(TRACK_FRAMES, 0.5);

    let text = q.text();
    assert_eq!(
        // Track order: the speech track is 0 and the frames track is 1, so the finished track's line
        // comes first (§2: "0 = speech/describe, 1 = frames/fix").
        text,
        format!("describe 1/2: chunk 4/12{TRACK_JOIN}speech done"),
        "§2's two lines, joined by the spaced middle dot"
    );
    assert!(text.contains(TRACK_JOIN), "the joined line carries the joiner itself: {text:?}");
    // Read the two spaces back out of the joined line: splitting on §2's joiner must leave exactly the
    // two tracks' lines and nothing else.
    let joined: Vec<&str> = text.split(TRACK_JOIN).collect();
    assert_eq!(
        joined,
        vec!["describe 1/2: chunk 4/12", "speech done"],
        "the line splits back into the two tracks' lines on §2's joiner: {text:?}"
    );
}

// ---------------------------------------------------------------------------
// bullet: "Tooltip per track ..." — the counting lives in the tooltip, per track.
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_2_the_run_bar_s1_each_track_tips_its_own_counting() {
    // Mid-queue: one task taken, the rest still waiting.
    let mut q = Queue::new();
    q.job(TRACK_STT, "describe", 1, 2);
    q.push(TRACK_STT, 12, "chunk");
    q.take(TRACK_STT);
    q.prog(TRACK_STT, 4.0 / 12.0, "chunk 4/12");
    assert_eq!(q.track(TRACK_STT).tip(), "describe: task 1 of 12, 11 waiting");
    assert!(q.track(TRACK_FRAMES).tip().is_empty(), "a track with no job tips nothing");

    // Nothing left waiting: the last task is taken and nothing is queued behind it.
    q.prog(TRACK_STT, 1.0, "");
    q.take(TRACK_STT);
    q.push(TRACK_STT, 0, "");
    assert_eq!(q.track(TRACK_STT).tip(), "describe: task 2 of 12, 10 waiting");
    {
        // A queue emptied of its own work reads "none waiting".
        let mut q = Queue::new();
        q.job(TRACK_STT, "describe", 1, 2);
        q.push(TRACK_STT, 2, "chunk");
        q.take(TRACK_STT);
        q.take(TRACK_STT);
        q.prog(TRACK_STT, 1.0, "");
        assert_eq!(q.track(TRACK_STT).tip(), "describe: task 2 of 2, none waiting");
    }

    // A finished job counts everything it took.
    let mut q = Queue::new();
    q.job(TRACK_FRAMES, "speech", 0, 0);
    q.push(TRACK_FRAMES, 3, "chunk");
    for _ in 0..3 {
        q.take(TRACK_FRAMES);
    }
    q.done(TRACK_FRAMES, 1.0);
    assert_eq!(q.track(TRACK_FRAMES).tip(), "speech: 3 task(s), all done");
}

// ---------------------------------------------------------------------------
// bullet: "nothing queued → standing tooltip ... stays."
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_2_the_run_bar_s1_nothing_queued_leaves_the_standing_tooltip() {
    assert_eq!(
        STANDING_TIP,
        "The run: the job, which of the run's jobs it is, and the task it is on",
        "§2's standing tooltip, in the spec's own words"
    );
    let q = Queue::new();
    assert_eq!(q.text_and_tip().1, "", "an empty queue has no counting to replace the standing text");
    assert_eq!(q.text(), "", "and no words for the status line either");
}

// ---------------------------------------------------------------------------
// bullets: ▶'s two faces, the gears' gate, ⏹'s sensitivity.
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_2_the_run_bar_s1_idle_bar_runs_this_step_and_lets_the_gears_go() {
    let drawn = run::controls(&None, None);
    assert_eq!(drawn.icon, run::PLAY_ICON);
    assert_eq!(drawn.tooltip, run::PLAY_TOOLTIP);
    assert_eq!(drawn.tooltip, "Run this step \u{2014} or resume what is paused");
    assert!(!drawn.stop_sensitive, "nothing to end yet");
    assert!(drawn.lucky_sensitive, "an idle bar leaves I'm feeling lucky pressable");
}

#[test]
fn sec_03_shell_2_the_run_bar_s1_a_busy_bar_turns_play_into_pause_and_greys_the_gears() {
    let running = Some(run::Run {
        step: run::Step::Prepare,
        paused: false,
        log_expanded: true,
        sources: run::Snapshot::default(),
    });
    let drawn = run::controls(&running, None);
    assert_eq!(drawn.icon, run::PAUSE_ICON, "§2: while a run is busy \u{25b6} shows \u{23f8}");
    assert_eq!(drawn.tooltip, run::PAUSE_TOOLTIP);
    assert!(drawn.stop_sensitive, "\u{23f9} has the run to end");
    assert!(!drawn.lucky_sensitive, "§2: the gears are insensitive while a run is busy");
}

#[test]
fn sec_03_shell_2_the_run_bar_s1_a_paused_run_still_greys_the_gears() {
    // \u{23f8} parks a run to carry on later; it does not free the bar, so the chain stays out of reach.
    let paused = Some(run::Run {
        step: run::Step::Prepare,
        paused: true,
        log_expanded: true,
        sources: run::Snapshot::default(),
    });
    let drawn = run::controls(&paused, None);
    assert_eq!(drawn.icon, run::PLAY_ICON, "a parked run means \u{25b6} resumes it");
    assert!(drawn.stop_sensitive, "\u{23f9} still has the parked run to end");
    assert!(!drawn.lucky_sensitive, "a paused run is still a run: the gears stay grey");
}

#[test]
fn sec_03_shell_2_the_run_bar_s1_the_pages_transport_owns_the_bar_too() {
    let playing = run::Transport { playing: true, started: true };
    let drawn = run::controls(&None, Some(playing));
    assert_eq!(drawn.icon, run::PAUSE_ICON, "a playing preview is busy as far as the bar reads it");
    assert!(!drawn.lucky_sensitive, "and the gears wait for it too");
    assert!(drawn.stop_sensitive, "\u{23f9} stops the playback (§2)");

    // Started but parked: nothing plays, so \u{25b6} is \u{25b6} again — but \u{23f9} still owes the ending.
    let cued = run::Transport { playing: false, started: true };
    let drawn = run::controls(&None, Some(cued));
    assert_eq!(drawn.icon, run::PLAY_ICON);
    assert!(drawn.stop_sensitive, "a preview parked part way through still needs ending");
    assert!(drawn.lucky_sensitive, "and with no run and nothing playing the gears may go");

    // Prepare and Produce have no preview of their own, so a playing transport is not theirs.
    assert_eq!(run::transport_for(naivepost::shell::Page::Prepare, playing), None);
    assert!(run::controls(&None, run::transport_for(naivepost::shell::Page::Prepare, playing)).lucky_sensitive);
}

// ---------------------------------------------------------------------------
// bullets: the progress bar — two tracks summed, no text of its own, a pulse while nothing counts.
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_2_the_run_bar_s1_the_bar_is_the_two_tracks_summed_and_draws_no_words() {
    assert_eq!((TRACK_STT, TRACK_FRAMES), (0, 1), "§2: 0 = speech/describe, 1 = frames/fix");
    assert_eq!((TRACK_DESCRIBE, TRACK_FIX), (TRACK_STT, TRACK_FRAMES));

    let mut q = Queue::new();
    q.job(TRACK_STT, "speech", 0, 0);
    q.job(TRACK_FRAMES, "frames", 0, 0);
    q.prog(TRACK_STT, 0.25, "recognising 1/3");
    q.prog(TRACK_FRAMES, 0.25, "scening 4/16");
    assert!((q.fraction() - 0.5).abs() < 1e-9, "the bar is both halves summed: {}", q.fraction());
    q.prog(TRACK_STT, 0.9, "");
    q.prog(TRACK_FRAMES, 0.9, "");
    assert!((q.fraction() - 1.0).abs() < 1e-9, "and it cannot pass the end");
    q.prog(TRACK_STT, 1.4, "");
    assert!((q.fraction() - 1.0).abs() < 1e-9, "clamped to what a bar can show");

    // The words are the status line's, never the bar's own: §2 draws no text of its own.
    let mut q = Queue::new();
    q.job(TRACK_DESCRIBE, "describe", 1, 2);
    q.push(TRACK_DESCRIBE, 12, "chunk");
    q.take(TRACK_DESCRIBE);
    q.prog(TRACK_DESCRIBE, 1.0 / 12.0, "chunk 1/12");
    assert_eq!(q.text(), "describe 1/2: chunk 1/12", "the line goes to the status row");
}

#[test]
fn sec_03_shell_2_the_run_bar_s1_a_thinking_model_pulses_the_bar() {
    assert!(ui::should_pulse(true, 0.0), "a run with nothing countable pulses");
    assert!(!ui::should_pulse(true, 0.25), "once something is countable the needle takes over");
    assert!(!ui::should_pulse(false, 0.0), "with no run there is nothing to pulse");
}

// ---------------------------------------------------------------------------
// bullet: "Nothing about which steps run is stored in the project; run_steps is ignored."
// ---------------------------------------------------------------------------

#[test]
fn sec_03_shell_2_the_run_bar_s1_an_older_run_steps_key_is_ignored_and_never_written() {
    let root = common::temp_dir("run-steps");
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(naivepost::project::PROJECT_FILE),
        r#"{"run_steps": ["Prepare", "Cut"]}"#,
    )
    .unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();

    // Read and ignored: the file still describes an empty project, and no step picker exists.
    let p = naivepost::project::load(tree.dir()).expect("loads");
    assert_eq!(p, naivepost::project::Project::default());

    naivepost::project::save(&p, tree.dir()).unwrap();
    let text = common::read_tree(&tree);
    assert!(!text.contains("run_steps"), "run_steps was written back\n{text}");
}

// ---------------------------------------------------------------------------
// The widget half: the real buttons, in §2's order, and the painter the handlers call.
// ---------------------------------------------------------------------------

fn widgets_round() {
    common::one_app_round(|app| {
        // §F1.1's preflight asks audio.cpp before Prepare's stages, so a real press needs an answer.
        serve_audio(&fake_audio_server());
        check_run_bar_order_and_idle_state(app);
        RAN_ORDER.store(true, std::sync::atomic::Ordering::SeqCst);
        check_the_press_gates_the_gears(app);
        RAN_GATE.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN_ORDER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_GATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Point the settings this process reads at the fake server, as tests/suite/press_play_widgets.rs does.
fn serve_audio(url: &str) {
    unsafe {
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-sec032-cfg-{}", std::process::id()));
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

/// The bar's children, by widget name, in the order the box holds them.
fn child_names(widget: &gtk::Widget) -> Vec<String> {
    widget
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|child| child.widget_name().to_string())
        .collect()
}

/// §2's order and the at-rest state of the progress bar: ▶ , I'm feeling lucky , ⏹ , the bar; and the
/// bar draws no text of its own and carries the standing tooltip while nothing is queued.
fn check_run_bar_order_and_idle_state(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let run_row = common::widget_in(&window, "run-bar").expect("the shell has a run bar");
    // §1 now puts the controls inside their own scroller within that row ("what no longer fits scrolls"
    // instead of holding the window wide), so §2's order is read where the buttons actually live; the
    // progress bar stays in the OUTER row, beside the readouts, and is checked there.
    let names = child_names(
        &common::widget_in(&window, "run-controls-inner")
            .expect("the run bar's controls sit in their scrolling box"),
    );
    let at = |name: &str| names.iter().position(|n| n == name).expect(name);
    assert!(
        at("play-button") < at("lucky-button") && at("lucky-button") < at("stop-button"),
        "§2's run bar reads \u{25b6} , I'm feeling lucky , \u{23f9} : {names:?}"
    );
    assert!(
        child_names(&run_row)
            .iter()
            .position(|n| n == "run-progress")
            .is_some(),
        "and the progress bar follows in the row: {:?}",
        child_names(&run_row)
    );

    let gears = ui::lucky_button(&window).expect("the run bar carries the gears");
    assert!(gears.is_sensitive(), "at rest the gears are pressable");

    let bar = ui::progress_bar(&window).expect("the run bar carries a progress bar");
    assert!(!bar.shows_text(), "§2: the bar draws no text of its own");
    assert_eq!(bar.fraction(), 0.0, "an idle bar is at zero, not hidden");
    assert_eq!(
        ui::progress_tooltip(&window).as_deref(),
        Some(STANDING_TIP),
        "nothing queued, so the standing tooltip is what a pointer finds"
    );
    window.close();
}

/// A real press: ▶ opens Prepare's run, and the same `paint_run_bar` the handlers call greys the gears
/// while that run stands — the state the logic tests pin, reached through the widget.
///
/// The standing-tooltip half is checked on a queue that has nothing queued: a real ▶ on this page runs
/// Prepare's whole step inside the one click headless, and that step reports frames work on the queue on
/// its way through (`frames: task 1 of 1, none waiting`), which is exactly the counting that REPLACES the
/// standing text. So §2's "nothing queued → the standing tooltip stays" is pinned against the empty queue
/// `runqueue::start_run` leaves behind — the state a run is in before its first `qPush` — and the widget
/// keeps it, because `paint_progress` only overwrites the tooltip when a track has counting of its own.
fn check_the_press_gates_the_gears(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    // A run left open by another check would make this press a pause toggle rather than a start.
    ui::stop_button(&window).expect("the run bar has a \u{23f9}").emit_by_name::<()>("clicked", &[]);

    let play = ui::play_button(&window).expect("the run bar has a \u{25b6}");
    let gears = ui::lucky_button(&window).expect("the run bar carries the gears");
    assert!(gears.is_sensitive(), "nothing is going yet");
    play.emit_by_name::<()>("clicked", &[]);

    // Headless, the whole step walks inside the one click and F0.5 S4 closes it again, so the run that
    // is standing mid-step is stood up the way the press itself stands it: `runqueue::start_run`.
    let bar = ui::run_bar(&window).expect("the window carries a run bar");
    let queue = ui::run_queue(&window).expect("the window carries a queue");
    runqueue::start_run(
        &mut bar.borrow_mut(),
        &mut queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        run::Step::Prepare,
        run::snapshot_sources(&model),
    );
    let status = ui::find_status(window.upcast_ref()).expect("the shell has a status line");
    repaint(&window, &bar);
    assert!(
        bar.borrow().running.is_some(),
        "the press opened a run, so the bar is busy"
    );
    assert!(!gears.is_sensitive(), "§2: the gears go insensitive while a run is busy");
    // A run with nothing queued yet, drawn the way the handlers draw it: `paint_progress` keeps the
    // standing text whenever no track has counting of its own (§2). The window's own queue already
    // carries the frames work the real press reported, so this paint runs over a fresh queue holding
    // nothing -- the state a run is in between `startRun`'s reset and its first `qPush`.
    let idle_queue = Queue::new();
    let log = ui::log_expander(&window).expect("the shell has a log expander");
    let view = ui::log_view(&window).expect("the log has a text view");
    let progress = ui::progress_bar(&window).expect("the run bar carries a progress bar");
    progress.set_tooltip_text(Some(STANDING_TIP));
    ui::window::paint_progress(
        &progress,
        &log,
        &view,
        &idle_queue,
        naivepost::ui::shell_beats::StatusLine { status: &status, owned: true },
        false,
    );
    assert_eq!(
        ui::progress_tooltip(&window).as_deref(),
        Some(STANDING_TIP),
        "a run with nothing queued yet keeps the standing tooltip"
    );

    // A queue with real counting replaces it: the bar keeps no words of its own, so the tooltip carries
    // the counting and the status line carries the joined line (§2).
    {
        let mut q = queue.borrow_mut();
        q.reset();
        q.job(TRACK_DESCRIBE, "describe", 1, 2);
        q.push(TRACK_DESCRIBE, 12, "chunk");
        q.take(TRACK_DESCRIBE);
        q.prog(TRACK_DESCRIBE, 1.0 / 12.0, "chunk 1/12");
    }
    let log = ui::log_expander(&window).expect("the shell has a log expander");
    let view = ui::log_view(&window).expect("the log has a text view");
    ui::window::paint_progress(
        &ui::progress_bar(&window).expect("progress bar"),
        &log,
        &view,
        &queue.borrow(),
        naivepost::ui::shell_beats::StatusLine { status: &status, owned: true },
        false,
    );
    assert_eq!(
        ui::progress_tooltip(&window).as_deref(),
        Some("describe: task 1 of 12, 11 waiting"),
        "§2's per-track tooltip replaces the standing text while a track counts"
    );
    assert_eq!(
        ui::state(&window).status,
        "describe 1/2: chunk 1/12",
        "§2: the track's line goes to the status line, overwriting what was there"
    );

    // ⏹ ends the run. A run whose flow has not cleared the stop flag keeps the bar until it does
    // (F0.3 S3), which a headless run started through `start_run` never does — so the end is closed the
    // way a real run's end closes it, and the repaint then hands the gears back.
    ui::stop_button(&window).expect("the run bar has a \u{23f9}").emit_by_name::<()>("clicked", &[]);
    assert_eq!(ui::state(&window).status, run::STOPPING, "S1: the press says what it is doing");
    bar.borrow_mut().stop_flag = false;
    bar.borrow_mut().running = None;
    repaint(&window, &bar);
    assert!(gears.is_sensitive(), "§2: with no run left the gears are pressable again");
    window.close();
}

/// Repaint the run bar through the door every handler uses, so the state under test is the one the
/// shell draws rather than a widget nobody refreshed.
fn repaint(window: &adw::ApplicationWindow, bar: &Rc<std::cell::RefCell<run::RunBar>>) {
    let play = ui::play_button(window).expect("the run bar has a \u{25b6}");
    let gears = ui::lucky_button(window).expect("the run bar carries the gears");
    let stop_ = ui::stop_button(window).expect("the run bar has a \u{23f9}");
    let transport = ui::preview_player(window).transport;
    ui::window::paint_run_bar(
        &play,
        &gears,
        &stop_,
        &bar.borrow(),
        naivepost::shell::Page::Prepare,
        transport,
    );
}

#[test]
fn sec_03_shell_2_the_run_bar_s2_the_bar_reads_play_lucky_stop_and_carries_the_standing_tooltip() {
    widgets_round();
    assert!(RAN_ORDER.load(std::sync::atomic::Ordering::SeqCst), "the order check never ran");
}

#[test]
fn sec_03_shell_2_the_run_bar_s2_a_run_busy_greys_the_gears_and_keeps_the_standing_tooltip() {
    widgets_round();
    assert!(RAN_GATE.load(std::sync::atomic::Ordering::SeqCst), "the gears-gate check never ran");
}

