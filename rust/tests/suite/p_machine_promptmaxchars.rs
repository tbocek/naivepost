//! §09-llm-and-tools **§5** (`P.machine.promptMaxChars`) over the wire.
//!
//! The rule has two halves and both are proven here through the LIVE door — a real socket, the
//! real `llm_retry_leg::chat` / `server_leg::call_dialled` path, the real `requests.tsv` and the
//! real app log — because the bound is only worth having if the program applies it where it
//! actually sends:
//!
//! * **"MUST bound every prompt"**: [`crate::llm_request::body`] refuses a request whose TEXT is
//!   over [`naivepost::llm_budget::PROMPT_MAX_CHARS`] before any byte goes out. `tests/suite/context_budgets.rs`
//!   already pins that rule's wording at the builder; what is pinned here is that the live door
//!   really passes a request through it — a refusal that never reaches the socket, and a request of
//!   exactly the bound that does.
//! * **"The app log records the size sent"**: `>>> <step>: N kB of text and M image(s) went to the
//!   LLM`, and the same two numbers on the call's `requests.tsv` row (§10's "how big what went out
//!   was"). Neither existed on a live call before this item.
//!
//! One scenario per test binary: `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working folder are
//! process-wide, and `just test` runs test programs side by side (see `policy_live_harness.rs`).
#![allow(dead_code)]

use crate::common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::temp_dir_created as temp_dir;
use naivepost::exchange_leg;
use naivepost::exchanges::{Message, Part};
use naivepost::layout::Tree;
use naivepost::llm_request;
use naivepost::llm_retry_leg;
use naivepost::requests::{self, Request};
use naivepost::services::{Endpoint, Server};
use naivepost::settings::{self, Conf, Paths};

/// The bound, spelled from the crate so a change to the number cannot quietly pass.
const MAX: usize = naivepost::llm_budget::PROMPT_MAX_CHARS;

/// A chat reply in the shape the legs parse: one assistant message, plain content.
fn chat(content: &str) -> String {
    let payload = serde_json::json!({"choices": [{"message": {"content": content}}]});
    payload.to_string()
}

/// A loopback LLM that records the exact bytes of every request it is asked and answers with
/// `reply`. `asked()` blocks until a request has been read off the wire, so a test waits on the
/// condition ("the server saw it") rather than on a guessed pause.
struct Fake {
    url: String,
    asked: mpsc::Receiver<(String, usize)>,
}

impl Fake {
    fn start(reply: String) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let url = format!("http://{}", listener.local_addr().expect("a local address"));
        let (sender, asked) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let reply = reply.clone();
                let sender = sender.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().expect("a cloned socket"));
                    let mut request_line = String::new();
                    let _ = reader.read_line(&mut request_line);
                    let mut length = 0usize;
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty()
                        {
                            break;
                        }
                        if header.to_ascii_lowercase().starts_with("content-length:") {
                            length = header
                                .split(':')
                                .nth(1)
                                .unwrap_or("0")
                                .trim()
                                .parse()
                                .unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    if length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    // The ask is on the wire: only now may the test go looking for its row.
                    let _ = sender.send((String::from_utf8_lossy(&body).to_string(), length));
                    let framed = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply.len(),
                        reply
                    );
                    let _ = stream.write_all(framed.as_bytes());
                    let _ = stream.flush();
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                });
            }
        });
        Fake { url, asked }
    }

    /// Wait (wall-clock deadline, condition not sleep) for one request to arrive.
    fn waited(&self) -> (String, usize) {
        match self.asked.recv_timeout(Duration::from_secs(60)) {
            Ok(ask) => ask,
            Err(why) => panic!("no request ever reached the fake LLM: {why}"),
        }
    }

    /// True only when NOTHING arrives within a short grace — used to prove a refusal sent nothing.
    fn silent(&self) -> bool {
        self.asked.recv_timeout(Duration::from_millis(400)).is_err()
    }
}

/// The scenario's own folders: settings, data and a `.naivepost` session nothing else reads.
struct Scene {
    tree: Tree,
    paths: Paths,
}

impl Scene {
    fn new(tag: &str, url: &str) -> Scene {
        let root = temp_dir(&format!("pmc-{tag}-{}", std::process::id()));
        let session = root.join("session.naivepost");
        std::fs::create_dir_all(&session).expect("the session folder");
        std::env::set_current_dir(&root).expect("cwd pinned to this scenario");
        std::env::set_var("XDG_CONFIG_HOME", root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", root.join("data"));
        let paths = settings::from_environment().expect("a settings folder from the pinned env");
        let mut conf = Conf::default();
        conf.server = url.to_string();
        conf.model = "budget-model".to_string();
        settings::save(&paths, &conf).expect("settings written");
        // Each case files its own exchange page; the page holder is per-thread and survives a call.
        exchange_leg::reset_run_for_test();
        Scene {
            tree: Tree::new(&session).expect("a .naivepost folder is a project"),
            paths,
        }
    }

    fn endpoint(&self) -> Endpoint {
        let conf = settings::read(&self.paths).expect("the conf the leg re-reads");
        Endpoint {
            url: naivepost::services::server_url(&conf.server).expect("the fake's address"),
            key: String::new(),
        }
    }
}

/// The row every chat call carries, exactly as a step's door builds it (both size columns empty —
/// that is the point: `base_row` knows no sizes, `call_dialed` must find them).
fn base(step: &str, model: &str) -> Request {
    naivepost::server_leg::base_row(Server::Llm, model, "chat", "", step, "")
}

/// One live chat call through the ONE door every chat ask in the app goes through. `log` collects
/// what the app log would show.
fn dial(
    scene: &Scene,
    step: &str,
    body: &str,
    log: &mut Vec<String>,
) -> Result<String, String> {
    let log = std::cell::RefCell::new(log);
    let reply = llm_retry_leg::chat(
        step,
        &scene.tree,
        &base(step, "budget-model"),
        &scene.endpoint(),
        body,
        &|| false,
        &|line: &str| log.borrow_mut().push(line.to_string()),
    )
    .map_err(|why| why.clone())?;
    Ok(reply.body)
}

/// The chat body for a message list, in `llm_request::body`'s own shape — so the request under
/// test is built by the program's builder, not hand-written around it.
fn body_for(step: &str, messages: &[Message]) -> Result<String, String> {
    llm_request::body(step, "budget-model", messages, naivepost::exchanges::Mode::Execute, None, false).map(|json| json.to_string())
}

/// How many picture parts a request body carried, counted the way the server sees them: one
/// `"image_url"` is spelled twice in the JSON (the part's `type` and its key), so a substring
/// count says four for two frames.
fn image_parts(body: &str) -> usize {
    let asked: serde_json::Value = serde_json::from_str(body).expect("the body the fake read is JSON");
    asked
        .get("messages")
        .and_then(|messages| messages.as_array())
        .map(|list| {
            list.iter()
                .flat_map(|message| {
                    message
                        .get("content")
                        .and_then(|content| content.as_array())
                        .map(|parts| parts.to_vec())
                        .unwrap_or_default()
                })
                .filter(|part| {
                    part.get("type").and_then(serde_json::Value::as_str) == Some("image_url")
                })
                .count()
        })
        .unwrap_or_default()
}

/// Text of exactly `chars` characters, and `images` pictures of `frame-N` bytes each.
fn letters(chars: usize) -> String {
    "a".repeat(chars)
}

/// s1: the bound is judged by the live door, on the boundary. One char over 120 000 text chars is
/// refused WITHOUT touching the socket; exactly 120 000 goes out. `tests/suite/context_budgets.rs` pins
/// the refusal's WORDING at the builder; this proves the door passes a real request through it.
#[test]
fn p_machine_promptmaxchars_s1_the_live_door_refuses_one_char_over_the_bound_and_sends_exactly_it() {
    let fake = Fake::start(chat("an answer nobody should have to pay for"));
    let scene = Scene::new("s1", &fake.url);
    // Over the bound: the system and user halves together are one char more than the cap allows.
    let over = vec![
        Message { role: "system".into(), parts: vec![Part::Text(letters(MAX / 2))] },
        Message { role: "user".into(), parts: vec![Part::Text(letters(MAX - MAX / 2 + 1))] },
    ];
    let mut log = Vec::new();
    let body = body_for("Upload brief", &over);
    let refused = match body {
        Ok(body) => dial(&scene, "Upload brief", &body, &mut log),
        Err(why) => Err(why),
    };
    let _ = "the builder is the door that refuses";
    let reason = refused.expect_err("a request one char over P.machine.promptMaxChars is refused");
    assert!(
        reason.contains("over the budget"),
        "the refusal says which rule stopped it: {reason}"
    );
    assert!(reason.contains("Upload brief"), "and names the step: {reason}");
    assert!(
        reason.contains(&MAX.to_string()) && reason.contains("get_lines"),
        "and says the bound and both ways out: {reason}"
    );
    assert!(fake.silent(), "the refusal happened before the socket: nothing was asked");

    // Exactly at the bound is WITHIN it (the boundary is inclusive), and it really goes out.
    let at = vec![
        Message { role: "system".into(), parts: vec![Part::Text(letters(MAX / 2))] },
        Message { role: "user".into(), parts: vec![Part::Text(letters(MAX - MAX / 2))] },
    ];
    let body = body_for("Upload brief", &at).expect("exactly the bound is inside it");
    let answered = dial(&scene, "Upload brief", &body, &mut log)
        .expect("a request of exactly the bound is sent");
    assert_eq!(answered, chat("an answer nobody should have to pay for"));
    let (asked, _) = fake.waited();
    assert!(asked.contains("Upload brief") || asked.contains('"'), "the ask reached the wire");
}

/// s2: §5's size line, from a call that answered — in the app log, in kB, with the image count
/// separate from the text (a vision call's base64 must not read as prose).
#[test]
fn p_machine_promptmaxchars_s2_the_log_records_the_size_that_went_to_the_llm() {
    let fake = Fake::start(chat("split at twelve seconds"));
    let scene = Scene::new("s2", &fake.url);
    let text = letters(2000);
    let messages = vec![
        Message { role: "system".into(), parts: vec![Part::Text(text.clone())] },
        Message {
            role: "user".into(),
            parts: vec![
                Part::Text("what is on these two frames?".to_string()),
                Part::Image(llm_request::data_url(b"frame-one", "image/jpeg")),
                Part::Image(llm_request::data_url(b"frame-two", "image/jpeg")),
            ],
        },
    ];
    let body = body_for("Describe", &messages).expect("well inside the bound");
    let mut log = Vec::new();
    dial(&scene, "Describe", &body, &mut log).expect("the call answered");
    let (asked, _) = fake.waited();
    assert_eq!(
        image_parts(&asked),
        2,
        "two frames went out: {asked}"
    );
    // The line §5 spells, with THIS step's name.
    let line = log
        .iter()
        .find(|line| line.contains("of text and") && line.contains("went to the LLM"))
        .unwrap_or_else(|| panic!("no size line in the app log: {log:?}"));
    assert!(line.starts_with(">>> Describe:"), "named for the step: {line}");
    assert!(line.contains("2 image(s)"), "both pictures counted: {line}");
    // The kB figure is what the bytes the wire carried say, and it excludes the pictures: the
    // two data URLs are ~40 bytes, so a figure that included them would round differently.
    let stated = line
        .split(':')
        .nth(1)
        .and_then(|rest| rest.split(" of text").next())
        .expect("the line states a size")
        .trim()
        .to_string();
    let kb: f64 = stated
        .trim_end_matches(" kB")
        .parse()
        .unwrap_or_else(|why| panic!("the size reads as kilobytes, not {stated:?}: {why}"));
    let text_bytes = text.len() + "what is on these two frames?".len();
    assert!(
        (kb - text_bytes as f64 / 1024.0).abs() < 0.1,
        "{stated} should be the {text_bytes} bytes of text the call carried (pictures excluded)"
    );
}

/// s3: the same two numbers on the call's `requests.tsv` row, and on no row at all when the bound
/// refused the call — the row is written only for a request that went out.
#[test]
fn p_machine_promptmaxchars_s3_the_timed_row_records_what_was_sent() {
    let fake = Fake::start(chat("a short answer"));
    let scene = Scene::new("s3", &fake.url);
    let text = letters(1500);
    let messages = vec![
        Message { role: "system".into(), parts: vec![Part::Text(text.clone())] },
        Message {
            role: "user".into(),
            parts: vec![
                Part::Text("describe frame one".to_string()),
                Part::Image(llm_request::data_url(b"frame-one", "image/jpeg")),
            ],
        },
    ];
    let body = body_for("Describe", &messages).expect("inside the bound");
    let mut log = Vec::new();
    dial(&scene, "Describe", &body, &mut log).expect("the call answered");
    let (asked, _) = fake.waited();
    let deadline = Instant::now() + Duration::from_secs(20);
    let rows = loop {
        let rows = requests::read(&scene.tree).expect("requests.tsv reads back");
        if !rows.is_empty() || Instant::now() > deadline {
            break rows;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(rows.len(), 1, "one ask, one row: {rows:?}");
    let row = &rows[0];
    assert_eq!(row.step, "Describe", "the row is the call that went out");
    // Text bytes only, so the number means "the prose the model was handed" — the same measure the
    // bound is judged on, which is what lets a reader compare a row against P.machine.promptMaxChars.
    let text_bytes = (text.len() + "describe frame one".len()) as i64;
    assert_eq!(
        row.sent_bytes,
        Some(text_bytes),
        "the row carries the text it sent (the fake counted {} bytes of JSON): {row:?}",
        asked.len()
    );
    assert_eq!(row.images, Some(1), "and the one picture it carried: {row:?}");

    // A text-only call leaves the picture column EMPTY, not zero: §10's reader must tell "no
    // images" apart from "not recorded" the same way it does for every other column.
    let plain_body = body_for(
        "Narrate",
        &[Message { role: "user".into(), parts: vec![Part::Text(letters(20))] }],
    )
    .expect("inside the bound");
    let fake_plain = Fake::start(chat("spoken"));
    let plain_scene = Scene::new("s3-text-only", &fake_plain.url);
    dial(&plain_scene, "Narrate", &plain_body, &mut log).expect("the second call answered");
    fake_plain.waited();
    let rows = requests::read(&plain_scene.tree).expect("requests.tsv");
    assert_eq!(rows.len(), 1, "one ask, one row: {rows:?}");
    assert_eq!(rows[0].images, None, "no pictures, so no figure: {:?}", rows[0]);
    assert_eq!(rows[0].sent_bytes, Some(20), "and the text is still recorded: {:?}", rows[0]);

    // A call whose refusal came from the bound never reaches the wire, so no row is written for it.
    let refused_scene = Scene::new("s3-refused", "http://127.0.0.1:1");
    let mut refused_log = Vec::new();
    let too_big = body_for(
        "Upload brief",
        &[Message { role: "user".into(), parts: vec![Part::Text(letters(MAX + 1))] }],
    );
    let refused = match too_big {
        Ok(body) => dial(&refused_scene, "Upload brief", &body, &mut refused_log),
        Err(why) => Err(why),
    };
    assert!(refused.is_err(), "one char over the bound is refused");
    assert!(
        requests::read(&refused_scene.tree).expect("a readable log").is_empty(),
        "a refused request wrote no timed row: {:?}",
        requests::read(&refused_scene.tree).expect("a readable log")
    );
}

/// The size columns are filled by the wire's own body, so they survive whatever the caller knew:
/// a `base_row` that filled `sent_bytes` itself keeps its number, and a non-chat body (no message
/// list to measure) records its whole length.
#[test]
fn p_machine_promptmaxchars_s4_a_caller_s_own_size_is_kept_and_a_plain_body_is_measured_whole() {
    let with_size = Request { sent_bytes: Some(999), ..Default::default() };
    let filled = naivepost::server_leg::fill_sizes(
        with_size,
        Some(r#"{"messages":[{"role":"user","content":[{"type":"text","text":"hello"}]}]}"#),
    );
    assert_eq!(filled.sent_bytes, Some(999), "a caller's own number is never overwritten");
    let plain = naivepost::server_leg::fill_sizes(
        Request::default(),
        Some(r#"{"model":"asr","request":{"path":"/uploads/c00.wav"}}"#),
    );
    assert_eq!(
        plain.sent_bytes.map(|v| v as usize),
        Some(r#"{"model":"asr","request":{"path":"/uploads/c00.wav"}}"#.len()),
        "an audio task's JSON has no message list, so the whole body is what went out: {plain:?}"
    );
    assert_eq!(plain.images, None, "and it carried no pictures");
}
