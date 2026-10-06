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
    // A fresh process has never heard this player's last volume ask, so the stamp starts blank; otherwise an
    // apply that looks "already done" would be skipped and the new run would play at the old loudness.
    if let Ok(mut stamp) = applied_volume().lock() {
        *stamp = None;
    }
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

/// The file list the live process was cued with, empty when nothing was ever cued. Kept beside the pid for
/// exactly one reason: [`set_volume`] has to hand the same files back with a new `--volume`.
fn running_files() -> &'static Mutex<Vec<String>> {
    static SLOT: std::sync::OnceLock<Mutex<Vec<String>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
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

/// The loudness the live footage process was last asked for, and the second it was put back at — so a tick
/// that repeats the previous answer does nothing at all.
fn applied_volume() -> &'static Mutex<Option<(f64, f64)>> {
    static SLOT: std::sync::OnceLock<Mutex<Option<(f64, f64)>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Ask the live footage process to play at `gain`, in the only way this player can be asked.
///
/// DECISION (`the footage's own sound: muted by property when the scene silences it`): `gst-play-1.0` takes
/// its loudness as playbin's volume PROPERTY at startup — `--help-all` lists `--volume  Volume` and
/// `-s, --start-position`, and no runtime volume command — so "by property, never by stopping" is honoured
/// here by re-cueing the SAME files at the SAME second with a new `--volume`. That is a seek, not a stop:
/// the stream is never torn down between answers, which is the difference the spec draws, and it costs
/// nothing while the answer holds still — [`volume_ask`]'s stamp makes a repeating ask do exactly nothing.
/// `Player::footage_muted` stays the flag the rest of the app reads; this is only how it reaches the box's
/// player.
pub fn set_volume(gain: f64, at_secs: f64) -> bool {
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
    let files = running_files()
        .lock()
        .expect("the file list")
        .clone();
    (!files.is_empty()).then_some(files)
}

/// [`set_volume`] over an explicit file list — the seam a test uses to watch what the mix asks for without
/// a media process at all (the same reasoning as [`crate::lane_hear::apply_for_test`]). The stamp is still
/// consulted, so a repeated ask does nothing here either: that is the rule being tested, not an artefact of
/// spawning. `None` means "nothing was asked", which is what a settled answer looks like.
/// The argv a loudness ask WOULD make, and whether it is an ask at all: `None` when the live process has
/// already been asked for exactly this gain at this second, which is what ten ticks out of ten look like.
///
/// A pure door on purpose — it consults and formats but spawns nothing — so "muted by property, never by
/// stopping" can be read off the argv without a media process. [`set_volume`] is this ask carried out.
pub fn volume_ask(files: &[String], gain: f64, at_secs: f64) -> Option<Vec<String>> {
    let asked = round_gain(gain);
    if already_asked(asked, at_secs) {
        return None;
    }
    Some(volume_args(files, asked, at_secs))
}

/// Record an ask as having been made, WITHOUT spawning anything. The write half of [`volume_ask`]'s stamp:
/// together they let a caller (a test, or a future leg that owns its own process) settle the "already asked"
/// answer without going through [`set_volume`]'s spawn.
fn note_ask(gain: f64, at_secs: f64) {
    let asked = round_gain(gain);
    if let Ok(mut stamp) = applied_volume().lock() {
        *stamp = Some((asked, at_secs));
    }
}

/// [`note_ask`] under a name that says who may call it: the write half of [`volume_ask`]'s stamp, for a test
/// that has to settle "already asked" without spawning a process. Nothing in the live path calls it.
pub fn note_ask_for_test(gain: f64, at_secs: f64) {
    note_ask(gain, at_secs);
}

/// The gain rounded to thousandths, which is the precision a volume property is asked in.
fn round_gain(gain: f64) -> f64 {
    (gain.max(0.0) * 1000.0).round() / 1000.0
}

/// Has the live process already been asked for exactly this loudness at this second? A tenth of a second is
/// the finest time any leg of this app speaks in, so two asks inside it are the same ask.
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
/// A test seam with one honest reason to exist: the stamp is process-wide, and a test that wants to watch an
/// ask it has already made in this same process would otherwise be answered by the earlier one. The live
/// path never needs it — [`cue`] clears the stamp itself, because a fresh process has not heard any ask.
pub fn forget_volume_for_test() {
    if let Ok(mut stamp) = applied_volume().lock() {
        *stamp = None;
    }
}
