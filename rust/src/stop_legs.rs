//! F0.3 ⏹ — the legs that make a press *do* something outside the rule table.
//!
//! [`crate::run::RunBar::press_stop`] holds the precedence (S1 → S2 → S3) and answers with what
//! should happen: stop playback, set the flag, cancel the context, kill these pids. That half is
//! pure state and is tested in `tests/press_stop.rs`. What it cannot do from inside a data struct is
//! reach the operating system, so the three effects that leave the process live here:
//!
//! - **signal** the children S3 names (§S3 "kill registered subprocesses"),
//! - **stop** the page's preview player on disk-and-state, not only in the status line (§S1),
//! - **say which stage is running**, because §S5 arms "describe from the start" on a stop that
//!   reached Describe, and the shell's visible page alone cannot tell that apart from merely
//!   sitting on the Prepare page while some other stage runs.
//!
//! Kept out of `run.rs` so the rule module stays free of OS calls and testable without them, and out
//! of `ui/window.rs`, which is past its size budget.

use std::cell::Cell;
use std::time::Duration;

use crate::run::{self, Subprocesses};

/// Terminate each pid with SIGTERM and report the ones that were really signalled.
///
/// A pid already gone (`ESRCH`) is NOT reported and NOT an error: the child exited between the run
/// registering it and the stop reaching it, and §S4 says a subprocess stopped by this stop is not a
/// failure. Anything else (a pid we have no right to signal) is likewise left out of the returned
/// list rather than turned into a failed stop — ⏹ must never fail because one child was stubborn.
///
/// SIGTERM first, then SIGKILL for what ignored it: `sleep` in this container accepts SIGTERM
/// (`kill(2)` returns 0) and carries on running, so TERM alone would leave every ffmpeg and ffprobe
/// alive behind a "stopping…" line. The escalation waits one polling beat, which is also why §S4's
/// "takes effect between subprocesses" is honoured rather than raced: nothing here interrupts a stage
/// mid-write, it only ends children the run registered as killable.
pub fn signal(pids: &[u32]) -> Vec<u32> {
    let term = pids.iter().copied().filter(|pid| send(*pid, libc::SIGTERM));
    // Collect before escalating: the second pass must see the state after the first had its beat.
    let mut still_there: Vec<u32> = term.collect();
    std::thread::sleep(KILL_GRACE);
    still_there.retain(|pid| {
        if alive(*pid) {
            // Ignored SIGTERM: end it outright and keep it in the reported set, since ⏹ did kill it.
            send(*pid, libc::SIGKILL)
        } else {
            true
        }
    });
    still_there
}

/// How long a SIGTERM gets before the same pid is killed outright. One poll beat: long enough for a
/// well-behaved tool to clean up after itself, short enough that ⏹ feels instant.
const KILL_GRACE: Duration = Duration::from_millis(500);

/// Send one signal to one pid, reporting whether it landed.
fn send(pid: u32, signal: i32) -> bool {
    // SAFETY: `kill(2)` with a pid read from our own registry and a constant signal number. A stale
    // pid can only mean ESRCH or, recycled into another user's process, EPERM; both fall through as
    // "not signalled".
    unsafe { libc::kill(pid as libc::pid_t, signal) == 0 }
}

/// Whether this pid still exists, by asking the kernel with signal 0 (which delivers nothing).
fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 performs error checking without sending anything.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Drain the registry and terminate everything in it: the one call ⏹ makes for §S3's kill.
///
/// The drain happens first and is not optional — a stopped pid left listed gets killed again by the
/// next ⏹, and by then the number may name somebody else's process (`run.rs`'s note on
/// [`Subprocesses::kill_all`]). Returns the pids actually terminated so the caller can log them.
pub fn stop_registered(procs: &mut Subprocesses) -> Vec<u32> {
    let drained = procs.kill_all();
    signal(&drained)
}

thread_local! {
    /// Whether the run is *inside* the Describe stage right now — §S5's question.
    static IN_DESCRIBE: Cell<bool> = const { Cell::new(false) };
}

/// Opened at the head of the Describe stage, closed when it returns. While set, a ⏹ press reads
/// `true` for `in_describe` and arms the restart.
pub fn enter_describe() {
    IN_DESCRIBE.with(|flag| flag.set(true));
}

/// Close the stage. Called even when the stage ended early or failed: a stale `true` would arm a
/// restart for the *next* unrelated stop.
pub fn leave_describe() {
    IN_DESCRIBE.with(|flag| flag.set(false));
}

/// What a ⏹ press passes as `in_describe`: the stage the run is in, not the page showing.
pub fn describe_stage() -> bool {
    IN_DESCRIBE.with(|flag| flag.get())
}

thread_local! {
    /// A stop that landed inside Describe, still waiting to be taken up by the next Prepare run.
    ///
    /// Sticky rather than read off [`describe_stage`] at the moment of decision: the stop happens
    /// *during* Describe and closes it, while the restart is decided at the NEXT `begin`, long after
    /// the stage flag went back to false. Consumed on read ([`take_describe_restart`]), so one stop
    /// affects exactly one run — the same contract `RunBar::describe_restarts` holds.
    static RESTART_ARMED: Cell<bool> = const { Cell::new(false) };
}

/// Record that a stop reached Describe, arming the next Prepare run to start over.
pub fn arm_describe_restart() {
    RESTART_ARMED.with(|flag| flag.set(true));
}

/// Take the arming, clearing it: true once per armed stop, false every time after.
pub fn take_describe_restart() -> bool {
    RESTART_ARMED.with(|flag| {
        let armed = flag.get();
        // Written back explicitly rather than through `Cell::replace` so the clearing is visible at the
        // call site and cannot be mistaken for a plain read.
        flag.set(false);
        armed
    })
}

/// Whether the next Prepare run describes from the start rather than taking the description up where
/// it stopped.
///
/// Either half alone is enough, because both say the same thing about what is on disk: the middle of the
/// description cannot be trusted. It is untrusted because a ⏹ cut through the stage (`armed`), or
/// because the last run left its working files behind without finishing them (`resume_marker_present`
/// — a lane's `events.tsv` / `state.txt`, which only exist mid-description; F1.1 S3 reads exactly
/// those). Starting over deletes only those two files: the scaled frames, the extracted frames and the
/// transcripts are kept, and that kept work is what spec/00-principles #4 protects — "a stopped run
/// resumes" speaks of the outputs, not of resuming a half-written event stream.
pub fn describes_from_the_start(armed: bool, resume_marker_present: bool) -> bool {
    armed || resume_marker_present
}

/// The transport this page's preview must be left in after a ⏹ press, if the press stops one.
///
/// `Some(stopped)` when the page owns a preview ([`run::transport_for`]) and it was playing; `None`
/// when the page has no preview (Prepare, Produce) or none was playing, so the caller changes nothing.
/// `started` is carried through untouched: a stopped-but-still-loaded preview stays loaded, which is
/// what makes ▶ mean "resume" rather than "start over".
pub fn stop_page_preview(page: crate::shell::Page, transport: run::Transport) -> Option<run::Transport> {
    run::transport_for(page, transport)
        .filter(|t| t.playing)
        .map(|t| run::Transport { playing: false, started: t.started })
}
