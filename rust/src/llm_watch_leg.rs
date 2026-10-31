//! F6.3 §4's live half: the stall watch put around a real request.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::llm_liveness::{self, Poll, Watch};

/// What the caller says this call is, for the log's `>>> <step>` prefix, and whether §4's silence rule applies.
#[derive(Debug, Clone)]
pub struct Liveness {
    pub step: String,
    pub streamed: bool,
}

impl Liveness {
    /// A streamed chat call: §4's stall rule applies.
    pub fn streamed(step: &str) -> Liveness {
        Liveness {
            step: step.to_string(),
            streamed: true,
        }
    }
}

/// The thing the reader pings and the watch reads: when did a byte last arrive, and did any ever?
#[derive(Debug, Default)]
pub struct Probe {
    /// Arrival times in the same epoch seconds the watch works in, which is what makes them comparable.
    last_byte: AtomicU64,
    ever_arrived: AtomicBool,
    gave_up: AtomicBool,
}

impl Probe {
    pub fn new() -> Arc<Probe> {
        Arc::new(Probe::default())
    }

    /// Something came off the wire. Any byte counts — §4 says so outright, keep-alives included — and the caller
    /// does not have to know what the bytes meant to make this call.
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

    /// Say that the watch ended this call. Written by [`guarded`]'s give-up arm and read by the leg that
    /// filed the request, so the row can say `stalled` rather than `error nothing arrived in 5m0s`.
    pub fn note_gave_up(&self) {
        self.gave_up.store(true, Ordering::Relaxed);
    }

    /// Whether the watch ended this call for silence.
    pub fn gave_up(&self) -> bool {
        self.gave_up.load(Ordering::Relaxed)
    }
}

fn poll_wait() -> Duration {
    // Four looks per heartbeat (§4), floored so a watch still gets its four looks inside a short test minute.
    Duration::from_millis((llm_liveness::heartbeat_seconds() * 1000 / 4).max(50))
}

/// The watch's clock: one epoch second, plus a monotonic elapsed from the moment it started.
#[derive(Debug, Clone)]
pub struct WatchClock {
    origin: std::time::Instant,
    epoch: u64,
}

impl WatchClock {
    fn start() -> WatchClock {
        // One origin for the whole call: `epoch` fixes where the clock started (a shifted start included) and
        // `origin.elapsed()` is what adds to it, so an elapsed time never depends on the wall clock moving mid-call.
        WatchClock {
            origin: std::time::Instant::now(),
            epoch: now_secs(),
        }
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

fn now_secs() -> u64 {
    now_millis().div_ceil(1000)
}

/// Run `send` under §4's watch: heartbeats out of the calling thread every minute, and a give-up when the silence
/// bound passes.
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
                return Err(format!(
                    "{}: the call ended without an answer",
                    liveness.step
                ))
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
                // Tell the leg that filed this request that THIS is why it ended (§10's `stalled`).
                probe.note_gave_up();
                // §7's watchdog dump: the give-up is exactly the stall a `hang-*.txt` exists to
                // record, so it is written here rather than by a second watcher nobody wired up.
                // Failing to write it changes nothing about the call's ending (principle 8).
                crate::machine_store::note_hang(
                    &liveness.step,
                    std::time::Duration::from_secs(watch.quiet(now)),
                );
                // §4's give-up ENDS the call here and now: no wait on the worker, no wait on the server. Dropping the
                // receiver leaves the abandoned read to finish on its own and drop its own socket.
                return Err(blame);
            }
        }
    }
}
