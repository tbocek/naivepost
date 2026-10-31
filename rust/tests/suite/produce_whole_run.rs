//! ▶ on the Produce page, through real ffmpeg, down to the finished file.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

fn ffmpeg(args: &[&str], out: &Path) {
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(args)
        .arg(out)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "ffmpeg wrote {}", out.display());
}

fn seconds(file: &Path) -> f64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .expect("ffprobe runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0.0)
}

fn brightest(file: &Path, at: f64) -> u8 {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at}"), "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .expect("ffmpeg runs");
    out.stdout.into_iter().max().unwrap_or(0)
}

fn loudest(file: &Path, from: f64, to: f64) -> f64 {
    let out = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-ss",
            &format!("{from}"),
            "-t",
            &format!("{}", to - from),
            "-i",
        ])
        .arg(file)
        .args(["-map", "0:a", "-af", "volumedetect", "-f", "null", "-"])
        .output()
        .expect("ffmpeg runs");
    String::from_utf8_lossy(&out.stderr)
        .split("max_volume:")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|db| db.parse().ok())
        .unwrap_or(f64::NEG_INFINITY)
}

#[test]
fn play_renders_a_cut_with_effects_across_a_split_to_a_finished_file() {
    common::one_app_round(|app| {
        let root = common::temp_dir_created("whole-run");
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

        let dir = root.join("whole.naivepost");
        std::fs::create_dir_all(dir.join("sources")).unwrap();
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "color=black:s=320x180:r=25:d=30",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=30",
                "-shortest",
            ],
            &dir.join("sources/2026-10-18 19-21-52.mkv"),
        );
        std::fs::write(
            dir.join("naivepost.json"),
            r#"{"sources": [{"path": "project:sources/2026-10-18 19-21-52.mkv", "footage": true}], "produce": {"preset": "ultrafast", "blurred_edges": false}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("cut")).unwrap();
        std::fs::write(
            dir.join("cut/cut.json"),
            r#"{"segs": [{"s": 0.0, "e": 8.0},
                         {"s": 8.0, "e": 8.0, "dur": 3.0, "ins": "project:card.svg"},
                         {"s": 8.0, "e": 16.0}, {"s": 16.0, "e": 20.0},
                         {"s": 20.0, "e": 24.0, "ins": "project:quiet.wav"}],
                "fx": [{"kind": "text", "t": 5.0, "dur": 6.0, "text": "WORDS"},
                       {"kind": "speed", "t": 16.0, "dur": 4.0, "rate": 2.0},
                       {"kind": "volume", "t": 12.0, "dur": 3.0, "gain": 0.05}]}"#,
        )
        .unwrap();
        ffmpeg(
            &["-f", "lavfi", "-i", "anullsrc=r=48000:cl=mono", "-t", "6"],
            &dir.join("quiet.wav"),
        );
        std::fs::write(
            dir.join("card.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><rect width="320" height="180" fill="gray"/></svg>"#,
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
            "the finished file: {} / {:?}",
            common::status_text(&window),
            ui::window_logs().iter().rev().take(14).collect::<Vec<_>>()
        );
        // 8 s, the 3 s card, 8 s, 4 s of footage at 2×, and 4 s under a sound.
        assert!(
            (seconds(&done) - 25.0).abs() < 0.5,
            "{} {:?}",
            seconds(&done),
            ui::window_logs()
                .iter()
                .filter(|l| l.contains("clip ") && !l.contains("ffmpeg"))
                .collect::<Vec<_>>()
        );
        assert!(brightest(&done, 3.0) < 50, "before the box");
        assert!(brightest(&done, 6.0) > 200, "the box, first clip");
        let card = brightest(&done, 9.5);
        assert!(card > 100, "the card plays where it was placed: {card}");
        assert!(
            brightest(&done, 12.0) > 200,
            "the box goes on after the card"
        );
        assert!(brightest(&done, 16.0) < 50, "after the box");
        // The cue sits at session 12–15, which plays at 15–18 after the card.
        assert!(
            loudest(&done, 15.5, 17.5) < loudest(&done, 12.0, 14.0) - 15.0,
            "the duck lands where its seconds play"
        );
        // The footage hums; the file laid over its last four seconds is silent, and the picture goes on.
        assert!(
            loudest(&done, 18.5, 20.5) > -40.0,
            "the footage's own sound"
        );
        assert!(
            loudest(&done, 21.5, 24.5) < -60.0,
            "the laid sound in its place"
        );
        RAN.store(true, Ordering::SeqCst);
        window.close();
        common::settle();
    });
    assert!(RAN.load(Ordering::SeqCst), "the check never ran");
}
