//! F2.1's media leg: the process that actually plays the recording.
//!
//! Naivepost draws no frames of its own (§0 "the app does the arithmetic"): ▶ hands the session's footage
//! files to a real player and stops it again. This module is that hand — a subprocess, not a library, so
//! nothing new is linked and the pipeline is the box's own GStreamer.
//!
//! # Which program
//!
//! The spec names no player program (F2.1 says only what the press means), so this decides: `gst-play-1.0`,
//! the one media player the container ships (`mpv` and `ffplay` are absent; ffmpeg is here but cuts, it does
//! not play a timeline). The seam is the environment variable `NAIVEPOST_PLAYER`, checked first, exactly as
//! [`crate::audio_task::ffmpeg_program`] lets a configured box win over the bare name: a test points it at a
//! fake script through the same door the program uses, which is the same shape as pointing a server URL at a
//! loopback fake. No for-test helper is ever asked for an answer.

use std::sync::{Mutex, OnceLock};

/// The player this build reaches for when nobody named another.
pub const PLAYER: &str = "gst-play-1.0";

/// Where the running player's pid lives. A static rather than a thread-local because the child outlives the
/// call that started it and ⏹/▶ can be pressed from any main-context turn.
fn held() -> &'static Mutex<Option<u32>> {
    static SLOT: OnceLock<Mutex<Option<u32>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Which program plays, in the precedence every other leg of this app uses for a tool: the Settings box
/// first, then `NAIVEPOST_PLAYER`, then [`PLAYER`].
///
/// The spec names no player program (F2.1 says only what the press means), so this decides — and it decides
/// the way `audio_task::ffmpeg_program` and `speak_leg` decide theirs, so a person who set a path in the
/// Settings gets it honoured and a test points at a fake script through the same door the program reads.
/// No for-test helper is ever asked for an answer.
/// Which program plays: `NAIVEPOST_PLAYER` when set and non-empty, else [`PLAYER`]. Read per press like every
/// other settings value, so re-pointing it takes effect at once.
pub fn player_program() -> String {
    let configured = std::env::var("NAIVEPOST_PLAYER").unwrap_or_default();
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return PLAYER.to_string();
    }
    trimmed.to_string()
}

/// The argv: quiet (`--no-position` prints a position line forever), gapless between the files so a
/// recording's end walks on without a reload, and `-s` for the red line the press started at. Times go out
/// to a tenth of a second — finer is below what a preview cares about.
pub fn command_for(files: &[String], at_secs: f64) -> Vec<String> {
    let mut args = vec![
        "--no-position".to_string(),
        "--gapless".to_string(),
        "-s".to_string(),
        format!("{:.1}", (at_secs * 10.0).round() / 10.0),
    ];
    args.extend(files.iter().cloned());
    args
}

/// Start the player over `files` at `at_secs` and remember its pid. A failure is returned, never swallowed:
/// the caller logs it where the press's other lines go.
pub fn cue(files: &[String], at_secs: f64, log: &dyn Fn(&str)) -> Result<u32, String> {
    halt(); // one player at a time: a second cue replaces the run nobody paused instead of doubling sound
    let program = player_program();
    let args = command_for(files, at_secs);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    log(&crate::subprocess::log_line(&program, &refs));
    let child = crate::subprocess::spawn_piped(&program, &refs)
        .map_err(|err| format!("{} failed: {err}", crate::subprocess::shown(&program, &refs)))?;
    let pid = child.id();
    *held().lock().expect("the player slot") = Some(pid);
    // The child is deliberately not waited on: it plays until `halt`. Dropping it here leaves it running.
    std::mem::forget(child);
    Ok(pid)
}

/// The pid of the player this leg started, if one is running.
pub fn playing_pid() -> Option<u32> {
    *held().lock().expect("the player slot")
}

/// Stop the player. Answers whether there was one to stop.
pub fn halt() -> bool {
    let mut slot = held().lock().expect("the player slot");
    match slot.take() {
        Some(pid) => {
            // SIGTERM, not KILL: gst-play-1.0 tears its pipeline down on a term signal, which is what makes
            // the next cue start clean instead of finding the last one holding the file.
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            true
        }
        None => false,
    }
}
