//! §07-narrate's preview transport driven through its real buttons with the real built-in player: the page's ▶/⏸
//! (`narrate-play`), back and forward three seconds, and the run bar's ⏹. Every press is asserted on the page (the
//! ▶/⏸ face, ⏹'s grey) AND on the MEDIA: the playbin really playing, paused or stopped (F4.5 S1/S4/S6: ▶ plays the
//! cut from the playhead, ⏸ holds both players where they stand, ▶ again plays on, a seek moves the picture, ⏹
//! stops both players and hands ▶ back to the step).
//!
//! Needs GStreamer with `playbin` and `gtk4paintablesink` (the dev container installs both).

use std::process::Command;
use std::time::Duration;

use crate::common;

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
