//! F0.3 ⏹ — the legs that make a press *do* something outside the rule table.
//!
//! [`crate::run::RunBar::press_stop`] holds the precedence (S1 → S2 → S3) and answers with what
//! should happen: stop playback, set the flag, cancel the context, kill these pids. That half is
//! pure state and is tested in `tests/suite/press_stop.rs`. What it cannot do from inside a data struct is
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

use std::sync::atomic::{AtomicBool, Ordering};
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
///
/// The beat is waited on a thread of its own: ⏹ is pressed on the window thread, which never waits
/// (§03-shell §1; the first rewrite slept the half second there, children or not). What comes back is
/// every pid the SIGTERM reached; the SIGKILL follows for any of them still alive after the beat.
pub fn signal(pids: &[u32]) -> Vec<u32> {
    let termed: Vec<u32> = pids
        .iter()
        .copied()
        .filter(|pid| send(*pid, libc::SIGTERM))
        .collect();
    if !termed.is_empty() {
        let escalate = termed.clone();
        std::thread::spawn(move || {
            std::thread::sleep(KILL_GRACE);
            for pid in escalate {
                // Ignored SIGTERM: end it outright.
                if alive(pid) {
                    send(pid, libc::SIGKILL);
                }
            }
        });
    }
    termed
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
pub fn alive(pid: u32) -> bool {
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

/// Whether the run is *inside* the Describe stage right now, §S5's question. Process-wide: the walk runs on a
/// worker and says which stage it is in, while ⏹ asks on the window thread.
static IN_DESCRIBE: AtomicBool = AtomicBool::new(false);

/// Opened at the head of the Describe stage, closed when it returns. While set, a ⏹ press reads
/// `true` for `in_describe` and arms the restart.
pub fn enter_describe() {
    IN_DESCRIBE.store(true, Ordering::SeqCst);
}

/// Close the stage. Called even when the stage ended early or failed: a stale `true` would arm a
/// restart for the *next* unrelated stop.
pub fn leave_describe() {
    IN_DESCRIBE.store(false, Ordering::SeqCst);
}

/// What a ⏹ press passes as `in_describe`: the stage the run is in, not the page showing.
pub fn describe_stage() -> bool {
    IN_DESCRIBE.load(Ordering::SeqCst)
}

/// A stop that landed inside Describe, still waiting to be taken up by the next Prepare run.
///
/// Sticky rather than read off [`describe_stage`] at the moment of decision: the stop happens
/// *during* Describe and closes it, while the restart is decided at the NEXT `begin`, long after
/// the stage flag went back to false. Consumed on read ([`take_describe_restart`]), so one stop
/// affects exactly one run — the same contract `RunBar::describe_restarts` holds. Process-wide: ⏹ arms
/// it on the window thread, and the next run's `begin` takes it on that run's worker.
static RESTART_ARMED: AtomicBool = AtomicBool::new(false);

/// Record that a stop reached Describe, arming the next Prepare run to start over.
pub fn arm_describe_restart() {
    RESTART_ARMED.store(true, Ordering::SeqCst);
}

/// Take the arming, clearing it: true once per armed stop, false every time after.
pub fn take_describe_restart() -> bool {
    RESTART_ARMED.swap(false, Ordering::SeqCst)
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
/// `Some(stopped)` when the page owns a preview ([`run::transport_for`]) that was playing or is parked part way
/// (started); `None` when the page has no preview (Prepare, Produce) or none was started, so the caller changes
/// nothing. The stopped preview is ENDED, `started` down: §11 §2 gives the bar's ▶ to the preview "until ⏹", so
/// the next ▶ is the page's step again (Narrate's ⏹ does the same, F4.5 S6). The first rewrite carried
/// `started` through and only stopped a playing preview, so after one preview ▶ never reached Suggest again.
pub fn stop_page_preview(
    page: crate::shell::Page,
    transport: run::Transport,
) -> Option<run::Transport> {
    run::transport_for(page, transport)
        .filter(|t| t.playing || t.started)
        .map(|_| run::Transport {
            playing: false,
            started: false,
        })
}
