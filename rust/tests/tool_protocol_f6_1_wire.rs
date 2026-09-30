//! F6.1 S5 over the wire: a server that refuses the `tools` field, dialled for real.
//!
//! `tests/tool_protocol_f6_1.rs` proves §2's rules with two closures. What it cannot prove is that a chat flow
//! ever honours them — and until this round none did: every ask built its body with `Some(tools)` and failed on
//! the first error. These tests point the SETTINGS address (the seam the program itself reads) at a loopback
//! fake that answers 400 to a request carrying a `tools` key, and assert what actually went out.
//!
//! One scenario per binary would be needed if the settings env were re-pointed here; instead the flows take the
//! endpoint from the settings file written once per fixture, so all three checks share this binary by using
//! three different ports and re-writing the settings between them (`point_at`, as `produce_upload_wire` does).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::produce_upload_call as call;
use naivepost::project::Publish;
use naivepost::settings::{self, Conf};

const MODEL: &str = "refusing-model";

/// The wording a real llama.cpp-style server answers with. It has to contain "tool" because that is
/// [`naivepost::degraded::tools_refused`]'s test, and must not contain "stop"/"end of turn", which say the model
/// finished rather than that the field was refused.
const REFUSAL: &str = "the requested tools field is not supported by this server";

/// A §3.9 prose answer in the shape `produce_upload::peel` reads: a `TITLE:` label, then the description body.
/// ("Title:" is not a label -- `produce_details::labelled_value` wants >= 4 uppercase characters.)
const TITLE_ANSWER: &str = "TITLE: a win nobody expected\nA loss everybody saw coming.";
const TITLE_FIRST: &str = "TITLE: first attempt\nThe first description.";

// ---- the fake LLM -------------------------------------------------------------------------------

/// A loopback server that answers each request from a script and hands each request's body back to the test.
/// Replies are framed with `Connection: close` and the socket is shut immediately: ureq reads to EOF, and a
/// keep-alive connection stalls the whole binary (this happened before in `produce_upload_wire`).
struct FakeLlm {
    port: u16,
    received: Receiver<String>,
    script: Arc<Mutex<Vec<Reply>>>,
}

impl FakeLlm {
    fn start(replies: Vec<Reply>) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(replies));
        let serving = script.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let serving = serving.clone();
                let sender = sender.clone();
                std::thread::spawn(move || {
                    if let Some(body) = serve(stream, &serving) {
                        let _ = sender.send(body);
                    }
                });
            }
        });
        FakeLlm { port, received, script }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The next request's body. A timeout here means the flow never dialled, which is itself an answer worth
    /// failing on rather than a hang.
    fn next(&self) -> String {
        self.received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the call reached the fake LLM")
    }

    /// Every request body that arrived within `wait`, oldest first. Read as a set rather than one at a time:
    /// a refused call makes TWO connections for one round, so what matters is which bodies went out, not how
    /// many rounds the flow counted.
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

    #[allow(dead_code)]
    fn expect_silent(&self) {
        match self.received.recv_timeout(std::time::Duration::from_millis(400)) {
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            other => panic!("the server was dialled when nothing should have been: {other:?}"),
        }
    }
}

/// One scripted answer: `(status line, content type, body)`.
#[derive(Clone)]
struct Reply {
    status: u16,
    body: String,
}

fn serve(mut stream: TcpStream, script: &Arc<Mutex<Vec<Reply>>>) -> Option<String> {
    let taken = {
        let mut list = script.lock().expect("script lock");
        if list.is_empty() {
            return None;
        }
        // One entry per CONNECTION. A refused round makes two of them, and a server that stays down makes as
        // many as the flow attempts -- so a script longer than one entry hands out its entries in order and
        // repeats its LAST one, which is how "refuse once then answer" and "always dead" are both written.
        if list.len() > 1 {
            list.remove(0)
        } else {
            list[0].clone()
        }
    };
    let seen = serve_inner(&mut stream, &taken);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    seen
}

fn serve_inner(stream: &mut TcpStream, reply: &Reply) -> Option<String> {
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
    let reason = if reply.status == 400 { "Bad Request" } else { "OK" };
    let framed = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reply.body.len(),
        reply.body
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(String::from_utf8_lossy(&payload).to_string())
}

/// Wrap one answer's content in the chat envelope.
fn envelope(content: &str) -> String {
    serde_json::json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// The 400 body a schema-refusing server sends: an `error.message` naming the field.
fn refusal_body() -> String {
    serde_json::json!({ "error": { "message": REFUSAL, "type": "invalid_request_error" } }).to_string()
}

// ---- the scenario -------------------------------------------------------------------------------

/// Held while a scenario re-points the process-wide settings env.
fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

struct Fixture {
    root: PathBuf,
    tree: naivepost::layout::Tree,
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
    let root = std::env::temp_dir().join(format!("naivepost-f61wire-{tag}"));
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
    // No drawing server in any of these scenarios: left empty it falls back to the loopback default and a poll
    // would sit in that connect timeout for the whole binary.
    conf.sd_server = "http://127.0.0.1:1".to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree, _scenario: scenario }
}

fn cancelled() -> impl Fn() -> bool {
    || false
}

fn silent(_line: &str) {}

fn frames<'a>(moments: &'a [f64], starts: &'a [f64]) -> call::Frames<'a> {
    call::Frames { moments, starts }
}

/// Does this request body carry a `tools` field?
fn offers_tools(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .is_some_and(|value| value.get("tools").is_some())
}

/// The messages array of a request body, as text, so two requests can be compared apart from their `tools` key.
fn messages_of(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("messages").cloned())
        .map(|messages| messages.to_string())
        .unwrap_or_default()
}

// ---- S5 over the wire ---------------------------------------------------------------------------

/// Every refusal line this process has logged. §2's memory is per server, so the count is what proves it held
/// across two separate calls of the same flow.
fn window_refusals() -> Vec<String> {
    naivepost::ui::window::window_logs()
        .into_iter()
        .filter(|line| line.contains("the server refused the request with tools"))
        .collect()
}

/// S5: a server that answers the tool-bearing request with a refusal gets the IDENTICAL request again without
/// the field, and the step says the refusal once.
#[test]
fn f6_1_s5_a_server_that_refuses_tools_is_asked_again_without_them() {
    // Two connections for one refused round, and then the loop's own quiet-round nudge (§2 S3) makes a third --
    // the script repeats its last answer from there on. The claim this check makes is about the FIRST TWO.
    let llm = FakeLlm::start(vec![
        Reply { status: 400, body: refusal_body() },
        Reply { status: 200, body: envelope(TITLE_ANSWER) },
    ]);
    let scene = fixture("refused", &llm.url());
    let moments = [1.0f64];
    let starts = [0.0f64];
    let collected = call::call(
        &scene.tree,
        "BRIEF",
        &Publish::default(),
        &frames(&moments, &starts),
        false,
        &cancelled(),
        silent,
    );
    // The second answer is prose, so the flow ends with what it heard rather than failing.
    let got = collected.expect("the resend without tools answered");
    assert!(
        got.publish.thumb_title.contains("a win nobody expected"),
        "the resent answer is what the record got: {:?}",
        got.publish.thumb_title
    );

    let asked = llm.all(std::time::Duration::from_millis(600));
    assert!(asked.len() >= 2, "the refusal and its resend both reached the server: {}", asked.len());
    assert!(offers_tools(&asked[0]), "the first request offered the job's tools");
    assert!(!offers_tools(&asked[1]), "the resent request carries no `tools` field");
    assert_eq!(
        messages_of(&asked[0]),
        messages_of(&asked[1]),
        "S5 resends the IDENTICAL request minus the field"
    );

    // And the refusal was said exactly once, in §2's own words, on the log sink every headless flow writes to.
    let lines = window_refusals();
    assert_eq!(lines.len(), 1, "the refusal is logged exactly once: {lines:?}");
    assert!(lines[0].contains(REFUSAL), "the line quotes the server's reason: {}", lines[0]);
    assert!(lines[0].starts_with(&format!(">>> {}:", call::STEP)), "named by step: {}", lines[0]);
}

/// S5's memory: once this server has refused, the NEXT call asks without the field and says nothing more. That
/// is what stops narrate's three attempts logging the same refusal three times.
#[test]
fn f6_1_s5_a_refusal_is_remembered_so_the_next_attempt_sends_no_tools_field() {
    // The refused request is answered, and EVERY later round of this call gets the prose answer -- including the
    // loop's quiet-round nudge (§2 S3), whose reply is what the flow finally peels. Same title either way.
    let llm = FakeLlm::start(vec![
        Reply { status: 400, body: refusal_body() },
        Reply { status: 200, body: envelope(TITLE_FIRST) },
    ]);
    let scene = fixture("remembered", &llm.url());
    let moments = [1.0f64];
    let starts = [0.0f64];
    let run_once = || {
        call::call(
            &scene.tree,
            "BRIEF",
            &Publish::default(),
            &frames(&moments, &starts),
            false,
            &cancelled(),
            silent,
        )
    };
    let first = run_once().expect("the refused call still answered");
    assert!(first.publish.thumb_title.contains("first attempt"));
    assert_eq!(window_refusals().len(), 1, "first call said it once: {:?}", window_refusals());

    let second = run_once().expect("the second call answers without being refused");
    assert!(
        !second.publish.thumb_title.is_empty(),
        "the remembered-refusal call still got its title: {:?}",
        second.publish.thumb_title
    );
    // The refused call made two requests (refused + resend); this one makes ONE, and it carries no `tools`
    // field at all, because the refusal is remembered per server rather than re-offered and re-logged.
    let asked = llm.all(std::time::Duration::from_millis(600));
    assert!(asked.len() >= 3, "both calls reached the server: {}", asked.len());
    assert!(
        !offers_tools(asked.last().expect("at least one request")),
        "a remembered refusal means no `tools` field on the next call"
    );
    assert_eq!(
        window_refusals().len(),
        1,
        "said once for the SERVER, not once per attempt: {:?}",
        window_refusals()
    );
}

/// S5's other half, which the prototype got wrong: a transport death or a 500 on the first round is NOT a
/// refusal, so nothing is resent and no refusal line is logged.
#[test]
fn f6_1_s5_a_dead_server_is_not_read_as_a_refusal() {
    // A 500 whose message says nothing about tools: `degraded::tools_refused` must not read it as a refusal.
    let llm = FakeLlm::start(vec![Reply {
        status: 500,
        body: serde_json::json!({ "error": { "message": "internal server error" } }).to_string(),
    }]);
    let scene = fixture("dead", &llm.url());
    let moments = [1.0f64];
    let starts = [0.0f64];
    let outcome = call::call(
        &scene.tree,
        "BRIEF",
        &Publish::default(),
        &frames(&moments, &starts),
        false,
        &cancelled(),
        silent,
    );
    assert!(outcome.is_err(), "a dead server fails the call, it does not degrade it: {outcome:?}");
    let asked = llm.next();
    assert!(offers_tools(&asked), "the request went out with its tools: {asked}");
    // §3 (F6.2) now gives an answered-but-unusable 500 its one short retry, so the fake sees that second,
    // identical request. What this check is about is unchanged and still holds: none of them was a
    // resend-WITHOUT-tools, and no refusal was claimed for a server that merely died.
    let rest = llm.all(std::time::Duration::from_millis(600));
    assert!(
        rest.iter().all(|body| offers_tools(body)),
        "every request still offered the job's tools -- nothing was resent without the field: {} more",
        rest.len()
    );
    assert!(
        window_refusals().is_empty(),
        "no refusal is claimed for a server that merely died: {:?}",
        window_refusals()
    );
}
