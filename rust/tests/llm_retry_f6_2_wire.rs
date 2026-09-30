//! F6.2 §3 over the wire: the patience every chat ask rides, dialled for real.
//!
//! `tests/llm_retries_f6_2.rs` proves §3's rules against the pure functions with two closures. What it cannot
//! prove is that a chat call ever rides them — and until this round none did: every ask dialled once and failed.
//! These tests point the SETTINGS address (the seam the program itself reads) at a loopback fake and drive the
//! very door the flows call, [`naivepost::llm_retry_leg`], so what is asserted is which requests went out, in
//! what order, and what the log said between them.
//!
//! The waits are observed through `chat_on`'s `sleep` argument — a closure that RECORDS the seconds instead of
//! sleeping — so §3's real ladder `[5, 20, 60, 120, 240]` is checked in milliseconds rather than in eight
//! minutes. Returning `false` from it is ⏹ landing, the same signal `nap` gives the loop when the switch flips.
//!
//! One scenario per process-wide fixture: `cwd`, `XDG_CONFIG_HOME` and the settings file belong to the process,
//! so each check owns them behind [`scenario_lock`] for as long as its fixture lives — which is also why the
//! scenarios never overlap even though cargo runs a binary's tests in parallel.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::server_leg;
use naivepost::services::{self, Endpoint};
use naivepost::settings::{self, Conf};

const MODEL: &str = "retry-model";
/// The step name these checks hand the door, so §3's `<step>` prefix is recognisable in the log lines.
const STEP: &str = "f62test";

// ---- the fake LLM -------------------------------------------------------------------------------

/// One scripted answer.
#[derive(Clone)]
struct Reply {
    status: u16,
    body: String,
}

impl Reply {
    fn ok(content: &str) -> Reply {
        Reply { status: 200, body: good(content) }
    }

    /// A mid-call death: the endpoint was asked, the connection is cut with nothing written back. §3's first-listed
    /// failure, and what a container killed while replying looks like from the client.
    fn died() -> Reply {
        Reply { status: 0, body: String::new() }
    }

    /// A status carrying an `error.message`, the shape llama.cpp-family servers answer with.
    fn error(status: u16, message: &str) -> Reply {
        Reply { status, body: serde_json::json!({ "error": { "message": message } }).to_string() }
    }
}

fn good(content: &str) -> String {
    serde_json::json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// The fake LLM: answers each request from a script, handing each request's body back to the test.
struct FakeLlm {
    port: u16,
    received: Receiver<String>,
}

impl FakeLlm {
    /// `Reply { status: 0, .. }` in the script means "accept the connection and close it without answering".
    fn start(replies: Vec<Reply>) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(Script {
            replies,
            // Requests read off the wire so far. The entry a request is answered from is chosen by THIS count, not
            // by how many connections were accepted: ureq can open and abandon a socket, and a dying attempt still
            // sends its body, so counting accepts would let one call eat two entries.
            asked: 0,
        }));
        // Served IN ORDER on one thread, each request answered before the next is accepted: a retried call must
        // see script entry 2 because it is the second request, not because its reader won a race.
        let held_script = script.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sender = sender.clone();
                if let Some(body) = serve(stream, &held_script) {
                    let _ = sender.send(body);
                }
            }
        });
        std::mem::forget(thread);
        FakeLlm { port, received }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn all(&self, wait: std::time::Duration) -> Vec<String> {
        let deadline = std::time::Instant::now() + wait;
        let mut seen = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.received.recv_timeout(left) {
                Ok(body) => seen.push(body),
                Err(_) => break,
            }
            if seen.len() > 8 {
                break;
            }
        }
        seen
    }
}

/// The script plus how many requests have actually been read off the wire.
struct Script {
    replies: Vec<Reply>,
    asked: usize,
}

fn serve(mut stream: TcpStream, script: &Mutex<Script>) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if line.to_ascii_lowercase().starts_with("content-length:") {
            length = line.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    // The request is on the wire now: claim its script entry and answer from it.
    let reply = {
        let mut state = script.lock().expect("script lock");
        let index = state.asked.min(state.replies.len().saturating_sub(1));
        state.asked += 1;
        state.replies[index].clone()
    };
    // §3's mid-call death over a real socket: the request was read off the wire and then the connection is cut
    // with NOTHING written back. ureq fails it as `io: Peer disconnected`, which `server_leg` words as "‹server›
    // (‹url›) is not answering: …" — the wording §3 lists first among the deaths.
    if reply.status == 0 {
        // A scripted death answers NOTHING: the socket is closed and ureq reports the call as dead. §3's ladder
        // then turns on whether this endpoint has answered before, which is what each scenario arranges for itself.
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return Some(String::from_utf8_lossy(&payload).to_string());
    }
    let reason = if reply.status == 200 { "OK" } else { "Server Error" };
    let framed = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reply.body.len(),
        reply.body
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(String::from_utf8_lossy(&payload).to_string())
}

// ---- the scenario -------------------------------------------------------------------------------

/// Held while a scenario re-points the process-wide settings env.
fn scenario_lock() -> &'static Mutex<()> {
    static SCENARIO: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| Mutex::new(()))
}

struct Fixture {
    root: PathBuf,
    tree: naivepost::layout::Tree,
    endpoint: Endpoint,
    _scenario: std::sync::MutexGuard<'static, ()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Settings + data + project under one temp root, with the LLM address pointed at `address`.
fn fixture(tag: &str, address: &str) -> Fixture {
    let scenario = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-f62wire-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.model = MODEL.to_string();
    conf.server = address.to_string();
    // No drawing server: left empty it falls back to the loopback default and a poll would sit in that connect
    // timeout for the whole binary.
    conf.sd_server = "http://127.0.0.1:1".to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    // Resolved exactly as a flow resolves it, from the settings just written.
    let endpoint = services::llm_endpoint(&conf);
    Fixture { root, tree, endpoint, _scenario: scenario }
}

/// The lines the door logged. `Vec` behind a `Mutex` because `log` reaches the sink as a shared `&dyn Fn`.
#[derive(Default)]
struct Sink(Arc<Mutex<Vec<String>>>);

impl Sink {
    fn log(&self) -> impl Fn(&str) {
        let held = self.0.clone();
        move |line: &str| held.lock().expect("sink lock").push(line.to_string())
    }

    fn lines(&self) -> Vec<String> {
        self.0.lock().expect("sink lock").clone()
    }

    /// §3's death line, `!!! <step>: the server went away mid-call (…) -- waiting T and asking again (i of 5)`.
    fn gone_lines(&self) -> Vec<String> {
        self.lines()
            .into_iter()
            .filter(|line| line.contains("the server went away mid-call"))
            .collect()
    }

    /// The short retry's line — the branch §3 leaves unworded and `llm_retry::waited_log` spells anyway.
    fn short_lines(&self) -> Vec<String> {
        self.lines()
            .into_iter()
            .filter(|line| line.contains("asked again after 2 s"))
            .collect()
    }
}

/// Waits recorded instead of slept, so the ladder is observable without living through it.
#[derive(Default)]
struct Waits(Arc<Mutex<Vec<u64>>>);

impl Waits {
    /// Records every wait and lets it "happen".
    fn recorder(&self) -> impl Fn(u64) -> bool {
        let held = self.0.clone();
        move |seconds: u64| {
            held.lock().expect("waits lock").push(seconds);
            true
        }
    }

    /// Records waits and reports the `stop_at`-th one as stopped — ⏹ landing during that wait, which is what
    /// `nap` returns false for.
    fn stopping_at(&self, stop_at: usize) -> impl Fn(u64) -> bool {
        let held = self.0.clone();
        move |seconds: u64| {
            let mut seen = held.lock().expect("waits lock");
            seen.push(seconds);
            seen.len() < stop_at
        }
    }

    fn seconds(&self) -> Vec<u64> {
        self.0.lock().expect("waits lock").clone()
    }
}

/// A cancel context that is never cancelled: these checks drive the door directly, and the ⏹ half is exercised by
/// `sleep` returning false — the same signal `nap` gives the loop.
fn never_cancelled() -> impl Fn() -> bool {
    || false
}

fn body(text: &str) -> String {
    format!(r#"{{"model":"{MODEL}","messages":[{{"role":"user","content":"{text}"}}]}}"#)
}

/// One call through the door, with the waits and the log observed. `endpoint` is a parameter rather than
/// `scene.endpoint` because a scenario may drive the door at a SECOND fake without creating a second fixture:
/// [`fixture`] holds the exclusive [`scenario_lock`] for its whole life, so a scenario that called it twice would
/// deadlock its own thread.
fn ask(
    scene: &Fixture,
    endpoint: &Endpoint,
    text: &str,
    sink: &Sink,
    sleep: &dyn Fn(u64) -> bool,
) -> Result<server_leg::Sent, String> {
    let base = server_leg::base_row(services::Server::Llm, MODEL, "chat", "", STEP, "");
    naivepost::llm_retry_leg::chat_on(
        STEP,
        &scene.tree,
        &base,
        endpoint,
        &body(text),
        &never_cancelled(),
        &sink.log(),
        sleep,
    )
}

// ---- s1: the transport death ---------------------------------------------------------------------
//
// The scenarios take turns behind [`scenario_lock`] for as long as a fixture lives: two of them cannot point the
// ONE process-wide LLM address at their own port at the same time. Whether an endpoint has EVER answered is what
// decides §3's two patiences (`llm_retry_leg` gates its ladder on it), so each scenario asks its OWN fake rather
// than trusting a process-wide history another scenario may have written.

/// A call that dies IN TRANSIT to a server that had answered before is a mid-call death: §3's ladder is walked in
/// full, the IDENTICAL request going out after every rung, and only then does the step fail. The five waits are
/// the whole rule — they land past a container restart and a weight load — so they are asserted as the exact
/// sequence, not as "some waiting happened".
#[test]
fn f6_2_s1_a_mid_call_death_walks_the_ladder_and_asks_the_same_request_again() {
    // One listener, one scenario: the first call answers, every later one is cut with nothing written back — eof
    // mid-reply, §3's first-listed death and what a watchdog-killed container looks like from the client.
    let llm = FakeLlm::start(vec![Reply::died()]);
    let scene = fixture("ladder", &llm.url());
    let sink = Sink::default();
    let waits = Waits::default();
    // Every attempt dies. The endpoint's mid-call history is what §3 gates the ladder on, and `chat_on` records it
    // itself when a call answers — so this scenario hands its fresh port that history up front rather than earning
    // it with a call that would have to be answered by the same fake.
    naivepost::llm_retry_leg::note_answer(&scene.endpoint.url);
    let outcome = ask(&scene, &scene.endpoint, "same question", &sink, &waits.recorder());
    assert!(outcome.is_err(), "five deaths and the step fails: {outcome:?}");
    assert!(outcome.is_err(), "five deaths and the step fails: {outcome:?}");
    assert_eq!(waits.seconds(), vec![5, 20, 60, 120, 240], "§3's ladder in full, in order, no sixth wait");
    let gone = sink.gone_lines();
    assert_eq!(gone.len(), 5, "one line per rung: {gone:?}");
    assert!(
        gone[0].starts_with(&format!("!!! {STEP}: the server went away mid-call ("))
            && gone[0].contains("-- waiting 5 s and asking again (1 of 5)"),
        "§3's line verbatim: {}",
        gone[0]
    );
    assert!(
        gone[4].contains("-- waiting 4 min and asking again (5 of 5)"),
        "the last rung says 4 min and 5 of 5: {}",
        gone[4]
    );
    // Every attempt reached the wire, including the ones the server died on: the request was sent and read back
    // before the socket closed, so "asked again" counts them — seven bodies for six attempts (the first dial,
    // §3's five retries, and one more ask that got out before the ladder ran out), all carrying the identical body.
    let asked = llm.all(std::time::Duration::from_millis(800));
    assert_eq!(asked.len(), 6, "six asks reached the wire: {}", asked.len());
    assert!(asked.windows(2).all(|pair| pair[0] == pair[1]), "every retry sent the identical body");
}

/// The ladder ends — and ⏹ ends it sooner. A four-minute wait nobody can interrupt is a button that does nothing,
/// so the run stops mid-ladder and dials nobody after that.
#[test]
fn f6_2_s1_the_ladder_ends_and_a_stop_ends_it_early() {
    // A MID-CALL death, not a dead port: the ladder only applies to an endpoint that has answered (§3's "went away
    // MID-CALL"), so this fake answers once and then closes every later connection. Its own fresh port keeps the
    // process-wide answered-endpoint table from lending another scenario's history to it.
    let llm = FakeLlm::start(vec![Reply::ok("one"), Reply::died()]);
    let scene = fixture("stopped", &llm.url());
    // The warm-up call earns the mid-call history through the door itself (§3's ladder is for a server that went
    // away MID-CALL); nothing about it is asserted.
    let warm = Sink::default();
    let warm_waits = Waits::default();
    let _ = ask(&scene, &scene.endpoint, "warm", &warm, &warm_waits.recorder());
    let sink = Sink::default();
    let waits = Waits::default();
    let outcome = ask(&scene, &scene.endpoint, "q", &sink, &waits.stopping_at(3));
    assert!(outcome.is_err(), "a stopped run gets no answer");
    assert_eq!(waits.seconds(), vec![5, 20, 60], "it stopped on the third rung, not the fifth");
    let why = outcome.unwrap_err();
    assert!(why.contains("cancelled"), "the reason says the run was stopped, not that the server broke: {why}");
    assert_eq!(sink.gone_lines().len(), 3, "one line per rung it got through: {:?}", sink.gone_lines());
}

/// A server this app has NEVER got an answer from is not "gone away mid-call": §02-services#1's down service
/// gives an error, and sitting 5 s → 4 min in front of a box nobody started would hold a run for eight minutes
/// over nothing. So it takes §3's short patience — the identical request once more after 2 s — and then reports.
#[test]
fn f6_2_s1_a_server_that_never_answered_is_not_waited_through() {
    // Nothing listens at this port at all: refused from the first contact, with no answer ever.
    let listener = TcpListener::bind("127.0.0.1:0").expect("hand out a dead port");
    let dead = format!("http://127.0.0.1:{}", listener.local_addr().expect("a local address").port());
    drop(listener);
    let scene = fixture("never", &dead);
    let sink = Sink::default();
    let waits = Waits::default();
    let outcome = ask(&scene, &scene.endpoint, "q", &sink, &waits.recorder());
    let why = outcome.expect_err("a server that never answers still does not answer");
    assert_eq!(waits.seconds(), vec![2], "one short retry, no ladder: {:?}", waits.seconds());
    assert!(sink.gone_lines().is_empty(), "no mid-call claim for a server never in one: {:?}", sink.gone_lines());
    assert_eq!(sink.short_lines().len(), 1, "the short retry is said once: {:?}", sink.short_lines());
    assert!(why.contains("not answering"), "the reason names the server: {why}");
}

// ---- s2: anything else gets one short retry ------------------------------------------------------

/// A 4xx is the server ANSWERING, so §3 allows exactly one more ask, after 2 s, and then the step fails. Never a
/// third: the same request gets the same answer, which is what makes the ladder wrong for this branch.
#[test]
fn f6_2_s2_a_400_is_asked_again_once_after_2_s_then_stops() {
    let llm = FakeLlm::start(vec![Reply::error(400, "bad schema here")]);
    let scene = fixture("four-hundred", &llm.url());
    let sink = Sink::default();
    let waits = Waits::default();
    let outcome = ask(&scene, &scene.endpoint, "q", &sink, &waits.recorder());
    assert!(outcome.is_err(), "a 400 that keeps answering 400 fails the step");
    assert_eq!(waits.seconds(), vec![2], "the one short retry, and only that one");
    assert_eq!(sink.short_lines().len(), 1, "said once: {:?}", sink.short_lines());
    assert!(sink.gone_lines().is_empty(), "a status is never \"the server went away\": {:?}", sink.gone_lines());
    let asked = llm.all(std::time::Duration::from_millis(600));
    assert_eq!(asked.len(), 2, "two asks, never a third: {}", asked.len());
    assert_eq!(asked[0], asked[1], "the retry sent the identical body");
}

/// The count §3's short patience allows, counted where it can be counted exactly: a server that never answers is
/// dialled TWICE — the ask and its one retry after 2 s — and never a third. `tests/f1_10_s3_a_down_llm_is_reported_and_every_word_stands.rs`
/// checks the same bound through `requests.tsv` from a whole flow; this pins it at the door itself, with no caller
/// counters in the way.
#[test]
fn f6_2_s2_a_down_server_is_asked_exactly_twice_over_the_wire() {
    // A live listener that answers NOTHING: every attempt sends its request and gets an eof, so the number of
    // bodies on the wire IS the number of asks (a dead port would deliver none and prove nothing).
    let llm = FakeLlm::start(vec![Reply::died()]);
    let scene = fixture("twice", &llm.url());
    // No mid-call history for this fresh port: §3 files it as a box nobody started, which is the short-patience
    // branch rather than the ladder.
    let sink = Sink::default();
    let waits = Waits::default();
    let outcome = ask(&scene, &scene.endpoint, "q", &sink, &waits.recorder());
    let why = outcome.expect_err("a server that dies on every ask leaves the step without an answer");
    assert_eq!(waits.seconds(), vec![2], "one wait of 2 s, and no ladder: {:?}", waits.seconds());
    assert_eq!(sink.short_lines().len(), 1, "the short retry is said once: {:?}", sink.short_lines());
    assert!(sink.gone_lines().is_empty(), "no death line for the short branch: {:?}", sink.gone_lines());
    assert!(why.contains("not answering"), "the reason names the server: {why}");
    let asked = llm.all(std::time::Duration::from_millis(800));
    assert_eq!(asked.len(), 2, "two asks over the wire, never a third: {}", asked.len());
}

// ---- s3: the new rule — a 5xx naming a device ----------------------------------------------------

/// §3's NEW rule, asserted through the door rather than only through the classifier: a 5xx whose body names a
/// device or memory failure backs off like a transport death, because the box has no room YET and a wait is what
/// fixes that; a plain 500 is an answer and keeps the 2 s retry.
#[test]
fn f6_2_s3_a_500_naming_a_device_backs_off_and_a_plain_500_does_not() {
    // ONE fixture for both halves: a second [`fixture`] call while this one is alive would wait forever on the
    // scenario lock it holds. The plain-500 half therefore drives the door at a SECOND fake by handing its address
    // to `chat_on` directly — which the door accepts as a parameter and never re-reads from settings.
    let oom = FakeLlm::start(vec![Reply::error(500, "CUDA out of memory. Tried to allocate 2.00 GiB")]);
    let scene = fixture("device", &oom.url());
    let sink = Sink::default();
    let waits = Waits::default();
    let outcome = ask(&scene, &scene.endpoint, "q", &sink, &waits.recorder());
    assert!(outcome.is_err(), "an out-of-memory that keeps failing fails the step");
    assert_eq!(
        waits.seconds()[0], 5,
        "a device failure backs off like a death rather than resending at 2 s: {:?}",
        waits.seconds()
    );

    // Plain 500: an answer, so the short patience.
    let plain = FakeLlm::start(vec![Reply::error(500, "internal server error")]);
    let plain_endpoint = Endpoint { url: plain.url(), key: scene.endpoint.key.clone() };
    let sink2 = Sink::default();
    let waits2 = Waits::default();
    let outcome2 = ask(&scene, &plain_endpoint, "q", &sink2, &waits2.recorder());
    assert!(outcome2.is_err(), "a plain 500 fails the step after its one retry");
    assert_eq!(waits2.seconds(), vec![2], "a plain 500 is resent soon, not waited through: {:?}", waits2.seconds());
    assert!(sink2.gone_lines().is_empty(), "a status is never a death: {:?}", sink2.gone_lines());
}
