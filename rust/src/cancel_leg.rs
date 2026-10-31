//! F0.3 S3's third leg: the cancel context as something a live HTTP request can obey.
//!
//! A ⏹ press happens on the GUI thread while the request is in flight on a worker, so the
//! `cancelled: &dyn Fn() -> bool` that [`crate::server_leg`] asks cannot borrow the bar; it reads a shared
//! cell instead: one switch per session, written by [`crate::run::RunBar::press_stop`], read by every leg.
//! §02-services#1: *every* request rides the run's cancel context (`services::rides_cancel_context`).
//!
//! Generation-scoped (as `runqueue::bump_cancel_context`): a `cancelled` left over from the run before must
//! not abort this one. A leg captures the generation at start (`capture`) and answers true only while that
//! generation is the stopped one, as [`crate::runqueue::is_cancelled`] does.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// How long a dropped-thread poll waits between reads of the switch: 50 ms keeps ⏹ felt well inside a blink
/// without spinning a core, and is short against the shortest leg (sd.cpp capabilities, 15 s).
const POLL: Duration = Duration::from_millis(50);

static STOPPED_GEN: AtomicU64 = AtomicU64::new(0);
static ANY_STOPPED: AtomicBool = AtomicBool::new(false);
/// Legs that captured no generation are never cancelled by a run's ⏹: 0 is the idle number, a stop names a run's own.
static CURRENT_GEN: AtomicU64 = AtomicU64::new(0);

/// Force the current generation to a known number, so the switch and `RunBar::run_epoch` stay one counter.
/// Called by `runqueue::cancel_switch` right after the bar bumps its own.
pub fn reset_to(epoch: u64) {
    ANY_STOPPED.store(false, Ordering::SeqCst);
    CURRENT_GEN.store(epoch, Ordering::SeqCst);
    HOLDING.swap(0, Ordering::SeqCst);
}

/// The closure for a call that belongs to no run (a settings probe, a request before a project exists).
pub const NO_RUN: u64 = 0;

/// The generation a leg should capture if it does not care which run it belongs to.
pub fn current_generation() -> u64 {
    CURRENT_GEN.load(Ordering::SeqCst)
}

/// ⏹ was pressed: stop everything riding the cancel context, from now on.
///
/// Idempotent and lock-free: the press handler cannot wait on a worker that may hold a mutex.
pub fn cancel_now() {
    STOPPED_GEN.store(CURRENT_GEN.load(Ordering::SeqCst), Ordering::SeqCst);
    ANY_STOPPED.store(true, Ordering::SeqCst);
}

/// Whether the generation `epoch` has been cancelled: only an equal generation talks.
pub fn cancelled_at(epoch: u64) -> bool {
    ANY_STOPPED.load(Ordering::SeqCst) && STOPPED_GEN.load(Ordering::SeqCst) == epoch
}

/// The closure [`crate::server_leg`] takes, for the generation captured at `epoch`. A leg in a run captures
/// the bar's epoch ([`crate::runqueue::cancel_epoch`]); one belonging to no run passes [`NO_RUN`], which no ⏹ names.
pub fn cancel_check(epoch: u64) -> impl Fn() -> bool {
    move || cancelled_at(epoch)
}

/// The closure for whatever generation is current right now, what a settings probe uses.
pub fn cancel_check_now() -> impl Fn() -> bool {
    cancel_check(current_generation())
}

/// The same switch read without a generation, for a caller that only wants to know whether ⏹ has been
/// pressed at all (the end-of-run housekeeping thread asks before it dials anything).
pub fn cancelled() -> bool {
    ANY_STOPPED.load(Ordering::SeqCst)
}

/// Clear the switch; `runqueue::end_run` calls it once a stopped run has unwound, so no later run inherits the stop.
pub fn clear() {
    ANY_STOPPED.store(false, Ordering::SeqCst);
}

/// How many legs are in flight right now, so a stop knows whether anything is left to wait for.
static HOLDING: AtomicU64 = AtomicU64::new(0);

/// Note that a leg has started, so [`await_stop`] knows there is something out there.
pub fn leg_started() {
    HOLDING.fetch_add(1, Ordering::SeqCst);
}

/// Note that a leg finished, cancelled or not.
pub fn leg_finished() {
    HOLDING.fetch_sub(1, Ordering::SeqCst);
}

/// Legs still outstanding.
pub fn legs_outstanding() -> u64 {
    HOLDING.load(Ordering::SeqCst)
}

/// Wait out every leg in flight, or give up as soon as ⏹ lands, whichever comes first.
///
/// S4's "takes effect between subprocesses" for calls that are NOT subprocesses: an HTTP leg can only be
/// abandoned, and a stop landing late is not a failure. Reports how many legs were still out so the caller
/// can say what it gave up on. Returns immediately when nothing is outstanding.
pub fn await_stop(budget: Duration) -> StoppedLegs {
    if !ANY_STOPPED.load(Ordering::SeqCst) || budget.is_zero() {
        return StoppedLegs {
            waited: false,
            legs: legs_outstanding(),
        };
    }
    let started = std::time::Instant::now();
    loop {
        let left = budget.saturating_sub(started.elapsed());
        if legs_outstanding() == 0 || !ANY_STOPPED.load(Ordering::SeqCst) || left.is_zero() {
            return StoppedLegs {
                waited: true,
                legs: legs_outstanding(),
            };
        }
        std::thread::sleep(POLL.min(left));
    }
}

/// What [`await_stop`] gave up on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoppedLegs {
    /// Whether a stop was under way at all (false = nothing to wait for).
    pub waited: bool,
    /// Legs still unanswered when the wait ended.
    pub legs: u64,
}

/// Give the legs already on the wire a moment to be abandoned, and SAY how many are being dropped.
///
/// Lives here because the wait belongs to the cancel switch, not the shell: the press must neither hang on
/// an in-flight synthesis nor leave one running unseen. Bounded by the stop's own grace (`budget`). Returns
/// the count logged, 0 when nothing was outstanding.
pub fn report_abandoned(budget: Duration) -> u64 {
    let dropped = crate::settings_probe::drain_unload(budget);
    if dropped.legs > 0 {
        crate::ui::window::log_line(&format!(
            ">>> stop: {legs} server call(s) still in flight, abandoned",
            legs = dropped.legs
        ));
    }
    dropped.legs
}
