//! §09-llm-and-tools#1-request-contract S4's streaming checks, shared: the loopback chat server that answers
//! in `text/event-stream` framing, and the scenario root every socket check needs.
//!
//! Every live chat call now ASKS for a stream (`stream: true`, §4's "every call is streamed"), so the SSE half
//! of §1's contract has to be met by a real socket rather than a string in a unit test. Each scenario lives in
//! its OWN binary (the sibling files here are one-scenario-per-binary for a reason): [`fixture`] pins
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory, which are process-wide, and `cargo test` runs
//! one binary's tests as parallel threads — two scenarios in one process would overwrite each other's settings
//! and neither would dial the fake it was handed.

#![allow(dead_code)] // every binary compiles this whole module; a helper it does not call is not a warning here

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use naivepost::settings::{self, Conf};

/// The model name the fake's settings carry, so a check can see the settings' model came back on the wire.
pub const MODEL: &str = "stream-model";

/// The answer the decorations check scripts: the EFFECTS document with TWO decorations, whose JSON is split
/// across chunks so a reader that expects one whole body finds nothing.
///
/// Ordering matters and is the point of the shape: every content chunk arrives BEFORE the `finish_reason`
/// chunk, because §1 says a `finish_reason` ends the reply — [`crate::llm_request::from_events`] stops reading
/// there, and a tail that arrives after it did not arrive at all. Payloads are built with `serde_json`, so no
/// backslash counting is involved.
pub fn effects_chunks() -> Vec<String> {
    let fx = serde_json::json!({"fx": [
        {"clip": 1, "kind": "zoom", "start": 1.0, "end": 3.0},
        {"clip": 2, "kind": "volume", "start": 4.0, "end": 6.0, "gain": 0.5},
    ]})
    .to_string();
    let bytes = fx.as_bytes();
    let third = bytes.len() / 3 + 1;
    let piece = |from: usize, to: usize| {
        String::from_utf8_lossy(&bytes[from.min(bytes.len())..to.min(bytes.len())]).to_string()
    };
    let prose = |words: &str| serde_json::json!({"choices": [{"delta": {"content": words}}]});
    vec![
        // A keep-alive the reader must skip, and a thinking chunk it must keep APART from the answer.
        ": keep-alive".to_string(),
        format!(
            "data: {}",
            serde_json::json!({"choices": [{"delta": {"reasoning_content": "two decorations"}}]})
        ),
        format!("data: {}", prose(&piece(0, third))),
        format!("data: {}", prose(&piece(third, 2 * third))),
        format!("data: {}", prose(&piece(2 * third, bytes.len()))),
        // Why the stream ended, last: everything the answer holds has arrived by then.
        format!(
            "data: {}",
            serde_json::json!({"choices": [{"finish_reason": "stop", "delta": {}}]})
        ),
        "data: [DONE]".to_string(),
    ]
}

/// A stream that closes before `[DONE]`: only the first piece of the document arrived, then EOF.
pub fn truncated_chunks() -> Vec<String> {
    let fx = serde_json::json!({"fx": [
        {"clip": 1, "kind": "zoom", "start": 1.0, "end": 3.0},
        {"clip": 2, "kind": "volume", "start": 4.0, "end": 6.0, "gain": 0.5},
    ]})
    .to_string();
    let head = &fx[..fx.len() / 2];
    vec![format!(
        "data: {}",
        serde_json::json!({"choices": [{"delta": {"content": head}}]})
    )]
}

/// What the fake saw from one request.
pub struct Seen {
    pub body: String,
}

/// The chat server on loopback that answers in `text/event-stream` framing from the script it is handed.
///
/// It serves every request it is given with the SAME script, so a check may make more than one ask; each
/// request's body still arrives on the channel for assertion.
pub struct StreamLlm {
    port: u16,
    received: Receiver<Seen>,
}

impl StreamLlm {
    pub fn start(chunks: Vec<String>) -> StreamLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = std::sync::Arc::new(std::sync::Mutex::new(chunks));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let lines = script.lock().expect("script lock").clone();
                if let Some(seen) = serve(stream, &lines) {
                    let _ = sender.send(seen);
                }
            }
        });
        StreamLlm { port, received }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The next request that reached the server.
    pub fn next(&self) -> Seen {
        self.received
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the ask reached the fake LLM")
    }
}

/// Answer one request with the SSE framing and no `Content-Length`: the body ends when the connection does,
/// which is how a real streaming reply arrives.
fn serve(mut stream: TcpStream, chunks: &[String]) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
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
    let body = String::from_utf8_lossy(&payload).to_string();
    let framing = chunks.join("\n") + "\n\n";
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(framing.as_bytes());
    let _ = stream.flush();
    Some(Seen { body })
}

/// The scenario root: settings, data folder, project and cwd all under one temp dir. Built once per binary,
/// which is why every scenario is a binary of its own.
pub fn fixture(tag: &str) -> (PathBuf, naivepost::layout::Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-p9s1-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned");
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_DATA_HOME", &data);
    }
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = Conf::default();
    conf.model = MODEL.to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    (root, tree)
}

/// The seam the program itself uses: the leg re-reads settings per ask, so writing the address there is what
/// makes the next call dial the fake.
pub fn point_at(url: &str) {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.server = url.to_string();
    conf.model = MODEL.to_string();
    settings::save(&paths, &conf).expect("address written");
}

/// The settings every `ask_at` door takes, read through the same path the program reads.
pub fn conf() -> Conf {
    settings::read(&settings::from_environment().expect("settings")).expect("conf")
}

pub fn never_stops() -> impl Fn() -> bool {
    || false
}

/// The clips every door is asked about: two, long enough for §10's floor.
pub fn clips() -> Vec<(u32, f64, f64)> {
    vec![(1, 0.0, 20.0), (2, 20.0, 40.0)]
}

/// The decorated clip list needs a frame interval set before a span survives the parser.
pub fn decorated_clips() -> naivepost::tools::clips::Clips {
    let mut held = naivepost::tools::clips::Clips::new(&clips());
    held.set_frame_interval(1.0);
    held
}
