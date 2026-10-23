//! F6.3 §4, the LIVE half: a chat call that sits under the stall watch while a real socket answers nothing.
//!
//! This is its own test binary on purpose, and the reason is a read order rather than a rule: `llm_liveness` reads
//! each of §4's two seams ONCE per process (`OnceLock`), and `Watch::poll` touches the heartbeat one on every pure
//! call too. In a binary that mixes the pure moment-driven checks with these live ones, whichever check reaches those
//! functions first fixes the number for the whole process — after which setting the seam is simply ignored and a
//! silent server is waited on for §4's real five minutes (measured: 30.00 s of nothing, no give-up, no heartbeat).
//! A process that chooses short bounds therefore contains nothing that wants the real ones.
//!
//! What is asserted is what the user would see, through the door every chat ask goes through
//! (`llm_retry_leg::chat_on`) pointed at a fake that takes the request and says nothing: the minute's line in the log,
//! and the call given up rather than waited on. The rules run untouched; only their numbers are short — §4's silence
//! bound at 3 s and its heartbeat at 1 s, so the guard says "nothing yet" twice before it gives up.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;

use naivepost::server_leg;
use naivepost::services::{self, Endpoint};
use naivepost::settings::{self, Conf};

const MODEL: &str = "f63model";
/// The step name these checks hand the door, so §4's `>>> <step>` prefix is recognisable in the log.
const STEP: &str = "f63live";

/// §4's bounds for this binary, in real seconds: silence 3 s, heartbeat 1 s. They are the program's OWN seams
/// (`llm_liveness::stall_seconds` / `heartbeat_seconds`), each read once per process.
///
/// The heartbeat decides how often the guard speaks, so it is worth knowing why a test may shorten it: §4's line is
/// "every minute", and the rule is `now - last_said >= heartbeat_seconds()`. A check that wants to SEE that line must
/// run at a minute small enough to appear inside its own call — otherwise nothing is ever logged and the wording under
/// test never reaches the log at all. That is shortening the NUMBER the rule is measured against, never the rule.
fn bounds() {
    std::env::set_var("NAIVEPOST_LLM_STALL_SECONDS", "3");
    std::env::set_var("NAIVEPOST_LLM_HEARTBEAT_SECONDS", "1");
    // §3's ladder underneath §4's watch: divided so the patience does not dominate a check about silence.
    std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
}

/// Set the seams, then prove the program read them. If anything ever again fixes them earlier in the process, this
/// names the bound in the first second instead of leaving a mystery wait to be guessed at afterwards.
fn bounds_agreed() {
    bounds();
    assert_eq!(
        naivepost::llm_liveness::stall_seconds(),
        3,
        "the silence seam was already read at §4's default by something earlier in this process"
    );
    assert_eq!(
        naivepost::llm_liveness::heartbeat_seconds(),
        1,
        "the heartbeat must be 1 s here"
    );
}

/// A listener that reads each request and then writes NOTHING back, holding the socket open. This is §4's worst
/// case — a server that is there, took the work, and has said nothing since.
fn silent_llm() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
    let port = listener.local_addr().expect("the bound address").port();
    std::thread::spawn(move || {
        // Every connection is read and then held: the answer never comes.
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("a clone of the socket"));
                let mut headers = String::new();
                while reader.read_line(&mut headers).is_ok_and(|read| read > 0) {}
                // Genuine silence: the request is taken, the socket stays OPEN, and nothing is ever written back.
                // Closing before the sleep would turn the check into a disconnect, which is §3's transport death, not
                // §4's stall — the shutdown happens only after the client has long since given up. The sleep runs on
                // its OWN thread and the fake never waits for it, so a test that ends in seconds is not held open.
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                });
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

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
///
/// The bounds outlive the scenario on purpose (`llm_liveness` reads each seam once per process, so there is nothing
/// to restore and no later live check that could be poisoned by them); the lock is what has to go back.
fn fixture(tag: &str, address: &str) -> Fixture {
    bounds_agreed();
    let scenario = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // A ⏹ left pressed by an earlier test would stop a chat leg before it dials (§3), which looks exactly like a
    // server that never answers and would hide what these checks are about.
    naivepost::cancel_leg::clear();
    let root = std::env::temp_dir().join(format!("naivepost-f63live-{tag}"));
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
    Fixture {
        root,
        tree,
        endpoint,
        _scenario: scenario,
    }
}

fn chat_body(text: &str) -> String {
    format!(r#"{{"model":"{MODEL}","messages":[{{"role":"user","content":"{text}"}}]}}"#)
}

/// One call through the door. The waits are recorded rather than slept: §3's patience is not what is under test here,
/// and a ladder of five would turn a second's check into eight minutes.
fn ask(scene: &Fixture, text: &str) -> Result<server_leg::Sent, String> {
    let base = server_leg::base_row(services::Server::Llm, MODEL, "chat", "", STEP, "");
    naivepost::llm_retry_leg::chat_on(
        STEP,
        &scene.tree,
        &base,
        &scene.endpoint,
        &chat_body(text),
        &|| false,
        &|_| {},
        &|_| true,
    )
}

#[test]
fn f6_3_s1_live_a_silent_server_is_given_up_over_the_wire() {
    // §4's silence rule on a real socket that answers nothing. The bound is the program's own seam, 3 real seconds,
    // so the rule runs in full and only its number is short.
    let address = silent_llm();
    let scene = fixture("stall", &address);
    // Only what THIS call logs: the log sink is thread-local and process-wide, and other checks in this binary write
    // to it too, so a line found anywhere in it could belong to an earlier call.
    let logs_from = naivepost::ui::window_logs().len();
    let started = std::time::Instant::now();
    let outcome = ask(&scene, "say something");
    let took = started.elapsed();
    assert!(
        took < std::time::Duration::from_secs(25),
        "the call was waited on for {took:?}, not given up"
    );
    let why = outcome.expect_err("a server that answers nothing must not come back as an answer");
    // §4's first cancellation sentence, spelled with the bound that ended it. The watch owns silence now (§4): the
    // socket carries no bound of its own, so this is the watch's sentence and nothing else can produce it.
    assert!(why.contains("nothing arrived in 3s"), "{why}");
    let said = {
        let lines = naivepost::ui::window_logs();
        lines[logs_from..]
            .iter()
            .find(|line| line.starts_with(">>> f63live:") && line.contains("giving up"))
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "the give-up belongs in the log the user reads: {:?}",
                    &lines[logs_from..]
                )
            })
    };
    assert_eq!(said, ">>> f63live: nothing for 3s \u{2014} giving up");
}

#[test]
fn f6_3_s1_live_the_heartbeat_ticks_while_the_server_is_silent() {
    // §4's first heartbeat shape, seen TWICE: a one-second minute against a three-second bound means the guard says
    // "nothing yet" more than once before it gives up, which is the whole point of a liveness line — it keeps saying
    // it while nothing arrives.
    let address = silent_llm();
    let scene = fixture("heartbeat", &address);
    let logs_from = naivepost::ui::window_logs().len();
    let outcome = ask(&scene, "say something");
    // It ends eventually — the same watch goes on to the stall — but the line under test is the minute's one.
    let why = outcome.expect_err("a silent server is still given up");
    assert!(why.contains("nothing arrived in 3s"), "{why}");
    let lines = naivepost::ui::window_logs();
    let heartbeats = lines[logs_from..]
        .iter()
        .filter(|line| line.starts_with(">>> f63live:") && line.contains("nothing yet,"))
        .map(|line| (*line).clone())
        .collect::<Vec<String>>();
    assert!(
        heartbeats.len() >= 2,
        "the guard must tick every minute, not once: {heartbeats:?}"
    );
    // §4's first heartbeat shape, spelled: the elapsed time is the minute count, so a 1-second minute says 1s then
    // 2s. Both lines are what the item is about, so they are asserted whole, not just counted.
    assert_eq!(heartbeats[0], ">>> f63live: nothing yet, 1s in");
    assert_eq!(heartbeats[1], ">>> f63live: nothing yet, 2s in");
}

/// §10's half of the same live call: whatever §4's watch ends, the request still leaves its line in the open
/// project's `requests.tsv`. The two checks above read the log and the return value only, so nothing here looked
/// at the FILE before — and "written down the moment it ends, whether it succeeded, failed, was given up by the
/// stall watch or cancelled" is exactly the ending that is easiest to lose, because a give-up returns through the
/// error path where a recorder could plausibly decide there was nothing to write.
///
/// No new seam is set: this binary fixes §4's bounds once (`bounds_agreed`), and a second value would be silently
/// ignored (see this file's header). What is asserted is what the row says about a call the watch ended.
#[test]
fn f6_3_s1_live_a_stalled_call_is_recorded_in_requests_tsv() {
    let address = silent_llm();
    let scene = fixture("stall-row", &address);
    let outcome = ask(&scene, "say nothing");
    let why = outcome.expect_err("a server that answers nothing must not come back as an answer");
    assert!(why.contains("nothing arrived in 3s"), "{why}");

    let mine: Vec<naivepost::requests::Request> = naivepost::requests::read(&scene.tree)
        .expect("requests.tsv reads back")
        .into_iter()
        .filter(|row| row.step == STEP)
        .collect();
    assert!(
        !mine.is_empty(),
        "a call the watch gave up on still left its line: {mine:?}"
    );
    for row in &mine {
        // §4's give-up is its OWN outcome in the log, not folded into a generic error: the whole reason §10 lists
        // "given up by the stall watch" beside success and failure is so a reader can tell a silent server from a
        // broken one, and the slot counts and model comparisons downstream are read off that column.
        assert!(
            matches!(row.outcome, naivepost::requests::Outcome::Stalled),
            "§4's ending is its own outcome in the row, got {:?}",
            row.outcome
        );
    }
    assert_eq!(mine[0].attempt, 1, "the first line is the first try");
    assert!(
        mine[0].on_wire_s.unwrap_or(-1.0) >= 2.5,
        "and it costs the silence bound that ended it (§4's 3 s here), got {:?}",
        mine[0].on_wire_s
    );
    if mine.len() > 1 {
        // §10: "A retry is a line of its own." Whatever the ladder underneath did, each attempt is its own row
        // and none repeats another's number.
        let numbers: Vec<u32> = mine.iter().map(|row| row.attempt).collect();
        let expected: Vec<u32> = (1..=numbers.len() as u32).collect();
        assert_eq!(
            numbers, expected,
            "each attempt is numbered once, in order: {mine:?}"
        );
    }
}
