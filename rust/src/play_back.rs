//! F2.1's media leg: the player that actually plays the recording.
//!
//! ▶ hands the session's footage to a real player (§0: the app draws no frames). Two players answer to the doors here ([`running`], [`halt`], [`set_volume`]): the built-in one
//! ([`crate::play_built_in`], used whenever no program is named) and the subprocess below, which plays a NAMED program (`NAIVEPOST_PLAYER`) in its own process.
//!
//! The spec names no player program, so `gst-play-1.0` (the one media player the container ships) is the default; `NAIVEPOST_PLAYER` wins, as [`crate::audio_task::ffmpeg_program`] lets a configured box win, so a test points it at a fake script.

use std::sync::{Mutex, OnceLock};

pub const PLAYER: &str = "gst-play-1.0";

/// Where the running player's pid lives: a static, since the child outlives the call that started it and ⏹/▶ can be pressed from any main-context turn.
fn held() -> &'static Mutex<Option<u32>> {
    static SLOT: OnceLock<Mutex<Option<u32>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Which program plays, in the precedence every other tool leg uses: the Settings box first, then `NAIVEPOST_PLAYER` (read per press), then [`PLAYER`].
pub fn player_program() -> String {
    let configured = std::env::var("NAIVEPOST_PLAYER").unwrap_or_default();
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return PLAYER.to_string();
    }
    trimmed.to_string()
}

/// The argv: quiet (`--no-position` prints a position line forever), gapless between files, `-s` for the red line the press started at. Times to a tenth of a second.
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

/// Start the player over `files` at `at_secs` and remember its pid. A failure is returned; the caller logs it.
pub fn cue(files: &[String], at_secs: f64, log: &dyn Fn(&str)) -> Result<u32, String> {
    halt_process(); // one player at a time: a second cue replaces the run nobody paused instead of doubling sound
                    // A fresh process has never heard this player's last volume ask, so the stamp starts blank, or the new run would play at the old loudness.
    if let Ok(mut stamp) = applied_volume().lock() {
        *stamp = None;
    }
    let program = player_program();
    let args = command_for(files, at_secs);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    log(&crate::subprocess::log_line(&program, &refs));
    let child = crate::subprocess::spawn_piped(&program, &refs).map_err(|err| {
        format!(
            "{} failed: {err}",
            crate::subprocess::shown(&program, &refs)
        )
    })?;
    let pid = child.id();
    *held().lock().expect("the player slot") = Some(pid);
    // The list travels beside the pid so [`set_volume`] can re-cue THE SAME files at a new loudness.
    if let Ok(mut files) = running_files().lock() {
        *files = files_of(&args);
    }
    // The child is deliberately not waited on: it plays until `halt`. Dropping it here leaves it running.
    std::mem::forget(child);
    Ok(pid)
}

/// The file list the live process was cued with, kept so [`set_volume`] can hand the same files back with a new `--volume`.
fn running_files() -> &'static Mutex<Vec<String>> {
    static SLOT: std::sync::OnceLock<Mutex<Vec<String>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
}

/// The footage names out of a built argv: everything after the four flags [`command_for`] puts first.
fn files_of(args: &[String]) -> Vec<String> {
    args.iter().skip(4).cloned().collect()
}

/// The pid of the player this leg started, if one is running.
pub fn playing_pid() -> Option<u32> {
    *held().lock().expect("the player slot")
}

/// Is ▶'s player playing: the subprocess this leg started, or the built-in one ([`crate::play_built_in`])?
pub fn running() -> bool {
    playing_pid().is_some() || crate::play_built_in::playing()
}

/// Stop the player — the subprocess, and pause the built-in one on its frame. Answers whether there was one.
pub fn halt() -> bool {
    let paused = crate::play_built_in::pause();
    halt_process() || paused
}

/// Stop the subprocess. Answers whether there was one to stop.
fn halt_process() -> bool {
    let mut slot = held().lock().expect("the player slot");
    match slot.take() {
        Some(pid) => {
            // SIGTERM, not KILL: gst-play-1.0 tears its pipeline down on term, so the next cue starts clean.
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            true
        }
        None => false,
    }
}

/// The loudness the live footage process was last asked for and the second it was put back at, so a repeating tick does nothing.
fn applied_volume() -> &'static Mutex<Option<(f64, f64)>> {
    static SLOT: std::sync::OnceLock<Mutex<Option<(f64, f64)>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Ask the live footage process to play at `gain`, in the only way this player can be asked.
///
/// DECISION (`the footage's own sound: muted by property when the scene silences it`): `gst-play-1.0` takes its loudness as playbin's volume PROPERTY at startup, with no runtime
/// volume command, so "by property, never by stopping" is honoured by re-cueing the SAME files at the SAME second with a new `--volume`: a seek, not a stop.
/// [`volume_ask`]'s stamp makes a repeating ask do nothing. `Player::footage_muted` stays the flag the rest of the app reads.
pub fn set_volume(gain: f64, at_secs: f64) -> bool {
    // The built-in player has a live property to ask, so it is asked and nothing is re-cued.
    if crate::play_built_in::set_volume(gain) {
        return true;
    }
    let Some(files) = cued_files() else {
        return false;
    };
    if playing_pid().is_none() {
        return false;
    }
    let Some(args) = volume_ask(&files, gain, at_secs) else {
        // Already asked for exactly this: do nothing at all, and say it was handled.
        return true;
    };
    let program = player_program();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let Ok(child) = crate::subprocess::spawn_piped(&program, &refs) else {
        return false;
    };
    let pid = child.id();
    std::mem::forget(child);
    if let Some(previous) = held().lock().expect("the player slot").replace(pid) {
        unsafe { libc::kill(previous as i32, libc::SIGTERM) };
    }
    note_ask(gain, at_secs);
    true
}

/// The files the live process was cued with, or `None` when nothing is running to be asked about.
pub fn cued_files() -> Option<Vec<String>> {
    let files = running_files().lock().expect("the file list").clone();
    (!files.is_empty()).then_some(files)
}

/// The argv a loudness ask WOULD make: `None` when the live process was already asked for exactly this gain at this second (ten ticks out of ten).
///
/// Pure: it consults and formats but spawns nothing, so "muted by property, never by stopping" can be read off the argv. [`set_volume`] carries it out.
pub fn volume_ask(files: &[String], gain: f64, at_secs: f64) -> Option<Vec<String>> {
    let asked = round_gain(gain);
    if already_asked(asked, at_secs) {
        return None;
    }
    Some(volume_args(files, asked, at_secs))
}

/// Record an ask as made WITHOUT spawning: the write half of [`volume_ask`]'s stamp.
pub fn note_ask(gain: f64, at_secs: f64) {
    let asked = round_gain(gain);
    if let Ok(mut stamp) = applied_volume().lock() {
        *stamp = Some((asked, at_secs));
    }
}

/// The gain rounded to thousandths, which is the precision a volume property is asked in.
fn round_gain(gain: f64) -> f64 {
    (gain.max(0.0) * 1000.0).round() / 1000.0
}

/// Has the live process already been asked for exactly this loudness at this second? Two asks inside a tenth of a second (the finest time any leg speaks in) are the same ask.
fn already_asked(asked: f64, at_secs: f64) -> bool {
    applied_volume()
        .lock()
        .map(|stamp| {
            stamp
                .map(|(last, last_at)| {
                    (last - asked).abs() < f64::EPSILON && (last_at - at_secs).abs() < 0.05
                })
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// The argv that carries a loudness: quiet, seeked back to the same second, at `asked`, over the same files.
fn volume_args(files: &[String], asked: f64, at_secs: f64) -> Vec<String> {
    let mut args = vec![
        "--no-position".to_string(),
        "-s".to_string(),
        format!("{:.1}", (at_secs * 10.0).round() / 10.0),
        "--volume".to_string(),
        format!("{:.3}", asked),
    ];
    args.extend(files.iter().cloned());
    args
}

/// Mute or unmute the footage's own sound (F2.5 S2): silence when the scene under the line hushes it, `gain`
/// when it does not. Same seek-not-stop reasoning as [`set_volume`].
pub fn set_muted(muted: bool, gain: f64, at_secs: f64) -> bool {
    set_volume(if muted { 0.0 } else { gain }, at_secs)
}

/// Forget what loudness was last asked for, so the next ask is applied whatever it is.
///
/// A test seam: the stamp is process-wide, so a test would otherwise be answered by its own earlier ask. [`cue`] clears the stamp itself on the live path.
pub fn forget_volume_for_test() {
    if let Ok(mut stamp) = applied_volume().lock() {
        *stamp = None;
    }
}
