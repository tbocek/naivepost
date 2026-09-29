//! F0.3 S3's third leg: the cancel context as something a live HTTP request can obey.
//!
//! [`crate::run::RunBar::press_stop`] writes the stop flag and marks the context cancelled, and
//! [`crate::server_leg`] asks a caller-supplied `cancelled: &dyn Fn() -> bool` before it sends and
//! again after the reply arrives. What was missing is the bridge between those two: a ⏹ press happens
//! on the GUI thread while the request is in flight on a worker, so the closure cannot borrow the
//! bar — it has to read a cell both sides can reach. That is this module: one shared switch per
//! session, written by the stop, read by every leg.
//!
//! The rule it implements is §02-services#1's last paragraph: *every* request rides the run's
//! cancel context (`services::rides_cancel_context` says true for all kinds), so a ⏹ during a long
//! synthesis ends the wait rather than leaving the app hanging on a server it already gave up on.
//!
//! Generation-scoped rather than a bare boolean, for the reason `runqueue::bump_cancel_context`
//! gives: a `cancelled` left over from the run before must not abort this one. A leg captures the
//! generation at start ([`capture`]) and its closure then answers true only while that same
//! generation is the stopped one — the same equality [`crate::runqueue::is_cancelled`] applies,
//! read off this cell instead of off the bar.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// How long a dropped-thread poll waits between reads of the switch.
///
/// Not a tuning knob the spec names: 50 ms keeps a ⏹ felt well inside a blink without spinning a
/// core, and it is short against the shortest leg the app makes (sd.cpp capabilities, 15 s).
const POLL: Duration = Duration::from_millis(50);

/// The session's cancel state: which generation was stopped, and whether anything is stopped now.
static STOPPED_GEN: AtomicU64 = AtomicU64::new(0);
static ANY_STOPPED: AtomicBool = AtomicBool::new(false);
/// Legs that captured no generation are never cancelled by a run's ⏹: generation 0 is the idle
/// number, and a stop always names a run's own, so the two never match.
static CURRENT_GEN: AtomicU64 = AtomicU64::new(0);


/// Force the current generation to a known number, so the switch and `RunBar::run_epoch` stay one
/// counter rather than two that can drift. Called by `runqueue::cancel_switch` right after the bar
/// bumps its own; a stop of generation N then cancels exactly the legs that captured N.
pub fn reset_to(epoch: u64) {
    ANY_STOPPED.store(false, Ordering::SeqCst);
    CURRENT_GEN.store(epoch, Ordering::SeqCst);
    HOLDING.swap(0, Ordering::SeqCst);
}



/// The closure for a call that belongs to no run — a settings probe, a request made before a
/// project exists — where a later run's ⏹ must not reach back and cancel it.
pub const NO_RUN: u64 = 0;

/// The generation a leg should capture if it does not care which run it belongs to.
pub fn current_generation() -> u64 {
    CURRENT_GEN.load(Ordering::SeqCst)
}

/// ⏹ was pressed: stop everything riding the cancel context, from now on.
///
/// Idempotent and lock-free because the press handler cannot wait on a worker that may be holding a
/// mutex; it just flips the word and carries on repainting the bar.
pub fn cancel_now() {
    STOPPED_GEN.store(CURRENT_GEN.load(Ordering::SeqCst), Ordering::SeqCst);
    ANY_STOPPED.store(true, Ordering::SeqCst);
}

/// Whether the generation `epoch` has been cancelled: only an equal generation talks, so a stop
/// belonging to another run cannot end this leg, and vice versa.
pub fn cancelled_at(epoch: u64) -> bool {
    ANY_STOPPED.load(Ordering::SeqCst) && STOPPED_GEN.load(Ordering::SeqCst) == epoch
}

/// The closure [`crate::server_leg`] takes, for the generation captured at `epoch`. A leg that
/// belongs to a run captures the bar's epoch ([`crate::runqueue::cancel_epoch`]) so a stop of a
/// DIFFERENT run leaves it alone; a leg belonging to no run passes [`NO_RUN`], which no ⏹ names.
pub fn cancel_check(epoch: u64) -> impl Fn() -> bool {
    move || cancelled_at(epoch)
}

/// The closure for whatever generation is current right now — what a settings probe uses.
pub fn cancel_check_now() -> impl Fn() -> bool {
    cancel_check(current_generation())
}

/// The same switch as a cheap function call, for a caller that holds nothing.
/// The same switch read without a generation, for a caller that only wants to know whether ⏹ has
/// been pressed at all — the end-of-run housekeeping thread asks this before it dials anything.
/// The same switch read without a generation, for a caller that only wants to know whether ⏹ has
/// been pressed at all — the end-of-run housekeeping thread asks this before it dials anything.
pub fn cancelled() -> bool {
    ANY_STOPPED.load(Ordering::SeqCst)
}

/// Clear the switch. `runqueue::end_run` calls it once a stopped run has fully unwound, so a
/// later run cannot inherit a stop that belongs to the one before it.
pub fn clear() {
    ANY_STOPPED.store(false, Ordering::SeqCst);
}

/// How many legs are in flight right now. Counted because a stop has to know whether anything is
/// left to wait for: with nothing outstanding there is no leg to abandon, and no reason to poll.
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

/// Wait out every leg in flight, or give up as soon as ⏹ lands — whichever comes first.
///
/// This is S4's "takes effect between subprocesses" for the calls that are NOT subprocesses: an
/// HTTP leg cannot be killed, only abandoned, and §S4 says the stop landing late is not a failure.
/// Reports how many legs were still out when the wait ended, so the caller can say what it gave up
/// on instead of leaving the user wondering whether a half-sent synthesis did something.
/// Returns immediately when nothing is outstanding.
pub fn await_stop(budget: Duration) -> StoppedLegs {
    if !ANY_STOPPED.load(Ordering::SeqCst) || budget.is_zero() {
        return StoppedLegs { waited: false, legs: legs_outstanding() };
    }
    let started = std::time::Instant::now();
    loop {
        let left = budget.saturating_sub(started.elapsed());
        if legs_outstanding() == 0 || !ANY_STOPPED.load(Ordering::SeqCst) || left.is_zero() {
            return StoppedLegs { waited: true, legs: legs_outstanding() };
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
/// The ⏹ handler calls this rather than doing it itself because the wait belongs to the cancel switch,
/// not to the shell: §S4 makes a stop that lands late a non-failure, so the press must neither hang on
/// an in-flight synthesis nor leave one running unseen. Bounded by the stop's own grace (`budget`),
/// never by the request's ceiling. Returns the count logged, 0 when nothing was outstanding.
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
