//! §03-shell.md F0.5 A run's bookkeeping — through the widget.
//!
//! The rules themselves are proved in `run_bookkeeping.rs` against `naivepost::runqueue`; what these
//! checks assert is that a press reaches them and that the two widgets F0.5 owns draw the state, not
//! that a press means something (spec/00-principles.md §5).


use adw::prelude::*;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir};

/// One running GTK application for this test binary, taken in turn by the checks below.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            idle_window_carries_both_widgets(app);
            play_expands_the_log_through_the_widget(app);
            stop_leaves_the_bar_where_the_step_left_it(app);
            lucky_opens_the_bookkeeping_the_same_way(app);
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
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
    assert!(!state.log_expanded, "and the same collapse through `state()`");
    window.close();
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
    assert!(
        expander.is_expanded(),
        "S1: startRun expands the log, and the widget says so"
    );
    assert!(ui::state(&window).log_expanded, "the same through `state()`");
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
    assert!(stop.is_sensitive(), "with a run under way ⏹ must be sensitive");

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
        ui::log_expander(&window).expect("log expander").is_expanded(),
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
        !ui::log_expander(&window).expect("log expander").is_expanded(),
        "closed before the press"
    );

    lucky.emit_clicked();

    assert!(
        ui::log_expander(&window).expect("log expander").is_expanded(),
        "S1: the chain opened the log the same way ▶ does"
    );
    assert!(ui::state(&window).log_expanded, "and `state()` agrees");
    // The chain walked all four steps and ended, so the bar is back where it started and the gears are
    // pressable again — the run is gone rather than parked.
    assert_eq!(
        ui::progress_bar(&window).expect("progress bar").fraction(),
        0.0,
        "S4: the chain's end leaves no progress standing"
    );
    assert!(lucky.is_sensitive(), "the gears are handed back once the chain ends");
    window.close();
}

#[test]
fn f0_5_the_run_bookkeeping_reaches_the_widgets() {
    window_round();
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the F0.5 widget checks never ran"
    );
}
