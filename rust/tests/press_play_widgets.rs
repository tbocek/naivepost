//! §03-shell.md F0.2 Press ▶ — through the widgets. The button on screen decides nothing: it draws
//! [`naivepost::run::controls`] and forwards its click to [`naivepost::run::RunBar::press`], so what
//! these checks assert is that the forwarding happened, not what a press means (spec/00-principles.md §5).


use adw::prelude::*;
use naivepost::run;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir};

/// One running GTK application for this test binary, taken in turn by the checks below — the same
/// single-main-loop arrangement tests/smoke.rs needs for the same reason (`g_application_run` refuses
/// a second claimant of the default main context). The presses happen inside one `activate`, in
/// order, because ▶'s meaning depends on what the press before it did.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_press_forwards(app);
            RAN_PRESS.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_PRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn f0_2_pressing_the_button_starts_the_visible_pages_step() {
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

    // Press 1: S3 + S4 — nothing was busy, so Prepare's step started. The button is now the pause
    // button, because a run is under way.
    play.emit_clicked();
    assert_eq!(
        ui::play_tooltip(&window).as_deref(),
        Some(run::PAUSE_TOOLTIP),
        "a run under way turns ▶ into ⏸"
    );
    assert_eq!(ui::state(&window).status, "", "the run's own progress owns the line");

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
