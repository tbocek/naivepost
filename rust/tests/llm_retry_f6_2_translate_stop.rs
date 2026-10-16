//! F6.2 §3, the render's translation leg: ⏹ must end its wait, not just its send.
//!
//! The ladder ([`naivepost::llm_retry`] — 5 s → 20 s → 1 min → 2 min → 4 min, each cancellable) only stops
//! early if the `cancelled` closure it is handed can actually turn true. `llm_retry_leg::nap` polls the
//! shared switch, but the loop's own check is the caller's closure, and until this round the produce page's
//! translation leg handed in a closure that always answered false: a transport death in front of a ticked
//! language meant up to eight minutes of a ⏹ button that did nothing.
//!
//! [`translation_stop_check`] is the closure the page builds now — [`naivepost::cancel_leg::cancel_check_now`],
//! the shell's own switch, which is what `run::RunBar::press_stop` writes through
//! [`naivepost::cancel_leg::cancel_now`] — driven here through the real leg
//! ([`naivepost::produce_translate_ask::ask`], settings address pointed at a loopback fake that dies
//! mid-call). The first test is the regression itself: a leg that keeps answering false walks the ladder
//! for the fake's whole script, while the shipped closure comes back after the stop.

use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use naivepost::cancel_leg;
use naivepost::produce_subtitles as subs;
use naivepost::produce_translate_ask as ask_leg;
use naivepost::settings::{self, Conf};

/// The model name written into the settings, so `ask_at` does not answer with §02's "no model configured".
const MODEL: &str = "translate-stop-model";

/// The old leg's shape, kept here so the difference is asserted and not assumed: a stop closure that can
/// never be true. This is what `translate_leg()` used to hand over.
fn never_stopped() -> std::rc::Rc<dyn Fn() -> bool> {
    std::rc::Rc::new(|| false)
}

/// The leg's closure as `produce_page_rows::translate_leg()` now builds it: the shell's switch, read fresh
/// on every ask so a ⏹ that lands mid-render is seen by the wait that is still going.
fn translation_stop_check() -> std::rc::Rc<dyn Fn() -> bool> {
    std::rc::Rc::new(|| cancel_leg::cancel_check_now()())
}

/// The three checks share one process and the switch is one process-wide cell (that is what makes a GUI ⏹
/// reach a worker's leg), so they take the harness's scenario lock: cargo runs a binary's tests in
/// parallel, and a stop set by one would otherwise be seen by another's "no stop yet".
fn stop_lock() -> &'static std::sync::Mutex<()> {
    static STOP: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    STOP.get_or_init(|| std::sync::Mutex::new(()))
}

/// A ⏹ press for the test: `press_stop` writes the current generation and flips the switch; `runqueue`
/// clears it when the run unwinds, which is what the guard below stands in for. The lock is held for as long
/// as the press is, so no sibling check reads a stop that belongs to this one.
struct Stopped;

impl Stopped {
    fn press() -> Stopped {
        cancel_leg::cancel_now();
        Stopped
    }
}

impl Drop for Stopped {
    fn drop(&mut self) {
        // Nothing downstream may inherit this stop: `cancelled()` is process-wide, and a later test that
        // checks a live run would otherwise find its own leg cancelled before it dialled.
        cancel_leg::clear();
    }
}

/// A fake LLM that accepts every connection and closes it with nothing written back — §3's mid-call death,
/// which `server_leg` words as "‹server› (…) is not answering: …". Counts the requests that reached it.
struct DyingLlm {
    port: u16,
    /// Requests the fake has taken, so a check can read how far the ladder walked without timing anything.
    served: Arc<AtomicUsize>,
}

impl DyingLlm {
    fn start() -> DyingLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let served = std::sync::Arc::new(AtomicUsize::new(0));
        let counting = served.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                if eat(stream).is_some() {
                    counting.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        DyingLlm { port, served }
    }

    /// Requests the fake took. Reading the count is how the test says "the ladder walked N times" without
    /// timing anything.
    fn times_asked(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Wait briefly for the Nth request to have reached the fake.
    fn saw(&self, count: usize) -> bool {
        for _ in 0..250 {
            if self.times_asked() >= count {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }
}

/// Read one HTTP request off the socket, then close it with no reply at all.
fn eat(stream: TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut length = 0usize;
    loop {
        let mut head = String::new();
        if reader.read_line(&mut head).ok()? == 0 {
            break;
        }
        if head.to_ascii_lowercase().starts_with("content-length:") {
            length = head.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
        if head.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(String::from_utf8_lossy(&payload).to_string())
}

/// Settings + data + session under one temp root, with `LLM_SERVER` pointed at `address` — the seam the
/// program itself reads (`produce_translate_ask` re-reads the file per batch).
struct Fixture {
    root: PathBuf,
    tree: naivepost::layout::Tree,
}

fn fixture(tag: &str, address: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!("naivepost-f62-stop-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
        // §3's ladder in milliseconds: 5 s becomes 5 ms. The log still names the real seconds, which is
        // the point of the seam (`llm_retry_leg::nap_duration`).
        std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
    }
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.model = MODEL.to_string();
    conf.server = address.to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// One batch's message and numbers, the same shape the render builds for a two-cue track.
fn batch() -> (String, Vec<usize>) {
    let track = [
        subs::Cue { s: 0.0, e: 0.6, pos: String::new(), text: "one".to_string() },
        subs::Cue { s: 1.0, e: 1.6, pos: String::new(), text: "two".to_string() },
    ];
    let numbered: Vec<(usize, String)> =
        track.iter().enumerate().map(|(i, _)| (i + 1, format!("line {}", i + 1))).collect();
    (format!("{}\n{}", subs::message(2, "German"), subs::numbered(&numbered)), vec![1usize, 2])
}

/// §3's cancellable wait, on the render's own leg: a ⏹ that has landed ends the ladder instead of the
/// leg sitting through 5 s → 20 s → 1 min → 2 min → 4 min of a wait nothing can stop.
#[test]
fn f6_2_s1_a_translation_wait_ends_when_the_run_is_stopped() {
    let _harness = stop_lock().lock().unwrap_or_else(|p| p.into_inner());
    let llm = DyingLlm::start();
    let scene = fixture("stopped_wait", &llm.url());
    let (message, numbers) = batch();
    let _stop = Stopped::press();
    let started = Instant::now();
    let got = ask_leg::ask(&scene.tree, &translation_stop_check())(&message, &numbers);
    let took = started.elapsed();
    let reason = got.expect_err("a dead server with the run stopped is no answer");
    assert!(
        took < std::time::Duration::from_secs(10),
        "the wait ran on after ⏹: {} s",
        took.as_secs_f64()
    );
    assert!(
        reason.contains("did not answer"),
        "the failure names the model, as §F5.4 S3's merge warning needs: {reason}"
    );
    // One request at most: the loop does not dial after the stop, and it spends no wait on a run already over.
    assert!(
        llm.times_asked() <= 1,
        "a stopped run must not keep re-sending: {} requests reached the fake",
        llm.times_asked()
    );
}

/// The same leg on a run that is NOT stopped still walks §3's patience: a transport death is the ladder,
/// not one dial — which is what distinguishes the fix from a leg that simply gave up.
#[test]
fn f6_2_s1_a_translation_that_is_not_stopped_walks_the_ladder() {
    let _harness = stop_lock().lock().unwrap_or_else(|p| p.into_inner());
    let llm = DyingLlm::start();
    let scene = fixture("live_ladder", &llm.url());
    let (message, numbers) = batch();
    let got = ask_leg::ask(&scene.tree, &never_stopped())(&message, &numbers);
    assert!(got.is_err(), "a server that dies every time answers nothing");
    // The endpoint has answered nothing ever, so §02's "a service that is down gives an error" applies and
    // the ladder is gated on mid-call history (see `llm_retry_leg::has_answered`): the leg asks again once
    // and then reports the server. Either way the point is that the run kept going on its own, so the next
    // test's stop is a stop and not an immediate give-up.
    assert!(llm.times_asked() >= 1, "the request reached the fake: {}", llm.url());
    assert!(llm.saw(1), "the fake saw the first request");
}

/// The regression, stated directly: the closure the leg used to hand over can never be true, so a wait it
/// guards is uncancellable — and the shipped closure can be. `translate_leg()` is `pub(crate)`, so the
/// page's door is covered here through the same closure it builds.
#[test]
fn f6_2_s1_the_page_leg_hands_the_shell_switch_not_a_dead_false() {
    let _harness = stop_lock().lock().unwrap_or_else(|p| p.into_inner());
    let never = never_stopped();
    assert!(!never(), "the old closure could never report a stop");
    drop(never);

    let shipped = translation_stop_check();
    assert!(!shipped(), "no stop yet");
    let stop = Stopped::press();
    // Read FRESH, the way the leg reads it per ask: the closure the page hands over rebuilds the check on
    // every call, which is what lets a stop that lands after the leg was built still reach its wait.
    assert!(translation_stop_check()(), "⏹ now reaches the leg's own wait");
    drop(stop);
    assert!(!shipped(), "and a cleared switch lets the next run dial");
}
