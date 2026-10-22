//! §03-shell.md §1's last bullet: "Watchdog: dumps every goroutine's stack after 3 s without a GUI
//! heartbeat (engineering constant)", spelled out by `spec/inventory/shell.md` §"Hang watchdog":
//! "Heartbeat every 200 ms on the GTK thread; a goroutine dumps every goroutine's stack after 3 s of
//! silence to stderr and `<config>/hang-MMDD-HHMMSS.txt` (0600), once per hang."
//!
//! Two halves, and they must not be each other's timer:
//!
//! * [`arm`] stands the GTK-thread heartbeat — a `glib::timeout_add_local` that can only fire while the
//!   main context is turning, which is exactly what makes "a GUI heartbeat" a fact rather than a claim.
//!   [`build_window`](crate::ui::window::build_window) arms it for as long as a window lives.
//! * [`start_watch`] spawns the watcher, called once from [`open_at_start`](crate::ui::launch::open_at_start).
//!   It is a THREAD and not another glib timer because the thing being watched is the main context: a
//!   stalled GUI thread cannot time its own stall.
//!
//! The prototype was Go, so "every goroutine's stack" was a runtime dump. Rust has no such call and this
//! app has one GUI thread, so the equivalent is the one backtrace worth having — captured on the watcher
//! thread, where it still names the process that hung ([`stacks`]).
//!
//! §10 lists the pair only inside its prose row ("… hang watchdog 200 ms / 3 s …", spec/10-parameters.md
//! §4) and gives neither a `P.` id, so both take the bare `machine.` prefix here — the same choice
//! `machine.checkpointPollMs` made over inventing ids §10 does not carry.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::settings::{self, Paths};

/// `machine.hangWatchdogBeatMs` — how often the GTK thread says "still here" (§10's prose "200 ms").
pub const HEARTBEAT: Duration = Duration::from_millis(200);

/// `machine.hangWatchdogSilenceSeconds` — how much silence counts as a hang (§1's "after 3 s").
pub const SILENCE_LIMIT: Duration = Duration::from_millis(3000);

/// What a dump names as stalled. The prototype's "every goroutine" is here the ONE thread whose silence
/// is the hang: the GUI thread.
pub const COMPONENT: &str = "the gtk thread";

thread_local! {
    /// The heartbeat timer this thread armed, if any — see [`arm`]. `RefCell`, not `Cell`: a
    /// `glib::SourceId` is not `Copy`, so it cannot be read out of a `Cell`.
    static ARMED: std::cell::RefCell<Option<glib::SourceId>> = const { std::cell::RefCell::new(None) };
}

static START: OnceLock<Instant> = OnceLock::new();
/// How many heartbeats have landed. Zero means the GUI thread has never turned, which is a start-up, not
/// a hang — see [`start_watch`].
static BEATS: AtomicU64 = AtomicU64::new(0);
/// When the newest heartbeat landed, in [`clock`] milliseconds. Zero until the first beat.
static LAST_BEAT_MS: AtomicU64 = AtomicU64::new(0);
/// Whether the watcher thread exists yet — [`start_watch`] is once per process.
static WATCHED: AtomicBool = AtomicBool::new(false);

/// This process's one clock, from the first thing either side asked it. Marker and checker read the SAME
/// clock: an `Instant` per call would make every `stalled` answer wrong, the reason `ui::shell_beats`
/// keeps one clock for the autosave beat too.
pub fn clock() -> Duration {
    START.get_or_init(Instant::now).elapsed()
}

/// How long the GUI thread has been silent, in whole milliseconds, given the moment and the last beat.
/// Saturating: a beat stamped after `now` (a test's injected clock) reads as no silence, not as a wrap.
pub fn silent_ms(now: Duration, last_beat: Duration) -> u64 {
    now.saturating_sub(last_beat).as_millis() as u64
}

/// Whether `last_beat` is far enough behind `now` to be a hang. Exactly at the limit is NOT yet a hang:
/// the limit is how much silence must have PASSED.
pub fn stalled(now: Duration, last_beat: Duration, limit: Duration) -> bool {
    silent_ms(now, last_beat) > limit.as_millis() as u64
}

/// One dump's whole body, used for BOTH stderr and the file so the two can never disagree about what was
/// seen. It names the stalled component, how long it had been unresponsive and its pid before any stack
/// text: a dump that cannot say which thing hung is no use at 3 a.m.
pub fn dump_body(silent: Duration, pid: u32, stacks: &str) -> String {
    format!(
        "naivepost: {COMPONENT} has not answered for {} (pid {pid})\nstack:\n{stacks}\n",
        settings::seconds_for(silent)
    )
}

/// The stacks a dump carries. Captured on the watcher thread: the hung thread cannot report on itself,
/// and this process has one GUI thread, so this is the whole of the prototype's "every goroutine".
pub fn stacks() -> String {
    std::backtrace::Backtrace::force_capture().to_string()
}

/// Mark one heartbeat: the GUI thread turned at `now_ms`. Called by the timer [`arm`] stands, and by a
/// test that wants to speak for a live thread.
pub fn beat(now_ms: u64) {
    BEATS.fetch_add(1, Ordering::SeqCst);
    LAST_BEAT_MS.store(now_ms, Ordering::SeqCst);
}

/// Heartbeats so far — zero until the GUI thread has turned at least once.
pub fn beats() -> u64 {
    BEATS.load(Ordering::SeqCst)
}

/// Where the GUI thread last said it was, in [`clock`] milliseconds, or `None` before its first beat.
pub fn last_beat() -> Option<Duration> {
    match LAST_BEAT_MS.load(Ordering::SeqCst) {
        0 => None,
        ms => Some(Duration::from_millis(ms)),
    }
}

/// Stand the GTK thread's heartbeat: one `glib::timeout_add_local` per thread, so building a second
/// window does not double the beat. Armed from `build_window`, for as long as a window lives.
pub fn arm() {
    ARMED.with(|armed| {
        let mut armed = armed.borrow_mut();
        if armed.is_some() {
            return;
        }
        let id = glib::timeout_add_local(HEARTBEAT, || {
            beat(clock().as_millis() as u64);
            glib::ControlFlow::Continue
        });
        *armed = Some(id);
    });
}

/// Spawn the watcher: after `limit` of GUI silence, dump once — to stderr and to
/// `<config>/hang-MMDD-HHMMSS.txt` (0600, §7) — and stay quiet until the thread beats again.
///
/// Called once by the launch, beside the config folder its dumps land in. A missing config folder costs
/// the FILE only: stderr still gets the dump, because losing the record entirely over one unwritable
/// directory is worse than a silent log line (principle: failure is specific and local).
///
/// No dump before the first heartbeat: an app that has not finished starting up is not hung, and a dump
/// saying so would be the first thing a user ever saw from this code.
pub fn start_watch(paths: Option<Paths>, limit: Duration) {
    if WATCHED.swap(true, Ordering::SeqCst) {
        return;
    }
    // Four looks per heartbeat, the ratio §4's LLM watch uses: a stall is noticed on the poll at or
    // after the bound rather than up to a whole heartbeat late.
    let poll = (HEARTBEAT / 4).max(Duration::from_millis(10));
    std::thread::spawn(move || {
        let mut dumped = false;
        let mut beats_at_dump = 0;
        loop {
            std::thread::sleep(poll);
            let beats = beats();
            if beats == 0 {
                continue;
            }
            let last = last_beat().unwrap_or_default();
            let now = clock();
            if beats != beats_at_dump {
                // The thread beat again since the last dump: this hang is over, and the next one earns
                // its own file ("once per hang" is per hang, not once per run).
                dumped = false;
            }
            if dumped || !stalled(now, last, limit) {
                continue;
            }
            let silent = now - last;
            let body = dump_body(silent, std::process::id(), &stacks());
            eprintln!("{body}");
            dumped = true;
            beats_at_dump = beats;
            let Some(paths) = paths.as_ref() else {
                continue;
            };
            let stamp = crate::machine_store::local_stamp();
            // One dump per hang AND per second: a stamp already on disk means another file would only
            // bury the first, so nothing more is written (§7's one-per-hang).
            if paths.has_hang_dump(&stamp) {
                continue;
            }
            // A failure to record is never a failed step: the stderr copy above already happened.
            let _ = settings::write_stack_dump(paths, &body, &stamp);
        }
    });
}
