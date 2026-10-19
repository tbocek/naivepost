//! §02-services#1's first clause over the browser legs: "every request to any of them — **and every web
//! search and page read** — is timed and written to the project's `requests.tsv`".
//!
//! [`tests/suite/four_servers_wire.rs`] proves that sentence for the three HTTP servers by dialing a loopback
//! fake. Nothing in it can prove the web half, because a search and a read launch a browser process rather
//! than opening a socket, so no leg in `server_leg` ever sees them. These tests take the same seam the
//! program offers — the Settings `firefox` box pointed at a stub executable — and go through
//! [`naivepost::web_browser::dispatch`], the one door both live web callers reach (`narrate_live`,
//! `produce_upload_call`). What lands in `requests.tsv` therefore came from the real ladder, with the real
//! timings, filed by the real recorder.
//!
//! # Why one scenario at a time
//!
//! Pointing the settings at a stub re-points PROCESS-WIDE state (`cwd`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`),
//! and the session folder the row is filed under is read off `cwd` — so two checks may not be inside their
//! scenarios at once. Same guard as `tests/suite/web_tools_f6_1_browser.rs`.

use std::path::{Path, PathBuf};

use naivepost::exchanges::ToolCall;
use naivepost::layout::Tree;
use naivepost::request_timing::{self, Timer};
use naivepost::requests::{self, Outcome, Request, Service};
use naivepost::settings::{self, Conf};
use naivepost::startup;
use naivepost::ui;
use naivepost::web_browser;
use naivepost::web_request_log;

use crate::common;

/// Held while a scenario re-points the process-wide settings env and `cwd`.
fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

/// A scenario: temp root + the guard that keeps this the only test inside the process-wide env. The session
/// folder (the project the row must land in) is `<root>/session.naivepost`, which is what `current_dir`
/// resolves to for the program's own door.
struct Scenario {
    root: PathBuf,
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
    let root = std::env::temp_dir().join(format!("naivepost-webtsv-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scenario root");
    // The working copy beside the root IS the open project for a headless leg; make it one `Tree` accepts.
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

/// The stub browser: prints whatever DOM the scenario asked for and ignores the URL, which is the point —
/// the leg must not care what it was pointed at.
fn write_stub(dir: &Path, dom: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("stub folder");
    let path = dir.join("fake-firefox.sh");
    // The DOM goes in its own file: quoting HTML through a shell string breaks on an apostrophe rather
    // than on the thing being checked.
    let dom_path = dir.join("dom.html");
    std::fs::write(&dom_path, dom).expect("dom written");
    let script = format!("#!/bin/sh\ncat \"{}\"\nexit 0\n", dom_path.display());
    common::write_script(&path, script).expect("stub written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("stub executable");
    }
    path
}

/// Point the settings' `firefox` box at `browser`, exactly as the Settings dialog writes it.
fn point_at(browser: &Path) {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).unwrap_or_default();
    conf.firefox = browser.to_string_lossy().to_string();
    settings::save(&paths, &conf).expect("settings written");
}

/// The old DuckDuckGo result-page shape, wide enough that hits are extracted from it.
fn ddg_page(urls: &[&str]) -> String {
    let articles = urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            format!(
                "<article><h2><a href=\"{url}\">Result number {}</a></h2><div>The snippet for result {}, which says something useful.</div></article>",
                i + 1,
                i + 1
            )
        })
        .collect::<Vec<String>>()
        .join("");
    format!("<html><head><title>search</title></head><body>{articles}</body></html>")
}

/// A page wide enough to clear `web_tools::MIN_PAGE_CHARS`.
fn long_page(body: &str) -> String {
    format!("<html><body><p>{body}</p><p>{}</p></body></html>", "x".repeat(250))
}

fn call(name: &str, args: &str) -> ToolCall {
    ToolCall { name: name.to_string(), args: args.to_string(), result: String::new() }
}

/// The rows the run left in the open project's `requests.tsv`.
fn rows() -> Vec<Request> {
    let tree = Tree::new(startup::session_dir(&std::env::current_dir().unwrap())).expect("session tree");
    requests::read(&tree).expect("requests.tsv reads back")
}

/// A DOM with no result markup in it: a real page, so the browser runs and answers, but nothing to extract.
const NO_RESULTS_PAGE: &str = "<html><body><p>DuckDuckGo is thinking. This page carries no results at all.</p></body></html>";

/// S1: a `web_search` goes out through the browser leg and leaves its timed row behind.
#[test]
fn sec_02_services_1_the_four_servers_s25_a_web_search_lands_in_requests_tsv() {
    let root = fixture("search-row");
    let stub = write_stub(&root, &ddg_page(&["https://one.example/a", "https://two.example/b"]));
    point_at(&stub);

    let answer = web_browser::dispatch(
        "narrate",
        &stub.to_string_lossy(),
        &call("web_search", r#"{"broad":"tower height","medium":"ez tower","narrow":"634 m"}"#),
    )
    .expect("web_search is answered by the browser leg");
    assert!(answer.contains("Results for"), "the model got the hit list: {answer}");

    let written = rows();
    let row = written
        .iter()
        .find(|row| row.kind == web_request_log::KIND_SEARCH)
        .expect("the search wrote a row of kind `search`: {written:?}");
    assert_eq!(row.service, Service::Web, "§09 §10 counts the web as one of the four outside services");
    assert_eq!(row.step, "narrate", "the row names the step that asked");
    assert_eq!(row.outcome, Outcome::Ok, "hits came back, so the row is not a failure");
    assert!(
        row.on_wire_s.unwrap_or(-1.0) >= 0.0,
        "the wire column is filled at all; a local stub can round to zero on the millisecond, got {:?}",
        row.on_wire_s
    );
    assert!(row.first_byte_s.is_some(), "an answer arrived, so there is a first byte");
    assert_eq!(row.wait_s, Some(0.0), "a web call takes no model slot, so it never queued (§09 §4)");
    assert!(
        row.sent_bytes.unwrap_or(0) > 0,
        "what went out is the query the model asked for, got {:?}",
        row.sent_bytes
    );
    assert!(
        row.received_bytes.unwrap_or(0) > 0,
        "what came back is the text handed to the model, got {:?}",
        row.received_bytes
    );
    assert_eq!(
        row.received_bytes.unwrap() as usize,
        answer.len(),
        "the received count is the size the conversation grew by"
    );
    assert_eq!(row.model, "", "a page fetch has no model; the column stays empty rather than naming a binary");
    assert_eq!(row.attempt, 1, "one call, one attempt");
    // §2's app-log line is unchanged by the recording: the row is extra, not a replacement. `dispatch`
    // writes it to the same sink every flow logs on, so a reader of the log still sees the ladder.
    assert!(
        ui::window::logs_for_test()
            .iter()
            .any(|line| line.starts_with(">>> narrate: searched \"tower height\" / ")),
        "§2's search line is still said: {:?}",
        ui::window::logs_for_test()
    );
    // A ladder that found nothing on any rung still went out three times and costs time, so its row says so:
    // §09 §10 wants every request in the file, and "no results" is an outcome a reader can act on rather
    // than a hole where a request should have been.
    // Same scenario, different page: the stub prints whatever `dom.html` holds, so rewriting it is a
    // second ladder inside this one scenario. A second `fixture()` here would re-lock the process-wide
    // scenario mutex this test already holds, and a non-reentrant mutex waits on its holder forever.
    std::fs::write(root.join("dom.html"), NO_RESULTS_PAGE).expect("empty dom written");
    let none = web_browser::dispatch(
        "narrate",
        &stub.to_string_lossy(),
        &call("web_search", r#"{"broad":"a","medium":"b","narrow":"c"}"#),
    )
    .expect("an empty ladder is still answered");
    assert!(none.starts_with("No results for any"), "{none}");
    let after = rows();
    let empty_row = after
        .iter()
        .rev()
        .find(|row| row.kind == web_request_log::KIND_SEARCH)
        .expect("the empty ladder left its row too");
    assert_eq!(empty_row.outcome, Outcome::Ok, "the engine answered; there was simply nothing in it");
    assert!(empty_row.on_wire_s.unwrap_or(-1.0) >= 0.0, "three queries were paid for and are timed");
}

/// S1: a page read leaves a row too — and a refused page still leaves one, naming why.
#[test]
fn sec_02_services_1_the_four_servers_s26_a_page_read_lands_in_requests_tsv() {
    let root = fixture("read-row");
    let stub = write_stub(&root, &long_page("The tower is 634 metres tall."));
    point_at(&stub);

    let answer = web_browser::dispatch(
        "publish",
        &stub.to_string_lossy(),
        &call("web_read", r#"{"url":"https://tower.example/height"}"#),
    )
    .expect("web_read is answered by the browser leg");
    assert!(answer.contains("634 metres"), "the page text reached the model: {answer}");

    let written = rows();
    let row = written
        .iter()
        .find(|row| row.kind == web_request_log::KIND_READ)
        .expect("the read wrote a row of kind `read`: {written:?}");
    assert_eq!(row.service, Service::Web);
    assert_eq!(row.step, "publish", "the upload-text step gets its own row");
    assert_eq!(row.outcome, Outcome::Ok);
    assert!(row.on_wire_s.unwrap_or(-1.0) >= 0.0, "timed like every other request");
    assert!(row.sent_bytes.unwrap_or(0) > 0 && row.received_bytes.unwrap_or(0) > 0);

    // The second branch: a bot wall. §09 §10 wants the failed request in the file as much as the good one.
    // Same scenario, different page — see s25's note on why one test gets exactly one `fixture()`.
    std::fs::write(root.join("dom.html"), "<html><body><p>Just a moment, please wait while we verify you are human.</p></body></html>").expect("walled dom written");
    let refused = web_browser::dispatch(
        "publish",
        &stub.to_string_lossy(),
        &call("web_read", r#"{"url":"https://walled.example/page"}"#),
    )
    .expect("a walled page is still answered — as an error the model can act on");
    assert!(refused.starts_with("web_read failed:"), "{refused}");

    let after = rows();
    let bad = after
        .iter()
        .rev()
        .find(|row| row.kind == web_request_log::KIND_READ)
        .expect("the refused read also left a row");
    match &bad.outcome {
        Outcome::Error(reason) => assert!(
            reason.contains("bot wall"),
            "the outcome names why the read failed, got {reason}"
        ),
        other => panic!("a refusal is an error row, got {other:?}"),
    }
    assert!(bad.on_wire_s.unwrap_or(-1.0) >= 0.0, "a failed request cost time and says so");
}

/// S1: the web rows join the per-service tally, which is what the run's closing "web 3 in 12s" line reads.
#[test]
fn sec_02_services_1_the_four_servers_s27_web_rows_join_the_service_tally() {
    let root = fixture("tally");
    let stub = write_stub(&root, &ddg_page(&["https://one.example/a"]));
    point_at(&stub);
    web_browser::dispatch(
        "narrate",
        &stub.to_string_lossy(),
        &call("web_search", r#"{"broad":"a","medium":"b","narrow":"c"}"#),
    )
    .expect("search answered");
    let page_stub = write_stub(&root, &long_page("The bridge spans 1600 metres."));
    point_at(&page_stub);
    web_browser::dispatch(
        "narrate",
        &page_stub.to_string_lossy(),
        &call("web_read", r#"{"url":"https://bridge.example/span"}"#),
    )
    .expect("read answered");

    let mut written = rows();
    // One LLM row alongside, built the way a chat leg builds it, so the grouping is proven against a
    // service the file already counted before this round.
    let base = Request {
        service: Service::Llm,
        kind: "chat".to_string(),
        step: "suggest".to_string(),
        ..Default::default()
    };
    let mut timer = Timer::sent(0);
    timer.finished(1500);
    written.push(request_timing::finish_row(&base, &timer, Outcome::Ok, 1));

    let tally = request_timing::tally(&written);
    let web = tally
        .iter()
        .find(|entry| entry.service == Service::Web)
        .expect("the web has an entry now: {tally:?}");
    assert_eq!(web.count, 2, "one search and one read were counted");
    assert!(
        tally.iter().any(|entry| entry.service == Service::Llm && entry.count == 1),
        "the llm rows keep counting beside them: {tally:?}"
    );
    assert_eq!(web.cached, 0, "nothing about a browser trip is a cache hit");
    assert!(
        !request_timing::summary_line(&tally).is_empty(),
        "the summary line has something to say about the web"
    );
}
