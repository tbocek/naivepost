//! §03-shell.md §1's last bullet: "Watchdog: dumps every goroutine's stack after 3 s without a GUI
//! heartbeat (engineering constant)", spelled out by `spec/inventory/shell.md` §"Hang watchdog":
//! "Heartbeat every 200 ms on the GTK thread; a goroutine dumps every goroutine's stack after 3 s of
//! silence to stderr and `<config>/hang-MMDD-HHMMSS.txt` (0600), once per hang."

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::settings::{self, Paths};

/// `machine.hangWatchdogBeatMs` — how often the GTK thread says "still here" (§10's prose "200 ms").
pub const HEARTBEAT: Duration = Duration::from_millis(200);

/// `machine.hangWatchdogSilenceSeconds` — how much silence counts as a hang (§1's "after 3 s").
pub const SILENCE_LIMIT: Duration = Duration::from_millis(3000);

pub const COMPONENT: &str = "the gtk thread";

thread_local! {
    static ARMED: std::cell::RefCell<Option<glib::SourceId>> = const { std::cell::RefCell::new(None) };
}

static START: OnceLock<Instant> = OnceLock::new();
static BEATS: AtomicU64 = AtomicU64::new(0);
/// When the newest heartbeat landed, in [`clock`] milliseconds. Zero until the first beat.
static LAST_BEAT_MS: AtomicU64 = AtomicU64::new(0);
/// Whether the watcher thread exists yet — [`start_watch`] is once per process.
static WATCHED: AtomicBool = AtomicBool::new(false);

pub fn clock() -> Duration {
    START.get_or_init(Instant::now).elapsed()
}

pub fn silent_ms(now: Duration, last_beat: Duration) -> u64 {
    now.saturating_sub(last_beat).as_millis() as u64
}

pub fn stalled(now: Duration, last_beat: Duration, limit: Duration) -> bool {
    silent_ms(now, last_beat) > limit.as_millis() as u64
}

pub fn dump_body(silent: Duration, pid: u32, stacks: &str) -> String {
    format!(
        "naivepost: {COMPONENT} has not answered for {} (pid {pid})\nstack:\n{stacks}\n",
        settings::seconds_for(silent)
    )
}

/// The stacks a dump carries: EVERY thread of this process, the GTK thread first among equals.
pub fn stacks() -> String {
    let pid = std::process::id();
    let mut out = threads_from_proc(pid);
    match eu_stack(pid) {
        Ok(frames) => out.push_str(&frames),
        Err(why) => out.push_str(&format!("(no frames: {why})\n")),
    }
    out
}

fn threads_from_proc(pid: u32) -> String {
    let task = format!("/proc/{pid}/task");
    let Ok(entries) = std::fs::read_dir(&task) else {
        return format!("(no thread list: {task} is not readable)\n");
    };
    let mut tids: Vec<u32> = entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .collect();
    tids.sort_unstable();
    let read = |tid: u32, file: &str| {
        std::fs::read_to_string(format!("{task}/{tid}/{file}"))
            .map(|text| text.trim().to_string())
            .unwrap_or_default()
    };
    let mut out = String::new();
    for tid in tids {
        // `stat` is `<tid> (<comm>) <state> …`; the comm may hold spaces, so the state is read after the
        // LAST closing parenthesis.
        let stat = read(tid, "stat");
        let state = stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .unwrap_or("?");
        let wchan = match read(tid, "wchan") {
            w if w.is_empty() || w == "0" => "user space".to_string(),
            w => w,
        };
        let gtk = if tid == pid { " <- the gtk thread" } else { "" };
        out.push_str(&format!(
            "tid {tid} ({}) {state} waiting in {wchan}{gtk}\n",
            read(tid, "comm")
        ));
    }
    out
}

/// How long `eu-stack` may take before the dump goes out without its frames.
const EU_STACK_WAIT: Duration = Duration::from_secs(10);

fn eu_stack(pid: u32) -> Result<String, String> {
    // SAFETY: prctl with integer arguments only changes this process's own ptrace permission.
    unsafe { libc::prctl(libc::PR_SET_PTRACER, libc::PR_SET_PTRACER_ANY, 0, 0, 0) };
    let frames = run_eu_stack(pid);
    // SAFETY: as above; 0 takes the permission back.
    unsafe { libc::prctl(libc::PR_SET_PTRACER, 0, 0, 0, 0) };
    frames
}

fn run_eu_stack(pid: u32) -> Result<String, String> {
    let mut child = crate::subprocess::spawn_piped("eu-stack", &["-p", &pid.to_string()])
        .map_err(|why| format!("eu-stack (elfutils) did not start: {why}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < EU_STACK_WAIT => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "eu-stack did not finish in {}s",
                    EU_STACK_WAIT.as_secs()
                ));
            }
            Err(why) => return Err(format!("eu-stack: {why}")),
        }
    }
    let out = child.wait_with_output().map_err(|why| why.to_string())?;
    let frames = String::from_utf8_lossy(&out.stdout).into_owned();
    if frames.trim().is_empty() {
        return Err(format!(
            "eu-stack printed nothing: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(frames)
}

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
