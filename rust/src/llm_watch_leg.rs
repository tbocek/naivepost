//! F6.3 §4's live half: the stall watch put around a real request.
//!
//! [`crate::llm_liveness`] holds §4's rules as pure state — `Watch`, `Poll`, `blame` — and
//! `tests/llm_liveness_f6_3.rs` pins them with moments handed in as arguments. What pure state cannot do is sit
//! next to a blocked socket read, which is exactly where §4's rule has to live: "no byte for P.eng.llmStallMinutes"
//! is a statement about a call that is *right now* producing nothing. This module is the ticker that makes that
//! true on the wire.
//!
//! # Why not a ureq timeout
//!
//! ureq 3 offers `timeout_recv_body`, and it is the wrong tool twice over: its budget is **total** for the body
//! ("the budget is not restarted for each read", per its own docs), so it bounds how long an answer may take rather
//! than how long it may be silent — and §4 is explicit that a streamed call may run as long as it takes. So silence
//! is judged by a watch polled from the outside while the send blocks on another thread. The polling period is
//! [`llm_liveness::POLL_SECONDS`], i.e. four looks per heartbeat, which is what catches a stall on the bound rather
//! than up to a minute after it.
//!
//! # Why the send goes on a thread, and why that thread is never joined
//!
//! [`guarded`] runs the caller's closure on a spawned thread and waits for it with [`std::sync::mpsc`] timeouts in
//! the **calling** thread. That is not decoration: the log sink (`ui::window::log_line`) is a `thread_local!` list,
//! so a heartbeat logged from the worker would land in a log nobody reads. Every line this module writes — every
//! "nothing yet, T in" and the give-up — is written by the thread that called [`guarded`].
//!
//! The worker is DETACHED, and that is the whole of what makes a give-up an ending: a call given up for silence is
//! given up while the worker sits in a read nothing can interrupt, so joining it (what a scoped thread does
//! silently on the way out) waits forever on a server that has no answer. Instead the worker owns everything it
//! touches, its answer reaches us only if it arrives before the bound, and after a give-up its channel send fails
//! harmlessly when the kernel finally ends its read. It holds one socket and nothing else.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::llm_liveness::{self, Poll, Watch};

/// What the caller says this call is, for the log's `>>> <step>` prefix, and whether §4's silence rule applies.
///
/// `streamed` is the caller's own decision about the request body (§1: a chat ask sends `"stream": true`), passed
/// through rather than re-derived here, so there is one answer to "is this call streamed?" per request.
/// What the caller says this call is, for the log's `>>> <step>` prefix, and whether §4's silence rule applies.
#[derive(Debug, Clone)]
pub struct Liveness {
    pub step: String,
    pub streamed: bool,
}

impl Liveness {
    /// A streamed chat call: §4's stall rule applies.
    pub fn streamed(step: &str) -> Liveness {
        Liveness { step: step.to_string(), streamed: true }
    }
}

/// The thing the reader pings and the watch reads: when did a byte last arrive, and did any ever?
///
/// Shared because the two facts are gathered on the worker thread (only it sees the reads) and judged on the calling
/// thread (only it owns the watch and the log). Two atomics rather than a mutex: a ping must never block a read.
#[derive(Debug, Default)]
pub struct Probe {
    /// Arrival times in the same epoch seconds the watch works in, which is what makes them comparable.
    last_byte: AtomicU64,
    ever_arrived: AtomicBool,
}

impl Probe {
    pub fn new() -> Arc<Probe> {
        Arc::new(Probe::default())
    }

/// Something came off the wire. Any byte counts — §4 says so outright, keep-alives included — and the caller
    /// does not have to know what the bytes meant to make this call.
    ///
    /// Stamped in whole seconds: that is the unit [`Watch::quiet`] subtracts, so a millisecond stamp here would be
    /// read as an absurdly long silence and give up on the first poll.
    pub fn byte(&self) {
        self.byte_at(now_secs());
    }

    /// [`Probe::byte`] with the stamp taken from the call's own clock, which is what a watched leg uses: the stamp
    /// must be comparable with the `now` handed to [`Watch::poll`], or a difference between them is not elapsed time.
    pub fn byte_at(&self, now: u64) {
        self.last_byte.store(now, Ordering::Relaxed);
        self.ever_arrived.store(true, Ordering::Relaxed);
    }

    /// When the most recent byte arrived; `None` when none has.
    pub fn last_byte_seen(&self) -> Option<u64> {
        match self.last_byte.load(Ordering::Relaxed) {
            0 => None,
            secs => Some(secs),
        }
    }
}

/// The gap between two looks at a running call: four per heartbeat (§4), so a stall is noticed on the poll at or
/// after the bound rather than up to a whole heartbeat late. Read in real time — the watch's clock is the wall clock,
/// and what makes a test fast is a short BOUND (`llm_liveness::stall_seconds`), never a warped clock.
fn poll_wait() -> Duration {
    // Four looks per heartbeat (§4), floored at 50 ms so a one-second test minute still gets its four looks.
    Duration::from_millis((llm_liveness::heartbeat_seconds() * 1000 / 4).max(50))
}

/// The watch's clock: one epoch second, plus a monotonic elapsed from the moment it started.
///
/// Both the probe's stamps and every `now` handed to [`Watch::poll`] come from one of these, so every difference the
/// watch takes is a real elapsed time. Reading the wall clock afresh at each end breaks that: a stamp taken at one
/// instant and a `now` taken at another differ by whatever the wall clock did in between.
#[derive(Debug, Clone)]
pub struct WatchClock {
    origin: std::time::Instant,
    epoch: u64,
}

impl WatchClock {
    fn start() -> WatchClock {
        // One origin for the whole call: `epoch` fixes where the clock started (a shifted start included) and
        // `origin.elapsed()` is what adds to it, so an elapsed time never depends on the wall clock moving mid-call.
        WatchClock { origin: std::time::Instant::now(), epoch: now_secs() }
    }

    /// The current moment in the whole seconds [`Watch`] works in.
    pub fn now(&self) -> u64 {
        self.epoch + self.origin.elapsed().as_secs()
    }
}

/// Milliseconds on the wall clock — what a call's own elapsed times are measured in, so a bound written in seconds
/// is compared against real elapsed time and never against a warped scale.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

/// The same moment in whole seconds — §4's lines spell whole seconds, so everything the watch compares against a
/// bound is divided down to them, and a sub-second remainder never leaks into a sentence. Rounded UP: a byte stamped
/// at 2.5 s read back as "2" would look quieter than it is, and a give-up could name a silence shorter than the
/// bound that caused it.
fn now_secs() -> u64 {
    now_millis().div_ceil(1000)
}

/// Run `send` under §4's watch: heartbeats out of the calling thread every minute, and a give-up when the silence
/// bound passes.
///
/// The returned error on a give-up is [`llm_liveness`]§4's own sentence ("nothing arrived in 5m0s" / "stopped
/// answering after T -- nothing more for 5m0s"), deliberately unprefixed: a caller reading `Err` has to be able to
/// tell a give-up from a refused connect, and `server_leg`'s transport wording names the address instead.
pub fn guarded<T: Send + 'static>(
    liveness: &Liveness,
    probe: &Arc<Probe>,
    send: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    // One clock for the whole call. The worker holds its OWN clone of it (a clock is two numbers), so its stamps and
    // the ticker's `now` share one origin — otherwise the differences the watch takes are not elapsed time.
    let clock = WatchClock::start();
    let worker_clock = clock.clone();
    let worker_probe = probe.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    // Detached on purpose. A §4 give-up must END the call, and the worker may be sitting in a read that nothing can
    // interrupt — joining it (a scoped thread does that on the way out) is what turned a give-up into a hang. So the
    // worker is spawned, owns everything it touches, and is simply left: its answer reaches us if it arrives before
    // the bound, and if it does not, its `send` fails harmlessly once the kernel ends its read.
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let answer = send();
            // One §4 courtesy before handing the answer over: if it finished, whatever it read was proof of life.
            worker_probe.byte_at(worker_clock.now());
            let _ = sender.send(answer);
        })
        .expect("a thread with an owned closure always starts");

    let mut watch = match liveness.streamed {
        true => Watch::streamed(&liveness.step, clock.now()),
        false => Watch::unstreamed(&liveness.step, clock.now()),
    }
    .with_stall_seconds(llm_liveness::stall_seconds());
    loop {
        match receiver.recv_timeout(poll_wait()) {
            Ok(answer) => return answer,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!("{}: the call ended without an answer", liveness.step))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some(secs) = probe.last_byte_seen() {
            watch.byte_arrived(secs);
        }
        let now = clock.now();
        match watch.poll(now) {
            Poll::Idle => {}
            Poll::Heartbeat(line) => crate::ui::window::log_line(&line),
            Poll::GaveUp { log, blame } => {
                crate::ui::window::log_line(&log);
                // §4's give-up ENDS the call here and now: no wait on the worker, no wait on the server. Dropping the
                // receiver leaves the abandoned read to finish on its own and drop its own socket.
                return Err(blame);
            }
        }
    }
}
