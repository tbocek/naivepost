//! `spec/04-prepare.md` §3 (Data written), the `describe/<source>/events.tsv` row of
//! `spec/01-project-and-files.md` §1: `one row per frame; state.txt`.
//!
//! "One row per frame" is the whole point of this file, and the case that used to break it is a batch
//! with no history: its FIRST frame has nothing behind it for the word `same` to point at, so
//! §6's Describe bullet force-writes `Calm; same view.` there (`spec/inventory/prepare.md`: "the
//! first frame of a batch with no history is forced to \"Calm; same view.\""; `spec/12-decisions.md`
//! item 8 says the same). `Batch::finish` naming that frame back to the model
//! (`spec/02-services.md` §3.2) is a DIFFERENT door and stays as it was — the model is still told the
//! opening second says nothing, and the app writes the line regardless.
//!
//! s1–s3 exercise the writer's rule through `describe::events_rows` / `describe::write_chunk`; s4
//! drives the live door the Prepare walk opens (`describe_call::stage_chunk`) against a loopback LLM,
//! so the literal is proven to reach the FILE on the real path, not only through the helper.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use naivepost::clock::frame_name;
use naivepost::describe::{self, FRAMES_PER_REQ};
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::settings::{self, Conf, Paths};
use naivepost::textfmt::FrameEvent;
use naivepost::tools::describe::{Batch, Event};

/// The lane every scene here uses: the stem of the one footage row's name, which names `events.tsv`
/// and `state.txt`.
const LANE: &str = "lecture";
/// The step the scenes space their frames on: describe's heartbeat, the spacing `describe::pick_frames`
/// walks and `is_every_frame` accepts (as in `tests/suite/p_machine_describe_frame_width.rs`).
const INTERVAL: f64 = naivepost::text_gate::HEARTBEAT_S;
/// §6's forced opening line, spelled once in the program. Named through the constant AND pinned as
/// bytes here, so a reword fails somewhere rather than moving silently.
const CALM: &str = "Calm; same view.";

/// The env and the working folder belong to the PROCESS and cargo runs these as threads of one
/// process, so whoever moves them takes them one at a time.
static ENV: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

// ---- the writer's rule ------------------------------------------------------

/// s1: one row per frame, even when the batch has no history — and the opening row is §6's forced
/// line, not an omitted frame and not a bare `same`.
#[test]
fn sec_04_prepare_3_data_written_s1_a_cold_batch_writes_the_calm_line_for_its_first_frame() {
    let frames = [0.0, 1.0, 2.0];
    let rows = describe::events_rows(&frames, &[], false, 3.0);
    assert_eq!(rows.len(), frames.len(), "one row per frame: {rows:?}");
    let first = rows[0].split('\t').next_back().expect("a row has its text");
    assert_eq!(
        first,
        Batch::SAME_WITHOUT_HISTORY,
        "the opening row is the forced line"
    );
    assert_eq!(first, CALM, "and the literal §6 spells");
    for row in &rows[1..] {
        assert!(
            row.ends_with("\tsame"),
            "every later frame still says `same`: {row:?}"
        );
    }

    // And they reach the file the stage appends to, in order, three lines.
    let (_root, tree) = scratch("s1");
    describe::write_chunk(&tree, LANE, &rows, Some("nothing yet")).expect("the chunk lands");
    let body = std::fs::read_to_string(tree.events_tsv(LANE)).expect("events.tsv");
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 3, "one line per frame: {body:?}");
    assert!(
        lines[0].ends_with(CALM),
        "the file OPENS on the forced line: {body:?}"
    );
    let state = std::fs::read_to_string(tree.describe_state(LANE)).expect("state.txt");
    assert_eq!(
        state, "nothing yet",
        "state.txt goes with the rows (§F1.7 S6)"
    );
}

/// s2: the visible half of the bug — read back, that opening line is a REAL event. A `same` row with
/// nothing before it folds into nothing, so a cold file that opened on `same` said nothing to any
/// reader (`fix_transcripts::load_events`, the upload brief, the resume).
#[test]
fn sec_04_prepare_3_data_written_s2_the_written_line_is_a_real_event_when_read_back() {
    let (_root, tree) = scratch("s2");
    let cold = describe::events_rows(&[0.0, 1.0], &[], false, 2.0);
    describe::write_chunk(&tree, "cold", &cold, None).expect("cold chunk");
    assert_eq!(
        naivepost::textfmt::read_events(&tree.events_tsv("cold")).expect("the cold file parses"),
        vec![FrameEvent {
            start: 0.0,
            end: 2.0,
            text: CALM.to_string()
        }],
        "the forced line is the file's first reading, and the `same` after it extends it"
    );

    // The warm fold is untouched: a `same` row adds no event, it lengthens the one before it.
    let warm = describe::events_rows(
        &[0.0, 1.0, 2.0],
        &[Event {
            frame: 1,
            text: "board up".into(),
            calm: false,
        }],
        true,
        3.0,
    );
    describe::write_chunk(&tree, "warm", &warm, None).expect("warm chunk");
    assert_eq!(
        naivepost::textfmt::read_events(&tree.events_tsv("warm")).expect("the warm file parses"),
        vec![FrameEvent {
            start: 0.0,
            end: 3.0,
            text: "board up".to_string()
        }],
        "a `same` after a real reading extends it and adds nothing"
    );
}

/// s3: the sibling case `Batch::finish` already reports — the model DID stamp `same` on a cold batch's
/// first frame. That row is force-written too; with history behind it the same input stays `same`.
#[test]
fn sec_04_prepare_3_data_written_s3_a_cold_frame_answered_same_is_force_written() {
    let frames = [0.0, 1.0];
    let said_same = [Event {
        frame: 1,
        text: "same".into(),
        calm: false,
    }];
    let cold = describe::events_rows(&frames, &said_same, false, 2.0);
    assert!(
        cold[0].ends_with(CALM),
        "a cold `same` from the model becomes the forced line: {cold:?}"
    );

    let warm = describe::events_rows(&frames, &said_same, true, 2.0);
    assert!(
        warm[0].ends_with("\tsame"),
        "with history the same answer is true: {warm:?}"
    );

    // The two doors agree on WHICH frame this is, so the report and the file cannot drift. Frame 2 is
    // also unanswered here; the point is that frame 1 is reported for the no-history reason, not as an
    // ordinary gap.
    let mut batch = Batch::new(0.0, 1.0, 2);
    batch.record_event(1, "same", false);
    let first = batch
        .finish()
        .into_iter()
        .find(|gap| gap.frame == 1)
        .expect("the cold opening frame is named back to the model");
    assert_eq!(
        first.reason,
        naivepost::tools::describe::MissingReason::FirstFrameWithNoHistory,
        "`finish` still names it (02-services §3.2), even though the app writes the line"
    );
}

// ---- the live door ---------------------------------------------------------

/// The §3.2 answer for a batch of ANY size, deliberately leaving frame 1 unmentioned: `record_event`
/// for frames 2..=n, a `set_state`, then `finish`. Frame 1 is the frame §6 lets the app fill in, so a
/// reply that skips it must still write a complete log.
fn answer_skipping_first(frames: usize) -> String {
    let mut calls = String::new();
    for frame in 2..=frames {
        calls.push_str(&format!(
            "{{\"function\":{{\"name\":\"record_event\",\"arguments\":\"{{\\\"frame\\\":{frame},\\\"text\\\":\\\"thing {frame} happens\\\"}}\"}}}},",
        ));
    }
    calls.push_str("{\"function\":{\"name\":\"set_state\",\"arguments\":\"{\\\"text\\\":\\\"board mid-write\\\"}\"}},");
    calls.push_str("{\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}");
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{calls}]}}}}]}}")
}

struct Fake {
    url: String,
    asks: std::sync::Arc<Mutex<usize>>,
    #[allow(dead_code)] // held so the reply outlives the connection threads reading it
    answer: std::sync::Arc<Mutex<String>>,
}

impl Fake {
    fn start(answer: String) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let asks = std::sync::Arc::new(Mutex::new(0usize));
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
                    if serve(stream, &answer) {
                        if let Ok(mut count) = recorder.lock() {
                            *count += 1;
                        }
                    }
                });
            }
        });
        Fake {
            url: format!("http://127.0.0.1:{port}"),
            asks,
            answer: held,
        }
    }

    /// Requests recorded so far. Recording happens on the connection's own thread, so a reply can
    /// return before its entry lands: wait on the COUNT with a wall-clock deadline, never a sleep.
    fn waited(&self, count: usize) -> usize {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let seen = *self.asks.lock().expect("the record lock");
            if seen >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

fn serve(mut stream: TcpStream, answer: &str) -> bool {
    let Ok(mut reader) = stream.try_clone().map(std::io::BufReader::new) else {
        return false;
    };
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        let Ok(read) = reader.read_line(&mut header) else {
            break;
        };
        if read == 0 || header.trim().is_empty() {
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
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        answer.len(),
        answer
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    true
}

fn conf_at(url: &str) -> Conf {
    let mut conf = Conf::default();
    conf.server = url.to_string();
    conf.model = "qwen-vl".to_string();
    conf
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    match ENV.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// A real JPEG, so `describe_scale::ensure` has something for ffmpeg to scale (the stage refuses to
/// send a frame whose copy is missing).
fn write_real_frame(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).expect("the frames folder");
    let out = dir.join(name);
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=1280x720:rate=1:duration=1",
            "-frames:v",
            "1",
        ])
        .arg(&out)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg wrote {name}");
}

fn seconds(count: usize) -> Vec<f64> {
    (0..count).map(|index| index as f64 * INTERVAL).collect()
}

/// One footage source with real frames on disk, its own settings folder pointed at its own fake LLM.
struct Scene {
    root: PathBuf,
    paths: Paths,
    server: String,
}

impl Scene {
    fn new(label: &str, frames: usize, llm_url: &str) -> Scene {
        let root = std::env::temp_dir().join(format!(
            "np-pdw-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let session = root.join("session.naivepost");
        std::fs::create_dir_all(&session).expect("the session folder");
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let project = Project {
            sources: vec![Source {
                path: "project:sources/lecture.mkv".to_string(),
                footage: true,
                narrator: 0,
                sepvoice: false,
                tracks: vec![],
            }],
            context: "a lecture on linear algebra".to_string(),
            ..Default::default()
        };
        naivepost::project::save(&project, &session).expect("the session file");
        let tree = Tree::new(&session).expect("the session is a project");
        for at in seconds(frames) {
            write_real_frame(&tree.frames_dir(LANE), &format!("{}.jpg", frame_name(at)));
        }
        settings::save(&paths, &conf_at(llm_url)).expect("the conf");
        Scene {
            root,
            paths,
            server: llm_url.to_string(),
        }
    }

    fn session(&self) -> PathBuf {
        self.root.join("session.naivepost")
    }

    fn tree(&self) -> Tree {
        Tree::new(self.session()).expect("the session is a project")
    }

    /// Move the process into this scene: settings through `XDG_CONFIG_HOME`, the session by cwd — how
    /// `describe_call::run_task` resolves both. The conf is written AGAIN because a sibling test binary
    /// moves the same process-wide variable and `ask` re-reads the settings folder at send time.
    fn enter(&self) {
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
        settings::save(&self.paths, &conf_at(&self.server)).expect("the scene's conf stands");
    }

    fn leave() {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    fn leg_conf(&self) -> Conf {
        let mut conf = conf_at(&self.server);
        if let Ok(read) = settings::read(&self.paths) {
            conf = read;
        }
        assert_eq!(
            conf.server, self.server,
            "the leg would dial another server than this fake"
        );
        conf
    }
}

/// s4: the live door. A cold chunk whose answer never mentions frame 1 must still SUCCEED and still
/// write one row per frame, opening on the forced line — the app fills the frame §6 lets it fill
/// instead of spending its attempts and failing over a line it owns.
#[test]
fn sec_04_prepare_3_data_written_s4_the_live_describe_run_lands_every_row() {
    let frames = FRAMES_PER_REQ;
    let fake = Fake::start(answer_skipping_first(frames));
    let scene = Scene::new("s4-live", frames, &fake.url);
    let tree = scene.tree();
    assert!(
        !tree.events_tsv(LANE).exists(),
        "this is a cold batch: nothing answered yet"
    );

    let ran = {
        let _guard = env_lock();
        scene.enter();
        let outcome = naivepost::describe_call::stage_chunk(
            &scene.tree(),
            &scene.paths,
            &scene.leg_conf(),
            LANE,
            Some(0),
            &mut |_line| {},
        );
        Scene::leave();
        outcome
    };
    assert_eq!(
        ran.expect("a cold chunk missing only its first frame is described, not refused"),
        1
    );
    assert_eq!(fake.waited(1), 1, "the chunk was asked about once");

    let body = std::fs::read_to_string(tree.events_tsv(LANE)).expect("events.tsv was written");
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(
        lines.len(),
        seconds(frames).len(),
        "one row per frame asked: {body:?}"
    );
    assert!(
        lines[0].ends_with(CALM),
        "the live file opens on §6's forced line: {body:?}"
    );
    for line in &lines[1..] {
        assert!(
            line.ends_with("\tsame") || line.contains("thing "),
            "every other frame carries the answer or `same`: {line:?}"
        );
    }
    let state = std::fs::read_to_string(tree.describe_state(LANE)).expect("state.txt");
    assert_eq!(
        state, "board mid-write",
        "the state the reply set reached the file"
    );
}

/// A scratch project folder under the name Describe files its work in.
fn scratch(tag: &str) -> (PathBuf, Tree) {
    let root =
        std::env::temp_dir().join(format!("naivepost-test-{}-pdw-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).expect("the project folder");
    (root, Tree::new(&dir).expect("the tree"))
}
