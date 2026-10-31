//! A pasted stretch through ▶ and real ffmpeg: it shows the seconds it was copied from.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

fn brightest(file: &Path, at: f64) -> u8 {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at}"), "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .expect("ffmpeg runs");
    out.stdout.into_iter().max().unwrap_or(0)
}

#[test]
fn a_paste_plays_what_was_copied_not_what_lies_at_the_paste_point() {
    common::one_app_round(|app| {
        let root = common::temp_dir_created("pasted-run");
        std::env::set_current_dir(&root).expect("cwd");
        let config = root.join("config");
        std::fs::create_dir_all(&config).unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &config);
            std::env::set_var("XDG_DATA_HOME", root.join("data"));
        }
        let paths = naivepost::settings::from_environment().expect("settings");
        let mut conf = naivepost::settings::Conf::default();
        conf.model = "nobody".to_string();
        conf.server = "http://127.0.0.1:1".to_string();
        conf.audio_server = "http://127.0.0.1:1".to_string();
        conf.sd_server = "http://127.0.0.1:1".to_string();
        naivepost::settings::save(&paths, &conf).expect("settings written");

        let dir = root.join("pasted.naivepost");
        std::fs::create_dir_all(dir.join("sources")).unwrap();
        // Black, then white from second 30.
        let made = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("color=black:s=320x180:r=25:d=40")
            .args([
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=mono",
                "-shortest",
                "-vf",
            ])
            .arg("drawbox=c=white:t=fill:enable='gte(t,30)'")
            .arg(dir.join("sources/2026-10-18 19-21-52.mkv"))
            .status()
            .expect("ffmpeg runs");
        assert!(made.success());
        std::fs::write(
            dir.join("naivepost.json"),
            r#"{"sources": [{"path": "project:sources/2026-10-18 19-21-52.mkv", "footage": true}], "produce": {"preset": "ultrafast", "blurred_edges": false}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("cut")).unwrap();
        std::fs::write(
            dir.join("cut/cut.json"),
            r#"{"segs": [{"s": 0.0, "e": 5.0},
                         {"s": 5.0, "e": 5.0, "dur": 3.0, "ins": "copy:32.000"},
                         {"s": 5.0, "e": 10.0}]}"#,
        )
        .unwrap();
        let model = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &model, "Produce", &dir);
        window.present();
        common::settle();
        ui::produce_page::press_produce_run(&window);
        assert!(
            ui::background::settle_for_test(Duration::from_secs(240)),
            "the run ended"
        );
        common::settle();
        let done = dir.join("produce/final.mp4");
        assert!(
            done.is_file(),
            "{:?}",
            ui::window_logs().iter().rev().take(14).collect::<Vec<_>>()
        );
        assert!(brightest(&done, 2.0) < 50, "before the paste");
        assert!(brightest(&done, 6.5) > 200, "the copied seconds");
        assert!(brightest(&done, 10.0) < 50, "after the paste");
        RAN.store(true, Ordering::SeqCst);
        window.close();
        common::settle();
    });
    assert!(RAN.load(Ordering::SeqCst), "the check never ran");
}
