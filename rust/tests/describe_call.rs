//! §02-services.md **§2 Model roles**, the "what is on screen" row: a footage source's frames go to
//! the LLM **with vision**, **thinking off**, **four frames a call**, and the answer is one EVENT per
//! frame plus a running STATE.
//!
//! Item id: `sec_02_services_2_model_roles_which_job_asks_which_server`. Before [`naivepost::describe_call`]
//! existed, `src/describe.rs` made no request of any kind: it held F1.7's rules and said so in its own
//! header ("the model call is not made here at all"), and nothing else made it — `grep -n
//! 'server_leg\|llm_request' src/describe.rs` was empty and stayed empty. These tests drive the stage
//! the Prepare walk reaches (`run_stages::plan` → the `VISION_TASK` row → `describe_call::run_task`)
//! against a loopback LLM that answers with the §3.2 tool calls, pointed at through the Settings box
//! exactly as §1's re-read-per-request seam allows. So what is proven is the program's request and the
//! program's two files, not a substitute's.
//!
//! **s5** covers the row: the four frames and the thinking flag on the wire, the EVENT/STATE pair on
//! disk, the cache that makes an unchanged chunk free, and the plan that keeps a call at four frames.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use naivepost::clock::frame_name;
use naivepost::describe::{self, FRAMES_PER_REQ};
use naivepost::settings::{self, Conf, Paths};
use naivepost::services;

/// The env and the working folder belong to the process, and cargo runs these tests as threads. Every walk
/// test takes them (see `Scene::enter`), one at a time, exactly as `tests/separation_run.rs` does.
static ENV: Mutex<()> = Mutex::new(());

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The §3.2 answer for a four-frame batch: one `record_event` per frame and a `set_state`, which is
/// what the row says the job answers with. Held as one string because the fake replies to every ask
/// with it — the point is that the program's own tool surface turns it into the two files.
const FOUR_EVENTS: &str = r#"{"choices":[{"message":{"content":"","tool_calls":[
 {"function":{"name":"record_event","arguments":"{\"frame\":1,\"text\":\"a lecture slide appears\"}"}},
 {"function":{"name":"record_event","arguments":"{\"frame\":2,\"text\":\"the speaker writes a formula\"}"}},
 {"function":{"name":"record_event","arguments":"{\"frame\":3,\"text\":\"same\"}"}},
 {"function":{"name":"record_event","arguments":"{\"frame\":4,\"text\":\"the class looks up\"}"}},
 {"function":{"name":"set_state","arguments":"{\"text\":\"mid-lecture, board half full\"}"}},
 {"function":{"name":"finish","arguments":"{}"}}
]}}]}"#;


/// The §3.2 answer for a batch of ANY size: a `record_event` for every frame of it, then a
/// `set_state`, then `finish`. One builder rather than one const per chunk size because the last
/// chunk of nine frames at four a call is a batch of ONE, and an answer written for four frames would
/// report that chunk as incomplete for a reason that has nothing to do with what is being checked.
fn events_answer(frames: usize) -> String {
    let mut calls = String::new();
    for frame in 1..=frames {
        calls.push_str(&format!(
            "{{\"function\":{{\"name\":\"record_event\",\"arguments\":\"{{\\\"frame\\\":{frame},\\\"text\\\":\\\"frame {frame} of the take\\\"}}\"}}}},"
        ));
    }
    calls.push_str(
        "{\"function\":{\"name\":\"set_state\",\"arguments\":\"{\\\"text\\\":\\\"mid-lecture, board half full\\\"}\"}},",
    );
    calls.push_str("{\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}");
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{calls}]}}}}]}}")
}


/// The reply marker that means "answer each request with exactly the frames it carried"; see `serve`.
const BY_SIZE: &str = "@by-size";

/// What the fake answers with, and how many chat asks it has taken.
struct Fake {
    url: String,
    asks: std::sync::Arc<Mutex<Asks>>,
    /// The reply it gives, read at ask time so a test can answer the size of the batch it was actually
    /// sent — which is what `events_answer` is for.
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
    /// A fake LLM that answers every `POST /v1/chat/completions` with `answer`.
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
            answer: held,
        }
    }

    /// Reply sizes: when set, each ask is answered with a `record_event` for exactly the frames THAT ask
    /// carried (counted off the request's own picture parts), which is the only way a chunk-size test can
    /// prove the batch the stage chose. Unset, the fake answers with whatever it was started with.
    fn answer_batch_sizes(&self) {
        *self.answer.lock().expect("the reply lock") = BY_SIZE.to_string();
    }

    /// The chat requests this fake has recorded. Recording happens on the connection's own thread, so
    /// a reply can return before its entry is in the list; callers wait for the count they expect, the
    /// same way `tests/separation_run.rs` does.
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
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
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
    // The reply is chosen from the request, because one fake answers several chunk sizes: `BY_SIZE` means
    // "one record_event per picture THIS request carried", which is the only way a chunk-size test can
    // prove the batch the stage chose without the answer giving away the frame count first.
    let text = match answer {
        BY_SIZE => events_answer(String::from_utf8_lossy(&body).matches("\"image_url\"").count()),
        fixed => fixed.to_string(),
    };
    let payload = text.as_str();
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

/// What the scene hands the stage as its conf, and what `leg_conf` returns: the fake's address, or the
/// dead address the one "no server" scene plants for itself.
const UNUSED: &str = "http://unused.invalid";

/// The describe interval the scenes space their frames on: `Project::default()`'s own, so the project
/// file a scene writes and the frames folder it plants cannot disagree about the spacing (see `Scene::new`).
const SCENE_INTERVAL: f64 = 1.0;

/// One footage source, its frames and scaled copies on disk, and its own settings folder pointing at
/// its own fake LLM.
///
/// The scene is passed to the doors explicitly (`tree`/`paths`), but the WALK still needs the process's
/// env -- the settings folder through `XDG_CONFIG_HOME` and the session as the working folder -- because
/// that is how the app resolves both, so every walk enters the scene and takes the ENV lock (`walk`
/// below). cargo runs these tests as threads of one process, so two walks in flight would otherwise
/// move each other's cwd and settings folder mid-request, and `ask`'s re-read of the conf (§1's
/// re-read-per-request seam) would dial a server that is not this scene's.
struct Scene {
    root: PathBuf,
    paths: Paths,
    /// The fake's address, written to this scene's OWN settings folder and to no other. Written by
    /// `Scene::new` alone -- never by another thread's env -- so `leg_conf` below answers what THIS scene
    /// points at however the process's `XDG_CONFIG_HOME` is moving underneath it.
    server: String,
}

impl Scene {
    fn new(label: &str, frames: usize, llm_url: &str) -> Scene {
        // Frames on the describe INTERVAL (P.project.frameInterval, 1.0 s here), not on the extraction
        // grid: `describe::pick_frames` walks one frame every interval inside a scene, so a folder spaced
        // any finer than that is asked for pictures that were never written, and `describe_scale::ensure`
        // names the missing one and fails the stage before a request goes out -- which reads to a test as
        // "the fake was asked 0 times". `is_every_frame` wants the same spacing or it refuses the source
        // outright as an every-frame extraction. Both rules are about the interval, so this is the one
        // spacing that lets the stage reach the wire.
        let interval = SCENE_INTERVAL;
        let root = std::env::temp_dir().join(format!(
            "np-desc-{label}-{}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let session = root.join("session.naivepost");
        std::fs::create_dir_all(&session).expect("the session folder");
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        // One footage row, described in the project file the walk loads.
        let project = naivepost::project::Project {
            sources: vec![naivepost::project::Source {
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
        // Same number the frames were spaced at, said to the project file `interval` is read back from, so
        // the frames on disk and the interval the stage reads cannot drift apart.
        assert!(
            (project.interval - interval).abs() < 1e-9,
            "the scene spaces its frames on its own interval: {}",
            project.interval
        );
        let lane = "lecture".to_string();
        // F1.6's output: the grid on the frames' own clock, plus F1.7's scaled copies beside them.
        let frames_dir = naivepost::layout::Tree::new(&session)
            .expect("the tree")
            .frames_dir(&lane);
        let llm_dir = naivepost::layout::Tree::new(&session)
            .expect("the tree")
            .llm_frames_dir(&lane);
        std::fs::create_dir_all(&frames_dir).expect("the frames folder");
        std::fs::create_dir_all(&llm_dir).expect("the scaled folder");
        for index in 0..frames {
            // `frames_on_disk` reads the seconds out of the names with `clock::read_frame` — whose
            // epoch is the one `clock::frame_name` writes from — and rebases them on the earliest
            // picture that exists, so a name at the epoch reads as 0.0 s and the rest follow on the
            // grid. The `.llmframes/` copy has to carry the bare clock name because
            // `describe::scaled_name` looks it up by exactly that.
            let at = index as f64 * interval;
            let name = format!("{}.jpg", frame_name(at + epoch_seconds()));
            std::fs::write(frames_dir.join(&name), b"jpeg").expect("the frame");
            std::fs::write(
                llm_dir.join(format!("{}.jpg", frame_name(at))),
                format!("scaled-{index}"),
            )
            .expect("the scaled copy");
        }
        settings::save(&paths, &conf_at(llm_url)).expect("the conf");
        Scene { root, paths, server: llm_url.to_string() }
    }

    /// Take the process into this scene for the walk's own resolution: the settings folder through
    /// `XDG_CONFIG_HOME` (the ROOT — `settings::paths_from` appends the `naivepost` folder itself) and
    /// the session by cwd. Every walk test takes it now, not just the `plan` one: `ask` re-reads the conf
    /// from `settings::from_environment()` mid-request, so a second thread's `leave()` between the
    /// stage's own read and the leg's sends the ask to `llm_default()` and the fake is never asked.
    fn enter(&self) {
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
    }

    fn leave() {
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_DATA_HOME");
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    fn tree(&self) -> naivepost::layout::Tree {
        naivepost::layout::Tree::new(&self.session()).expect("the session is a project")
    }

    /// Describe this scene's footage source the way the stage does, but with the scene's own tree and
    /// settings folder: [`naivepost::describe_call::stage_chunk`] is the door `run_task` opens after it
    /// resolves them from the environment, and it is the same door.
    ///
    /// The process's env stands as this scene's for the whole walk and no other thread's: every test body
    /// that calls this holds the ENV lock across it, and `enter()` says why the walk needs the env at all.
    /// The address the walk's leg will dial, read back from the settings file rather than handed over by
    /// the test, so the conf on disk and the conf the leg dials are one fact and a fake that has not been
    /// written yet reads as "no conf to dial" instead of a silent ask to the shipped default port. A scene
    /// whose folder was rewritten after it was made answers the rewrite: that is what the leg will dial.
    fn server(&self) -> Result<String, String> {
        let on_disk = settings::read(&self.paths).map_err(|why| format!("{}: {why}", self.server))?;
        Ok(services::server_url(&on_disk.server).unwrap_or_else(|| self.server.clone()))
    }

    /// The fake's address the stage will reach, for the `conf` argument it is handed: an unwritten settings
    /// folder is this binary's own bug, so it panics here rather than letting a walk dial nothing.
    fn dial_to(&self) -> String {
        self.server().expect("this scene's fake is in its own settings folder")
    }

    /// The conf the LEG will actually dial to, read back through the same door `ask` uses (`ask` throws
    /// away the `conf` argument it is handed and re-reads the settings folder at send time, §1's
    /// re-read-per-request seam) and checked against this scene's own fake. The ENV lock is held across
    /// every call of this, so the folder it finds is this scene's; the assert is what still catches a
    /// mistake rather than letting a walk dial `services::llm_default()`, answer `Ok(0)` and leave a test
    /// asserting a bare `[]`.
    fn leg_conf(&self) -> Conf {
        let mut conf = conf_at(&self.dial_to());
        if let Some(paths) = settings::from_environment() {
            if let Ok(read) = settings::read(&paths) {
                conf = read;
            }
        }
        assert_eq!(
            services::server_url(&conf.server).as_deref().unwrap_or_default(),
            self.dial_to(),
            "the leg would dial a different server than this scene's fake -- the env is not this scene's"
        );
        conf
    }


    fn describe(&self, conf: &Conf, log: &mut impl FnMut(&str)) -> Result<usize, String> {
        naivepost::describe_call::stage_chunk(&self.tree(), &self.paths, conf, "lecture", Some(0), log)
    }

    /// The frames this scene's footage row holds, as the stage will read them.
    /// The frames this scene's footage row holds, as the stage will read them.
    /// THIS scene's frames, counted the way the stage counts them: straight from the scene's own tree,
    /// with no cwd or settings folder involved, so an answer to "how many does this row have?" cannot come
    /// back as another thread's scene (0 of them reads exactly like a folder that was never entered).
    fn frame_count(&self) -> usize {
        naivepost::describe::frames_on_disk(&self.tree().frames_dir("lecture")).len()
    }

    fn session(&self) -> PathBuf {
        self.root.join("session.naivepost")
    }

    fn events(&self) -> String {
        std::fs::read_to_string(
            naivepost::layout::Tree::new(&self.session())
                .expect("the tree")
                .events_tsv("lecture"),
        )
        .unwrap_or_default()
    }

    fn state(&self) -> String {
        std::fs::read_to_string(
            naivepost::layout::Tree::new(&self.session())
                .expect("the tree")
                .describe_state("lecture"),
        )
        .unwrap_or_default()
    }

    /// The one path that goes through the walk's own planning: the stage the Prepare step builds from
    /// this session's project, and its rows run the way `drive_walk` runs them -- resolving the tree and
    /// the settings folder from the environment, which is why the caller holds the ENV lock and stands in
    /// this scene.
    fn planned_walk(&self) -> Result<(usize, Vec<String>), String> {
        let project = naivepost::project::load(&self.session()).expect("the session");
        let stages = naivepost::run_stages::plan(naivepost::run::Step::Prepare, &project);
        let stage = stages
            .iter()
            .find(|stage| stage.name == "describe")
            .ok_or("the walk planned no describe stage")?;
        let mut log = Vec::new();
        let mut ran = 0usize;
        for task in &stage.tasks {
            naivepost::describe_call::run_task(task, &mut |line| log.push(line.to_string()))?;
            ran += 1;
        }
        Ok((ran, log))
    }
}

/// The walk's row for the footage source is a plan row, not a spawn: `plan(Prepare)` must carry the
/// describe stage after the ingest stages, its rows must name the vision door, and that door must run
/// end to end -- the stage's own `run_task` resolving its tree and settings folder from the environment
/// and reaching this fake with one ask per footage source.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s5_what_is_on_screen_plans_a_vision_row() {
    let fake = Fake::start(FOUR_EVENTS.to_string());
    let scene = Scene::new("plan", 4, &fake.url);
    let _guard = env_lock();
    scene.enter();
    let project = naivepost::project::load(&scene.session()).expect("the session");
    let stages = naivepost::run_stages::plan(naivepost::run::Step::Prepare, &project);
    let names: Vec<&str> = stages.iter().map(|stage| stage.name).collect();
    assert!(
        names.contains(&"describe"),
        "the walk plans a describe stage: {names:?}"
    );
    let describe = stages
        .iter()
        .find(|stage| stage.name == "describe")
        .expect("the describe stage");
    assert_eq!(describe.tasks.len(), 1, "one row per footage source");
    assert_eq!(
        describe.tasks[0].program,
        naivepost::run_stages::VISION_TASK,
        "the row is a chat dial, named by its own program"
    );
    // And the row runs: `run_task` takes no tree and no paths, it resolves both from this process's env
    // and from the working folder -- the half of the walk that only the planned path exercises.
    let (ran, log) = scene.planned_walk().expect("the planned row runs");
    Scene::leave();
    drop(_guard);
    assert_eq!(ran, 1, "one row walked: {log:?}");
    let asks = fake.waited(1);
    assert_eq!(asks.len(), 1, "the planned row asked once: {log:?}");
}

/// The epoch `clock::frame_name` names its frames from, found the only way that cannot drift: name
/// second 0 and second 1 and read back how far apart `clock::read_frame` says they are.
fn epoch_seconds() -> f64 {
    let zero = frame_name(0.0);
    let one = frame_name(1.0);
    let (a, b) = (
        naivepost::clock::read_frame(&zero).expect("the name just written"),
        naivepost::clock::read_frame(&one).expect("the name just written"),
    );
    // One second apart by construction, so the difference is the epoch the names encode.
    a - (b - a).round()
}

/// The conf a test hands the stage directly: the shipped shape with the fake's address and a model id,
/// written to the scene's own settings folder (the file the stage re-reads per request, §1).
fn conf_at(url: &str) -> Conf {
    let mut conf = Conf::default();
    conf.server = url.to_string();
    conf.model = "qwen-vl".to_string();
    conf
}

/// The row's shape on the wire: FOUR frames per request (P.machine.describeFramesPerReq), each sent
/// as a picture, with thinking forced off, and the §3.2 tool surface offered so the answer can be a
/// `record_event` per frame.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s5_what_is_on_screen_asks_with_four_frames_and_thinking_off(
) {
    let fake = Fake::start(FOUR_EVENTS.to_string());
    let scene = Scene::new("wire", 4, &fake.url);
    let mut log = Vec::new();
    let ran = walk(&scene, &mut |line| log.push(line.to_string())).expect("the stage describes the row");
    let asks = fake.waited(1);
    assert_eq!(ran, 1, "one footage row walked");
    assert_eq!(asks.len(), 1, "four frames fit one call: {asks:?}");
    assert!(
        asks[0].path.contains("/v1/chat/completions"),
        "the vision job is a chat call: {}",
        asks[0].path
    );
    let sent: serde_json::Value = serde_json::from_str(&asks[0].body).expect("a JSON body");
    let parts = sent["messages"][0]["content"].as_array().expect("the user parts");
    let images: Vec<&serde_json::Value> = parts
        .iter()
        .filter(|part| part["type"].as_str() == Some("image_url"))
        .collect();
    assert_eq!(
        images.len(),
        FRAMES_PER_REQ,
        "P.machine.describeFramesPerReq frames go in one call"
    );
    assert!(
        images
            .iter()
            .all(|image| image["image_url"]["url"]
                .as_str()
                .is_some_and(|url| url.starts_with("data:image/jpeg;base64,"))),
        "the frames travel as pictures in the request: {sent}"
    );
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
        .expect("the §3.2 tools are offered")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    for name in ["record_event", "set_state", "finish"] {
        assert!(offered.contains(&name), "{name} is offered to the describe job: {offered:?}");
    }
    // The message says what §F1.7 S3 says it says, so the frames and the STATE arrive labelled.
    let text = parts[0]["text"].as_str().expect("the text part");
    assert!(
        text.contains("STATE so far:") && text.contains("FRAME 1 of 4") && text.contains("NEW SCENE"),
        "the request stamps its frames and carries the running state: {text}"
    );
    assert!(
        text.contains("a lecture on linear algebra"),
        "the User Context heads the message: {text}"
    );
    assert!(
        log.iter().any(|line| line.contains("event log complete")),
        "the run said what it finished: {log:?}"
    );
}

/// A poison is another test's panic, not this one's problem: the env gets restored and the walk goes
/// ahead, because "the scene's env is intact" is a fact this function re-establishes itself by entering,
/// and reporting someone else's failure a second time here only buries the first one.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    match ENV.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// One walk, in this binary's one-serialised world: take the ENV lock, put the process into the scene,
/// describe, then put the process back. `describe_call`'s scenes resolve the settings folder and the
/// session from the environment exactly as the app does, and cargo runs tests as threads of one process,
/// so two walks in flight hand each other's `XDG_CONFIG_HOME` and cwd -- the stage's own `conf` argument
/// survives that, `ask`'s later re-read does not, and the walk then dials `services::llm_default()` and
/// records zero asks. `tests/separation_run.rs` pays the same cost the same way.
fn walk(scene: &Scene, log: &mut impl FnMut(&str)) -> Result<usize, String> {
    let _guard = env_lock();
    scene.enter();
    let outcome = scene.describe(&scene.leg_conf(), log);
    Scene::leave();
    outcome
}

/// Serialise on the env, enter the scene, run TWO walks back to back, and put the process back. The
/// resume test needs both walks inside one holding: a second `walk()` would be a second lock, and a
/// lock this binary's own thread cannot re-enter.
fn walk_twice(
    scene: &Scene,
    first: &mut impl FnMut(&str),
    second: &mut impl FnMut(&str),
) -> (Result<usize, String>, Result<usize, String>) {
    let _guard = env_lock();
    scene.enter();
    let conf = scene.leg_conf();
    let one = scene.describe(&conf, first);
    let two = scene.describe(&conf, second);
    Scene::leave();
    (one, two)
}

/// "One EVENT per frame + a running STATE" is what lands on disk: `events.tsv` gains a row per frame
/// and `state.txt` the state the model set. A chunk with a frame left unanswered never reaches these
/// two files at all, so a full file is itself the proof the answer was complete.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s5_what_is_on_screen_writes_one_event_per_frame_and_the_state(
) {
    let fake = Fake::start(FOUR_EVENTS.to_string());
    let scene = Scene::new("files", 4, &fake.url);
    let mut log = Vec::new();
    walk(&scene, &mut |line| log.push(line.to_string())).expect("the stage describes the row");
    fake.waited(1);
    let events = scene.events();
    let rows: Vec<&str> = events.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(rows.len(), FRAMES_PER_REQ, "one row per frame: {events}");
    assert!(
        rows[0].contains("a lecture slide appears") && rows[1].contains("the speaker writes a formula"),
        "the model's words are what the rows hold: {events}"
    );
    assert!(
        rows[2].contains("same"),
        "a frame the model called unchanged is written, not dropped: {events}"
    );
    // §6's row is `start\tend\ttext`, so the first two fields are numbers.
    for row in &rows {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields.len(), 3, "§6's three fields: {row}");
        assert!(
            fields[0].parse::<f64>().is_ok() && fields[1].parse::<f64>().is_ok(),
            "both times are numbers: {row}"
        );
    }
    assert_eq!(
        scene.state().trim(),
        "mid-lecture, board half full",
        "the running STATE is what the model set"
    );
}

/// §01's resume rule for this stage: a chunk already answered costs no request. The second walk asks
/// nothing and says so — the cached count is what distinguishes its line from the first run's.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s5_what_is_on_screen_costs_nothing_when_nothing_changed(
) {
    let fake = Fake::start(FOUR_EVENTS.to_string());
    let scene = Scene::new("cache", 4, &fake.url);
    let mut first = Vec::new();
    let mut second = Vec::new();
    // Both walks inside one holding: the second is the resume, and it must see the first's cache and
    // events, which means the same scene, the same env, and no other thread's folder in between.
    let (one, two) = walk_twice(
        &scene,
        &mut |line| first.push(line.to_string()),
        &mut |line| second.push(line.to_string()),
    );
    one.expect("the first walk describes the row");
    let after_first = fake.waited(1).len();
    // The fake would answer again if it were asked; the second pass must not ask it at all.
    two.expect("the second pass describes the row");
    assert_eq!(after_first, 1, "the first pass made one ask");
    let asks = fake.asks.lock().expect("the record lock").bodies.len();
    assert_eq!(asks, 1, "the second pass made no ask at all: {asks}");
    assert!(
        second
            .iter()
            .any(|line| line.contains("1 answered from the cache") || line.contains("from the cache")),
        "the pass said the answer came from the cache: {second:?}"
    );
    assert_eq!(
        scene.events().lines().filter(|line| !line.is_empty()).count(),
        FRAMES_PER_REQ,
        "a cached resume does not duplicate the rows"
    );
}

/// The fourth-frames-a-call bound is a plan, not a hope: nine frames are three chunks of ≤4, and the
/// walk asks once per chunk — which is also what makes the resume arithmetic (§F1.7 S2) meaningful.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s5_what_is_on_screen_batches_four_frames_a_call() {
    // Nine frames at four a call is chunks of 4, 4 and 1, and the third has to be answered too: the
    // fake answers each request with a `record_event` for exactly the pictures THAT request carried (see
    // `BY_SIZE`), which is more than any one batch needs only if the stage chose smaller batches than the
    // bound allows -- so the size question stays with the request, as it must.
    let scene = Scene::new("batch", 9, UNUSED);
    let fake = Fake::start(String::new());
    settings::save(&scene.paths, &conf_at(&fake.url)).expect("the conf points at the fake");
    assert_eq!(scene.frame_count(), 9, "the scene planted nine frames");
    let project = naivepost::project::load(&scene.session()).expect("the session");
    let frames: Vec<f64> = (0..9).map(|index| index as f64 * project.interval).collect();
    let chunks = describe::chunk_plan(&frames, FRAMES_PER_REQ);
    // Each ask's answer covers only the frames that ask carried: the fake reads its own reply when the
    // request arrives, so the size that reaches it is the size the stage chose.
    fake.answer_batch_sizes();
    walk(&scene, &mut |_line| {}).expect("the stage describes the row");
    let asks = fake.waited(3);
    assert_eq!(chunks.len(), 3, "nine frames at four a call: {chunks:?}");
    assert_eq!(asks.len(), 3, "one call per chunk: {asks:?}");
    let third: serde_json::Value = serde_json::from_str(&asks[2].body).expect("a JSON body");
    let parts = third["messages"][0]["content"].as_array().expect("the user parts");
    let images = parts
        .iter()
        .filter(|part| part["type"].as_str() == Some("image_url"))
        .count();
    assert_eq!(images, 1, "the last chunk carries the frames that are left: {images}");
}

/// A chunk the model would not answer whole is not written half: `finish` reporting a frame with no
/// EVENT is put in front of the model on the next try, and only a complete answer reaches disk. Named
/// for the row it proves, because "one EVENT per frame" is the row's own wording.
#[test]
fn sec_02_services_2_model_roles_which_job_asks_which_server_s5_what_is_on_screen_refuses_a_chunk_missing_an_event() {
    let fake = Fake::start(
        r#"{"choices":[{"message":{"content":"","tool_calls":[
          {"function":{"name":"record_event","arguments":"{\"frame\":1,\"text\":\"a slide\"}"}},
          {"function":{"name":"finish","arguments":"{}"}}
        ]}}]}"#
        .to_string(),
    );
    let scene = Scene::new("missing", 4, &fake.url);
    let walked = walk(&scene, &mut |_line| {});
    let asks = fake.waited(1).len();
    assert!(
        walked.is_err(),
        "a chunk with three frames unanswered is a failure, not a quiet short file"
    );
    assert!(
        asks >= 1,
        "the model was asked, and told what was missing on each try: {asks}"
    );
    assert!(
        !Path::new(&scene.session()).join("prepare/describe/lecture/events.tsv").exists(),
        "an incomplete answer writes no event log"
    );
}

/// `speech_around` answers with every recording's words over the span, not the capped two-per-side extract
/// the request's own brief carries -- so a word from the OTHER recording that the brief omitted reaches the
/// model when it asks.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s3_speech_around_answers_with_every_recording() {
    let frames = 1;
    let fake = Fake::start(events_asked(frames, "speech_around"));
    let scene = Scene::new("speechread", frames, &fake.url);
    // The scene's project names one source; the tool walks EVERY source the project lists, which is what
    // the request's own per-side-capped speech block cannot carry. So the second recording has to be in the
    // project file for its words to be reachable.
    let project = naivepost::project::Project {
        sources: vec![
            naivepost::project::Source {
                path: "project:sources/lecture.mkv".to_string(),
                footage: true,
                narrator: 0,
                sepvoice: false,
                tracks: vec![],
            },
            naivepost::project::Source {
                path: "project:sources/commentary.wav".to_string(),
                footage: false,
                narrator: 1,
                sepvoice: false,
                tracks: vec![],
            },
        ],
        context: "a lecture on linear algebra".to_string(),
        ..Default::default()
    };
    naivepost::project::save(&project, &scene.session()).expect("two sources in the project");
    naivepost::requests::write_words(
        &scene.tree(),
        "commentary",
        &naivepost::requests::WordsDoc {
            text: "eigenvalue".to_string(),
            words: vec![naivepost::requests::Word {
                word: "eigenvalue".to_string(),
                start_sample: naivepost::transcribe::SAMPLE_RATE / 2,
                end_sample: naivepost::transcribe::SAMPLE_RATE,
            }],
        },
    )
    .expect("the other recording's words");
    let mut log = Vec::new();
    walk(&scene, &mut |line| log.push(line.to_string())).expect("the chunk is described");
    fake.waited(1);
    let answered = answer_of(&log, "speech_around");
    let body: serde_json::Value = serde_json::from_str(&answered).expect("the read answered in JSON");
    let spoken = body["lines"].as_array().expect("the lines spoken in the span");
    assert!(
        spoken.iter().any(|line| line.as_str().is_some_and(|text| text.contains("eigenvalue"))),
        "the other recording's word came back: {answered}"
    );
}

/// The §3.2 answer for a batch of any size, with a READ in front of the `record_event`s: one round asks for
/// the readings and answers the batch, which is how a model that wants its events before it describes is
/// shaped. The span it asks about runs past the last frame so a word spoken over that frame is inside it.
fn events_asked(frames: usize, name: &str) -> String {
    let args = serde_json::json!({ "from": 0.0, "to": frames as f64 + 1.0 });
    let escaped = args.to_string().replace('"', "\\\"");
    let mut calls = format!("{{\"function\":{{\"name\":\"{name}\",\"arguments\":\"{escaped}\"}}}},");
    for frame in 1..=frames {
        calls.push_str(&format!(
            "{{\"function\":{{\"name\":\"record_event\",\"arguments\":\"{{\\\"frame\\\":{frame},\\\"text\\\":\\\"frame {frame} of the take\\\"}}\"}}}},"
        ));
    }
    calls.push_str(
        "{\"function\":{\"name\":\"set_state\",\"arguments\":\"{\\\"text\\\":\\\"board half full\\\"}\"}},",
    );
    calls.push_str("{\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}");
    format!("{{\"choices\":[{{\"message\":{{\"content\":\"\",\"tool_calls\":[{calls}]}}}}]}}")
}

/// The answer the leg gave to one read, taken from the stage's own log: `>>> <name> answered: <json>`.
fn answer_of(log: &[String], name: &str) -> String {
    let prefix = format!(">>> {name} answered: ");
    log.iter()
        .find(|line| line.starts_with(&prefix))
        .map(|line| line[prefix.len()..].to_string())
        .unwrap_or_else(|| panic!("{name} was never answered: {log:?}"))
}

/// §3.2's Reads row offers the vision job `get_events`, `get_frames`, `get_context` and `speech_around`, and
/// nothing used to answer them: a model that asked for the readings it was continuing from got silence, and
/// the retry then told it about frames it had never been given the chance to use. The leg answers a read in
/// the SAME round that asked, so the chunk is written in full and costs no extra ask.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s3_the_reads_describe_is_offered_are_answered() {
    let frames = 4;
    let fake = Fake::start(events_asked(frames, "get_events"));
    let scene = Scene::new("reads", frames, &fake.url);
    // Two readings already on the log, at 3 s and 4 s: inside the span the read asks about (0 to frames+1
    // seconds) but past every chunk START of this four-frame grid, so `starts_already_logged` still leaves
    // the chunk to ask -- a row at 0 s would skip it and no read would ever be called.
    std::fs::write(
        scene.tree().events_tsv("lecture"),
        "3.00\t11.00\tslide one is up\n4.00\t12.00\tslide two is up\n",
    )
    .expect("the event log");
    let mut log = Vec::new();
    walk(&scene, &mut |line| log.push(line.to_string())).expect("the chunk is described");
    let asks = fake.waited(1);
    assert_eq!(asks.len(), 1, "a read answered in the round that asked costs no extra ask: {asks:?}");

    // Not a coincidence: the reads were offered on the ask, so the model had a reason to call one.
    let offered: Vec<String> = serde_json::from_str::<serde_json::Value>(&asks[0].body)
        .expect("a JSON body")["tools"]
        .as_array()
        .expect("tools are offered")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_string))
        .collect();
    for name in ["get_events", "get_frames", "get_context", "speech_around"] {
        assert!(offered.iter().any(|n| n == name), "{name} is offered to describe: {offered:?}");
    }

    // The read came back with the chunk's own readings, and not as an error.
    let answered = answer_of(&log, "get_events");
    assert!(!answered.contains("\"error\""), "a well-formed read is not refused: {answered}");
    let body: serde_json::Value = serde_json::from_str(&answered).expect("the read answered in JSON");
    let rows = body["events"].as_array().expect("the EVENT lines in the range");
    assert_eq!(rows.len(), 2, "both readings over 0-5 s: {answered}");
    assert!(rows[0].as_str().unwrap().contains("slide one is up"), "{answered}");
    assert!(rows[1].as_str().unwrap().contains("slide two is up"), "{answered}");

    // And the answer still lands: every frame of the chunk got its row beside the readings that stood.
    let events = scene.events();
    let written: Vec<&str> = events.lines().filter(|row| !row.is_empty()).collect();
    assert!(events.contains("frame 1 of the take"), "{events}");
    assert!(events.contains("frame 4 of the take"), "{events}");
    assert_eq!(written.len(), frames + 2, "four new rows plus the two that stood: {events}");
}
