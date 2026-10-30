//! F1.7 Describe — the ▶ path, spec/04-prepare.md.
//!
//! `tests/suite/describe_flow.rs` proves [`naivepost::describe`]'s rules with no window and no model, and
//! `tests/suite/describe_call.rs` proves the request leg through the stage's own door. This file is the
//! walk: `run_stages::plan(Step::Prepare)` filtered to the describe stage, run through
//! `run_stages::drive_walk`, so the row's DIAL (`VISION_TASK`) is what gets exercised end to end —
//! S0's refusals, S1's scaling into `.llmframes/` (real ffmpeg, stubbed here the way `tests/suite/f1_6_frames_run.rs`
//! stubs it: the stub CREATES the file its argv names), S2's resume, S5's tools, S6's two files and
//! S7's closing sentence. A rule test cannot see the bug this file exists for: a stage that composed a
//! perfect request but never scaled a frame, never wrote a row, or asked twice for a chunk already done.
//!
//! The parameter the flow runs on is pinned after its token too: `P.machine.describeFramesPerReq`.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::common;
use common::temp_dir_tagged;
use naivepost::clock::frame_name;
use naivepost::describe::{self, FRAMES_PER_REQ};
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::run::{self, Subprocesses};
use naivepost::run_stages;
use naivepost::settings::{self, Conf, Paths};

/// The lane of every scene here: the stem of the footage row's name, which is what names `events.tsv`,
/// `state.txt` and `.llmframes/` (§F1.3 stores under the lane).
const LANE: &str = "lecture";

/// The env and the working folder belong to the PROCESS and cargo runs these tests as threads, so the
/// tests that move them take them one at a time and put them back before asserting.
static ENV: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The §3.2 answer for a batch of ANY size: a `record_event` for every frame of it, then a
/// `set_state`, then `finish` — copied from `tests/suite/describe_call.rs`, where it is explained: the last
/// chunk of nine frames at four a call is a batch of ONE, and an answer written for four would report
/// that chunk as incomplete for a reason unrelated to what is being checked.
fn events_answer(frames: usize) -> String {
    let mut calls = String::new();
    for frame in 1..=frames {
        calls.push_str(&format!(
            "{{\"function\":{{\"name\":\"record_event\",\"arguments\":\"{{\\\"frame\\\":{frame},\\\"text\\\":\\\"frame {frame} of the take\\\"}}\"}}}},",
        ));
    }
    calls.push_str(
        "{\"function\":{\"name\":\"set_state\",\"arguments\":\"{\\\"text\\\":\\\"mid-lecture, board half full\\\"}\"}},",
    );
    calls.push_str("{\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}");
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{calls}]}}}}]}}")
}

/// What the fake answers with, and how many chat asks it has taken. Copied from
/// `tests/suite/describe_call.rs`: the seam is Settings' address, so nothing here hands the program a reply.
struct Fake {
    url: String,
    asks: std::sync::Arc<Mutex<Vec<ChatAsk>>>,
    /// Held so the reply outlives the connection threads that read it; no test here changes it.
    #[allow(dead_code)]
    answer: std::sync::Arc<Mutex<String>>,
}

use common::{serve_chat as serve, ChatAsk};

impl Fake {
    fn start(answer: String) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let asks = std::sync::Arc::new(Mutex::new(Vec::new()));
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
                            row.push(ask);
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

    /// The chat requests recorded so far. Recording happens on the connection's own thread, so a reply
    /// can return before its entry lands; callers wait for the count they expect.
    fn waited(&self, count: usize) -> Vec<ChatAsk> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let seen = self.asks.lock().expect("the record lock").clone();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// How many asks arrived, without waiting: the refusal cases must prove NONE did.
    fn count(&self) -> usize {
        self.asks.lock().expect("the record lock").len()
    }
}

/// One scene: a session whose project holds the given sources, frames planted on disk (and optionally
/// pre-scaled copies), a settings folder whose conf names this scene's stub ffmpeg and this scene's
/// fake LLM, and the recorded ffmpeg calls.
/// Describe's heartbeat: the scenes plant one frame per heartbeat, so every planted frame is one describe sends.
const HEARTBEAT: f64 = naivepost::text_gate::HEARTBEAT_S;

struct Scene {
    root: PathBuf,
    /// The scene's own settings folder, kept so a test could re-point the conf if it needed to.
    #[allow(dead_code)]
    paths: Paths,
    tools: PathBuf,
    tree: Tree,
}

impl Scene {
    /// Frames at `interval` seconds apart, starting at the epoch `clock::frame_name` names from, so
    /// `describe::frames_on_disk` reads them back as 0.0, interval, 2·interval… Nothing is pre-scaled:
    /// the copies are the program's job, which is most of what these tests check.
    fn new(interval: f64, frames: usize, llm_url: &str) -> Scene {
        let seconds: Vec<f64> = (0..frames).map(|index| index as f64 * interval).collect();
        Scene::planted(interval, &seconds, llm_url)
    }

    /// The same, with the exact seconds to plant — for an every-frame folder, whose spacing at the 250 ms
    /// grid is itself the thing S0 reads.
    fn planted(_interval: f64, seconds: &[f64], llm_url: &str) -> Scene {
        let root = temp_dir_tagged(&format!(
            "f17-run-{}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(session.join("sources")).expect("the sources folder");
        std::fs::write(session.join("sources/lecture.mkv"), b"mkv-bytes").expect("the recording");
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let tools = stubs(&format!("{}", COUNTER.load(Ordering::SeqCst)));
        let mut conf = Conf::default();
        conf.server = llm_url.to_string();
        conf.model = "qwen-vl".to_string();
        conf.ffmpeg = tools.join("ffmpeg").to_string_lossy().into_owned();
        settings::save(&paths, &conf).expect("the conf is written");
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
        let tree = Tree::new(session).expect("the session is a project");
        let frames_dir = tree.frames_dir(LANE);
        std::fs::create_dir_all(&frames_dir).expect("the frames folder");
        for at in seconds {
            std::fs::write(frames_dir.join(format!("{}.jpg", frame_name(*at))), b"jpeg")
                .expect("the frame");
        }
        // F1.6's marker, so a walk that reaches the frames stage says "already extracted" rather than
        // failing over placeholder bytes: describe's own work is what these tests watch.
        std::fs::write(tree.frames_marker(LANE), "0.25|1.0\n").expect("the marker");
        Scene {
            root,
            paths,
            tools,
            tree,
        }
    }

    /// Add a second, non-footage source: the microphone heard alongside the video (S1's log line).
    fn with_audio(&mut self, name: &str) {
        let mut project = self.project();
        project.sources.push(Source {
            path: format!("project:sources/{name}"),
            footage: false,
            narrator: 1,
            sepvoice: false,
            tracks: vec![],
        });
        naivepost::project::save(&project, self.tree.dir()).expect("the session saves");
    }

    /// Pre-write one chunk's first row: §S2's resume marker, exactly as a stopped run left it.
    fn seed_event_row(&self, at: f64, text: &str) {
        std::fs::create_dir_all(self.tree.describe_dir(LANE)).expect("the describe folder");
        std::fs::write(
            self.tree.events_tsv(LANE),
            format!("{}\t{}\t{text}\n", fmt(at), fmt(at + 1.0)),
        )
        .expect("events.tsv");
    }

    fn project(&self) -> Project {
        naivepost::project::load(self.tree.dir()).expect("the session")
    }

    /// Point the process at this scene exactly as the app resolves both.
    fn enter(&self) {
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
    }

    fn leave(&self) {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    /// The describe stage only, planned from the project file the way ▶ plans it.
    fn stages(&self) -> Vec<run_stages::Stage> {
        run_stages::plan(run::Step::Prepare, &self.project())
            .into_iter()
            .filter(|stage| stage.name == "describe")
            .collect()
    }

    /// Walk it. Returns the drive's answer and every line the walk logged, oldest first.
    fn walk(&self) -> (run_stages::Drive, Vec<String>) {
        let mut queue = naivepost::runqueue::Queue::new();
        let mut procs = Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let drive = run_stages::drive_walk(
            &self.stages(),
            &mut queue,
            || false,
            || false,
            |line| {
                log.push(line.to_string());
            },
            &mut procs,
        );
        run_stages::say_outcome(&drive, |line| log.push(line.to_string()));
        (drive, log)
    }

    /// Every ffmpeg call the stub was asked to make, oldest first, one argv per line.
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.tools.join("argv.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The scaled copies on disk, sorted by name (which is sorted by second).
    fn copies(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.tree.llm_frames_dir(LANE))
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .filter(|name| name.ends_with(".jpg"))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn events(&self) -> String {
        std::fs::read_to_string(self.tree.events_tsv(LANE)).unwrap_or_default()
    }

    fn event_rows(&self) -> Vec<String> {
        self.events()
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn state(&self) -> String {
        std::fs::read_to_string(self.tree.describe_state(LANE)).unwrap_or_default()
    }
}

/// Seconds as `textfmt` spells them in a row, so an assertion compares spellings rather than guessing.
fn fmt(secs: f64) -> String {
    naivepost::textfmt::event_row(secs, secs, "x")
        .split('\t')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Walk a scene under the one guard the env needs, and hand back what the walk came to.
fn walk(scene: &Scene) -> (run_stages::Drive, Vec<String>) {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    scene.enter();
    let outcome = scene.walk();
    scene.leave();
    outcome
}

/// The stub tools, copied in shape from `tests/suite/f1_6_frames_run.rs`: `ffprobe` answers a duration, and
/// `ffmpeg` logs every argv and writes the file its argv names. Here the output is the LAST argument
/// (the scaling leg ends with the output directory and ffmpeg builds the file name from the input),
/// so the stub writes beside that directory under the input's own name — a stub that only succeeded
/// would let a pass that wrote no copies look like work.
fn stubs(tag: &str) -> PathBuf {
    let dir = temp_dir_tagged(&format!("np-f17-tools-{tag}"));
    let probe = dir.join("ffprobe");
    common::write_script(&probe, "#!/bin/sh\necho \"20.0\"\n").expect("the probe stub");
    let script = dir.join("ffmpeg.sh");
    // Log the argv, then write the FILE the argv names: an output ending in `.jpg` is written as given
    // (that is the spelling the scaling leg now uses), anything else is the directory the copy goes into,
    // named after the `-i` file. `[ -f ... ]` makes the stub fail loudly if the path is nonsense.
    common::write_script(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{dir}/argv.log\"\ninput=''\nout=''\nprev=''\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-i\" ]; then input=\"$a\"; fi\n  out=\"$a\"\n  prev=\"$a\"\ndone\ncase \"$out\" in\n  *.jpg) mkdir -p \"$(dirname -- \"$out\")\"; printf JPEG > \"$out\" ;;\n  *) base=$(basename -- \"$input\"); mkdir -p \"$out\"; printf JPEG > \"$out/$base\" ;;\nesac\n[ -s \"$out\" ] || [ -d \"$out\" ]\n",
            dir = dir.display()
        ),
    )
    .expect("the ffmpeg stub");
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("the script runs");
    let ffmpeg = dir.join("ffmpeg");
    common::write_script(
        &ffmpeg,
        format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", script.display()),
    )
    .expect("the wrapper");
    for tool in [&probe, &ffmpeg] {
        std::fs::set_permissions(tool, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("the stub runs");
    }
    dir
}

// ---- S1 ---------------------------------------------------------------------

/// S1: every frame that goes is scaled ONCE into `.llmframes/`, at 896 wide, and the stored frames are
/// left alone. P.machine.describeFrameWidth
#[test]
fn f1_7_s1_the_frames_are_scaled_once_into_llmframes() {
    let fake = Fake::start(events_answer(FRAMES_PER_REQ));
    let scene = Scene::new(HEARTBEAT, 4, &fake.url);
    let before = scene.copies().len();
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "the row walked: {drive:?} / {log:?}"
    );
    let picked = naivepost::describe::frames_on_disk(&scene.tree.frames_dir(LANE));
    let expected: Vec<String> = picked
        .iter()
        .map(|at| {
            describe::scaled_name(naivepost::describe_scale::name_base(&scene.tree, LANE), *at)
        })
        .collect();
    let copies = scene.copies();
    assert_eq!(before, 0, "nothing is pre-scaled by the test");
    assert_eq!(
        copies, expected,
        "one scaled copy per frame that goes, named by its second: {copies:?}"
    );
    let calls = scene.calls();
    let scales: Vec<&String> = calls
        .iter()
        .filter(|line| line.contains("scale=896:-2"))
        .collect();
    assert_eq!(
        scales.len(),
        picked.len(),
        "one ffmpeg per scaled frame: {calls:?}"
    );
    assert_eq!(describe::FRAME_WIDTH, 896, "P.machine.describeFrameWidth");
    // The kept frames keep their own size: no extraction ever scales, only the copies do.
    assert!(
        !calls
            .iter()
            .any(|line| line.contains("scale=") && !line.contains("scale=896:-2")),
        "only the sent copies are scaled: {calls:?}"
    );
    fake.waited(1);
}

/// S1 + S2 together: a second walk over the same folder scales nothing again and asks nothing again —
/// the copies and the rows are both resume markers.
#[test]
fn f1_7_s1_a_second_walk_scales_nothing_again() {
    let fake = Fake::start(events_answer(FRAMES_PER_REQ));
    let scene = Scene::new(HEARTBEAT, 4, &fake.url);
    let (_first, _) = walk(&scene);
    fake.waited(1);
    let calls_after_first = scene.calls().len();
    let asks_after_first = fake.count();
    let (_second, log) = walk(&scene);
    assert_eq!(
        scene.calls().len(),
        calls_after_first,
        "the second walk ran no ffmpeg at all: {:?}",
        &scene.calls()[calls_after_first..]
    );
    assert_eq!(
        fake.count(),
        asks_after_first,
        "and no chat request: {log:?}"
    );
    assert!(
        log.iter().any(|line| line.contains("event log complete")),
        "it still said what it finished: {log:?}"
    );
}

// ---- S0 ---------------------------------------------------------------------

/// S0: a folder extracted frame-by-frame is refused, in words, and costs no request. The row is NOT
/// fatal either: §Failure-is-specific keeps the run going with the sources that can be described.
#[test]
fn f1_7_s0_an_every_frame_folder_is_refused_and_costs_no_request() {
    // Spacing finer than the 250 ms grid (the video's own frames, 1/30 s apart) is what an unthinned extraction
    // looks like; a folder ON the grid is the normal one and is described.
    let seconds: Vec<f64> = (0..90).map(|index| index as f64 / 30.0).collect();
    let fake = Fake::start(events_answer(12));
    let scene = Scene::planted(0.25, &seconds, &fake.url);
    let (drive, log) = walk(&scene);
    assert!(
        log.iter()
            .any(|line| line.contains(&describe::every_frame_refusal(LANE))),
        "S0's refusal, in its own words: {log:?}"
    );
    assert_eq!(
        fake.count(),
        0,
        "an every-frame folder asks nothing of the model: {log:?}"
    );
    assert!(scene.copies().is_empty(), "and scales nothing");
    assert!(
        !log.iter()
            .any(|line| line.starts_with("!!! describe failed")),
        "the refusal is local, not the run's end: {log:?}"
    );
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "the stage completed: {drive:?}"
    );
}

/// S1's other refusal, and the reason the scaling leg runs BEFORE any request: a frame F1.6 never
/// extracted stops the pass naming that picture, rather than a chunk being stamped with no image beside
/// it. The seconds come from the grid the marker says was used, so a hole in the folder is an error and
/// not a short batch.
#[test]
fn f1_7_s1_a_frame_that_was_never_extracted_names_itself() {
    let fake = Fake::start(events_answer(1));
    let scene = Scene::new(HEARTBEAT, 4, &fake.url);
    // Remove one STORED frame before anything runs: the leg must refuse that second by name rather than
    // scale three of four and stamp a batch whose pictures do not all exist.
    let gone = scene
        .tree
        .frames_dir(LANE)
        .join(format!("{}.jpg", frame_name(2.0 * HEARTBEAT)));
    std::fs::remove_file(&gone).expect("one stored frame goes");
    let (drive, log) = walk(&scene);
    assert!(
        matches!(drive, run_stages::Drive::Failed { .. }),
        "a missing source frame fails the row, named: {drive:?}"
    );
    assert_eq!(
        fake.count(),
        0,
        "and nothing is asked of the model first: {log:?}"
    );
    // A frame F1.6 never extracted is refused by name, before any request: the seconds come from the
    // grid the marker says was used, so a picture missing from the folder is an error, not a short batch.
    assert!(
        log.iter()
            .any(|line| line.contains("no scaled frame") && line.contains("00-00-40.000")),
        "the refusal names the picture that is missing: {log:?}"
    );
}

/// S1: each recording heard alongside the video is logged with the offset its words sit at, BEFORE any
/// request — the words are what the descriptions are built from.
#[test]
fn f1_7_s1_an_alongside_recording_is_logged_with_its_offset() {
    let fake = Fake::start(events_answer(FRAMES_PER_REQ));
    let mut scene = Scene::new(HEARTBEAT, 4, &fake.url);
    scene.with_audio("mic.wav");
    let (_drive, log) = walk(&scene);
    // The video starts at second nought, so the mic's placement IS its offset.
    let project = scene.project();
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|s| s.path.rsplit('/').next().unwrap())
        .collect();
    let offsets = naivepost::fix_transcripts::placement(&names);
    let mic = offsets.get(1).copied().unwrap_or(0.0);
    let wanted = describe::hearing_log(LANE, "mic.wav", mic);
    assert!(
        log.contains(&wanted),
        "S1's alongside line, verbatim: wanted {wanted} in {log:?}"
    );
    fake.waited(1);
}

// ---- S5 / S6 ----------------------------------------------------------------

/// S5's answer through S6's files: one `events.tsv` row per frame in §6's three-field shape, covering
/// the grid in order, plus the STATE the model set — and the request carried four pictures.
#[test]
fn f1_7_s5_and_s6_the_answer_lands_as_one_row_per_frame_plus_the_state() {
    let fake = Fake::start(events_answer(FRAMES_PER_REQ));
    let scene = Scene::new(HEARTBEAT, 4, &fake.url);
    let (_drive, log) = walk(&scene);
    let asks = fake.waited(1);
    assert_eq!(asks.len(), 1, "four frames fit one call: {log:?}");
    let sent: serde_json::Value = serde_json::from_str(&asks[0].body).expect("a JSON body");
    // The pictures ride on the USER turn; §8's assembled house rules now lead as their own system message,
    // so picking "the first message" here would read the system turn and count no pictures at all.
    let parts = sent["messages"][1]["content"]
        .as_array()
        .expect("the user parts");
    let images = parts
        .iter()
        .filter(|part| part["type"].as_str() == Some("image_url"))
        .count();
    assert_eq!(
        images, FRAMES_PER_REQ,
        "P.machine.describeFramesPerReq pictures in one call"
    );
    let rows = scene.event_rows();
    assert_eq!(
        rows.len(),
        FRAMES_PER_REQ,
        "one row per frame: {}",
        scene.events()
    );
    let picked = naivepost::describe::frames_on_disk(&scene.tree.frames_dir(LANE));
    for (index, row) in rows.iter().enumerate() {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 3, "§6's three fields: {row}");
        let start: f64 = fields[0].parse().expect("start is a number");
        let end: f64 = fields[1].parse().expect("end is a number");
        assert!(
            (start - picked[index]).abs() <= 0.01,
            "row {} starts at its own frame's second {picked:?}: {row}",
            index + 1
        );
        assert!(end >= start, "a row ends no earlier than it starts: {row}");
        assert!(
            fields[2].starts_with("frame "),
            "the model's own words: {row}"
        );
    }
    assert_eq!(
        scene.state().trim(),
        "mid-lecture, board half full",
        "the running STATE landed"
    );
}

// ---- S2 ---------------------------------------------------------------------

/// S2: a chunk whose start already has a row is not asked again — the rows say what was answered, and
/// paying a request to reproduce work that is on disk would be the resume rule backwards.
#[test]
fn f1_7_s2_a_chunk_already_in_events_tsv_is_not_asked_again() {
    let fake = Fake::start(events_answer(FRAMES_PER_REQ));
    let scene = Scene::new(HEARTBEAT, 8, &fake.url);
    // Eight frames at four a call is two chunks; the first one's START is what marks it answered.
    scene.seed_event_row(0.0, "answered by an earlier run");
    let (_drive, log) = walk(&scene);
    let asks = fake.waited(1);
    assert_eq!(asks.len(), 1, "only the second chunk was asked: {log:?}");
    assert!(
        log.iter()
            .any(|line| line.contains("answered from the cache")),
        "the ending counts the resumed chunk: {log:?}"
    );
    // The rows the earlier run wrote survive: a resume appends, it does not rewrite history.
    assert!(
        scene.events().contains("answered by an earlier run"),
        "the existing rows are kept: {}",
        scene.events()
    );
}

// ---- S7 ---------------------------------------------------------------------

/// S7: the pass closes with its chunk count, live. Nine frames at four a call is three chunks.
#[test]
fn f1_7_s7_the_pass_ends_with_its_chunk_count() {
    let fake = Fake::start(events_answer(FRAMES_PER_REQ));
    let scene = Scene::new(HEARTBEAT, 9, &fake.url);
    let (_drive, log) = walk(&scene);
    fake.waited(3);
    let said: Vec<&str> = log
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with(">>> ["))
        .collect();
    let last = said.last().copied().unwrap_or_default();
    // Every chunk answered live, so §S7's plain spelling -- the cached variant is S4's test below.
    assert_eq!(
        last,
        describe::complete_log_cached(LANE, 3, 0).as_str(),
        "S7's sentence, counted: {log:?}"
    );
    // The bound the chunk count came from, spelled out after its token: four frames a call.
    assert_eq!(
        naivepost::params::find("P.machine.describeFramesPerReq")
            .expect("the row is catalogued")
            .spelled,
        "4",
        "P.machine.describeFramesPerReq"
    );
}

/// S4: the second walk over the same folder pays nothing — the cache is keyed on the whole content
/// including the images, so an unchanged chunk is free and says so.
#[test]
fn f1_7_s4_an_unchanged_run_answers_from_the_cache() {
    let fake = Fake::start(events_answer(FRAMES_PER_REQ));
    let scene = Scene::new(HEARTBEAT, 4, &fake.url);
    let (_first, _) = walk(&scene);
    fake.waited(1);
    // Wipe the ANSWER files so the second walk cannot lean on S2's row-resume: what remains is the
    // request cache, which is the thing S4 is about.
    let _ = std::fs::remove_file(scene.tree.events_tsv(LANE));
    let _ = std::fs::remove_file(scene.tree.describe_state(LANE));
    let (_second, log) = walk(&scene);
    assert_eq!(
        fake.count(),
        1,
        "the second walk made no ask at all: {log:?}"
    );
    assert!(
        log.iter()
            .any(|line| line.contains("answered from the cache")),
        "and said where the answer came from: {log:?}"
    );
    // The rows are BACK: the replay wrote one per frame while the wire stayed silent, each in the model's own
    // words. §6's forced line stands only where the first frame has none (`events_rows`); a cached answer that
    // names that frame keeps its reading. A cold replay once came back one row short, `write_batch` refused to
    // write half an answer, and a cached chunk cost nothing AND wrote nothing while the pass closed "event log complete".
    let rows = scene.event_rows();
    assert_eq!(
        rows.len(),
        FRAMES_PER_REQ,
        "the replay wrote one row per frame: {rows:?}"
    );
    assert!(
        rows[0].ends_with("frame 1 of the take"),
        "the cold replay opens on the model's own reading of its first frame: {rows:?}"
    );
    // The replay brings the running state back with the events: the cached answer holds the set_state call too.
    assert_eq!(
        scene.state().trim(),
        "mid-lecture, board half full",
        "the cached replay restores the state its answer set"
    );
    assert!(
        log.iter()
            .any(|line| line.starts_with(">>> [lecture] event log complete")),
        "the cached pass still closes: {log:?}"
    );
}
