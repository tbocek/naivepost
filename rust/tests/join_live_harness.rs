#![allow(dead_code)]

//! F1.10 S3's live half over a socket: the harness the one-scenario-per-binary tests share.
//!
//! `tests/joins_flow.rs` proves the rules with answers handed in as text. This proves the wire: the
//! spec's own message goes out over HTTP to the address the settings hold, the reply is read as a
//! chat response, and the words it left out end up out of `final.txt` and in `retakes.tsv`. The
//! seam is the settings address (`LLM_SERVER`), the same one a person types into the Settings box,
//! so no canned reply stands where the program would telephone — the answer travelled through a
//! socket and was parsed by the client.
//!
//! The whole session lives under one temp root: `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working
//! directory are all pointed at it, so the settings file the pass reads is this test's own file
//! and the project it writes is this test's own folder. One scenario per test binary (see the
//! sibling files), because those variables are process-wide.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use naivepost::layout::Tree;
use naivepost::project::{MarkingPass, Project, Source};
use naivepost::requests::{self, Word, WordsDoc};
use naivepost::settings::{self, Conf};
use serde_json::json;
use naivepost::transcribe::SAMPLE_RATE;

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
                    let reply = script
                        .lock()
                        .map(|mut list| {
                            // The first reply is spent; the rest stay for later asks. With one left
                            // it answers every ask, which is what a retry test wants to see.
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
            .expect("the ask reached the fake server")
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
        // Filled in by the caller that served it, so a test can assert on the bytes the pass read.
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

/// A chat envelope carrying `content` as the answer. Superseded by
/// `joined_answer::chat`, which builds the reply with serde_json; each test file imports that
/// one explicitly so it shadows this glob-imported version. Kept only so the harness still
/// compiles for anything that reaches for a plain content-only envelope.
pub fn chat_plain(content: &str) -> String {
    json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// Superseded by `joined_answer::chat`, which builds the same reply with serde_json. Kept as a
/// marker of where the hand-escaped version was, so the replacement is traceable; nothing calls it.
#[deprecated = "use joined_answer::chat, which builds this with serde_json"]
pub fn chat_escaped_unused(content: &str) -> String {
    let _ = content;
    String::new()
}

/// Two takes meeting at a seam, five words each, one second per word. `take-a` ends
/// "the network is live today" and `take-b` says it again as "the network goes live now": a
/// restart, so leaving take-a's tail out is one stretch at the join.
pub const TAKE_A: [&str; 5] = ["the", "network", "is", "live", "today"];
pub const TAKE_B: [&str; 5] = ["the", "network", "goes", "live", "now"];

/// The session folder this scenario runs in, with its settings folder and project beside it.
pub struct Fixture {
    pub root: PathBuf,
    pub tree: Tree,
    pub project: Project,
}

/// Build the whole session under one temp root and point every path the pass could read at it:
/// settings (`XDG_CONFIG_HOME`), data (`XDG_DATA_HOME`), the working directory, and the project
/// folder itself. Nothing outside this root is read or written.
pub fn fixture(tag: &str) -> Fixture {
    let depth = FIXTURES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("naivepost-f110live-{tag}-{depth}"));
    // Wipe the whole scenario root, INCLUDING any answer cached under an earlier run of this tag:
    // a cached reply would answer the ask without ever reaching the fake server, and the test would
    // then assert about a request that never went out.
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
    // The LLM address is deliberately NOT written here: `settings::save` replaces the whole file,
    // so an address set before this call would be wiped and the pass would dial the default
    // instead of the fake. Every scenario sets it with `point_at` AFTER building the fixture.
    let mut conf = Conf::default();
    conf.model = "textedit-model".to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    seed_words(&tree, "take-a", &TAKE_A);
    seed_words(&tree, "take-b", &TAKE_B);
    let mut project = Project::default();
    project.interval = 1.0;
    project.context = "A lecture about Ethereum.".to_string();
    project.sources = vec![
        footage("project:sources/take-a.mkv"),
        footage("project:sources/take-b.mkv"),
    ];
    Fixture {
        root,
        tree,
        project,
    }
}

/// Rewrite the settings' LLM address. This is the seam: the next ask dials what is in the file now.
pub fn point_at(port_url: &str) -> String {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.server = port_url.to_string();
    settings::save(&paths, &conf).expect("new address written");
    port_url.to_string()
}

/// How many times this process has built a fixture. Every fixture re-points the process-wide
/// environment, so fixtures nest: each one's root holds the ones made before it, and the previous
/// `session.naivepost` rides along as a `prepare/` folder inside the next one's. Counting keeps
/// every scenario's folder separate, which is what makes the pass read the words it was seeded with.
static FIXTURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn footage(path: &str) -> Source {
    Source {
        path: path.to_string(),
        footage: true,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }
}

/// Write one lane's `words.json`: each word one second long, word `n` spanning `n .. n+1`.
fn seed_words(tree: &Tree, lane: &str, words: &[&str]) {
    let doc = WordsDoc {
        text: words.join(" "),
        words: words
            .iter()
            .enumerate()
            .map(|(n, word)| Word {
                word: word.to_string(),
                start_sample: n as u64 * SAMPLE_RATE,
                end_sample: (n as u64 + 1) * SAMPLE_RATE,
            })
            .collect(),
    };
    requests::write_words(tree, lane, &doc).expect("words.json written");
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// A body clipped so a failure message stays readable.
pub fn clip(body: &str) -> String {
    let keep: String = body.chars().take(900).collect();
    if body.chars().count() > 900 {
        format!("{keep}…")
    } else {
        keep
    }
}

/// Run the pass the same way the ▶ handler does: one call, gated on the joins pass.
pub fn press(tree: &Tree, project: &Project) -> Vec<String> {
    naivepost::joins::press_joins(tree, project, MarkingPass::Joins)
}
