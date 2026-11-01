//! F2.2 S5's media leg: the cut preview's TWO real player processes — the one playing and the spare.

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
pub(crate) fn signal(pid: u32, signal: i32) {
    // SAFETY: `kill` on an integer pid cannot corrupt memory; every pid here came from `spawn_piped`, so
    // the only processes ever signalled are ones this leg started in this user's session.
    unsafe { libc::kill(pid as i32, signal) };
}

/// Spawn the player over `files` at `at_secs` and answer its pid, WITHOUT putting it in any slot.
fn spawn_at(files: &[String], at_secs: f64, log: &dyn Fn(&str)) -> Result<u32, String> {
    let program = play_back::player_program();
    let args = play_back::command_for(files, at_secs);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    log(&crate::subprocess::log_line(&program, &refs));
    let child = crate::subprocess::spawn_piped(&program, &refs).map_err(|err| {
        format!(
            "{} failed: {err}",
            crate::subprocess::shown(&program, &refs)
        )
    })?;
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
    let outgoing = running()
        .lock()
        .expect("the cut-preview player slot")
        .replace(pid);
    if let Some(old) = outgoing {
        signal(old, libc::SIGTERM);
    }
    Ok(pid)
}

/// Preroll the spare at `at_secs`: spawn it there and freeze it at once, so the pipeline is built and
/// prerolled by the time the line needs it — and silent while it waits.
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

pub fn swap_spare_in(log: &dyn Fn(&str)) -> Option<u32> {
    let incoming = spare().lock().expect("the spare slot").take()?;
    let outgoing = running()
        .lock()
        .expect("the cut-preview player slot")
        .replace(incoming);
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
    if drop_spare() {}
    was
}

/// S5, "stops show their still": freeze the running player's frame (`SIGSTOP`), or release it (`SIGCONT`).
pub fn hold_still(hold: bool) -> bool {
    let Some(pid) = playing_pid() else {
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
