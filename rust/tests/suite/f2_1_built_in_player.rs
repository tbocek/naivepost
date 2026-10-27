//! F2.1 through the built-in player: ▶ plays the recording INSIDE the preview, as the prototype does (a playbin
//! into the GTK4 sink, its paintable on the preview's picture), the red line follows the player (S4), ▶ again
//! pauses it on its frame, and at the recording's end, with nothing to walk on to, the preview stops there (S3).
//! The first port played through a `gst-play-1.0` subprocess that opened a window of its own while the preview
//! stood on one frame.
//!
//! Needs GStreamer with `playbin` and `gtk4paintablesink` (the dev container installs both); a box without them
//! has no built-in player, and the check says so rather than passing.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use adw::prelude::*;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

/// One stamped recording, 4 s long.
const A: &str = "2026-10-18 19-21-52";

fn project(root: &Path) -> PathBuf {
    let dir = root.join("play.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=25:duration=4",
        ])
        .arg(dir.join("sources").join(format!("{A}.mkv")))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.5, "e": 1.5}, {"s": 2.5, "e": 3.5}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("cut/line.json"), r#"{"t": 1.0}"#).unwrap();
    dir
}

/// Pump the loop until `done` or the limit; answers whether it came true.
fn wait_for(mut done: impl FnMut() -> bool, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if done() {
            return true;
        }
        common::settle();
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

fn line(window: &adw::ApplicationWindow) -> f64 {
    ui::line_position(window).t
}

#[test]
fn f2_1_s5_play_runs_the_recording_in_the_preview_and_the_line_follows_it() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir("built-in-player");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.present();
        common::settle();
        assert!(
            naivepost::play_built_in::in_use(),
            "this box has GStreamer with playbin and gtk4paintablesink, so \u{25b6} plays in the preview"
        );
        assert_eq!(line(&window), 1.0, "the line was restored at 1 s");

        let play = ui::play_recording_button(&window).expect("\u{25b6} is on the page");
        play.emit_clicked();
        assert!(
            wait_for(|| line(&window) > 1.6, Duration::from_secs(5)),
            "the line follows the playing recording: still at {}",
            line(&window)
        );
        assert!(naivepost::play_back::running(), "the built-in player runs");
        assert!(
            ui::cut_picture::video_shown(),
            "and the preview's picture is the player's"
        );
        assert_eq!(
            naivepost::play_back::playing_pid(),
            None,
            "no player process: nothing opens a window of its own"
        );

        // ▶ again pauses on the frame: the line stands still.
        play.emit_clicked();
        common::settle();
        assert!(!naivepost::play_back::running(), "\u{25b6} again paused it");
        let held = line(&window);
        wait_for(|| false, Duration::from_millis(400));
        assert!(
            (line(&window) - held).abs() < 0.05,
            "a paused preview's line stands still: {held} -> {}",
            line(&window)
        );
        assert!(
            ui::cut_picture::video_shown(),
            "and the picture stays on the player's frame"
        );

        // F2.2 through the same player: ▶✂ plays the CUT, so the line jumps the dropped 1.5..2.5 s and the
        // picture goes with it (the line's jump is followed by a seek), the player standing where the line does.
        ui::cut_picture::note_line_moved(&window, 0.6);
        let play_cut = ui::play_cut_button(&window).expect("\u{25b6}\u{2702} is on the page");
        // F2.2 S2: one press switches a paused preview to the cut AND plays it (the prototype's `playAs`); the
        // first rewrite only switched, and ▶✂ had to be pressed twice.
        play_cut.emit_clicked();
        let mut in_gap = Vec::new();
        let (mut at_join, mut past_join) = (None, None);
        let crossed = wait_for(
            || {
                let at = line(&window);
                if at > 1.6 && at < 2.4 {
                    in_gap.push(at);
                }
                if at >= 1.4 && at_join.is_none() {
                    at_join = Some(Instant::now());
                }
                if at >= 2.7 && past_join.is_none() {
                    past_join = Some(Instant::now());
                }
                at > 2.8
            },
            Duration::from_secs(6),
        );
        assert!(
            crossed,
            "\u{25b6}\u{2702} played on past the join: line {}",
            line(&window)
        );
        assert!(
            in_gap.is_empty(),
            "the line never stood in the dropped stretch: {in_gap:?}"
        );
        // 1.4 s to 2.7 s is 0.3 s of the CUT: the player seeks over the dropped second rather than playing it
        // while the line waits at the join.
        let join = past_join.expect("past the join") - at_join.expect("at the join");
        assert!(
            join < Duration::from_millis(800),
            "the join took {join:?}: the dropped second was played, not skipped"
        );
        let player = ui::cut_record_play::video_second().expect("the player says where it is");
        assert!(
            (player - line(&window)).abs() < 0.35,
            "the player stands where the line does: player {player}, line {}",
            line(&window)
        );
        play_cut.emit_clicked();
        common::settle();
        assert!(
            !naivepost::play_back::running(),
            "\u{25b6}\u{2702} again paused it"
        );
        // F2.2 S4: a paused cut holds its line, and with it the picture: the ▶✂ tick moves nothing.
        let held = line(&window);
        wait_for(|| false, Duration::from_millis(400));
        assert!(
            (line(&window) - held).abs() < 0.05,
            "a paused \u{25b6}\u{2702} holds its line: {held} -> {}",
            line(&window)
        );
        ui::play_recording_button(&window)
            .expect("\u{25b6}")
            .emit_clicked();
        common::settle();
        if naivepost::play_back::running() {
            // Pressing ▶ under ✂ switches the preview back to the recording (F2.1 S1) and may carry on playing;
            // pause it for the last check.
            play.emit_clicked();
            common::settle();
        }

        // From near the end: the recording runs out, nothing follows it, and the preview stops there.
        ui::cut_picture::note_line_moved(&window, 3.6);
        play.emit_clicked();
        assert!(
            wait_for(
                || !ui::preview_player(&window).transport.playing,
                Duration::from_secs(5)
            ),
            "at the recording's end the preview pauses (line {})",
            line(&window)
        );
        assert!(
            line(&window) >= 3.9,
            "the line reached the end: {}",
            line(&window)
        );
        window.close();
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(
        RAN.load(Ordering::SeqCst),
        "the built-in player round never ran"
    );
}
