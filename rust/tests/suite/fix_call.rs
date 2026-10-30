//! §02-services.md **§2 Model roles**, the "clean transcript" row: a source's ASR lines go to the LLM,
//! **thinking off**, and the answer is the same lines respelled: times and speakers unchanged, and
//! *enforced*.
//!
//! Item id: `sec_02_services_2_model_roles_which_job_asks_which_server`. These tests drive the door the
//! Prepare walk reaches (`fix_call::run_task` → `stage_chunk`) against a loopback LLM pointed at through the
//! Settings box. One block is ONE chat call with no tools, as the prototype asked it (`fixRows`): the User
//! Context (a script included) over the grounding and the block's rows, and the answer is the same rows with
//! only the text changed. The enforcement is the prototype's strict check: an answer whose count, times or
//! speakers moved is refused, the model is told why on its one retry, and a block refused twice keeps the
//! ASR's own lines. The first rewrite offered tools instead and sent no User Context; the model asked for an
//! empty context and answered nothing, and the subtitles kept "my name is thomas bocek".

use std::io::{BufRead, Read, Write};
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

/// A chat envelope whose content is `text`, as a model answers a call with no tools.
fn content_answer(text: &str) -> String {
    serde_json::json!({ "choices": [ { "message": { "content": text } } ] }).to_string()
}

/// The block's four rows answered back, with `changed` texts replacing theirs (1-based line numbers): what a
/// model that follows the job sends.
fn rows_answer(changed: &[(usize, &str)]) -> String {
    let rows: Vec<String> = SOURCE_ROWS
        .iter()
        .enumerate()
        .map(|(index, (start, end, speaker, text))| {
            let text = changed
                .iter()
                .find(|(n, _)| *n == index + 1)
                .map_or(*text, |(_, new)| *new);
            format!("{start}\t{end}\t{speaker}\t{text}")
        })
        .collect();
    content_answer(&rows.join("\n"))
}

/// The rows answered back with line 2's start moved and its speaker changed: what the strict check refuses.
fn moved_answer() -> String {
    let rows: Vec<String> = SOURCE_ROWS
        .iter()
        .enumerate()
        .map(|(index, (start, end, speaker, text))| {
            if index == 1 {
                format!("1.75\t{end}\tINTERVIEWER\t{text}")
            } else {
                format!("{start}\t{end}\t{speaker}\t{text}")
            }
        })
        .collect();
    content_answer(&rows.join("\n"))
}

/// What the fake answers with, and how many chat asks it has taken.
struct Fake {
    url: String,
    asks: std::sync::Arc<Mutex<Asks>>,
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
        Fake {
            url: format!("http://127.0.0.1:{port}"),
            asks,
        }
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
    Some(ChatAsk {
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
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
        naivepost::textfmt::write_lines(&lines, &tree.transcript_tsv(LANE))
            .expect("the transcript");
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
    Fake {
        url: format!("http://127.0.0.1:{port}"),
        asks,
    }
}

/// The text of the last user message of one recorded ask: what the model was shown.
fn user_text(ask: &ChatAsk) -> String {
    let sent: serde_json::Value = serde_json::from_str(&ask.body).expect("a JSON body");
    sent["messages"]
        .as_array()
        .expect("the messages")
        .iter()
        .rev()
        .find(|message| message["role"].as_str() == Some("user"))
        .expect("a user message")["content"]
        .as_array()
        .expect("the user parts")
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Fields of one §6 row: `start`, `end`, speaker, then the text (which may hold anything else).
fn fields(row: &str) -> Vec<&str> {
    row.split('\t').take(4).collect()
}

/// §2's row on the wire: the block goes out as its rows with their times and speaker (no line numbers, as the
/// job says), the model is asked for exactly that count, thinking is OFF in both places the switch is written,
/// and no tools are offered: the answer is the rows themselves.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_asks_the_llm_with_the_block_and_thinking_off(
) {
    let fake = Fake::start(rows_answer(&[(1, "So the algorithm receives the input")]));
    let scene = Scene::new("wire", &fake.url);
    let mut log = Vec::new();
    let blocks = scene
        .fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string()))
        .expect("the stage fixes the row");
    let asks = fake.waited(1);
    assert_eq!(
        blocks, 1,
        "four lines fit one block of P.machine.fixBlockLines"
    );
    assert_eq!(asks.len(), 1, "one ask for one block: {asks:?}");
    assert!(
        asks[0].path.contains("/v1/chat/completions"),
        "the fix job is a chat call: {}",
        asks[0].path
    );
    let sent: serde_json::Value = serde_json::from_str(&asks[0].body).expect("a JSON body");
    let user = user_text(&asks[0]);
    assert!(
        user.contains("Transcript lines to clean (4 lines, return exactly 4)"),
        "the ask names the count it wants back: {user}"
    );
    for (start, end, speaker, text) in SOURCE_ROWS.iter() {
        let row = format!("\n{start}\t{end}\t{speaker}\t{text}");
        assert!(user.contains(&row), "the row went out whole: {user}");
    }
    assert_eq!(
        sent["enable_thinking"],
        serde_json::json!(false),
        "thinking OFF: {sent}"
    );
    assert_eq!(
        sent["chat_template_kwargs"]["enable_thinking"],
        serde_json::json!(false),
        "thinking OFF in the template copy too: {sent}"
    );
    assert!(
        sent.get("tools").is_none(),
        "no tools are offered: the answer is the rows (the prototype asked with none): {sent}"
    );
}

/// The session note, a script included, reaches the fixer above the block: the spellings of names come from
/// it ("Use the script's spelling for names"), as the prototype's `ctxBlockFor("fix")` sent it. The first
/// rewrite sent no User Context, and a name the ASR wrote in lower case stayed that way in the subtitles.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_the_session_note_and_its_script_reach_the_fixer(
) {
    let fake = Fake::start(rows_answer(&[]));
    let scene = Scene::new("context", &fake.url);
    let project = naivepost::project::Project {
        context: "My name is Thomas Bocek. Script: the algorithm receives the input.".to_string(),
        ..Default::default()
    };
    naivepost::project::save(&project, &scene.session()).expect("the project file");
    scene
        .fix(&conf_at(&fake.url), &mut |_| {})
        .expect("the fix ran");
    let user = user_text(&fake.waited(1)[0]);
    assert!(
        user.starts_with(naivepost::narrate_pass::CTX_INTRO),
        "the User Context leads the message: {user}"
    );
    assert!(user.contains("My name is Thomas Bocek."), "{user}");
    let note = user.find("Thomas Bocek").unwrap();
    let rows = user.find("Transcript lines to clean").unwrap();
    assert!(note < rows, "above the block it is about: {user}");
}

/// The script between its SCRIPT START and SCRIPT STOP lines does not reach the fixer: with it in the request the
/// model takes what was said for a mishearing of the script and rewrites the line to match (08-dex 18-40-10: said
/// USDC, the script says DAI; 3 of 8 asks kept USDC with the script, 8 of 8 without). The rest of the note still
/// does, and since the script is no longer part of the question, changing it asks nothing again.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_the_fixer_is_not_shown_the_script() {
    let fake = Fake::start(rows_answer(&[]));
    let scene = Scene::new("no-script", &fake.url);
    let with_script = |script: &str| naivepost::project::Project {
        context: format!("A lecture by Thomas Bocek.\n\n****SCRIPT START****\n{script}\n****SCRIPT STOP****\n"),
        ..Default::default()
    };
    naivepost::project::save(&with_script("DAI kostet 1 Dollar."), &scene.session()).expect("the project file");
    scene.fix(&conf_at(&fake.url), &mut |_| {}).expect("the fix ran");
    let user = user_text(&fake.waited(1)[0]);
    assert!(user.contains("A lecture by Thomas Bocek."), "the note still leads: {user}");
    assert!(!user.contains("DAI") && !user.contains("SCRIPT"), "the script stays out: {user}");

    naivepost::project::save(&with_script("USDC kostet 1 Dollar."), &scene.session()).expect("the project file");
    scene.fix(&conf_at(&fake.url), &mut |_| {}).expect("the second fix ran");
    assert_eq!(
        fake.asks.lock().expect("the record lock").bodies.len(),
        1,
        "a changed script is not a changed question"
    );
}

/// The answer lands: line 1 keeps its own times and speaker and carries the respelling, and the rows
/// nobody fixed stand untouched. Same row count, same first three fields.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_rewrites_only_the_text_column(
) {
    let respelled = "so the algorithm receives the input";
    let fake = Fake::start(rows_answer(&[(1, respelled)]));
    let scene = Scene::new("files", &fake.url);
    let source = scene.source();
    let mut log = Vec::new();
    scene
        .fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string()))
        .expect("the fix ran");
    fake.waited(1);
    let fixed = scene.fixed();
    let before: Vec<&str> = source.lines().filter(|row| !row.is_empty()).collect();
    let after: Vec<&str> = fixed.lines().filter(|row| !row.is_empty()).collect();
    assert_eq!(
        after.len(),
        before.len(),
        "a fix never adds or drops a row: {fixed}"
    );
    let first = fields(after[0]);
    assert_eq!(
        first[3], respelled,
        "the model's spelling is what the row holds: {fixed}"
    );
    // Times and speaker byte-identical to the source row: the answer's own are held against them.
    let origin = fields(before[0]);
    assert_eq!(
        first[..3],
        origin[..3],
        "start, end and speaker are untouched: {fixed}"
    );
    for (index, (row, origin)) in after.iter().zip(&before).enumerate().skip(1) {
        assert_eq!(
            fields(row),
            fields(origin),
            "line {} came back unchanged: {row}",
            index + 1
        );
    }
}

/// A reply cannot move a time or a speaker: an answer whose line 2 starts later and names another speaker is
/// refused, the model is told what was wrong on its one retry, and a block refused twice keeps every field as
/// the ASR wrote it (the prototype's strict check: same count, byte-identical start, end and speaker).
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_a_reply_cannot_move_a_time_or_a_speaker(
) {
    let fake = Fake::start(moved_answer());
    let scene = Scene::new("moved", &fake.url);
    let source = scene.source();
    let mut log = Vec::new();
    scene
        .fix(&conf_at(&fake.url), &mut |line| log.push(line.to_string()))
        .expect("the fix ran");
    let asks = fake.waited(2);
    assert_eq!(asks.len(), 2, "the refusal costs its one retry: {asks:?}");
    assert!(
        user_text(&asks[1])
            .contains("line 2 must keep its start, end and speaker exactly as given"),
        "the model hears what it got wrong: {}",
        user_text(&asks[1])
    );
    assert_eq!(
        scene.fixed(),
        source,
        "every field stands as the ASR wrote it"
    );
    assert!(
        log.contains(&naivepost::fix_transcripts::give_up_log(LANE, 0, 1)),
        "and the block says it kept its lines: {log:?}"
    );
}

/// Out of tries is a block outcome, not a crash: an answer short of a row is refused twice,
/// so the block keeps the ASR's own lines and says so. Every other block would still have been fixed —
/// which is why a block, and not a source, is the unit of failure here.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_clean_transcript_a_block_that_runs_out_of_tries_keeps_the_asr_lines(
) {
    // Three rows back for a four-line block: refused, twice, which is all `P.machine.fixTries` allows.
    let three = rows_answer(&[]);
    let short: serde_json::Value = serde_json::from_str(&three).expect("json");
    let text = short["choices"][0]["message"]["content"].as_str().unwrap();
    let fake = Fake::start(content_answer(
        &text.lines().take(3).collect::<Vec<&str>>().join("\n"),
    ));
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

/// An answer with no rows in it (a model that talked instead of answering) is refused with the reason, asked
/// once more, and the block keeps its lines when the second answer has none either. The first rewrite's
/// tool rounds read "answered nothing at all" as nothing to fix and moved on.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s6_an_answer_with_no_rows_is_refused_and_asked_again(
) {
    let fake = scripted(vec![
        content_answer("Sure, I can clean these lines for you."),
        rows_answer(&[(2, "And writes it down again.")]),
    ]);
    let scene = Scene::new("norows", &fake.url);
    scene
        .fix(&conf_at(&fake.url), &mut |_| {})
        .expect("the fix ran");
    let asks = fake.waited(2);
    assert_eq!(asks.len(), 2, "a talking answer costs the retry");
    assert!(
        user_text(&asks[1]).contains("the answer held no lines"),
        "and the model hears why: {}",
        user_text(&asks[1])
    );
    assert!(
        scene.fixed().contains("And writes it down again."),
        "the retry's rows land: {}",
        scene.fixed()
    );
}

/// §6 on this job's live path: a block's answer is filed under `cache/llm/fix/<sha256 hex>` — §6's
/// digest over the block's rows — and the next pass over the same rows reads it back: no ask, no new row in
/// `requests.tsv`, same fixed transcript on disk. "Only its first attempt is cached" is pinned below.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_the_fixer_files_its_answer_and_replays_it_without_a_socket() {
    let respelled = "so the algorithm receives the input";
    let fake = Fake::start(rows_answer(&[(1, respelled)]));
    let scene = Scene::new("s7-cache", &fake.url);
    let conf = conf_at(&fake.url);
    let mut log = Vec::new();
    scene
        .fix(&conf, &mut |line| log.push(line.to_string()))
        .expect("the fix ran");
    assert_eq!(fake.waited(1).len(), 1, "the first pass made one ask");

    let tree = scene.tree();
    let dir = tree
        .dir()
        .join("cache/llm")
        .join(naivepost::fix_transcripts::FIX_STEP);
    let slots: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|why| panic!("{} exists after an answered block: {why}", dir.display()))
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    assert!(
        !slots.is_empty(),
        "an answered block leaves a slot under cache/llm/fix/"
    );
    for slot in &slots {
        assert_eq!(
            slot.len(),
            64,
            "every slot is named by a sha256 hex digest: {slot}"
        );
        assert!(
            slot.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "lowercase hex, §6's shape: {slot}"
        );
        assert!(tree
            .cache_llm(naivepost::fix_transcripts::FIX_STEP, slot)
            .exists());
    }
    // The key `fix_transcripts` resumes from is one of them: §6's digest over the block's rows.
    let lines = naivepost::fix_transcripts::load_transcript(&tree, LANE).expect("the source rows");
    let blocks = naivepost::fix_transcripts::blocks(&lines);
    assert_eq!(blocks.len(), 1, "four rows fit one block");
    let context = naivepost::project::load(scene.tree().dir())
        .map(|project| project.context)
        .unwrap_or_default();
    let prompt = naivepost::fix_call::cache_prompt(&scene.paths, &context);
    let block_key = naivepost::fix_transcripts::block_key(&prompt, &blocks[0]);
    assert_eq!(
        block_key.len(),
        64,
        "§6's digest names the resume marker: {block_key}"
    );
    assert!(
        slots.contains(&block_key),
        "the resume slot is filed: {slots:?}"
    );

    let rows = naivepost::requests::read(&tree).expect("requests.tsv");
    assert!(
        !rows.is_empty(),
        "the request that earned the answer is recorded"
    );
    let fixed = scene.fixed();
    assert!(fixed.contains(respelled), "{fixed}");

    // The second pass: same door, same rows, same model — so the cache answers before the gate and the
    // exchange log (§6 + §10), and the file it produces is the same file.
    let mut again = Vec::new();
    scene
        .fix(&conf, &mut |line| again.push(line.to_string()))
        .expect("the second fix ran");
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
    let cached_rows = naivepost::requests::read(&tree)
        .expect("requests.tsv")
        .iter()
        .filter(|row| row.outcome == naivepost::requests::Outcome::Cache)
        .count();
    assert_eq!(
        cached_rows, 1,
        "§10: the cached block wrote its own marked row, and no wire row"
    );
    assert_eq!(
        scene.fixed(),
        fixed,
        "and the cached pass writes the same fixed transcript"
    );
}

/// The names of the files filed under `cache/llm/fix/` — §6's slots for this job, at either height
/// (`store_block`'s resume rows).
fn slots_under(tree: &naivepost::layout::Tree) -> Vec<String> {
    let dir = tree
        .dir()
        .join("cache/llm")
        .join(naivepost::fix_transcripts::FIX_STEP);
    std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// §6's second irregularity on this job's live path: only the FIRST attempt's answer earns a cache
/// slot. A block repaired after a refusal keeps its fix — `transcript.fixed.tsv` says the same thing
/// either way — but the next run re-asks it and re-validates it, because that reply was only ever right
/// given the refusal it was shown. The first-attempt half is pinned beside it so the rule reads as a
/// pair rather than as "the fixer never caches".
#[test]
fn sec_09_llm_and_tools_6_cache_s7_the_fixer_files_its_answer_by_input_whichever_try_earned_it() {
    let respelled = "so the algorithm receives the input";
    // Round one moves a time, so the strict check refuses it and a second attempt actually happens; round
    // two answers properly.
    let fake = scripted(vec![moved_answer(), rows_answer(&[(1, respelled)])]);
    let scene = Scene::new("s7-first-attempt", &fake.url);
    let conf = conf_at(&fake.url);
    let mut log = Vec::new();
    scene
        .fix(&conf, &mut |line| log.push(line.to_string()))
        .expect("the repair still fixes the block");
    assert_eq!(fake.waited(2).len(), 2, "a refusal costs a second ask");

    let fixed = scene.fixed();
    assert!(
        fixed.contains(respelled),
        "the repaired answer writes the file: {fixed}"
    );
    // The repaired block is filed under its own rows, and the same rows cost nothing next time. The first
    // rewrite filed only a first-try answer, so a block answered on its retry was asked twice on every run.
    let slots = slots_under(&scene.tree());
    assert_eq!(slots.len(), 1, "the repaired block is filed -- {slots:?}");
    let mut replay_log = Vec::new();
    scene
        .fix(&conf, &mut |line| replay_log.push(line.to_string()))
        .expect("the replay ran");
    assert_eq!(
        fake.asks.lock().expect("the record lock").bodies.len(),
        2,
        "the replay of the unchanged block cost no ask: {replay_log:?}"
    );
    assert!(
        scene.fixed().contains(respelled),
        "and wrote the same fix: {}",
        scene.fixed()
    );

    // The other half: a first-try answer IS filed, under §6's digest, and the next pass replays it.
    let other = Fake::start(rows_answer(&[(1, respelled)]));
    let first_scene = Scene::new("s7-first-try", &other.url);
    let first_conf = conf_at(&other.url);
    let mut again_log = Vec::new();
    first_scene
        .fix(&first_conf, &mut |line| again_log.push(line.to_string()))
        .expect("the first try fixes the block");
    assert_eq!(other.waited(1).len(), 1, "one ask, answered straight off");
    let tree = first_scene.tree();
    let slots: Vec<String> = std::fs::read_dir(
        tree.dir()
            .join("cache/llm")
            .join(naivepost::fix_transcripts::FIX_STEP),
    )
    .expect("a first-try answer leaves a slot")
    .flatten()
    .filter_map(|entry| entry.file_name().into_string().ok())
    .collect();
    assert!(!slots.is_empty(), "the first attempt is filed");
    for slot in &slots {
        assert_eq!(slot.len(), 64, "named by a sha256 hex digest: {slot}");
        assert!(
            slot.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "lowercase hex, §6's shape: {slot}"
        );
    }
    let mut replay = Vec::new();
    first_scene
        .fix(&first_conf, &mut |line| replay.push(line.to_string()))
        .expect("the replay ran");
    assert_eq!(
        other.asks.lock().expect("the record lock").bodies.len(),
        1,
        "the replay cost no ask: {replay:?}"
    );
    assert_eq!(
        replay.last().map(String::as_str),
        Some(naivepost::fix_transcripts::cached_blocks_log(LANE, 1).as_str()),
        "and the stage says it answered from the cache: {replay:?}"
    );
}

/// A block no answer reached keeps its lines for this run and files nothing: stored, its raw text was replayed as the
/// block's answer on every later run. A ⏹ during the fix pass is the way this happens in practice (08-dex).
#[test]
fn sec_09_llm_and_tools_6_cache_a_block_the_server_never_answered_is_not_filed() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port();
    let url = format!("http://127.0.0.1:{port}");
    naivepost::llm_retry_leg::forget_answer_for_test(&url);
    let scene = Scene::new("no-answer", &url);
    let conf = conf_at(&url);
    scene.fix(&conf, &mut |_| {}).expect("the walk carries on");
    assert!(!scene.fixed().is_empty(), "the block keeps its lines for this run");
    let dir = scene
        .tree()
        .dir()
        .join("cache/llm")
        .join(naivepost::fix_transcripts::FIX_STEP);
    let slots = std::fs::read_dir(&dir).map(|found| found.count()).unwrap_or(0);
    assert_eq!(slots, 0, "nothing is filed for a block nobody answered");
}
