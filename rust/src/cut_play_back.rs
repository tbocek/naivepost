//! F2.2 S5's media leg: the cut preview's TWO real player processes — the one playing and the spare.
//!
//! [`crate::play_back`] is ▶'s leg: one process, cued at the red line. ▶✂ needs a second one. Inventory
//! §D ("Preload") and `gui/player_spare.go` state the shape the spec's S5 requires — "next jump preloaded
//! three seconds ahead in a spare pipeline so the join does not freeze" — as *a pipeline taken to PAUSED at
//! exactly the next second, decoded and prerolled, so that when the line lands there the two pipelines
//! change places instead of building one*. In the prototype both are playbin pipelines inside one process.
//! Here the media leg is a subprocess (`play_back`, because no GStreamer library is linked), so the same
//! two states are two processes:
//!
//! - **preroll** = spawn the player with `-s` already at the jump's second, then `SIGSTOP` it at once. A
//!   stopped process builds its pipeline and reads its first frames and then freezes: decoded, prerolled,
//!   silent, holding no CPU. That is the spare, and it costs one signal.
//! - **swap in** = `SIGCONT` the spare and `SIGTERM` the outgoing one. The join therefore never waits for a
//!   pipeline: the process that plays the second clip was already sitting on it.
//!
//! Both slots hold one pid each, for the reason `gui/player_spare.go` gives ("One spare, not one per row:
//! the line goes one place next"). Every answer here is a signal on a real process, so a test observes it
//! through the fake player script at `NAIVEPOST_PLAYER` (see [`crate::play_back::player_program`]).
//!
//! # What the player's argv can and cannot express
//!
//! `gst-play-1.0 --help-all` was read for this (the container's own copy). Its whole flag list is
//! `--gapless`, `-s/--start`, `-a/--accurate-seeks`, `-i/--instant-rate-changes`, `--wait-on-eos`,
//! `--no-position`, `--use-playbin2`, `-p/--playlist`, `--no-missing-plugin-installation`, `-q/--quiet` and
//! the `--gst-*` group: **there is no rate or speed flag, and no volume flag.** `-i` only makes *its own*
//! rate changes instant, and nothing in the argv asks for a rate. So:
//!
//! - **rate** cannot be asked of a running player. [`set_rate`] says so honestly: a flat (1.0) rate re-cues
//!   at the head so no earlier stretch's clock can leak into the flat seconds; any other rate is logged as
//!   unexpressible and answered `false`. The app's own clock arithmetic is unaffected —
//!   [`crate::cut_play::cut_time`] does apply rates, and that is the number the person reads.
//! - **still** is expressible: `SIGSTOP` freezes the frame, which is exactly "stops show their still"
//!   ([`hold_still`]).
//! - **volume** is not in the argv either, so it never goes through this leg: the preview's one volume door
//!   (`ui::set_preview_volume`, fed by `cut_hear`) keeps it. See the note in
//!   [`crate::cut_play_leg::start_cut_preview_tick`].
//!
//! # How a test sees a signal
//!
//! A `SIGSTOP` leaves a mark a test can read without any help from this module: `ps -o state= -p <pid>` says
//! `T`. [`is_stopped_for_test`] reads exactly that, and the other `_for_test` doors show what each slot holds.
//! Those are the TEST's outside look: the program's own doors are [`preroll`], [`swap_spare_in`],
//! [`hold_still`], [`set_rate`] and [`halt`], and none of them asks for a state back.

use std::sync::{Mutex, OnceLock};

use crate::play_back;

/// The running cut-preview player's pid.
fn running() -> &'static Mutex<Option<u32>> {
    static SLOT: OnceLock<Mutex<Option<u32>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// The spare's pid — the process prerolled at the next jump's second and held stopped.
fn spare() -> &'static Mutex<Option<u32>> {
    static SLOT: OnceLock<Mutex<Option<u32>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Whether a `SIGSTOP` is holding the running player's frame. Kept separate from the pid slot because
/// [`halt`] clears both, and a still must never be reported for a player that is gone.
fn held_still() -> &'static Mutex<bool> {
    static SLOT: OnceLock<Mutex<bool>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(false))
}

/// Send `signal` to a pid.
fn signal(pid: u32, signal: i32) {
    // SAFETY: `kill` on an integer pid cannot corrupt memory; every pid here came from `spawn_piped`, so
    // the only processes ever signalled are ones this leg started in this user's session.
    unsafe { libc::kill(pid as i32, signal) };
}

/// Spawn the player over `files` at `at_secs` and answer its pid, WITHOUT putting it in any slot.
///
/// Same argv as ▶'s leg — deliberately the SAME function ([`play_back::command_for`]), so the two players
/// can never drift apart in spelling: `--no-position --gapless -s <tenth> <files…>`.
fn spawn_at(files: &[String], at_secs: f64, log: &dyn Fn(&str)) -> Result<u32, String> {
    let program = play_back::player_program();
    let args = play_back::command_for(files, at_secs);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    log(&crate::subprocess::log_line(&program, &refs));
    let child = crate::subprocess::spawn_piped(&program, &refs)
        .map_err(|err| format!("{} failed: {err}", crate::subprocess::shown(&program, &refs)))?;
    let pid = child.id();
    // Not waited on: it runs until it is halted. Dropping it here would reap it.
    std::mem::forget(child);
    Ok(pid)
}

/// Start the cut preview's player over `files` at `at_secs`, replacing whatever was running.
pub fn cue_clips(files: &[String], at_secs: f64, log: &dyn Fn(&str)) -> Result<u32, String> {
    // A still belongs to the process being replaced, so it is forgotten before the new one is trusted.
    let _ = hold_still(false);
    let pid = spawn_at(files, at_secs, log)?;
    let outgoing = running().lock().expect("the cut-preview player slot").replace(pid);
    if let Some(old) = outgoing {
        // SIGTERM, not KILL, for the reason `play_back::halt` gives: a term lets the pipeline tear itself
        // down instead of leaving the next one to find the file held.
        signal(old, libc::SIGTERM);
    }
    Ok(pid)
}

/// Preroll the spare at `at_secs`: spawn it there and freeze it at once, so the pipeline is built and
/// prerolled by the time the line needs it — and silent while it waits.
///
/// An older spare is stopped first: one slot, one spare, because the line goes one place next. Re-arming at
/// the second the spare already holds costs a new process, which is what a real player would also need;
/// callers avoid that churn by arming only when the armed second CHANGED (see [`crate::cut_play_leg::arm_spare`]).
pub fn preroll(files: &[String], at_secs: f64, log: &dyn Fn(&str)) -> Result<u32, String> {
    drop_spare();
    let pid = spawn_at(files, at_secs, log)?;
    signal(pid, libc::SIGSTOP);
    *spare().lock().expect("the spare slot") = Some(pid);
    Ok(pid)
}

/// The spare's pid, if one is holding a second. The program reads the spare only by arming and swapping it;
/// this is the outside look a test needs.
pub fn spare_pid_for_test() -> Option<u32> {
    *spare().lock().expect("the spare slot")
}

/// Whether `pid` is stopped — `T` in `ps`'s state column, which is what "prerolled and waiting" looks like
/// from outside a process, with no cooperation asked of the player. A test's evidence only: the program
/// freezes and releases through [`hold_still`] and never asks for the state back.
pub fn is_stopped_for_test(pid: u32) -> bool {
    let out = std::process::Command::new("ps")
        .args(["-o", "state=", "-p", &pid.to_string()])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().starts_with('T'),
        _ => false,
    }
}

/// Forget and stop the spare. Answers whether there was one.
pub fn drop_spare() -> bool {
    let mut slot = spare().lock().expect("the spare slot");
    match slot.take() {
        Some(pid) => {
            signal(pid, libc::SIGTERM);
            true
        }
        None => false,
    }
}

/// The swap S5 exists for: the spare becomes the running player (`SIGCONT`), the outgoing one goes
/// (`SIGTERM`). Answers the pid now playing, or `None` when no spare was waiting — the caller then cues,
/// which is the reload the spare was meant to avoid.
///
/// The still flag is cleared: a swapped-in process runs, whatever the stop before the jump left behind.
pub fn swap_spare_in(log: &dyn Fn(&str)) -> Option<u32> {
    let incoming = spare().lock().expect("the spare slot").take()?;
    let outgoing = running().lock().expect("the cut-preview player slot").replace(incoming);
    signal(incoming, libc::SIGCONT);
    if let Some(old) = outgoing {
        signal(old, libc::SIGTERM);
    }
    let _ = hold_still(false);
    log("    >>> preview: the spare pipeline was already sitting there \u{2014} swapped in");
    Some(incoming)
}

/// The cut preview's running player, if any.
pub fn playing_pid() -> Option<u32> {
    *running().lock().expect("the cut-preview player slot")
}

/// Stop the running player AND drop the spare: pausing the cut preview releases both, which is what every
/// caller wants (S4's pause, and the switch over to ▶'s recording). Answers whether a player was running.
pub fn halt() -> bool {
    let _ = hold_still(false);
    let mut slot = running().lock().expect("the cut-preview player slot");
    let was = match slot.take() {
        Some(pid) => {
            signal(pid, libc::SIGTERM);
            true
        }
        None => false,
    };
    // The spare goes too, and its answer is not this one: the caller asks whether a PLAYER was running.
    if drop_spare() {
        // Nothing else to do; the arm log keeps the record of the spare that was released.
    }
    was
}

/// S5, "stops show their still": freeze the running player's frame (`SIGSTOP`), or release it (`SIGCONT`).
/// Answers whether a process was there to hold — `false` means no player, not that the stop was refused.
///
/// A frozen player keeps its last decoded frame on screen; that IS the still, so nothing is drawn here.
/// When the seconds the stop covers end, the caller releases with `hold_still(false)`.
pub fn hold_still(hold: bool) -> bool {
    let Some(pid) = playing_pid() else {
        // Nothing to freeze, so nothing can be holding a still when the next stop arrives.
        if hold {
            *held_still().lock().expect("the still slot") = false;
        }
        return false;
    };
    signal(pid, if hold { libc::SIGSTOP } else { libc::SIGCONT });
    *held_still().lock().expect("the still slot") = hold;
    true
}

/// S5, "speed effects at their flat rate", handed to the player.
///
/// The limits are the player's, measured, not a choice about what to build: `gst-play-1.0 --help-all`
/// (module doc lists every flag) has **no rate flag**. So a rate cannot be asked of a live process and this
/// function does not pretend otherwise:
///
/// - no player at all → `false`, nothing happens.
/// - `rate == 1.0`, the flat rate a preview is asked to hold: the player is RE-CUED at the head with
///   `--gapless` kept, so the flat seconds run on a fresh flat pipeline and a rate written on the stretch
///   before them cannot leak in. `true` when a process was there to replace.
/// - any other rate: logged as not expressible through this player's argv and answered `false`.
///
/// The person's clock does not depend on any of this: [`crate::cut_play::cut_time`] applies the cut's rates
/// to the number printed, so the readout stays the finished video's time whatever the process can do.
/// S5 "speed effects at their flat rate", handed to the player at the head.
///
/// A rate change under a running preview is asked for as a re-cue, and re-cueing on every tick that asks is a
/// stutter the ear hears: inventory §D's `rateSeekGap 250 ms` is the floor the prototype holds its flushing
/// seeks to (`gui/player.go`'s `SetRateNow` — inside the gap the rate is put back so a later tick asks again).
/// The same floor applies to the one re-cue this player can do, so it lives here, and [`rate_seek_action`] is the
/// rule apart from any clock so a test can ask it directly.
pub const RATE_SEEK_GAP_MS: u64 = 250;

/// What a rate change asks of the player: re-cue now, or leave the rate alone and let a later tick ask again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateSeek {
    /// Too soon after the last one: the rate goes back and nothing is re-cued.
    Hold,
    /// At or past the gap: the re-cue may happen, and its moment is the one to remember.
    Seek,
}

/// §D's `rateSeekGap`: the plain answer, asked of elapsed time rather than of a clock, so a test needs neither.
/// An unchanged rate never asks at all — nothing moved, so nothing stumbles.
pub fn rate_seek_action(changed: bool, since_last_ms: Option<u64>) -> RateSeek {
    if !changed {
        return RateSeek::Hold;
    }
    match since_last_ms {
        // No seek has happened yet on this player: the first one is never too soon.
        None => RateSeek::Seek,
        Some(elapsed) if elapsed >= RATE_SEEK_GAP_MS => RateSeek::Seek,
        Some(_) => RateSeek::Hold,
    }
}

/// The last moment a rate change was allowed to re-cue, in `Instant`-free milliseconds this module keeps so
/// [`set_rate`] can answer its own `since_last_ms`. Per process, not per window: one player is running.
fn last_rate_seek() -> &'static std::sync::Mutex<Option<u64>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<u64>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// Milliseconds since the process started — the only clock this module reads, and it never decides a rule
/// itself: [`rate_seek_action`] is handed the elapsed number.
fn uptime_ms() -> u64 {
    std::time::UNIX_EPOCH
        .elapsed()
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

pub fn set_rate(files: &[String], rate: f64, at_secs: f64, log: &dyn Fn(&str)) -> bool {
    if playing_pid().is_none() {
        return false;
    }
    if (rate - 1.0).abs() > f64::EPSILON {
        log(&format!(
            "    >>> preview: gst-play-1.0 takes no rate flag \u{2014} a rate of {rate} is not expressible; \
             the clock still reads the cut's own time"
        ));
        return false;
    }
    // The flat rate WANTS a re-cue, but only once per gap: two ticks asking in a row would replace a player
    // that was already re-cued a hundred milliseconds ago, which is the stutter the gap exists to prevent.
    let now = uptime_ms();
    let since_last = *last_rate_seek().lock().expect("the rate-seek slot");
    if rate_seek_action(true, since_last.map(|last| now.saturating_sub(last))) == RateSeek::Hold {
        return false;
    }
    *last_rate_seek().lock().expect("the rate-seek slot") = Some(now);
    match cue_clips(files, at_secs, log) {
        Ok(_) => true,
        Err(why) => {
            log(&format!("    >>> preview: {why}"));
            false
        }
    }
}
