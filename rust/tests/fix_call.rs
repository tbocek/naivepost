//! §02-services.md **§2 Model roles**, the "clean transcript" row: a source's ASR lines go to the LLM,
//! **thinking off**, and the answer is the same lines respelled — times and speakers unchanged, and
//! *enforced*, not checked afterwards.
//!
//! Item id: `sec_02_services_2_model_roles_which_job_asks_which_server`. Before
//! [`naivepost::fix_call`] existed, `fix_transcripts::fix_block` took an `ask` closure that nothing in
//! `src` supplied: the respelling job had no request leg at all, so no block ever reached a server.
//! These tests drive the door the Prepare walk reaches (`run_stages::plan` → the `TEXT_TASK` row →
//! `fix_call::run_task` → `stage_chunk`) against a loopback LLM answering with §3.3's `fix_line` calls,
//! pointed at through the Settings box exactly as §1's re-read-per-request seam allows. So what is
//! proven is the program's request and the program's own file, not a substitute's.
//!
//! The enforcement is structural and these tests say so out loud: `tools::fix::Block::fix_line(n, text)`
//! takes no time and no speaker argument, so a reply has nowhere to put either. That is why one test
//! sends only `flag_line` notes — the channel §3.3 gives a model for "the speaker on this row is wrong"
//! — and the fixed file comes back with every original time and speaker still standing.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use naivepost::settings::{self, Conf, Paths};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The four ASR rows every scenario starts from. Times and speakers are what must survive a fix, so
/// they are deliberately uneven (not all `x.00`) and share one speaker label.
const SOURCE_ROWS: [(&str, &str, &str, &str); 4] = [
    ("0.00", "1.50", "S1", "so the algorithum recieves the input"),
    ("1.50", "3.25", "S1", "and writes it down again"),
    ("3.25", "5.00", "S1", "wich is where it gets faster"),
    ("5.00", "6.75", "S1", "than doing it twice"),
];

/// One `fix_line` call inside the chat envelope §3.3's tool surface answers with.
fn fix_answer(n: u32, text: &str) -> String {
    let args = serde_json::json!({ "n": n, "text": text }).to_string();
    let escaped = args.replace('"', "\\\"");
    format!(
        "{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[\n{{\"function\":{{\"name\":\"fix_line\",\"arguments\":\"{escaped}\"}}}},\n{{\"function\":{{\"name\":\"finish\",\"arguments\":\"{{}}\"}}}}\n]}}}}]}}"
    )
}

/// A reply whose only calls are `flag_line` notes: the model saw something wrong it may not change.
fn flag_answer(n: u32, why: &str) -> String {
    let args = serde_json::json!({ "n": n, "why": why }).to_string();
    let escaped = args.replace('"', "\\\"");
    format!(
        "{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[\n{{\"function\":{{\"name\":\"flag_line\",\"arguments\":\"{escaped}\"}}}},\n{{\"function\":{{\"name\":\"finish\",\"arguments\":\"{{}}\"}}}}\n]}}}}]}}"
    )
}

/// What the fake answers with, and how many chat asks it has taken.
struct Fake {
    url: String,
    asks: std::sync::Arc<Mutex<Asks>>,
    /// The reply it gives, read at ask time so a scenario can change its mind between asks.
    answer: std::sync::Arc<Mutex<String>>,
}

#[derive(Default)]
struct Asks {
    /// One entry per chat request received: the path it came to and its body.
    bodies: Vec<ChatAsk>,
}

#[derive(Debug, Clone)]
struct ChatAsk {
    path: String,
    body: String,
}

type Mutex<T> = std::sync::Mutex<T>;

impl Fake {
    /// A fake LLM that answers every `POST /v1/chat/completions` with the held reply.
    fn start(answer: String) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let asks = std::sync::Arc::new(Mutex::new(Asks::default()));
        let recorder = asks.clone();
        let held = std::sync::Arc::new(Mutex::new(answer));
        let replies = held.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let recorder = recorder.clone();
                let replies = replies.clone();
                std::thread::spawn(move || {
                    let answer = replies.lock().expect("the reply lock").clone();
                    if let Some(ask) = serve(stream, &answer) {
                        if let Ok(mut row) = recorder.lock() {
                            row.bodies.push(ask);
                        }
                    }
                });
            }
        });
        Fake { url: format!("http://127.0.0.1:{port}"), asks, answer: held }
    }

    /// The chat requests this fake has recorded. Recording happens on the connection's own thread, so
    /// a reply can return before its entry is in the list; callers wait for the count they expect.
    fn waited(&self, count: usize) -> Vec<ChatAsk> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let seen = self.asks.lock().expect("the record lock").bodies.clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn serve(mut stream: TcpStream, answer: &str) -> Option<ChatAsk> {
    let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
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
    let payload = answer.to_string();
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(ChatAsk { path, body: String::from_utf8_lossy(&body).into_owned() })
}

/// One recording, its transcript on disk, and its own settings folder pointing at its own fake LLM.
///
/// The scene is handed to the door explicitly (`tree`/`paths`), which is how the stage is tested
/// without pretending a thread owns the process: cargo runs these tests as threads of one binary, so a
/// scenario that chdir'd and set `XDG_CONFIG_HOME` would move every other thread's cwd and settings
/// with it. Nothing here touches the environment.
struct Scene {
    root: PathBuf,
    paths: Paths,
}

impl Scene {
    fn new(label: &str, llm_url: &str) -> Scene {
        let root = std::env::temp_dir().join(format!(
            "np-fix-{label}-{}",
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
        // §6's transcript rows, written by the same writer the ASR step uses so the shape is real.
        let lines: Vec<naivepost::textfmt::Line> = SOURCE_ROWS
            .iter()
            .map(|(start, end, speaker, text)| naivepost::textfmt::Line {
                start: start.parse().expect("a number"),
                end: end.parse().expect("a number"),
                speaker: (*speaker).to_string(),
                text: (*text).to_string(),
            })
            .collect();
        naivepost::textfmt::write_lines(&lines, &tree.transcript_tsv(LANE)).expect("the transcript");
        settings::save(&paths, &conf_at(llm_url)).expect("the conf");
        Scene { root, paths }
    }

    fn tree(&self) -> naivepost::layout::Tree {
        naivepost::layout::Tree::new(&self.session()).expect("the session is a project")
    }

    /// Fix this scene's transcript the way the stage does, with the scene's own tree and settings
    /// folder: [`naivepost::fix_call::stage_chunk`] is the door `run_task` opens after it resolves them
    /// from the environment, and it is the same door.
    fn fix(&self, conf: &Conf, log: &mut impl FnMut(&str)) -> Result<usize, String> {
        naivepost::fix_call::stage_chunk(&self.tree(), &self.paths, conf, LANE, true, false, log)
    }

    fn session(&self) -> PathBuf {
        self.root.join("session.naivepost")
    }

    fn read(&self, path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn source(&self) -> String {
        self.read(&self.tree().transcript_tsv(LANE))
    }

    fn fixed(&self) -> String {
        self.read(&self.tree().transcript_fixed_tsv(LANE))
    }
}

/// The lane name both files are filed under.
const LANE: &str = "lecture";

/// The conf a test hands the stage directly: the shipped shape with the fake's address and a model id,
/// written to the scene's own settings folder (the file the stage re-reads per request, §1).
fn conf_at(url: &str) -> Conf {
    let mut conf = Conf::default();
    conf.server = url.to_string();
    conf.model = "text-model".to_string();
    conf
}

/// A fake LLM that answers each ask from a QUEUE of replies, popping one per request and repeating the
/// last once the queue is empty. [`Fake::start`] is the one-reply case; a scenario whose model asks for a
/// read first needs the answer to depend on which round it is.
fn scripted(replies: Vec<String>) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let asks = std::sync::Arc::new(Mutex::new(Asks::default()));
    let recorder = asks.clone();
    let queue = std::sync::Arc::new(Mutex::new(replies));
    let serving = queue.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let recorder = recorder.clone();
            let serving = serving.clone();
            std::thread::spawn(move || {
                let answer = {
                    let mut left = serving.lock().expect("the queue lock");
                    if left.len() > 1 {
                        left.remove(0)
                    } else {
                        left.first().cloned().unwrap_or_default()
                    }
                };
                if let Some(ask) = serve(stream, &answer) {
                    if let Ok(mut row) = recorder.lock() {
                        row.bodies.push(ask);
                    }
                }
            });
        }
    });
    Fake { url: format!("http://127.0.0.1:{port}"), asks, answer: std::sync::Arc::new(Mutex::new(String::new())) }
}

/// One call inside the chat envelope, with whatever arguments the caller built.
fn call_answer(name: &str, args: serde_json::Value) -> String {
    let escaped = args.to_string().replace('"', "\\\"");
    format!(
        "{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[\n{{\"function\":{{\"name\":\"{name}\",\"arguments\":\"{escaped}\"}}}}\n]}}}}]}}"
    )
}

/// The message list of one recorded ask, as JSON — so a test can look at what the model was shown on
/// its second round, including the tool result the app fed back.
fn messages_of(ask: &ChatAsk) -> Vec<serde_json::Value> {
    let sent: serde_json::Value = serde_json::from_str(&ask.body).expect("a JSON body");
    sent["messages"].as_array().cloned().unwrap_or_default()
}

/// The text of every message whose role is `tool`, joined: what the app answered the model's calls with.
fn tool_answers(ask: &ChatAsk) -> String {
    messages_of(ask)
        .iter()
        .filter(|message| message["role"].as_str() == Some("tool"))
        .flat_map(|message| message["content"].as_array().cloned().unwrap_or_default())
        .filter_map(|part| part["text"].as_str().map(str::to_string))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Fields of one §6 row: `start`, `end`, speaker, then the text (which may hold anything else).
fn fields(row: &str) -> Vec<&str> {
    row.split('\t').take(4).collect()
}

/// §2's row on the wire: the block goes out numbered with its times and speaker, the model is asked for
/// exactly that count, thinking is OFF in both places the switch is written, and §3.3's tools are
/// offered so the answer can be a `fix_line` per line.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_asks_the_llm_with_the_block_and_thinking_off(
) {
    let fake = Fake::start(fix_answer(1, "so the algorithm receives the input"));
    let scene = Scene::new("wire", &fake.url);
    let mut log = Vec::new();
    let blocks = scene
        .fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string()))
        .expect("the stage fixes the row");
    let asks = fake.waited(1);
    assert_eq!(blocks, 1, "four lines fit one block of P.machine.fixBlockLines");
    assert_eq!(asks.len(), 1, "one ask for one block: {asks:?}");
    assert!(
        asks[0].path.contains("/v1/chat/completions"),
        "the fix job is a chat call: {}",
        asks[0].path
    );
    let sent: serde_json::Value = serde_json::from_str(&asks[0].body).expect("a JSON body");
    // The last message is the user's block, spelled as `fix_transcripts::ask_text` spells it. The wire
    // field is `content` (an array of parts; a text part carries no `image_url`).
    let messages = sent["messages"].as_array().expect("the messages");
    let user = messages
        .iter()
        .rev()
        .find(|message| message["role"].as_str() == Some("user"))
        .expect("a user message")["content"]
        .as_array()
        .expect("the user parts")
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<&str>>()
        .join("\n");
    assert!(
        user.contains("Transcript lines to clean (4 lines, return exactly 4)"),
        "the ask names the count it wants back: {user}"
    );
    for (index, (start, end, speaker, text)) in SOURCE_ROWS.iter().enumerate() {
        let row = format!("{n}\t{start}\t{end}\t{speaker}\t{text}", n = index + 1);
        assert!(user.contains(&row), "line {} went out whole: {user}", index + 1);
    }
    // §2's thinking column for this row is OFF. `llm_request` writes the switch in both places a
    // server reads it, so both copies have to say false.
    assert_eq!(sent["enable_thinking"], serde_json::json!(false), "thinking OFF: {sent}");
    assert_eq!(
        sent["chat_template_kwargs"]["enable_thinking"],
        serde_json::json!(false),
        "thinking OFF in the template copy too: {sent}"
    );
    let offered: Vec<&str> = sent["tools"]
        .as_array()
        .expect("the §3.3 tools are offered")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    for name in ["fix_line", "flag_line", "finish"] {
        assert!(offered.contains(&name), "{name} is offered to the fix job: {offered:?}");
    }
}

/// The answer lands: line 1 keeps its own times and speaker and carries the respelling, and the rows
/// nobody fixed stand untouched. Same row count, same first three fields.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_rewrites_only_the_text_column(
) {
    let respelled = "so the algorithm receives the input";
    let fake = Fake::start(fix_answer(1, respelled));
    let scene = Scene::new("files", &fake.url);
    let source = scene.source();
    let mut log = Vec::new();
    scene.fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string())).expect("the fix ran");
    fake.waited(1);
    let fixed = scene.fixed();
    let before: Vec<&str> = source.lines().filter(|row| !row.is_empty()).collect();
    let after: Vec<&str> = fixed.lines().filter(|row| !row.is_empty()).collect();
    assert_eq!(after.len(), before.len(), "a fix never adds or drops a row: {fixed}");
    let first = fields(after[0]);
    assert_eq!(first[3], respelled, "the model's spelling is what the row holds: {fixed}");
    // Times and speaker byte-identical to the source row: `fix_line(n, text)` has nowhere to change them.
    let origin = fields(before[0]);
    assert_eq!(first[..3], origin[..3], "start, end and speaker are untouched: {fixed}");
    for (index, (row, origin)) in after.iter().zip(&before).enumerate().skip(1) {
        assert_eq!(fields(row), fields(origin), "line {} was not asked about: {row}", index + 1);
    }
}

/// A reply cannot move a time or a speaker, because the tool surface has no place to ask for one. The
/// channel for "this row's speaker is wrong" is `flag_line`, which notes it and leaves the row alone.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_a_reply_cannot_move_a_time_or_a_speaker(
) {
    let fake = Fake::start(flag_answer(2, "the speaker on this row is the interviewer, not S1"));
    let scene = Scene::new("flags", &fake.url);
    let source = scene.source();
    let mut log = Vec::new();
    scene.fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string())).expect("the fix ran");
    fake.waited(1);
    let fixed = scene.fixed();
    let before: Vec<&str> = source.lines().filter(|row| !row.is_empty()).collect();
    let after: Vec<&str> = fixed.lines().filter(|row| !row.is_empty()).collect();
    assert_eq!(after.len(), before.len(), "a flag changes no row count: {fixed}");
    for (row, origin) in after.iter().zip(&before) {
        assert_eq!(fields(row), fields(origin), "every field stands as the ASR wrote it: {fixed}");
    }
    assert!(
        log.iter().any(|line| line.contains("line 2") && line.contains("interviewer")),
        "the note reaches the log rather than being dropped: {log:?}"
    );
}

/// Out of tries is a block outcome, not a crash: `fix_line` refuses a number outside the block twice,
/// so the block keeps the ASR's own lines and says so. Every other block would still have been fixed —
/// which is why a block, and not a source, is the unit of failure here.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_a_block_that_runs_out_of_tries_keeps_the_asr_lines(
) {
    // Line 99 is not in a four-line block, so `Block::fix_line` refuses it -- twice, which is all
    // `P.machine.fixTries` allows.
    let fake = Fake::start(fix_answer(99, "an answer about a line that does not exist"));
    let scene = Scene::new("gaveup", &fake.url);
    let source = scene.source();
    let mut log = Vec::new();
    let blocks = scene
        .fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string()))
        .expect("a failed block does not fail the stage");
    fake.waited(1);
    assert_eq!(blocks, 1, "one block walked");
    assert!(
        log.contains(&naivepost::fix_transcripts::give_up_log(LANE, 0, 1)),
        "the give-up line is what §F1.8 S4 says: {log:?}"
    );
    assert_eq!(scene.fixed(), source, "the originals stand, byte for byte");
}

/// §3.1's shared reads, answered on the fix leg. The job is OFFERED `get_lines` (and the other three), so
/// a model may ask for its own rows back before respelling them — and before [`naivepost::tool_reads`] it
/// got silence, which reads to a model as a tool that does not exist. This drives the whole door: round one
/// asks for lines 1-2, the app answers from the block in hand, round two gets the answer beside the same
/// question and then does the actual fix.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_a_shared_read_reaches_the_model_and_the_answer_comes_back(
) {
    let read_first = call_answer("get_lines", serde_json::json!({ "from": 1, "to": 2 }));
    let fake = scripted(vec![read_first, fix_answer(1, "so the algorithm receives the input")]);
    let scene = Scene::new("readback", &fake.url);
    let mut log = Vec::new();
    scene
        .fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string()))
        .expect("a read round does not fail the block");
    let asks = fake.waited(2);
    assert_eq!(asks.len(), 2, "the read costs a round: {asks:?}");

    // Not a coincidence: the read was offered on the FIRST ask, so the model had a reason to call it.
    let offered: Vec<String> = serde_json::from_str::<serde_json::Value>(&asks[0].body)
        .expect("a JSON body")["tools"]
        .as_array()
        .expect("tools are offered")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_string))
        .collect();
    assert!(offered.iter().any(|name| name == "get_lines"), "get_lines is offered to the fix job: {offered:?}");

    // The second ask carries the answer to the read: both requested rows, stamped, with their speaker.
    // Read back out of the JSON rather than matched as raw text, so the escaping is not what is tested.
    let answers = tool_answers(&asks[1]);
    let body: serde_json::Value = serde_json::from_str(&answers).expect("the read answered in JSON");
    let rows = body["lines"].as_array().expect("the lines the model asked for");
    assert_eq!(rows.len(), 2, "both rows came back: {answers}");
    assert_eq!(rows[0], "00:00-00:01\tS1\tso the algorithum recieves the input");
    assert_eq!(rows[1], "00:01-00:03\tS1\tand writes it down again");

    // And the fix the model made AFTER reading still lands: the read cost a round, not the answer.
    let fixed = scene.fixed();
    assert!(
        fixed.contains("so the algorithm receives the input"),
        "the later fix_line is what the file holds: {fixed}"
    );
}

/// A read with bad arguments comes back as `{"error": …}` naming what to send (§3: a problem is text the
/// model can act on, never silence and never a dropped flow).
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s7_a_read_with_bad_arguments_is_answered_with_an_error_not_silence(
) {
    let read_first = call_answer("get_lines", serde_json::json!({}));
    let fake = scripted(vec![read_first, fix_answer(2, "and writes it down again")]);
    let scene = Scene::new("badread", &fake.url);
    let mut log = Vec::new();
    scene
        .fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string()))
        .expect("a bad argument is not a failure");
    let asks = fake.waited(2);
    let answers = tool_answers(&asks[1]);
    assert!(answers.contains("\"error\""), "the refusal shape reaches the model: {answers}");
    assert!(
        answers.contains("from") && answers.contains("to"),
        "it names the arguments to send instead: {answers}"
    );
    // The flow continued past the error: the next round's fix is in the file.
    assert!(scene.fixed().contains("and writes it down again"), "the block was still fixed");
}

/// §6 on this job's live path: a block's answer is filed under `cache/llm/fix/<sha256 hex>` — §6's
/// digest, model id included — and the next pass over the same rows reads it back: no ask, no new row in
/// `requests.tsv`, same fixed transcript on disk. Two slots stand there after one pass because this job
/// files at two heights (`fix_call::ask_once` keeps the reply the round ended with; `store_block` keeps
/// the respelled rows that resume marker needs), so what is pinned is each slot's NAME and HEX SHAPE, not
/// a count. "Only its first attempt is cached" is already pinned in `tests/fix_transcripts.rs`.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_the_fixer_files_its_answer_and_replays_it_without_a_socket() {
    let respelled = "so the algorithm receives the input";
    let fake = Fake::start(fix_answer(1, respelled));
    let scene = Scene::new("s7-cache", &fake.url);
    let conf = conf_at(&fake.url);
    let mut log = Vec::new();
    scene
        .fix(&conf, &mut |line| log.push(line.to_string()))
        .expect("the fix ran");
    assert_eq!(fake.waited(1).len(), 1, "the first pass made one ask");

    let tree = scene.tree();
    let dir = tree.dir().join("cache/llm").join(naivepost::fix_transcripts::FIX_STEP);
    let slots: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|why| panic!("{} exists after an answered block: {why}", dir.display()))
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    assert!(!slots.is_empty(), "an answered block leaves a slot under cache/llm/fix/");
    for slot in &slots {
        assert_eq!(slot.len(), 64, "every slot is named by a sha256 hex digest: {slot}");
        assert!(
            slot.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "lowercase hex, §6's shape: {slot}"
        );
        assert!(tree.cache_llm(naivepost::fix_transcripts::FIX_STEP, slot).exists());
    }
    // The key `fix_transcripts` resumes from is one of them: §6's digest over the block's rows.
    let lines = naivepost::fix_transcripts::load_transcript(&tree, LANE).expect("the source rows");
    let blocks = naivepost::fix_transcripts::blocks(&lines);
    assert_eq!(blocks.len(), 1, "four rows fit one block");
    let block_key = naivepost::fix_transcripts::block_key(&blocks[0]);
    assert_eq!(block_key.len(), 64, "§6's digest names the resume marker: {block_key}");
    assert!(slots.contains(&block_key), "the resume slot is filed: {slots:?}");

    let rows = naivepost::requests::read(&tree).expect("requests.tsv");
    assert!(!rows.is_empty(), "the request that earned the answer is recorded");
    let fixed = scene.fixed();
    assert!(fixed.contains(respelled), "{fixed}");

    // The second pass: same door, same rows, same model — so the cache answers before the gate and the
    // exchange log (§6 + §10), and the file it produces is the same file.
    let mut again = Vec::new();
    scene.fix(&conf, &mut |line| again.push(line.to_string())).expect("the second fix ran");
    assert_eq!(
        fake.asks.lock().expect("the record lock").bodies.len(),
        1,
        "the replay made no ask: {again:?}"
    );
    // The stage's own count of blocks answered from the cache, not a test's guess about it.
    assert_eq!(
        again.last().map(String::as_str),
        Some(naivepost::fix_transcripts::cached_blocks_log(LANE, 1).as_str()),
        "the second pass says it answered its block from the cache: {again:?}"
    );
    assert_eq!(
        naivepost::requests::read(&tree).expect("requests.tsv").len(),
        rows.len(),
        "a cache hit writes no row"
    );
    assert_eq!(scene.fixed(), fixed, "and the cached pass writes the same fixed transcript");
}
