//! §05-cut's transport row driven through its real buttons with the real built-in player: ▶, ▶✂, ▶✂✂, the four
//! frame steps and the run bar's ⏹. Every press is asserted twice over: on the page (faces, which one is lit, the
//! review) and on the MEDIA (`play_back::running`, the playbin really playing or not). The first rewrite's ▶✂✂
//! cleared the page's state on its second press and left the video playing behind a ▶ face.
//!
//! The rules (F2.1-F2.4): "each wears ⏸ only while its own thing runs; pressing it pauses; pressing another
//! switches the preview without stopping; exactly one is lit"; a frame step steps the recording under the line,
//! then pauses; ⏹ stops whatever plays (F0.3 S1).
//!
//! Needs GStreamer with `playbin` and `gtk4paintablesink` (the dev container installs both).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use adw::prelude::*;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

/// One stamped recording, 24 s at 25 fps.
const A: &str = "2026-10-18 19-21-52";

fn project(root: &Path) -> PathBuf {
    let dir = root.join("transport.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=25:duration=24",
        ])
        .arg(dir.join("sources").join(format!("{A}.mkv")))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    // Three clips, two joins (at 6 s and 13 s of the recording): enough for a review.
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 1.0, "e": 6.0}, {"s": 8.0, "e": 13.0}, {"s": 15.0, "e": 20.0}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("cut/line.json"), r#"{"t": 2.0}"#).unwrap();
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

/// The media plays, waited for: a cue is finished by a poll, so "playing" lands a little after the press.
fn plays() -> bool {
    wait_for(naivepost::play_back::running, Duration::from_secs(5))
}

/// The media stays stopped: not playing now, and still not playing half a second later (a pending cue or a
/// tick that restarts it would show here).
fn stays_stopped() -> bool {
    wait_for(|| !naivepost::play_back::running(), Duration::from_secs(3))
        && !wait_for(naivepost::play_back::running, Duration::from_millis(600))
}

fn line(window: &adw::ApplicationWindow) -> f64 {
    ui::line_position(window).t
}

fn face(button: &gtk4::Button) -> String {
    button
        .label()
        .map(|label| label.to_string())
        .unwrap_or_default()
}

/// Which of ▶ / ▶✂ / ▶✂✂ wears the accent.
fn lit(buttons: &[&gtk4::Button; 3]) -> Vec<bool> {
    buttons
        .iter()
        .map(|button| button.has_css_class("suggested-action"))
        .collect()
}

#[test]
fn sec_05_cut_2_every_transport_button_moves_the_media_as_its_face_says() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir("transport-real-player");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.present();
        common::settle();
        common::settle_workers();
        assert!(
            naivepost::play_built_in::in_use(),
            "this box has GStreamer with playbin and gtk4paintablesink"
        );
        let rec = ui::play_recording_button(&window).expect("\u{25b6}");
        let cut = ui::play_cut_button(&window).expect("\u{25b6}\u{2702}");
        let review = ui::review_cuts_button(&window).expect("\u{25b6}\u{2702}\u{2702}");
        let stop = ui::stop_button(&window).expect("\u{23f9}");
        let three = [&rec, &cut, &review];
        let rec_pause = ui::cut_record_play::PAUSE_LABEL;
        let cut_pause = ui::cut_cut_play::PAUSE_LABEL;
        let review_pause = ui::cut_review_play::PAUSE_LABEL;

        // F2.1: ▶ plays, ▶ again pauses, the media with it.
        rec.emit_clicked();
        assert!(plays(), "\u{25b6} plays the recording");
        assert_eq!(
            face(&rec),
            rec_pause,
            "\u{25b6} wears \u{23f8} while it runs"
        );
        rec.emit_clicked();
        assert!(stays_stopped(), "\u{25b6} again pauses the media");
        assert_ne!(face(&rec), rec_pause, "and gives back its \u{25b6} face");

        // F2.3 S5: ▶✂✂ plays the review; ▶✂✂ again pauses it AND the media, and ends the review.
        review.emit_clicked();
        assert!(plays(), "\u{25b6}\u{2702}\u{2702} plays the review");
        assert_eq!(
            face(&review),
            review_pause,
            "\u{25b6}\u{2702}\u{2702} wears \u{23f8}"
        );
        assert_eq!(
            lit(&three),
            [false, false, true],
            "the review is the one lit"
        );
        review.emit_clicked();
        assert!(
            stays_stopped(),
            "\u{25b6}\u{2702}\u{2702} again pauses the media, not only the page"
        );
        assert_ne!(
            face(&review),
            review_pause,
            "the review's face goes back to \u{25b6}"
        );
        assert!(
            !ui::preview_player(&window).reviewing,
            "and the review is over"
        );

        // F2.3 S5: ▶✂ during a review switches to the plain cut WITHOUT stopping.
        review.emit_clicked();
        assert!(plays(), "the review runs again");
        cut.emit_clicked();
        assert!(
            plays(),
            "\u{25b6}\u{2702} during a review carries on playing"
        );
        assert!(!wait_for(
            || !naivepost::play_back::running(),
            Duration::from_millis(600)
        ));
        assert!(!ui::preview_player(&window).reviewing, "the review ended");
        assert_eq!(face(&cut), cut_pause, "\u{25b6}\u{2702} wears \u{23f8} now");
        assert_ne!(
            face(&review),
            review_pause,
            "\u{25b6}\u{2702}\u{2702} does not"
        );
        assert_eq!(lit(&three), [false, true, false], "the cut is the one lit");

        // F2.1 S1: ▶ during the cut switches to the recording, carrying on.
        rec.emit_clicked();
        assert!(plays(), "\u{25b6} during the cut carries on playing");
        assert!(!wait_for(
            || !naivepost::play_back::running(),
            Duration::from_millis(600)
        ));
        assert_eq!(face(&rec), rec_pause, "\u{25b6} wears \u{23f8}");
        assert_ne!(face(&cut), cut_pause, "\u{25b6}\u{2702} does not");
        assert_eq!(
            lit(&three),
            [true, false, false],
            "the recording is the one lit"
        );

        // F0.3 S1: ⏹ stops what plays, whichever preview it is.
        assert!(
            stop.is_sensitive(),
            "\u{23f9} is live while a preview plays"
        );
        stop.emit_clicked();
        assert!(stays_stopped(), "\u{23f9} stops the recording's media");
        assert_ne!(face(&rec), rec_pause, "\u{25b6} gives back its face");

        cut.emit_clicked(); // paused recording: switch only (F2.2 S2)
        common::settle();
        if !naivepost::play_back::running() {
            cut.emit_clicked();
        }
        assert!(plays(), "\u{25b6}\u{2702} plays the cut");
        stop.emit_clicked();
        assert!(stays_stopped(), "\u{23f9} stops the cut's media");
        assert_ne!(
            face(&cut),
            cut_pause,
            "\u{25b6}\u{2702} gives back its face"
        );

        review.emit_clicked();
        assert!(plays(), "\u{25b6}\u{2702}\u{2702} plays");
        stop.emit_clicked();
        assert!(stays_stopped(), "\u{23f9} stops the review's media");
        assert_ne!(
            face(&review),
            review_pause,
            "\u{25b6}\u{2702}\u{2702} gives back its face"
        );

        // F2.4 S2: a frame step steps the line, then pauses: the media and every face.
        let one = ui::line_step_button(&window, "line-step-forward").expect("f\u{203a}");
        let back = ui::line_step_button(&window, "line-step-back").expect("\u{2039}f");
        let five =
            ui::line_step_button(&window, "line-step-forward-five").expect("f\u{203a}\u{203a}");
        let back5 =
            ui::line_step_button(&window, "line-step-back-five").expect("\u{2039}\u{2039}f");
        rec.emit_clicked();
        if !naivepost::play_back::running() {
            rec.emit_clicked();
        }
        assert!(plays(), "\u{25b6} plays before the step");
        one.emit_clicked();
        assert!(stays_stopped(), "f\u{203a} pauses the media");
        assert_ne!(face(&rec), rec_pause, "and \u{25b6}'s face");
        let at = line(&window);
        one.emit_clicked();
        assert!(
            (line(&window) - (at + 0.04)).abs() < 0.011,
            "f\u{203a} is one frame: {at} -> {}",
            line(&window)
        );
        back.emit_clicked();
        assert!(
            (line(&window) - at).abs() < 0.011,
            "\u{2039}f is one back: {}",
            line(&window)
        );
        five.emit_clicked();
        assert!(
            (line(&window) - (at + 0.2)).abs() < 0.011,
            "f\u{203a}\u{203a} is five: {}",
            line(&window)
        );
        back5.emit_clicked();
        assert!(
            (line(&window) - at).abs() < 0.011,
            "\u{2039}\u{2039}f is five back: {}",
            line(&window)
        );
        assert!(stays_stopped(), "and stepping a paused line plays nothing");

        review.emit_clicked();
        assert!(plays(), "the review plays before the step");
        one.emit_clicked();
        assert!(stays_stopped(), "a step during the review pauses the media");
        assert_ne!(
            face(&review),
            review_pause,
            "and \u{25b6}\u{2702}\u{2702}'s face"
        );

        assert!(
            !ui::preview_player(&window).reviewing,
            "and a review paused by a step is over, as one \u{25b6}\u{2702}\u{2702} pauses is"
        );
        window.close();
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the transport round never ran");
}
