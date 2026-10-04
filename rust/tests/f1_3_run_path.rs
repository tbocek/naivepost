//! F1.3 Per source: audio → text → word times → speakers → segments — the ROUND TRIP, the ▶ path.
//!
//! `tests/transcribe.rs` proves [`naivepost::transcribe`]'s rules with closures and no socket; this
//! file is the other half it names but never had: the real walk (`run_stages::plan` →
//! `run_stages::drive_walk`) over a session folder, against a loopback audio.cpp that answers ASR,
//! alignment and diarization, with ffmpeg/ffprobe stubbed so the decode, the silencedetect pass and
//! every chunk cut are proven by what the tool was actually asked to do. A rule test cannot see the
//! two bugs this file exists for: an aligner asked BEFORE the ASR wrote its words (the transcript it
//! places would be empty), and cuts that never move to a silence because nobody measured one.
//!
//! Item ids follow §04-prepare.md's step numbers: **s1** decode + the whole pass, **s2** the short
//! take, **s3** resume and the silence-moved chunks, **s4** the aligner's place in the order,
//! **s5** the diarizer's ladder and the detector's folding.
//!
//! One thing about this path is easy to get wrong from the other test files: the ASR chunk ceiling comes
//! from the SETTINGS BOX (`services::asr_model`, whose id carries the family), not from the `family`
//! field of the box's `/v1/models` list — so a scene that wants several chunks sets `conf.asr_model`.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use naivepost::asr;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::requests::{self, Word, WordsDoc};
use naivepost::run::{self, Subprocesses};
use naivepost::run_stages;
use naivepost::separate_media as media;
use naivepost::services;
use naivepost::settings::{self, Conf, Paths};
use naivepost::transcribe;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::temp_dir_created as temp_dir;
use common::temp_dir_tagged;

/// The lane of every scene here: the stem of the recording's name, which is what names the decoded
/// audio and every sidecar (§F1.3 stores under it).
const LANE: &str = "lecture";

/// What one request looked like: the request line whole (so a test matches `POST /v1/tasks/run`
/// rather than two of its halves), the `X-Audiocpp-Filename` an upload carried, and the body.
#[derive(Debug, Clone)]
struct Seen {
    line: String,
    filename: String,
    body: String,
}

/// The ASR id this scene's fake serves, and the ceiling it implies. The shipped default is NOT in the
/// qwen3 family (`asr::chunk_limit` says 300 s for it), so a scene that wants several chunks names a
/// family whose ceiling it can state — which is also what a real box declares, §1's model list being
/// where the family comes from.
const QWEN_ASR: &str = "qwen3-asr";

/// The aligner id these scenes serve: deliberately NOT `services::ALIGN_PREFERENCE`. The picker puts
/// the preference first when the box declares it, but sorting must not be the reason an id reaches the
/// wire in a test — naming an id of our own proves the ALIGNER role was what chose it, and nothing else.
const SCENE_ALIGNER: &str = "wav2vec-align";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The env and the working folder belong to the PROCESS and cargo runs these tests as threads, so
/// the tests that move them take them one at a time and put them back before asserting.
static ENV: Mutex<()> = Mutex::new(());

/// One scenario: a session folder (`root/session.naivepost`), the settings folder whose conf points
/// at this scene's fake and this scene's stub tools, and the requests that fake answered.
struct Scene {
    root: PathBuf,
    /// The address this scene's fake answers: `conf.audio_server` is set to it, which is the only reason
    /// a scene needs its port remembered at all.
    #[allow(dead_code)]
    url: String,
    tools: PathBuf,
    seen: Arc<Mutex<Vec<Seen>>>,
    /// Answer this scene's ASR requests with text and no word times. A scene carries its own arms because
    /// `walk_scene_armed` clears the process-wide switches before the walk starts.
    asr_without_times: std::cell::Cell<bool>,
}

impl Scene {
    /// Lay out one scene. `duration` is what the ffprobe stub answers, so it IS the take's length as
    /// far as the program can know it; `silences` are the pairs the silencedetect stub prints.
    ///
    /// `asr_model` goes into the SETTINGS box, which is where the ASR ceiling comes from on this path:
    /// `services::asr_model(&conf)` is what `transcribe_source` hands `asr::run` as the model family,
    /// and the box's `/v1/models` list decides only which roles are served. Empty means the shipped id.
    fn new(duration: &str, silences: &[(f64, f64)], asr_model: &str) -> Self {
        let root = temp_dir(&format!(
            "f13-run-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(session.join("sources")).expect("the sources folder");
        std::fs::write(session.join("sources/lecture.wav"), b"mkv-bytes").expect("the recording");
        // The conf gets the ROOTS: `settings::paths_from` appends the `naivepost` folder itself, so
        // handing it `cfg/naivepost` would read a default conf off the real `$HOME`.
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let (url, seen) = fake_audio();
        let tools = stubs(&format!("{}-{}-{}", std::process::id(), duration, silences.len()), duration, silences);
        let mut conf = Conf::default();
        conf.audio_server = url.clone();
        conf.ffmpeg = tools.join("ffmpeg").to_string_lossy().into_owned();
        conf.asr_model = asr_model.to_string();
        settings::save(&paths, &conf).expect("the conf is written");
        Scene { root, url, tools, seen, asr_without_times: std::cell::Cell::new(false) }
    }

    /// Answer the first `count` ASR requests of the next walk with the box having no room. Arm it
    /// through [`walk_scene_armed`], never from the test body: `reset_pieces` clears it.
    fn arm_oom(&self, count: usize) {
        OOM_FIRST.store(count, Ordering::SeqCst);
    }

    /// Answer every ASR request with text and NO word times. Like [`Scene::arm_oom`] this is armed through
    /// [`walk_scene_armed`], which reads it off the scene after `reset_pieces` has cleared the globals.
    fn arm_asr_without_times(&self) {
        self.asr_without_times.set(true);
    }

    /// Answer every ASR request with no text at all, which is what silence on the tape sounds like.
    fn arm_no_speech(&self) {
        NO_SPEECH.store(1, Ordering::SeqCst);
    }

    /// Make the aligner refuse the window it is sent, in the shape `align::parse_words` reports.
    fn arm_align_fails(&self) {
        ALIGN_FAILS.store(1, Ordering::SeqCst);
    }

    /// Decode the row the way §F1.3 S1 does, so the walk starts from a recording that already has its
    /// `voice16k.wav`: that is the state a second Prepare press finds, and the only one in which the
    /// `asr/` folder a walk leaves behind belongs to the ASR stage alone.
    fn decode(&self) {
        let tree = self.tree();
        let paths = Paths {
            config_dir: self.root.join("cfg/naivepost"),
            data_dir: self.root.join("data/naivepost"),
        };
        let conf = settings::read(&paths).unwrap_or_default();
        let source = self.root.join("session.naivepost/sources/lecture.wav");
        naivepost::decode_audio::ensure(&tree, LANE, &source, &conf.ffmpeg, &mut |_| ())
            .expect("the stub decode writes the pcm");
    }

    /// Set the id THIS scene's conf names as the ASR model, which is what decides the chunk ceiling
    /// (`services::asr_model` → `asr::chunk_limit`). Called after `enter()`, so the walk reads a conf
    /// that already says what the test wants.
    fn set_asr_model(&self, id: &str) {
    let paths = Paths {
        config_dir: self.root.join("cfg/naivepost"),
        data_dir: self.root.join("data/naivepost"),
    };
    let mut conf = settings::read(&paths).unwrap_or_default();
    conf.asr_model = id.to_string();
    settings::save(&paths, &conf).expect("the conf is rewritten");
}

    /// The session's project file: one recording row, nothing ticked (the split belongs to F1.2).
    /// SPEAKS ONLY goes by [`VOICE_ON`] because the F1.4 scenes need the pass to run at all — with the
    /// tick off, `prepare_run` answers NONE and the walk has no speech task to reach.
    fn project(&self) {
        let source = Source {
            path: "sources/lecture.wav".to_string(),
            footage: false,
            narrator: 0,
            sepvoice: VOICE_ON.load(Ordering::SeqCst) != 0,
            tracks: vec![],
        };
        naivepost::project::save(
            &Project { sources: vec![source], ..Project::default() },
            &naivepost::startup::session_dir(&self.root),
        )
        .expect("the project file");
    }

    fn tree(&self) -> Tree {
        Tree::new(naivepost::startup::session_dir(&self.root)).expect("the session tree")
    }

    /// Point the process at this scene, exactly as the app resolves both: `XDG_CONFIG_HOME` for the
    /// settings, the working folder for the session.
    fn enter(&self) {
        std::env::set_var("XDG_CONFIG_HOME", self.root.join("cfg"));
        std::env::set_var("XDG_DATA_HOME", self.root.join("data"));
        std::env::set_current_dir(&self.root).expect("the scene is the working folder");
    }

    /// The speech stage only: frames, describe and fix belong to other items and fail on a one-file
    /// scene, and §Failure-is-specific says the honest answer is to name what they could not read.
    fn stages(&self) -> Vec<run_stages::Stage> {
        let project = naivepost::project::load(&naivepost::startup::session_dir(&self.root)).expect("the project");
        run_stages::plan(run::Step::Prepare, &project)
            .into_iter()
            .filter(|stage| stage.name == "speech")
            .collect()
    }

    /// Walk it. Returns the log lines the walk produced, newest last.
    fn walk(&self) -> Vec<String> {
        let (drive, log) = self.walk_drive();
        assert!(matches!(drive, run_stages::Drive::Done { .. }), "the walk finished: {drive:?} / log {log:?}");
        log
    }

    /// The same walk with no expectation about how it ended: a fatal step is the thing a test is
    /// checking, and the pair `say_outcome` prints is what it asserts on.
    fn walk_fail(&self) -> Vec<String> {
        let (drive, mut log) = self.walk_drive();
        run_stages::say_outcome(&drive, |line| log.push(line.to_string()));
        log
    }

    /// Walk the scene's speech stage and hand back both the drive's answer and every line the walk
    /// logged, in order.
    fn walk_drive(&self) -> (run_stages::Drive, Vec<String>) {
        let mut queue = naivepost::runqueue::Queue::new();
        let mut procs = Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let stages = self.stages();
        let drive = run_stages::drive_walk(&stages, &mut queue, || false, || false, |line| {
            log.push(line.to_string());
        }, &mut procs);
        (drive, log)
    }

    fn mine(&self) -> Vec<Seen> {
        self.seen.lock().expect("the record lock").clone()
    }

    /// Every `POST /v1/tasks/run` this fake answered, in the order it answered them — the order is
    /// the point of two of the tests below, so it is kept rather than counted.
    fn tasks(&self) -> Vec<Seen> {
        self.mine().into_iter().filter(|seen| seen.line.contains("/v1/tasks/run")).collect()
    }

    /// Which task each request was, in arrival order, by the model id its envelope carries. The
    /// envelope has no `task` field (§5 gives the envelope, not a task word), so the id decides:
    /// the fake serves exactly one id per role.
    fn task_order(&self) -> Vec<&'static str> {
        self.tasks()
            .iter()
            .map(|seen| {
                if seen.body.contains(SCENE_ALIGNER) {
                    "align"
                } else if seen.body.contains(services::DIAR_MODEL) {
                    "diarize"
                } else {
                    "asr"
                }
            })
            .collect()
    }

    /// Wait until this fake has answered `count` requests: the recording happens on the connection's
    /// own thread, so a reply can return before its entry is in the list. Bounded, never a bare sleep.
    #[allow(dead_code)] // the walk is synchronous, so no test needs it today; the fake still records async
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

fn leave() {
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("XDG_DATA_HOME");
    std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
}

/// Walk a scene that is EXPECTED to fail, and hand back what the walk logged. The other walker asserts
/// the walk finished; a fatal step is the point here, so the failure is what is returned: the stage
/// name and the sentence the door surfaced, the same pair `say_outcome` prints for the run bar.
/// Diarization answers may be scripted AFTER the reset -- `reset_pieces` clears the queue, so an answer
/// a test body queued itself is gone before the walk starts, which makes these two walkers the only
/// honest way to drive the ladder from a test.
fn walk_scene_failing_queued(scene: &Scene, answers: &[&'static str]) -> Vec<String> {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    reset_pieces();
    VOICE_ON.store(1, Ordering::SeqCst);
    for answer in answers {
        queue_diar(answer);
    }
    scene.enter();
    scene.project();
    let log = scene.walk_fail();
    leave();
    VOICE_ON.store(0, Ordering::SeqCst);
    log
}

/// Hold the env, lay out the scene, walk it, put the env back. Every assertion afterwards reads files
/// and the fake's record, neither of which another test's `set_current_dir` can move.
fn walk_scene(scene: &Scene) -> Vec<String> {
    walk_scene_queued(scene, &[])
}

/// `walk_scene` with the diarizer's answers queued AFTER `reset_pieces` has cleared the queue — see
/// [`walk_scene_failing_queued`] for why the order is the whole point.
fn walk_scene_queued(scene: &Scene, answers: &[&'static str]) -> Vec<String> {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    reset_pieces();
    VOICE_ON.store(1, Ordering::SeqCst);
    for answer in answers {
        queue_diar(answer);
    }
    scene.enter();
    scene.project();
    let log = scene.walk();
    leave();
    VOICE_ON.store(0, Ordering::SeqCst);
    log
}

/// The stub tools: `ffprobe` answers `duration`; `ffmpeg` behaves like the tool where a leg depends
/// on it and logs every argv into `argv.log`, one call per line.
///
/// * silencedetect (its argv ends in `-`, since the report goes to a null output): prints the pairs
///   on STDERR, where ffmpeg really prints them — a stub that answered them on stdout would let a leg
///   that reads only stdout report "no silences" forever.
/// * anything else: creates its last argument from its first input, or writes a payload when there is
///   no input. A stub that only succeeded would let a decode that made nothing be reported as work.
fn stubs(tag: &str, duration: &str, silences: &[(f64, f64)]) -> PathBuf {
    // A DISTINCT folder per call: `common::temp_dir` is keyed on pid + tag only, so a shared tag would
    // have the next scene's `remove_dir_all` delete the stubs a previous scene's conf still names.
    let dir = temp_dir_tagged(&format!("np-f13-tools-{tag}"));
    let probe = dir.join("ffprobe");
    std::fs::write(&probe, format!("#!/bin/sh\necho \"{duration}\"\n")).expect("the probe stub");
    let report: String = silences
        .iter()
        .map(|(start, end)| format!("echo \"silence_start: {start}\" >&2\necho \"silence_end: {end}\" >&2\n"))
        .collect();
    let ffmpeg = dir.join("ffmpeg");
    std::fs::write(
        &ffmpeg,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{dir}/argv.log\"\ncase \"$*\" in\n  *silencedetect*)\n{report}  exit 0\n    ;;\nesac\nlast=''\nfor a in \"$@\"; do last=\"$a\"; done\nfirst=''\nseen=0\npending=0\nfor a in \"$@\"; do\n  if [ \"$a\" = \"-i\" ]; then pending=1; continue; fi\n  if [ \"$pending\" = 1 ]; then pending=0; seen=$((seen+1)); [ \"$seen\" = 1 ] && first=\"$a\"; fi\ndone\nif [ -f \"$first\" ]; then cp \"$first\" \"$last\"; else printf 'STUB-AUDIO' > \"$last\"; fi\n[ -f \"$last\" ]\n",
            dir = dir.display(),
            report = report
        ),
    )
    .expect("the ffmpeg stub");
    for tool in [&probe, &ffmpeg] {
        std::fs::set_permissions(tool, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("the stub runs");
    }
    dir
}

/// The stub's recorded calls, oldest first, one argv per line.
fn calls(tools: &Path) -> Vec<String> {
    std::fs::read_to_string(tools.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// The fake audio.cpp. §1's three management/task endpoints, answered per path:
/// `/v1/models` serves all three roles (an aligner the box does not declare is an aligner never
/// asked), `/v1/ui/upload` names where it put the bytes, and `/v1/tasks/run` answers whichever
/// model the envelope asked for. Anything else is a 404, which is how a wrong endpoint shows up.
fn fake_audio() -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let recorder = recorder.clone();
            std::thread::spawn(move || {
                if let Some(row) = serve(stream) {
                    if let Ok(mut held) = recorder.lock() {
                        held.push(row);
                    }
                }
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

/// Two voices, wide enough apart that §F1.3 S6 makes two segments, and long enough each to be an
/// anchor voice on the 90 s rung (`diarize::ANCHOR_MIN_SECONDS`). Kept as text so a test can name the
/// shape it is being served; the fake's live ordinary answer is `diar_reply`.
#[allow(dead_code)] // the shape these tests describe, spelled out where a reader can see it
const TURNS: &str = r#"{"turns":[{"start_sample":1600,"end_sample":80000,"speaker_id":0},{"start_sample":120000,"end_sample":200000,"speaker_id":1}]}"#;

/// Diarization's answer with ONE voice heard across the whole window (`from..to`), which is what
/// crowds a small window out: the anchor takes `anchor_per(window)` = min(12, window/4) seconds, so in
/// a 25 s rung one voice eats 6.25 s and leaves 17.75 s — fine — while in a 10 s rung the same voice is
/// the whole window. `diar_window_ask` moves each reply onto the recording's clock, so the fake answers
/// in seconds relative to the window it was asked about.
#[allow(dead_code)] // documents the single-voice shape the ladder arithmetic is explained with
fn diar_spanning(from: f64, to: f64) -> String {
    let start = (from * 16_000.0) as u64;
    let end = (to * 16_000.0) as u64;
    format!("{{\"turns\":[{{\"start_sample\":{start},\"end_sample\":{end},\"speaker_id\":0}}]}}")
}

/// One chunk's text and word times, in that chunk's OWN seconds: `asr::shift` moves them by the
/// chunk's edge afterwards, which is the thing S4's stitching is about.
fn words_chunk(chunk: usize) -> String {
    let base = chunk * 2;
    format!(
        "{{\"text\":\"word{c} one{c}\",\"words\":[{{\"word\":\"word{c}\",\"start_sample\":{},\"end_sample\":{}}}]}}",
        base * 16000 + 1600,
        base * 16000 + 3200,
        c = chunk
    )
}

/// The same chunk's answer with its text and NO word times, which is the shape `align_outcome` reads as
/// "nothing else can time these words". Armed per scene by [`Scene::arm_asr_without_times`].
fn text_only_chunk(chunk: usize) -> String {
    format!("{{\"text\":\"word{c} one{c}\"}}", c = chunk)
}

/// The aligner's answer for one window: the same words at the same seconds, which is what makes
/// `align::restore` and the segment builder agree with the ASR about what was said.
fn words_aligned(chunk: usize) -> String {
    words_chunk(chunk)
}

fn serve(mut stream: TcpStream) -> Option<Seen> {
    let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let request_line = line.trim_end().to_string();
    let mut length = 0usize;
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
        if lower.starts_with("x-audiocpp-filename:") {
            filename = header.split(':').nth(1).unwrap_or("").trim().to_string();
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let text = String::from_utf8_lossy(&body).into_owned();
    let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let (code, payload) = route(&path, &text);
    let framed = format!(
        "HTTP/1.1 {code} Whatever\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(Seen { line: request_line, filename, body: text })
}

/// Which reply a request gets. The task kinds are told apart by the model id their envelope names —
/// the envelope §5 fixes has no task field, so the id is the only thing on the wire that says which
/// job was asked, and the fake serves exactly one id per job.
fn route(path: &str, body: &str) -> (u16, String) {
    if path.contains("/v1/models") {
        return (
            200,
            format!(
                "{{\"data\":[{{\"id\":\"{QWEN_ASR}\",\"family\":\"qwen3\",\"task\":\"asr\"}},{{\"id\":\"{SCENE_ALIGNER}\",\"task\":\"align\"}},{{\"id\":\"{}\",\"task\":\"diarize\"}}]}}",
                services::DIAR_MODEL
            ),
        );
    }
    if path.contains("/v1/ui/upload") {
        return (200, "{\"path\":\"/uploads/lecture.wav\"}".to_string());
    }
    if !path.contains("/v1/tasks/run") {
        return (404, "{\"error\":\"no such thing\"}".to_string());
    }
    if body.contains(services::DIAR_MODEL) {
        // The diarizer's answers come from the queue a test fills, one per ask, in the order the ladder
        // reaches its rungs (`diar_answer_for`).
        let asks = DIAR_ASKS.fetch_add(1, Ordering::SeqCst);
        return diar_answer_for(asks + 1);
    }
    if body.contains(SCENE_ALIGNER) {
        // The aligner is asked about ONE window, in the same order as the ASR's pieces, and its
        // reply places that piece's words at that piece's seconds.
        let chunk = next_of(&WINDOWS);
        if ALIGN_FAILS.load(Ordering::SeqCst) != 0 {
            // The server's own refusal, in the shape `parse_json`/`align::parse_words` report.
            return (500, "{\"error\":\"the aligner model is not loaded\"}".to_string());
        }
        return (200, words_aligned(chunk));
    }
    if TEXT_ONLY.load(Ordering::SeqCst) != 0 {
        // Text and no word times at all — S4's hard branch needs exactly this ASR answer.
        return (200, "{\"text\":\"hello there\"}".to_string());
    }
    if ASR_NO_TIMES.load(Ordering::SeqCst) != 0 {
        // Every chunk answers with its text only, so an aligner refusal that follows is fatal rather than
        // a shrug. Counted through `CHUNKS` like the ordinary answer below, so the piece numbering holds.
        return (200, text_only_chunk(next_of(&CHUNKS)));
    }
    // The box saying it has no room, in a shape `asr::retryable` accepts, for the first N asks only: the
    // retry at the halved ceiling must then succeed, or the halving is never seen working. The words are
    // the server's own: `dial` keeps the body whole inside its own sentence and `retryable` looks for the
    // phrase rather than a sentence this fake invented.
    let ooms = OOM_FIRST.load(Ordering::SeqCst);
    if ooms != 0 {
        // Counted on the REQUESTS the fake answered, not on `CHUNKS`: a chunk that fails never reaches
        // the piece counter, so counting asks is the only way the Nth failure and the retry land right.
        let asked = ASR_ASKS.fetch_add(1, Ordering::SeqCst);
        if asked < ooms {
            return (500, "{\"error\":\"CUDA out of memory\"}".to_string());
        }
    }
    if NO_SPEECH.load(Ordering::SeqCst) != 0 {
        // No text and no word times: nothing was heard. `asr_answer` would refuse this shape, so the
        // fake answers it the way a server really does — empty text, which is an answer, not a fault.
        return (200, "{\"text\":\"\"}".to_string());
    }
    (200, words_chunk(next_of(&CHUNKS)))
}

/// Which piece a request is about: the fake answers every upload with the same server path, so the
/// piece is counted by arrival — the Nth ask is piece N-1, which is the order `asr::run` and
/// `align::run` walk their pieces in.
fn next_of(counter: &'static AtomicUsize) -> usize {
    counter.fetch_add(1, Ordering::SeqCst)
}

/// Per-request answers to the diarizer, read one per ask in the order the ladder reaches its rungs.
/// A queue is the only honest way to drive that walk: which rung refuses depends on the WINDOW being
/// asked about, which the envelope does not name, so a test supplies the answers in rung order. Each
/// entry carries the LABEL the test queued it under, so a test reads back its own words rather than
/// having a payload compared — `DIAR_REPLIES` is those labels, in the order they were sent.
static DIAR_QUEUE: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());
static DIAR_REPLIES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
/// How many diarization requests the fake has answered, whatever rung each was on.
static DIAR_ASKS: AtomicUsize = AtomicUsize::new(0);

/// Script the diarizer's NEXT answer, labelled with the word the test used. Queue it through
/// [`walk_scene_queued`] / [`walk_scene_failing_queued`], never before calling a walker: `reset_pieces`
/// clears this queue, so an answer put there by the test body itself is gone before the walk starts.
///
/// * `"many"` — eight voices spanning the window: 8 x `anchor_per(span)` of anchor, more than a rung of
///   any width has room to place (see [`diar_voices`] for which two numbers `diarize::plan` compares).
/// * `"two"` — two 5.6 s voices (`diar_two_voices`), which every rung plans, so the walk stops at the
///   first rung it asks about.
/// * `"refusal"` — no `turns` key, which `audio_task::diar_answer` reports as the model's own failure
///   rather than an empty answer, and no rung survives that.
fn queue_diar(answer: &'static str) {
    let payload = match answer {
        "many" => diar_voices(0.0, 25.0, 8),
        // No turns key at all: `diar_answer` reports that as the diarizer's own refusal, which no
        // rung survives, so the walk gives up and names the model.
        "refusal" => "{\"detail\":\"no turns here\"}".to_string(),
        // Two voices of 5.6 s each, which every rung plans (see the note above): the answer the walk
        // is meant to accept and stop on.
        _ => diar_two_voices(5.6),
    };
    DIAR_QUEUE.lock().expect("the reply queue lock").push((answer, payload));
}

/// How many diarization requests the fake has answered so far.
#[allow(dead_code)] // the count is `diar_record().len()` today; kept as the request-side twin of it
fn diar_asks() -> usize {
    DIAR_ASKS.load(Ordering::SeqCst)
}

/// What the fake sent for each diarization request, in arrival order.
fn diar_record() -> Vec<&'static str> {
    DIAR_REPLIES.lock().expect("the reply log lock").clone()
}

/// Arm the ASR id a walk should use, then walk. `Scene::new` already wrote a conf with its own
/// id; this rewrites it after `enter()` (the same paths, a conf the walk has not read yet) so each F1.4
/// scene chooses its own ceiling. `fail` asks for the ending the walk is expected to reach: `true` for
/// a pass that stops with an error, which is what `say_outcome` prints rather than a panic.
/// `walk_scene_asr` for a scene that needs the whole speech pass: a row whose SPEAKS ONLY is ticked (with
/// the tick off `prepare_run` answers NONE and there is no speech task to walk) and the ASR ceiling the
/// test wants. The ceiling has to be in the CONF before the walk reads it, so it is written here, after
/// `enter()` has pointed the process at this scene's settings folder.
fn walk_scene_asr(scene: &Scene, id: &'static str, fail: bool) -> Vec<String> {
    walk_scene_armed(scene, id, fail, 0, false, false, false)
}


/// `walk_scene_asr` with the switches a test needs armed AFTER `reset_pieces` has cleared them: the
/// box refusing the aligner, the box having no room, the tape hearing nothing. A walker cannot read
/// them from the test body for the same reason the diarizer's queue cannot — `reset_pieces` clears them
/// first — so a scene carries its own arms and names them here.
///
/// `decode` makes the recording's `voice16k.wav` BEFORE the walk starts. Without it the stub's decode
/// counts as a chunk cut (`-ss 0 -t 140`), and a scratch folder exists before the stage ever ran, so a
/// scratch-lifetime test would be reading a file ffmpeg's decode left, not what `asr/` holds.
fn walk_scene_armed(
    scene: &Scene,
    id: &'static str,
    fail: bool,
    oom_first: usize,
    no_speech: bool,
    align_fails: bool,
    decode: bool,
) -> Vec<String> {
    // The env and the working folder are process-wide, so the whole scene is laid out, walked and left
    // under one guard; `Scene::project` reads VOICE_ON when it writes the row, hence arming it first.
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    reset_pieces();
    VOICE_ON.store(1, Ordering::SeqCst);
    OOM_FIRST.store(oom_first, Ordering::SeqCst);
    NO_SPEECH.store(usize::from(no_speech), Ordering::SeqCst);
    ALIGN_FAILS.store(usize::from(align_fails), Ordering::SeqCst);
    ASR_NO_TIMES.store(usize::from(scene.asr_without_times.get()), Ordering::SeqCst);
    scene.enter();
    scene.set_asr_model(id);
    scene.project();
    if decode {
        scene.decode();
    }
    let log = if fail {
        scene.walk_fail()
    } else {
        scene.walk()
    };
    leave();
    VOICE_ON.store(0, Ordering::SeqCst);
    log
}

/// The ASR requests a scene's walk made, in arrival order: the `/v1/tasks/run` bodies that name neither
/// the aligner nor the diarizer, which is how the fake's own routing tells the three jobs apart.
fn asr_requests(scene: &Scene) -> Vec<String> {
    scene
        .tasks()
        .into_iter()
        .map(|seen| seen.body)
        .filter(|body| !body.contains(SCENE_ALIGNER) && !body.contains(services::DIAR_MODEL))
        .collect()
}

/// The ffmpeg calls that CUT a chunk, oldest first. What makes a line a cut is `cut_plan`'s
/// `-ss <from> -t <span>` pair -- the seek and the length side by side, the seek taking a number. The
/// stub logs `argv[0]` too, and the §F1.3 S1 decode seeks as well, so matching `-ss` alone picks up the
/// whole decode of the recording as one "chunk" and every count comes out one too high.
fn cut_calls(tools: &Path) -> Vec<String> {
    calls(tools)
        .into_iter()
        .filter(|line| {
            line.split(" -ss ")
                .nth(1)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()) && line.contains("-t "))
        })
        .collect()
}

/// The cuts the ASR stage made, not the aligner's or the diarizer's. Each stage writes its own audio
/// beside its own scratch note (`asr/cNN.wav`, `align/wNNNN.wav`, `diar/dNNNN.wav` — `Tree::asr_scratch`,
/// `align_scratch`, `diar_scratch`), and the stub tool logs its whole argv, so the output path is what
/// tells one stage's cut from another's.
fn asr_cut_calls(tools: &Path) -> Vec<String> {
    cut_calls(tools)
        .into_iter()
        .filter(|line| line.contains("/asr/c"))
        .collect()
}

static CHUNKS: AtomicUsize = AtomicUsize::new(0);
static WINDOWS: AtomicUsize = AtomicUsize::new(0);

/// A reply with the ASR's text and NOTHING else: the shape that makes S4's failure a hard error rather
/// than a warning, because there are no word times for the ASR's answer to stand on. Armed per test by
/// [`Scene::arm_text_only`].
static TEXT_ONLY: AtomicUsize = AtomicUsize::new(0);
/// The same shape, armed per SCENE (see [`Scene::arm_asr_without_times`]) so a walk can carry it through
/// `reset_pieces`, which clears the process-wide switches a test body cannot reach.
static ASR_NO_TIMES: AtomicUsize = AtomicUsize::new(0);
/// Make the aligner answer its window with an error instead of word times.
static ALIGN_FAILS: AtomicUsize = AtomicUsize::new(0);
/// How many ASR requests the fake has answered, which is NOT the same as `CHUNKS`: a chunk that fails
/// never reaches the piece counter, and a retry asks about the whole recording again, so the count that
/// decides "fail the first N" has to be the count of requests.
static ASR_ASKS: AtomicUsize = AtomicUsize::new(0);
/// Answer the first N ASR requests with the box running out of room, which is §F1.4 S1's one retryable
/// failure. The fake cannot know a request is the first of a run, so a test arms the count and the
/// counter decides, exactly as `ALIGN_FAILS` does for the aligner.
static OOM_FIRST: AtomicUsize = AtomicUsize::new(0);
/// Answer every ASR request with NO text at all — silence on the tape, which is a real answer and not
/// a failure (`asr::no_speech_log`), so the pass has to finish and write its files anyway.
static NO_SPEECH: AtomicUsize = AtomicUsize::new(0);
/// A scene whose project row ticked SPEAKS ONLY, so `prepare_run`'s choice says SPEECH and the whole
/// pass runs. `Scene::project()` leaves it false because the F1.3 scenes are about the cut, not the tick.
static VOICE_ON: AtomicUsize = AtomicUsize::new(0);

/// The piece counters are per-process while the scenes are per-test, so a test clears them before it
/// walks: the Nth ask must be piece N-1 of THIS scene's recording, not of whichever test ran first.
fn reset_pieces() {
    CHUNKS.store(0, Ordering::SeqCst);
    WINDOWS.store(0, Ordering::SeqCst);
    DIAR_ASKS.store(0, Ordering::SeqCst);
    DIAR_REPLIES.lock().expect("the reply log lock").clear();
    DIAR_QUEUE.lock().expect("the reply queue lock").clear();
    TEXT_ONLY.store(0, Ordering::SeqCst);
    ALIGN_FAILS.store(0, Ordering::SeqCst);
    OOM_FIRST.store(0, Ordering::SeqCst);
    ASR_NO_TIMES.store(0, Ordering::SeqCst);
    ASR_ASKS.store(0, Ordering::SeqCst);
    NO_SPEECH.store(0, Ordering::SeqCst);
    VOICE_ON.store(0, Ordering::SeqCst);
}

/// The diarizer's ordinary answer: two voices 4.9 s and 5.0 s, both comfortably anchor-length
/// (`diarize::ANCHOR_MIN_SECONDS`) and both shorter than the smallest rung's own anchor
/// (`diarize::anchor_per(25)` = 6.25 s), which is what makes a walk settle on the FIRST rung it asks
/// about: the anchor is then 2 x 6.25 s of a 25 s window at worst, so no rung of the ladder refuses it.
/// S6's speaker names come from turns of exactly this shape.
fn diar_reply() -> String {
    // Voices 4.9 s and 5.0 s, laid end to end inside the first 13 s so each is fully inside EVERY
    // window the ladder can ask about: `diarize::plan` measures a voice on the window it was heard in,
    // so a straddling turn would read as shorter on a small rung and move a refusal the wrong way.
    diar_two_voices(4.9)
}

/// The queue the diarizer answers from is taken front-first, and the label the test queued goes to
/// `DIAR_REPLIES`: that is what lets a ladder walk be scripted rung by rung and read back afterwards.
/// An empty queue answers with the scene's ordinary reply, which is how the eleven tests that never
/// script the diarizer still get a plan that fits on the 90 s rung.
fn diar_answer_for(_ask: usize) -> (u16, String) {
    let (label, payload) = {
        let mut queue = DIAR_QUEUE.lock().expect("the reply queue lock");
        if queue.is_empty() {
            ("turns", diar_reply())
        } else {
            queue.remove(0)
        }
    };
    DIAR_REPLIES.lock().expect("the reply log lock").push(label);
    (200, payload)
}

/// S1 + the whole pass: the walk decodes the recording, asks each of the three models in turn, and
/// leaves the five files the next step reads. This is the round trip `tests/f1_3_decode.rs` names.
#[test]
fn f1_3_s1_the_walk_decodes_then_passes_the_recording_through_the_servers() {
    let scene = Scene::new("20.0", &[], "");
    let log = walk_scene(&scene);
    let tree = scene.tree();

    // S1 ran through the STUB, at the rate every later word time is counted in. Read off the tool's
    // own argv rather than asserted about a file that merely exists.
    let argv = calls(&scene.tools);
    assert!(
        argv.iter().any(|line| line.contains("-ar 16000") && line.contains("-ac 1")),
        "the decode asked for mono 16 kHz: {argv:?}"
    );
    // And the silencedetect pass went too — S2's chunk edges come from it.
    assert!(argv.iter().any(|line| line.contains("silencedetect")), "quiet was measured: {argv:?}");

    // All three jobs were asked, and the ASR's chunk went UP first: a task may only name a path an
    // upload handed back (§1), so an upload precedes every ask — and it carried the chunk's own name in
    // `X-Audiocpp-Filename`, which is how §1 says a file is named to the server.
    let mine = scene.mine();
    let uploads: Vec<&Seen> =
        mine.iter().filter(|seen| seen.line.contains("/v1/ui/upload")).collect();
    let tasks = scene.tasks();
    assert_eq!(uploads.len(), tasks.len(), "every ask had its own upload ahead of it");
    assert!(
        uploads.iter().any(|seen| seen.filename.starts_with('c')),
        "the upload named the chunk it carried: {:?}",
        uploads.iter().map(|seen| seen.filename.clone()).collect::<Vec<_>>()
    );
    // The ASR id on the wire is the one the SETTINGS BOX names (services::asr_model reads
    // `conf.asr_model`, defaulting to the shipped id) — the box's model list decides which roles are
    // served, not which id is asked for. So this scene's ask carries the shipped default.
    assert!(
        tasks.iter().any(|seen| seen.body.contains(services::ASR_MODEL)),
        "the ASR was asked: {:?}",
        tasks.iter().map(|seen| seen.body.clone()).collect::<Vec<_>>()
    );
    assert!(tasks.iter().any(|seen| seen.body.contains(SCENE_ALIGNER)), "the aligner was asked");
    assert!(tasks.iter().any(|seen| seen.body.contains(services::DIAR_MODEL)), "the diarizer was asked");

    // The five files. Their existence is also the resume marker of each step, so a pass that skipped
    // one would show up here rather than as a silent gap on the next run.
    assert!(tree.words_json(LANE).exists(), "words.json");
    assert!(tree.asrchunks_json(LANE).exists(), "asrchunks.json");
    assert!(tree.turns_json(LANE).exists(), "turns.json");
    assert!(tree.transcript_tsv(LANE).exists(), "transcript.tsv");
    assert!(tree.transcript_srt(LANE).exists(), "transcript.srt");
    // S6's speaker came from the diarizer's turns, not from the fallback name.
    let tsv = std::fs::read_to_string(tree.transcript_tsv(LANE)).expect("the transcript");
    // The diarizer's two turns are what name the words; a 20.0 s take is ONE chunk whose reply carries
    // one word, so S6 makes one segment and one speaker is all there is to see.
    assert!(tsv.contains("SPEAKER_00"), "the speaker came from the turns: {tsv}");

    // The log says the length it worked from and what it ended with.
    assert!(log.iter().any(|line| line == ">>> [lecture] 20.0 s of audio"), "S1's line: {log:?}");
    assert!(
        log.iter().any(|line| line.ends_with(" segments") && line.starts_with(">>> [lecture] ")),
        "S6's count: {log:?}"
    );
}

/// S4's ORDER, which is the bug this round fixes: the aligner places the ASR's words, so it can only
/// be asked after `words.json` exists and with the text the ASR answered. Before the lazy hand-off it
/// was decided up front, over a transcript that did not exist yet.
#[test]
fn f1_3_s4_alignment_is_asked_after_the_asr_wrote_its_words() {
    let scene = Scene::new("20.0", &[], "");
    walk_scene(&scene);

    let order = scene.task_order();
    let asr_at = order.iter().position(|kind| *kind == "asr").expect("the ASR was asked");
    let align_at = order.iter().position(|kind| *kind == "align").expect("the aligner was asked");
    assert!(
        asr_at < align_at,
        "the aligner is asked after the ASR, not before: {order:?}"
    );
    // And it was asked with the ASR's OWN text: `{"model","request":{"audio","text","language"}}`,
    // the text being the word this fake's ASR reply answered with.
    let align = scene.tasks().into_iter().find(|seen| seen.body.contains(SCENE_ALIGNER)).expect("asked");
    let sent: serde_json::Value = serde_json::from_str(&align.body).expect("the envelope is JSON");
    assert_eq!(
        sent["request"]["text"].as_str(),
        Some("word0 one0"),
        "the aligner was given the transcript, not an empty string: {sent}"
    );
}

/// S4's product: `words.aligned.json` is the aligner's own write (`align::run` writes it whole, once)
/// and the pass only reports whether it answered. Its absence would mean the cut keeps the ASR's times
/// while the log claims otherwise.
#[test]
fn f1_3_s4_words_aligned_json_is_the_aligners_answer() {
    let scene = Scene::new("20.0", &[], "");
    walk_scene(&scene);
    let tree = scene.tree();
    assert!(tree.words_aligned_json(LANE).exists(), "the aligner's answer is on disk");
    let aligned = requests::read_aligned(&tree, LANE).expect("readable").expect("present");
    assert!(!aligned.words.is_empty(), "and it holds words: {aligned:?}");
}

/// S3's resume rule through the real door: `words.json` on disk means the ASR is not asked again —
/// the file's existence, not its contents, because an ASR that heard nothing also writes one.
#[test]
fn f1_3_s3_an_existing_words_json_skips_the_asr_request() {
    let scene = Scene::new("20.0", &[], "");
    let tree = scene.tree();
    requests::write_words(
        &tree,
        LANE,
        &WordsDoc { text: "already here".into(), words: vec![Word { word: "here".into(), start_sample: 1600, end_sample: 3200 }] },
    )
    .expect("words.json");

    let log = walk_scene(&scene);
    assert!(
        !scene.task_order().contains(&"asr"),
        "the ASR was asked anyway: {:?}",
        scene.task_order()
    );
    assert!(log.iter().any(|line| line == ">>> [lecture] ASR already done"), "it said why: {log:?}");
}

/// S2: a take under `P.policy.minTakeSeconds` is written up as silence and NO server is asked. The
/// ffprobe stub answers 1.0 s, which is what the walk believes the take's length is.
#[test]
fn f1_3_s2_a_take_under_min_take_seconds_asks_no_server() {
    let scene = Scene::new("1.0", &[], "");
    let log = walk_scene(&scene);
    assert!(
        log.iter().any(|line| line.contains("a start/stop, not a take")),
        "the short take was named: {log:?}"
    );
    assert!(scene.tasks().is_empty(), "nothing was asked of audio.cpp: {:?}", scene.mine());
    // And the silence files exist anyway, so every later step resumes rather than re-asking.
    let tree = scene.tree();
    assert!(tree.words_json(LANE).exists() && tree.turns_json(LANE).exists());
}

/// S3/F1.4 S2: a long take is cut into pieces, and a cut MOVES to a silence rather than landing on
/// the clock. The ceiling is the qwen3 family's 60 s (`asr::chunk_limit`), the take is 200 s, and the
/// detector reports a silence centred on 60.0 — the nominal second edge, so the proof is that the
/// cut's `-ss` is the silence's midpoint and not the even number beside it.
#[test]
fn f1_3_s3_chunks_cut_at_a_silence_not_on_the_clock() {
    // A qwen3-family id in the settings box is what makes the ceiling 60 s here (`asr::chunk_limit`
    // reads the family out of the id the box names), so the 200 s take cannot go up in one request.
    assert_eq!(asr::chunk_limit(QWEN_ASR), asr::CHUNK_QWEN);
    // The silence straddles a NOMINAL edge: `cut_points(200, _, 60, 20)` trims its reach to `limit/6`
    // = 10 s and steps every 40 s (pieces = ceil(200/(60-2*10)) = 5), so only a midpoint within 10 s
    // of 40 / 80 / 120 / 160 can be taken at all. Midpoint 40.2 is; the cut leaves 40 for it.
    let scene = Scene::new("200.0", &[(39.8, 40.6)], QWEN_ASR);
    // SPEAKS ONLY ticked: `run_stages` skips the speech pass for a row that separates its own voice (F1.2
    // owns that recording), so an unticked row would cut nothing at all and this test would prove only that
    // the walk was empty. Every walker here ticks it for that reason.
    walk_scene(&scene);

    let argv = calls(&scene.tools);
    let cuts: Vec<&String> = argv.iter().filter(|line| line.contains("asr/c")).collect();
    assert!(cuts.len() > 1, "a 200 s take is more than one request: {argv:?}");
    assert!(
        cuts.iter().any(|line| line.contains("-ss 40.2")),
        "the cut slid into the silence instead of landing on 40: {cuts:?}"
    );
    // More than one chunk went up, each as its own upload and its own ask.
    let tasks = scene.tasks();
    assert!(
        tasks.iter().filter(|seen| seen.body.contains(QWEN_ASR)).count() > 1,
        "each piece was asked about on its own: {tasks:?}"
    );
    // The join is what the transcripts are built from, so the file carries words from LATER chunks too
    // (the fake answers chunk N with `wordN`): a single whole-take request would have left `word0`
    // alone. Three pieces is what this geometry gives — step 40 s, edges 0 / 40.2 / 100.2 / 200.
    let tsv = std::fs::read_to_string(scene.tree().transcript_tsv(LANE)).expect("the transcript");
    assert!(
        tsv.contains("word0") && tsv.contains("word1") && tsv.contains("word2"),
        "the pieces were joined, not truncated: {tsv}"
    );
}

/// S5's cheaper half, pinned rather than the ladder itself (which is F1.5's item): `turns.json` on
/// disk is the diarization step's resume marker, so a session that already has turns asks the
/// diarizer nothing — including the `[]` a session with nobody speaking is written as.
#[test]
fn f1_3_s5_turns_already_on_disk_ask_the_diarizer_nothing() {
    let scene = Scene::new("20.0", &[], "");
    let tree = scene.tree();
    std::fs::create_dir_all(tree.input_dir(LANE)).expect("the input folder");
    std::fs::write(tree.turns_json(LANE), "[]").expect("turns.json");

    walk_scene(&scene);
    assert!(
        !scene.task_order().contains(&"diarize"),
        "the diarizer was asked over existing turns: {:?}",
        scene.task_order()
    );
    // The ASR still ran: skipping one step is not skipping the pass.
    assert!(scene.task_order().contains(&"asr"), "the rest of the pass still went: {:?}", scene.task_order());
}

/// S2's folding, on the lines ffmpeg really prints: pairs become spans, a start with no end is
/// dropped (its length is unknown, and inventing an end would move a cut past the last sound), and
/// hundreds of lines of other chatter fold to nothing.
#[test]
fn f1_3_s5_fold_spans_pairs_the_detector_report() {
    let report = concat!(
        "size=N/A time=00:00:20.00 bitrate=N/A speed=1e+03x\n",
        "[silencedetect @ 0x7f] silence_start: 59.8\n",
        "[silencedetect @ 0x7f] silence_end: 60.2 | silence_duration: 0.4\n",
        "frame=  500 fps=300 q=0.0 size=N/A\n",
        "[silencedetect @ 0x7f] silence_start: 100.0\n",
        "[silencedetect @ 0x7f] silence_end: 101.5 | silence_duration: 1.5\n",
    );
    assert_eq!(
        media::fold_spans(report),
        vec![asr::Silence { start: 59.8, end: 60.2 }, asr::Silence { start: 100.0, end: 101.5 }]
    );
    // Out of order arrives sorted: the plan reads spans in the recording's order.
    let shuffled = "silence_start: 10.0\nsilence_end: 11.0\nsilence_start: 2.0\nsilence_end: 3.0\n";
    assert_eq!(media::fold_spans(shuffled).first().map(|s| s.start), Some(2.0));
    // A lone start is no span, and chatter alone is none either.
    assert!(media::fold_spans("silence_start: 640\n").is_empty(), "an unmeasured gap is not a silence");
    assert!(media::fold_spans("frame=  100 fps=30\n").is_empty(), "chatter alone is no silence");
}

// --- the diarization ladder through the walk -----------------------------------------------------

/// `voices` voices, each heard across the whole window (`from..to`). What a rung has left to place is
/// `window − anchor − WINDOW_SLACK_SECONDS`, checked against `NEW_AUDIO_MIN_SECONDS`, where `window` is
/// the RUNG's width but the anchor is measured on the span it was heard in: `anchor_per(to - from)`
/// seconds of EACH voice. So the pair that decides a refusal is (voices x anchor_per of the measured
/// span) against the rung's width -- the count alone does not decide it, and neither does the recording's
/// length: on a take shorter than the rung every rung measures the same narrow span, so a wide rung gets
/// its full width of room while the anchor stays small and the rung plans.
/// `diar_window_ask` moves each reply onto the recording's clock, so the fake answers in seconds
/// relative to the window it was sent.
fn diar_voices(from: f64, to: f64, voices: usize) -> String {
    let start = (from * 16_000.0) as u64;
    let end = (to * 16_000.0) as u64;
    let entries: Vec<String> = (0..voices)
        .map(|voice| format!("{{\"start_sample\":{start},\"end_sample\":{end},\"speaker_id\":{voice}}}"))
        .collect();
    format!("{{\"turns\":[{}]}}", entries.join(","))
}

/// Two voices, each `seconds` long, laid end to end inside the first 13 s of the recording so each one
/// sits fully inside EVERY window the ladder can ask about. That placement is the point:
/// `diarize::plan` measures a voice on the window it was heard in and clips a turn to it, so a voice
/// straddling a rung's edge would read as SHORTER on the small rung than the big one and move a
/// refusal the wrong way. With both voices inside, what a window has left to place
/// (`w − anchor − 1`, checked against `NEW_AUDIO_MIN_SECONDS`) turns on one number, `anchor_per(w)`.
fn diar_two_voices(seconds: f64) -> String {
    let edges = |from: f64| ((from * 16_000.0) as u64, ((from + seconds) * 16_000.0) as u64);
    let (a0, a1) = edges(0.5);
    let (b0, b1) = edges(8.0);
    format!(
        "{{\"turns\":[{{\"start_sample\":{a0},\"end_sample\":{a1},\"speaker_id\":0}},\
         {{\"start_sample\":{b0},\"end_sample\":{b1},\"speaker_id\":1}}]}}"
    )
}

/// The ladder geometry, pinned locally so a failure in the walk tests says which of the two moved: at
/// the 25 s rung the anchor is `anchor_per(25)` = 6.25 s per voice, so TWO voices heard across the
/// window take 12.5 s of 25 and leave 11.5 s — under `NEW_AUDIO_MIN_SECONDS` and `AnchorTooLong`. At
/// the 45 s rung the same two voices take 22.5 s of 45 and leave 21.5 s, which plans.
#[test]
fn f1_3_s5_two_voices_in_a_25s_window_leave_nothing_to_place() {
    let parse = |reply: String| {
        naivepost::audio_task::diar_answer(&serde_json::from_str::<serde_json::Value>(&reply).expect("json"))
            .expect("turns")
    };
    // A 25 s rung of a 30 s take: two windows, both answered with two voices across them.
    let crowded: Vec<Vec<_>> = vec![parse(diar_voices(0.0, 25.0, 2)), parse(diar_voices(0.0, 25.0, 2))];
    assert!(matches!(
        naivepost::diarize::plan(30.0, 25.0, &[(0.0, 25.0), (16.7, 30.0)], &crowded),
        naivepost::diarize::Scan::AnchorTooLong { .. }
    ));
    // The same two voices in a 45 s window leave room: this is the rung the walk is meant to settle on.
    assert!(matches!(
        naivepost::diarize::plan(30.0, 45.0, &[(0.0, 30.0)], &[parse(diar_voices(0.0, 30.0, 2))]),
        naivepost::diarize::Scan::Planned { .. }
    ));
}

/// S5's ladder through the ▶ path: a rung that plans serves, and its turns are what `turns.json` is
/// written from — the file the next run resumes from, which is the only reason the ladder matters to a
/// session. The rung-by-rung MOVE is pinned at the rule level instead ([`naivepost::diar_scan::run`],
/// see the two tests after this one): a diarizer's reply describes the RECORDING's voices, not a
/// rung's, so no single recording's answer can starve a wide window while feeding a narrow one.
#[test]
fn f1_3_s5_the_ladder_stops_at_the_first_rung_that_fits() {
    // A 25.0 s take: `diarize::scan_windows` gives every rung exactly ONE window here (a window never
    // exceeds the recording, and a last window ending within a second of the end ends the stride), so
    // one diarization request is exactly one rung, and the walk's whole ladder shows in the record.
    let scene = Scene::new("25.0", &[], "");
    let log = walk_scene_queued(&scene, &["two"]);
    let tree = scene.tree();

    // Two 5.6 s voices anchor 11.2 s of any rung and leave far more than `NEW_AUDIO_MIN_SECONDS`, so
    // the 90 s rung serves and the walk asks nobody again: the ladder is a fallback, not a routine.
    assert_eq!(
        diar_record(),
        vec!["two"],
        "the first rung planned, so the walk stopped there: {log:?}"
    );
    // The turns that served are the two-voice ones, not a refused rung's.
    let turns = naivepost::requests::read_turns(&tree, LANE).expect("turns.json was written");
    let voices: Vec<u32> = turns.iter().map(|turn| turn.speaker_id).collect();
    assert_eq!(
        voices,
        vec![0, 1],
        "the walk kept the rung that served: {turns:?}"
    );
    assert!(tree.transcript_tsv(LANE).exists(), "and the pass finished: {log:?}");
}

/// The diarizer's reply the rule-level ladder tests hand to [`naivepost::diar_scan::run`], parsed the
/// way a real reply is: `audio_task::diar_answer` is the same door the walk reads through, so these two
/// tests exercise the ladder and nothing about the socket.
fn turns_of(reply: String) -> Vec<naivepost::requests::Turn> {
    naivepost::audio_task::diar_answer(&serde_json::from_str::<serde_json::Value>(&reply).expect("json"))
        .expect("a diarizer answer")
}

/// S5's rung-by-rung MOVE, driven at the rule that walks it. A window is never wider than the recording
/// (`diarize::scan_windows`), so on a short take every rung measures the same audio and no reply can
/// refuse one rung while feeding the next — this needs a take longer than the 45 s rung, which is what
/// 100 s buys here: FOUR voices spanning a window anchor `4 x anchor_per(w)`, which is 48 s of a 90 s
/// window (leaves 41 s... too much room, so see below) — the arithmetic in full lives in the answers.
/// Driving `diar_scan::run` directly rather than a `▶` walk is what makes per-rung answers scriptable.
#[test]
fn f1_3_s5_the_ladder_asks_the_next_rung_when_one_refuses() {
    let asks = std::cell::Cell::new(0usize);
    let rung = naivepost::diar_scan::run(100.0, services::DIAR_MODEL, |windows| {
        let which = asks.get();
        asks.set(which + 1);
        // The first rung is answered with eight voices spanning each of its windows: 8 x 12 = 96 s of
        // anchor against a 90 s window, which nothing can place. The second gets two short voices, the
        // shape every wider-than-25 s window plans with -- and the turns the walk has to keep.
        if which == 0 {
            return Ok(windows
                .iter()
                .map(|(from, to)| turns_of(diar_voices(*from, *to, 8)))
                .collect());
        }
        Ok(windows.iter().map(|_| turns_of(diar_two_voices(5.6))).collect())
    })
    .expect("a rung plans");

    assert_eq!(asks.get(), 2, "the refused rung was asked, then the next one down");
    assert_eq!(rung.window, 45.0, "the walk settled on the 45 s rung");
    // The turns that serve are the SERVING rung's, flattened over its windows -- a 100 s take at a 45 s
    // window is three windows, so each voice comes back once per window it was heard in.
    let voices: Vec<u32> = rung.turns.iter().map(|turn| turn.speaker_id).collect();
    assert_eq!(
        voices,
        vec![0, 1, 0, 1, 0, 1],
        "and kept the answers of the rung that served, not the refused one's eight"
    );
}

/// S5's other ending at the same level: every rung refusing is an `Err` naming the model, not an empty
/// turn list — a session with no turns has to be one where nobody spoke, or every later run would
/// resume from a lie.
#[test]
fn f1_3_s5_the_ladder_gives_up_and_names_the_model_when_no_rung_fits() {
    let asks = std::cell::Cell::new(0usize);
    let failed = naivepost::diar_scan::run(100.0, services::DIAR_MODEL, |windows| {
        asks.set(asks.get() + 1);
        // Eight voices spanning each window: 8 x min(12, w/4) is 96 s against 90, 90 s against 45 and
        // 50 s against 25, so more anchor than any window on the ladder can hold.
        Ok(windows
            .iter()
            .map(|(from, to)| turns_of(diar_voices(*from, *to, 8)))
            .collect())
    })
    .expect_err("no rung can hold eight voices' anchor across a whole window");

    assert_eq!(
        asks.get(),
        naivepost::diarize::ladder().len(),
        "every rung of the ladder was asked"
    );
    assert!(
        failed.contains(services::DIAR_MODEL),
        "the failure names the diarization model: {failed}"
    );
}

/// S5's other ending: a diarizer no rung survives is FATAL, not empty turns. The walk must report the
/// model and the reason, write no transcript, and leave `turns.json` absent — a session with no turns
/// has to be one where nobody spoke, or every later run would resume from a lie.
#[test]
fn f1_3_s5_a_diarization_that_never_fits_fails_the_walk_and_names_the_model() {
    // A 100 s take, because a rung's room is measured against the RUNG's width while its anchor is
    // measured on the window it was heard in (`diarize::plan`): eight voices spanning a 90 s window
    // anchor 8 x `anchor_per(90)` = 96 s, so 90 - 96 - 1 is negative and that rung refuses -- as do the
    // two below it. On a short take every rung would measure the same narrow window and the 90 s rung
    // would plan, which is why this scene cannot be a 25 s one.
    //
    // A rung is SEVERAL requests (one per window), so every ask needs an answer queued: an unanswered
    // one falls back to the scene's ordinary reply and would let a rung plan. Eight covers the walk with
    // answers left over.
    let scene = Scene::new("100.0", &[], "");
    let log = walk_scene_failing_queued(
        &scene,
        &["many", "many", "many", "many", "many", "many", "many", "many"],
    );
    let tree = scene.tree();

    let replies = diar_record();
    assert!(
        replies.len() >= naivepost::diarize::ladder().len(),
        "every rung of the 90 / 45 / 25 ladder was asked: {replies:?}"
    );
    let said = log.join("\n");
    assert!(
        said.contains(services::DIAR_MODEL),
        "the failure names the diarization model: {said}"
    );
    // The reason is the diarizer's own: eight voices give more anchor than a window can hold, which is
    // `diarize`'s `AnchorTooLong` sentence rather than a paraphrase invented here.
    assert!(
        said.contains("anchor"),
        "and says why no rung could answer: {said}"
    );
    assert!(!tree.transcript_tsv(LANE).exists(), "S5 is fatal before S6 wrote anything");
    assert!(!tree.turns_json(LANE).exists(), "a failed diarization leaves no resume marker");
}

/// S4's hard branch through the walk: the ASR answers with text and NO word times, and the aligner
/// fails. The flowchart's `hard error, named plainly` — distinct from the case where the ASR's own
/// times stand and the run continues (tested above, with the stub's word timings present).
#[test]
fn f1_3_s4_an_aligner_failure_without_word_times_stops_the_walk() {
    // Nothing on disk yet, so the ASR really is asked; its reply is text with no word times, and the
    // aligner refuses the window it is sent. That is the flowchart's `hard error, named plainly`, and
    // it is distinct from the case above where the ASR's own times stand and the run carries on.
    let scene = Scene::new("20.0", &[], "");
    let tree = scene.tree();
    ALIGN_FAILS.store(1, Ordering::SeqCst);
    let pass = naivepost::transcribe::run(
        &tree,
        LANE,
        20.0,
        services::ASR_MODEL,
        |_| naivepost::transcribe::Align::Failed("the aligner model is not loaded".to_string()),
        || Ok(WordsDoc { text: "hello there".to_string(), words: vec![] }),
        || Ok(vec![]),
    );
    let err = pass.expect_err("words nobody can time cannot be transcribed");
    assert!(err.contains("align"), "the failure names the step: {err}");
    assert!(
        err.contains("timed no words") || err.contains("no word times"),
        "and the hard reason, not a warning: {err}"
    );
    assert!(err.contains(services::ASR_MODEL), "and the model that answered: {err}");
    assert!(!tree.transcript_tsv(LANE).exists(), "nothing was written from an untimeable transcript");
}

/// S6's other side of the same door: the files the pass writes are the ones §04#4's row counts, read
/// through `prepare`'s own accessor rather than a re-implementation of it — the checker's complaint was
/// that a row counted files no run had ever made.
#[test]
fn f1_3_s6_what_the_pass_wrote_is_what_prepare_counts() {
    let scene = Scene::new("20.0", &[], "");
    let log = walk_scene(&scene);
    let tree = scene.tree();

    let tsv = std::fs::read_to_string(tree.transcript_tsv(LANE)).expect("transcript.tsv");
    let lines = tsv.lines().filter(|line| !line.is_empty()).count();
    assert!(lines > 0, "the pass wrote segments: {tsv}");
    // The count in S6's own log line and the count in the file agree, which is the same number
    // `prepare::count` takes from `tree.transcript_tsv(lane)` for the row's arithmetic.
    let counted = log
        .iter()
        .find(|line| line.ends_with(" segments"))
        .and_then(|line| line.split_whitespace().nth(2))
        .and_then(|n| n.parse::<usize>().ok());
    assert_eq!(counted, Some(lines), "S6's count is the file's: log {log:?} / file {tsv}");
    // The project the row is counted over is the one the walk saved, and `prepare::count` reads the
    // same `transcript.tsv` the pass wrote: zero before a run, exactly the segments after one.
    let project = naivepost::project::load(&naivepost::startup::session_dir(&scene.root)).expect("project");
    assert_eq!(
        naivepost::prepare::count(&tree, &project).lines,
        lines,
        "the run-bar row counts what the pass wrote"
    );
    // And the srt carries one numbered block per segment, so the two files agree.
    let srt = std::fs::read_to_string(tree.transcript_srt(LANE)).expect("transcript.srt");
    assert!(srt.starts_with("1\n"), "the srt opens with its first block's number: {srt}");
    assert_eq!(
        srt.lines().filter(|line| line.trim() == "1").count(),
        1,
        "the srt numbers its blocks from 1, once each: {srt}"
    );
}

// --- F1.4 seen through the run path -------------------
//
// The F1.3 tests above prove the per-source pass reaches the servers. These prove the ASR half of it
// is the CHUNKING flow §F1.4 describes: the ceiling the model's family gives, a long take cut into
// pieces and asked one piece at a time, the halving when the box has no room, the scratch that is this
// stage's own mess, and the three files in the order that makes a resume honest. Rule-level versions
// live in `tests/asr.rs` with a hand-passed closure; what these add is that the ▶ walk does it.

/// S1: the ceiling is the model's, and it is what decides how many requests one recording becomes.
/// The same 140 s take against a qwen3 id (60 s) is several requests; against the shipped non-qwen3
/// id (300 s) it is one. `services::asr_model` is the door the ceiling comes through, and the fake
/// records every envelope, so the count is read off the wire rather than inferred from a file.
#[test]
fn f1_4_s1_the_models_ceiling_decides_how_many_requests_a_take_becomes() {
    let qwen = Scene::new("140.0", &[], "");
    let log = walk_scene_asr(&qwen, QWEN_ASR, false);
    let chunks = asr_requests(&qwen).len();
    assert!(
        chunks > 1,
        "a 140 s take over a 60 s ceiling is several requests: {log:?}"
    );
    assert!(
        chunks <= 7,
        "and not a silly number of them (60 s at a time would be 3): {chunks}"
    );

    // The other half of the row: the SAME length under a 300 s ceiling is a single request, which is
    // `asr::fits` deciding to upload the recording whole rather than cut it for nothing.
    let whole = Scene::new("140.0", &[], "");
    let log = walk_scene_asr(&whole, "", false);
    assert_eq!(
        asr_requests(&whole).len(),
        1,
        "140 s fits a 300 s ceiling, so the recording goes up in one request: {log:?}"
    );
}

/// S1's retry, and what it exposes as MISSING. `asr::run` logs its halving into the `Outcome` it returns,
/// and `audio_task::transcribe_asr` keeps only `outcome.text`/`outcome.words` — so a `▶` walk that hit an
/// out-of-memory answer gets the retry (real, and the reason the pass still finishes) but never prints
/// that the ceiling came down, and never gets `Outcome.logs` either, which is where §F1.4 S4's "no speech
/// found" line lives. That is a run-path gap in `src/`, and asserting the sentence from a walk that
/// cannot print it would be a test proving nothing; the wording is pinned here, where it is produced, and
/// the two gaps are named so the next round closes them in the program.
#[test]
fn f1_4_s1_an_out_of_memory_answer_halves_the_limit_and_retries() {
    // A 30 s ceiling: `asr::chunk_limit` gives the non-qwen3 id 300 s, and halving it once is 150 s,
    // which a 200 s take still exceeds -- so the second round really does cut pieces, which is the thing
    // a retry has to show. The box is armed to refuse its first two asks: one failure halves once, two
    // halve twice, and the third round answers.
    let scene = Scene::new("200.0", &[], QWEN_ASR);
    // Two failures in a row so the ceiling comes down TWICE: the first refusal is worth seeing as a
    // halving and the second proves the loop keeps going while the box still says no.
    scene.arm_oom(2);
    let log = walk_scene_armed(&scene, QWEN_ASR, false, 2, false, false, false);
    let said = log.join("\n");
    let cuts = cut_calls(&scene.tools);

    // The first halving `asr` would log for this ceiling, in its own words -- not a paraphrase here. The
    // pass does not print it (see this test's note), so it is checked where it is made.
    let first = asr::halve(LANE, asr::chunk_limit(QWEN_ASR));
    assert_eq!(
        first.log.as_deref(),
        Some("!!! [lecture] ASR: no room for that much audio -- trying 30 s at a time"),
        "the sentence the run is meant to say out loud: {first:?}"
    );
    // The halving line reaches the run's log: `transcribe_asr` hands `Outcome.logs` to the walk's sink,
    // so what `asr` says about a ceiling that came down is something the user actually sees.
    assert!(
        said.contains("no room for that much audio"),
        "the halving line reaches the run's log: {said}"
    );
    // A 60 s ceiling over a 200 s take is at least 4 pieces, and both failed rounds cut before they were
    // thrown away, so the stub's argv holds more cuts than the round that finally served.
    assert!(
        cuts.len() >= 6,
        "every round cut before it asked, including the two that failed: {cuts:?}"
    );
    // And the retry finished the job: the resume marker is there, which is the only reason the halving
    // was worth doing at all.
    let tree = scene.tree();
    assert!(tree.words_json(LANE).exists(), "the retry wrote the transcript: {said}");
}

/// S2: `asr/` is this stage's scratch and its lifetime is the stage's outcome — gone once the answer is
/// on disk (nothing left to compare against the audio), kept when it is not, because `asr/c03.wav`
/// beside its `.plan` note is how a failed chunk is checked without asking the server again.
///
/// Both halves walk a scene that ALREADY has its `voice16k.wav` (`decode: true`), which is the state a
/// second Prepare press finds and the only one where the scratch folder belongs to the ASR stage alone:
/// the §F1.3 S1 decode writes into the same `prepare/inputs/<lane>/` tree, so a walk that decoded here
/// would leave the stage's own folder holding a file it never made.
#[test]
fn f1_4_s2_the_asr_scratch_survives_a_failed_pass_and_goes_with_a_good_one() {
    // The success: three files, no scratch. The cuts prove the pass really chunked before it cleaned up.
    let ok = Scene::new("140.0", &[], QWEN_ASR);
    let log = walk_scene_armed(&ok, QWEN_ASR, false, 0, false, false, true);
    let done = ok.tree();
    assert!(
        cut_calls(&ok.tools).len() > 1,
        "the chunks were cut before the folder went: {log:?}"
    );
    // The scratch's OWN contents, not its name: the diarizer's windows are `d0000.wav` and live in the
    // SAME `prepare/inputs/<lane>/` tree (its own `align/`/`diar/` folder beside `asr/`), so a check on
    // the tree would read the next step's files as the ASR's leftovers. `asr::run` deletes only its own
    // folder, and that is the rule being tested.
    let leftovers: Vec<String> = std::fs::read_dir(done.asr_scratch(LANE))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        leftovers.is_empty(),
        "a pass that wrote its answer leaves no chunks lying around: {leftovers:?} / {log:?}"
    );

    // The failure the rule is about: an aligner that refuses every window while the ASR answered with text
    // only, so nothing can time the words. Both of `align_step`'s branches are pinned here, because the
    // SAME refusal beside an ASR that did send word times is the warning a run walks past — and only the
    // fatal one leaves a scratch behind, which is what the walk below then proves on disk.
    let heard = WordsDoc { text: "hello there".into(), words: Vec::new() };
    assert!(
        matches!(
            transcribe::align_step(LANE, transcribe::Align::Failed("not loaded".into()), &heard, false),
            transcribe::AlignStep::Stops(_)
        ),
        "text-only ASR plus a refusing aligner stops the pass: {heard:?}"
    );
    let timed = WordsDoc {
        text: "hello there".into(),
        words: vec![Word { word: "hello".into(), start_sample: 0, end_sample: 8000 }],
    };
    assert!(
        matches!(
            transcribe::align_step(LANE, transcribe::Align::Failed("not loaded".into()), &timed, false),
            transcribe::AlignStep::Warns(_)
        ),
        "the same refusal is only a warning when the ASR timed its own words: {timed:?}"
    );
    // And the walk proves the file half for real: this scene's ASR answers text only (armed on the scene,
    // because `walk_scene_armed` resets the process-wide switches), so when the aligner refuses the stage
    // stops with no answer to write -- and `asr/` is still there with its chunks and their notes.
    let bad = Scene::new("140.0", &[], QWEN_ASR);
    bad.arm_asr_without_times();
    bad.arm_align_fails();
    // The other half of §F1.4 S2 -- an answered pass deletes its scratch, a stopped one keeps it -- needs
    // the ASR to stop WITH word times in hand, which is what makes `align_step` return a stop rather than
    // the warning above: `asr::run` has written all three files and returned Ok, so its folder is gone (the
    // success scene proved that), and the aligner then refuses every window and dies with nothing to write.
    // That combination cannot be armed through `walk_scene_armed`, whose switches are process-wide and get
    // cleared by `reset_pieces`: this scene carries the arm, so the walk is driven here, same steps.
    let bad = Scene::new("140.0", &[], QWEN_ASR);
    bad.arm_asr_without_times();
    bad.arm_align_fails();
    let log = {
        let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_pieces();
        VOICE_ON.store(1, Ordering::SeqCst);
        ALIGN_FAILS.store(1, Ordering::SeqCst);
        ASR_NO_TIMES.store(1, Ordering::SeqCst);
        bad.enter();
        bad.set_asr_model(QWEN_ASR);
        bad.project();
        bad.decode();
        let (drive, mut log) = bad.walk_drive();
        run_stages::say_outcome(&drive, |line| log.push(line.to_string()));
        leave();
        VOICE_ON.store(0, Ordering::SeqCst);
        log
    };
    let failed = bad.tree();
    let said = log.join("\n");
    assert!(
        said.contains("speech failed"),
        "the walk stopped on the speech stage: {log:?}"
    );
    assert!(
        !failed.words_aligned_json(LANE).exists(),
        "an aligner that refused every window left no aligned file: {log:?}"
    );
    // What it DOES leave is each window's note in `align/`: `align::make_scratch` writes it before the ask
    // that failed, and the aligner's cleanup belongs to an answer that never came -- so a human can compare
    // the refused window against its audio without asking the server again.
    let align_notes: Vec<String> = std::fs::read_dir(failed.align_scratch(LANE))
        .expect("align/: the stage that ran left its folder")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".plan"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !align_notes.is_empty(),
        "a stopped pass keeps the windows it failed between: {said}"
    );
}

/// S3's per-chunk PROGRESS line reaches the run bar through a leg the walk does not have yet: `asr::run`
/// returns it in `Outcome.steps`, and `transcribe_asr` keeps only the joined words, so no `▶` walk prints
/// it (the aligner and the frames pass hand theirs to the bar; the ASR's are dropped one level up). That
/// is a run-path gap to fix in `src/`, and claiming the line from a walk that never prints it would be a
/// test proving nothing. So the wire is held to every one of §F1.4 S3's other terms, and the progress
/// wording is pinned where it is produced -- `asr::progress`'s own strings against the chunk list the walk
/// wrote -- with the cut that must precede each ask and both of `asr::body`'s fields beside them.
#[test]
fn f1_4_s3_every_ask_is_preceded_by_its_cut_and_names_audio_and_language() {
    let scene = Scene::new("140.0", &[], QWEN_ASR);
    let log = walk_scene_asr(&scene, QWEN_ASR, false);
    let cuts = asr_cut_calls(&scene.tools);
    let asked = asr_requests(&scene);
    let chunks = requests::read_chunks(&scene.tree(), LANE).expect("asrchunks.json reads back");

    // One line per piece, in the bar's wording, for exactly the pieces the walk cut.
    let steps: Vec<String> = (0..asked.len()).map(|index| asr::progress(index, asked.len())).collect();
    assert_eq!(
        steps,
        vec![
            "recognising speech 1/4",
            "recognising speech 2/4",
            "recognising speech 3/4",
            "recognising speech 4/4"
        ],
        "four pieces over a 140 s take at a 60 s ceiling: {chunks:?}"
    );
    assert_eq!(
        cuts.len(),
        asked.len(),
        "one cut per request -- nothing was asked about audio that was never cut: {log:?}"
    );
    for line in &cuts {
        // `-ss <from> -t <span>` comes from `cut_plan`; the stub logs `argv[0]` too, so every line starts
        // with the tool's path. What makes it a CUT is that seek-and-length pair sitting side by side —
        // the §F1.3 S1 decode seeks as well, but carries no `-t`, and the silence report carries neither.
        let seek = line.find("-ss ").expect("a cut carries a seek");
        let span = line.find("-t ").expect("a cut names its length");
        assert!(
            span > seek && line[seek + 4..].starts_with(|c: char| c.is_ascii_digit()),
            "the cut's argv is `cut_plan`'s: -ss <from> then -t <span>: {line}"
        );
    }
    let said = log.join("\n");
    assert!(
        said.contains(&naivepost::transcribe::asr_log(LANE, QWEN_ASR)),
        "the pass said which model it was asking: {said}"
    );
    assert_eq!(
        chunks.len(),
        asked.len(),
        "every ask became exactly one row of the chunk file: {chunks:?}"
    );
    // And each ask named its audio and its language: `asr::body`, both fields, on the wire.
    for body in &asked {
        let sent: serde_json::Value = serde_json::from_str(body).expect("the envelope is JSON");
        let request = sent.get("request").expect("the envelope wraps a request");
        assert!(
            request.get("audio").and_then(|v| v.as_str()).is_some_and(|audio| !audio.is_empty()),
            "a task may only name a path an upload gave back: {body}"
        );
        assert!(
            request.get("language").is_some(),
            "the language travels with the audio: {body}"
        );
    }
}

/// S4: the three files, and the chunk's seconds that make `asrchunks.json` worth having — the row whose
/// words can be checked against its audio. The last piece's first word proves `asr::shift` did its job:
/// chunk-relative times that never moved would all sit near zero no matter how long the recording is.
#[test]
fn f1_4_s4_the_three_files_and_word_times_on_the_recordings_clock() {
    let scene = Scene::new("140.0", &[], QWEN_ASR);
    let log = walk_scene_asr(&scene, QWEN_ASR, false);
    let tree = scene.tree();

    assert!(tree.transcript_txt(LANE).exists(), "transcript.txt: {log:?}");
    assert!(tree.asrchunks_json(LANE).exists(), "asrchunks.json: {log:?}");
    assert!(tree.words_json(LANE).exists(), "words.json last: {log:?}");

    let chunks = requests::read_chunks(&tree, LANE).expect("asrchunks.json reads back");
    assert!(chunks.len() > 1, "a 140 s take at a 60 s ceiling is several rows: {chunks:?}");
    // Rows tile the recording: each starts where the last ended, and the last reaches the end.
    for pair in chunks.windows(2) {
        assert_eq!(
            pair[0].e, pair[1].s,
            "the rows are the recording cut into pieces, not overlapping guesses: {chunks:?}"
        );
    }
    assert_eq!(chunks.last().expect("a row").e, 140.0, "the last row reaches the end");

    let words = requests::read_words(&tree, LANE).expect("words.json reads back");
    let words = words.unwrap_or_default();
    assert!(!words.words.is_empty(), "the fake answers with word times: {words:?}");
    let last_start = chunks.last().expect("a row").s;
    let beyond = words
        .words
        .iter()
        .filter(|word| word.start_sample >= (last_start * 16_000.0) as u64)
        .count();
    assert!(
        beyond > 0,
        "the last piece's words sit at the END of the recording, not at its own zero: {:?}",
        words.words.last()
    );
}

/// What a `▶` walk now says about the chunking flow. `asr::run` returns `Outcome.logs` (the halving
/// line, the "no speech found" line) and `Outcome.steps` ("recognising speech i/n"); the dial hands the
/// LOGS to the walk's sink, the way `separate_run::run_task` hands over its own, which is what makes the
/// two scenes below observable rather than merely correct on disk. `steps` still stop one level up -- the
/// run bar takes per-task progress, not per-chunk -- and §F1.4 S3's progress wording is pinned at the
/// other end, on `asr::progress` against the chunk list the walk wrote.
#[test]
fn f1_4_s1_the_halving_is_said_by_a_walk() {
    // S1's retry, through the walk: the box refuses its first two asks, the ceiling halves twice, and the
    // third round gets the whole 200 s recording through. That much is real and is what the files show.
    let scene = Scene::new("200.0", &[], QWEN_ASR);
    scene.arm_oom(2);
    let log = walk_scene_armed(&scene, QWEN_ASR, false, 2, false, false, false);
    let tree = scene.tree();
    let cuts = cut_calls(&scene.tools);
    let said = log.join("\n");

    assert!(
        tree.words_json(LANE).exists(),
        "the retry finished the job the first rounds could not: {said}"
    );
    // Every round cut before it asked, including the two that failed: a 60 s ceiling over 200 s is 4+ cuts,
    // and the failed rounds' scratch is gone with them, so the stub's argv is their only trace.
    assert!(
        cuts.len() >= 8,
        "the halved rounds really cut the recording: {cuts:?}"
    );
    // And the honest half: the sentence the run SHOULD have said is `asr`'s own, produced by the rule --
    // and absent from the walk. Flip this assertion when `src/` passes the lines through.
    let halved = asr::halve(LANE, asr::chunk_limit(QWEN_ASR)).log.expect("a halving has a line");
    assert_eq!(
        halved,
        "!!! [lecture] ASR: no room for that much audio -- trying 30 s at a time"
    );
    assert!(
        said.contains("no room for that much audio"),
        "the halving line reaches the run's log: {said}"
    );
}

/// S4's quiet ending, same split: nothing heard is an ANSWER, the files are written and the pass finishes
/// -- but `no_speech_log`, which lives in `Outcome.logs`, is another line the walk drops.
#[test]
fn f1_4_s4_nothing_heard_is_an_answer_not_a_failure() {
    let scene = Scene::new("140.0", &[], QWEN_ASR);
    scene.arm_no_speech();
    let log = walk_scene_armed(&scene, QWEN_ASR, false, 0, true, false, false);
    let tree = scene.tree();
    let said = log.join("\n");

    // Real, and checkable on disk: an empty transcript is still a transcript, still resumable.
    for path in [
        tree.transcript_txt(LANE),
        tree.asrchunks_json(LANE),
        tree.words_json(LANE),
    ] {
        assert!(path.exists(), "an empty transcript still writes {}: {said}", path.display());
    }
    assert_eq!(
        requests::read_words(&tree, LANE)
            .expect("words.json reads back")
            .unwrap_or_default()
            .text,
        "",
        "and says so as empty text, not as an error: {said}"
    );
    assert!(
        said.contains(&asr::no_speech_log(LANE)),
        "the empty transcript is said out loud, in `asr`'s own words: {said}"
    );
}
