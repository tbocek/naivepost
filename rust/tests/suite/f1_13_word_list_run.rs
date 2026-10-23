//! F1.13 The session's word list — the RUN path (spec/04-prepare.md).
//!
//! `tests/suite/session_word_list.rs` proves the list's arithmetic with closures handed to `word_list::list`.
//! This file is the half that was missing: the row on the walk. §F1.13 draws the build AFTER the fix pass
//! and BEFORE the marking pass reads the spelling, so what matters here is where the planner puts the row
//! and whether the real door reaches the files the stages wrote — the aligner's tokens (F1.5), the raw
//! transcript (F1.4) and `transcript.fixed.tsv` (F1.8) — rather than dressing words nothing produced.
//! S2's stray test is reached through the real wave producer (`wave_build::ensure`, F1.11's), not a
//! hand-written cache, because "timed at the end of the word before · logged" is only reachable once some
//! program path measures an envelope.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use naivepost::layout::Tree;
use naivepost::project::{Field, MarkingPass, Origin, Project, Source};
use naivepost::requests::{self, AlignedDoc, Word as DocWord};
use naivepost::run::{self, Subprocesses};
use naivepost::run_stages::{self, Stage, Task};
use naivepost::runqueue::{self, Queue};
use naivepost::textfmt::{self, Line};
use naivepost::transcribe;
use naivepost::word_list::{self as words, WORDS_TASK};

const LANE: &str = "lecture";

/// The env and the working folder belong to the PROCESS and cargo runs these tests as threads, so the
/// checks that move them do it one at a time (the rule `tests/suite/f1_3_run_path.rs` holds to).
static ENV: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// One scene: `<root>/session.naivepost` plus the settings-free environment the door needs. `run_task`
/// resolves the session off `startup::session_dir(cwd)`, so the check that drives the walk pins cwd and
/// holds the guard until after the drive.
struct Scene {
    /// Held so no other thread moves the process's working folder mid-drive; read only by `leave`'s caller
    /// dropping it, which is why the field is never otherwise named.
    #[allow(dead_code)]
    guard: MutexGuard<'static, ()>,
    tree: Tree,
}

impl Scene {
    fn enter(tag: &str) -> Self {
        let guard = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = std::env::temp_dir().join(format!(
            "f113-run-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let session = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(&session).expect("the session folder");
        std::env::set_current_dir(&root).expect("cwd is the scene root");
        Scene {
            guard,
            tree: Tree::new(&session).expect("a .naivepost folder is a project"),
        }
    }

    fn leave(&self) {
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("back to the crate folder");
    }

    /// Plant the recording row inside the project, so its stored path is `project:sources/…` like a real
    /// session's, and return the source for the project file.
    fn plant_source(&self, name: &str, bytes: usize) -> Source {
        let path = self.tree.sources_dir().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![7u8; bytes]).unwrap();
        Source {
            path: format!("project:sources/{name}"),
            footage: true,
            narrator: 0,
            sepvoice: false,
            tracks: vec![],
        }
    }

    /// Write the project file the door loads back.
    fn save_project(&self, project: &Project) {
        naivepost::project::save(project, self.tree.dir()).expect("the project file");
    }

    /// Drive ONLY the word-list row through the real walk. Returns the log lines, newest last; panics if
    /// the walk did not finish, since a failed row is a bug rather than a state to assert on.
    fn drive_row(&self) -> Vec<String> {
        let (drive, log) = self.drive_row_open();
        assert!(
            matches!(drive, run_stages::Drive::Done { .. }),
            "the row finished: {drive:?} / log {log:?}"
        );
        log
    }

    /// The same drive without an expectation about how it ended.
    fn drive_row_open(&self) -> (run_stages::Drive, Vec<String>) {
        let stage = Stage {
            track: runqueue::TRACK_FIX,
            name: "fix",
            phase: 2,
            of: 2,
            kind: "chunk",
            tasks: vec![words_row()],
        };
        let mut queue = Queue::new();
        let mut procs = Subprocesses::default();
        let mut log: Vec<String> = Vec::new();
        let drive = run_stages::drive_walk(
            &[stage],
            &mut queue,
            || false,
            || false,
            |line| log.push(line.to_string()),
            &mut procs,
        );
        (drive, log)
    }
}

/// A policy that names a marking pass, which is what makes the row exist at all.
fn policy_pass(pass: MarkingPass) -> naivepost::project::Policy {
    naivepost::project::Policy {
        marking_pass: Field {
            value: pass,
            origin: Origin::User,
            because: None,
        },
        ..Default::default()
    }
}

/// The row under test, for a session that has the recording it plants.
fn words_row() -> Task {
    words::row(&Project {
        sources: vec![Source {
            path: "project:sources/lecture.mkv".into(),
            footage: true,
            ..Default::default()
        }],
        policy: policy_pass(MarkingPass::Retakes),
        ..Project::default()
    })
    .expect("a Retakes policy with a source plans a word-list row")
}

/// Which row each task is, by its program marker, in plan order.
fn programs(tasks: &[Task]) -> Vec<&'static str> {
    tasks
        .iter()
        .map(|task| {
            if task.program == WORDS_TASK {
                "words"
            } else if task.program == run_stages::TEXT_TASK {
                "fix"
            } else if task.program == naivepost::fix_session::PLACEMENT_TASK {
                "place"
            } else if task.program == naivepost::fix_session::SESSION_TASK {
                "merge"
            } else {
                "other"
            }
        })
        .collect()
}

/// Hand the aligned doc F1.13 reads first: word timings in 16 kHz samples off the source's own start.
fn write_aligned(tree: &Tree, lane: &str, pairs: &[(&str, f64, f64)]) {
    let hz = transcribe::SAMPLE_RATE as f64;
    let docs = pairs
        .iter()
        .map(|(word, start, end)| DocWord {
            word: (*word).to_string(),
            start_sample: (start * hz) as u64,
            end_sample: (end * hz) as u64,
        })
        .collect();
    requests::write_aligned(tree, lane, &AlignedDoc { words: docs }).unwrap();
}

/// Plant `transcript.txt`, the raw transcript S3 dresses from.
fn write_raw_transcript(tree: &Tree, lane: &str, text: &str) {
    let path = tree.transcript_txt(lane);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{text}\n")).unwrap();
}

/// Plant the fix pass's answer for one line, through the writer F1.8 uses.
fn write_fixed_line(tree: &Tree, lane: &str, start: f64, end: f64, text: &str) {
    let lines = [Line {
        start,
        end,
        speaker: "SPEAKER_00".into(),
        text: text.into(),
    }];
    textfmt::write_lines(&lines, &tree.transcript_fixed_tsv(lane)).expect("the fixed file");
}

/// A mono 16 kHz s16 wav — the shape `decode_audio` leaves behind and `wave_build` reads.
fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let data = samples.len() * 2;
    let mut bytes = Vec::with_capacity(44 + data);
    let put = |bytes: &mut Vec<u8>, value: u32| bytes.extend_from_slice(&value.to_le_bytes());
    let put16 = |bytes: &mut Vec<u8>, value: u16| bytes.extend_from_slice(&value.to_le_bytes());
    bytes.extend_from_slice(b"RIFF");
    put(&mut bytes, 36 + data as u32);
    bytes.extend_from_slice(b"WAVEfmt ");
    put(&mut bytes, 16);
    put16(&mut bytes, 1);
    put16(&mut bytes, 1);
    put(&mut bytes, 16000);
    put(&mut bytes, 32000);
    put16(&mut bytes, 2);
    put16(&mut bytes, 16);
    bytes.extend_from_slice(b"data");
    put(&mut bytes, data as u32);
    for sample in samples {
        put16(&mut bytes, sample.unsigned_abs());
    }
    bytes
}

/// `seconds` of samples at a constant amplitude.
fn tone(seconds: f64, amplitude: i16) -> Vec<i16> {
    vec![amplitude; (seconds * transcribe::SAMPLE_RATE as f64) as usize]
}

// --- S7: the row on the walk ----------------------------------------------------------------------

/// §F1.13's place in the pipeline, read off the planner rather than asserted about a hand-made list: the
/// word list comes after the fix rows (it re-dresses from their `transcript.fixed.tsv`) and before the
/// merge row, which is the row that runs the marking pass and shows the model this very spelling.
#[test]
fn f1_13_s7_the_fix_stage_plans_a_word_list_row_after_the_fix_rows() {
    let project = Project {
        sources: vec![Source {
            path: "project:sources/lecture.mkv".into(),
            footage: true,
            ..Default::default()
        }],
        policy: policy_pass(MarkingPass::Retakes),
        ..Project::default()
    };

    let order = programs(&naivepost::fix_call::stage(&project).tasks);
    let words_at = order
        .iter()
        .position(|kind| *kind == "words")
        .expect("the fix stage plans the row");
    let fix_last = order
        .iter()
        .rposition(|kind| *kind == "fix")
        .expect("the per-source fix rows are there too");
    let merge_at = order
        .iter()
        .position(|kind| *kind == "merge")
        .expect("the merge row is there");
    assert_eq!(
        order[0], "place",
        "S1's placement row still goes first: {order:?}"
    );
    assert!(
        fix_last < words_at,
        "the list is dressed from the fix rows' output: {order:?}"
    );
    assert!(
        words_at < merge_at,
        "the marking pass must find the list already written: {order:?}"
    );

    // And the real Prepare plan carries it, inside the stage whose bar slice is Understand.
    let stages = run_stages::plan(run::Step::Prepare, &project);
    let fix = stages
        .iter()
        .find(|stage| stage.name == "fix")
        .expect("the fix stage is planned");
    assert!(
        fix.tasks.iter().any(|task| task.program == WORDS_TASK),
        "the walked plan holds the row: {:?}",
        programs(&fix.tasks)
    );
    assert_eq!(
        run_stages::phase_of_stage("fix"),
        Some(naivepost::prepare_run::Phase::Understand),
        "and it stays in the understand phase §S6 names"
    );
}

/// No marking pass means nothing reads the list, so the stage plans no row at all rather than a task that
/// would sit on the bar doing nothing.
#[test]
fn f1_13_s7_a_policy_with_no_marking_pass_plans_no_row() {
    let project = Project {
        sources: vec![Source {
            path: "project:sources/lecture.mkv".into(),
            footage: true,
            ..Default::default()
        }],
        policy: policy_pass(MarkingPass::None),
        ..Project::default()
    };
    assert!(
        words::row(&project).is_none(),
        "MarkingPass::None plans nothing"
    );
    let order = programs(&naivepost::fix_call::stage(&project).tasks);
    assert!(!order.contains(&"words"), "{order:?}");
    for stage in run_stages::plan(run::Step::Prepare, &project) {
        assert!(
            !stage.tasks.iter().any(|task| task.program == WORDS_TASK),
            "the walked plan has no word-list row under stage {}",
            stage.name
        );
    }
}

/// The door over a session whose stages really ran: aligned tokens, the raw transcript and the fix pass's
/// line all planted as the stages write them, then driven through `run_stages::drive_walk`. The saved list
/// is what §F1.13 promises: every word, the fix pass's spelling on the first word of the run, and the
/// seconds the aligner gave unchanged.
#[test]
fn f1_13_s7_the_walk_builds_the_list_from_the_files_the_stages_wrote() {
    let scene = Scene::enter("build");
    let source = scene.plant_source("lecture.mkv", 4096);
    scene.save_project(&Project {
        sources: vec![source],
        policy: policy_pass(MarkingPass::Retakes),
        ..Project::default()
    });
    write_aligned(
        &scene.tree,
        LANE,
        &[
            ("hello", 0.0, 0.4),
            ("world", 0.4, 0.8),
            ("again", 0.8, 1.2),
            ("here", 1.2, 1.6),
        ],
    );
    // The raw transcript S3 dresses from is deliberately the bare tokens here: the dressing rule itself is
    // `tests/suite/session_word_list.rs`'s S3 case. What this scene proves is that the door reads the three files
    // the stages wrote off their real paths — a wrong lane or a wrong folder leaves the words undressed and
    // the list without the fix pass's line, both asserted below.
    write_raw_transcript(&scene.tree, LANE, "Hello, world again here.");
    // The fix pass's answer for the whole take: one line covering all four words, which is what lets S4
    // fold its first two into the run's first word and empty the rest.
    write_fixed_line(&scene.tree, LANE, 0.0, 1.7, "hello world again here");

    let log = scene.drive_row();
    scene.leave();

    assert!(
        log.iter()
            .any(|line| line.starts_with(">>> words: 4 words in the session list")),
        "the build line: {log:?}"
    );
    assert!(
        scene.tree.session_word_list_json().exists(),
        "the press left the list behind"
    );
    let list = words::load(&scene.tree)
        .expect("readable")
        .expect("written");
    assert_eq!(list.len(), 4, "every aligned token made a word");
    // The line is printed whole on the run's first word and the rest emptied (§F1.13 S4), which is what
    // `joins` later matches its answer against — one row of text per line the fix pass answered.
    assert_eq!(
        words::written(&list)[0],
        "hello world again here",
        "the fix pass's spelling landed on the list"
    );
    assert_eq!(
        list[0].match_word, "hello",
        "matching never follows the dressing"
    );
    assert_eq!(
        list[1].match_word, "world",
        "an emptied word still holds its seconds"
    );
    // Times never change, only the written form (§F1.13's own sentence).
    assert_eq!((list[0].start, list[0].end), (0.0, 0.4));
    assert_eq!((list[3].start, list[3].end), (1.2, 1.6));
    assert!(
        list.iter().all(|word| !word.stray),
        "nothing here sits on no sound"
    );
}

/// Nothing transcribed yet is NOT a reason to write a list: the file is this step's resume marker, so an
/// empty one would make every later ▶ answer "already built -- 0 words" and the session would never get
/// the real list. The row says what is missing and leaves the marker unwritten.
#[test]
fn f1_13_s7_a_session_with_nothing_transcribed_yet_writes_no_list() {
    let scene = Scene::enter("empty");
    let source = scene.plant_source("lecture.mkv", 4096);
    scene.save_project(&Project {
        sources: vec![source],
        policy: policy_pass(MarkingPass::Retakes),
        ..Project::default()
    });

    let log = scene.drive_row();
    scene.leave();

    assert!(
        log.iter()
            .any(|line| line.contains("nothing transcribed") && line.starts_with("!!!")),
        "the refusal names why: {log:?}"
    );
    assert!(
        !scene.tree.session_word_list_json().exists(),
        "no list, so the next run still builds one"
    );
}

/// The saved file IS the evidence the step ran (spec/00-principles): a second walk resumes off it instead
/// of re-gluing over it.
#[test]
fn f1_13_s7_a_second_walk_resumes_off_the_saved_list() {
    let scene = Scene::enter("resume");
    let source = scene.plant_source("lecture.mkv", 4096);
    scene.save_project(&Project {
        sources: vec![source],
        policy: policy_pass(MarkingPass::Retakes),
        ..Project::default()
    });
    write_aligned(
        &scene.tree,
        LANE,
        &[
            ("hello", 0.0, 0.4),
            ("world", 0.4, 0.8),
            ("again", 0.8, 1.2),
            ("here", 1.2, 1.6),
        ],
    );
    write_raw_transcript(&scene.tree, LANE, "Hello, world again here.");

    let first = scene.drive_row();
    let bytes = std::fs::read(scene.tree.session_word_list_json()).unwrap();
    assert!(
        first
            .iter()
            .any(|line| line.starts_with(">>> words: 4 words in the session list")),
        "{first:?}"
    );

    // Change what the stages wrote AFTER the list exists: the resume must not reach it.
    write_fixed_line(
        &scene.tree,
        LANE,
        0.0,
        1.7,
        "completely different words now",
    );
    let second = scene.drive_row();
    scene.leave();

    assert_eq!(second.len(), 1, "a resume says one thing: {second:?}");
    assert!(
        second[0].starts_with(">>> words: session list already built -- 4 words"),
        "{}",
        second[0]
    );
    assert_eq!(
        std::fs::read(scene.tree.session_word_list_json()).unwrap(),
        bytes,
        "resuming leaves the file byte-for-byte alone"
    );
}

// --- S2: the stray test off a measured envelope ---------------------------------------------------

/// S2's branch through the REAL producer: `wave_build::ensure` folds `voice16k.wav` into the cache that
/// `run_stages::dial_task` writes on a live run, and the word list then finds the late word on no sound.
/// Before F1.11 landed nothing wrote that cache, so this branch could not fire outside a unit test.
#[test]
fn f1_13_s2_the_wave_cache_a_prepare_run_measures_is_what_finds_the_stray() {
    let scene = Scene::enter("stray");
    let source = scene.plant_source("lecture.mkv", 4096);
    scene.save_project(&Project {
        sources: vec![source],
        policy: policy_pass(MarkingPass::Retakes),
        ..Project::default()
    });
    // Loud for the first second, near-silent from 4.5 s: the breath "Geld." was put on.
    let mut samples = tone(1.0, 12000);
    samples.extend(tone(4.5, 20));
    let decoded = scene.tree.voice16k_wav(LANE);
    std::fs::create_dir_all(decoded.parent().unwrap()).unwrap();
    std::fs::write(&decoded, wav_bytes(&samples)).expect("the decoded audio");
    write_aligned(
        &scene.tree,
        LANE,
        &[("geld", 0.0, 0.4), ("und", 0.4, 0.8), ("spat", 5.0, 5.4)],
    );

    // The producer itself, at the same lane key the edge rules read it back with. Its stamp is the
    // recording's own size/mtime, which is what `wave::read` accepts the cache against.
    let recording: &Path = &scene.tree.sources_dir().join("lecture.mkv");
    naivepost::wave_build::ensure(&scene.tree, LANE, recording, &mut |_| ())
        .expect("the envelope is measured");
    assert!(
        scene.tree.wave(LANE).exists(),
        "the cache is where the reader looks"
    );

    let log = scene.drive_row();
    scene.leave();

    let list = words::load(&scene.tree)
        .expect("readable")
        .expect("written");
    let stray = list
        .iter()
        .find(|word| word.stray)
        .expect("the late word was retimed");
    assert_eq!(stray.match_word, "spat");
    assert_eq!(
        (stray.start, stray.end),
        (0.8, 0.8),
        "timed at the end of the word before it"
    );
    let line = log
        .iter()
        .find(|line| line.starts_with(">>> words: \u{201c}"))
        .unwrap_or_else(|| panic!("S2's sentence is missing: {log:?}"));
    assert_eq!(
        *line,
        ">>> words: \u{201c}spat\u{201d} was placed on almost no sound, well after the word before it \
         -- timed at the end of that word instead (00:00)",
        "spec wording, quoted"
    );
    assert!(
        log.iter()
            .any(|line| line.starts_with(">>> words: 3 words in the session list, 1 retimed")),
        "the summary counts what moved: {log:?}"
    );
}
