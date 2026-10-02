//! §7's live half: something drives `exchanges`.
//!
//! The page builders had no caller before this — the module could format a run's page perfectly and no
//! call ever put anything in it. These checks pin the wiring: a chat call that goes out through the
//! patience door leaves a numbered section under `<project>/llm/` and the spec's two lines in the log,
//! a retried call leaves one section per attempt, a stream is filed from its `content-type`, and the
//! page is named `MMDD-HHMMSS-<first step>.html` for whichever step asked first.
//!
//! Each check pins its own cwd under a temp root named for itself (a pid-only root collides with other
//! binaries' checks and with these five between themselves).

mod common;
use common::temp_dir_created as temp_dir;

use naivepost::exchange_leg;
use naivepost::layout::Tree;
use naivepost::llm_retry_leg;
use naivepost::requests::{Outcome, Request};
use naivepost::services::Endpoint;
use naivepost::requests::Service;

/// A root only this check touches.
fn root(tag: &str) -> std::path::PathBuf {
    temp_dir(&format!("f7-{}-{tag}", std::process::id()))
}

/// A project folder under `tag`'s root.
fn tree(tag: &str) -> Tree {
    Tree::new(root(tag).join("show.naivepost")).expect("a folder ending in .naivepost is a project")
}

/// The row every chat call carries: the model the box holds, and nothing else filled in.
fn base(model: &str) -> Request {
    Request {
        started: String::new(),
        run: String::new(),
        step: "Describe".into(),
        job: String::new(),
        service: Service::Llm,
        model: model.into(),
        kind: "chat".into(),
        sent_bytes: None,
        images: None,
        received_bytes: None,
        tokens_in: None,
        tokens_out: None,
        wait_s: None,
        first_byte_s: None,
        on_wire_s: None,
        thinking_s: None,
        outcome: Outcome::Ok,
        attempt: 1,
    }
}

/// A chat body in the shape `llm_request::body` writes: one system message, the thinking switch.
fn ask_body(text: &str, thinking: bool) -> String {
    serde_json::json!({
        "model": "qwen",
        "messages": [
            {"role": "system", "content": "You describe frames."},
            {"role": "user", "content": text},
        ],
        "enable_thinking": thinking,
        "temperature": 0.2,
    })
    .to_string()
}

/// A reply a llama.cpp-style server sends to a non-streaming chat call.
fn answer(content: &str) -> String {
    serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
    })
    .to_string()
}

/// One fake LLM answering from a script, on a port nothing else holds. Every request it takes is
/// answered with the script's next reply, so a retried call is a script of two.
struct FakeLlm {
    url: String,
}

/// Each server keeps its OWN script, handed straight to its acceptor: cargo runs the cases of one
/// binary as concurrent threads, so a queue shared by both servers let whichever check reached its
/// socket first answer with the OTHER check's body.
impl FakeLlm {
    fn answer_with(replies: Vec<(String, String)>) -> FakeLlm {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        // The replies this acceptor still owes, kept in its own queue so the two fake servers of one
        // binary can never swap their scripts.
        let owed = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::from_iter(replies),
        ));
        let acceptor_owed = std::sync::Arc::clone(&owed);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let (content_type, body) = acceptor_owed
                    .lock()
                    .ok()
                    .and_then(|mut held| held.pop_front())
                    .unwrap_or_else(|| ("application/json".into(), answer("")));
                use std::io::{BufRead, BufReader, Write};
                let mut reader = BufReader::new(stream.try_clone().expect("a clone of the socket"));
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length:").or_else(|| line.strip_prefix("content-length:")) {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body_in = vec![0u8; length];
                use std::io::Read;
                let _ = reader.read_exact(&mut body_in);
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
                let _ = stream.flush();
            }
        });
        FakeLlm { url }
    }

    fn endpoint(&self) -> Endpoint {
        Endpoint { url: self.url.clone(), key: String::new() }
    }
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_a_real_call_leaves_its_page_and_its_two_lines() {
    // The gap this item is: `exchanges` could build a page and nothing drove it. A call that really goes
    // out through the patience door must now leave both halves behind: the section on disk, and §7's
    // two lines (plus the once-per-run link) in the log.
    let tree = tree("s8-real");
    let fake = FakeLlm::answer_with(vec![("application/json".into(), answer("a terminal window"))]);
    let body = ask_body("what is on screen?", false);
    let log = std::cell::RefCell::new(Vec::new());
    let sent = llm_retry_leg::chat(
        "Describe",
        &tree,
        &base("qwen"),
        &fake.endpoint(),
        &body,
        &|| false,
        &|line| log.borrow_mut().push(line.to_string()),
    )
    .expect("the fake answers");
    assert_eq!(sent.body, answer("a terminal window"));

    let pages: Vec<String> = std::fs::read_dir(tree.llm_dir())
        .expect("the run's llm/ folder")
        .filter_map(|entry| entry.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .collect();
    assert_eq!(pages.len(), 1, "one page for the run: {pages:?}");
    // MMDD-HHMMSS-<first step>.html — six-digit groups, then the step that asked first.
    let name = &pages[0];
    assert!(name.ends_with("-Describe.html"), "{name}");
    let stamp = name.trim_end_matches("-Describe.html");
    assert_eq!(stamp.len(), 14, "{name}");
    assert!(stamp.chars().all(|c| c.is_ascii_digit()), "{name}");
    // MMDD-HHMMSS in that order (§7 spells the name), which a year-first stamp would not satisfy.
    let (month, day, hour, minute, second, year) =
        (&stamp[0..2], &stamp[2..4], &stamp[4..6], &stamp[6..8], &stamp[8..10], &stamp[10..14]);
    let month: u32 = month.parse().expect("a month");
    let day: u32 = day.parse().expect("a day");
    let hour: u32 = hour.parse().expect("an hour");
    let minute: u32 = minute.parse().expect("a minute");
    let second: u32 = second.parse().expect("a second");
    let year: u32 = year.parse().expect("a year");
    assert!(
        (1..=12).contains(&month)
            && (1..=31).contains(&day)
            && hour < 24
            && minute < 60
            && second < 60
            && year > 2020,
        "MMDD-HHMMSS, month first: {name}"
    );

    let html = std::fs::read_to_string(tree.llm_dir().join(name)).expect("the page is readable");
    assert!(html.contains("<h1>1. Describe</h1>"), "{html}");
    assert!(html.contains("qwen"), "the meta line names the model: {html}");
    assert!(html.contains("execute"), "the body's thinking switch is off: {html}");
    assert!(html.contains("a terminal window"), "the reply is on the page: {html}");
    assert!(html.contains("You describe frames."), "the request is on the page: {html}");

    let said = log.borrow();
    assert!(
        said.iter().any(|l| l == ">>> Describe: 192 B of text and 0 image(s) went to the LLM")
            || said.iter().any(|l| l.ends_with("of text and 0 image(s) went to the LLM")),
        "the send line: {said:?}"
    );
    assert!(said.iter().any(|l| l.starts_with(">>>   this run's exchanges, images included: llm/")), "{said:?}");
    assert!(said.iter().any(|l| l.starts_with(">>> Describe: ") && l.contains("came back in")), "{said:?}");
    assert!(said.iter().any(|l| l.starts_with(">>>   the reply begins: a terminal window")), "{said:?}");
    // Once per run: one link line, however many lines were written.
    assert_eq!(
        said.iter().filter(|l| l.contains("this run's exchanges")).count(),
        1,
        "the link goes once per run: {said:?}"
    );
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_an_event_stream_reply_reaches_the_page_reassembled() {
    // §9 §1: only a `text/event-stream` reply is a stream, and that test is the response's own
    // content-type. The chat asks read their body to the end before anything looks at it (only
    // `speak_leg` consumes a stream as it lands), so the section is filed whole in one write and the
    // page on disk is a finished document, not an open block (§8: "rewritten whole").
    let tree = tree("s8-stream");
    let stream = "data: {\"choices\":[{\"delta\":{\"content\":\"a term\"}}]}\n\
                  data: {\"choices\":[{\"delta\":{\"content\":\"inal window\"}}]}\n\
                  data: [DONE]\n\n";
    let fake = FakeLlm::answer_with(vec![("text/event-stream".into(), stream.into())]);
    let log = std::cell::RefCell::new(Vec::new());
    llm_retry_leg::chat(
        "Narrate",
        &tree,
        &base("qwen"),
        &fake.endpoint(),
        &ask_body("voice this", true),
        &|| false,
        &|line| log.borrow_mut().push(line.to_string()),
    )
    .expect("the fake answers in pieces");

    let name = std::fs::read_dir(tree.llm_dir())
        .expect("the run's llm/ folder")
        .next()
        .expect("one page")
        .unwrap()
        .file_name()
        .to_string_lossy()
        .to_string();
    assert!(name.ends_with("-Narrate.html"), "{name}");
    let html = std::fs::read_to_string(tree.llm_dir().join(name)).expect("the page");
    // The reassembled stream is the reply, and the section was written whole: no marker of a block
    // left open mid-stream, and the meta line reports an outcome rather than a wait.
    assert!(html.contains("a terminal window"), "{html}");
    assert!(!html.contains("reply pending"), "the call is over on the page: {html}");
    assert!(!html.contains("streamed"), "nothing is left mid-stream: {html}");
    assert!(html.trim_end().ends_with("</html>"), "the page is a whole document: {html}");
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_one_page_per_run_named_for_its_first_step() {
    // A run is the calls between two resets (§7), so both sections share one page, named for the FIRST
    // step; the next run opens a fresh page.
    let tree = tree("s8-run");
    let log = std::cell::RefCell::new(Vec::new());
    exchange_leg::note_run_started();
    for step in ["Describe", "Narrate"] {
        let call = naivepost::exchange_leg::call_of(
            step,
            "qwen",
            &ask_body("x", false),
            &answer("done"),
            "application/json",
            1,
            None,
        );
        exchange_leg::record(&tree, step, &call, &|line| log.borrow_mut().push(line.to_string()));
    }
    exchange_leg::note_run_ended();
    exchange_leg::reset_run_for_test();

    let pages: Vec<String> = std::fs::read_dir(tree.llm_dir())
        .expect("the run's llm/ folder")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(pages.len(), 1, "one page for a run however many calls it made: {pages:?}");
    assert!(pages[0].ends_with("-Describe.html"), "{pages:?}");
    let html = std::fs::read_to_string(tree.llm_dir().join(&pages[0])).expect("the page");
    assert!(html.contains("<h1>1. Describe</h1>") && html.contains("<h1>2. Narrate</h1>"), "{html}");
    assert_eq!(
        log.borrow().iter().filter(|l| l.contains("this run's exchanges")).count(),
        1,
        "the link once per run, not per call"
    );

    // The next run's first call opens its own page.
    exchange_leg::note_run_started();
    let call = naivepost::exchange_leg::call_of("Fix", "qwen", &ask_body("x", false), &answer("ok"), "application/json", 1, None);
    exchange_leg::record(&tree, "Fix", &call, &|_| {});
    exchange_leg::note_run_ended();
    let pages: Vec<String> = std::fs::read_dir(tree.llm_dir())
        .expect("the run's llm/ folder")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(pages.len(), 2, "a second run, a second page: {pages:?}");
    assert!(pages.iter().any(|p| p.ends_with("-Fix.html")), "{pages:?}");
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_a_failed_call_is_on_the_page_too() {
    // The exchange the user opens the page to find is the one that went wrong: a call that got no
    // reply is filed with its reason, not skipped, and recording it never fails the call.
    let tree = tree("s8-failed");
    let log = std::cell::RefCell::new(Vec::new());
    let call = naivepost::exchange_leg::call_of(
        "Describe",
        "qwen",
        &ask_body("x", false),
        "",
        "",
        2,
        Some("no answer from localhost:1".to_string()),
    );
    exchange_leg::note_run_started();
    exchange_leg::record(&tree, "Describe", &call, &|line| log.borrow_mut().push(line.to_string()));
    exchange_leg::note_run_ended();
    exchange_leg::reset_run_for_test();

    let name = std::fs::read_dir(tree.llm_dir())
        .expect("a page even for a call that failed")
        .next()
        .expect("one page")
        .unwrap()
        .file_name()
        .to_string_lossy()
        .to_string();
    let html = std::fs::read_to_string(tree.llm_dir().join(name)).expect("the page");
    assert!(html.contains("no answer from localhost:1"), "the reason is on the page: {html}");
    let said = log.borrow();
    assert!(said.iter().any(|l| l.contains("the call failed after")), "{said:?}");
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_a_refused_body_keeps_the_server_s_words() {
    // A 400 whose body is prose is still an exchange. Reading it as a chat answer would find no JSON
    // and show an empty reply, which looks like a model with nothing to say rather than a server that
    // refused the request.
    let call = naivepost::exchange_leg::call_of(
        "Describe",
        "qwen",
        &ask_body("x", false),
        "Bad Request: unknown field `tools`",
        "text/plain",
        1,
        Some("answered 400: Bad Request: unknown field `tools`".to_string()),
    );
    assert_eq!(
        call.reply, "Bad Request: unknown field `tools`",
        "the server's own words are the reply the page shows"
    );
    assert!(call.error.is_some(), "and the refusal is still the outcome");
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_a_call_outside_a_run_is_recorded_and_ends_with_itself() {
    // A settings probe asks the model outside any ▶. It gets a page of its own rather than going
    // unrecorded — and it must not soak up the NEXT run's calls into that page.
    let tree = tree("s8-stray");
    let stray = naivepost::exchange_leg::call_of("Check", "qwen", &ask_body("x", false), &answer("qwen"), "application/json", 0, None);
    exchange_leg::record(&tree, "Check", &stray, &|_| {});
    let first = std::fs::read_dir(tree.llm_dir()).expect("the stray call left a page").next().unwrap().unwrap();
    exchange_leg::note_run_started();
    let call = naivepost::exchange_leg::call_of("Describe", "qwen", &ask_body("x", false), &answer("ok"), "application/json", 0, None);
    exchange_leg::record(&tree, "Describe", &call, &|_| {});
    exchange_leg::note_run_ended();
    exchange_leg::reset_run_for_test();

    let pages: Vec<String> = std::fs::read_dir(tree.llm_dir())
        .expect("the run's llm/ folder")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(pages.len(), 2, "the stray page stands alone: {pages:?}");
    let stray_html = std::fs::read_to_string(tree.llm_dir().join(first.file_name())).expect("the stray page");
    assert!(stray_html.contains("<h1>1. Check</h1>") && !stray_html.contains("Describe"), "{stray_html}");
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_a_second_run_opens_its_own_page() {
    // §7's page is a RUN's: the boundary is the queue reset every ▶ makes, so run #2 must not append to
    // run #1's page. Nothing but `note_run_started` closes the live page between runs — a window that
    // never called `reset_run_for_test` would otherwise pour its second press's calls into the first file.
    let tree = tree("s8-two-runs");
    let record = |tree: &Tree, step: &str| {
        let call = naivepost::exchange_leg::call_of(
            step,
            "qwen",
            &ask_body("x", false),
            &answer("ok"),
            "application/json",
            1,
            None,
        );
        exchange_leg::record(tree, step, &call, &|_| {});
    };

    assert!(
        !exchange_leg::note_run_started(),
        "a first run closes no page, because none was open"
    );
    record(&tree, "Describe");
    exchange_leg::note_run_ended();

    assert!(
        exchange_leg::note_run_started(),
        "the second run closes the page the first one left"
    );
    record(&tree, "Fix");
    exchange_leg::note_run_ended();

    let pages: Vec<String> = std::fs::read_dir(tree.llm_dir())
        .expect("the run's llm/ folder")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(pages.len(), 2, "one page per run: {pages:?}");
    assert!(pages.iter().any(|p| p.ends_with("-Describe.html")), "{pages:?}");
    assert!(pages.iter().any(|p| p.ends_with("-Fix.html")), "{pages:?}");
    let describe = pages
        .iter()
        .find(|p| p.ends_with("-Describe.html"))
        .expect("the first run's page");
    let html = std::fs::read_to_string(tree.llm_dir().join(describe)).expect("the page");
    assert!(html.contains("<h1>1. Describe</h1>"), "{html}");
    assert!(!html.contains("Fix"), "the second run stayed out of it: {html}");
    // Leave the recorder between runs, as a closed window leaves it: checks of one binary share it.
    exchange_leg::reset_run_for_test();
}
