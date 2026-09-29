#![allow(dead_code)]

//! F3.9's live half over a socket: the harness the one-scenario-per-binary tests share.
//!
//! `tests/cut_captions_proposed.rs` proves the rules with answers handed in as text. This proves the
//! wire: the spec's own clip message goes out over HTTP to the address the settings hold, the CAPTIONS
//! reply comes back through a chat envelope, and the calls it made are what `cut_captions::place` then
//! judges. The seam is the settings address (`LLM_SERVER`), the same one a person types into the
//! Settings box — so no canned reply stands where the program would telephone.
//!
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory are process-wide, so each scenario
//! gets its own test BINARY (the sibling files) sharing this one harness by `#[path]` include.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use naivepost::layout::Tree;
use naivepost::settings::{self, Conf};
use serde_json::json;

/// What one request looked like from the server's side.
pub struct Seen {
    pub method: String,
    pub path: String,
    pub body: String,
    /// The exact bytes this request was answered with, so a test can assert what the pass was
    /// handed rather than only what it sent.
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
                    // With one reply left it answers every ask, which is what a retry test wants to
                    // see; more than one and each ask spends the next.
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
            .recv_timeout(Duration::from_secs(15))
            .expect("the captions ask reached the fake server")
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

/// The CAPTIONS document §F3.9's prompt specifies, wrapped in the chat envelope a real server answers
/// with, so the pass reads it through `llm_request::parse_reply` exactly as it would live.
///
/// `clips` is one entry per clip the model answered about: its number and its `(start, end, text)`
/// captions in the order it asked for them.
pub fn caption_json(clips: &[(u32, &[(f64, f64, &str)])]) -> String {
    let entries: Vec<_> = clips
        .iter()
        .map(|(number, captions)| {
            json!({
                "i": number,
                "fx": captions.iter().map(|(start, end, text)| json!({
                    "start": start,
                    "end": end,
                    "text": text,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    envelope_of(&json!({ "clips": entries }).to_string())
}

/// Wrap arbitrary content in the chat envelope. Anything that is not a CAPTIONS document goes through
/// here when a scenario wants to see the pass refuse it.
pub fn envelope_of(content: &str) -> String {
    json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// The session folder this scenario runs in, with its settings folder and project beside it.
pub struct Fixture {
    pub root: PathBuf,
    pub tree: Tree,
}

/// Build the whole scenario under one temp root and point every path the ask could read at it:
/// settings (`XDG_CONFIG_HOME`), data (`XDG_DATA_HOME`), the working directory, and the project
/// folder itself. Nothing outside this root is read or written.
///
/// One scenario per test BINARY. `set_var` is unsafe and process-wide, so two fixtures in one binary
/// would each read whichever env the other set last; that is why the F3.9 live scenarios are separate
/// test targets rather than two `#[test]`s sharing a file.
pub fn fixture(tag: &str) -> Fixture {
    if FIXTURES.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
        panic!("two fixtures in one test binary: the process-wide env races -- one scenario per binary");
    }
    let root = std::env::temp_dir().join(format!("naivepost-f39live-{tag}"));
    // Wipe the whole scenario root, including any answer cached under an earlier run of this tag: a
    // cached reply would answer the ask without ever reaching the fake server, and the test would then
    // assert about a request that never went out.
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
    // an address set before this call would be wiped and the ask would dial the default instead of
    // the fake. Every scenario sets it with `point_at` AFTER building the fixture.
    let mut conf = Conf::default();
    conf.model = "captions-model".to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree }
}

/// Rewrite the settings' LLM address. This is the seam: the next ask dials what is in the file now.
pub fn point_at(port_url: &str) -> String {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.server = port_url.to_string();
    settings::save(&paths, &conf).expect("new address written");
    port_url.to_string()
}

/// How many fixtures this process has built. Each re-points the process-wide environment, so counting
/// keeps every scenario's folder separate.
static FIXTURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Read a file the scenario wrote, empty when it never appeared (so the assertion names the path).
pub fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("expected a file at {path:?}"))
}

/// Two kept clips of 14.2 s and 9.0 s, with three words inside the first at +0.4, +2.0 and +5.0.
/// Returns the clip list, the word list and the offsets in the shapes `captions_ask::ask` takes.
pub fn two_clips_with_words() -> (Vec<(u32, f64)>, Vec<(u32, f64, String)>) {
    let clips = vec![(1u32, 14.2f64), (2u32, 9.0f64)];
    let words = vec![
        (1u32, 0.4f64, "the network".to_string()),
        (1u32, 2.0f64, "is live".to_string()),
        (2u32, 0.5f64, "today".to_string()),
    ];
    (clips, words)
}
