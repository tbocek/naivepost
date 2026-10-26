//! §07-narrate's preview transport driven through its real buttons with the real built-in player: the page's ▶/⏸
//! (`narrate-play`), back and forward three seconds, and the run bar's ⏹. Every press is asserted on the page (the
//! ▶/⏸ face, ⏹'s grey) AND on the MEDIA: the playbin really playing, paused or stopped (F4.5 S1/S4/S6: ▶ plays the
//! cut from the playhead, ⏸ holds both players where they stand, ▶ again plays on, a seek moves the picture, ⏹
//! stops both players and hands ▶ back to the step).
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

const A: &str = "2026-10-18 19-21-52";

fn project(root: &Path) -> PathBuf {
    let dir = root.join("narrate-transport.naivepost");
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
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=24",
            "-shortest",
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
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 1.0, "e": 10.0}, {"s": 12.0, "e": 22.0}]}"#,
    )
    .unwrap();
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

fn media_plays() -> bool {
    naivepost::play_built_in::playing()
}

fn plays() -> bool {
    wait_for(media_plays, Duration::from_secs(5))
}

/// Not playing now, and still not half a second later (a pending cue or a tick restarting it shows here).
fn stays_stopped() -> bool {
    wait_for(|| !media_plays(), Duration::from_secs(3))
        && !wait_for(media_plays, Duration::from_millis(600))
}

fn face(button: &gtk4::Button) -> String {
    button
        .label()
        .map(|label| label.to_string())
        .unwrap_or_default()
}

#[test]
fn sec_07_narrate_5_the_preview_transport_moves_the_media_as_its_face_says() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    std::env::remove_var("NAIVEPOST_PICTURE_PLAYER");
    let root = common::temp_dir("narrate-transport-real-player");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.present();
        common::settle();
        common::settle_workers();
        assert!(
            naivepost::play_built_in::available(),
            "this box has GStreamer with playbin and gtk4paintablesink"
        );
        let play = common::narrate_button(&window, "narrate-play");
        let back = common::narrate_button(&window, "narrate-back-3s");
        let forward = common::narrate_button(&window, "narrate-forward-3s");
        let stop = ui::stop_button(&window).expect("\u{23f9}");

        // S1: ▶ plays the cut, the media with it, and ⏹ is live.
        play.emit_clicked();
        common::settle_workers();
        assert!(plays(), "\u{25b6} plays the preview's picture");
        assert_eq!(
            face(&play),
            "\u{23f8}",
            "\u{25b6} wears \u{23f8} while it plays"
        );
        assert!(
            stop.is_sensitive(),
            "\u{23f9} is live while the preview plays"
        );

        // S4: a seek while playing moves the picture and plays on.
        forward.emit_clicked();
        common::settle_workers();
        assert!(plays(), "forward three seconds plays on");
        assert!(
            !wait_for(|| !media_plays(), Duration::from_millis(600)),
            "and keeps playing"
        );
        back.emit_clicked();
        common::settle_workers();
        assert!(plays(), "back three seconds plays on");

        // S6: ⏸ holds the media where it stands; ▶ again plays on.
        play.emit_clicked();
        common::settle_workers();
        assert!(stays_stopped(), "\u{23f8} holds the media");
        assert_eq!(face(&play), "\u{25b6}", "and gives back the \u{25b6} face");
        assert!(stop.is_sensitive(), "a held preview still owes \u{23f9}");
        play.emit_clicked();
        common::settle_workers();
        assert!(plays(), "\u{25b6} again plays on");
        assert_eq!(face(&play), "\u{23f8}", "wearing \u{23f8}");

        // S6: ⏹ stops both players and hands ▶ back to the step.
        stop.emit_clicked();
        common::settle_workers();
        assert!(stays_stopped(), "\u{23f9} stops the media");
        assert_eq!(face(&play), "\u{25b6}", "the page's \u{25b6} face is back");
        assert!(
            !ui::narrate_page::narrate_preview_playing(),
            "and the page says nothing plays"
        );

        // A seek while stopped cues the still and plays nothing.
        forward.emit_clicked();
        common::settle_workers();
        assert!(stays_stopped(), "a seek on a stopped preview plays nothing");

        // §1 **8**: the take band's ▶ plays the voice's recording from the red bar (no take picked: the
        // recording plays on), wearing ⏹; a second press stops the sound. On the default player
        // (`gst-play-1.0`) the first rewrite's ffplay flags were refused and nothing ever sounded.
        let take_play = common::narrate_button(&window, "take-play");
        take_play.emit_clicked();
        common::settle_workers();
        assert!(
            wait_for(
                naivepost::narrate_sample::is_playing,
                Duration::from_secs(3)
            ) && !wait_for(
                || !naivepost::narrate_sample::is_playing(),
                Duration::from_millis(800)
            ),
            "the take band's \u{25b6} sounds the recording, and keeps sounding: {:?}",
            ui::window::window_logs().last()
        );
        assert_eq!(
            face(&take_play),
            "\u{23f9}",
            "wearing \u{23f9} while it walks"
        );
        take_play.emit_clicked();
        common::settle_workers();
        assert!(
            wait_for(
                || !naivepost::narrate_sample::is_playing(),
                Duration::from_secs(3)
            ),
            "a second press stops the sound"
        );
        assert_eq!(face(&take_play), "\u{25b6}", "and gives back \u{25b6}");

        window.close();
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the transport round never ran");
}

/// Every voice the Narrate page plays goes through `audio_argv`, in the language of the player asked: the default
/// `gst-play-1.0` really plays the file from the second asked (it is still sounding a moment later rather than
/// gone with "Unknown option"), and ffplay is handed its own flags.
#[test]
fn sec_07_narrate_5_the_voice_player_is_spoken_to_in_its_own_flags() {
    let root = common::temp_dir_created("voice-player-flags");
    let wav = root.join("tone.wav");
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=4",
        ])
        .arg(&wav)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the tone was written");
    let gst = naivepost::narrate_preview_leg::audio_argv(
        "gst-play-1.0",
        &wav.to_string_lossy(),
        1.0,
        Some(3.0),
        0.5,
    );
    let mut child = Command::new("gst-play-1.0")
        .args(&gst)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("gst-play-1.0 starts");
    std::thread::sleep(Duration::from_millis(800));
    let still = child.try_wait().expect("try_wait").is_none();
    let _ = child.kill();
    let out = child.wait_with_output().expect("reaped");
    assert!(
        still,
        "gst-play-1.0 plays the file with {gst:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ff = naivepost::narrate_preview_leg::audio_argv(
        "/usr/bin/ffplay",
        "tone.wav",
        1.0,
        Some(3.0),
        0.5,
    );
    assert_eq!(
        ff,
        [
            "-nodisp",
            "-autoexit",
            "-loglevel",
            "quiet",
            "-volume",
            "50",
            "-ss",
            "1.000",
            "-t",
            "2.000",
            "-i",
            "tone.wav"
        ],
        "ffplay gets ffplay's flags"
    );
}
