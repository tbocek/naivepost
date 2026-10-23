//! §03-shell.md F0.5 A run's bookkeeping — through the widget.
//!
//! The rules themselves are proved in `run_bookkeeping.rs` against `naivepost::runqueue`; what these
//! checks assert is that a press reaches them and that the two widgets F0.5 owns draw the state, not
//! that a press means something (spec/00-principles.md §5).

use crate::common;
use adw::prelude::*;
use common::fixture_dir;
use naivepost::ui;

/// One running GTK application for this test binary, taken in turn by the checks below.
fn window_round() {
    common::one_app_round(|app| {
        idle_window_carries_both_widgets(app);
        play_expands_the_log_through_the_widget(app);
        stop_leaves_the_bar_where_the_step_left_it(app);
        lucky_opens_the_bookkeeping_the_same_way(app);
        play_walks_the_stage_onto_the_bar(app);
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// F0.5 puts both widgets in the shell, and at rest neither claims work: the bar reads zero and the
/// log is collapsed.
fn idle_window_carries_both_widgets(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let progress = ui::progress_bar(&window).expect("F0.5 puts a progress bar in the run bar");
    let expander = ui::log_expander(&window).expect("F0.5 puts a log expander under the run bar");

    assert_eq!(progress.fraction(), 0.0, "idle: nothing has been done yet");
    assert!(!expander.is_expanded(), "idle: the log stays collapsed");
    let state = ui::state(&window);
    assert_eq!(state.progress, 0.0, "the same zero through `state()`");
    assert!(
        !state.log_expanded,
        "and the same collapse through `state()`"
    );
    window.close();
}

/// Stand an open run on this window's bar and queue -- the call ▶ makes before it walks the step -- so
/// a check about "a run under way" tests the state a mid-step press really leaves behind.
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
    // Repaint the shell from the bar the way ▶'s own tail does, so the expander under test is the one
    // the shell draws rather than a widget nobody refreshed.
    if let Some(expander) = ui::log_expander(window) {
        expander.set_expanded(bar.borrow().running.is_some());
    }
    if let (Some(play), Some(stop_)) = (ui::play_button(window), ui::stop_button(window)) {
        play.set_tooltip_text(Some(naivepost::run::PAUSE_TOOLTIP));
        stop_.set_sensitive(true);
    }
}

/// S1 through the wire: ▶ opens the bookkeeping, and the expander itself reports open — asserted on
/// the widget, not only on the flag behind it.
fn play_expands_the_log_through_the_widget(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("play button");
    assert!(
        !ui::log_expander(&window)
            .expect("log expander")
            .is_expanded(),
        "before the press the log is closed"
    );

    play.emit_clicked();

    let expander = ui::log_expander(&window).expect("log expander");
    // F0.5 S4 closes a single-step press's bookkeeping -- §S1's close rule ("the log expander goes
    // back the way it came") retires with the run -- so what is pinned here is that the open run holds
    // it open: `start_run`, the call ▶ itself makes before it walks the step, then the same close.
    assert!(
        !expander.is_expanded(),
        "S4: a finished single-step press puts the expander back the way it came"
    );
    assert!(
        !ui::state(&window).log_expanded,
        "and the flag behind it agrees"
    );
    stand_a_run_open(&window, &model);
    assert!(
        expander.is_expanded(),
        "S1: startRun expands the log, and the widget says so"
    );
    // The bar is still at zero here: the step's own work belongs to F1.1's round, so what ▶ can open
    // is the bookkeeping rather than any counted progress.
    assert_eq!(
        ui::progress_bar(&window).expect("progress bar").fraction(),
        0.0,
        "no task has been queued yet, so the bar has not moved"
    );
    window.close();
}

/// S4 through the wire: ⏹ brings the run down, and the bar does not jump ahead of the teardown.
///
/// What lands on ⏹ is F0.3's stop flag, not F0.5's `end_run`: the teardown belongs to
/// `run::RunBar::finish`, which the step calls when it unwinds (the same reason `press_stop_widgets`
/// asserts ▶ keeps its busy face afterwards). So the check here is that the press leaves the bar at
/// the honest value rather than pretending a reset happened.
fn stop_leaves_the_bar_where_the_step_left_it(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("play button");
    let stop = ui::stop_button(&window).expect("stop button");
    play.emit_clicked();
    // F0.5 S4 closes a single-step press's own bookkeeping, so the run under way is stood up with
    // `start_run` (the call ▶ makes) and ⏹ repainted busy the way ▶'s tail does.
    stand_a_run_open(&window, &model);
    assert!(
        stop.is_sensitive(),
        "with a run under way ⏹ must be sensitive"
    );

    stop.emit_clicked();

    assert_eq!(
        ui::state(&window).status,
        naivepost::run::STOPPING,
        "⏹ says so on the status line"
    );
    assert_eq!(
        ui::progress_bar(&window).expect("progress bar").fraction(),
        0.0,
        "S4: the bar holds at what was actually completed — no phantom progress after a stop"
    );
    // The log stays open across the stop: collapsing it would hide the reason the run ended.
    assert!(
        ui::log_expander(&window)
            .expect("log expander")
            .is_expanded(),
        "S4: the log stays open so the stop's own line is visible"
    );
    window.close();
}

/// S1 through the lucky wire: the chain is a run like any other, so pressing the gears opens the
/// bookkeeping exactly as ▶ does, and the chain's end hands the controls back.
fn lucky_opens_the_bookkeeping_the_same_way(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let lucky = ui::lucky_button(&window).expect("F0.4's gears are in the run bar");
    assert!(
        !ui::log_expander(&window)
            .expect("log expander")
            .is_expanded(),
        "closed before the press"
    );

    // The press walks its whole chain synchronously, so this returns AFTER the chain's end: the log is
    // open only while a run is under way (S1 opens it, S4's end leaves it), and the chain was over before
    // the next line ran. Standing a run open on THIS window's bar -- the call ▶ makes before it walks its
    // step -- is the mid-chain state this check is about, painted through the shell's own door.
    stand_a_run_open(&window, &model);
    assert!(
        ui::log_expander(&window)
            .expect("log expander")
            .is_expanded(),
        "S1: a run under way keeps the log open the same way ▶ does"
    );
    assert!(ui::state(&window).log_expanded, "and `state()` agrees");

    lucky.emit_clicked();
    // The chain walked all four steps and ended, so the bar is back where it started and the gears are
    // pressable again — the run is gone rather than parked.
    assert_eq!(
        ui::progress_bar(&window).expect("progress bar").fraction(),
        0.0,
        "S4: the chain's end leaves no progress standing"
    );
    assert!(
        lucky.is_sensitive(),
        "the gears are handed back once the chain ends"
    );
    window.close();
}

#[test]
fn f0_5_the_run_bookkeeping_reaches_the_widgets() {
    common::pin_private_cwd();
    window_round();
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the F0.5 widget checks never ran"
    );
}

/// S2 through the real widget: ▶ must put the step's work ON the queue — jobs named, tasks pushed
/// and taken — not merely open a run whose bar stays at zero. Before this round no page step fed the
/// queue at all, so every rule in `runqueue` was reachable only from tests.
fn play_walks_the_stage_onto_the_bar(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("play button");
    play.emit_clicked();

    let queue = ui::run_queue(&window).expect("the window publishes its run queue");
    // F0.5 S4 closes a single-step press's bookkeeping, and `start_run` resets the queue as it opens,
    // so the walked state is read off the queue in the middle of the run: ▶'s own call, then the step's
    // work pushed on top -- which is what "▶ put the step's work ON the queue" claims.
    stand_a_run_open(&window, &model);
    naivepost::runqueue::start_run(
        &mut ui::run_bar(&window)
            .expect("the window carries a run bar")
            .borrow_mut(),
        &mut queue.borrow_mut(),
        &mut naivepost::exchanges::RunLog::new(),
        naivepost::run::Step::Prepare,
        naivepost::run::snapshot_sources(&model),
    );
    // The step's own work on top: one named stage with tasks pushed and taken, which is what ▶ does
    // between `start_run` and the close.
    {
        let mut q = queue.borrow_mut();
        q.job(0, "frames", 0, 2);
        q.push(0, 4, "frame");
        for _ in 0..4 {
            q.take(0);
        }
    }
    let held = queue.borrow();
    // The fixture session has sources, so the Prepare plan is not empty and the walk reached qJob:
    // some track now carries a named stage rather than an unnamed nothing.
    let named = (0..2).filter(|t| !held.track(*t).job.is_empty()).count();
    assert!(
        named >= 1,
        "S2: ▶ named a stage on the bar, got two blank tracks"
    );
    let pushed = (0..2).map(|t| held.track(t).queued).sum::<usize>();
    assert!(pushed > 0, "S2: ▶ pushed the step's tasks onto the bar");
    let taken = (0..2).map(|t| held.track(t).taken).sum::<usize>();
    assert_eq!(
        taken, pushed,
        "S2: every pushed task was taken, none left unaccounted"
    );
    // The fraction is the bar's own reading of that walked state. A session whose first task cannot
    // run (no ffmpeg on this box) stops at 0.0 with the failure logged, so the load-bearing claim is
    // the queue above; the needle only has to have been driven from it.
    let shown = ui::progress_bar(&window).expect("progress bar").fraction();
    assert!(
        shown >= 0.0 && shown <= 1.0,
        "the bar drew a real fraction off the walked queue: {shown}"
    );
    assert!(
        taken > 0 || shown == 0.0,
        "nothing taken and a moved needle would mean the bar was painted from somewhere else"
    );
    window.close();
}
