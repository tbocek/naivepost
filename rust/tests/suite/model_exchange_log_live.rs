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

use crate::common;
use common::temp_dir_created as temp_dir;

use adw::prelude::*;
use std::time::Duration;
use gtk4 as gtk;
use naivepost::exchange_leg;
use naivepost::exchanges;
use naivepost::layout::Tree;
use naivepost::llm_retry_leg;
use naivepost::requests::{Outcome, Request};
use naivepost::services::Endpoint;
use naivepost::requests::Service;
use naivepost::startup;
use naivepost::ui;

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
    ask_stream(text, thinking, false)
}

/// The same, with §9 §1's `stream` switch spelled out or left absent. Which of the two it is decides whether
/// §7's live page opens at all, so a check that cares about the live half says so here rather than getting it
/// from a default.
fn ask_stream(text: &str, thinking: bool, streaming: bool) -> String {
    let mut asked = serde_json::json!({
        "model": "qwen",
        "messages": [
            {"role": "system", "content": "You describe frames."},
            {"role": "user", "content": text},
        ],
        "enable_thinking": thinking,
        "temperature": 0.2,
    });
    // Absent means false to every server in §1's table, and `llm_request::body` omits it for exactly that
    // reason, so a non-streaming ask here has to omit it too rather than write `false`.
    if streaming {
        asked["stream"] = serde_json::json!(true);
    }
    asked.to_string()
}

/// A reply a llama.cpp-style server sends to a non-streaming chat call.
fn answer(content: &str) -> String {
    serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
    })
    .to_string()
}

/// One `data:` line of an event stream carrying `piece` as this chunk's words.
fn sse(piece: &str) -> String {
    let payload = serde_json::json!({"choices": [{"delta": {"content": piece}}]}).to_string();
    format!("data: {payload}\n")
}

/// An event stream whose pieces are the ones given, ending with `[DONE]` like every real one.
fn sse_stream(pieces: &[&str]) -> String {
    // `[DONE]` is written by the fake with the last piece, not part of this string: see `SlowLlm::streaming`.
    pieces.iter().map(|piece| sse(piece)).collect::<String>()
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
        said.iter().any(|l| l == ">>> Describe: 0.2 kB of text and 0 image(s) went to the LLM")
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
        // §9 §1 recognises a stream by the REPLY's content-type, but §7's live section is opened when the
        // request goes out, and only a body that asked for a stream gets one (see the `ask_stream` note).
        &ask_stream("voice this", true, true),
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

// ---------------------------------------------------------------------------
// §1's other half of §7: of all the lines the log holds, exactly ONE may be a
// link — the run's exchange page — and clicking anything else must open nothing.
// These drive `naivepost::ui::log_link`'s own doors.
// ---------------------------------------------------------------------------

/// The lines a run's log holds: §7's two per call, then the once-per-run link.
fn log_batch(page_rel: &str) -> Vec<String> {
    vec![
        ">>> Describe: 451.4 kB of text and 0 image(s) went to the LLM".to_string(),
        ">>> Describe: 28.6 kB came back in 2m39s".to_string(),
        exchanges::link_line(page_rel),
    ]
}

/// GTK once per binary, on the main thread: `just test` runs cases on several, and GTK is
/// single-threaded — a second init from another thread is refused outright. So the widget cases below
/// live in ONE test that runs them in order, and `common`'s single-window slot keeps a view's window
/// alive for the binary's whole life.
static GTK: std::sync::Once = std::sync::Once::new();

fn gtk_up() {
    GTK.call_once(|| {
        gtk::init().expect("GTK starts under xvfb");
    });
}

/// The window every log view here belongs to. A text view outside a window cannot be asked for its
/// buffer, so each case hands its view to this one — and GTK keeps only one default window per process,
/// which is why this is a slot and not a fresh window per case.
fn log_window() -> adw::ApplicationWindow {
    gtk_up();
    let window = adw::ApplicationWindow::builder().build();
    window.set_default_size(900, 600);
    window.present();
    common::hold_last_window(window.clone());
    window
}

/// The page name §7 writes: under `llm/`, `.html`, named for the step that asked first.
fn page_name(step: &str) -> String {
    format!("{}/{step}-page.html", exchanges::LLM_DIR)
}

/// One headless log view: a TextView, the door the shell installs on it, and the openings it recorded.
struct LogView {
    view: gtk::TextView,
    opened: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl LogView {
    fn new(window: &adw::ApplicationWindow) -> LogView {
        gtk_up();
        let view = gtk::TextView::new();
        view.set_editable(false);
        window.set_child(Some(&view));
        LogView { view, opened: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())) }
    }

    /// The view with §7's door installed and the opening leg recorded instead of run.
    fn installed(window: &adw::ApplicationWindow) -> LogView {
        let made = LogView::new(window);
        let logged = std::rc::Rc::clone(&made.opened);
        ui::log_link::install_with(
            &made.view,
            std::rc::Rc::new(move |uri| {
                logged.borrow_mut().push(uri.to_string());
                Ok(())
            }),
        );
        made
    }

    fn buffer(&self) -> gtk::TextBuffer {
        self.view.buffer()
    }

    /// Paint the batch as the shell repaints it: through the module's own painter, cwd as the session.
    fn paint(&self, lines: &[String]) {
        ui::log_link::paint_lines(&self.buffer(), lines);
    }

    /// The lines carrying the link tag, by index.
    fn tagged(&self) -> Vec<i32> {
        let buffer = self.buffer();
        let tag = buffer
            .tag_table()
            .lookup("exchange-link")
            .expect("install drew the link tag");
        (0..buffer.line_count())
            .filter_map(|line| {
                let iter = buffer.iter_at_line(line)?;
                iter.has_tag(&tag).then_some(line)
            })
            .collect()
    }
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_names_only_the_run_page_line() {
    // §1: "Only one log line is a link ... no other path clickable." The test is that a line naming a
    // path of any other kind — or §7's own wording with a non-page tail — answers `None`.
    let session = std::path::Path::new("/tmp/show.naivepost/session");
    let rel = page_name("Describe");
    let uri = ui::log_link::target_of(&exchanges::link_line(&rel), session)
        .unwrap_or_else(|| panic!("{} should point at the run's page", exchanges::link_line(&rel)));
    assert_eq!(
        uri,
        format!("file://{}{rel}", session.display().to_string() + "/"),
        "the link is the page under THIS session's folder"
    );

    for not_a_link in [
        ">>> Describe: 451.4 kB of text and 0 image(s) went to the LLM".to_string(),
        format!("{} audio/pass.wav", exchanges::link_line("").trim_end_matches(": ")),
        format!("{} /tmp/elsewhere/page.html", exchanges::link_line("").trim_end_matches(": ")),
        exchanges::link_line(&format!("{}/page.txt", exchanges::LLM_DIR)),
        exchanges::link_line("page.html"),
    ] {
        assert!(
            ui::log_link::target_of(&not_a_link, session).is_none(),
            "not the run's page, so not clickable: {not_a_link}"
        );
    }
}

/// A server that answers in PIECES, pausing between them so a check can read the page mid-call.
///
/// The whole point of these checks is "what does the file look like WHILE the call runs", which a server that
/// writes its body in one `write_all` cannot show: ureq's first read would already return everything. So this
/// one writes each piece and sleeps, and hands back a channel that says when the LAST piece went out — a check
/// reads the page after the first pause and waits on that before it reads the finished page.
struct SlowLlm {
    url: String,
    /// Notified once the stream's last byte has been written (or the attempt died trying).
    done: std::sync::Arc<std::sync::Mutex<bool>>,
}

impl SlowLlm {
    fn streaming(body: String, pieces: usize, every: Duration) -> SlowLlm {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let done = std::sync::Arc::new(std::sync::Mutex::new(false));
        let writer_done = std::sync::Arc::clone(&done);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                use std::io::{BufRead, BufReader, Read, Write};
                let mut reader = BufReader::new(stream.try_clone().expect("a clone of the socket"));
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) =
                        line.strip_prefix("Content-Length:").or_else(|| line.strip_prefix("content-length:"))
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut asked = vec![0u8; length];
                let _ = reader.read_exact(&mut asked);
                // Chunked, no `content-length`: how a real server streams an answer whose length it does not
                // know yet, and what forces the client into many reads rather than one.
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
                );
                // Cut the body into `pieces` roughly equal parts at LINE boundaries, so every part is whole SSE
                // lines: §9 §1's reader needs complete lines, and this check is about WHEN they land, not about
                // a line split across two TCP segments (which `Delta`'s own partial-line rule covers elsewhere).
                // ONE SSE line per piece, which is what "pieces" means here: one chunk of the answer is one
                // `data:` line, so a check about WHEN words land gets exactly one word per pause. The stream's
                // own `[DONE]` marker and its blank terminator ride along with the last piece.
                let lines: Vec<&str> = body.lines().collect();
                debug_assert!(lines.len() >= pieces.max(1), "one line per piece at least: {lines:?}");
                for (index, line) in lines.iter().enumerate() {
                    // `\r\n`, not a bare `\n`: an SSE line ends with CRLF on the wire, and a client that has
                    // to wait for the CR before it can use the LF is a client whose first read yields nothing.
                    // The stream's `[DONE]` marker and its terminator ride with the LAST word, as they do on a
                    // real server that finishes writing the moment it decides the answer is over.
                    let block = if index + 1 == lines.len() { format!("{line}\r\ndata: [DONE]\r\n\r\n") } else { format!("{line}\r\n") };
                    let _ = stream.write_all(format!("{:x}\r\n{}\r\n", block.len(), block).as_bytes());
                    let _ = stream.flush();
                    std::thread::sleep(every);
                }
                let _ = stream.write_all(b"0\r\n\r\n");
                let _ = stream.flush();
                if let Ok(mut held) = writer_done.lock() {
                    *held = true;
                }
            }
        });
        SlowLlm { url, done }
    }

    fn endpoint(&self) -> Endpoint {
        Endpoint { url: self.url.clone(), key: String::new() }
    }

    /// Until the server has finished writing, however long it takes. Bounded so a dead server fails the check
    /// with a message instead of hanging the suite.
    fn waited(&self) -> bool {
        for _ in 0..400 {
            if *self.done.lock().expect("the flag") {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s9_the_page_grows_while_the_answer_is_still_arriving() {
    // §7's live half, measured on the file itself: open the page DURING the call and the request is there,
    // `reply pending` is gone, and SOME words have landed while the reply as a whole has not. Then the call ends
    // and the same page holds the finished exchange. A recorder that only files at the end passes nothing here.
    // The wait is for the FIRST word only: the fake pauses between pieces, so the moment one fragment has landed
    // the sentence cannot be on the page yet — no sleep, no luck.
    let tree = tree("s9-live");
    let pieces = ["terminal", " window", " on a desk"];
    let fake = SlowLlm::streaming(sse_stream(&pieces), pieces.len(), Duration::from_millis(120));
    let log = std::cell::RefCell::new(Vec::new());
    let thread = std::thread::spawn({
        let tree = tree.clone();
        let endpoint = fake.endpoint();
        move || {
            llm_retry_leg::chat(
                "Describe",
                &tree,
                &base("qwen"),
                &endpoint,
                &ask_stream("what is on screen?", false, true),
                &|| false,
                &|line| log.borrow_mut().push(line.to_string()),
            )
        }
    });

    // Mid-call: the page exists, holds the ask, and shows words without holding ALL of them.
    let name = loop {
        if let Ok(entries) = std::fs::read_dir(tree.llm_dir()) {
            if let Some(found) = entries.flatten().next() {
                break found.file_name().to_string_lossy().to_string();
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let path = tree.llm_dir().join(&name);
    assert!(name.ends_with("-Describe.html"), "{name}");
    // Read the page WHILE the call is still running: the fake pauses between pieces, so a page written at the
    // end could never satisfy what follows. Poll until words appear rather than sleeping a fixed time — the
    // first read can land before the socket's first chunk, and a sleep would make this check depend on luck.
    // Mid-call: the page exists, holds the ask, and shows words without holding ALL of them. The wait is for
    // the FIRST word only — waiting on `pieces[0]` would let the loop sleep past the moment worth seeing.
    let mut mid = String::new();
    for _ in 0..200 {
        mid = std::fs::read_to_string(&path).expect("the page is readable mid-call");
        if !mid.contains("reply pending") {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(mid.contains("You describe frames."), "the ask is on the page before the answer: {mid}");
    assert!(!mid.contains("reply pending"), "the reply has begun, so no pending marker: {mid}");
    // SOME but not ALL: one fragment landed, the sentence has not. Asserting only the first fragment would pass
    // trivially once the whole reply had arrived, which is the failure mode this check exists to catch.
    assert!(mid.contains("a"), "the first words are on the page: {mid}");
    assert!(
        !mid.contains("abbccc"),
        "the reply is still arriving, so the page cannot hold it whole yet: {mid}"
    );

    // And the call still finishes properly: same page, whole reply, no leftover marker of an open block.
    // Checked on the PAGE, never on `sent.body`: that is the raw event stream (`data: {"choices"…}` per word),
    // so the reassembled sentence is not a substring of it — §9 §1's reader is what turns it into prose.
    assert!(fake.waited(), "the fake finished writing");
    thread
        .join()
        .expect("the call thread did not panic")
        .expect("the fake answered");
    let end = std::fs::read_to_string(&path).expect("the page is readable after");
    // The reassembled reply, not the wire: a page that still shows `data:` framing means the live fragments
    // were left in place when the finished section should have replaced them.
    assert!(!end.contains("data:"), "the closed section holds the answer, not the stream: {end}");
    assert!(end.contains("terminal window on a desk"), "the reply is complete: {end}");
    assert!(!end.contains("reply pending"), "{end}");
    assert!(end.trim_end().ends_with("</html>"), "a whole document: {end}");
    // One page for the call, however many times it was rewritten.
    let pages: Vec<String> = std::fs::read_dir(tree.llm_dir())
        .expect("llm/")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(pages, vec![name.clone()], "one page, grown in place");
    exchange_leg::reset_run_for_test();
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s9_a_one_shot_ask_gets_no_live_section() {
    // Only a body that ASKED for a stream has chunks to show (§9 §1), so a plain chat call must still be filed
    // — exactly as before this item existed — and must not leave a section sitting open or a `pending` marker
    // behind. This is the other half of the rule: the live page is conditional, the recording is not.
    let tree = tree("s9-one-shot");
    let fake = FakeLlm::answer_with(vec![("application/json".into(), answer("nothing streamed"))]);
    let log = std::cell::RefCell::new(Vec::new());
    llm_retry_leg::chat(
        "Check",
        &tree,
        &base("qwen"),
        &fake.endpoint(),
        &ask_body("no stream please", false),
        &|| false,
        &|line| log.borrow_mut().push(line.to_string()),
    )
    .expect("the fake answers");

    let name = std::fs::read_dir(tree.llm_dir())
        .expect("llm/")
        .next()
        .expect("one page")
        .unwrap()
        .file_name()
        .to_string_lossy()
        .to_string();
    let html = std::fs::read_to_string(tree.llm_dir().join(name)).expect("the page");
    assert!(html.contains("nothing streamed"), "{html}");
    assert!(!html.contains("reply pending"), "a one-shot call is filed whole: {html}");
    assert!(html.trim_end().ends_with("</html>"), "{html}");
    exchange_leg::reset_run_for_test();
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_is_the_only_tagged_line() {
    // The four checks a live view needs run HERE in order, on one thread: GTK holds one default window
    // per process and refuses an init from a second thread, so they cannot be four `#[test]`s.
    let window = log_window();

    // (a) Exactly one line of the run's log is tagged: the page line, never the report lines.
    let session = root("s8-tagged");
    std::fs::create_dir_all(&session).expect("the session root exists to be cwd");
    std::env::set_current_dir(&session).expect("cwd is the session root");
    let view = LogView::installed(&window);
    view.paint(&log_batch(&page_name("Describe")));
    assert_eq!(
        view.tagged(),
        vec![2],
        "the run's page line is the ONE clickable line among the run's log"
    );
    assert_eq!(view.opened.borrow().len(), 0, "painting opens nothing");

    // (b) The page line opens its page; a report line opens nothing, however it is activated.
    let session = root("s8-activate");
    std::fs::create_dir_all(&session).expect("the session root exists to be cwd");
    std::env::set_current_dir(&session).expect("cwd is the session root");
    let view = LogView::installed(&window);
    view.paint(&log_batch(&page_name("Fix")));
    let page = format!(
        "file://{}{}",
        startup::session_dir(&session).display().to_string() + "/",
        page_name("Fix")
    );
    let opener = |uri: &str| -> Result<(), String> {
        view.opened.borrow_mut().push(uri.to_string());
        Ok(())
    };
    let opened = ui::log_link::activate_line(&view.view, 2, &opener);
    assert_eq!(opened.as_deref(), Some(page.as_str()), "the page line opens its page");
    assert_eq!(view.opened.borrow().len(), 1, "one activation, one opening");
    for plain in [0, 1] {
        let before = view.opened.borrow().len();
        assert_eq!(
            ui::log_link::activate_line(&view.view, plain, &opener),
            None,
            "line {plain} is a report, not a link"
        );
        assert_eq!(view.opened.borrow().len(), before, "and it opened nothing");
    }

    // (c) A page that cannot be opened is said out loud, and the activation still counts as the link's.
    let session = root("s8-refused");
    std::fs::create_dir_all(&session).expect("the session root exists to be cwd");
    std::env::set_current_dir(&session).expect("cwd is the session root");
    // `installed`, not `new`: the tag lives only as long as something installed §7's door on this view,
    // and a view with no door has no link line to activate.
    let view = LogView::installed(&window);
    view.paint(&log_batch(&page_name("Narrate")));
    ui::window::clear_logs_for_test();
    let opened = ui::log_link::activate_line(&view.view, 2, &|_uri| Err("no browser".to_string()));
    assert!(opened.is_some(), "the activation WAS the link's, even though nothing could open");
    let said: Vec<String> = ui::window::window_logs()
        .into_iter()
        .filter(|line| line.contains("could not open"))
        .collect();
    assert_eq!(said.len(), 1, "exactly one line about the failure: {said:?}");
    assert!(
        said[0].contains("Narrate-page.html") && said[0].contains("no browser"),
        "it names the page and says why: {}",
        said[0]
    );

    // (d) F0.10's Save as moves the whole session folder, so the page is resolved again at the click.
    let here = std::env::current_dir().expect("a cwd to go back to");
    let before = root("s8-moved-before");
    let after = root("s8-moved-after");
    for dir in [&before, &after] {
        std::fs::create_dir_all(dir).expect("both session roots exist");
    }
    std::env::set_current_dir(&before).expect("painted under the first folder");
    let view = LogView::installed(&window);
    view.paint(&log_batch(&page_name("Cut")));
    std::env::set_current_dir(&after).expect("the session moved");
    let opened = ui::log_link::activate_line(&view.view, 2, &|_uri| Ok(()));
    std::env::set_current_dir(&here).expect("the cwd goes back for the other checks");
    let page = opened.expect("the line is still the link after the move");
    assert!(
        page.starts_with(&format!("file://{}", startup::session_dir(&after).display())),
        "the link followed the folder: {page}"
    );
    assert!(
        !page.contains(&startup::session_dir(&before).display().to_string()),
        "and does not still point at the folder that no longer holds it: {page}"
    );
}
