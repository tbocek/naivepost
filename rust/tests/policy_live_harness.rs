#![allow(dead_code)]

//! F0.7 S2's live half over a socket: the harness the one-scenario-per-binary tests share.
//!
//! `tests/policy_derive.rs` proves the rules with proposals handed in as text, and
//! `tests/cut_policy_form_widgets.rs` proves the ⚙ wire with an injected sender. This proves the
//! middle that neither covers: the derivation dials the address the settings hold, the policy prompt
//! and both tools ride out over HTTP, and a `set_policy` call read back off the socket is what lands on
//! the policy. The seam is `LLM_SERVER`, the same one a person types into the Settings box, so no canned
//! reply stands where the program would telephone.
//!
//! One scenario per test binary, because `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory
//! are process-wide: a second fixture in the same process would read the first one's settings file.
//! The shape is `tests/join_live_harness.rs`'s; the socket fixture is copied rather than imported
//! because that harness seeds join words and runs the joins pass, neither of which belongs here.

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use naivepost::layout::Tree;
use naivepost::policy::{self, DerivationRequest, Proposal};
use naivepost::project::{Policy, Project};
use naivepost::settings::{self, Conf};
use serde_json::json;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
pub use common::{serve, Seen};

/// A fake LLM on a free loopback port, answering each ask with the next scripted reply.
pub struct FakeLlm {
    port: u16,
    /// Shared so `next_seen` can take one connection itself when the accept thread has not been
    /// scheduled yet (see `next_seen`).
    listener: Arc<Mutex<TcpListener>>,
    script: Arc<Mutex<Vec<String>>>,
    received: mpsc::Receiver<Seen>,
}

impl FakeLlm {
    pub fn start(replies: Vec<String>) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        // Non-blocking so neither the accept thread nor a caller polling from its own thread can park:
        // `accept()` answers WouldBlock and the loop moves on. That is what lets `next_seen` serve the
        // request itself if the thread below never got a slice of CPU.
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(replies));
        let shared = Arc::new(Mutex::new(listener));
        {
            let shared = shared.clone();
            let script = script.clone();
            std::thread::spawn(move || loop {
                // Take the listener only for the instant `accept` needs it: a caller polling in
                // `next_seen` must be able to win the lock between connections.
                let accepted = shared.lock().map(|l| l.accept()).unwrap_or(Err(other_would_block()));
                match accepted {
                    Ok((stream, _)) => serve_and_report(stream, &sender, &script),
                    // WouldBlock: nothing waiting; another poller may have taken this one. Spin briefly
                    // rather than parking, so a request that arrives while nobody else is looking is
                    // still answered within microseconds.
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::yield_now();
                    }
                    Err(_) => return,
                }
            });
        }
        FakeLlm {
            port,
            listener: shared,
            script,
            received,
        }
    }

    /// A port nothing listens on. UNUSED HERE on purpose: `connect` SUCCEEDS against a bound listener's
    /// backlog (measured: refused=false in ~85µs) and `server_leg` has no read timeout, so a chat call to
    /// one sends its request and then waits minutes for a reply nobody will write. A down-server scenario
    /// points at `http://127.0.0.1:1` instead — privileged, so nothing can bind it and the dial is refused
    /// the instant it is attempted. Kept because `tests/join_live_harness.rs` holds the same shape for a
    /// leg that DOES time its reads.
    pub fn dead_listener() -> TcpListener {
        TcpListener::bind("127.0.0.1:0").expect("bind to hand out a port")
    }

    /// [`FakeLlm::dead_listener`]'s port. The caller holds the listener for as long as the address must
    /// stay refused.
    pub fn dead_port(listener: &TcpListener) -> u16 {
        listener.local_addr().expect("a local address").port()
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn next_seen(&self) -> Seen {
        // 120 s, not the 30 s this started at: alone the request lands in ~10 ms, but a full `just test`
        // runs 332 binaries and this one's accept loop only gets a thread once the scheduler reaches it,
        // so under load 30 s was measured expiring on a request that had not been accepted yet. The
        // assertion is unchanged — the derivation must still really arrive.
        //
        // Under that load the accept THREAD could still starve, so the wait is on the CONDITION (a request
        // has been read and answered), not on a thread being scheduled: the caller polls the same listener
        // itself and serves the connection on its own thread when it wins. Same bytes on the wire, same
        // deadline, no assertion weakened — only the identity of the thread that accepts becomes flexible.
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        loop {
            match self.received.recv_timeout(Duration::from_millis(5)) {
                Ok(seen) => return seen,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("the fake server stopped before the derivation arrived")
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if let Some(seen) = self.poll_once() {
                return seen;
            }
            if std::time::Instant::now() >= deadline {
                panic!("the derivation reached the fake server: timed out after 120 s");
            }
        }
    }

    /// One non-blocking accept-and-serve from the calling thread. `None` when nothing is waiting or when
    /// the accept thread already took this connection.
    fn poll_once(&self) -> Option<Seen> {
        let accepted = self.listener.lock().ok()?.accept().ok()?;
        let reply = self
            .script
            .lock()
            .map(|mut list| {
                // The first reply is spent; the rest stay for later asks. With one left it answers
                // every ask, which is what a single-round test wants.
                if list.len() > 1 {
                    list.remove(0)
                } else {
                    list.first().cloned().unwrap_or_default()
                }
            })
            .unwrap_or_default();
        let mut seen = serve(accepted.0, &reply)?;
        seen.reply = reply;
        Some(seen)
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

/// The error the accept loops treat as "nothing waiting yet" when a poller holds the listener.
fn other_would_block() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::WouldBlock, "another poller holds the listener")
}

/// Read one request, answer it with `reply`, and hand what was seen to the channel. Shared by the accept
/// thread and by `FakeLlm::poll_once`, so both halves put identical bytes on the wire.
fn serve_and_report(
    stream: TcpStream,
    sender: &mpsc::Sender<Seen>,
    script: &Arc<Mutex<Vec<String>>>,
) {
    let reply = script
        .lock()
        .map(|mut list| {
            // The first reply is spent; the rest stay for later asks. With one left it answers every
            // ask, which is what a single-round test wants.
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
}

// ---- the replies a policy call can be answered with --------------------------------------

/// A chat envelope whose message carries tool calls — the shape §F0.7 S2's answer takes. Each entry
/// is `(name, args JSON)`, and the arguments are serialised by serde rather than hand-escaped.
pub fn tool_calls(calls: &[(&str, serde_json::Value)]) -> String {
    json!({
        "choices": [{
            "message": {
                "content": "",
                "tool_calls": calls
                    .iter()
                    .enumerate()
                    .map(|(index, (name, args))| json!({
                        "id": format!("call_{index}"),
                        "type": "function",
                        "function": { "name": name, "arguments": args.to_string() }
                    }))
                    .collect::<Vec<_>>()
            }
        }]
    })
    .to_string()
}

/// One `set_policy(field, value, because)` call, as the model spells its arguments.
pub fn set_policy(field: &str, value: &str, because: &str) -> String {
    tool_calls(&[(
        "set_policy",
        json!({ "field": field, "value": value, "because": because }),
    )])
}

/// A plain content answer with no tool calls at all: the whole answer S2's prompt allows ("a default is
/// a whole answer"), used to prove the leg reads silence as silence rather than as a failure.
pub fn chat(content: &str) -> String {
    json!({ "choices": [{ "message": { "content": content } }] }).to_string()
}

// ---- the session folder ---------------------------------------------------------------

/// The session folder one scenario runs in: settings, data, working copy, cwd all pointed at it.
pub struct Fixture {
    pub root: PathBuf,
    pub tree: Tree,
    pub context: String,
    /// Where the working copy lives -- the folder `policy_call` files its cache and its `requests.tsv`
    /// row under, and the one a check reads back.
    pub session: PathBuf,
}

/// Build the session under one temp root and point every path the leg could read at it. The LLM address
/// is deliberately NOT written here — `point_at` sets it afterwards, because `settings::save` replaces
/// the whole file and would wipe an address set before it.
pub fn fixture(tag: &str) -> Fixture {
    let depth = FIXTURES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    // The pid rides the name: `cargo test` runs binaries in parallel, two of them may ask for the same tag, and
    // `depth` restarts at 0 in every process -- without it they share one folder and one `requests.tsv`. Same rule
    // as `join_live_harness::fixture`.
    let root = std::env::temp_dir().join(format!("naivepost-f07live-{tag}-{}-{depth}", std::process::id()));
    // Wipe everything, INCLUDING any answer cached under an earlier run of this tag: a cached reply
    // would answer the ask without ever reaching the fake server, and the test would then assert about
    // a request that never went out.
    let _ = std::fs::remove_dir_all(&root);
    let config = root.join("config");
    let data = root.join("data");
    let session = root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the scenario root");
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("XDG_DATA_HOME", &data);
    let paths = settings::from_environment().expect("settings folder from the pinned env");
    let mut conf = Conf::default();
    conf.model = "policy-model".to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture {
        root,
        tree,
        session,
        context: "A lecture read from a script; the speaker restarts after a stumble.".to_string(),
    }
}

static FIXTURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Rewrite the settings' LLM address. This is the seam: the next derivation dials what is in the file
/// now, through the same endpoint resolution a typed address goes through.
pub fn point_at(url: &str) -> String {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.server = url.to_string();
    settings::save(&paths, &conf).expect("new address written");
    url.to_string()
}

/// One derivation's outcome: what the rounds did, and the policy they wrote.
#[derive(Debug, Clone)]
pub struct Run {
    pub derived: policy::Derived,
    pub policy: Policy,
}

/// Run one derivation the way the window does: the live leg as the answer, `policy::derive` owning the
/// rounds and the refusal loop. Nothing here injects a reply -- the leg dials the address the settings
/// hold, which is the whole point of these files.
pub fn derive_lively(fixture: &Fixture, context: &str) -> Run {
    // Hand the leg the same starting policy the window would: this session's, read off disk if the
    // scenario saved one, else the defaults the form would show.
    let mut policy = naivepost::project::load(&fixture.session)
        .map(|project| project.policy.clone())
        .unwrap_or_default();
    naivepost::policy_call::set_live_policy(&policy);
    let derived = policy::derive(&mut policy, context, &mut |req, refusals| {
        naivepost::policy_call::answer(req, refusals)
    });
    Run { derived, policy }
}

/// Serialise a policy and hand back the JSON for one field, so assertions read the STORED shape (the
/// same trick `tests/policy_derive.rs` uses: a reason dropped on the way to disk fails there too).
pub fn stored(policy: &Policy, field: &str) -> serde_json::Value {
    let all = serde_json::to_value(policy).expect("a Policy serialises");
    all.get("policy")
        .cloned()
        .unwrap_or(all)
        .get(field)
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

/// A proposal as a test hands one to a canned sender (only the widget-side checks need this).
pub fn proposal(field: &str, value: &str, because: &str) -> Proposal {
    Proposal {
        field: field.to_string(),
        value: value.to_string(),
        because: because.to_string(),
    }
}

/// The request the leg sends, built the way the leg builds it — so a test can name the prompt key and
/// the two tools without reaching past the leg for them.
pub fn request_for(context: &str) -> DerivationRequest {
    policy::request(context)
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

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// A project carrying one footage source, for anything that wants a session rather than a policy.
pub fn project_with_source() -> Project {
    Project {
        context: "a session".to_string(),
        ..Project::default()
    }
}

/// Pin the derivation's patience so a down-server scenario sits through §3's short retry in
/// milliseconds instead of seconds. `llm_retry_leg`'s own seam, read from the environment per wait.
pub fn scale_the_patience() {
    std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
}
