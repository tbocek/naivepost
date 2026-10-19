//! §09-llm-and-tools#10-every-request-timed-new — the three request doors that used to leave no row.
//!
//! §10: "Every request the app sends outside — to the LLM, to audio.cpp …, to the image server, and the
//! web tools' searches and page reads — is measured and written down in the open project's folder,
//! `requests.tsv` … one line per request, appended the moment it ends". The web half arrived with
//! [`crate::web_request_log`]; these three doors were the rest of what was missing:
//!
//! * **`GET /v1/models`** — the Settings dialog's Fetch models (§9). A socket opened, an answer parsed,
//!   and no row: it went out through [`crate::server_leg::send_budget`], the unrecorded door, because §9
//!   bounds it at fifteen seconds and `call` carried no per-call ceiling. [`crate::server_leg::call_budget`]
//!   is what closes that: the row and the bound on one request.
//! * **audio.cpp's catalogue in a run** — `audio_task::catalog`, called with `tree: None` by the missing-model
//!   sentence and by the aligner's choice. Same fix, the same door as every other audio call.
//! * **`web_search` and `web_read` with no project open** — the row is filed when there is a project to file
//!   it in, and the ladder's own answer decides `ok` vs `error <reason>` (§10 counts a refusal as a request
//!   that happened).
//!
//! Every address here is a loopback port this file opened or left closed, so nothing leaves the machine and
//! no timing assertion needs a sleep. Rows come back through [`requests::read`], i.e. off disk.
//!
//! # Why one scenario at a time
//!
//! Pointing the settings at a fake re-points PROCESS-WIDE state (`cwd`, `XDG_CONFIG_HOME`,
//! `XDG_DATA_HOME`), and the folder a row is filed under is read off `cwd` — so two checks may not be
//! inside their scenarios at once. Same guard as `tests/suite/web_requests_tsv.rs`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use naivepost::layout::Tree;
use naivepost::requests::{self, Outcome, Service};
use naivepost::server_leg;
use naivepost::services::{self, Endpoint, Kind, Server};
use naivepost::settings::{self, Conf};
use naivepost::startup;
use naivepost::{model_list_fetch, settings_probe};

use crate::common;

/// Held while a scenario re-points the process-wide settings env and `cwd`.
fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

/// A scenario: temp root + the guard that keeps this the only test inside the process-wide env. The open
/// project is `<root>/session.naivepost`, which is what the program's own door resolves `cwd` to.
struct Scenario {
    root: std::path::PathBuf,
    _scenario: std::sync::MutexGuard<'static, ()>,
}

impl std::ops::Deref for Scenario {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.root
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture(tag: &str) -> Scenario {
    let scenario = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-more-timed-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scenario root");
    std::fs::create_dir_all(startup::session_dir(&root)).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
    }
    let paths = settings::from_environment().expect("settings from the pinned env");
    settings::save(&paths, &Conf::default()).expect("settings written");
    Scenario { root, _scenario: scenario }
}

/// A fake server on a loopback port: answers `answer(path)` to every request and records each request line
/// on a channel, so an assertion about what went out comes from the other end of the wire.
struct Fake {
    url: String,
    seen: mpsc::Receiver<(String, String)>,
}

impl Fake {
    fn start(answer: impl Fn(&str) -> (u16, String) + Send + Sync + 'static) -> Fake {
        let answer = std::sync::Arc::new(answer);
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let (sent, received) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sent = sent.clone();
                let answer = answer.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().expect("a socket clone"));
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).ok().filter(|n| *n > 0).is_none() {
                        return;
                    }
                    let method = request_line.split_whitespace().next().unwrap_or("").to_string();
                    let path = request_line.split_whitespace().nth(1).unwrap_or("").to_string();
                    let mut length = 0usize;
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                            break;
                        }
                        if let Some(rest) = header.to_lowercase().strip_prefix("content-length:") {
                            length = rest.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    if length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    let _ = sent.send((format!("{method} {path}"), String::from_utf8_lossy(&body).into_owned()));
                    let (status, text) = answer(&path);
                    let reply = format!(
                        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                        text.len()
                    );
                    let mut stream = stream;
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                });
            }
        });
        Fake {
            url: format!("http://127.0.0.1:{port}"),
            seen: received,
        }
    }

    /// The next request the server saw, waiting up to five seconds for it (§10's row must exist by then).
    fn next_seen(&self) -> (String, String) {
        self.seen
            .recv_timeout(Duration::from_secs(5))
            .expect("a request reached the fake server")
    }
}

/// A port nothing listens on, held so the number stays off the market for the test that dials it.
fn dead_url() -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a spare port");
    let port = listener.local_addr().expect("an address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// Point the settings' Server box at `url`, the way the dialog writes it, so the LIVE door reads it back.
fn point_llm_at(url: &str) {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).unwrap_or_default();
    conf.server = url.to_string();
    settings::save(&paths, &conf).expect("settings written");
}

fn endpoint_at(url: &str) -> Endpoint {
    Endpoint {
        url: services::server_url(url).expect("a loopback URL parses"),
        key: String::new(),
    }
}

fn never() -> bool {
    false
}

/// The rows the open project's `requests.tsv` holds now.
fn rows() -> Vec<requests::Request> {
    let tree = Tree::new(startup::session_dir(&std::env::current_dir().unwrap())).expect("session tree");
    requests::read(&tree).expect("requests.tsv reads back")
}

/// The item under test, kept in a string so it survives grepping and cannot be a stale comment.
const ITEM: &str = "§09-llm-and-tools#10-every-request-timed-new";

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s0_the_item_under_test_is_named() {
    assert_eq!(ITEM, "§09-llm-and-tools#10-every-request-timed-new");
}

/// §9's listing + §10's rule: Fetch models is a request the app sent outside, so it leaves a row.
///
/// The door under test is [`naivepost::settings_probe::fetch_models`] — the live provider's answer to the
/// Fetch models key — not [`model_list_fetch::models_at`] directly, so a press on the button and this check
/// run the same code.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s1_the_model_listing_lands_in_requests_tsv() {
    let project = fixture("listing-row");
    let fake = Fake::start(|path| {
        assert_eq!(path, "/v1/models", "§9 asks for the listing and nothing else");
        (200, r#"{"data":[{"id":"qwen2.5vl:7b"}]}"#.to_string())
    });
    point_llm_at(&fake.url);

    let body = settings_probe::fetch_models(&fake.url).expect("the listing came back");
    assert!(body.contains("qwen2.5vl:7b"), "the body arrived as the server wrote it: {body}");
    let asked = fake.next_seen();
    assert_eq!(asked.0, "GET /v1/models", "one GET, on §9's path");

    let written = rows();
    let row = written
        .iter()
        .find(|row| row.kind == "GET" && row.service == Service::Llm)
        .expect("the listing wrote a row: {written:?}");
    assert_eq!(row.step, "settings", "the dialog asked, outside any run's step");
    assert_eq!(row.outcome, Outcome::Ok, "a listing that arrived is not a failure");
    assert!(
        row.on_wire_s.unwrap_or(-1.0) >= 0.0,
        "timed like every other request, got {:?}",
        row.on_wire_s
    );
    assert_eq!(row.attempt, 1, "the first attempt is attempt 1");
    // §10's run ends with a per-service line; the row has to be tallied, not just written.
    let tallies = naivepost::request_timing::tally(&written);
    let llm = tallies
        .iter()
        .find(|tally| tally.service == Service::Llm)
        .expect("the listing counts as an llm request in the per-service summary");
    assert_eq!(llm.count, 1);
    drop(project);
}

/// §10's second half: a listing that FAILED is a row too, with the failure in the outcome column.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s2_a_failed_model_listing_still_leaves_its_row() {
    let project = fixture("listing-error");
    let dead = dead_url();
    point_llm_at(&dead);

    let answer = settings_probe::fetch_models(&dead);
    assert!(answer.is_err(), "nothing listens there: {answer:?}");

    let written = rows();
    let row = written
        .iter()
        .find(|row| row.service == Service::Llm && row.kind == "GET")
        .expect("a request that could not be answered is still a request: {written:?}");
    match &row.outcome {
        Outcome::Error(reason) => assert!(
            !reason.trim().is_empty(),
            "the outcome names what went wrong, got {reason:?}"
        ),
        other => panic!("a refused connection is an error row, got {other:?}"),
    }
    assert!(
        row.on_wire_s.unwrap_or(-1.0) >= 0.0,
        "it cost time to fail and says so, got {:?}",
        row.on_wire_s
    );
    drop(project);
}

/// The §9 ceiling has to survive the move onto the logged path: fifteen seconds on the whole call, so a
/// wrong address stops the dialog rather than hanging it. §1 leaves [`Kind::ListModels`] unbounded, which
/// is exactly why the bound is passed per call and not read from the table.
///
/// `NAIVEPOST_RETRY_SCALE` shrinks the bound for this check — the hook §3's waits already use — because a test
/// that had to sit through the full fifteen seconds would be skipped rather than run.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s3_the_listing_keeps_its_fifteen_second_ceiling_under_a_tree() {
    let project = fixture("listing-budget");
    unsafe { std::env::set_var("NAIVEPOST_RETRY_SCALE", "15") };
    let tree = Tree::new(startup::session_dir(&project.root)).expect("session tree");
    // A port that accepts and never answers: only the client's own ceiling ends this call.
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a spare port");
    let port = listener.local_addr().expect("an address").port();
    let sink = std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // Hold the connection open, say nothing, and let the client's own ceiling fire.
            std::thread::spawn(move || {
                let mut sink = stream;
                let mut black_hole = String::new();
                let _ = sink.read_to_string(&mut black_hole);
            });
        }
    });
    let began = std::time::Instant::now();
    let answer = model_list_fetch::models_at(
        &endpoint_at(&format!("http://127.0.0.1:{port}")),
        &Some(tree.clone()),
    );
    let took = began.elapsed();
    assert!(answer.is_err(), "a server that never answers must not read as a listing: {answer:?}");
    assert!(
        took < Duration::from_secs(6),
        "§9's bound ends the logged path too (scaled here to 1 s); the call took {took:?}",
    );
    // And the ceiling gave up AFTER the row was filed, so §10 keeps the abandoned request too.
    let written = requests::read(&tree).expect("requests.tsv reads back");
    assert_eq!(written.len(), 1, "the given-up listing left its row: {written:?}");
    assert!(matches!(written[0].outcome, Outcome::Error(_)), "as an error row");
    drop(sink);
    unsafe { std::env::remove_var("NAIVEPOST_RETRY_SCALE") };
    drop(project);
}

/// audio.cpp's catalogue asked by a RUN (the missing-model sentence, and the aligner's choice) goes out
/// through the recorded door when there is a tree to record into — the same door every other audio call rides.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s4_the_audio_catalogue_dials_through_the_recorded_door() {
    let project = fixture("audio-catalogue");
    let fake = Fake::start(|path| {
        assert_eq!(path, "/v1/models", "§2's audio listing");
        (200, r#"{"data":[{"id":"nemotron-asr","family":"qwen3","task":"asr"}]}"#.to_string())
    });
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).unwrap_or_default();
    conf.audio_server = fake.url.clone();
    settings::save(&paths, &conf).expect("the audio address is written");
    let tree = Tree::new(startup::session_dir(&project.root)).expect("session tree");

    let models = naivepost::audio_task::catalog(&Some(tree.clone()), &paths, "Prepare").expect("the listing arrives");
    assert!(
        models.iter().any(|model| model.id == "nemotron-asr"),
        "the run reads the ids the server declared, got {models:?}"
    );
    let asked = fake.next_seen();
    assert_eq!(asked.0, "GET /v1/models");

    let written = requests::read(&tree).expect("requests.tsv reads back");
    let row = written
        .iter()
        .find(|row| row.service == Service::Audio && row.step == "Prepare")
        .expect("the catalogue is logged under the step that asked: {written:?}");
    assert_eq!(row.outcome, Outcome::Ok);
    assert!(row.on_wire_s.unwrap_or(-1.0) >= 0.0, "timed, got {:?}", row.on_wire_s);
    drop(project);
}

/// The same catalogue with NO tree keeps working — §00 rule 8, a missing log never refuses the work — and
/// writes nothing, because there is no project folder to write into.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s5_a_catalogue_with_no_project_answers_and_files_nothing() {
    let project = fixture("audio-catalogue-lost");
    let fake = Fake::start(|_| (200, r#"{"data":[]}"#.to_string()));
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).unwrap_or_default();
    conf.audio_server = fake.url.clone();
    settings::save(&paths, &conf).expect("the audio address is written");
    // No session folder on disk: nothing exists for a row to be filed under.
    let session = startup::session_dir(&project.root);
    std::fs::remove_dir_all(&session).expect("session folder removed");

    let models = naivepost::audio_task::catalog(&None, &paths, "Prepare").expect("an empty listing is an answer");
    assert!(models.is_empty(), "the server served nothing, and that arrived as an empty list");
    let asked = fake.next_seen();
    assert_eq!(asked.0, "GET /v1/models");
    assert!(!session.exists(), "the unrecorded door creates no project folder");
    drop(project);
}

/// A web tool call with no opened project: §03 F0.6 says a launch with nothing to open lands on "a blank
/// session at `root/session.naivepost`", so a headless leg's working root always has a session folder, and
/// §10's row goes into it. What this pins is the pair: the tool answered (no project, no refusal — §00 rule
/// 8), and its row landed in the session beside the working root, which did not exist before the call.
/// The row for the same call inside an opened project is `tests/suite/web_requests_tsv.rs` s25/s26.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s6_a_web_call_outside_an_opened_project_lands_in_the_session() {
    let scenario = fixture("web-no-project");
    let session = startup::session_dir(&scenario.root);
    std::fs::remove_dir_all(&session).expect("session folder removed");
    let elsewhere = scenario.root.join("plain-folder");
    std::fs::create_dir_all(&elsewhere).expect("a non-project cwd");
    std::env::set_current_dir(&elsewhere).expect("cwd moved off the project root");
    assert!(
        !startup::session_dir(&elsewhere).exists(),
        "the session this call will meet does not exist yet"
    );
    // The browser is pointed at a page with no result markup: the ladder runs, finds nothing, and answers
    // with §2's own sentence. What this scenario asks is where the ROW went.
    let stub = stub_browser(&scenario.root, "<html><body><p>Nothing here at all.</p></body></html>");
    let call = naivepost::exchanges::ToolCall {
        name: "web_search".to_string(),
        args: r#"{"broad":"a","medium":"b","narrow":"c"}"#.to_string(),
        result: String::new(),
    };
    let answer = naivepost::web_browser::dispatch("narrate", &stub.to_string_lossy(), &call)
        .expect("web_search is answered by the browser leg");
    assert!(
        answer.contains("No results") || answer.contains("Results for"),
        "the ladder answered in its own words: {answer}"
    );
    // §10's half: the row exists, filed in the session that belongs to this working root (§03 F0.6 gives a
    // launch with nothing to open a blank session there, so a headless leg always has somewhere to file).
    let outside = naivepost::web_request_log::session_tree().expect("a headless leg always has its session");
    let written = requests::read(&outside).expect("the session's requests.tsv reads back");
    let row = written
        .iter()
        .find(|row| row.service == Service::Web && row.kind == "search")
        .expect("the search left its row in the session beside the working root: {written:?}");
    assert_eq!(row.step, "narrate", "the step that asked");
    match &row.outcome {
        Outcome::Error(reason) => assert!(
            !reason.trim().is_empty(),
            "a search that failed failed for a nameable reason, got {reason:?}"
        ),
        Outcome::Ok => {} // "No results…" is §2's own answer: content, not a fault
        other => panic!("the row says what happened, got {other:?}"),
    }
    assert!(row.on_wire_s.unwrap_or(-1.0) >= 0.0, "and it was timed");
    assert!(
        startup::session_dir(&elsewhere).exists(),
        "the session the row went into is the one beside the working root"
    );
    drop(scenario);
}

/// The stub browser: prints whatever DOM the scenario asked for and ignores the URL, which is the point —
/// the leg must not care what it was pointed at.
fn stub_browser(dir: &std::path::Path, dom: &str) -> std::path::PathBuf {
    let path = dir.join("fake-firefox.sh");
    let dom_path = dir.join("dom.html");
    std::fs::write(&dom_path, dom).expect("dom written");
    common::write_script(&path, format!("#!/bin/sh\ncat \"{}\"\nexit 0\n", dom_path.display()))
        .expect("stub written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("stub executable");
    }
    path
}

/// The recorded door with a budget is what makes §9's bound and §10's row compatible: prove the pair
/// together on one call, so neither half can be dropped without a failure here.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s7_a_budgeted_call_is_recorded_and_bounded() {
    let project = fixture("budgeted-row");
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a spare port");
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut hold = stream;
                let mut nothing = String::new();
                let _ = hold.read_to_string(&mut nothing);
            });
        }
    });
    let tree = Tree::new(startup::session_dir(&project.root)).expect("session tree");
    let base = server_leg::base_row(Server::Llm, "", "GET", "", "settings", "");
    let began = std::time::Instant::now();
    let (_row, reply) = server_leg::call_budget(
        &tree,
        &base,
        Server::Llm,
        &endpoint_at(&format!("http://127.0.0.1:{port}")),
        Kind::ListModels,
        None,
        None,
        Some(Duration::from_millis(400)),
        &never,
    );
    let took = began.elapsed();
    assert!(reply.is_err(), "a server that says nothing must not answer: {reply:?}");
    assert!(
        took < Duration::from_secs(5),
        "the per-call ceiling ended it, it did not sit on the socket: {took:?}",
    );

    let written = requests::read(&tree).expect("requests.tsv reads back");
    assert_eq!(written.len(), 1, "one attempt, one row");
    match &written[0].outcome {
        Outcome::Error(reason) => assert!(!reason.trim().is_empty(), "named reason: {reason:?}"),
        other => panic!("a timed-out call is an error row, got {other:?}"),
    }
    assert!(written[0].on_wire_s.unwrap_or(-1.0) >= 0.0, "and timed");
    drop(project);
}

/// §3's waits, recorded rather than slept: what is under test below is the LOG's shape, not the patience, and a
/// check that sat through even §3's one short retry would be skipped rather than run. Each entry is the number of
/// seconds the ladder asked for, in the order it asked.
///
/// `chat_on` takes the wait as an argument precisely so a caller can decide this; the rule itself runs untouched.
fn waited() -> &'static std::sync::Mutex<Vec<u64>> {
    static WAITED: std::sync::OnceLock<std::sync::Mutex<Vec<u64>>> = std::sync::OnceLock::new();
    WAITED.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

fn clear_waited() {
    waited().lock().expect("wait list").clear();
}

/// §10's "A retry is a line of its own", with its attempt number — the half no other check in the suite produced:
/// every other one writes at most one row per request, so a recorder that OVERWROTE the first attempt's line
/// instead of appending the second's would pass them all. A plain 500 is the cleanest way to force a retry: it is
/// neither a transport death (so no ladder) nor a refusal of the `tools` field (which the caller answers with a
/// DIFFERENT request), so §3 gives exactly one short retry of the identical body.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s8_a_retry_is_a_line_of_its_own() {
    let project = fixture("retry-lines");
    let fake = Fake::start(|_| (500, r#"{"error":"nope"}"#.to_string()));
    point_llm_at(&fake.url);
    let tree = Tree::new(startup::session_dir(&project.root)).expect("session tree");
    clear_waited();
    let base = server_leg::base_row(Server::Llm, "s8model", "chat", "", "s8step", "");
    let outcome = naivepost::llm_retry_leg::chat_on(
        "s8step",
        &tree,
        &base,
        &endpoint_at(&fake.url),
        r#"{"model":"s8model","messages":[{"role":"user","content":"say it"}]}"#,
        &never,
        &|_| {},
        // Recorded, not spent: the wait is §3's business, the two rows are §10's.
        &|seconds| {
            waited().lock().expect("wait list").push(seconds);
            true
        },
    );
    let why = outcome.expect_err("a server that refuses every try is not an answer: {outcome:?}");
    assert!(why.contains("500"), "the caller gets the status back: {why}");
    // One wait, and it is §3's short retry (`RETRY_SECONDS`), not the dead-server ladder: a status is never
    // "the server went away".
    assert_eq!(
        waited().lock().expect("wait list").as_slice(),
        &[naivepost::llm_retry::RETRY_SECONDS],
        "one short retry, asked for once"
    );
    // The point: BOTH attempts are in the file, each numbered, neither replacing the other.
    let mine: Vec<requests::Request> = requests::read(&tree)
        .expect("requests.tsv reads back")
        .into_iter()
        .filter(|row| row.step == "s8step")
        .collect();
    assert_eq!(mine.len(), 2, "two attempts went out, so two lines: {mine:?}");
    assert_eq!(mine[0].attempt, 1, "the first line is the first try");
    assert_eq!(mine[1].attempt, 2, "and the retry is a line of its own, numbered");
    for row in &mine {
        match &row.outcome {
            Outcome::Error(reason) => assert!(reason.contains("500"), "{reason}"),
            other => panic!("a refused response is an error row, got {other:?}"),
        }
        assert!(row.on_wire_s.unwrap_or(-1.0) >= 0.0, "and each is timed: {row:?}");
    }
    drop(project);
}

/// §10's fourth named ending — "cancelled by ⏹" — reaching the FILE. Nothing outside a hand-written row has ever
/// put `cancelled` in `requests.tsv`, and the case is easy to lose: the row is written where the call RETURNS, so
/// a stop that keeps a request from going out at all still has to leave its line. `send_pinged`'s guard is that
/// path ("chat: cancelled before sending"), and `call_dialed` must not swallow it.
#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s9_a_request_stopped_by_the_stop_key_leaves_its_cancelled_row() {
    let project = fixture("cancelled-row");
    let tree = Tree::new(startup::session_dir(&project.root)).expect("session tree");
    let base = server_leg::base_row(Server::Llm, "s9model", "chat", "", "s9step", "");
    // The address is irrelevant — nothing goes out — but a closed port keeps the check honest if the guard ever
    // stops firing: the failure would then be a refused connect, i.e. an ERROR row, and this check would say so.
    let (_row, result) = server_leg::call(
        &tree,
        &base,
        Server::Llm,
        &endpoint_at(&dead_url()),
        Kind::Chat,
        None,
        Some("{}"),
        &|| true,
    );
    let why = result.expect_err("a stopped call has no reply: {result:?}");
    assert!(why.contains("cancelled"), "the caller is told it was stopped: {why}");
    assert_eq!(_row.outcome, Outcome::Cancelled, "the row says which ending this was");
    let mine: Vec<requests::Request> = requests::read(&tree)
        .expect("requests.tsv reads back")
        .into_iter()
        .filter(|row| row.step == "s9step")
        .collect();
    assert_eq!(mine.len(), 1, "a ⏹ stop leaves exactly one line: {mine:?}");
    assert!(
        matches!(mine[0].outcome, Outcome::Cancelled),
        "⏹ is an ending the log records, not a request that never happened: {:?}",
        mine[0].outcome
    );
    assert!(mine[0].on_wire_s.unwrap_or(-1.0) >= 0.0, "and it is timed all the same: {:?}", mine[0].on_wire_s);
    drop(project);
}
