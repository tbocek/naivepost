//! `P.machine.briefMaxChars` — the upload brief's bound, and the escape hatch that makes a bound
//! differ from a truncation.
//!
//! §08-produce.md F5.6 S1: "For long sessions the brief is bounded (P.machine.briefMaxChars,
//! 120 kB): each clip folded to its first lines and events; the model reads more with `get_lines`."
//! Two halves, and the tests follow them: **S1** the number and the fold it drives, **S2** the read
//! back — `get_lines` answered on the live call, over the same rows the brief was built from, so
//! what the fold dropped can actually be fetched. The wire test dials the program's own settings
//! seam at a loopback fake LLM: no answer is scripted inside the program.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use naivepost::narration::Entry;
use naivepost::params;
use naivepost::produce_upload as up;
use naivepost::produce_upload_call as call;
use naivepost::project::Publish;
use naivepost::settings::{self, Conf};
use naivepost::textfmt::SessionLine;
use naivepost::tools;
use serde_json::Value;

use crate::common;

/// The model name every scenario asks with.
const MODEL: &str = "briefmax-model";

// ---- the fake LLM (the shape tests/suite/produce_upload_wire.rs uses: one reply per round) ----

struct FakeLlm {
    port: u16,
    received: Receiver<Seen>,
    script: Arc<Mutex<Script>>,
}

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

    /// How many requests the fake has answered — what proves how many rounds ran.
    fn served(&self) -> usize {
        self.script.lock().expect("script lock").served
    }
}

fn serve(mut stream: TcpStream, script: &Arc<Mutex<Script>>, seen: &mpsc::Sender<Seen>) {
    serve_inner(&mut stream, script, seen);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Records the request BEFORE answering: the program's next call follows the answer, so a record
/// sent after the reply would let that call's thread report first, out of order.
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
    let framed_reply = if reply.trim_start().starts_with("{\"choices\"") {
        reply
    } else {
        envelope(&reply)
    };
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

/// An answer as tool calls, one entry per call.
fn tool_calls(calls: &[(&str, Value)]) -> String {
    let list: Vec<Value> = calls
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

/// The `tools` array of a request body, as the names the server was handed.
fn offered_names(body: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(body).expect("the chat body is JSON");
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("the request carried no tools array: {body}"));
    tools
        .iter()
        .map(|tool| {
            tool.pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("<unnamed>")
                .to_string()
        })
        .collect()
}

/// Every message role in a request body, as `role` strings.
fn roles_of(body: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(body).expect("the chat body is JSON");
    value["messages"]
        .as_array()
        .expect("the body carries messages")
        .iter()
        .map(|m| m["role"].as_str().unwrap_or("?").to_string())
        .collect()
}

// ---- the scenario folder -------------------------------------------------------------


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

/// Settings (model + address) and a `.naivepost` folder under one temp root, guard held throughout.
fn fixture(tag: &str, address: &str) -> Fixture {
    let scenario = common::scenario_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = std::env::temp_dir().join(format!("np-briefmax-{tag}-{}", std::process::id()));
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
    // No drawing server in this item: point it at a closed port so nothing could wait on one.
    conf.sd_server = "http://127.0.0.1:1".to_string();
    settings::save(&paths, &conf).expect("settings written");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    Fixture { root, tree, _scenario: scenario }
}

fn cancelled() -> impl Fn() -> bool {
    || false
}

fn silent(_line: &str) {}

fn empty() -> Publish {
    Publish::default()
}

// ---- fixtures for the logic half -----------------------------------------------------

/// A clip list of `n` twelve-second clips starting at 0, each carrying `lines` seen and said rows so
/// the whole thing has a size a fold can be measured against.
fn clips_with(n: usize, lines: usize) -> Vec<up::BriefClip> {
    (0..n)
        .map(|i| up::BriefClip {
            number: i + 1,
            at_output: i as f64 * 12.0,
            seconds: 12.0,
            session: format!(
                "{}\u{2013}{}",
                tools::mm_ss(i as f64 * 12.0),
                tools::mm_ss(i as f64 * 12.0 + 12.0)
            ),
            seen: (0..lines).map(|k| format!("clip {} event {}", i + 1, k + 1)).collect(),
            said: (0..lines).map(|k| format!("clip {} spoken {}", i + 1, k + 1)).collect(),
            rate: 1.0,
            marked: Vec::new(),
        })
        .collect()
}

fn row(start: f64, end: f64, who: &str, text: &str) -> SessionLine {
    SessionLine {
        start,
        end,
        source: "cam.mp4".to_string(),
        who: who.to_string(),
        text: text.to_string(),
    }
}

fn entry(at: f64, text: &str) -> Entry {
    Entry {
        s: at,
        e: at + 2.0,
        at,
        text: text.to_string(),
        emotion: "calm".to_string(),
        pos: String::new(),
        roll: 0,
    }
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The bound that forces a fold: one character under the length `brief` would need with nothing
/// folded. Derived from the printed brief rather than guessed.
fn unbounded_estimate(clips: &[up::BriefClip], lines: &[Entry]) -> usize {
    up::brief(clips, lines, usize::MAX).len()
}

// ---- S1: the bound and the fold it drives --------------------------------------------

/// The number is the one §10 spells, and it is catalogued under this item's id exactly once.
/// (`params::find()` only searches the Prepare section, so it cannot see a `P.machine.` id — the
/// produce listing is where this row lives.)
#[test]
fn p_machine_briefmaxchars_s1_the_bound_is_the_number_the_catalogue_names() {
    assert_eq!(up::BRIEF_MAX_CHARS, 120_000, "§10 §4: 120 kB");
    let listed = params::produce();
    let rows: Vec<&params::Param> = listed
        .iter()
        .filter(|row| row.id == "P.machine.briefMaxChars")
        .collect();
    assert_eq!(rows.len(), 1, "the catalogue lists this id {} times", rows.len());
    assert_eq!(rows[0].from, "produce_upload::BRIEF_MAX_CHARS", "one home for the number");
}

/// Under the bound nothing is folded: every seen and said line reaches the brief, and the fold
/// sentence appears nowhere.
#[test]
fn p_machine_briefmaxchars_s1_a_brief_under_the_bound_is_not_folded() {
    let clips = clips_with(3, 5);
    let shown = up::brief(&clips, &[], up::BRIEF_MAX_CHARS);
    assert!(!shown.contains("(folded"), "an under-bound brief must not fold:\n{shown}");
    assert_eq!(count(&shown, "  SEEN: "), 15, "three clips x five events");
    assert_eq!(count(&shown, "  SAID: "), 15, "three clips x five lines");
    assert!(shown.starts_with("THE FINISHED VIDEO: 3 clips, 00:36 long.\n\nWHAT IS IN EACH CLIP:\n"));
    assert!(shown.contains("CLIP 2 (at 00:12 in the video, 12.0 s): session 00:12\u{2013}00:24"));
    assert!(shown.contains("THE NARRATION SPOKEN OVER IT, at its time in the finished video:"));
    assert!(shown.contains("(no narration has been written for this video)"));
}

/// Over the bound each clip keeps its first `FOLDED_LINES` lines and events, and says so naming
/// `get_lines` — the sentence is what makes a bound differ from cutting the tail off.
#[test]
fn p_machine_briefmaxchars_s1_over_the_bound_each_clip_folds_to_its_first_lines() {
    let clips = clips_with(4, 6);
    // `brief` decides the fold on `unbounded_length`, an estimate of the whole brief — so "no fold" is
    // shown by asking for a bound above that estimate (the app's real one) and the fold is forced by
    // asking for one character less than the estimate needs. No size of this fixture is hard-coded.
    let full = up::brief(&clips, &[], up::BRIEF_MAX_CHARS * 2);
    assert_eq!(count(&full, "  SAID: "), 24, "nothing folded when nothing has to be");
    // A bound of 1 is "this session's brief cannot fit in anything": every clip folds. The point the
    // test pins is HOW it folds (first FOLDED_LINES, and the sentence naming get_lines), not the size.
    let folded = up::brief(&clips, &[], 1);
    assert_eq!(
        count(&folded, "  SAID: "),
        4 * up::FOLDED_LINES,
        "each clip keeps its first {} said lines: {folded}",
        up::FOLDED_LINES
    );
    assert_eq!(count(&folded, "  SEEN: "), 4 * up::FOLDED_LINES, "and the same for events");
    assert_eq!(
        count(&folded, "  (folded \u{2014} read the rest with get_lines)"),
        4,
        "every folded clip says so"
    );
    assert!(folded.contains("get_lines"), "the escape hatch is named: {folded}");
    // The kept ones are the FIRST ones, not an arbitrary slice.
    assert!(folded.contains("clip 1 spoken 1"), "{folded}");
    assert!(!folded.contains("clip 1 spoken 6"), "the tail is what got folded: {folded}");
}

/// The narration side folds too, and says so; with no lines at all the brief still answers the
/// spec's own sentence rather than printing an empty heading.
#[test]
fn p_machine_briefmaxchars_s1_the_fold_says_so_on_the_narration_side_too() {
    let clips = clips_with(1, 1);
    let lines: Vec<Entry> = (0..7).map(|i| entry(i as f64 * 3.0, &format!("spoken line {i}"))).collect();
    let full = up::brief(&clips, &lines, up::BRIEF_MAX_CHARS * 2);
    assert_eq!(count(&full, "spoken line "), 7, "under the bound every line prints");
    // Same seam as above: one character below the estimate is what folds.
    let folded = up::brief(&clips, &lines, unbounded_estimate(&clips, &lines) - 1);
    assert_eq!(
        count(&folded, "spoken line "),
        up::FOLDED_LINES,
        "only the first {} narration lines survive the fold: {folded}",
        up::FOLDED_LINES
    );
    assert!(
        folded.contains("(folded \u{2014} read the rest with get_lines)"),
        "the narration fold points at get_lines too: {folded}"
    );
    let quiet = up::brief(&clips, &[], usize::MAX);
    assert!(quiet.contains("(no narration has been written for this video)"));
}

// ---- S2: reading back what the fold dropped ------------------------------------------

/// `read_back` answers over the SAME rows the brief was built from, stamped like the brief's own
/// lines, and bounds itself at the one row cap the app already has.
#[test]
fn p_machine_briefmaxchars_s2_read_back_returns_what_the_fold_dropped() {
    let rows = vec![
        row(0.0, 2.0, "Ann", "the opening"),
        row(21.0, 23.0, "Ann", "inside the window"),
        row(25.0, 26.0, "Ben", "also inside"),
        row(28.5, 30.0, "Ann", "right at the edge"),
        row(60.0, 61.0, "Ben", "far outside"),
    ];
    let answer = up::read_back(&rows, 20.0, 30.0);
    let parsed: Value = serde_json::from_str(&answer).expect("the answer is JSON");
    let lines = parsed["lines"].as_array().expect("the answer carries lines");
    assert_eq!(lines.len(), 3, "exactly the three rows in the window: {answer}");
    assert_eq!(parsed["from"], serde_json::json!(20.0));
    assert_eq!(parsed["to"], serde_json::json!(30.0));
    assert_eq!(
        lines[0].as_str().unwrap(),
        format!("{} {}: {}", tools::mm_ss(21.0), "Ann", "inside the window"),
        "stamped the way the brief stamps its own narration lines"
    );
    assert_eq!(parsed["truncated"], serde_json::json!(false));

    // An empty window is said, not left ambiguous.
    let none: Value = serde_json::from_str(&up::read_back(&rows, 40.0, 50.0)).expect("JSON");
    assert_eq!(none["lines"].as_array().expect("lines").len(), 0);
    let note = none["note"].as_str().expect("a note naming the window");
    assert!(
        note.contains(&tools::mm_ss(40.0)) && note.contains(&tools::mm_ss(50.0)),
        "the note names the seconds it looked between: {note}"
    );

    // Past the cap the answer says it cut rows off rather than answering less than it was asked.
    let many: Vec<SessionLine> = (0..(naivepost::narrate_pass::GET_LINES_MAX_ROWS + 5))
        .map(|i| row(i as f64, i as f64 + 0.5, "Ann", &format!("line {i}")))
        .collect();
    let capped: Value = serde_json::from_str(&up::read_back(&many, 0.0, 1e6)).expect("JSON");
    assert_eq!(
        capped["lines"].as_array().expect("lines").len(),
        naivepost::narrate_pass::GET_LINES_MAX_ROWS,
        "one row cap, the one narrate_pass already sets"
    );
    assert_eq!(capped["truncated"], serde_json::json!(true), "and it is admitted: {capped}");
}

/// THE wire proof: the live `youtube` call answers `get_lines` itself. The fake LLM asks for the
/// window on round 1, the program answers from the rows it was handed, and the SECOND request the
/// fake receives carries that answer back — so the folded brief's instruction leads somewhere.
#[test]
fn p_machine_briefmaxchars_s2_the_live_upload_call_answers_get_lines_over_the_wire() {
    let llm = FakeLlm::start(vec![
        tool_calls(&[("get_lines", serde_json::json!({ "from": 20.0, "to": 30.0 }))]),
        tool_calls(&[
            ("set_title", serde_json::json!({ "text": "A title that fits" })),
            ("set_description", serde_json::json!({ "text": "the description body" })),
        ]),
        tool_calls(&[("finish", serde_json::json!({}))]),
    ]);
    let scene = fixture("get_lines", &llm.url());
    let rows = vec![
        row(2.0, 4.0, "Ann", "before the window"),
        row(21.0, 22.0, "Ann", "the line the fold dropped"),
        row(29.0, 29.5, "Ben", "the last one asked for"),
        row(80.0, 81.0, "Ben", "well past it"),
    ];
    let moments = [1.0f64];
    let starts = [0.0f64];
    let frames = call::Frames {
        moments: &moments,
        starts: &starts,
        interval: 1.0,
        lines: &rows,
    };
    let collected = call::call(
        &scene.tree,
        "THE FINISHED VIDEO: 1 clips, 0:12 long. (folded \u{2014} read the rest with get_lines)",
        &empty(),
        &frames,
        false,
        &cancelled(),
        silent,
    )
    .expect("the fake answers");

    // (a) the read was ANSWERED, not refused as an unknown tool.
    let said = collected.answers.join("\n");
    assert!(
        !said.contains("no such tool"),
        "the fold's escape hatch fell through: {said}"
    );
    let expected = format!("{} {}: {}", tools::mm_ss(21.0), "Ann", "the line the fold dropped");
    assert!(
        collected.answers.iter().any(|answer| answer.contains(&expected)),
        "the read-back carries the stamped line: {said}"
    );
    assert!(
        !collected.answers.iter().any(|answer| answer.contains("well past it")),
        "the window bounded the answer: {said}"
    );
    assert_eq!(collected.publish.thumb_title, "A title that fits", "the conversation continued");
    assert_eq!(collected.publish.description, "the description body");

    // (b) the second request carried the tool result back, and still offered the tool. The fake
    // records every request in order and the call is synchronous, so all rounds are recorded by now.
    let bodies: Vec<String> = drain_bodies(&llm);
    assert!(bodies.len() >= 2, "the call made at least two requests: {}", bodies.len());
    let second = &bodies[1];
    assert!(
        roles_of(second).contains(&"tool".to_string()),
        "the follow-up request carried no tool result back: {:?}",
        roles_of(second)
    );
    assert!(
        second.contains("the line the fold dropped"),
        "the answer the model was owed is missing from its next request: {second}"
    );
    assert!(
        offered_names(second).iter().any(|name| name == "get_lines"),
        "get_lines stopped being offered mid-conversation: {:?}",
        offered_names(second)
    );
    assert_eq!(llm.served(), 3, "read, write, finish — three rounds");
}

/// Everything the fake has recorded so far, oldest first. The channel is FIFO and the call is
/// synchronous, so by the time `call` returns every request it made has been recorded.
fn drain_bodies(llm: &FakeLlm) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(seen) = llm.received.try_recv() {
        out.push(seen.body);
    }
    out
}
