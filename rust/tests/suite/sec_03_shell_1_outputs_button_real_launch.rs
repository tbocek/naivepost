//! §03-shell §1 badge 15, the Outputs folder button: no seam, no forced sensitivity. A project opened the way the
//! program opens one, the readouts left to land, then a click that reaches the opener. A test binary opens nothing
//! on the real desktop (`log_link::open` counts the press instead), so reaching the opener is what is checked.

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
        // The press answers late, off the readouts: turn the loop until it has reached the opener.
        let until = Instant::now() + Duration::from_secs(10);
        let context = glib::MainContext::default();
        while Instant::now() < until && ui::log_link::blocked_opens_for_test() == before {
            context.iteration(false);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            ui::log_link::blocked_opens_for_test(),
            before + 1,
            "the press reached the opener, once"
        );
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
