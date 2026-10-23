// F4.2 — the narration CALL's own three rules, pinned at the logic layer: no window, no run bar, no
// scripted-reply door. (1) The streaming progress a call reports is ONE STEP PER CLOSED CLIP, in order,
// never one jump at the end; (2) the answer may arrive as the `{"entries":[..]}` document OR as a plain
// JSON list — both are the same answer, and an unreadable one is refused rather than read as silence;
// (3) the fit gate inside the call names WHICH clip's lines will not fit (§12 #23), measured by the same
// ladder the render runs, so a refusal says what to rewrite instead of letting Produce squeeze it later.

use naivepost::cut::Seg;
use naivepost::narrate_ask::unfitted_clips;
use naivepost::narrate_call::drive;
use naivepost::narrate_pass as pass;
use naivepost::narrate_run::Written;

/// Two clips, each 10 s of footage: enough for "N/M" to be unambiguous and for one line to overrun.
fn two_clips() -> Vec<Seg> {
    vec![seg(0.0, 10.0), seg(10.0, 20.0)]
}

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

/// One entry in the shape §F4.2 asks the model for: its own bounds echoed back, plus the placement.
fn entry(start: f64, end: f64, at: f64, text: &str) -> String {
    format!(
        "{{\"start\":{start},\"end\":{end},\"at\":{at},\"text\":\"{text}\",\"emotion\":\"calm\"}}"
    )
}

/// The wrapper document.
fn document(entries: &[String]) -> String {
    format!("{{\"entries\":[{}]}}", entries.join(","))
}

/// The same answer with no wrapper: a plain JSON list.
fn plain_list(entries: &[String]) -> String {
    format!("[{}]", entries.join(","))
}

/// A line comfortably short for a 10 s clip at the page's estimate rate.
const SHORT_LINE: &str = "Proof of stake changed the whole cost.";
/// ~420 characters: about 28 s of speech at the page's estimate rate, which no remedy in the ladder can
/// absorb (grow 4 s → 14 s; slide to the lead → 13.7 s; speed up to 1.25x still leaves 22.4 s), so this is
/// a genuine overrun and not a "tight" warning.
const TOO_LONG_LINE: &str = "And that is why every single person who ever tried to explain the whole history of consensus mechanisms to a room full of newcomers ended up writing another thousand words about it, which nobody read, and then they wrote a second thousand about why the first thousand went unread, and a third about the argument those two started online, which outlasted the upgrade itself.";

/// Everything fits: the gate is not what these first two checks are about.
fn everything_fits(_: &[Written]) -> Vec<u32> {
    Vec::new()
}

#[test]
fn f4_2_progress_reports_one_step_per_closed_clip() {
    let clips = two_clips();
    let answer = document(&[
        entry(0.0, 10.0, 1.0, SHORT_LINE),
        // Clip 2 answered with an empty line: silence is an ANSWER the writer gave, not a gap.
        entry(10.0, 20.0, 0.0, ""),
    ]);
    let mut steps: Vec<(u32, u32)> = Vec::new();
    let got = drive(
        &clips,
        |_, _| Ok(answer.clone()),
        |written, total| steps.push((written, total)),
        &everything_fits,
        |_| {},
    );
    got.expect("both clips were answered");
    // The bug this pins: a callback list holding only (2,2) means the bar arrived in one jump at the
    // end, which is exactly what §F4.2 forbids ("the bar moves per clip, not all at once").
    assert_eq!(
        steps,
        vec![(1, 2), (2, 2)],
        "one step per closed clip, in order"
    );
    // And each step is worded by the module that owns the stage, so the bar shows a sentence and not
    // a bare count.
    assert_eq!(pass::progress(1, 2), "writing 1/2 clips");
    assert_eq!(pass::progress(2, 2), "writing 2/2 clips");
}

#[test]
fn f4_2_a_plain_json_answer_is_accepted_as_well_as_the_entries_document() {
    let clips = two_clips();
    let entries = [
        entry(0.0, 10.0, 1.0, SHORT_LINE),
        entry(10.0, 20.0, 0.0, ""),
    ];
    let wrapped = document(&entries);
    let plain = plain_list(&entries);
    let from_document = drive(
        &clips,
        |_, _| Ok(wrapped.clone()),
        |_, _| {},
        &everything_fits,
        |_| {},
    )
    .expect("the wrapper document parses");
    let from_list = drive(
        &clips,
        |_, _| Ok(plain.clone()),
        |_, _| {},
        &everything_fits,
        |_| {},
    )
    .expect("a plain JSON list is the same answer");
    assert_eq!(
        from_document, from_list,
        "both shapes describe one answer and must land identically"
    );
    // What was stored is the CLIP's own bounds and the echoed placement, with the silence kept.
    assert_eq!(from_list.len(), 2);
    assert_eq!((from_list[0].start, from_list[0].end), (0.0, 10.0));
    assert_eq!(from_list[0].at, 1.0);
    assert!(
        from_list[1].text.is_empty(),
        "clip 2's silence survives the parse"
    );

    // An unreadable answer costs an attempt and SAYS so; it is never mistaken for "nothing to write".
    let mut rejected: Vec<String> = Vec::new();
    let junk = "I'm sorry, but I can't write narration for this clip.".to_string();
    let refused = drive(
        &clips,
        |_, _| Ok(junk.clone()),
        |_, _| {},
        &everything_fits,
        |_| {},
    )
    .expect_err("prose is not an entries document");
    assert!(
        refused.contains("could not be read") || refused.contains("expected a list of entries"),
        "the refusal names the shape problem: {refused}"
    );
    // Every attempt was logged as a rejection, so the log says which try failed and why.
    drive(
        &clips,
        |_, _| Ok(junk.clone()),
        |_, _| {},
        &everything_fits,
        |line| rejected.push(line.to_string()),
    )
    .expect_err("same answer, this time watching the log");
    assert_eq!(
        rejected
            .iter()
            .filter(|l| l.contains("narration attempt"))
            .count(),
        pass::attempts() as usize,
        "each rejected attempt is logged with its number: {rejected:?}"
    );
}

#[test]
fn f4_2_the_fit_gate_names_the_clip_whose_lines_will_not_fit() {
    let clips = two_clips();
    let written = |text: &str| {
        vec![
            Written {
                start: 0.0,
                end: 10.0,
                at: 1.0,
                text: SHORT_LINE.into(),
                emotion: "calm".into(),
            },
            Written {
                start: 10.0,
                end: 20.0,
                at: 0.0,
                text: text.into(),
                emotion: "calm".into(),
            },
        ]
    };
    // Short lines: nothing for the gate to complain about.
    assert_eq!(
        unfitted_clips(&clips, &written(SHORT_LINE)),
        Vec::<u32>::new(),
        "a line that fits is not named"
    );
    // The long line sits on clip 2, so the gate answers [2] and nothing else.
    assert_eq!(
        unfitted_clips(&clips, &written(TOO_LONG_LINE)),
        vec![2u32],
        "the gate names the clip whose lines overrun it"
    );
    // And the refusal the caller gets carries that clip number, phrased as the fix rather than the
    // symptom (§12 #23: rewrite shorter, do not let the render squeeze it).
    let answer = document(&[
        entry(0.0, 10.0, 1.0, SHORT_LINE),
        entry(10.0, 20.0, 0.0, TOO_LONG_LINE),
    ]);
    let refused = drive(
        &clips,
        |_, _| Ok(answer.clone()),
        |_, _| {},
        |lines| unfitted_clips(&clips, lines),
        |_| {},
    )
    .expect_err("an overrunning line is refused, not accepted");
    assert!(
        refused.contains("clip(s) 2") && refused.contains("will not fit"),
        "the refusal names the clip and the remedy: {refused}"
    );
    // Same call, same gate, short line: accepted. The gate is the only thing that differed.
    let fine = document(&[
        entry(0.0, 10.0, 1.0, SHORT_LINE),
        entry(10.0, 20.0, 0.0, SHORT_LINE),
    ]);
    let fine_answer = fine;
    drive(
        &clips,
        |_, _| Ok(fine_answer.clone()),
        |_, _| {},
        |lines| unfitted_clips(&clips, lines),
        |_| {},
    )
    .expect("a fitting answer passes the same gate");
}

// --- The wire: the call really dials the address the Settings hold ---------------------------------
//
// Headless on purpose. The same claim used to be made by pressing the shell's ▶ and watching the run bar,
// which needed the window, the bar and the scripted-reply doors all in step and could not be made
// deterministic. Here the seam is the one `narrate_live::ask_round` uses — it re-reads `llm.conf` per
// request (§02-services#1) — so nothing hands an answer to the code: the request goes out over a socket
// and the reply comes back over it.
//
// `XDG_CONFIG_HOME` is process-wide, so exactly ONE scenario lives in this binary (the guard below makes
// that a panic rather than a silent race). Nothing outside the scenario root is written: NOT
// `$HOME/.config/naivepost/llm.conf`, NOT `/etc/xdg` — sibling test binaries legitimately point
// `$HOME/.config` at their own fakes, and overwriting that file steals their endpoint.

/// Guard for the one-scenario-per-binary rule: `set_var` is process-wide.
static WIRE_SCENARIOS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A conf addressed at the fake writer, saved through the env the program will resolve. Returns the temp
/// config folder it wrote under.
fn point_the_settings_at(fake_url: &str) -> std::path::PathBuf {
    if WIRE_SCENARIOS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
        panic!("two live scenarios in one binary: the process-wide env races");
    }
    let root = std::env::temp_dir().join(format!("np-f42-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &root) };
    let paths = naivepost::settings::Paths {
        config_dir: root.join("naivepost"),
        data_dir: root.join("data"),
    };
    // `Conf::default()` + field writes: `render` is line-ordered by key and `parse` keeps what it sees
    // last, so a `..Default::default()` literal can silently leave `LLM_SERVER` empty — and an empty
    // address means the call dials the compiled-in :8731 instead of the fake.
    let mut conf = naivepost::settings::Conf::default();
    conf.server = fake_url.to_string();
    conf.model = "f42-wire-model".to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written under the temp root");
    assert!(
        naivepost::settings::from_environment()
            .and_then(|p| naivepost::settings::read(&p).ok())
            .is_some_and(|read| read.server == fake_url),
        "the address the program reads must be the fake's, not the compiled-in default"
    );
    root
}

#[test]
fn f4_2_the_writer_really_dials_the_configured_address() {
    let answer = entries_answer(&[
        (0.0, 10.0, 1.0, SHORT_LINE, "calm"),
        (10.0, 20.0, 0.0, "", ""),
    ]);
    let llm = FakeLlm::start(vec![answer]);
    let _root = point_the_settings_at(&llm.url);
    let session = _root.join("session.naivepost");
    std::fs::create_dir_all(&session).expect("session folder");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder is a project");
    naivepost::cut::save(
        &naivepost::cut::Cut {
            segs: two_clips(),
            ..Default::default()
        },
        &tree,
    )
    .expect("cut.json written");
    tree.write_file(
        std::path::Path::new("prepare/transcript/session.tsv"),
        b"1.0\t3.0\tlecture.mkv\tAnn\tThe chain moved to proof of stake.\n",
    )
    .expect("session.tsv written");
    let project = naivepost::project::Project {
        context: "A lecture about Ethereum.".to_string(),
        ..Default::default()
    };

    let request = naivepost::narrate_call::request_from_tree(
        &tree,
        &project,
        &naivepost::narrate_data::read_voice(&tree),
        "",
        "",
    );
    let round = naivepost::narrate_live::ask_round(
        &tree,
        &request,
        &two_clips(),
        &[],
        "",
        1,
        true,
        &|| false,
    );
    let round = match round {
        Ok(round) => round,
        Err(why) => panic!("the live call should have dialled the fake; it said: {why}"),
    };
    assert_eq!(
        round.requests, 2,
        "the content answer is followed by the finish nudge round"
    );
    // What arrived on the wire, from the server's side. The fake hands back the request body; the path
    // and verb are asserted through what the body itself is: a chat-completion body, not a probe.
    let seen = llm.next_body();
    assert!(
        seen.contains("messages"),
        "the chat body went out: {:.200}",
        seen
    );
    assert!(
        seen.contains("f42-wire-model"),
        "the model from the Settings file rode with the request"
    );
    // And the answer that came back over that socket is the entries document, parsing to both clips.
    let parsed = naivepost::narrate_reply::entries_of(&round.answer)
        .expect("the answer off the socket reads as narration entries");
    assert_eq!(
        parsed.len(),
        2,
        "one line and one silence, both clips answered"
    );
    assert_eq!(parsed[0].text, SHORT_LINE);
    assert!(
        parsed[1].text.is_empty(),
        "clip 2's silence survives the wire"
    );
}

/// A fake LLM on a free loopback port: answers each ask with the next scripted reply, and keeps answering
/// with the last one once they run out (so a retry sees the same answer, not a reset). Same shape as
/// `tests/suite/narrate_live_harness.rs`, which pins cwd and allows one scenario per binary — this binary needs
/// its own copy because that harness is included by path elsewhere.
struct FakeLlm {
    url: String,
    received: std::sync::mpsc::Receiver<String>,
}

impl FakeLlm {
    fn start(replies: Vec<String>) -> FakeLlm {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let (sender, received) = std::sync::mpsc::channel();
        let script = std::sync::Arc::new(std::sync::Mutex::new(replies));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let sender = sender.clone();
                let script = script.clone();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) == 0 {
                            break;
                        }
                        let lower = header.to_ascii_lowercase();
                        if let Some(value) = lower.strip_prefix("content-length:") {
                            length = value.trim().parse().unwrap_or(0);
                        }
                        if header.trim().is_empty() {
                            break;
                        }
                    }
                    let mut body = vec![0u8; length];
                    let _ = reader.read_exact(&mut body);
                    let _ = sender.send(String::from_utf8_lossy(&body).to_string());
                    let reply = script
                        .lock()
                        .map(|mut list| match list.len() {
                            0 => String::new(),
                            1 => list[0].clone(),
                            _ => list.remove(0),
                        })
                        .unwrap_or_default();
                    let framed = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{reply}",
                        reply.len()
                    );
                    let _ = stream.write_all(framed.as_bytes());
                    let _ = stream.flush();
                });
            }
        });
        FakeLlm {
            url: format!("http://127.0.0.1:{port}"),
            received,
        }
    }

    /// The next request body, waiting long enough for a real dial.
    fn next_body(&self) -> String {
        self.received
            .recv_timeout(std::time::Duration::from_secs(25))
            .expect("the narration ask reached the fake server")
    }
}

/// Wrap an entries document in the chat envelope a real server answers with.
fn entries_answer(entries: &[(f64, f64, f64, &str, &str)]) -> String {
    let list: Vec<serde_json::Value> = entries
        .iter()
        .map(|(start, end, at, text, emotion)| {
            serde_json::json!({ "start": start, "end": end, "at": at, "text": text, "emotion": emotion })
        })
        .collect();
    let document = serde_json::json!({ "entries": list }).to_string();
    serde_json::json!({ "choices": [ { "message": { "content": document } } ] }).to_string()
}
