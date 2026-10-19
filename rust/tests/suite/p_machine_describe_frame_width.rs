//! `P.machine.describeFrameWidth` — spec/01-project-and-files.md §6, the row
//! `describe/<source>/events.tsv … state.txt; .llmframes/ (the frames sent, scaled to
//! P.machine.describeFrameWidth)`.
//!
//! The catalogued number is only worth having if a live run actually makes those copies and sends
//! them, so every case here goes through the doors the Prepare walk uses:
//! [`naivepost::describe_scale::ensure`] (the ffmpeg leg `describe_call::stage_chunk` calls) and
//! [`naivepost::describe_call::stage_chunk`] itself against a loopback LLM pointed at through the
//! Settings address — the seam every live job in this crate uses. The three artifacts of the row
//! (the scaled copies, `events.tsv`, `state.txt`) are read back off disk afterwards, and the
//! pictures the fake received are compared byte for byte against the files on disk, so a stage that
//! sent the native-size frames instead of the scaled ones fails here rather than quietly.
//!
//! The frames are REAL pictures: ffmpeg's own `testsrc` at 1280x720, and the width of what came
//! back is measured with ffprobe rather than trusted to the arguments we passed.

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

/// The lane every scene here uses: the stem of the one footage row's name, which names `events.tsv`,
/// `state.txt` and `.llmframes/`.
const LANE: &str = "lecture";
/// The native width the source frames are made at: anything wider than 896, so "scaled" is visible
/// as a difference rather than as ffmpeg agreeing with whatever it was asked for.
const SOURCE_WIDTH: u32 = 1280;
/// The interval the scenes space their frames on. `Project::default()`'s own (`P.project.frameInterval`):
/// `describe::pick_frames` walks one frame every interval inside a scene and `is_every_frame` refuses a
/// folder spaced at the extraction grid, so this is the one spacing that lets the stage reach the wire
/// (the same reason `tests/suite/describe_call.rs` gives for its own scenes).
const INTERVAL: f64 = 1.0;

/// The env and the working folder belong to the PROCESS and cargo runs these tests as threads of one
/// process, so whoever moves them takes them one at a time.
static ENV: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The §3.2 answer for a batch of ANY size: one `record_event` per frame, a `set_state`, then
/// `finish`. A batch of ONE (the last chunk of five frames at four a call) has to be answerable too,
/// which is why this is a builder and not a constant.
fn events_answer(frames: usize) -> String {
    let mut calls = String::new();
    for frame in 1..=frames {
        calls.push_str(&format!(
            "{{\"function\":{{\"name\":\"record_event\",\"arguments\":\"{{\\\"frame\\\":{frame},\\\"text\\\":\\\"slide {frame}\\\"}}\"}}}},"
        ));
    }
    calls.push_str("{\"function\":{\"name\":\"set_state\",\"arguments\":\"{\\\"text\\\":\\\"mid-lecture, board half full\\\"}\"}},");
    calls.push_str("{\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}");
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{calls}]}}}}]}}")
}

/// Answer each request with exactly the pictures THAT request carried: a chunk of five and a chunk of
/// one arrive at the same fake, and an answer written for four would report the second as incomplete.
const BY_SIZE: &str = "@by-size";

struct Fake {
    url: String,
    asks: std::sync::Arc<Mutex<Vec<ChatAsk>>>,
    #[allow(dead_code)] // held so the reply outlives the connection threads reading it
    answer: std::sync::Arc<Mutex<String>>,
}

#[derive(Debug, Clone)]
struct ChatAsk {
    #[allow(dead_code)] // the path matters to a wire test; kept so the struct is complete
    path: String,
    body: String,
}

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
    /// can return before its entry lands: wait on the COUNT, with a wall-clock deadline, never a sleep.
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
    let text = match answer {
        BY_SIZE => events_answer(String::from_utf8_lossy(&body).matches("\"image_url\"").count()),
        fixed => fixed.to_string(),
    };
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(ChatAsk {
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
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

/// A real JPEG at [`SOURCE_WIDTH`] px wide, made by ffmpeg itself. The scaled copy is only a proof if
/// its input had a different width to be scaled down from.
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
            &format!("testsrc=size={SOURCE_WIDTH}x720:rate=1:duration=1"),
            "-frames:v",
            "1",
        ])
        .arg(&out)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg wrote a source frame at {SOURCE_WIDTH}px");
    assert!(out.exists(), "ffmpeg wrote {}", out.display());
}

/// The width ffprobe reads for a picture. The measurement the item is about: the number on disk, not
/// the number in the arguments we passed.
fn width_of(path: &Path) -> u32 {
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .expect("ffprobe runs");
    let text = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    text.parse::<u32>()
        .unwrap_or_else(|_| panic!("ffprobe named no width for {}: {text}", path.display()))
}

/// The seconds `describe::frames_on_disk` will report for `count` frames written on `interval`.
fn seconds(count: usize) -> Vec<f64> {
    (0..count).map(|index| index as f64 * INTERVAL).collect()
}

/// The clock names F1.6 writes frames under: `frame_name(start + at)`, and `describe_scale::ensure`
/// looks each one up that way. `clock_start` for a single unstamped source is 0.0 (§8: an unstamped
/// file sits at the session's start), so `start` is 0.0 here — the same assumption in one place.
fn seed_real_frames(tree: &Tree, count: usize) -> Vec<f64> {
    let dir = tree.frames_dir(LANE);
    for at in seconds(count) {
        write_real_frame(&dir, &format!("{}.jpg", frame_name(at)));
    }
    seconds(count)
}

/// One footage source with real frames on disk, its own settings folder pointing at its own fake LLM.
///
/// `.llmframes/` is deliberately NOT created: a test that wants it there seeds it itself, and a stage
/// that failed to make it must fail rather than find a stranger's copy.
struct Scene {
    root: PathBuf,
    paths: Paths,
    server: String,
}

impl Scene {
    fn new(label: &str, frames: usize, llm_url: &str) -> Scene {
        let root = std::env::temp_dir().join(format!(
            "np-pdfw-{label}-{}-{}",
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
            interval: INTERVAL,
            context: "a lecture on linear algebra".to_string(),
            ..Default::default()
        };
        naivepost::project::save(&project, &session).expect("the session file");
        let tree = Tree::new(&session).expect("the session is a project");
        seed_real_frames(&tree, frames);
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

    /// Move the process into this scene: the settings folder through `XDG_CONFIG_HOME` and the session
    /// by cwd, which is how `describe_call::run_task` resolves both. The conf is written AGAIN here
    /// because a sibling binary moves the same process-wide variable (`just test` runs test programs
    /// concurrently) and `ask` re-reads the settings folder at send time (§1's seam).
    fn enter(&self) {
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
        settings::save(&self.paths, &conf_at(&self.server)).expect("the scene's conf stands for its walk");
        let project = Project {
            sources: vec![Source {
                path: "project:sources/lecture.mkv".to_string(),
                footage: true,
                narrator: 0,
                sepvoice: false,
                tracks: vec![],
            }],
            interval: INTERVAL,
            context: "a lecture on linear algebra".to_string(),
            ..Default::default()
        };
        naivepost::project::save(&project, &self.session()).expect("the session still holds its footage row");
    }

    fn leave() {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    /// The conf the leg will actually dial, read back from this scene's own settings folder — `ask`
    /// throws away the `conf` argument it is handed and re-reads at send time, so the file and the dial
    /// have to be one fact.
    fn leg_conf(&self) -> Conf {
        let mut conf = conf_at(&self.server);
        if let Ok(read) = settings::read(&self.paths) {
            conf = read;
        }
        assert_eq!(
            conf.server, self.server,
            "the leg would dial a different server than this scene's fake"
        );
        conf
    }

    /// The stage's own door — the one `run_task` opens after resolving the tree and the settings folder.
    fn describe(&self, conf: &Conf, log: &mut impl FnMut(&str)) -> Result<usize, String> {
        naivepost::describe_call::stage_chunk(&self.tree(), &self.paths, conf, LANE, Some(0), log)
    }
}

fn walk(scene: &Scene, log: &mut impl FnMut(&str)) -> Result<usize, String> {
    let _guard = env_lock();
    scene.enter();
    let outcome = scene.describe(&scene.leg_conf(), log);
    Scene::leave();
    outcome
}

/// The scaled copy's path for a second, as the stage names it.
fn copy_path(tree: &Tree, at: f64) -> PathBuf {
    tree.llm_frames_dir(LANE).join(describe::scaled_name(at))
}

/// How many files `.llmframes/` holds.
fn copy_count(tree: &Tree) -> usize {
    match std::fs::read_dir(tree.llm_frames_dir(LANE)) {
        Ok(entries) => entries.filter_map(|entry| entry.ok()).count(),
        Err(_) => 0,
    }
}

/// s1: ffmpeg really runs, really writes `.llmframes/`, and the copy really is 896 px — the same
/// number `describe::FRAME_WIDTH`, `frames::LLM_FRAME_WIDTH` and `params`' `P.machine.describeFrameWidth`
/// row carry. The STORED frame keeps its own size, and a second pass pays for nothing.
#[test]
fn p_machine_describeframewidth_s1_the_scaled_copies_are_made_at_the_catalogued_width() {
    let fake = Fake::start(BY_SIZE.to_string());
    let scene = Scene::new("s1-width", 3, &fake.url);
    let tree = scene.tree();
    let taken = seconds(3);
    assert_eq!(
        describe::frames_on_disk(&tree.frames_dir(LANE)),
        taken,
        "the scene's own frames are the ones the stage will read"
    );
    assert!(
        !tree.llm_frames_dir(LANE).exists(),
        "the scene plants no scaled copies: the stage has to make them"
    );
    // The one value, in its three spellings: the constant F1.6 owns, the alias F1.7 reads, and the
    // §10 catalogue row's answer. All three must be what ffmpeg produced.
    assert_eq!(naivepost::frames::LLM_FRAME_WIDTH, 896, "P.machine.describeFrameWidth");
    assert_eq!(describe::FRAME_WIDTH, naivepost::frames::LLM_FRAME_WIDTH);
    let row = naivepost::params::prepare()
        .into_iter()
        .find(|param| param.id == "P.machine.describeFrameWidth")
        .expect("the frame width is catalogued for Prepare, whose describe pass scales by it");
    assert_eq!(row.spelled, "896", "§10 spells the width plainly");
    assert_eq!(row.from, "frames::LLM_FRAME_WIDTH", "the row names the constant its rule reads");

    let conf = scene.leg_conf();
    let mut log = Vec::new();
    let made = naivepost::describe_scale::ensure(&tree, &conf, LANE, 0.0, &taken, &mut |line| {
        log.push(line.to_string())
    })
    .expect("the scaling leg completes");
    assert_eq!(made, 3, "one copy per frame, made here");
    for at in &taken {
        let copy = copy_path(&tree, *at);
        assert!(copy.exists(), "{}: the scaled copy is on disk", copy.display());
        assert_eq!(
            width_of(&copy),
            describe::FRAME_WIDTH,
            "the copy on disk is as wide as the catalogued number"
        );
        assert_eq!(
            width_of(&tree.frames_dir(LANE).join(format!("{}.jpg", frame_name(*at)))),
            SOURCE_WIDTH,
            "the STORED frame keeps the video's own size: only the copies sent are scaled"
        );
    }
    // "Scaled ONCE": a copy already on disk is not paid for twice, which is also S2's resume marker.
    let again = naivepost::describe_scale::ensure(&tree, &conf, LANE, 0.0, &taken, &mut |_| {})
        .expect("a second pass completes");
    assert_eq!(again, 0, "an existing copy is never made twice");
    assert_eq!(copy_count(&tree), 3, "and nothing extra was written");
}

/// s2: what reaches the model IS the scaled copy. Byte for byte: the `image_url` data URL of every
/// request is compared with `checks::base64` of the file on disk and against the stored frame, so the
/// two are told apart and a regression to sending native-size frames fails.
#[test]
fn p_machine_describeframewidth_s2_the_bytes_sent_are_the_scaled_copies_not_the_stored_ones() {
    let fake = Fake::start(BY_SIZE.to_string());
    // Five frames at four a call: two asks, and the second batch is a single frame.
    let scene = Scene::new("s2-sent", FRAMES_PER_REQ + 1, &fake.url);
    let tree = scene.tree();
    assert!(
        !tree.llm_frames_dir(LANE).exists(),
        "the stage must create .llmframes/ itself"
    );
    let mut log = Vec::new();
    let ran = walk(&scene, &mut |line| log.push(line.to_string())).expect("the stage describes the row");
    assert_eq!(
        ran, 2,
        "the stage answers how many CHUNKS it walked (spec F1.7 S7: \"event log complete (N chunks)\"),"
    );
    assert!(
        log.iter().any(|line| line.contains("event log complete (2 chunks")),
        "two chunks at four frames a call for five frames: {log:?}"
    );
    assert!(
        tree.llm_frames_dir(LANE).exists(),
        "the live describe run created the .llmframes/ folder: {log:?}"
    );
    let asks = fake.waited(2);
    assert_eq!(asks.len(), 2, "five frames at four a call is two calls: {log:?}");
    let sent: Vec<String> = asks
        .iter()
        .flat_map(|ask| {
            let value: serde_json::Value = serde_json::from_str(&ask.body).expect("a JSON body");
            let parts = value["messages"][1]["content"]
                .as_array()
                .expect("the user parts")
                .to_vec();
            parts
                .into_iter()
                .filter(|part| part["type"].as_str() == Some("image_url"))
                .filter_map(|part| {
                    part["image_url"]["url"]
                        .as_str()
                        .map(|url| url.rsplit(',').next().unwrap_or_default().to_string())
                })
                .collect::<Vec<String>>()
        })
        .collect();
    assert_eq!(
        sent.len(),
        FRAMES_PER_REQ + 1,
        "every frame of the walk travelled as a picture"
    );
    // The set of pictures sent must be exactly the set of scaled copies on disk — and must NOT be the
    // stored frames. Compare on the encoded bytes, which is what the model was handed.
    let taken = seconds(FRAMES_PER_REQ + 1);
    let copies: Vec<String> = taken
        .iter()
        .map(|at| naivepost::checks::base64(&std::fs::read(copy_path(&tree, *at)).expect("the scaled copy")))
        .collect();
    let stored: Vec<String> = taken
        .iter()
        .map(|at| {
            naivepost::checks::base64(
                &std::fs::read(tree.frames_dir(LANE).join(format!("{}.jpg", frame_name(*at))))
                    .expect("the stored frame"),
            )
        })
        .collect();
    for url in &sent {
        assert!(
            copies.iter().any(|copy| copy == url),
            "a picture was sent that is not one of the scaled copies: it must be a native-size frame"
        );
        assert!(
            !stored.iter().any(|frame| frame == url),
            "the stored native-size frame was sent instead of its scaled copy"
        );
    }
    assert_eq!(
        sent.len(),
        copies.len(),
        "the frames sent and the frames scaled are the same count"
    );
}

/// s3: the row's three artifacts, together and after one live walk: the scaled copies beside
/// `events.tsv` (one row per frame asked) and `state.txt` (the state the model set).
#[test]
fn p_machine_describeframewidth_s3_the_run_writes_events_tsv_and_state_next_to_the_frames_it_sent() {
    let fake = Fake::start(BY_SIZE.to_string());
    let scene = Scene::new("s3-files", FRAMES_PER_REQ, &fake.url);
    let tree = scene.tree();
    let mut log = Vec::new();
    let ran = walk(&scene, &mut |line| log.push(line.to_string())).expect("the stage describes the row");
    assert_eq!(ran, 1);
    assert!(fake.waited(1).len() == 1, "the row was asked once");
    let taken = seconds(FRAMES_PER_REQ);
    assert_eq!(
        copy_count(&tree),
        taken.len(),
        "one scaled copy per frame the run asked about"
    );
    for at in &taken {
        assert_eq!(
            width_of(&copy_path(&tree, *at)),
            describe::FRAME_WIDTH,
            "and each copy is at the catalogued width"
        );
    }
    let rows = describe::read_events(&tree, LANE);
    assert_eq!(
        rows.len(),
        taken.len(),
        "events.tsv holds one row per frame asked: {rows:?}"
    );
    let state = std::fs::read_to_string(tree.describe_state(LANE)).expect("state.txt was written");
    assert!(
        state.contains("mid-lecture"),
        "state.txt carries the state the model set: {state:?}"
    );
    assert!(
        tree.describe_dir(LANE).join(".llmframes").is_dir(),
        "the three artifacts of the row are siblings under describe/{LANE}/"
    );
}
