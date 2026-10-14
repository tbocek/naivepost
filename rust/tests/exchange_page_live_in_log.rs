//! §7's page and its log line made by ONE real call: the loop closed end to end.
//!
//! `model_exchange_log.rs` pins the page format, `model_exchange_log_live.rs` the recorder's wiring,
//! `sec05_live_log_link.rs` which log line is clickable. What none of them did was join the two ends in
//! one run: a call that really goes out through the patience door must leave an HTML page under
//! `<session>/llm/`, put its lines in the shell's own log through the shell's own door, and the line
//! that names the page must open THAT file — not a name typed twice in a test.
//!
//! The seams are the program's own, so nothing here is scripted into the app: the LLM address is a fake
//! server on a loopback port (`speak_leg`'s note: the address IS the seam, no program code can tell the
//! difference), and the session folder is whatever the process's cwd resolves to, which is what BOTH
//! `Tree::llm_dir()` writes into and `log_link::session_now()` resolves the link against.

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::exchange_leg;
use naivepost::exchanges;
use naivepost::layout::Tree;
use naivepost::llm_retry_leg;
use naivepost::requests::{Outcome, Request};
use naivepost::requests::Service;
use naivepost::services::Endpoint;
use naivepost::startup;
use naivepost::ui;

/// One GTK application, one `activate`, exactly one `#[test]`: this round touches the process-wide cwd
/// and the shell's shared log, so it runs once and reports through the flag asserted at the end.
static GTK: std::sync::Once = std::sync::Once::new();
static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A root only this check touches: pid AND thread name, the pattern `narrate_surface_widgets.rs` uses,
/// because every binary of one `cargo test` run shares a pid.
fn root() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "np-exch-page-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("round").replace(':', "_")
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("temp root created");
    dir
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

/// A chat body in the shape `llm_request::body` writes: one system message, the thinking switch off,
/// and no `stream` key — absent means false to every server in §1's table, so this call files its
/// exchange whole at the end rather than growing an open section.
fn ask_body(text: &str) -> String {
    serde_json::json!({
        "model": "qwen",
        "messages": [
            {"role": "system", "content": "You describe frames."},
            {"role": "user", "content": text},
        ],
        "enable_thinking": false,
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

/// One fake LLM on a port nothing else holds, answering every ask with `reply`. HELD for the whole round:
/// a released ephemeral port goes back to the pool, where another binary can take it and this call would
/// then dial an unrelated answerer.
struct FakeLlm {
    url: String,
}

impl FakeLlm {
    fn answer_with(reply: &str) -> FakeLlm {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let body = answer(reply);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
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
                    if let Some(value) = line
                        .strip_prefix("Content-Length:")
                        .or_else(|| line.strip_prefix("content-length:"))
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body_in = vec![0u8; length];
                use std::io::Read;
                let _ = reader.read_exact(&mut body_in);
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
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

/// Every line in the log's buffer, one per line, in order.
fn lines(view: &gtk::TextView) -> Vec<String> {
    let buffer = view.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    text.lines().map(str::to_string).collect()
}

/// Which lines carry §7's link tag.
fn tagged(view: &gtk::TextView) -> Vec<i32> {
    let buffer = view.buffer();
    let tag = buffer
        .tag_table()
        .lookup("exchange-link")
        .expect("painting the link line draws its tag");
    (0..buffer.line_count())
        .filter_map(|line| {
            let iter = buffer.iter_at_line(line)?;
            iter.has_tag(&tag).then_some(line)
        })
        .collect()
}

#[test]
fn sec_03_shell_7_the_model_exchange_log_llm_s8_a_live_call_writes_the_page_the_log_links_to() {
    // Pin the cwd FIRST: the session folder is what both halves of this item resolve against — the page
    // is written under `<session>/llm/` and the log's link line is resolved against the same folder at
    // click time — so one cwd choice puts both on the same path without either being told where to look.
    let root = root();
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    GTK.call_once(|| {
        // Built here because `connect_activate`'s closure must own it (`'static`): the folder already
        // exists on disk from the cwd pin above, so this only names it.
        let tree = Tree::new(startup::session_dir(&std::env::current_dir().unwrap()))
            .expect("the session folder ends in .naivepost");
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(move |app| {
            let fake = FakeLlm::answer_with("a terminal window");
            release_last_window();
            let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
            let window = ui::build_window(app, &model, "Prepare");
            window.present();
            hold_last_window(window.clone());
            let view = ui::log_view(&window).expect("the shell's log has a text view");

            // §7's page boundary, through the door every ▶ makes (`runqueue::start_run` calls this):
            // a run is the calls between two resets, and the page is named for the first of them.
            exchange_leg::note_run_started();
            // ONE real call, out through the patience door and answered by the fake over a socket. Its
            // log sink is the SHELL's own door, so the lines land in the window's log and repaint the
            // expander exactly as a live run's do — the test never touches the buffer.
            llm_retry_leg::chat(
                "Describe",
                &tree,
                &base("qwen"),
                &fake.endpoint(),
                &ask_body("what is on screen?"),
                &|| false,
                &naivepost::ui::note_exchange_line,
            )
            .expect("the fake answers");

            // --- the page exists, named MMDD-HHMMSS-Describe.html -----------------------------------
            let pages: Vec<String> = std::fs::read_dir(tree.llm_dir())
                .expect("the run wrote its llm/ folder")
                .map(|entry| entry.expect("an entry").file_name().to_string_lossy().to_string())
                .collect();
            assert_eq!(pages.len(), 1, "one page for the run: {pages:?}");
            let name = &pages[0];
            assert!(name.ends_with("-Describe.html"), "{name}");
            let stamp = name.trim_end_matches("-Describe.html");
            assert_eq!(stamp.len(), 14, "MMDD-HHMMSS, six digit groups: {name}");
            assert!(stamp.chars().all(|c| c.is_ascii_digit()), "{name}");
            let html = std::fs::read_to_string(tree.llm_dir().join(name)).expect("the page reads back");
            assert!(html.contains("<h1>1. Describe</h1>"), "{html}");
            assert!(html.contains("qwen"), "the meta line names the model: {html}");
            assert!(html.contains("execute"), "the body's thinking switch is off: {html}");
            assert!(html.contains("a terminal window"), "the reply is on the page: {html}");
            assert!(html.contains("You describe frames."), "the request is on the page: {html}");

            // --- the log shows this call's lines, last in the buffer ---------------------------------
            // The shell may have logged before the call (the fixture's red line restores one), so the
            // exchange lines are counted from the END: this is about what the recorder wrote, not about
            // what the window printed on its way up. §7's two per-call lines plus the reply preview make
            // three at the end; the once-per-run link rode in just ahead of them with the send line.
            let said = lines(&view);
            let first = said.len() - 3;
            assert!(
                said[first].ends_with("of text and 0 image(s) went to the LLM"),
                "the send line: {said:?}"
            );
            assert!(
                said[first + 1].contains("came back in"),
                "then the verdict line: {said:?}"
            );
            assert!(
                said[first + 2].starts_with(">>>   the reply begins: a terminal window"),
                "and the preview closes the call: {said:?}"
            );
            let link = exchanges::link_line(&format!("{}/{}", exchanges::LLM_DIR, name));
            let links: Vec<usize> = said
                .iter()
                .enumerate()
                .filter_map(|(at, line)| (line == &link).then_some(at))
                .collect();
            assert_eq!(links.len(), 1, "once per run, naming the page that exists: {said:?}");
            let link_line = links[0] as i32;
            assert!(
                link_line <= first as i32,
                "the link belongs to this call's own opening, not to some later news: {said:?}"
            );
            assert_eq!(tagged(&view), vec![link_line], "and exactly that line is the link");

            // --- the loop: clicking that line opens the file the call just wrote ---------------------
            let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let watched = opened.clone();
            let door = move |uri: &str| {
                watched.borrow_mut().push(uri.to_string());
                Ok(())
            };
            let uri = ui::log_link::activate_line(&view, link_line, &door)
                .expect("the run's page line opens");
            assert!(
                uri.ends_with(&format!("{}/{name}", exchanges::LLM_DIR)),
                "the link points at the page under the session: {uri}"
            );
            assert_eq!(*opened.borrow(), vec![uri.clone()], "one activation, one thing opened");
            let target = std::path::Path::new(
                uri.strip_prefix("file://").expect("a file:// URI"),
            );
            assert!(target.is_file(), "the line the log links to IS the file on disk: {}", target.display());
            assert_eq!(
                target,
                tree.llm_dir().join(name).as_path(),
                "and it is the very page the call wrote"
            );

            // End the run's page so a later check of this process does not inherit it.
            exchange_leg::reset_run_for_test();
            window.close();
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(RAN.load(std::sync::atomic::Ordering::SeqCst), "the widget round never ran");
}
