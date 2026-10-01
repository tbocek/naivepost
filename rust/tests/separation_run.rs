//! §02-services.md **§1 The four servers**, the voice-separation half: a row's ✂ tick becomes a real
//! `POST /v1/tasks/run` to audio.cpp, and the two stems that come back become the two files
//! [`naivepost::separate`] names.
//!
//! Item id: `sec_02_services_1_the_four_servers`. Before this file's work nothing in the program asked
//! audio.cpp to split anything: `separate::separate` and the task door had no caller outside their own
//! tests. These tests drive the run path — the planned row and the walk's dial branch — against a
//! loopback audio.cpp that behaves like the real one, pointing the Settings box at it exactly as §1's
//! "re-read per request" seam allows, so what is proven is the program's request and the program's
//! bytes, not a substitute.
//!
//! **s1** covers the plan and the round trip; **s4** the refusal, which is where §1's install hint
//! reaches a log.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::run::{self, Subprocesses};
use naivepost::run_stages;
use naivepost::separate_run;
use naivepost::settings::{self, Conf, Paths};
use naivepost::services;

/// What the fake took: the request line (`POST /v1/tasks/run HTTP/1.1` — one string, so a test matches
/// the request rather than two of its halves), the `Host:` it came to (which is how a recorded request
/// is traced back to the one fake that answered it), and the body it carried.
#[derive(Debug, Clone)]
struct Seen {
    line: String,
    host: String,
    /// The upload header §1 names the file with; empty on every other kind of request.
    filename: String,
    body: String,
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// What the fake serves as its model list: an ASR and nothing else, so a refusal about the shipped
/// separation id is refused because it is genuinely not there (§1's install hint then names the package).
const MODEL_LIST: &str = r#"{"data":[{"id":"nemotron-asr","family":"qwen3","task":"asr"}]}"#;

/// The env and the working folder belong to the process and cargo runs these tests as threads, so the
/// two walk tests take them one at a time and restore them before returning.
static ENV: Mutex<()> = Mutex::new(());

/// Point the process at one scene's folders: the settings folder through `XDG_CONFIG_HOME` and the
/// session by working directory, which is exactly how the app resolves both.
///
/// The variables get the ROOTS, not `scene.paths.config_dir`: [`naivepost::settings::paths_from`] appends
/// the `naivepost` folder itself, so handing it `…/cfg/naivepost` would look in `…/cfg/naivepost/naivepost`
/// and quietly read a default conf — an empty ffmpeg box and the loopback address, which is the wrong
/// server and the container's real ffprobe.
fn enter(scene: &Scene) {
    std::env::set_var("XDG_CONFIG_HOME", scene.root.join("cfg"));
    std::env::set_var("XDG_DATA_HOME", scene.root.join("data"));
    std::env::set_current_dir(&scene.root).expect("the scene is the working folder");
}

/// Put back what [`enter`] moved, so the next test starts from the real environment.
fn leave() {
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
    std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
}

/// A session folder (`cwd/session.naivepost`) plus the `Paths` a test's conf lives under, and the
/// fake's recorded requests. `SEEN` is per test binary, so every assertion here filters by the fake's
/// own port — a port can be recycled by another fake, but never while its own listener is alive.
struct Scene {
    root: PathBuf,
    paths: Paths,
    url: String,
    /// The scene's own ffmpeg stub, which is also where its ffprobe lives (§1: ffprobe comes from the
    /// same folder as the configured ffmpeg).
    tools: PathBuf,
    /// Requests this scene's fake answered, newest last.
    seen: std::sync::Arc<Mutex<Vec<Seen>>>,
}

impl Scene {
    /// One scenario per test BINARY, the shape the wire harnesses use: the session is resolved from the
    /// working folder and the settings folder from the environment, and both belong to the process, so
    /// a test lays them out once, holds the lock, and restores them before returning.
    fn new(answer: &'static str, status: u16) -> Self {
        let root = std::env::temp_dir().join(format!(
            "np-sep-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the temp folder");
        let session = naivepost::startup::session_dir(&root);
        // The lane comes from the source name §F1.3 stores it under, and the split reads THAT file's
        // decoded audio, so the two have to agree or the row honestly reports having nothing to split.
        // §F1.3 S1's decoded audio is what the split reads; ffprobe is stubbed below, so its bytes are
        // only ever counted, never decoded.
        std::fs::create_dir_all(session.join(format!("prepare/inputs/{LANE}"))).expect("the input folder");
        std::fs::write(session.join(format!("prepare/inputs/{LANE}/voice16k.wav")), b"pcm-bytes")
            .expect("the decoded audio");

        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let (url, seen) = fake_audio(answer, status);
        let tools = stubs(std::process::id().to_string().as_str());
        // The conf names both the endpoint and the tool box: §1 says ffprobe comes from the same folder
        // as a configured ffmpeg, and these stubs let the split be proven without ffmpeg installed.
        let mut conf = Conf::default();
        conf.audio_server = url.clone();
        conf.ffmpeg = tools.join("ffmpeg").to_string_lossy().into_owned();
        settings::save(&paths, &conf).expect("the conf is written");
        Scene { root, paths, url, tools, seen }
    }

    fn project(&self, sources: Vec<Source>) {
        naivepost::project::save(&Project { sources, ..Project::default() }, &naivepost::startup::session_dir(&self.root))
            .expect("the project file");
    }

    /// The session the scene laid out: the same folder the walk resolves, because the test chdir'd to
    /// `root` before anything read it.
    fn tree(&self) -> Tree {
        Tree::new(naivepost::startup::session_dir(&self.root)).expect("the session tree")
    }

    /// This fake's own requests, newest last — read out of the list this fake owns, not a shared one.
    fn mine(&self) -> Vec<Seen> {
        self.seen.lock().expect("the record lock").clone()
    }

    /// Wait until this fake has answered `count` requests. The recording happens on the connection's
    /// own thread, so a reply can return before its entry is in the list — the same reason
    /// `tests/audio_task_dial.rs` polls instead of sleeping a fixed time.
    fn wait_until(&self, count: usize) -> Vec<Seen> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let seen = self.mine();
            if seen.len() >= count || std::time::Instant::now() > deadline {
                return seen;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// One folder holding both stub tools, unique per test so parallel tests never share a log.
///
/// `ffprobe` answers a duration; `ffmpeg` answers a `volumedetect` mean on stderr (where ffmpeg
/// prints it) and does nothing for the scene pass, which only needs its command to succeed.
fn stubs(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-sep-tools-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the stub folder");
    for (name, script) in [
        ("ffprobe", "#!/bin/sh\necho \"12.5\"\n"),
        (
            "ffmpeg",
            "#!/bin/sh\nfor last in \"$@\"; do :; done\ncase \"$last\" in\n  -) echo \"avg_volume: -18.0 dB\" >&2 ;;\nesac\n",
        ),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, script).expect("the stub is written");
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("the stub runs");
    }
    dir
}

/// A fake audio.cpp: every `POST /v1/tasks/run` gets `answer` at `status`; every other path answers
/// 404, except a stem path, which answers the bytes named in it (the shape §1 implies: the server
/// keeps what it was given inside its own folders).
fn fake_audio(answer: &'static str, status: u16) -> (String, std::sync::Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();
    let host = format!("127.0.0.1:{port}");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let recorder = recorder.clone();
            std::thread::spawn(move || {
                if let Some(reply) = serve(stream, answer, status) {
                    if let Ok(mut row) = recorder.lock() {
                        row.push(reply);
                    }
                }
            });
        }
    });
    (format!("http://{host}"), seen)
}

fn serve(mut stream: TcpStream, answer: &'static str, status: u16) -> Option<Seen> {
    let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let request_line = line.trim_end().to_string();
    let mut length = 0usize;
    let mut host_seen = String::new();
    let mut filename = String::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            length = header.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
        }
        if lower.starts_with("host:") {
            host_seen = header.split(':').nth(1).unwrap_or("").trim().to_string();
        }
        if lower.starts_with("x-audiocpp-filename:") {
            filename = header.split(':').nth(1).unwrap_or("").trim().to_string();
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
    // The order matters: the two management endpoints are matched before the stem reads, because a
    // stem path lives under `/uploads/` like an upload's answer does. Each answers what §1 says it
    // must: the upload names where it put the file, and the model list is what a missing-model refusal
    // reads before it can say what the server does serve.
    let (code, payload) = if path.contains("/v1/tasks/run") {
        (status, answer.to_string())
    } else if path.contains("/v1/ui/upload") {
        (200, "{\"path\":\"/uploads/lecture.wav\"}".to_string())
    } else if path.contains("/v1/models") {
        (200, MODEL_LIST.to_string())
    } else if path.ends_with("voice.wav") {
        (200, "VOICE-BYTES".to_string())
    } else if path.contains("/uploads/") {
        (200, "REST-BYTES".to_string())
    } else {
        (404, "{\"error\":\"no such thing\"}".to_string())
    };
    let framed = format!(
        "HTTP/1.1 {code} Whatever\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(Seen {
        line: request_line,
        host: host_seen,
        filename,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// The run's session folder: the working folder's own `session.naivepost`, which is where a ▶ run's
/// session sits for the app as it is walked today (`run_stages::dial_task` resolves it the same way).
/// The test that drives the walk from its own temp folder puts one there and chdir's to it; the app
/// launches with the project as its working folder, so the two agree.
fn session_folder() -> PathBuf {
    naivepost::startup::session_dir(&std::env::current_dir().expect("a working folder"))
}

/// The `source` a video row carries, in the shape §F1.3 stores it.
fn source(path: &str, sepvoice: bool) -> Source {
    Source { path: path.to_string(), footage: false, narrator: 1, sepvoice, tracks: vec![] }
}

/// The lane of the scene's recording: §F1.3's stem rule, which is what names the decoded audio.
const LANE: &str = "lecture";

/// Drive the walk with only the separation stage: the session's `ffmpeg` box points at the scene's
/// stub, but the frames pass reads a file that is not there, and §Failure-is-specific says the honest
/// answer is to name what it could not read. The split's own rules are what these two tests are for.
fn only_separation(stages: Vec<run_stages::Stage>) -> Vec<run_stages::Stage> {
    stages
        .into_iter()
        .filter(|stage| stage.name == "separation")
        .collect()
}

/// The plan carries a separation row for a ticked recording and nothing for an unticked one, and the
/// row is a dial (audio.cpp is asked, nothing is spawned).
#[test]
fn sec_02_services_1_the_four_servers_s1_a_ticked_row_plans_a_separation_dial() {
    let mut split = Project::default();
    split.sources.push(source("sources/lecture.mkv", true));
    let stages = run_stages::plan(run::Step::Prepare, &split);
    let names: Vec<&str> = stages.iter().map(|stage| stage.name).collect();
    assert!(names.contains(&"separation"), "the tick asked for it: {names:?}");
    let stage = stages.iter().find(|stage| stage.name == "separation").expect("the stage");
    assert_eq!(stage.tasks.len(), 1, "one row, one task");
    assert_eq!(stage.tasks[0].program, run_stages::AUDIO_TASK, "it is a dial, not a spawn");
    assert_eq!(stage.tasks[0].args[0], separate_run::SEPARATE_MARK, "the row says which dial");
    assert_eq!(stage.tasks[0].args[1], "sources/lecture.mkv", "and which recording");
    assert_eq!(stage.tasks[0].args[2], "0", "and where that row is");

    let mut plain = Project::default();
    plain.sources.push(source("sources/lecture.mkv", false));
    let none = run_stages::plan(run::Step::Prepare, &plain);
    assert!(
        !none.iter().any(|stage| stage.name == "separation"),
        "nothing ticked, so nothing is asked: {:?}",
        none.iter().map(|stage| stage.name).collect::<Vec<_>>()
    );
}

/// The round trip: the recording goes up, the task run names the shipped separation model, and BOTH
/// stems come back as the two files `separate` names — the bytes are on disk, not just promised.
#[test]
fn sec_02_services_1_the_four_servers_s1_the_separation_asks_audio_cpp_and_writes_both_halves() {
    let scene = Scene::new("{\"voice\":\"/uploads/lecture/voice.wav\",\"rest\":\"/uploads/lecture/rest.wav\"}", 200);
    let _guard = ENV.lock().expect("the env lock");
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    enter(&scene);
    scene.project(vec![source("sources/lecture.mkv", true)]);

    let mut queue = naivepost::runqueue::Queue::new();
    let mut procs = Subprocesses::default();
    let mut log: Vec<String> = Vec::new();
    let stages = run_stages::plan(
        run::Step::Prepare,
        &naivepost::project::load(&session_folder()).expect("the project"),
    );
    // Only the split's own stage: the frames pass belongs to another item and its ffmpeg would need
    // the source file itself. `tools` keeps the probe for its own folder, not a shared one.
    let stages = only_separation(stages);
    let drive = run_stages::drive_walk(&stages, &mut queue, || false, || false, |line| {
        log.push(line.to_string());
    }, &mut procs);
    leave();
    drop(_guard);
    assert!(
        matches!(drive, run_stages::Drive::Done { .. }),
        "the walk finished: {drive:?} / log {log:?}"
    );

    let tree = scene.tree();
    let voice = tree.split_voice(LANE);
    // `separate` gives the no-voice half the SOURCE's own container, and this row is a video (§F1.2 S5),
    // so the halves are `.wav` and `.mkv` — which is also how a person finds them again.
    let rest = tree.split_novoice(LANE, "mkv");
    assert_eq!(std::fs::read(&voice).ok().as_deref(), Some("VOICE-BYTES".as_bytes()), "{voice:?}");
    assert_eq!(std::fs::read(&rest).ok().as_deref(), Some("REST-BYTES".as_bytes()), "{rest:?}");
    // S7: the row became two, and the wish is cleared so the next ▶ does not split a voice off a voice.
    let after = naivepost::project::load(&naivepost::startup::session_dir(&scene.root)).expect("the project");
    assert_eq!(after.sources.len(), 2, "the session now holds both halves");
    assert!(after.sources.iter().all(|row| !row.sepvoice), "neither half asks again");

    // The fake's task reply is fixed, so the stems it names get read from the fake by that name.
    let mine = scene.wait_until(4);
    // The upload's `X-Audiocpp-Filename` is how §1 names the file; the task envelope says where it went.
    let upload = mine
        .iter()
        .find(|seen| seen.line.contains("/v1/ui/upload"))
        .expect("the recording went up before anything was asked about it");
    assert!(
        upload.filename.contains("lecture"),
        "the upload named the recording it carried: {:?}",
        upload
    );
    let runs: Vec<&Seen> = mine
        .iter()
        .filter(|seen| seen.line.contains("/v1/tasks/run"))
        .collect();
    assert_eq!(runs.len(), 1, "one task run for one recording: {runs:?}");
    assert!(runs[0].line.contains("POST"), "a task run is a POST: {}", runs[0].line);
    // The envelope §5 confirms: the shipped separation id, and a server path rather than a local one.
    let sent: serde_json::Value = serde_json::from_str(&runs[0].body).expect("a JSON envelope");
    assert_eq!(sent["model"], services::SEP_MODEL, "the separation model §1 ships");
    assert!(
        sent["request"]["audio"]
            .as_str()
            .is_some_and(|audio| audio.starts_with('/')),
        "the task names a server path, not a local one: {sent}"
    );
    assert!(
        log.iter().any(|line| line.contains("split into lecture.split-novoice.mkv")),
        "the run said what the split produced: {log:?}"
    );
    // §6 names every exchange's step, so a separation is findable in `requests.tsv` by what it was.
    let ledger = std::fs::read_to_string(tree.requests_tsv()).expect("the request log");
    assert!(
        ledger
            .lines()
            .any(|row| row.split('\t').nth(2) == Some("Separate")),
        "the split left its row in the log: {ledger}"
    );
    // And the request went to the address the settings box names, which is the point of §1's base URL.
    // The port is compared separately because ureq leaves it out of the authority when it is writing an
    // `HTTP://` address that already carries it, and this fake is one of those.
    let host = runs[0].host.as_str();
    let port = scene.url.rsplit(':').next().unwrap_or("");
    assert!(
        host.starts_with("127.0.0.1") && scene.url.contains(port) && !host.contains(port),
        "the split asked the loopback address the settings name: {:?} from {}",
        runs[0],
        scene.url
    );
}

/// A shipped default the server does not serve must reach the log with §1's install hint, because that
/// sentence is the only thing that tells a person what to run.
#[test]
fn sec_02_services_1_the_four_servers_s4_a_missing_separation_model_reaches_the_log_with_its_hint() {
    let scene = Scene::new("{\"error\":\"model not found\"}", 404);
    let _guard = ENV.lock().expect("the env lock");
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    enter(&scene);
    scene.project(vec![source("sources/lecture.mkv", true)]);

    let mut queue = naivepost::runqueue::Queue::new();
    let mut procs = Subprocesses::default();
    let mut log: Vec<String> = Vec::new();
    let stages = run_stages::plan(
        run::Step::Prepare,
        &naivepost::project::load(&naivepost::startup::session_dir(&scene.root)).expect("the project"),
    );
    let stages = only_separation(stages);
    let drive = run_stages::drive_walk(&stages, &mut queue, || false, || false, |line| {
        log.push(line.to_string());
    }, &mut procs);
    leave();
    drop(_guard);
    let error = match drive {
        run_stages::Drive::Failed { stage, error } => {
            assert_eq!(stage, "separation", "the separation stage is the one that failed");
            error
        }
        other => panic!("a 404 on the shipped model must fail the walk, got {other:?}"),
    };
    assert!(
        error.contains("the weights install with:") && error.contains(services::SEP_MODEL),
        "the reason carries §1's install hint and names the model: {error}"
    );
}
