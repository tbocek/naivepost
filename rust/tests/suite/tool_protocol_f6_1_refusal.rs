//! F6.1 S5 over the wire: a server that refuses the `tools` field is asked the identical request again without
//! it, the line is said once, the refusal is remembered (never re-offered, never re-said), and a dead server is
//! not a refusal. The rule lives in ONE leg, [`naivepost::llm_tool_refusal_leg::dial_honouring_refusal`], which
//! every step that offers tools dials through (the retake pool, the cut, captions, speeds, effects); these checks
//! drive that leg with the retake job's tools against a fake that answers 400 to any body carrying a `tools` key,
//! and assert on the bodies the fake ACTUALLY received. The transcript fixer offers no tools any more (one call,
//! the rows answered back, as the prototype asked it), so a refusing server costs it nothing: pinned last.
//!
//! One scenario per test function, each with its own fake port and its own settings folder handed to the door
//! explicitly (`tree`/`paths`), so no thread touches the process env.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use naivepost::settings::{self, Conf, Paths};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Wording a real llama.cpp-style server answers with. It must contain "tool" (that is
/// `degraded::tools_refused`'s test) and must not contain "stop"/"end of turn", which say the model finished.
const REFUSAL: &str = "the requested tools field is not supported by this server";

/// The four ASR rows a fix scene starts from — the same shape `tests/suite/fix_call.rs` uses.
const SOURCE_ROWS: [(&str, &str, &str, &str); 4] = [
    ("0.00", "1.50", "S1", "so the algorithum recieves the input"),
    ("1.50", "3.25", "S1", "and writes it down again"),
    ("3.25", "5.00", "S1", "wich is where it gets faster"),
    ("5.00", "6.75", "S1", "than doing it twice"),
];
const LANE: &str = "lecture";

/// The fixer's answer: the four rows back with line 1 respelled, as the content of a chat reply.
fn rows_answer() -> String {
    let rows: Vec<String> = SOURCE_ROWS
        .iter()
        .enumerate()
        .map(|(index, (start, end, speaker, text))| {
            let text = if index == 0 {
                "so the algorithm receives the input"
            } else {
                text
            };
            format!("{start}\t{end}\t{speaker}\t{text}")
        })
        .collect();
    serde_json::json!({ "choices": [ { "message": { "content": rows.join("\n") } } ] }).to_string()
}

/// A plain answer for the retake-tool asks below.
fn plain_answer() -> String {
    serde_json::json!({ "choices": [ { "message": { "content": "{\"abandoned\":[]}" } } ] })
        .to_string()
}

// ---- the refusing fake ---------------------------------------------------------------------------

/// A loopback chat server that answers 400 to any request whose body carries a `tools` key and 200 otherwise,
/// recording every body it saw. Replies are framed `Connection: close` and the socket is shut: ureq reads to EOF.
struct RefusingLlm {
    url: String,
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl RefusingLlm {
    fn start(answer: String) -> RefusingLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = seen.clone();
        let held = answer.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let recorder = recorder.clone();
                let held = held.clone();
                std::thread::spawn(move || {
                    if let Some(body) = serve(stream, &held) {
                        if let Ok(mut row) = recorder.lock() {
                            row.push(body);
                        }
                    }
                });
            }
        });
        RefusingLlm {
            url: format!("http://127.0.0.1:{port}"),
            seen,
        }
    }

    /// Every body recorded, waiting for at least `count` to arrive.
    fn waited(&self, count: usize) -> Vec<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let seen = self.seen.lock().expect("the record lock").clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn serve(mut stream: TcpStream, answer: &str) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
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
    let text = String::from_utf8_lossy(&body).to_string();
    let refuses = offers_tools(&text);
    let (status, reason, payload) = if refuses {
        (400u16, "Bad Request", refusal_body())
    } else {
        (200u16, "OK", answer.to_string())
    };
    let framed = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(text)
}

fn refusal_body() -> String {
    serde_json::json!({ "error": { "message": REFUSAL, "type": "invalid_request_error" } })
        .to_string()
}

fn offers_tools(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .is_some_and(|value| value.get("tools").is_some())
}

/// The messages array as text, so two requests can be compared apart from their `tools` key.
fn messages_of(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("messages").cloned())
        .map(|messages| messages.to_string())
        .unwrap_or_default()
}

// ---- the scene ----------------------------------------------------------------------------------

/// One recording, its transcript on disk, and its own settings folder aimed at one fake. Nothing here touches
/// the process env, so the scenarios can run as threads of one binary.
struct Scene {
    root: PathBuf,
    paths: Paths,
    llm: String,
}

impl Scene {
    fn new(label: &str, llm_url: &str) -> Scene {
        let root = std::env::temp_dir().join(format!(
            "np-f61refuse-{label}-{}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let session = root.join("session.naivepost");
        std::fs::create_dir_all(&session).expect("the session folder");
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let tree = naivepost::layout::Tree::new(&session).expect("the tree");
        let lines: Vec<naivepost::textfmt::Line> = SOURCE_ROWS
            .iter()
            .map(|(start, end, speaker, text)| naivepost::textfmt::Line {
                start: start.parse().expect("a number"),
                end: end.parse().expect("a number"),
                speaker: (*speaker).to_string(),
                text: (*text).to_string(),
            })
            .collect();
        naivepost::textfmt::write_lines(&lines, &tree.transcript_tsv(LANE))
            .expect("the transcript");
        let mut conf = Conf::default();
        conf.server = llm_url.to_string();
        conf.model = "text-model".to_string();
        settings::save(&paths, &conf).expect("the conf");
        Scene {
            root,
            paths,
            llm: llm_url.to_string(),
        }
    }

    fn tree(&self) -> naivepost::layout::Tree {
        naivepost::layout::Tree::new(self.root.join("session.naivepost"))
            .expect("the session is a project")
    }

    fn conf(&self) -> Conf {
        let mut conf = Conf::default();
        conf.server = self.llm.clone();
        conf.model = "text-model".to_string();
        conf
    }

    /// Fix this scene's transcript the way the Prepare stage does.
    fn fix(&self, log: &mut impl FnMut(&str)) -> Result<usize, String> {
        naivepost::fix_call::stage_chunk(
            &self.tree(),
            &self.paths,
            &self.conf(),
            LANE,
            true,
            false,
            log,
        )
    }

    /// One ask through the shared refusal leg, offering the retake job's tools, as every tool-offering step
    /// dials: the body built by `llm_request`, sent by `llm_retry_leg`.
    fn ask(&self) -> Result<naivepost::server_leg::Sent, String> {
        let tree = self.tree();
        let conf = self.conf();
        let endpoint = naivepost::services::llm_endpoint(&conf);
        let tools = naivepost::tools::offered(naivepost::roles::Job::Retakes);
        let messages = vec![naivepost::exchanges::Message {
            role: "user".to_string(),
            parts: vec![naivepost::exchanges::Part::Text("BRIEF".to_string())],
        }];
        let base = naivepost::server_leg::base_row(
            naivepost::services::Server::Llm,
            &conf.model,
            "POST",
            "",
            STEP,
            STEP,
        );
        naivepost::llm_tool_refusal_leg::dial_honouring_refusal(
            STEP,
            &endpoint.url,
            Some(&tools),
            &|offer| {
                Ok(naivepost::llm_request::body(
                    STEP,
                    &conf.model,
                    &messages,
                    naivepost::prompts::mode_of(STEP),
                    offer,
                    true,
                )?
                .to_string())
            },
            &|body| {
                naivepost::llm_retry_leg::chat(STEP, &tree, &base, &endpoint, body, &|| false, &say)
            },
            &say,
        )
    }

    fn drop_root(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

thread_local! {
    /// What the leg said on this thread when these checks drove it directly.
    static SAID: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The log the leg is handed in these checks.
fn say(line: &str) {
    SAID.with(|said| said.borrow_mut().push(line.to_string()));
}

/// Every refusal line said on this thread: by the leg these checks drive, and by the program's own log.
fn refusals() -> Vec<String> {
    SAID.with(|said| said.borrow().clone())
        .into_iter()
        .chain(naivepost::ui::window::window_logs())
        .filter(|line| line.contains("the server refused the request with tools"))
        .collect()
}

// ---- S5 for the steps that never honoured it -----------------------------------------------------

/// The step the requests are made for: one that offers tools.
const STEP: &str = "retake";

/// A tool-offering step: the refusal costs the identical request again WITHOUT the field, the line is said once,
/// and the step gets its answer: the job survives a server that cannot take tools.
#[test]
fn f6_1_s5_a_step_asks_again_without_the_tools_field() {
    let llm = RefusingLlm::start(plain_answer());
    let scene = Scene::new("resend", &llm.url);
    naivepost::ui::window::clear_logs_for_test();
    scene
        .ask()
        .expect("a refused tools field does not lose the answer");

    let asked = llm.waited(2);
    assert_eq!(
        asked.len(),
        2,
        "the refusal and its resend both reached the server"
    );
    assert!(
        offers_tools(&asked[0]),
        "the first request offered the job's tools"
    );
    assert!(
        !offers_tools(&asked[1]),
        "the resent request carries no `tools` field"
    );
    assert_eq!(
        messages_of(&asked[0]),
        messages_of(&asked[1]),
        "S5 resends the IDENTICAL request minus the field"
    );
    let said = refusals();
    assert_eq!(said.len(), 1, "the refusal is logged once: {said:?}");
    assert!(
        said[0].contains(&format!(">>> {STEP}:")),
        "the line names the step: {}",
        said[0]
    );
    scene.drop_root();
}

/// The remembering half: a SECOND call to the same server neither re-offers the field nor says the line again.
/// This is what the prototype could not do, and what §5 says the rewrite SHOULD.
#[test]
fn f6_1_s5_a_step_that_already_heard_the_refusal_is_asked_silently() {
    let llm = RefusingLlm::start(plain_answer());
    let scene = Scene::new("silent", &llm.url);
    naivepost::ui::window::clear_logs_for_test();
    scene.ask().expect("the first call survived the refusal");
    let after_first = refusals().len();
    assert_eq!(after_first, 1, "the first call said it once");

    scene.ask().expect("the second call too");
    assert_eq!(
        refusals().len(),
        after_first,
        "the remembered refusal is silent from then on"
    );
    let asked = llm.waited(3);
    assert_eq!(asked.len(), 3, "the second call costs one request, not two");
    assert!(
        !offers_tools(&asked[2]),
        "a remembered refusal is never re-offered"
    );
    scene.drop_root();
}

/// A dead server is NOT a refusal: nothing about the server is remembered, so a later call still offers the field.
#[test]
fn f6_1_s5_a_dead_server_is_not_a_refusal() {
    // Port 1: nothing listens, so the dial dies in the connect rather than being answered.
    let scene = Scene::new("dead", "http://127.0.0.1:1");
    naivepost::ui::window::clear_logs_for_test();
    assert!(scene.ask().is_err(), "a dead server answers nothing");
    assert!(
        refusals().is_empty(),
        "a transport death is not reported as a refusal: {:?}",
        refusals()
    );

    // The same scene's endpoint comes alive: the next call offers the field again, because nothing was remembered.
    let llm = RefusingLlm::start(plain_answer());
    let back = Scene::at(scene.root.clone(), &llm.url);
    back.ask()
        .expect("with a server answering, the step answers");
    let asked = llm.waited(2);
    assert!(
        offers_tools(&asked[0]),
        "the field is offered again to a server that never refused it"
    );
    back.drop_root();
}

/// The transcript fixer offers no tools (one call, the block's rows answered back, as the prototype asked it), so
/// a server that refuses the `tools` field fixes its block in ONE request and nothing is refused or said.
#[test]
fn f6_1_s5_the_fix_step_offers_no_tools_to_refuse() {
    let llm = RefusingLlm::start(rows_answer());
    let scene = Scene::new("fix", &llm.url);
    naivepost::ui::window::clear_logs_for_test();
    let blocks = scene.fix(&mut |_: &str| {}).expect("the fix ran");
    assert_eq!(blocks, 1, "one block");
    let asked = llm.waited(1);
    assert_eq!(asked.len(), 1, "one request for one block");
    assert!(!offers_tools(&asked[0]), "with no `tools` field in it");
    assert!(
        refusals().is_empty(),
        "so nothing was refused: {:?}",
        refusals()
    );
    let fixed =
        std::fs::read_to_string(scene.tree().transcript_fixed_tsv(LANE)).unwrap_or_default();
    assert!(
        fixed.contains("so the algorithm receives the input"),
        "and the block is fixed: {fixed}"
    );
    scene.drop_root();
}

impl Scene {
    /// Re-point an existing scene's settings at a different address (the seam the program re-reads per request).
    fn at(root: PathBuf, llm_url: &str) -> Scene {
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let mut conf = Conf::default();
        conf.server = llm_url.to_string();
        conf.model = "text-model".to_string();
        settings::save(&paths, &conf).expect("the conf rewritten");
        Scene {
            root,
            paths,
            llm: llm_url.to_string(),
        }
    }
}
