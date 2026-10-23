//! §7's live half, proven on the WIRE rather than on a builder: the page is written when the request goes out and
//! then GROWS while the reply streams.
//!
//! The older checks in `model_exchange_log_live.rs` cover everything except the one claim this file exists for.
//! Its stream case answers with ONE write, so that page is legitimately filed whole — it asserts exactly that.
//! Here the fake writes its event stream in TWO flushed chunks and then STOPS until the test tells it to continue,
//! which is the only way to look at the page while an answer is genuinely half-arrived. A recorder that files only
//! at the end cannot pass either check below, and neither can one that opens the section but never grows it.
//!
//! No timing is guessed: the fake blocks on a channel between chunks and the test blocks on a channel waiting for
//! the first word, so the mid-call read happens because the program reached that state, not because a sleep was
//! long enough. The bounded polls are failure detectors only.

use crate::common;
use common::temp_dir_created as temp_dir;

use naivepost::exchange_leg;
use naivepost::layout::Tree;
use naivepost::llm_retry_leg;
use naivepost::requests::{Outcome, Request, Service};
use naivepost::services::Endpoint;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

/// A root only this binary touches, so no other check reads these pages.
fn tree(tag: &str) -> Tree {
    Tree::new(temp_dir(&format!("f7s-{}-{tag}", std::process::id())).join("show.naivepost"))
        .expect("a folder ending in .naivepost is a project")
}

/// The row every chat call carries.
fn base(model: &str) -> Request {
    Request {
        started: String::new(),
        run: String::new(),
        step: "Narrate".into(),
        job: String::new(),
        service: Service::Llm,
        model: model.into(),
        kind: "chat".into(),
        sent_bytes: None,
        images: None,
        received_bytes: None,
        tokens_in: None,
        tokens_out: None,
        first_byte_s: None,
        on_wire_s: None,
        thinking_s: None,
        outcome: Outcome::Ok,
        attempt: 1,
        wait_s: None,
    }
}

/// A chat body in `llm_request::body`'s shape, asking for a stream — without `"stream": true` there is nothing
/// incremental to show and §7's live section never opens (see that module's note on the switch).
fn ask_stream(prompt: &str) -> String {
    serde_json::json!({
        "model": "qwen",
        "messages": [
            {"role": "system", "content": "You narrate this session."},
            {"role": "user", "content": prompt},
        ],
        "enable_thinking": false,
        "temperature": 0.2,
        "stream": true,
    })
    .to_string()
}

/// One SSE `data:` line carrying `piece` as this chunk's words.
fn sse(piece: &str) -> String {
    format!(
        "data: {}\r\n",
        serde_json::json!({"choices": [{"delta": {"content": piece}}]})
    )
}

/// The ONE page this run wrote, by name. Bounded: a call that produced no page must fail with a message, not hang.
fn only_page(dir: &std::path::Path) -> std::path::PathBuf {
    for _ in 0..400 {
        // `read_dir` fails until the program has created `llm/`, which is exactly what the early write does.
        if let Some(found) = std::fs::read_dir(dir)
            .ok()
            .and_then(|entries| entries.flatten().next())
        {
            return found.path();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no exchange page appeared under {}", dir.display());
}

/// Read the page until it satisfies `seen`, handing back the text that did. This is the handshake, not a wait:
/// the loop ends the moment the program writes what was asked for, and only fails after 2s of nothing happening.
fn page_until(path: &std::path::Path, seen: impl Fn(&str) -> bool) -> String {
    let mut text = String::new();
    for _ in 0..200 {
        text = std::fs::read_to_string(path).expect("the page is readable");
        if seen(&text) {
            return text;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the page never said what §7 promises it would:\n{text}")
}

/// A fake LLM that streams its answer in two flushed chunks and holds the third until told to go on.
///
/// # Why it waits instead of pausing
///
/// The check is about a page caught mid-answer. A fixed pause makes the test read the page at a time unrelated to
/// what the program has done; here the fake cannot write the second fragment until the test asks, so the read it
/// forces is guaranteed to be mid-stream. Both directions are channels: `first_landed` says the first chunk is on
/// the wire, `continue_on` releases the rest.
struct StalledLlm {
    url: String,
    continue_on: mpsc::Sender<()>,
    first_landed: mpsc::Receiver<()>,
    /// Signalled once the server has read the whole request off the wire.
    arrived: mpsc::Receiver<()>,
    /// The fake keeps its socket while ANY copy of this survives. Never sent on: holding the value is the whole of
    /// its job, and dropping it is what ends the call.
    #[allow(dead_code)]
    keep_open: mpsc::Sender<()>,
}

/// A fake LLM that takes the request and then says NOTHING at all, holding the socket open until told otherwise.
///
/// # Why it holds rather than answering 500
///
/// The page's pending state only exists while the call is running, so a server that refuses immediately puts the
/// verdict on the page before a test could look. Holding the connection is what makes "the first write is the
/// request half" observable. `keep_open` is the release: the acceptor keeps one receiver and the test keeps another,
/// so the socket dies only when the test decides the answer is never coming.
struct SilentLlm {
    url: String,
    arrived: mpsc::Receiver<()>,
    keep_open: mpsc::Sender<()>,
}

impl SilentLlm {
    fn silent() -> SilentLlm {
        exchange_leg::reset_run_for_test();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let (arrived_tx, arrived_rx) = mpsc::channel::<()>();
        let (keep_open, held) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let mut stream = listener
                .incoming()
                .next()
                .expect("one connection")
                .expect("a socket");
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
            let mut asked = vec![0u8; length];
            let _ = reader.read_exact(&mut asked);
            let _ = arrived_tx.send(());
            // Headers and nothing after them: no chunk ever arrives, and this holds the socket until `held` ends.
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
            );
            let _ = stream.flush();
            let _ = held.recv();
        });
        SilentLlm {
            url,
            arrived: arrived_rx,
            keep_open,
        }
    }

    fn endpoint(&self) -> Endpoint {
        Endpoint {
            url: self.url.clone(),
            key: String::new(),
        }
    }
}

impl StalledLlm {
    fn stalled(first: &str, second: &str, last: &str) -> StalledLlm {
        // One page per check: the run page is per-thread and cargo runs these two cases as threads of one process,
        // so a check says plainly that its call starts a run of its own.
        exchange_leg::reset_run_for_test();
        let (keep_open, held) = mpsc::channel::<()>();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let (land_tx, land_rx) = mpsc::channel::<()>();
        let (arrived_tx, arrived_rx) = mpsc::channel::<()>();
        let (first, second, last) = (first.to_string(), second.to_string(), last.to_string());
        std::thread::spawn(move || {
            let mut stream = listener
                .incoming()
                .next()
                .expect("one connection")
                .expect("a socket");
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
            let mut asked = vec![0u8; length];
            let _ = reader.read_exact(&mut asked);
            // The request arrived: only now may the test go looking for its page, so "the first write is the
            // request half" is settled by the wire, not by who happened to poll first.
            let _ = arrived_tx.send(());
            // Chunked and content-length-free: what forces the client into many reads rather than one.
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
            );
            let _ = stream.flush();
            let chunk = |text: &str| format!("{:x}\r\n{}\r\n", text.len(), text);
            let _ = stream.write_all(chunk(&sse(&first)).as_bytes());
            let _ = stream.flush();
            // The first fragment is on the wire: from here the page owes §7 a growing section.
            let _ = land_tx.send(());
            // And now NOTHING until the test has looked.
            let _ = go_rx.recv();
            let _ = stream.write_all(chunk(&sse(&second)).as_bytes());
            let _ = stream.flush();
            // `[DONE]` rides with the last word, as it does on a server that finishes when it decides it is done.
            let _ =
                stream.write_all(chunk(&format!("{}data: [DONE]\r\n\r\n", sse(&last))).as_bytes());
            let _ = stream.flush();
            let _ = stream.write_all(b"0\r\n\r\n");
            let _ = stream.flush();
            // Hold the connection open until the test lets go, so the client sees a finished body rather than a
            // reset mid-read: what is being measured is WHEN words land, not whether the socket survived.
            let _ = held.recv();
        });
        StalledLlm {
            url,
            continue_on: go_tx,
            first_landed: land_rx,
            arrived: arrived_rx,
            keep_open,
        }
    }

    fn endpoint(&self) -> Endpoint {
        Endpoint {
            url: self.url.clone(),
            key: String::new(),
        }
    }
}

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s8_the_page_grows_while_the_reply_streams() {
    let tree = tree("grow");
    let fake = StalledLlm::stalled("Split ", "at twelve", " seconds.");
    let log: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let thread = std::thread::spawn({
        let tree = tree.clone();
        let endpoint = fake.endpoint();
        let log = std::sync::Arc::clone(&log);
        move || {
            llm_retry_leg::chat(
                "Narrate",
                &tree,
                &base("qwen"),
                &endpoint,
                &ask_stream("say it over"),
                &|| false,
                &|line| log.lock().expect("the log").push(line.to_string()),
            )
        }
    });

    fake.arrived
        .recv_timeout(Duration::from_secs(5))
        .expect("the fake took the request");
    let path = only_page(&tree.llm_dir());
    assert!(
        path.to_string_lossy().ends_with("-Narrate.html"),
        "named for its first step: {path:?}"
    );
    // The request half is on disk before any answer: that early write is the whole point of writing early.
    let opened = std::fs::read_to_string(&path).expect("the page exists once the call went out");
    assert!(
        opened.contains("You narrate this session."),
        "the ask is on the page: {opened}"
    );

    // Mid-call, with only the first fragment delivered: inside a `<pre>`, the second nowhere, no pending marker.
    fake.first_landed
        .recv_timeout(Duration::from_secs(5))
        .expect("the fake wrote its first chunk");
    let mid = page_until(&path, |page| page.contains("Split"));
    let open = mid
        .rfind("<pre>")
        .expect("the streamed reply sits in a pre");
    let close = mid[open..]
        .find("</pre>")
        .map(|at| open + at)
        .expect("and that pre is closed");
    assert!(
        mid[open + "<pre>".len()..close].contains("Split"),
        "the words are inside the pre:\n{mid}"
    );
    assert!(
        !mid.contains("at twelve"),
        "the second fragment has not been sent yet:\n{mid}"
    );
    assert!(
        !mid.contains(naivepost::exchanges::REPLY_PENDING),
        "words landed, so nothing is pending:\n{mid}"
    );
    assert!(
        !mid.contains("came back"),
        "the call is not over, so no verdict line on the page:\n{mid}"
    );

    // Let the answer finish, on the test's signal rather than on a timer.
    fake.continue_on
        .send(())
        .expect("the fake is still listening");
    thread
        .join()
        .expect("the call completes")
        .expect("the fake answered");
    drop(fake);
    let whole = std::fs::read_to_string(&path).expect("the finished page");
    assert!(
        whole.contains("Split at twelve seconds."),
        "the rewrite holds the whole reply:\n{whole}"
    );
    // Replaced, not repeated: the fragment that grew the section appears once in the finished page.
    assert_eq!(
        whole.matches("at twelve").count(),
        1,
        "the streamed fragment is replaced, not repeated:\n{whole}"
    );
    assert!(
        !whole.contains(naivepost::exchanges::REPLY_PENDING),
        "closed: {whole}"
    );
    assert!(
        !whole.contains("streamed"),
        "the internal marker never survives to disk: {whole}"
    );
    assert!(
        whole.trim_end().ends_with("</html>"),
        "a whole document: {whole}"
    );
    // The closed section's meta line is the OUTCOME (§7), which is what replaced `reply pending`.
    let meta = whole
        .split("<p class=\"meta\">")
        .nth(1)
        .expect("the section has a meta line")
        .split("</p>")
        .next()
        .expect("and it closes");
    assert!(
        meta.contains("qwen") && meta.contains("execute"),
        "model and mode: {meta}"
    );
    assert!(
        !meta.contains("is not answering"),
        "the call succeeded, so no failure wording: {meta}"
    );
    // And the log got §7's per-call lines, send first.
    let lines = log.lock().expect("the log").clone();
    assert!(
        lines.iter().any(|line| line.contains("went to the LLM")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("came back")),
        "{lines:?}"
    );
    assert!(
        lines.iter().position(|l| l.contains("went to the LLM"))
            < lines.iter().position(|l| l.contains("came back")),
        "the send line comes before the verdict: {lines:?}"
    );
}

#[test]
fn sec_09_llm_and_tools_7_exchange_log_s8_a_pending_section_is_on_disk_before_any_answer() {
    // A server that takes the request and NEVER answers. Everything the page can show before an answer is what this
    // pins: the section, the sent prompt, and `reply pending` — which together prove the first write was the
    // request half and not the exchange. Then the socket dies, and the same page has to carry the reason.
    let tree = tree("silent");
    let fake = SilentLlm::silent();
    let log: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let thread = std::thread::spawn({
        let tree = tree.clone();
        let endpoint = fake.endpoint();
        let log = std::sync::Arc::clone(&log);
        move || {
            llm_retry_leg::chat(
                "Narrate",
                &tree,
                &base("qwen"),
                &endpoint,
                &ask_stream("say it over"),
                &|| false,
                &|line| log.lock().expect("the log").push(line.to_string()),
            )
        }
    });

    // The request reached the server; from here the page's only possible content is the ask.
    fake.arrived
        .recv_timeout(Duration::from_secs(5))
        .expect("the silent server took the request");
    let path = only_page(&tree.llm_dir());
    let pending = page_until(&path, |page| {
        page.contains(naivepost::exchanges::REPLY_PENDING)
    });
    assert!(
        pending.contains("say it over"),
        "the sent prompt is on disk before any answer:\n{pending}"
    );
    assert!(
        pending.contains("Narrate"),
        "the section names its step:\n{pending}"
    );
    assert!(
        !pending.contains("came back"),
        "nothing has come back yet:\n{pending}"
    );
    assert!(
        pending.contains("<pre>"),
        "and the ask itself is readable:\n{pending}"
    );

    // Release the server: the read dies, so the call ends and the same page must report why it did.
    drop(fake.keep_open);
    let outcome = thread
        .join()
        .expect("the call came back with an answer or a reason");
    assert!(
        outcome.is_err(),
        "a server that stops answering must not read as success: {outcome:?}"
    );
    let whole = std::fs::read_to_string(&path).expect("the page after the failure");
    assert!(
        !whole.contains(naivepost::exchanges::REPLY_PENDING),
        "the pending marker is gone: {whole}"
    );
    assert!(
        whole.contains("failed") || whole.contains("not answering"),
        "the page says it failed:\n{whole}"
    );
    assert!(
        whole.contains("say it over"),
        "and keeps the ask it failed on:\n{whole}"
    );
    let lines = log.lock().expect("the log").clone();
    let send = lines.iter().position(|l| l.contains("went to the LLM"));
    let verdict = lines
        .iter()
        .position(|l| !l.contains("went to the LLM") && l.starts_with(">>> Narrate"));
    assert!(
        send.is_some_and(|send| verdict.is_some_and(|verdict| send < verdict)),
        "the send line arrived before the verdict: {lines:?}"
    );
}
