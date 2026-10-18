//! §09-llm-and-tools#6-cache — the five jobs §6 names, driven over a socket.
//!
//! Spec: `spec/09-llm-and-tools.md` §6. `tests/llm_cache.rs` proves the digest and the store rules;
//! what only a wire can prove is that each job's OWN ask leg files its answer under
//! `cache/llm/<step>/<key>` and then answers an identical ask without opening one — §6's "a cache hit
//! is answered before the gate and the exchange log, so a resumed run's page holds only uncached
//! calls". Each test below asks twice through the real door with the settings address pointed at a
//! loopback fake (the seam a typed address goes through), and asserts three things about the second
//! ask: no request reached the fake, no new row landed in `requests.tsv`, and the answer came back the
//! same. The negative half (§6's "not by the cut, narration or upload text") needs no server at all.
//!
//! One shared `ENV` guard: `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working folder belong to the
//! process and cargo runs these as threads, so exactly one test may hold them at a time.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use naivepost::layout::Tree;
use naivepost::llm_cache;
use naivepost::requests;
use naivepost::roles::Job;
use naivepost::settings::{self, Conf, Paths};
use serde_json::json;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::temp_dir_tagged;

/// The env and the working folder are process-wide, so the tests that move them take them one at a time.
static ENV: Mutex<()> = Mutex::new(());

/// A fake LLM on a loopback port answering every request with one scripted body, recording each body
/// it was asked with. Held answers live behind a mutex so a test could change them mid-run.
struct Fake {
    url: String,
    asks: Arc<Mutex<Vec<String>>>,
    #[allow(dead_code)] // held so the reply outlives the connection threads that read it
    answer: Arc<Mutex<String>>,
}

impl Fake {
    fn start(answer: String) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("an address").port();
        let asks = Arc::new(Mutex::new(Vec::new()));
        let recorder = asks.clone();
        let held = Arc::new(Mutex::new(answer));
        let replies = held.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let recorder = recorder.clone();
                let replies = replies.clone();
                std::thread::spawn(move || {
                    let answer = replies.lock().expect("the reply lock").clone();
                    if let Some(body) = serve(stream, &answer) {
                        if let Ok(mut row) = recorder.lock() {
                            row.push(body);
                        }
                    }
                });
            }
        });
        Fake { url: format!("http://127.0.0.1:{port}"), asks, answer: held }
    }

    /// Wait until `count` asks have been recorded — recording happens on the connection's own thread.
    fn waited(&self, count: usize) -> Vec<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let seen = self.asks.lock().expect("the record lock").clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// How many asks arrived, without waiting.
    fn count(&self) -> usize {
        self.asks.lock().expect("the record lock").len()
    }

    /// Change what the fake answers with, mid-run: a scenario whose second call must differ from its
    /// first (the walk's gap-filling re-ask, §6) needs this and nothing else.
    fn set_answer(&self, answer: String) {
        *self.answer.lock().expect("the reply lock") = answer;
    }
}

fn serve(mut stream: TcpStream, answer: &str) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
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
    Some(String::from_utf8_lossy(&body).into_owned())
}

/// A session, its settings folder pointing at the fake, and the process pointed at both.
struct Scene {
    root: PathBuf,
    paths: Paths,
    conf: Conf,
    tree: Tree,
}

impl Scene {
    fn new(job: &str, fake_url: &str) -> Scene {
        let root = temp_dir_tagged(&format!("cache-live-{job}"));
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(&session).expect("the session folder");
        let paths =
            Paths { config_dir: root.join("cfg/naivepost"), data_dir: root.join("data/naivepost") };
        let mut conf = Conf::default();
        conf.server = fake_url.to_string();
        conf.model = format!("{job}-model");
        settings::save(&paths, &conf).expect("the conf is written");
        let tree = Tree::new(session).expect("the session is a project");
        Scene { root, paths, conf, tree }
    }

    /// Point the process at this scene exactly as the app resolves both, and take §3's patience ladder
    /// down by 1000 so a refused ask cannot park a test for minutes.
    fn enter(&self) {
        std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000");
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
    }

    fn leave(&self) {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::remove_var("NAIVEPOST_RETRY_SCALE");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    /// The names of the files this step has filed under `cache/llm/<step>/`.
    fn cached_names(&self, step: &str) -> Vec<String> {
        let dir = self.tree.dir().join("cache/llm").join(step);
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect()
    }
}

/// §10's two counts on a folder's log: requests that went out, and hits served from the cache.
fn wire_rows(tree: &naivepost::layout::Tree) -> usize {
    requests::read(tree)
        .expect("requests.tsv")
        .iter()
        .filter(|row| row.outcome != naivepost::requests::Outcome::Cache)
        .count()
}

fn cache_rows(tree: &naivepost::layout::Tree) -> usize {
    requests::read(tree)
        .expect("requests.tsv")
        .iter()
        .filter(|row| row.outcome == naivepost::requests::Outcome::Cache)
        .count()
}

/// Exactly one file under this step's folder, and its name is §6's sha256 hex.
fn assert_one_hex_slot(names: &[String], step: &str) -> String {
    assert_eq!(names.len(), 1, "{step}: one answer filed, got {names:?}");
    let name = names[0].clone();
    assert_eq!(name.len(), 64, "{step}'s file is named by a sha256 hex digest: {name}");
    assert!(
        name.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "lowercase hex, §6's shape: {name}"
    );
    name
}

// --- translate -----------------------------------------------------------------------------------------

/// §6's first irregularity is translate's: it stores the reconstructed numbered text and never caches
/// its gap-filling call. What this pins is the plain half on the live leg — a COMPLETE batch is filed
/// under `cache/llm/translate/<hex>` and the next identical batch costs no socket and no row.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_translate_files_its_answer_and_replays_it_without_a_socket() {
    let _turn = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Every number asked for comes back: a partial answer would be refused a cache slot (§6), which is
    // a different rule and already pinned elsewhere.
    let answer = json!({"choices": [{"message": {"content": null, "tool_calls": [
        {"id": "c1", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 1, "text": "eins"}).to_string()}},
        {"id": "c2", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 2, "text": "zwei"}).to_string()}},
        {"id": "c3", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 3, "text": "drei"}).to_string()}},
        {"id": "cf", "type": "function", "function": {"name": "finish", "arguments": "{}"}}
    ]}}]})
    .to_string();
    let fake = Fake::start(answer);
    let scene = Scene::new("translate", &fake.url);
    scene.enter();
    let numbers = [1usize, 2, 3];
    let message = format!(
        "{}\n{}",
        naivepost::produce_subtitles::message(3, "de"),
        naivepost::produce_subtitles::numbered(&[
            (1, "one".to_string()),
            (2, "two".to_string()),
            (3, "three".to_string()),
        ])
    );

    let first = naivepost::produce_translate_ask::ask_at(
        &scene.tree,
        &scene.conf,
        &message,
        &numbers,
        &|| false,
    )
    .expect("the batch translated");
    let want: BTreeMap<usize, String> = [(1, "eins"), (2, "zwei"), (3, "drei")]
        .into_iter()
        .map(|(n, text)| (n, text.to_string()))
        .collect();
    assert_eq!(first, want, "the answer came off the wire");
    assert_eq!(fake.waited(1).len(), 1, "one request went out");
    let rows_after_first = requests::read(&scene.tree).expect("requests.tsv").len();
    assert!(rows_after_first >= 1, "the request that earned the answer is timed and recorded");
    let slot = assert_one_hex_slot(&scene.cached_names(naivepost::produce_translate_ask::STEP), "translate");

    // The identical batch again: §6 answers it from disk, BEFORE the gate and the exchange log, so the
    // fake sees nothing. §10 still files the hit: one `cache` row, no new WIRE row.
    let second = naivepost::produce_translate_ask::ask_at(
        &scene.tree,
        &scene.conf,
        &message,
        &numbers,
        &|| false,
    )
    .expect("the cached batch still translates");
    assert_eq!(second, want, "the replay says the same thing");
    assert_eq!(fake.count(), 1, "the second ask opened no socket: {}", fake.count());
    assert_eq!(wire_rows(&scene.tree), rows_after_first, "a cache hit adds no wire row");
    assert_eq!(cache_rows(&scene.tree), 1, "a cache hit writes its own marked row");
    assert_eq!(assert_one_hex_slot(&scene.cached_names(naivepost::produce_translate_ask::STEP), "translate"), slot);
    scene.leave();
}

// --- retake --------------------------------------------------------------------------------------------

/// The pooled draws share a question and must not share an answer: the run index rides in the key
/// (§6's list names it), so draw 0's reply is filed in its own slot and only THAT draw replays.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_retake_files_each_draw_and_replays_it_without_a_socket() {
    let _turn = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // `finish` with no marks is a whole answer ("nothing abandoned"), and it is the body this run's
    // cache entry stands for — see `final_body` in `retakes_ask::ask_run`.
    let answer = json!({"choices": [{"message": {"content": "", "tool_calls": [
        {"id": "c1", "type": "function", "function": {"name": "finish", "arguments": "{}"}}
    ]}}]})
    .to_string();
    let fake = Fake::start(answer);
    let scene = Scene::new("retakes", &fake.url);
    scene.enter();
    use naivepost::tools::retakes::{Line, Marks};
    let lines: Vec<Line> = (1..=6u32)
        .map(|n| Line { n, start: f64::from(n) - 1.0, end: f64::from(n) - 0.5 })
        .collect();
    let speech = naivepost::retakes::speech_seconds(&lines);
    let brief = naivepost::retakes::brief(&lines, &[]);
    let mut marks = Marks::new(lines.clone(), speech);

    let first = naivepost::retakes_ask::ask_run(&scene.tree, &scene.paths, &scene.conf, &brief, 0, &mut marks);
    assert!(matches!(first, naivepost::retakes_ask::RunOutcome::Answered { .. }), "the draw answered: {first:?}");
    assert_eq!(fake.waited(1).len(), 1, "one request went out");
    let rows_after_first = requests::read(&scene.tree).expect("requests.tsv").len();
    let slot = assert_one_hex_slot(&scene.cached_names(naivepost::retakes_ask::STEP), "retakes");
    // The name on disk is the one this round's naming door spells, so the digest has one spelling. The
    // system half of that digest is the ASSEMBLED §8 message (house rules cut to the job + the job's own
    // wording), not the bare `retake` prompt — §6 keys a slot on what went out, so an edited house prompt
    // has to miss rather than replay an answer asked under different rules.
    let named = naivepost::retakes_ask::run_key(
        &naivepost::retakes_ask::system_message(&scene.paths, ""),
        &brief,
        "retakes-model",
        0,
    )
    .expect("a brief always keys");
    assert_eq!(named, slot, "the file is the slot the door names");

    let second = naivepost::retakes_ask::ask_run(&scene.tree, &scene.paths, &scene.conf, &brief, 0, &mut marks);
    assert!(matches!(second, naivepost::retakes_ask::RunOutcome::Cached { .. }), "the replay is a hit: {second:?}");
    assert_eq!(fake.count(), 1, "the second draw opened no socket: {}", fake.count());
    assert_eq!(wire_rows(&scene.tree), rows_after_first, "a cache hit adds no wire row");
    assert_eq!(cache_rows(&scene.tree), 1, "a cache hit writes its own marked row");
    scene.leave();
}

// --- textedit (joins) ----------------------------------------------------------------------------------

/// §6 names textedit among the caching jobs, and §F1.10 adds that a REFUSED seam answer is never
/// cached. This is the usable half on the live leg: an accepted repair is filed under
/// `cache/llm/textedit/<hex>` and the next press of the same join costs no socket.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_textedit_files_its_answer_and_replays_it_without_a_socket() {
    let _turn = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Two takes meeting at a seam, five words each. BEFORE keeps its first word and drops its last four;
    // AFTER stands whole — one contiguous stretch left out, all of it at the join, which is what
    // `answer_join` accepts (two separate gaps it refuses). Only an accepted answer earns a cache slot.
    let before: Vec<String> = ["the", "network", "is", "live", "today"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let after: Vec<String> = ["the", "network", "goes", "live", "now"]
        .into_iter()
        .map(str::to_string)
        .collect();
    // §6's tool-flow rule: "with tools, the cached unit is the finished item list of a job with
    // identical inputs". So the fake answers with CALLS — `drop_words(4 from before)` then `finish`, one
    // contiguous stretch at the join, which is what `Join` accepts (two separate gaps it refuses) — and
    // what lands in the slot is the answer those accepted calls came to, not the raw call list.
    let answer = json!({"choices": [{"message": {"content": null, "tool_calls": [
        {"id": "c1", "type": "function", "function": {"name": "drop_words",
            "arguments": json!({"side": "before", "count": 4}).to_string()}},
        {"id": "cf", "type": "function", "function": {"name": "finish", "arguments": "{}"}}
    ]}}]})
    .to_string();
    let fake = Fake::start(answer);
    let scene = Scene::new("textedit", &fake.url);
    scene.enter();

    let first = naivepost::join_ask::ask_at(&scene.tree, &scene.conf, "A lecture.", 1, 3, &before, &after, 1, &|| false);
    assert!(!first.failed, "the ask reached the fake and was read: {:?}", first.outcome);
    assert!(
        matches!(
            first.outcome,
            naivepost::joins::Outcome::Applied { .. } | naivepost::joins::Outcome::Kept
        ),
        "an accepted answer is what gets cached, not a refusal: {:?}",
        first.outcome
    );
    assert!(!first.from_cache, "the first ask cannot be a hit");
    let asked_first_time = fake.waited(1).len();
    assert!(asked_first_time >= 1, "at least one request went out: {asked_first_time}");
    let rows_after_first = requests::read(&scene.tree).expect("requests.tsv").len();
    assert_one_hex_slot(&scene.cached_names(naivepost::joins::STEP), "textedit");
    // What is filed is an ANSWER this door can read back, and for a tool flow it is the answer the
    // ACCEPTED calls came to — `drop_answer`'s `{"joined":…}` document — not the raw call list. That is
    // §6's "with tools, the cached unit is the finished item list of a job with identical inputs", and it
    // is what makes the replay below cost no socket.
    let slot_name = scene.cached_names(naivepost::joins::STEP).remove(0);
    let kept = std::fs::read_to_string(scene.tree.cache_llm(naivepost::joins::STEP, &slot_name))
        .expect("the slot is readable");
    assert!(
        naivepost::joins::parse_joined(&kept).is_some(),
        "the slot holds the join's own answer text, which is what makes the replay cost nothing: {kept}"
    );

    let second = naivepost::join_ask::ask_at(&scene.tree, &scene.conf, "A lecture.", 1, 3, &before, &after, 1, &|| false);
    assert!(second.from_cache, "the replay came from disk: {:?}", second.outcome);
    assert_eq!(
        second.outcome, first.outcome,
        "the replay decides the same join: {:?} vs {:?}",
        second.outcome, first.outcome
    );
    assert_eq!(
        fake.count(),
        asked_first_time,
        "the second ask opened no socket at all: {} vs {asked_first_time}",
        fake.count()
    );
    assert_eq!(wire_rows(&scene.tree), rows_after_first, "a cache hit adds no wire row");
    assert_eq!(cache_rows(&scene.tree), 1, "a cache hit writes its own marked row");
    scene.leave();
}

// --- who does NOT cache -------------------------------------------------------------------------------

/// §6's other sentence: the cut, the narration and the upload text never populate `cache/llm`. Pinned
/// across every job so a new one cannot quietly start caching, and pinned against `model_calls`' own
/// flag so the table on the Details page and the cache cannot disagree about who replays.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_the_three_non_users_never_cache() {
    let users = [Job::Describe, Job::CleanTranscript, Job::Joins, Job::Retakes, Job::Subtitles];
    let every = [
        Job::Asr,
        Job::Align,
        Job::Diarize,
        Job::Separate,
        Job::Describe,
        Job::CleanTranscript,
        Job::Joins,
        Job::Retakes,
        Job::ModelCut,
        Job::ClipRules,
        Job::Narrate,
        Job::UploadText,
        Job::Thumbnail,
        Job::Subtitles,
    ];
    for job in every {
        let named_by_spec = users.contains(&job);
        assert_eq!(
            llm_cache::uses_cache(job),
            named_by_spec,
            "{job:?}: §6 lists describe, transcript, textedit, retake and translate as the users and nobody else"
        );
    }
    // The Details-page flag is the same predicate, so the two spellings cannot drift apart.
    let prepare = naivepost::model_calls::prepare(4, 25, naivepost::project::MarkingPass::Retakes, 2);
    for (model, job) in [("describe", Job::Describe), ("fix", Job::CleanTranscript), ("retake", Job::Retakes)] {
        let call = prepare.of(model).unwrap_or_else(|| panic!("prepare names {model}"));
        assert_eq!(call.cached, llm_cache::uses_cache(job), "{model}'s row reads the cache's own answer");
    }
    let cut = naivepost::model_calls::cut(5, naivepost::project::CutMode::Model, true, true, true);
    let model_cut = cut.of("cutMode model").or_else(|| cut.calls.first());
    if let Some(call) = model_cut {
        assert!(!call.cached || llm_cache::uses_cache(Job::ModelCut), "the cut's row says what §6 says");
    }
    let narrate = naivepost::model_calls::narrate(4);
    assert!(
        narrate.of("narrate").is_some_and(|call| !call.cached),
        "the narration call caches nothing: {:?}",
        narrate.calls.iter().map(|call| (call.model, call.cached)).collect::<Vec<_>>()
    );
    let produce = naivepost::model_calls::produce("", false, 10, 1);
    let upload = produce.of("youtube").expect("produce names the upload text");
    assert!(!upload.cached, "the upload text is never cached");
    let translate = produce.of("translate").expect("produce names translate");
    assert!(translate.cached, "and translate is one of the five");
}

/// §6's first irregularity, the OTHER half of it: what sits in the file is the reconstructed numbered
/// text, not the chat reply. A replay then answers `merge` directly and never re-parses prose, so a
/// model that worded its answer oddly cannot break a resumed run. Pinned on the live leg: bytes read
/// back off disk, then the same batch asked again with no socket and no row.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_translate_files_the_numbered_text_not_the_reply() {
    let _turn = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let answer = json!({"choices": [{"message": {"content": null, "tool_calls": [
        {"id": "c1", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 1, "text": "eins"}).to_string()}},
        {"id": "c2", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 2, "text": "zwei"}).to_string()}},
        {"id": "c3", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 3, "text": "drei"}).to_string()}},
        {"id": "cf", "type": "function", "function": {"name": "finish", "arguments": "{}"}}
    ]}}]})
    .to_string();
    let fake = Fake::start(answer.clone());
    let scene = Scene::new("numbered", &fake.url);
    scene.enter();
    let numbers = [1usize, 2, 3];
    let part: Vec<(usize, String)> =
        [(1, "one"), (2, "two"), (3, "three")].map(|(n, t)| (n, t.to_string())).into();
    let message = format!(
        "{}\n{}",
        naivepost::produce_subtitles::message(3, "de"),
        naivepost::produce_subtitles::numbered(&part)
    );
    let want: BTreeMap<usize, String> = [(1, "eins"), (2, "zwei"), (3, "drei")]
        .into_iter()
        .map(|(n, text)| (n, text.to_string()))
        .collect();

    let first = naivepost::produce_translate_ask::ask_at(
        &scene.tree,
        &scene.conf,
        &message,
        &numbers,
        &|| false,
    )
    .expect("the batch translated");
    assert_eq!(first, want, "the answer came off the wire");
    assert_eq!(fake.waited(1).len(), 1, "one request went out");
    let rows_after_first = requests::read(&scene.tree).expect("requests.tsv").len();

    // One slot, and its BYTES are the numbered text rather than the envelope §3.10 answered with.
    let slot = assert_one_hex_slot(&scene.cached_names(naivepost::produce_translate_ask::STEP), "translate");
    let filed = std::fs::read_to_string(
        scene.tree.dir().join("cache/llm").join(naivepost::produce_translate_ask::STEP).join(&slot),
    )
    .expect("the slot reads back");
    assert_eq!(filed, "1\teins\n2\tzwei\n3\tdrei", "one numbered line per number, in the order asked");
    for folded in ["1\teins", "2\tzwei", "3\tdrei"] {
        assert!(filed.contains(folded), "{folded} is in the filed text: {filed}");
    }
    assert!(!filed.contains("choices") && !filed.contains("tool_calls"),
        "the reply envelope is NOT what was filed: {filed}");
    assert!(!answer.contains(&filed), "and the filed text is not a copy of the reply either");

    // The numbered slot replays: same map, no socket, no new row.
    let second = naivepost::produce_translate_ask::ask_at(
        &scene.tree,
        &scene.conf,
        &message,
        &numbers,
        &|| false,
    )
    .expect("the cached batch still translates");
    assert_eq!(second, want, "the replay says the same thing");
    assert_eq!(fake.count(), 1, "the second ask opened no socket: {}", fake.count());
    assert_eq!(wire_rows(&scene.tree), rows_after_first, "a cache hit adds no wire row");
    assert_eq!(cache_rows(&scene.tree), 1, "a cache hit writes its own marked row");
    scene.leave();
}

/// §6's other translate half, on the live leg: the gap-filling second call never earns a place in the
/// cache. The leg sees only a message and its numbers, so the walk's own repair wording is what has to
/// decide it — which is why the marker is checked against THAT text and not against a copy of it here.
#[test]
fn sec_09_llm_and_tools_6_cache_s7_the_gap_filling_call_is_never_filed() {
    let _turn = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Two lines answered, one held back, so the walk's re-ask path is the one under test.
    let answer = json!({"choices": [{"message": {"content": null, "tool_calls": [
        {"id": "c1", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 1, "text": "eins"}).to_string()}},
        {"id": "cf", "type": "function", "function": {"name": "finish", "arguments": "{}"}}
    ]}}]})
    .to_string();
    let fake = Fake::start(answer);
    let scene = Scene::new("gapfill", &fake.url);
    scene.enter();
    let step = naivepost::produce_translate_ask::STEP;
    let part: Vec<(usize, String)> =
        [(1, "one"), (2, "two"), (3, "three")].map(|(n, t)| (n, t.to_string())).into();
    let first_message = format!(
        "{}\n{}",
        naivepost::produce_subtitles::message(3, "de"),
        naivepost::produce_subtitles::numbered(&part)
    );

    // The marker against the two wordings the walk actually sends.
    assert!(
        !naivepost::produce_subtitles::is_repair_request(&first_message),
        "the first call is not a repair: {first_message}"
    );
    // The batch as the walk builds it: line 2 never came back, so 2 and 3 are what it asks again for.
    let mut batch = naivepost::produce_subtitles::Batch::new((0, 3));
    batch.translate_line(1, "eins").expect("line 1 is inside this batch");
    assert_eq!(batch.missing(), vec![2, 3], "the walk's own bookkeeping decides the re-ask");
    let repair_message = batch.ask_again_text(&part);
    assert!(
        naivepost::produce_subtitles::is_repair_request(&repair_message),
        "the walk's repair text carries the marker: {repair_message}"
    );

    // First round: complete enough to be filed? No — a PARTIAL answer is refused a slot (§S3), so this
    // scene starts with nothing filed, which is what makes the repair half below readable.
    let got = naivepost::produce_translate_ask::ask_at(
        &scene.tree,
        &scene.conf,
        &first_message,
        &[1usize, 2, 3],
        &|| false,
    )
    .expect("the batch answered what it could");
    assert_eq!(got.len(), 1, "only line 1 came back: {got:?}");
    assert!(scene.cached_names(step).is_empty(), "a partial answer is never filed: {:?}", scene.cached_names(step));

    // The repair call: correct answer, and STILL no slot for it (§6's "never caches its gap-filling
    // second call"), even though every number it asked for comes back this time.
    let repaired = json!({"choices": [{"message": {"content": null, "tool_calls": [
        {"id": "r2", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 2, "text": "zwei"}).to_string()}},
        {"id": "r3", "type": "function", "function": {"name": "translate_line",
            "arguments": json!({"n": 3, "text": "drei"}).to_string()}},
        {"id": "rf", "type": "function", "function": {"name": "finish", "arguments": "{}"}}
    ]}}]})
    .to_string();
    fake.set_answer(repaired);
    let again = naivepost::produce_translate_ask::ask_at(
        &scene.tree,
        &scene.conf,
        &repair_message,
        &[2usize, 3],
        &|| false,
    )
    .expect("the repair call answered");
    let want: BTreeMap<usize, String> = [(2, "zwei"), (3, "drei")]
        .into_iter()
        .map(|(n, text)| (n, text.to_string()))
        .collect();
    assert_eq!(again, want, "the repair did its job: {again:?}");
    assert_eq!(fake.count(), 2, "both calls went out: {}", fake.count());
    assert!(
        scene.cached_names(step).is_empty(),
        "§6: the gap-filling call is never filed -- {:?}",
        scene.cached_names(step)
    );
    scene.leave();
}
