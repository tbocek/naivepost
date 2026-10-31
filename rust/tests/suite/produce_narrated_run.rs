//! ▶ on the Produce page with spoken lines: real takes, real ffmpeg, measured in the finished file.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::narration::{self, Entry, Narration};
use naivepost::produce_stamp::wav_of;
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

fn tone(hz: u32, seconds: f64, out: &Path) {
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={hz}:duration={seconds}:sample_rate=24000"),
        ],
        out,
    );
}

fn seconds(file: &Path) -> f64 {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration"])
        .args(["-of", "csv=p=0"])
        .arg(file)
        .output()
        .expect("ffprobe runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0.0)
}

/// The peak of one pitch over a span, in dB.
fn heard(file: &Path, hz: u32, from: f64, to: f64) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-ss", &format!("{from}")])
        .args(["-t", &format!("{}", to - from), "-i"])
        .arg(file)
        .args(["-map", "0:a", "-af"])
        .arg(format!(
            "bandpass=f={hz}:width_type=h:w=60,bandpass=f={hz}:width_type=h:w=60,volumedetect"
        ))
        .args(["-f", "null", "-"])
        .output()
        .expect("ffmpeg runs");
    String::from_utf8_lossy(&out.stderr)
        .split("max_volume:")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|db| db.parse().ok())
        .unwrap_or(f64::NEG_INFINITY)
}

fn line(s: f64, e: f64, at: f64, text: &str) -> Entry {
    Entry {
        s,
        e,
        at,
        text: text.into(),
        ..Default::default()
    }
}

#[test]
fn takes_are_heard_where_they_were_put_and_a_long_one_is_heard_out() {
    common::one_app_round(|app| {
        let root = common::temp_dir_created("narrated-run");
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

        let dir = root.join("narrated.naivepost");
        std::fs::create_dir_all(dir.join("sources")).unwrap();
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "color=black:s=320x180:r=25:d=40",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=mono",
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
            r#"{"segs": [{"s": 0.0, "e": 10.0}, {"s": 10.0, "e": 16.0}, {"s": 25.0, "e": 31.0}]}"#,
        )
        .unwrap();
        let tree = naivepost::layout::Tree::new(&dir).unwrap();
        let mut chosen = line(0.0, 10.0, 2.0, "the chosen take");
        chosen.roll = 1;
        let long = line(10.0, 16.0, 1.0, "runs past its clip");
        let mut caption = line(
            25.0,
            31.0,
            1.0,
            "READ THIS FIRST, IT IS LONG ENOUGH TO TAKE A WHILE",
        );
        caption.pos = "top".to_string();
        tone(1000, 3.0, &wav_of(&tree, &chosen, None, 1).unwrap());
        tone(3000, 3.0, &wav_of(&tree, &chosen, None, 0).unwrap());
        tone(2000, 9.0, &wav_of(&tree, &long, None, 0).unwrap());
        let record = Narration {
            entries: vec![chosen, long, caption],
            ..Default::default()
        };
        narration::save(&record, &tree).unwrap();

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
        // 10 s, then the second clip's 6 s grown to 1 + 9 to hear its line out, then 6 s.
        assert!((seconds(&done) - 26.0).abs() < 0.5, "{}", seconds(&done));
        const QUIET: f64 = -40.0;
        assert!(
            heard(&done, 1000, 0.2, 1.8) < QUIET,
            "not before its second"
        );
        assert!(
            heard(&done, 1000, 2.2, 4.8) > QUIET,
            "the line where it was put"
        );
        assert!(
            heard(&done, 1000, 5.3, 9.8) < QUIET,
            "and over when it ends"
        );
        assert!(
            heard(&done, 3000, 0.2, 9.8) < QUIET,
            "not the take the roll left"
        );
        assert!(
            heard(&done, 2000, 5.3, 9.8) < QUIET,
            "the next line waits for its clip"
        );
        assert!(heard(&done, 2000, 11.2, 13.0) > QUIET, "and starts in it");
        assert!(
            heard(&done, 2000, 18.0, 19.8) > QUIET,
            "and is heard to its end"
        );
        assert!(
            heard(&done, 2000, 20.3, 22.0) < QUIET,
            "the next clip starts after it"
        );
        let sheet = std::fs::read_to_string(dir.join("produce/final.srt")).expect("the subtitles");
        for (words, at) in [
            ("the chosen take", "00:00:02,000"),
            // Growth stops at its ceiling, so the rest of the room comes from starting 0.2 s early.
            ("runs past its clip", "00:00:10,"),
            ("READ THIS FIRST", "00:00:21,000"),
        ] {
            let lines: Vec<&str> = sheet.lines().collect();
            let row = lines.iter().position(|text| text.contains(words)).unwrap();
            assert!(lines[row - 1].starts_with(at), "{words} at {at}: {sheet}");
        }
        assert!(
            !sheet.contains("00:00:21,000 --> 00:00:21,"),
            "a caption stays up to be read: {sheet}"
        );
        RAN.store(true, Ordering::SeqCst);
        window.close();
        common::settle();
    });
    assert!(RAN.load(Ordering::SeqCst), "the check never ran");
}
