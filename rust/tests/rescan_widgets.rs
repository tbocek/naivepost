//! §03-shell.md F0.11 Rescan — through the widgets. The header bar's ⟳ decides nothing: it calls
//! [`naivepost::rescan`] and draws the answer, so what this check asserts is that a click named the
//! row it dropped and said "rescanned", never that anything was re-rendered (spec/00-principles.md §5).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::rescan;
use naivepost::ui;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir};

/// One running GTK application for this test binary — the same single-main-loop arrangement
/// `tests/save_as_widgets.rs` needs for the same reason. The check records that it ran, so one that
/// never happened is a failure rather than a quiet pass.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_rescan(app);
            RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN: AtomicBool = AtomicBool::new(false);
static OK: AtomicBool = AtomicBool::new(false);

fn check_rescan(app: &adw::Application) {
        // The fixture names one source and ships no file for it, so a scan of this window really
        // does have something to drop.
        let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();

        let button = ui::rescan_button(&window).expect("the header bar has a Rescan button");
        assert_eq!(
            ui::rescan_tooltip(&window).as_deref(),
            Some("Rescan inputs and outputs"),
            "§1's wording for badge 8"
        );

        button.emit_clicked();

        assert_eq!(ui::state(&window).status, rescan::RESCANNED);
        // `window_logs` accumulates across checks in this binary, so the line is looked for rather
        // than counted.
        assert!(
            ui::window_logs()
                .iter()
                .any(|line| line == "!!! dropped project:sources/lecture.mkv -- it is no longer there"),
            "S1 names the row it dropped: {:?}",
            ui::window_logs()
        );
        OK.store(true, Ordering::SeqCst);
}

#[test]
fn f0_11_s1_rescanning_the_window_names_what_it_dropped() {
    window_round();
    assert!(RAN.load(Ordering::SeqCst), "the application never ran");
    assert!(OK.load(Ordering::SeqCst), "the check never finished");
}
