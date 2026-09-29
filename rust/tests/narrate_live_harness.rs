#![allow(dead_code)]

//! F4.2's live half over a socket: the harness the one-scenario-per-binary tests share.
//!
//! `tests/narrate_call.rs` and `tests/narrate_call_flow.rs` prove the message pair and the attempt
//! loop with answers handed in as text. This proves the wire: the pair `narrate_call::request`
//! assembled goes out over HTTP to the address the Settings hold, §3.8's six tools are offered on it,
//! a tool-call answer is dispatched to the SAME `narrate_pass` tools the program uses, and each round
//! leaves its timed row in `requests.tsv`. The seam is the settings address (`LLM_SERVER`), the same
//! one a person types into the Settings box — so no canned reply stands where the program would
//! telephone.
//!
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory are process-wide, so each scenario
//! gets its own test BINARY (the sibling files) sharing this harness by `#[path]` include.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf, Paths};
use serde_json::json;

/// What one request looked like from the server's side.
pub struct Seen {
    pub method: String,
    pub path: String,
    pub body: String,
    /// The exact bytes this request was answered with, so a test can assert what the leg was handed
    /// rather than only what it sent.
    pub reply: String,
}

/// A fake LLM on a free loopback port, answering each ask with the next scripted reply.
pub struct FakeLlm {
    port: u16,
    received: mpsc::Receiver<Seen>,
}

impl FakeLlm {
    pub fn start(replies: Vec<String>) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(replies));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sender = sender.clone();
                let script = script.clone();
                std::thread::spawn(move || {
                    // With one reply left it answers every ask with it, which is what a retry test
                    // wants to see; more than one and each ask spends the next.
                    let reply = script
                        .lock()
                        .map(|mut list| {
                            if list.len() > 1 {
                                list.remove(0)
                            } else {
                                list.first().cloned().unwrap_or_default()
                            }
                        })
                        .unwrap_or_default();
                    if let Some(mut seen) = serve(stream, &reply) {
                        seen.reply = reply;
                        let _ = sender.send(seen);
                    }
                });
            }
        });
        FakeLlm { port, received }
    }

    /// A port nothing listens on: the down-server case.
    pub fn dead_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind to hand out a port");
        listener.local_addr().expect("a local address").port()
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn next_seen(&self) -> Seen {
        self.received
            .recv_timeout(Duration::from_secs(25))
            .expect("the narration ask reached the fake server")
    }

    /// How many further requests arrive, waiting a moment for stragglers.
    pub fn drained(&self) -> usize {
        let mut got = 0usize;
        loop {
            match self.received.recv_timeout(Duration::from_millis(700)) {
                Ok(_) => got += 1,
                Err(_) => return got,
            }
        }
    }
}

fn serve(mut stream: TcpStream, body: &str) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    let seen = Seen {
        method: parts.first().copied().unwrap_or_default().to_string(),
        path: parts.get(1).copied().unwrap_or_default().to_string(),
        body: String::from_utf8_lossy(&payload).to_string(),
        reply: String::new(),
    };
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(seen)
}

// --- the two answer shapes a real server sends --------------------------------------------------

/// Wrap content in the chat envelope a real server answers with.
pub fn envelope_of(content: &str) -> String {
    json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// The prototype's own content answer: `{"entries":[{start,end,at,text,emotion}]}` (§F4.2's last
/// paragraph), as an INLINE JSON value inside the envelope's `content` string — the way a real server
/// sends it, since `content` is a string and the document is what the writer wrote inside it.
pub fn entries_answer(entries: &[(f64, f64, f64, &str, &str)]) -> String {
    let list: Vec<_> = entries
        .iter()
        .map(|(start, end, at, text, emotion)| {
            json!({ "start": start, "end": end, "at": at, "text": text, "emotion": emotion })
        })
        .collect();
    let document = json!({ "entries": list }).to_string();
    json!({ "choices": [ { "message": { "content": document } } ] }).to_string()
}

/// A round that answers with tool calls instead of content. `calls` is `(tool name, args JSON)`.
pub fn tool_calls_answer(calls: &[(&str, serde_json::Value)]) -> String {
    let list: Vec<_> = calls
        .iter()
        .map(|(name, args)| {
            json!({ "type": "function", "function": {
                "name": name,
                "arguments": args.to_string(),
            } })
        })
        .collect();
    json!({ "choices": [ { "message": { "content": "", "tool_calls": list } } ] }).to_string()
}

// --- the session this scenario runs in --------------------------------------------------------

/// The scenario's folder, its settings, and the session tree the call reads.
pub struct Fixture {
    pub root: PathBuf,
    pub tree: Tree,
    pub project: Project,
}

/// Build the whole scenario under one temp root and point every path the call could read at it:
/// settings (`XDG_CONFIG_HOME`), data (`XDG_DATA_HOME`), the working directory, and the project
/// folder itself. Nothing outside this root is read or written.
///
/// ONE scenario per test BINARY. `set_var` is process-wide, so two fixtures in one binary would each
/// read whichever env the other set last; the guard below turns that silent race into a loud failure.
pub fn fixture(tag: &str) -> Fixture {
    if FIXTURES.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
        panic!("two fixtures in one test binary: the process-wide env races -- one scenario per binary");
    }
    let root = std::env::temp_dir().join(format!("naivepost-f42live-{tag}"));
    // Wipe the whole scenario root, including anything an earlier run left under this tag: a stale
    // cut or transcript would answer the brief without the seeded one.
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
    // The LLM address is deliberately NOT written here: `settings::save` replaces the whole file, so
    // an address set before this call would be wiped and the call would dial the default instead of
    // the fake. Every scenario sets it with `point_at` AFTER building the fixture.
    let mut conf = Conf::default();
    conf.model = "narrate-model".to_string();
    settings::save(&paths, &conf).expect("settings written");

    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    // Two clips of ten seconds, and a transcript line over the first, so the brief has something to
    // show and a speech premise to decide on.
    let cut = Cut {
        segs: vec![seg(0.0, 10.0), seg(10.0, 20.0)],
        ..Default::default()
    };
    naivepost::cut::save(&cut, &tree).expect("cut.json written");
    write_transcript(&tree);

    let mut project = Project::default();
    project.interval = 1.0;
    project.context = "A lecture about Ethereum.".to_string();
    project.sources = vec![Source {
        path: "project:sources/lecture.mkv".into(),
        footage: true,
        ..Default::default()
    }];
    Fixture { root, tree, project }
}

/// The transcript the brief reads: two spoken lines over the first clip's seconds.
fn write_transcript(tree: &Tree) {
    let body = "1.0\t3.0\tlecture.mkv\tAnn\tThe chain moved to proof of stake.\n\
               5.0\t7.0\tlecture.mkv\tAnn\tThat cut the energy use by most.\n";
    tree.write_file(Path::new("prepare/transcript/session.tsv"), body.as_bytes())
        .expect("session.tsv written");
}

/// Rewrite the settings' LLM address. This is the seam: the next ask dials what is in the file now.
pub fn point_at(port_url: &str) -> String {
    let paths = settings_folder();
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.server = port_url.to_string();
    settings::save(&paths, &conf).expect("new address written");
    port_url.to_string()
}

/// The settings folder as the environment answers it right now.
pub fn settings_folder() -> Paths {
    settings::from_environment().expect("settings folder")
}

/// How many fixtures this process has built. Each re-points the process-wide environment, so counting
/// keeps every scenario's folder separate — and guards the one-per-binary rule.
static FIXTURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The clip segments this scenario's cut holds.
pub fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// Read a file the scenario wrote; empty-string-free assertions read this (panics with the path).
pub fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("expected a file at {path:?}"))
}

/// The timed rows this scenario's `requests.tsv` holds, header off.
pub fn rows(fixture: &Fixture) -> Vec<String> {
    read(&fixture.tree.requests_tsv())
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.to_string())
        .collect()
}

/// Build the request pair the leg would send, the same way the program's seam does.
pub fn build_request(fixture: &Fixture) -> naivepost::narrate_call::Request {
    let voice = naivepost::narrate_data::read_voice(&fixture.tree);
    let paths = settings_folder();
    let read_prompt = |key: &str| settings::prompt_text(&paths, key, "").unwrap_or_default();
    naivepost::narrate_call::request_from_tree(
        &fixture.tree,
        &fixture.project,
        &voice,
        &read_prompt("narrate"),
        &read_prompt("system"),
    )
}

/// Ask one round the way the program's own seam does: gather the session's material, build the pair,
/// hand it to `narrate_live::ask_round`. Tests call THIS rather than restating the gathering, so what
/// a test drives is what `narrate_ask::run` drives.
#[allow(clippy::too_many_arguments)]
pub fn ask_one(
    fixture: &Fixture,
    attempt: u32,
    thinking: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<naivepost::narrate_live::Round, String> {
    let (segs, transcript, narrator) = material(fixture);
    let request = build_request(fixture);
    naivepost::narrate_live::ask_round(
        &fixture.tree, &request, &segs, &transcript, &narrator, attempt, thinking, cancelled,
    )
}

/// The segments and transcript rows the leg reads, off the scenario's own session.
pub fn material(fixture: &Fixture) -> (Vec<Seg>, Vec<naivepost::textfmt::SessionLine>, String) {
    let cut = naivepost::cut::load(&fixture.tree).expect("cut read back");
    let rows = naivepost::textfmt::read_session(&fixture.tree.session_tsv())
        .expect("session.tsv read back");
    let narrator = naivepost::sources::narrator_of(&fixture.project, 1).unwrap_or("").to_string();
    (cut.segs, rows, narrator)
}
