//! §05-cut §1, the session's rows: each footage file's length is measured on a worker, and only a file ffprobe
//! cannot measure is reported, once. The first worker version logged "no row -- ffprobe gave no length" for every
//! row still being measured, so every open of a good project listed each recording as a failure.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use naivepost::project::{Project, Source};
use naivepost::ui;

/// A two-second test video written with ffmpeg.
fn video(at: &std::path::Path) {
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=10:duration=2",
        ])
        .arg(at)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test video was written");
}

#[test]
fn sec_05_cut_1_screen_a_length_being_measured_is_not_a_failure() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = common::temp_dir_created("lengths-said");
        let session = root.join("lengths.naivepost");
        std::fs::create_dir_all(session.join("sources")).expect("sources folder");
        video(&session.join("sources").join("2026-10-25 10-28-46.mkv"));
        std::fs::write(
            session.join("sources").join("2026-10-25 10-30-00.mkv"),
            b"not a video",
        )
        .expect("a broken file");
        let project = Project {
            sources: ["2026-10-25 10-28-46.mkv", "2026-10-25 10-30-00.mkv"]
                .iter()
                .map(|name| Source {
                    path: format!("project:sources/{name}"),
                    footage: true,
                    ..Source::default()
                })
                .collect(),
            ..Project::default()
        };
        naivepost::project::save(&project, &session).expect("the project file");
        naivepost::startup::set_open_dir(&session);
        let before = ui::window_logs().len();
        let window = ui::window::build_window_at(app, &project, "Cut", &session);
        window.present();
        common::settle_workers();
        // A repaint asks the lengths again: a known-bad file is not reported twice.
        let _ = naivepost::ui::cut_session::for_open_session(&naivepost::cut::Cut::default());
        common::settle_workers();
        let said: Vec<String> = ui::window_logs()[before..]
            .iter()
            .filter(|line| line.contains("ffprobe gave no length"))
            .cloned()
            .collect();
        assert_eq!(
            said,
            ["2026-10-25 10-30-00: no row \u{2014} ffprobe gave no length"],
            "only the file ffprobe cannot measure is reported, and once"
        );
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
