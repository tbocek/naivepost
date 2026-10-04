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
        Scene { root, url, tools, seen }
    }

    /// The session's project file: one recording row, nothing ticked (the split belongs to F1.2).
    fn project(&self) {
        let source = Source {
            path: "sources/lecture.wav".to_string(),
            footage: false,
            narrator: 0,
            sepvoice: false,
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
        let mut queue = naivepost::runqueue::Queue::new();
        let mut procs = Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let stages = self.stages();
        let drive = run_stages::drive_walk(&stages, &mut queue, || false, || false, |line| {
            log.push(line.to_string());
        }, &mut procs);
        assert!(matches!(drive, run_stages::Drive::Done { .. }), "the walk finished: {drive:?} / log {log:?}");
        log
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

/// Hold the env, lay out the scene, walk it, put the env back. Every assertion afterwards reads files
/// and the fake's record, neither of which another test's `set_current_dir` can move.
fn walk_scene(scene: &Scene) -> Vec<String> {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for name in ["NAIVEPOST_TTS_URL", "AUDIOCPP_SERVER"] {
        std::env::remove_var(name);
    }
    reset_pieces();
    scene.enter();
    scene.project();
    let log = scene.walk();
    leave();
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
/// anchor voice on the 90 s rung (`diarize::ANCHOR_MIN_SECONDS`).
const TURNS: &str = r#"{"turns":[{"start_sample":1600,"end_sample":80000,"speaker_id":0},{"start_sample":120000,"end_sample":200000,"speaker_id":1}]}"#;

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
        return (200, TURNS.to_string());
    }
    if body.contains(SCENE_ALIGNER) {
        // The aligner is asked about ONE window, in the same order as the ASR's pieces, and its
        // reply places that piece's words at that piece's seconds.
        let chunk = next_of(&WINDOWS);
        return (200, words_aligned(chunk));
    }
    (200, words_chunk(next_of(&CHUNKS)))
}

/// Which piece a request is about: the fake answers every upload with the same server path, so the
/// piece is counted by arrival — the Nth ask is piece N-1, which is the order `asr::run` and
/// `align::run` walk their pieces in.
fn next_of(counter: &'static AtomicUsize) -> usize {
    counter.fetch_add(1, Ordering::SeqCst)
}

static CHUNKS: AtomicUsize = AtomicUsize::new(0);
static WINDOWS: AtomicUsize = AtomicUsize::new(0);

/// The piece counters are per-process while the scenes are per-test, so a test clears them before it
/// walks: the Nth ask must be piece N-1 of THIS scene's recording, not of whichever test ran first.
fn reset_pieces() {
    CHUNKS.store(0, Ordering::SeqCst);
    WINDOWS.store(0, Ordering::SeqCst);
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
