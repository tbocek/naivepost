//! §03-shell.md F0.8 New project — through the widgets. The header bar's ＋ New decides nothing: it
//! draws [`naivepost::new_project`]'s strings and forwards its press, so what these checks assert is
//! that a click asked rather than emptied (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use adw::prelude::*;
use naivepost::new_project;
use naivepost::ui;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// One running GTK application for this test binary, taken in turn by the checks below — the same
/// single-main-loop arrangement tests/smoke.rs needs for the same reason.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_button_and_dialog(app);
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn f0_8_s3_the_new_button_asks_before_it_empties_anything() {
    window_round();
    assert!(RAN.load(std::sync::atomic::Ordering::SeqCst), "the ＋ New check never ran");
}

/// The button is there with §1's tooltip; pressing it on a session that has something in it changes
/// nothing on screen — no refusal, no emptied project — because S3 asks first and the answer comes
/// later. And the dialog it shows is drawn from `new_project`'s strings rather than its own.
fn check_button_and_dialog(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let new_ = ui::new_button(&window).expect("the header bar has a New button");
    assert_eq!(
        ui::new_tooltip(&window).as_deref(),
        Some("New project \u{2014} name it, put it where you want it, and start over"),
        "badge 1 of §1's header bar"
    );

    new_.emit_clicked();
    // S3 is a question, so the window says nothing yet: no refusal (S1) and nothing emptied. The
    // status line is the only place this flow can speak, and it stays quiet until an answer.
    assert_eq!(ui::state(&window).status, "", "asking must not report anything");

    // The dialog itself, built from the module's strings: the question as its heading, and for a
    // session that never had a name, the paragraph that says so.
    let detail = new_project::confirm_detail(&naivepost::startup::session_dir(Path::new("/tmp/x")));
    let dialog = ui::new_project_confirm(None, &detail);
    assert_eq!(dialog.heading().as_deref(), Some(new_project::QUESTION));
    assert!(
        dialog.body().as_str().contains(
            "This session has never been saved under a name of its own, so there is nothing to come back to."
        ),
        "{}",
        dialog.body()
    );
    assert_eq!(
        dialog.response_appearance("start"),
        adw::ResponseAppearance::Destructive,
        "\"Start new…\" is the red button, as the spec's image draws it"
    );
    dialog.destroy();
}
