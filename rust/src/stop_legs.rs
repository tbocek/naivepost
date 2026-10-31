//! F0.3 ⏹ — the legs that make a press *do* something outside the rule table.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::run::{self, Subprocesses};

/// Terminate each pid with SIGTERM and report the ones that were really signalled.
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
static RESTART_ARMED: AtomicBool = AtomicBool::new(false);

/// Record that a stop reached Describe, arming the next Prepare run to start over.
pub fn arm_describe_restart() {
    RESTART_ARMED.store(true, Ordering::SeqCst);
}

/// Take the arming, clearing it: true once per armed stop, false every time after.
pub fn take_describe_restart() -> bool {
    RESTART_ARMED.swap(false, Ordering::SeqCst)
}

/// The transport this page's preview must be left in after a ⏹ press, if the press stops one.
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
