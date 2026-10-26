//! §03-shell §1 badge 15, the Outputs folder button, through its REAL opener: no seam, no forced sensitivity.
//! A project opened the way the program opens one, the readouts left to land, then a click. On a machine with no
//! file manager (this container) the press must still say what happened: the `xdg-open` fallback's own line, or
//! "could not open". The first rewrite handed a `file://` folder to GTK's URI launcher and ignored its answer, so
//! a press the desktop refused did nothing at all and said nothing.

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
        let before = ui::window_logs().len();
        folder.emit_clicked();
        // The launch answers late (a portal, a spawn): turn the loop until the press has said something.
        let said = |logs: &[String]| {
            logs.iter()
                .any(|line| line.contains("xdg-open") || line.contains("could not open"))
        };
        let until = Instant::now() + Duration::from_secs(10);
        let context = glib::MainContext::default();
        while Instant::now() < until && !said(&ui::window_logs()[before..]) {
            context.iteration(false);
            std::thread::sleep(Duration::from_millis(10));
        }
        let logs = ui::window_logs();
        assert!(
            said(&logs[before..]),
            "a press with no file manager falls back or says why, never nothing: {:?}",
            &logs[before..]
        );
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
