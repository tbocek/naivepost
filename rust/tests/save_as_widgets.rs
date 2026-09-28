//! §03-shell.md F0.10 Save as — through the widgets. The header bar's Save decides nothing: it asks
//! [`naivepost::save_as`] and forwards the chooser's answer, so what these checks assert is that a
//! click either opened the dialog or was refused, never that it saved (spec/00-principles.md §5).


use adw::prelude::*;
use naivepost::save_as;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir};

/// One running GTK application for this test binary, taken in turn by the two checks below — the
/// same single-main-loop arrangement tests/smoke.rs needs for the same reason. Each check records
/// that it ran, so one that never happened is a failure rather than a quiet pass.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_save_offers_the_project(app);
            RAN_OFFER.store(true, std::sync::atomic::Ordering::SeqCst);
            check_refused_during_a_run(app);
            RAN_REFUSED.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_OFFER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_REFUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// S2 through the button: it is there with §1's tooltip, and pressing it opens the chooser — which a
/// headless test never answers — so nothing is saved and the status line stays quiet.
#[test]
fn f0_10_s2_the_save_button_offers_the_project_for_naming() {
    window_round();
    assert!(
        RAN_OFFER.load(std::sync::atomic::Ordering::SeqCst),
        "the Save check never ran"
    );
}

/// S1 through the widgets: ▶ starts a run (F0.2), and Save then says why it will not go ahead.
#[test]
fn f0_10_s1_save_is_refused_while_a_run_is_on() {
    window_round();
    assert!(
        RAN_REFUSED.load(std::sync::atomic::Ordering::SeqCst),
        "the refused-during-a-run check never ran"
    );
}

fn check_save_offers_the_project(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let save_ = ui::save_button(&window).expect("the header bar has a Save button");
    assert_eq!(
        ui::save_tooltip(&window).as_deref(),
        Some("Save this project to a file"),
        "badge 3 of §1's header bar"
    );

    save_.emit_clicked();
    // The chooser is the desktop's and was never answered, so no branch of S3/S4 ran: nothing was
    // saved, nothing was refused.
    assert_eq!(ui::state(&window).status, "", "opening a dialog says nothing");
}

fn check_refused_during_a_run(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    // A run under way, started the way a user starts one.
    ui::play_button(&window).expect("the run bar has a ▶").emit_clicked();
    ui::save_button(&window)
        .expect("the header bar has a Save button")
        .emit_clicked();
    assert_eq!(ui::state(&window).status, save_as::RUN_REFUSAL);
}
