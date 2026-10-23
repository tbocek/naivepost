//! F6.1 S5 over the wire for the three steps that did not honour it: the transcript fixer, the retake pool and
//! the cut's own ask.
//!
//! `tests/suite/tool_protocol_f6_1_wire.rs` proves the rule for the upload text, whose flow already had a copy of it.
//! What §2 says the rewrite MUST NOT keep is the per-step forgetting — "the refusal is not remembered, so a step
//! retrying with tools re-offers the field and re-logs the line each time" — and until this round the fixer, the
//! retakes and the cut each built `Some(tools)` and gave up on the first error. They now share
//! [`naivepost::llm_tool_refusal_leg::dial_honouring_refusal`], and these checks point the SETTINGS address (the
//! seam the program itself reads) at a fake that answers 400 to any body carrying a `tools` key, then assert on
//! the bodies the fake ACTUALLY received.
//!
//! One scenario per test function, each with its own fake port and its own settings folder handed to the door
//! explicitly (`tree`/`paths`), so no thread touches the process env — the pattern `tests/suite/fix_call.rs` uses.

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

/// One `fix_line` + `finish` answer in the chat envelope §3.3's tool surface replies with.
fn fix_answer(n: u32, text: &str) -> String {
    let args = serde_json::json!({ "n": n, "text": text }).to_string();
    let escaped = args.replace('"', "\\\"");
    format!(
        "{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[\n{{\"function\":{{\"name\":\"fix_line\",\"arguments\":\"{escaped}\"}}}},\n{{\"function\":{{\"name\":\"finish\",\"arguments\":\"{{}}\"}}}}\n]}}}}]}}"
    )
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

    fn drop_root(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Every refusal line this thread's window log holds.
fn refusals() -> Vec<String> {
    naivepost::ui::window::window_logs()
        .into_iter()
        .filter(|line| line.contains("the server refused the request with tools"))
        .collect()
}

// ---- S5 for the steps that never honoured it -----------------------------------------------------

/// The fix step: the refusal costs the identical request again WITHOUT the field, the line is said once, and the
/// block still gets fixed — the job survives a server that cannot take tools.
#[test]
fn f6_1_s5_the_fix_step_asks_again_without_the_tools_field() {
    let llm = RefusingLlm::start(fix_answer(1, "so the algorithm receives the input"));
    let scene = Scene::new("fix", &llm.url);
    naivepost::ui::window::clear_logs_for_test();
    let fixed = scene.fix(&mut |_: &str| {});
    assert_eq!(
        fixed.expect("a refused tools field does not lose the fix"),
        1,
        "one line was fixed"
    );

    let asked = llm.waited(2);
    assert!(
        asked.len() >= 2,
        "the refusal and its resend both reached the server: {}",
        asked.len()
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
        said[0].contains(">>> fix:"),
        "the line names the step: {}",
        said[0]
    );
    scene.drop_root();
}

/// The remembering half: a SECOND call to the same server neither re-offers the field nor says the line again.
/// This is what the prototype could not do, and what §5 says the rewrite SHOULD.
#[test]
fn f6_1_s5_a_step_that_already_heard_the_refusal_is_asked_silently() {
    let llm = RefusingLlm::start(fix_answer(1, "so the algorithm receives the input"));
    let scene = Scene::new("silent", &llm.url);
    naivepost::ui::window::clear_logs_for_test();
    // First call: refused, resent, remembered. Its cache entry is cleared so the second call really dials again.
    let first = scene.fix(&mut |_: &str| {});
    assert!(
        first.is_ok(),
        "the first call survived the refusal: {first:?}"
    );
    let after_first = refusals().len();
    assert_eq!(after_first, 1, "the first call said it once");

    clear_cache(&scene);
    let second = scene.fix(&mut |_: &str| {});
    assert!(second.is_ok(), "the second call too: {second:?}");
    assert_eq!(
        refusals().len(),
        after_first,
        "the remembered refusal is silent from then on"
    );

    // And every request the second call made carried no `tools` key at all.
    let asked = llm.waited(3);
    let bare_after = &asked[2..];
    assert!(
        bare_after.iter().all(|body| !offers_tools(body)),
        "a remembered refusal is never re-offered: {:?}",
        bare_after
            .iter()
            .map(|b| offers_tools(b))
            .collect::<Vec<_>>()
    );
    scene.drop_root();
}

/// A dead server is NOT a refusal: nothing about the server is remembered, so a later call still offers the field.
#[test]
fn f6_1_s5_a_dead_server_is_not_a_refusal() {
    // Port 1: nothing listens, so every block's dial dies in the connect rather than being answered. The stage
    // carries on (a down server must not stop a run — §"failure is specific and local"), which is exactly why
    // this check cannot assert `is_err()`; what it asserts is that no refusal was RECORDED about that server.
    let scene = Scene::new("dead", "http://127.0.0.1:1");
    naivepost::ui::window::clear_logs_for_test();
    let _ = scene.fix(&mut |_: &str| {});
    assert!(
        refusals().is_empty(),
        "a transport death is not reported as a refusal: {:?}",
        refusals()
    );

    // The same scene's endpoint comes alive: the next call offers the field again, because nothing was remembered.
    let llm = RefusingLlm::start(fix_answer(1, "so the algorithm receives the input"));
    let back = Scene::at(scene.root.clone(), &llm.url);
    clear_cache(&back);
    let got = back.fix(&mut |_: &str| {});
    assert!(
        got.is_ok(),
        "with a server answering, the block fixes: {got:?}"
    );
    let asked = llm.waited(2);
    assert!(
        offers_tools(&asked[0]),
        "the field is offered again to a server that never refused it"
    );
    back.drop_root();
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

/// Empty this scene's LLM cache so a second call goes back to the wire instead of replaying the first answer.
fn clear_cache(scene: &Scene) {
    let dir = scene.tree().dir().join("cache/llm");
    let _ = std::fs::remove_dir_all(&dir);
}
