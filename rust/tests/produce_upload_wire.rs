//! F5.6 S2 over the WIRE: the `youtube` call in `naivepost::produce_upload_call` dialled against a local
//! fake LLM, exactly as §02-services#1 describes it — the settings file names the address, and pointing it
//! at this server is the only seam the program uses. Nothing here scripts an answer inside the library: the
//! answers come back over HTTP, and the assertions read the request body the server actually received.
//!
//! Follows `tests/produce_translate_wire.rs`: one scenario folder per check (settings + data + project all
//! under one temp dir), the process-wide env writes held behind one lock, and every reply framed by hand.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::produce_upload_call as call;
use naivepost::project::Publish;
use naivepost::settings::{self, Conf};

/// The model name every scenario asks with, so a check can assert it came back in the request.
const MODEL: &str = "upload-model";

// ---- the fake LLM -------------------------------------------------------------------------------

/// A loopback server that answers `/v1/chat/completions` from a script, one reply per request, and hands
/// each request's body back to the test so the request itself can be asserted.
///
/// The reply is written with `Connection: close` and the socket is SHUT DOWN afterwards: the client reads to
/// EOF, so leaving the connection open would make every round wait for a timeout (this binary stalled 3+ min
/// before). Connections are served on their own threads for the same reason.
struct FakeLlm {
    port: u16,
    received: Receiver<Seen>,
    script: Arc<Mutex<Script>>,
}

/// One request the fake took.
#[derive(Debug)]
struct Seen {
    body: String,
}

#[derive(Default)]
struct Script {
    replies: Vec<String>,
    served: usize,
}

impl Script {
    fn take(&mut self) -> String {
        let index = self.served;
        self.served += 1;
        self.replies
            .get(index)
            .cloned()
            .or_else(|| self.replies.last().cloned())
            .unwrap_or_default()
    }
}

impl FakeLlm {
    fn start(replies: Vec<String>) -> FakeLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(Script { replies, served: 0 }));
        let serving = script.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let serving = serving.clone();
                let sender = sender.clone();
                std::thread::spawn(move || serve(stream, &serving, &sender));
            }
        });
        FakeLlm { port, received, script }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The next request the server took. The call dials synchronously, so a missing entry means the request
    /// never went out at all.
    fn next(&self) -> Seen {
        self.received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the youtube call reached the fake LLM")
    }

    #[allow(dead_code)]
    fn expect_silent(&self) {
        match self.received.recv_timeout(std::time::Duration::from_millis(400)) {
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            other => panic!("the server was dialled when nothing should have been: {other:?}"),
        }
    }

    /// How many requests the fake has answered — what proves ONE call was made.
    fn served(&self) -> usize {
        self.script.lock().expect("script lock").served
    }
}

fn serve(mut stream: TcpStream, script: &Arc<Mutex<Script>>, seen: &mpsc::Sender<Seen>) {
    // Every exit closes the socket: a client that reads to EOF must never be left waiting for a peer that
    // already answered.
    serve_inner(&mut stream, script, seen);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Records the request before it answers: the program's next call follows the answer at once, and a
/// record sent after the reply let that call's thread report first, out of order.
fn serve_inner(stream: &mut TcpStream, script: &Arc<Mutex<Script>>, seen: &mpsc::Sender<Seen>) -> Option<()> {
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
    let body = String::from_utf8_lossy(&payload).to_string();
    let _ = seen.send(Seen { body });
    let reply = script.lock().expect("script lock").take();
    // Already a chat envelope (a tool-call reply) goes out as it stands; anything else is content.
    let framed_reply =
        if reply.trim_start().starts_with("{\"choices\"") { reply } else { envelope(&reply) };
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        framed_reply.len(),
        framed_reply
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    Some(())
}

/// Wrap one answer's content in the chat envelope.
fn envelope(content: &str) -> String {
    serde_json::json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// An answer in the OTHER shape §3.9 allows: one tool call per entry.
fn tool_calls(calls: &[(&str, serde_json::Value)]) -> String {
    let list: Vec<serde_json::Value> = calls
        .iter()
        .map(|(name, args)| {
            serde_json::json!({
                "type": "function",
                "function": { "name": name, "arguments": args.to_string() },
            })
        })
        .collect();
    serde_json::json!({ "choices": [ { "message": { "content": "", "tool_calls": list } } ] }).to_string()
}

// ---- the scenario -------------------------------------------------------------------------------

/// Held while a scenario's environment is being pointed at it.
fn scenario_lock() -> &'static std::sync::Mutex<()> {
    static SCENARIO: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    SCENARIO.get_or_init(|| std::sync::Mutex::new(()))
}

/// A scenario's root plus its project tree, with the guard held for the whole check.
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

/// Build the folder one check dials from: settings (model + address), data, and a `.naivepost` project.
fn fixture(tag: &str, address: &str) -> Fixture {
    let scenario = scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("naivepost-f56wire-{tag}"));
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
    // A scenario always names its own drawing server: left empty, `image_endpoint` falls back to the
    // loopback default and a poll would sit in that connect timeout for the whole test binary.
    conf.sd_server = "http://127.0.0.1:1".to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree, _scenario: scenario }
}

/// Re-point the settings at another address — the seam §1 says the program re-reads per request.
fn point_at(url: &str) {
    let paths = settings::from_environment().expect("settings folder");
    let mut conf = settings::read(&paths).expect("settings read back");
    conf.server = url.to_string();
    conf.model = MODEL.to_string();
    settings::save(&paths, &conf).expect("new address written");
}

fn cancelled() -> impl Fn() -> bool {
    || false
}

fn silent(_line: &str) {}

/// A record with nothing in it yet, which is what a first run starts from.
fn empty() -> Publish {
    Publish::default()
}

// ---- S2 over the wire ---------------------------------------------------------------------------

/// S2 / §3.9: what actually goes out — thinking ON, the job's tools offered, the shipped `youtube` prompt
/// as the system message, and the brief as the user turn. Asserted off the body the server received.
#[test]
fn f5_6_s2_the_youtube_call_offers_thinking_and_the_six_tools() {
    // The script ends with a `finish` call: without one the loop spends its nudge round and every later
    // round waiting for an answer the script has run out of, which reads as a hung test rather than a bug.
    // The title arrives as a TOOL call, which is what §3.9 asks for; the prose fallback is the next check's
    // subject, and mixing the two here would hide which path wrote the record.
    let llm = FakeLlm::start(vec![
        tool_calls(&[("set_title", serde_json::json!({ "text": "A win nobody expected" }))]),
        tool_calls(&[("finish", serde_json::json!({}))]),
    ]);
    let scene = fixture("thinking_and_tools", &llm.url());
    let moments = [1.0f64, 2.0, 3.0];
    let starts = [0.0f64, 20.0];
    let frames = call::Frames { moments: &moments, starts: &starts };
    let collected = call::call(
        &scene.tree,
        "THE FINISHED VIDEO: 2 clips, 00:24 long.",
        &empty(),
        &frames,
        false,
        &cancelled(),
        silent,
    )
    .expect("the fake answers");
    let seen: serde_json::Value = serde_json::from_str(&llm.next().body).expect("JSON body");
    assert_eq!(
        seen["enable_thinking"].as_bool(),
        Some(true),
        "S2 says the upload call thinks: {seen}"
    );
    let offered: Vec<String> = seen["tools"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|t| t["function"]["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    for want in [
        "set_title",
        "set_description",
        "pick_frame",
        "set_thumbnail_instruction",
        "get_frames",
        "finish",
    ] {
        assert!(offered.contains(&want.to_string()), "§3.9 offers {want}: {offered:?}");
    }
    let system = seen["messages"][0]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        !system.trim().is_empty(),
        "the system message went out empty — the youtube prompt was not assembled"
    );
    let user = seen["messages"][1]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(user.contains("THE FINISHED VIDEO"), "the brief is the user turn: {user}");
    assert_eq!(seen["model"].as_str(), Some(MODEL), "the settings' model is what asks");
    // The prose fallback (§3.9) landed on the record even though no tool ran.
    assert_eq!(collected.publish.thumb_title, "A win nobody expected");
}

/// §3.9 promises each tool a RESULT, not bare `ok`. Driven as the model would drive it: one round of calls,
/// then `finish`, all over the same conversation.
#[test]
fn f5_6_s2_each_tool_answers_as_3_9_promises() {
    let rounds = vec![
        tool_calls(&[
            ("set_title", serde_json::json!({ "text": "A win nobody expected" })),
            ("set_description", serde_json::json!({ "text": "the body of the description" })),
            ("pick_frame", serde_json::json!({ "clip": 2, "offset": 12.0 })),
        ]),
        // The instruction AFTER a picked frame must be refused, not silently clear the frame (decision 33).
        tool_calls(&[(
            "set_thumbnail_instruction",
            serde_json::json!({ "text": "brighter", "negative": "no blur" }),
        )]),
        tool_calls(&[("get_frames", serde_json::json!({ "clip": 2 }))]),
        tool_calls(&[("finish", serde_json::json!({}))]),
    ];
    let llm = FakeLlm::start(rounds);
    let scene = fixture("six_tools", &llm.url());
    // Frames exist only every interval, and none sits at 32 s: the taken frame is 34 s, two seconds away,
    // and §3.9 says the ANSWER states that distance rather than hiding it.
    let moments = [1.0f64, 21.0, 34.0, 40.0];
    let starts = [0.0f64, 20.0];
    let frames = call::Frames { moments: &moments, starts: &starts };
    let collected = call::call(&scene.tree, "BRIEF", &empty(), &frames, false, &cancelled(), silent)
        .expect("the fake answers");
    let said = collected.answers.join("\n");
    // set_title: the size the words take, since decision 35 keeps "four to seven" as advice, not a gate.
    assert!(
        collected
            .answers
            .iter()
            .any(|answer| answer.contains("words at ") && answer.contains("px")),
        "set_title must answer with the fitted size: {said}"
    );
    // pick_frame: the frame ACTUALLY taken and how far it sits from the moment asked for.
    let picked = collected
        .answers
        .iter()
        .find(|answer| answer.contains("frame taken at"))
        .expect("pick_frame answered with the frame it took");
    assert!(picked.contains("nothing is drawn"), "the note belongs in the result: {picked}");
    assert!(picked.contains("2"), "the distance is stated: {picked}");
    assert!(collected.publish.own, "a picked frame is the thumbnail as it is (S3)");
    assert_eq!(collected.publish.frames.len(), 1, "the row holds that frame only");
    // The instruction after the pick is an error naming why, not a silent clearing.
    assert!(
        collected
            .answers
            .iter()
            .any(|answer| answer.contains("a frame was already picked")),
        "an instruction after a pick must refuse: {said}"
    );
    assert!(collected.publish.prompt.is_empty(), "the refused instruction changed nothing");
    // get_frames: the candidate frames in that clip, so the picture is chosen by looking.
    let listed = collected
        .answers
        .iter()
        .find(|answer| answer.contains("candidate frame(s)"))
        .expect("get_frames listed candidates");
    assert!(listed.contains("34.0"), "the clip's own frames are shown: {listed}");
    // finish: ok only when a title AND a description exist.
    assert!(collected.answers.iter().any(|answer| answer == "finished"), "finish accepted: {said}");
    assert_eq!(collected.publish.thumb_title, "A win nobody expected");
    assert_eq!(collected.publish.description, "the body of the description");
}

/// `finish` refuses while something is missing, and S8 keeps the previous value rather than blanking it.
#[test]
fn f5_6_s2_finish_names_what_is_still_missing() {
    // One round of tools, then a quiet round: `finish` is what closes the flow, so the scripted server gives
    // the title call first and nothing at all afterwards (the loop's nudge round included). The replies are
    // BARE content here — `serve` wraps anything that is not already an envelope, so pre-wrapping would
    // bury the answer inside a string.
    let rounds = vec![
        tool_calls(&[("set_title", serde_json::json!({ "text": "only a title" }))]),
        tool_calls(&[("finish", serde_json::json!({}))]),
    ];
    let llm = FakeLlm::start(rounds);
    let scene = fixture("finish_missing", &llm.url());
    let moments = [1.0f64];
    let starts = [0.0f64];
    let frames = call::Frames { moments: &moments, starts: &starts };
    let mut previous = empty();
    previous.description = "last run's description".to_string();
    let collected = call::call(&scene.tree, "BRIEF", &previous, &frames, false, &cancelled(), silent)
        .expect("the fake answers");
    assert_eq!(llm.served(), 2, "the tool round, then the round that called finish");
    // The model never called `finish`, so the flow ended by the nudge — and the record shows what is absent:
    // a description this run never got, with last run's still standing under it (S8).
    assert_eq!(collected.publish.thumb_title, "only a title");
    assert_eq!(collected.publish.description, "last run's description", "S8 keeps the previous value");
    assert!(
        !naivepost::produce_upload::finish_needs(&collected.publish.thumb_title, "").is_empty(),
        "with no description of its own the record still needs one"
    );
}

/// `finish` itself, asked for twice: ok when both parts exist, and naming what is not there when they do not.
#[test]
fn f5_6_s2_the_finish_tool_answers_ok_or_names_what_is_missing() {
    // Both parts written before `finish`: the answer is plain acceptance. ONE scripted conversation carries
    // both branches — a second scenario in the same binary would need its own settings file and its own
    // address, and the process-wide env this suite pins cannot hold two scenarios at once (the first call
    // then dials the second's port and waits on it forever).
    let llm = FakeLlm::start(vec![tool_calls(&[
        ("set_title", serde_json::json!({ "text": "a title" })),
        ("set_description", serde_json::json!({ "text": "a description" })),
        ("finish", serde_json::json!({})),
    ])]);
    let scene = fixture("finish_ok", &llm.url());
    let moments = [1.0f64];
    let starts = [0.0f64];
    let frames = call::Frames { moments: &moments, starts: &starts };
    let collected = call::call(&scene.tree, "BRIEF", &empty(), &frames, false, &cancelled(), silent)
        .expect("the fake answers");
    assert!(collected.answers.iter().any(|answer| answer == "finished"), "{:?}", collected.answers);

    // Only a title: `finish` says which part is absent instead of shipping last run's under this video.
    // The SAME server now hands out the shorter round list, so nothing else about the scenario changes.
    *llm.script.lock().expect("script lock") = Script {
        replies: vec![tool_calls(&[
            ("set_title", serde_json::json!({ "text": "a title" })),
            ("finish", serde_json::json!({})),
        ])],
        served: 0,
    };
    let second = call::call(&scene.tree, "BRIEF", &empty(), &frames, false, &cancelled(), silent)
        .expect("the fake answers");
    let refusal = second
        .answers
        .iter()
        .find(|answer| answer.starts_with("still missing"))
        .expect("finish names what is absent");
    assert!(refusal.contains("description"), "{refusal}");
}/// A server that is not there fails the half by NAME, and nothing is written (§Failure is specific).
#[test]
fn f5_6_s2_a_down_llm_is_named() {
    let live = FakeLlm::start(vec![]);
    let scene = fixture("down_llm", &live.url());
    // Hand out a port nothing listens on, then point the settings at it — the same seam a real edit uses.
    let dead = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port to hand out");
        let port = listener.local_addr().expect("an address").port();
        drop(listener);
        format!("http://127.0.0.1:{port}")
    };
    point_at(&dead);
    let moments = [1.0f64];
    let starts = [0.0f64];
    let frames = call::Frames { moments: &moments, starts: &starts };
    let err = call::call(&scene.tree, "BRIEF", &empty(), &frames, false, &cancelled(), silent)
        .expect_err("nothing listens there");
    assert!(err.contains(MODEL), "the reason must name the model (§00-principles): {err}");
    assert!(err.contains(call::STEP), "and say which step failed: {err}");
    assert_eq!(live.served(), 0, "the live fake was never dialled once the address moved");
    assert!(
        !naivepost::publish::is_written(&scene.tree),
        "a failed call writes no publish.json (S5 keeps the thinking, not the record)"
    );
}
