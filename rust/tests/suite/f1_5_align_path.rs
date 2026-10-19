//! F1.5 Forced alignment — the two things only the ▶ path can show (spec/04-prepare.md §F1.5).
//!
//! `tests/suite/align.rs` proves [`naivepost::align`]'s rules with closures and no socket, including S1's
//! refusal text and S6's arithmetic. What a rule test cannot see is the door in front of it
//! (`audio_task::transcribe_source`, reached only through `run_stages::drive_walk`): a configured
//! aligner the box does not declare used to fail the WHOLE speech stage, and every sentence the
//! aligner itself produces (S1's picker line, the halving line, S6's bare-seconds warning) used to be
//! dropped on the floor with the `Outcome`. Both are the round trip below.
//!
//! This file is self-contained rather than a submodule of `tests/common`: the scene scaffolding in
//! `tests/suite/f1_3_run_path.rs` is ~700 lines of that file's own diarizer ladder, ASR chunk arms and stub
//! shapes, and sharing it would mean restructuring a green suite for a file that needs one recording,
//! three endpoints and two stub tools. The env and the working folder are process-wide, so every walk
//! here runs under `ENV`, one at a time, the same rule that file follows.

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use naivepost::align;
use naivepost::layout::Tree;
use naivepost::project::{Project, Source};
use naivepost::run::{self, Subprocesses};
use naivepost::run_stages;
use naivepost::services;
use naivepost::settings::{self, Conf, Paths};
use crate::common;
use common::temp_dir_tagged;

/// The lane of both scenes: the stem of the recording's name, which names the decoded audio and every
/// sidecar (§F1.3 stores under it).
const LANE: &str = "talk";
/// The recording row's path, relative to the session folder like every stored path.
const RECORDING: &str = "sources/talk.wav";

/// The aligner this scene's box declares for `task: align`. Naming an id of our own rather than
/// `services::ALIGN_PREFERENCE` keeps the preference out of the story: what puts an id on the wire here
/// is the role, nothing else.
const DECLARED_ALIGNER: &str = "wav2vec-align";
/// The id a test types into the settings box that the box does NOT serve. §F1.5 S1's whole point.
const REFUSED_ALIGNER: &str = "mms-aligner";

/// The env and the working folder belong to the PROCESS and cargo runs these tests as threads.
static ENV: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// One scene: a session folder, a settings folder whose conf names this scene's fake and stub tools,
/// and the answers this scene's fake gives. Every switch lives on the scene and is read off the port the
/// request arrived on — a process-wide switch would let another test's walk be answered with this
/// scene's refusal and send the suite flaky under `just test`'s thread count.
#[allow(dead_code)] // `paths` is the scene's own settings folder, read by the program's own legs rather
// than by a test, so nothing here calls it; `port` is used to key the fake.
struct Scene {
    root: PathBuf,
    /// This scene's fake address, as the conf holds it: §F1.5 S1's line names the server that refused,
    /// so a test has to know which one that is.
    url: String,
    /// This scene's loopback port, which is how its fake finds the scene a request came from.
    port: u16,
    /// The settings folder this scene's conf lives in, kept so a test can re-read what it armed.
    paths: Paths,
    seen: Arc<Mutex<Vec<Seen>>>,
    fakes: SceneFakes,
}

/// What one request looked like.
#[derive(Debug, Clone)]
struct Seen {
    line: String,
    body: String,
}

/// One scene's answer state, shared with its fake's threads through the port-keyed registry.
/// `align_model` is held here as well as in the conf because the fake's model list has to agree with
/// what the settings box says: the box is the thing a test is arming, and `align::aligners` compares
/// it against the list.
#[derive(Default, Clone)]
struct SceneFakes {
    /// The id the settings box names, as the fake read it off the scene that made it.
    align_model: Arc<std::sync::Mutex<String>>,
    /// Answer each aligner window with its word in the first second: what S6 measures as bare.
    words_in_first_second: Arc<std::sync::Mutex<bool>>,
    /// Refuse the first ask to the first-listed aligner of every window, so S3's halving line and S1's
    /// picker line are produced by the walk rather than asserted about a closure.
    align_refuses_first: Arc<std::sync::Mutex<bool>>,
    /// Asks that went to the first-listed aligner, so only its FIRST is refused.
    refuser_asks: Arc<AtomicUsize>,
    windows: Arc<AtomicUsize>,
    pieces: Arc<AtomicUsize>,
}

/// One scene's fake, keyed by its loopback port: which scene a request came from decides what answers
/// it, and that is what keeps these tests order-free.
static SCENES: Mutex<Vec<(u16, SceneFakes)>> = Mutex::new(Vec::new());

impl Scene {
    /// Lay out one scene: `duration` is what the ffprobe stub answers (so it IS the take's length as far
    /// as the program can know), `silences` what the silencedetect stub prints, and `align_model` goes
    /// into the settings box, which is the thing §F1.5 S1 is about.
    fn new(duration: &str, silences: &[(f64, f64)], align_model: &str) -> Self {
        let root = temp_dir_tagged(&format!(
            "f15-align-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(session.join("sources")).expect("the sources folder");
        std::fs::write(session.join(RECORDING), b"wav-bytes").expect("the recording");
        // The conf gets the ROOTS: `settings::paths_from` appends the `naivepost` folder itself, so
        // handing it `cfg/naivepost` would read a default conf off the real `$HOME`.
        let paths = Paths {
            config_dir: root.join("cfg/naivepost"),
            data_dir: root.join("data/naivepost"),
        };
        let (url, port, seen) = fake_audio();
        let fakes = SceneFakes::default();
        *fakes.align_model.lock().expect("the arms lock") = align_model.to_string();
        SCENES.lock().expect("the scene lock").push((port, fakes.clone()));
        let tools = stubs(&format!("{}-{}", std::process::id(), port), duration, silences);
        let mut conf = Conf::default();
        conf.audio_server = url.clone();
        conf.ffmpeg = tools.join("ffmpeg").to_string_lossy().into_owned();
        conf.align_model = align_model.to_string();
        settings::save(&paths, &conf).expect("the conf is written");
        Scene { root, url, port, paths, seen, fakes }
    }

    /// Make this scene's box declare `REFUSED_ALIGNER` instead of the configured id.
    /// Lay out a scene whose settings box names an aligner its model list does NOT declare — §F1.5 S1's
    /// refusal. Everything else is `new`'s 40 s take with its one late silence.
    fn new_refusing(align_model: &str) -> Self {
        Scene::new("40.0", SILENCES, align_model)
    }

    /// Answer every aligner window with its word placed at 0.2 s of the window.
    fn aligner_places_everything_at_the_start(&self) {
        *self.fakes.words_in_first_second.lock().expect("the arms lock") = true;
    }

    /// Refuse the first-listed aligner's every ask with an out-of-memory answer, so the picker walks on
    /// to the next one and S1's picker line is produced by the walk rather than asserted about a closure.
    fn aligner_has_no_room_once(&self) {
        *self.fakes.align_refuses_first.lock().expect("the arms lock") = true;
    }

    /// The project file: one recording row with SPEAKS ONLY ticked, which is what makes the walk have a
    /// speech task at all (`prepare_run` answers NONE when nothing is ticked).
    fn project(&self) {
        naivepost::project::save(
            &Project {
                sources: vec![Source {
                    path: RECORDING.to_string(),
                    footage: false,
                    narrator: 0,
                    sepvoice: true,
                    tracks: vec![],
                }],
                ..Project::default()
            },
            &naivepost::startup::session_dir(&self.root),
        )
        .expect("the project file");
    }

    fn tree(&self) -> Tree {
        Tree::new(naivepost::startup::session_dir(&self.root)).expect("the session tree")
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

    /// Write `turns.json` and tick the diarizer's resume marker, so the walk asks about alignment and
    /// not about who spoke. This item is not about the diarizer's ladder; `tests/suite/f1_3_run_path.rs`
    /// walks that, and a scene here that tripped it would fail for a reason this file is not about.
    fn skip_diarization(&self) {
        naivepost::requests::write_turns(
            &self.tree(),
            LANE,
            &[naivepost::requests::Turn {
                start_sample: 8_000,
                end_sample: 120_000,
                speaker_id: 0,
            }],
        )
        .expect("turns.json");
    }

    /// The speech stage only: frames, describe and fix belong to other items.
    fn stages(&self) -> Vec<run_stages::Stage> {
        let project = naivepost::project::load(&naivepost::startup::session_dir(&self.root)).expect("the project");
        run_stages::plan(run::Step::Prepare, &project)
            .into_iter()
            .filter(|stage| stage.name == "speech")
            .collect()
    }

    /// Walk the speech stage, with no expectation about how it ended: S1's ending IS the thing a test
    /// names, so `say_outcome` puts the walk's own verdict in the log either way.
    fn walk(&self) -> Vec<String> {
        let mut queue = naivepost::runqueue::Queue::new();
        let mut procs = Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let drive = run_stages::drive_walk(&self.stages(), &mut queue, || false, || false, |line| {
            log.push(line.to_string());
        }, &mut procs);
        run_stages::say_outcome(&drive, |line| log.push(line.to_string()));
        log
    }

    /// Every `POST /v1/tasks/run` this fake answered.
    fn tasks(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .expect("the record lock")
            .iter()
            .filter(|seen| seen.line.contains("/v1/tasks/run"))
            .cloned()
            .collect()
    }
}

/// Lay out, walk and leave a scene under the one guard the env needs.
fn walk(scene: &Scene) -> Vec<String> {
    let _guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    scene.enter();
    scene.project();
    scene.skip_diarization();
    let log = scene.walk();
    scene.leave();
    log
}

/// §F1.5 S1 through the real door: an aligner named in the settings box that the server does not
/// declare for alignment is NOT replaced by one it does declare. The pass says why, names the server
/// that refused, asks the declared aligner nothing, and carries on to the end with the ASR's times.
///
/// Before this rule was wired, the `Err` from `align::aligners` was propagated with `?` and the whole
/// speech stage died with it — and the url handed to `align::not_served` was `""`, so the line that
/// did get printed named no server at all.
#[test]
fn f1_5_s1_a_configured_aligner_that_is_not_served_skips_alignment_and_carries_on() {
    // The box names `mms-aligner`; the fake declares `wav2vec-align` for `align`, because that is all
    // a real box can declare -- the refused id is the one that is NOT on its list.
    let scene = Scene::new_refusing(REFUSED_ALIGNER);
    let log = walk(&scene);

    let line = log
        .iter()
        .find(|line| line.contains(&format!("does not serve {REFUSED_ALIGNER:?}")))
        .unwrap_or_else(|| panic!("the run never said why alignment was skipped: {log:?}"));
    assert!(
        line.contains(&format!(
            "does not serve {REFUSED_ALIGNER:?} for alignment -- cut points come off the waveform"
        )),
        "{line}"
    );
    // The server is named by the address the conf holds, not left blank.
    assert!(line.contains(&scene.url), "{line} names no server -- the refusal must say which box");
    assert_eq!(
        line,
        &format!(
            "!!! align: {} does not serve {REFUSED_ALIGNER:?} for alignment -- cut points come off the waveform",
            scene.url
        ),
        "the line is S1's sentence, whole"
    );
    // The id is not replaced: the declared aligner was never asked.
    assert!(
        !scene.tasks().iter().any(|seen| seen.body.contains(DECLARED_ALIGNER)),
        "a configured id that is not served must not be swapped for one that is: {:?}",
        scene.tasks().iter().map(|seen| &seen.body).collect::<Vec<_>>()
    );
    // And the run is not a failure: the ASR's times stand, so `words.json` is there and the segments
    // were written. `Align::None` answers S4's `Quiet`, so no alignment warning is logged either.
    assert!(
        !log.iter().any(|line| line.contains("speech failed")),
        "a skipped aligner is a working setup, not a failed stage: {log:?}"
    );
    let tree = scene.tree();
    assert!(tree.words_json(LANE).exists(), "the ASR's words are on disk: {log:?}");
    assert!(!tree.words_aligned_json(LANE).exists(), "nothing was aligned, so no marker: {log:?}");
    assert!(tree.transcript_txt(LANE).exists(), "the segments were still written: {log:?}");
}

/// §F1.5 S6 through the real door: the warning about voiced seconds with no word over them lives in
/// the aligner's `Outcome`, and the door used to throw that `Outcome` away, so the one visible sign
/// that the times are wrong never reached a run's log.
///
/// The numbers, from `align::BARE_WARN` 10 s and `align::BARE_SHARE` 0.08: the take is 40 s, silent
/// 34.0-40.0, so voiced is 34 s and the share bound is 0.08 × 34 = 2.72 s. The fake answers each
/// window's word at 0.2 s into that window, and the windows S2 makes are the ASR's own chunks
/// (0.0-20.0, 20.0-40.0), so the word covers 0.2±0.25 s and 19.7-20.2 s and everything from 0.45 to
/// 19.7 and from 20.25 to 34.0 is bare — about 33 s of 34.
#[test]
fn f1_5_s6_the_bare_voiced_seconds_warning_reaches_the_run() {
    let scene = Scene::new("40.0", SILENCES, DECLARED_ALIGNER);
    scene.aligner_places_everything_at_the_start();
    let log = walk(&scene);

    let line = log
        .iter()
        .find(|line| line.contains("has no word over it"))
        .unwrap_or_else(|| panic!("S6's warning never reached the run's log: {log:?}"));
    assert!(line.starts_with("!!! align:"), "{line}");
    assert!(
        line.contains("the times are wrong, and the cut will drop that footage"),
        "{line}"
    );
    // Both bounds hold in the numbers printed: N ≥ 10 and N ≥ 8 % of M.
    let (bare, spoken) = align_bare_and_spoken(line);
    assert!(
        align::bare_warns(bare as f64, spoken as f64),
        "the printed numbers do not pass the bounds they were warned for: {line}"
    );
    assert!(
        scene.tree().words_aligned_json(LANE).exists(),
        "the aligner answered, so the marker is on disk: {log:?}"
    );
}

/// §F1.5 S1's other sentence, through the real door: the box names no aligner, the fake declares two,
/// and the first of the two (sorted by name, `aaa-refuser` first) refuses every window for want of
/// room while the second answers. `align::picked_log` says exactly that, and until this round its line
/// — like S6's — was thrown away with the `Outcome` the door was discarding.
#[test]
fn f1_5_s1_the_pickers_line_reaches_the_run_when_the_first_aligner_refuses() {
    // An empty box: the fake then declares two aligners and `align::aligners` sorts them by name, so
    // `aaa-refuser` is the one tried first and is the one that has no room.
    let scene = Scene::new("40.0", SILENCES, "");
    scene.aligner_has_no_room_once();
    let log = walk(&scene);

    let picked = log
        .iter()
        .find(|line| line.contains("answers where"))
        .unwrap_or_else(|| panic!("S1's picker line never reached the run's log: {log:?}"));
    assert_eq!(
        picked,
        &format!(">>> align: {DECLARED_ALIGNER} answers where {FIRST_TRIED_ALIGNER} does not -- using it for the rest of the run"),
        "the line names which one answered and which did not"
    );
    // The refusal that made it a walk is said too, in front of the line that resolves it.
    assert!(
        log.iter().any(|line| line.contains(FIRST_TRIED_ALIGNER) && line.contains("align:")),
        "the refusing aligner was not named: {log:?}"
    );
    // And the walk was not a failure despite the first aligner refusing every window.
    assert!(scene.tree().words_aligned_json(LANE).exists(), "the second aligner served: {log:?}");
}

/// The two numbers out of S6's printed sentence, which is `!!! align: N s of the M s spoken ...`.
fn align_bare_and_spoken(line: &str) -> (u64, u64) {
    let rest = line.split("!!! align: ").nth(1).expect("the prefix");
    let mut parts = rest.split(" s of the ");
    let bare = parts.next().expect("N").trim().parse().expect("N is a number");
    let spoken = parts.next().expect("M s spoken").split(" s").next().expect("M").trim().parse().expect("M");
    (bare, spoken)
}

/// The aligner the fake declares first by name, i.e. the first `align::aligners` asks when the settings
/// box names none, because the list is sorted by name after `services::ALIGN_PREFERENCE`. It is the one
/// that answers `CUDA out of memory`, so the picker has to walk on to `DECLARED_ALIGNER` — which is the
/// case `align::picked_log` announces, and why this name must sort BEFORE it ("aaa" < "wav2vec").
const FIRST_TRIED_ALIGNER: &str = "aaa-refuser";

/// The stub tools. `ffprobe` answers `duration`; `ffmpeg` logs every argv into `argv.log`, prints the
/// silencedetect pairs on STDERR where the real tool prints them, and answers an aligner clip's cut
/// with a `silence_end:` line whose number is that cut's START — see [`aligner_cut_start`]. Otherwise it
/// creates its last argument from its first input; a stub that only succeeded would let a cut that made
/// nothing count as work.
fn stubs(tag: &str, duration: &str, silences: &[(f64, f64)]) -> PathBuf {
    // A DISTINCT folder per call: a shared tag would have the next scene's `remove_dir_all` delete the
    // stubs a previous scene's conf still names.
    let dir = temp_dir_tagged(&format!("np-f15-tools-{tag}"));
    let probe = dir.join("ffprobe");
    common::write_script(&probe, format!("#!/bin/sh\necho \"{duration}\"\n")).expect("the probe stub");
    let report: String = silences
        .iter()
        .map(|(start, end)| format!("echo \"silence_start: {start}\" >&2\necho \"silence_end: {end}\" >&2\n"))
        .collect();
    let ffmpeg = dir.join("ffmpeg");
    // The decode's output is a REAL (silent) wav rather than a copy of the input: this stub has no audio
    // to decode, and §F1.11's envelope reader parses `voice16k.wav` for real, so placeholder bytes would
    // stop the row with "not a RIFF/WAVE file" before any aligner was asked.
    std::fs::write(dir.join("silence.wav"), silent_wav_bytes(20.0)).expect("the stub's silence");
    common::write_script(
        &ffmpeg,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{dir}/argv.log\"\ncase \"$*\" in\n  *silencedetect*)\n{report}  exit 0\n    ;;\nesac\nlast=''\nfor a in \"$@\"; do last=\"$a\"; done\nfirst=''\nseen=0\npending=0\nfor a in \"$@\"; do\n  if [ \"$a\" = \"-ss\" ]; then echo \"silence_end: $a\" > \"$last\".probe; fi\n  if [ \"$a\" = \"-i\" ]; then pending=1; continue; fi\n  if [ \"$pending\" = 1 ]; then pending=0; seen=$((seen+1)); [ \"$seen\" = 1 ] && first=\"$a\"; fi\ndone\nif [ -f \"{dir}/silence.wav\" ]; then cp \"{dir}/silence.wav\" \"$last\"; elif [ -f \"$first\" ]; then cp \"$first\" \"$last\"; else printf 'STUB-AUDIO' > \"$last\"; fi\n[ -f \"$last\" ]\n",
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

/// A minimal mono 16 kHz `pcm_s16le` wav of `seconds` of silence: a real RIFF header and zeroed data,
/// which is what a real decode leaves in `voice16k.wav` and what §F1.11's envelope reader needs.
fn silent_wav_bytes(seconds: f64) -> Vec<u8> {
    let data = vec![0u8; (seconds * 16_000.0) as usize * 2];
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // one channel
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// The fake audio.cpp: §1's three management/task endpoints, answered per path, and every task answer
/// chosen from the arms of the scene the request arrived on.
fn fake_audio() -> (String, u16, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let recorder = recorder.clone();
            std::thread::spawn(move || {
                if let Some(row) = serve(stream, port) {
                    if let Ok(mut held) = recorder.lock() {
                        held.push(row);
                    }
                }
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), port, seen)
}

fn serve(mut stream: TcpStream, port: u16) -> Option<Seen> {
    let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let request_line = line.trim_end().to_string();
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
    let text = String::from_utf8_lossy(&body).into_owned();
    let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let (code, payload) = route(port, &path, &text);
    let framed = format!(
        "HTTP/1.1 {code} Whatever\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(Seen { line: request_line, body: text })
}

/// Which reply a request gets. The task kinds are told apart by the model id their envelope names — the
/// envelope §5 fixes has no task field, so the id is the only thing on the wire that says which job was
/// asked, and the fake serves exactly one id per job.
fn route(port: u16, path: &str, body: &str) -> (u16, String) {
    // Which job this envelope asks for is decided by the model id it names, so the ids have to be told
    // apart before anything answers: `wav2vec-align` and `aaa-refuser` are both aligners, and a request
    // naming either is an alignment ask whatever else the body happens to contain.
    if path.contains("/v1/models") {
        let aligner = DECLARED_ALIGNER;
        let configured = scene_fakes(port)
            .map(|fakes| fakes.align_model.lock().expect("the arms lock").clone())
            .unwrap_or_default();
        let _ = &aligner;
        // A box with TWO aligners whenever none is configured, so the picker has a list to walk. Neither
        // is `services::ALIGN_PREFERENCE`, which would sort first and leave the refuser last of all.
        // The list the picker walks: both aligners only when the box named none (S1's unnamed case);
        // when it named one, the list holds that declared id alone so the OTHER cannot be asked for it.
        let extra = if configured.is_empty() {
            format!(",{{\"id\":\"{FIRST_TRIED_ALIGNER}\",\"task\":\"align\"}}")
        } else {
            String::new()
        };
        return (
            200,
            format!(
                "{{\"data\":[{{\"id\":\"{}\",\"family\":\"qwen3\",\"task\":\"asr\"}},{{\"id\":\"{aligner}\",\"task\":\"align\"}}{extra},{{\"id\":\"{}\",\"task\":\"diarize\"}}]}}",
                services::ASR_MODEL,
                services::DIAR_MODEL
            ),
        );
    }
    if path.contains("/v1/ui/upload") {
        return (200, "{\"path\":\"/uploads/talk.wav\"}".to_string());
    }
    if !path.contains("/v1/tasks/run") {
        return (404, "{\"error\":\"no such thing\"}".to_string());
    }
    let fakes = scene_fakes(port).expect("every request arrives at a scene that registered itself");
    if body.contains(services::DIAR_MODEL) {
        // Two voices, both longer than `diarize::ANCHOR_MIN_SECONDS` and inside the smallest rung's
        // window, which is the shape every rung of the ladder plans. `skip_diarization` normally means
        // this answer is never reached.
        return (200, diar_two_voices());
    }
    if body.contains(DECLARED_ALIGNER) || body.contains(FIRST_TRIED_ALIGNER) {
        // Every window is asked about the same clip name and the take has one trim, so the aligner's own
        // ask counter is what says which window this is; `align::ask_span` adds the window's start to the
        // seconds the answer names.
        let window = fakes.windows.fetch_add(1, Ordering::SeqCst);
        // The first-listed aligner has no room for its first ask, so the picker walks on to the other and
        // S1's picker line is said by the walk. Refusing every ask would leave no answer at all.
        if *fakes.align_refuses_first.lock().expect("the arms lock")
            && body.contains(FIRST_TRIED_ALIGNER)
            && fakes.refuser_asks.fetch_add(1, Ordering::SeqCst) == 0
        {
            return (500, "{\"error\":\"CUDA out of memory\"}".to_string());
        }
        return (200, aligner_words(*fakes.words_in_first_second.lock().expect("the arms lock"), window));
    }
    // The ASR: two words a piece, timed in the piece's own seconds, which `asr::shift` then moves.
    (200, asr_words(fakes.pieces.fetch_add(1, Ordering::SeqCst) % 2))
}

fn scene_fakes(port: u16) -> Option<SceneFakes> {
    SCENES
        .lock()
        .expect("the scene lock")
        .iter()
        .find(|(each, _)| *each == port)
        .map(|(_, fakes)| fakes.clone())
}

/// One ASR piece's answer. Two words per piece, two seconds per piece, so a 40 s take is two pieces
/// (`asr::chunk_limit` gives 60 s to the qwen3 family the fake declares) and their word counts line up
/// with the text the aligner is then asked to place.
fn asr_words(piece: usize) -> String {
    let base = piece * 2;
    format!(
        "{{\"text\":\"word{c} one{c}\",\"words\":[{{\"word\":\"word{c}\",\"start_sample\":{},\"end_sample\":{}}}]}}",
        base * 16000 + 1600,
        base * 16000 + 3200,
        c = piece
    )
}

/// An aligner's answer for one window, in that window's OWN seconds (`align::ask_span` adds the
/// window's start afterwards, and every window here is 20 s of the take). Armed by
/// `aligner_places_everything_at_the_start`, the word sits at 0.2-0.4 s of the clip; otherwise it
/// spans the clip, which is what makes a walk that is NOT about S6 measure no bare seconds at all.
fn aligner_words(bare: bool, window: usize) -> String {
    let from = (window % 2) as f64 * 20.0;
    if bare {
        return format!("{{\"words\":[{{\"word\":\"word\",\"start\":{},\"end\":{}}}]}}", from + 0.2, from + 0.4);
    }
    format!("{{\"words\":[{{\"word\":\"word\",\"start\":{},\"end\":{}}}]}}", from, from + 19.0)
}

/// The diarizer's ordinary answer, in case a scene leaves its resume marker out.
fn diar_two_voices() -> String {
    let edges = |from: f64| ((from * 16_000.0) as u64, ((from + 4.9) * 16_000.0) as u64);
    let (a0, a1) = edges(0.5);
    let (b0, b1) = edges(8.0);
    format!(
        "{{\"turns\":[{{\"start_sample\":{a0},\"end_sample\":{a1},\"speaker_id\":0}},\
         {{\"start_sample\":{b0},\"end_sample\":{b1},\"speaker_id\":1}}]}}"
    )
}

/// The silences both scenes are stated against, kept as one constant so the tests' comments and the
/// stub they feed cannot drift apart: 34 s of 40 voiced, so `align::BARE_SHARE` 0.08 asks for 2.72 s.
const SILENCES: &[(f64, f64)] = &[(34.0, 40.0)];
