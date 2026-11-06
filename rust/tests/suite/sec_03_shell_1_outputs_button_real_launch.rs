//! §03-shell §1 badge 15, the Outputs folder button: no seam, no forced sensitivity. A project opened the way the
//! program opens one, the readouts left to land, then a click that puts up the in-app file dialog in the shown page's
//! output folder, the same dialog Open uses (first rewrite: the folder went to the desktop's launcher, and in the dev
//! container that did nothing). Nothing is handed to the desktop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::common;
use adw::prelude::*;
use naivepost::ui;

#[test]
fn sec_03_shell_1_the_outputs_button_opens_or_says_why() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = common::temp_dir_created("outputs-real");
        let session = root.join("outputs.naivepost");
        std::fs::create_dir_all(session.join("prepare")).expect("a project with a Prepare folder");
        let model = naivepost::project::load(&common::fixture_dir()).expect("the fixture loads");
        naivepost::project::save(&model, &session).expect("the project file");
        naivepost::startup::set_open_dir(&session);
        let window = ui::window::build_window_at(app, &model, "Prepare", &session);
        window.present();
        common::settle_workers();
        let folder = ui::outputs_folder_button(&window).expect("badge 15: the folder button");
        assert!(
            folder.is_sensitive(),
            "with a project open the button is live, without a test forcing it"
        );
        let before = ui::log_link::blocked_opens_for_test();
        folder.emit_clicked();
        // The press answers late, off the readouts: turn the loop until the dialog is up.
        let until = Instant::now() + Duration::from_secs(10);
        let context = glib::MainContext::default();
        while Instant::now() < until && ui::outputs_dialog_for_test().is_none() {
            context.iteration(false);
            std::thread::sleep(Duration::from_millis(10));
        }
        let shown = ui::outputs_dialog_for_test().expect("the press put up the file dialog");
        assert_eq!(
            shown.initial_folder().and_then(|f| f.path()),
            Some(session.join("prepare")),
            "in the Prepare page's output folder"
        );
        assert_eq!(ui::log_link::blocked_opens_for_test(), before, "and handed nothing to the desktop");
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
