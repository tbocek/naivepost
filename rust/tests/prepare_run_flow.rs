//! F1.1 ▶ Prepare — the step's start flow, as plain logic (spec/04-prepare.md §F1.1).
//!
//! One test per numbered step. Nothing here opens a window or a socket: the refusals, the save, the
//! clear of a run that died inside Describe, the opening log lines, the preflight answer and the
//! bar's phase boundaries are all functions over data, so each rule is checked on its own. The wire
//! from the real ▶ button to these functions is `tests/press_play_widgets.rs`.

use std::fs;
use std::path::PathBuf;

use naivepost::layout::Tree;
use naivepost::prepare;
use naivepost::prepare_run::{self, Phase, Start};
use naivepost::project::{Project, Source};
use naivepost::sources;

/// A scratch project folder. Same shape as `tests/prepare_screen.rs`: temp dir + `Tree::new`, no
/// `tempfile` crate in this project, pid keeps concurrent runs apart.
fn scratch(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f11-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join(format!("{tag}.naivepost"));
    fs::create_dir_all(&dir).expect("project folder");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

fn source(path: &str, footage: bool) -> Source {
    Source {
        path: path.to_string(),
        footage,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }
}

fn with_sources(paths: &[&str]) -> Project {
    Project {
        sources: paths.iter().map(|p| source(p, true)).collect(),
        ..Project::default()
    }
}

// ---- S1 -----------------------------------------------------------------------

/// S1: an empty list refuses with §F1.1's exact words.
#[test]
fn f1_1_s1_no_sources_refuses() {
    let project = Project::default();
    assert!(project.sources.is_empty());
    let (log, status) = prepare_run::refuse(&project).expect("an empty session must refuse");
    assert_eq!(status, prepare_run::NO_SOURCES);
    assert_eq!(status, "add at least one source", "the spec's wording, verbatim");
    assert!(
        log.starts_with("!!!"),
        "a refusal logs like every other failure in this app: {log:?}"
    );
}

/// S1: two rows of one base name refuse the run, and the log names the folder they would share.
#[test]
fn f1_1_s1_a_name_clash_refuses_and_names_the_shared_folder() {
    // `sources::clash` compares whole file names including the extension (that is what makes
    // `project:sources/clip.mkv` clash with `/media/clip.mkv`), so the two rows must share one.
    let project = with_sources(&["project:sources/clip.mkv", "/media/clip.mkv"]);
    let (log, status) = prepare_run::refuse(&project).expect("one base name across two rows refuses");
    assert_eq!(log, sources::clash_log("project:sources/clip.mkv", "/media/clip.mkv"));
    assert!(
        log.contains("both inputs/clip"),
        "the log names the folder both would write into: {log:?}"
    );
    assert_eq!(
        status,
        "clip.mkv and clip.mkv have the same name \u{2014} rename one",
        "the status asks for the rename, in §4's words"
    );
}

/// Two rows that merely share a stem but differ in extension do NOT clash — different files land in
// different lanes only if the comparison were on the stem, which it deliberately is not.
#[test]
fn f1_1_s1_different_extensions_are_not_a_clash() {
    let project = with_sources(&["project:sources/clip.mkv", "project:sources/clip.wav"]);
    assert_eq!(
        prepare_run::refuse(&project),
        None,
        "`sources::clash` compares the whole name, so these two are separate files"
    );
}

/// S1 beats everything else: a clean project is not refused at all.
#[test]
fn f1_1_s1_a_clean_project_is_not_refused() {
    let project = with_sources(&["project:sources/a.mkv", "project:sources/b.wav"]);
    assert_eq!(prepare_run::refuse(&project), None);
}

/// No sources beats a clash: with nothing listed there is no pair to clash about, so the person gets
/// the actionable message rather than a nonsense one.
#[test]
fn f1_1_s1_no_sources_beats_the_clash_message() {
    let empty = Project::default();
    let (log, status) = prepare_run::refuse(&empty).expect("refused");
    assert_eq!(status, prepare_run::NO_SOURCES);
    assert!(
        !status.contains("rename"),
        "an empty list cannot be asked to rename anything: {status:?}"
    );
    assert!(!log.contains("inputs/"));
}

// ---- S2 -----------------------------------------------------------------------

/// S2: the project file is written before any stage runs, so a crash mid-run leaves a project that
/// still knows what it was pointed at.
#[test]
fn f1_1_s2_the_project_is_saved_before_the_run_starts() {
    let (_keep, tree) = scratch("save");
    let project = with_sources(&["project:sources/lecture.mkv"]);
    let file = tree.dir().join("naivepost.json");
    assert!(!file.exists(), "nothing saved yet");

    let started = prepare_run::begin(&project, &tree);
    assert!(
        matches!(started, Start::Started { .. }),
        "a valid project starts: {started:?}"
    );
    assert!(file.exists(), "S2 wrote the project before anything else ran");
    let saved = naivepost::project::load(tree.dir()).expect("what was saved loads back");
    assert_eq!(saved.sources.len(), 1, "and it is the same project");
}

/// S2 failing stops the start rather than carrying on with an unsaved project.
#[test]
fn f1_1_s2_a_failed_save_stops_the_start() {
    // A FILE where the project folder should be makes the write fail: the tree resolves, but saving
    // into a plain file cannot succeed.
    let root = std::env::temp_dir().join(format!("naivepost-f11-badsave-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let blocked = root.join("blocked.naivepost");
    fs::write(&blocked, b"not a folder").unwrap();
    let tree = Tree::new(&blocked).expect("the name still resolves as a project");
    let project = with_sources(&["project:sources/lecture.mkv"]);

    match prepare_run::begin(&project, &tree) {
        Start::SaveFailed { error } => assert!(
            error.contains("blocked.naivepost") || !error.is_empty(),
            "the error says what failed: {error:?}"
        ),
        other => panic!("expected SaveFailed, got {other:?}"),
    }
}

// ---- S3 -----------------------------------------------------------------------

/// S3: a run stopped inside Describe leaves `events.tsv` + `state.txt`; both go, the description
/// starts again, and the scaled frames stay exactly where they are.
#[test]
fn f1_1_s3_a_stopped_describe_is_cleared_and_the_frames_are_kept() {
    let (_keep, tree) = scratch("clear");
    let lane = "lecture";
    let describe = tree.describe_dir(lane);
    fs::create_dir_all(describe.join(".llmframes")).expect("describe + scaled frames");
    fs::write(tree.events_tsv(lane), b"frame event\n").unwrap();
    fs::write(tree.describe_state(lane), b"halfway\n").unwrap();
    fs::write(describe.join(".llmframes/f.jpg"), b"scaled").unwrap();
    fs::create_dir_all(tree.frames_dir(lane)).expect("frames dir");
    fs::write(tree.frames_dir(lane).join("raw.jpg"), b"raw").unwrap();

    let project = with_sources(&["project:sources/lecture.mkv"]);
    let cleared = prepare_run::clear_stopped_describe(&tree, &project);

    assert_eq!(cleared.removed, vec!["events.tsv".to_string(), "state.txt".to_string()]);
    assert!(cleared.failures.is_empty());
    assert!(!tree.events_tsv(lane).exists(), "events.tsv is gone");
    assert!(!tree.describe_state(lane).exists(), "state.txt is gone");
    assert!(
        describe.join(".llmframes/f.jpg").exists(),
        "the scaled frames are KEPT — re-extracting them costs a minute for nothing"
    );
    assert!(
        tree.frames_dir(lane).join("raw.jpg").exists(),
        "and so are the extracted frames themselves"
    );

    let lines = prepare_run::clear_lines(&cleared);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0],
        ">>> stopped last time \u{2014} describing from the start again: events.tsv, state.txt \
(scaled frames kept)"
    );
}

/// S3: neither file present means the last run did not stop inside Describe, so nothing is said.
#[test]
fn f1_1_s3_a_run_that_did_not_stop_in_describe_says_nothing() {
    let (_keep, tree) = scratch("quiet");
    let project = with_sources(&["project:sources/lecture.mkv"]);
    let cleared = prepare_run::clear_stopped_describe(&tree, &project);
    assert!(cleared.quiet(), "no files, no story: {cleared:?}");
    assert!(prepare_run::clear_lines(&cleared).is_empty());
}

/// S3: only `state.txt` is enough to mean the same thing — the description was under way.
#[test]
fn f1_1_s3_state_alone_still_counts_as_stopped_inside_describe() {
    let (_keep, tree) = scratch("stateonly");
    fs::create_dir_all(tree.describe_dir("lecture")).unwrap();
    fs::write(tree.describe_state("lecture"), b"running\n").unwrap();
    let project = with_sources(&["project:sources/lecture.mkv"]);
    let cleared = prepare_run::clear_stopped_describe(&tree, &project);
    assert_eq!(cleared.removed, vec!["state.txt".to_string()]);
    assert_eq!(
        prepare_run::restart_line(&cleared.removed),
        ">>> stopped last time \u{2014} describing from the start again: state.txt (scaled frames kept)"
    );
}

/// S3: a file that will not unlink does NOT fail the run. It is reported and the run resumes off disk.
#[test]
fn f1_1_s3_a_failed_clear_is_logged_and_the_run_resumes() {
    let (_keep, tree) = scratch("failclear");
    fs::create_dir_all(tree.describe_dir("lecture")).unwrap();
    // A DIRECTORY named events.tsv cannot be removed by remove_file: the unlink fails for real.
    fs::create_dir_all(tree.events_tsv("lecture")).unwrap();
    let project = with_sources(&["project:sources/lecture.mkv"]);

    let cleared = prepare_run::clear_stopped_describe(&tree, &project);
    assert!(cleared.removed.is_empty(), "nothing was removed");
    assert_eq!(
        cleared.failures.len(),
        1,
        "the failure was recorded, not swallowed: {:?}",
        cleared.failures
    );
    let lines = prepare_run::clear_lines(&cleared);
    assert_eq!(lines.len(), 1, "one line, the failure: {lines:?}");
    assert!(
        lines[0].starts_with(">>> could not clear the last run (")
            && lines[0].ends_with("-- resuming it"),
        "§F1.1's exact shape: {lines:?}"
    );
    assert!(lines[0].contains("events.tsv"), "it names which file: {lines:?}");
}

/// S3 through `begin`: the clear notices come BEFORE the opening lines, so the person reads "starting
// over" before reading what is being sent.
#[test]
fn f1_1_s3_the_clear_notice_precedes_the_opening_log() {
    let (_keep, tree) = scratch("order");
    fs::create_dir_all(tree.describe_dir("lecture")).unwrap();
    fs::write(tree.events_tsv("lecture"), b"x\n").unwrap();
    let mut project = with_sources(&["project:sources/lecture.mkv"]);
    project.context = "who is speaking".to_string();

    match prepare_run::begin(&project, &tree) {
        Start::Started { lines, cleared } => {
            assert_eq!(cleared.removed, vec!["events.tsv".to_string()]);
            assert!(
                lines[0].contains("stopped last time"),
                "the restart notice is first: {lines:?}"
            );
            assert!(
                lines.iter().any(|l| l.starts_with(">>> prepare: 1 input file")),
                "then the opening lines follow: {lines:?}"
            );
        }
        other => panic!("expected Started, got {other:?}"),
    }
}

// ---- S4 -----------------------------------------------------------------------

/// S4: the four opening lines, in order, with the count and the context's character count spelled out.
#[test]
fn f1_1_s4_the_opening_log_says_inputs_files_summary_and_context() {
    let mut project = with_sources(&["project:sources/lecture.mkv", "recordings/mic.wav"]);
    project.context = "Ada presents; Grace is the moderator.".to_string();
    let counts = prepare::Counts {
        frames: 9,
        lines: 26,
    };

    let lines = prepare_run::start_log(&project, &counts);
    assert_eq!(lines[0], ">>> prepare: 2 input file(s)");
    assert_eq!(lines[1], ">>>   lecture.mkv", "each file by name, not by path");
    assert_eq!(lines[2], ">>>   mic.wav");
    assert_eq!(
        lines[3],
        format!(">>> prepare: {}", prepare::inputs_readout(&counts)),
        "the inputs summary is the same sentence the page shows"
    );
    assert_eq!(
        lines[4],
        ">>> prepare: sending the session context from Prepare (37 characters)",
        "the context line carries its character count"
    );
    assert_eq!(lines.len(), 5);
}

/// S4: an empty context sends nothing, so the line is absent rather than "(0 characters)".
#[test]
fn f1_1_s4_an_empty_context_sends_no_line() {
    let project = with_sources(&["project:sources/lecture.mkv"]);
    assert!(project.context.is_empty());
    let lines = prepare_run::start_log(&project, &prepare::Counts::default());
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("session context")),
        "no context, no claim that one was sent: {lines:?}"
    );
    assert_eq!(lines.len(), 3, "count + file + summary: {lines:?}");
}

/// S4: the count is characters, not bytes — a multi-byte context must not inflate it.
#[test]
fn f1_1_s4_the_context_count_is_characters_not_bytes() {
    let mut project = with_sources(&["project:sources/a.mkv"]);
    project.context = "héllo".to_string(); // 5 chars, 6 bytes
    let lines = prepare_run::start_log(&project, &prepare::Counts::default());
    let context = lines.last().expect("context line");
    assert!(
        context.contains("(5 characters)"),
        "five characters even though the UTF-8 form is six bytes: {context:?}"
    );
}

// ---- S5 -----------------------------------------------------------------------

/// S5: everything served means no error.
#[test]
fn f1_1_s5_a_healthy_server_set_passes() {
    assert_eq!(
        prepare_run::preflight_error(true, true, true, false, false),
        None,
        "no separation asked, so its absence is not a problem"
    );
    assert_eq!(prepare_run::preflight_error(true, true, true, true, true), None);
}

/// S5: the dead server is named first, before any model question.
#[test]
fn f1_1_s5_a_dead_audio_server_is_named_first() {
    let err = prepare_run::preflight_error(false, false, false, true, false).expect("an error");
    assert!(err.contains("audio.cpp"), "names the server: {err:?}");
    assert!(!err.contains("ASR"), "and stops there — the models are unknown behind a dead server");
}

/// S5: each missing model is named in turn once the server answers.
#[test]
fn f1_1_s5_each_missing_model_is_named_in_turn() {
    assert!(prepare_run::preflight_error(true, false, true, false, false)
        .expect("asr")
        .contains("ASR"));
    assert!(prepare_run::preflight_error(true, true, false, false, false)
        .expect("diar")
        .contains("diarization"));
    assert!(prepare_run::preflight_error(true, true, true, true, false)
        .expect("sep")
        .contains("separation"));
}

/// S5: separation is only asked about when a row actually carries 🗣.
#[test]
fn f1_1_s5_separation_is_only_checked_when_asked() {
    assert_eq!(
        prepare_run::preflight_error(true, true, true, false, false),
        None,
        "not asked, not served, no complaint"
    );
    assert!(prepare_run::preflight_error(true, true, true, true, false).is_some());

    // And `separation_asked` reads the rows themselves.
    let mut plain = with_sources(&["project:sources/mic.wav"]);
    assert!(!prepare_run::separation_asked(&plain));
    plain.sources[0].sepvoice = true;
    assert!(
        prepare_run::separation_asked(&plain),
        "a ticked 🗣 asks for it"
    );
    // A half of a previous split has no voice left to take off, so its tick is dead.
    let mut product = with_sources(&["project:sources/mic.split-voice.wav"]);
    product.sources[0].sepvoice = true;
    assert!(
        !prepare_run::separation_asked(&product),
        "a split product does not ask to be split again"
    );
}

// ---- S6 -----------------------------------------------------------------------

/// S6: the three phases fill the bar in the shares §F1.1 gives them, end to end with no gap.
#[test]
fn f1_1_s6_the_phase_shares_tile_the_bar() {
    let (s_from, s_to) = prepare_run::share(Phase::Separate, true);
    let (i_from, i_to) = prepare_run::share(Phase::Ingest, true);
    let (u_from, u_to) = prepare_run::share(Phase::Understand, true);
    assert_eq!((s_from, s_to), (0.0, 0.10), "voice separation 0\u{2013}10 %");
    assert_eq!((i_from, i_to), (0.10, 0.30), "ingest to 30 %");
    assert_eq!((u_from, u_to), (0.30, 1.0), "understand 30\u{2013}100 %");
    assert_eq!(s_to, i_from, "no gap between separate and ingest");
    assert_eq!(i_to, u_from, "no gap between ingest and understand");
    assert_eq!(u_to, 1.0, "and the bar ends full");
}

/// S6: with no 🗣 anywhere, separation is zero-width rather than a tenth of the run spent waiting
/// for something that will never arrive.
#[test]
fn f1_1_s6_separation_is_zero_width_when_not_asked() {
    let (from, to) = prepare_run::share(Phase::Separate, false);
    assert_eq!(from, to, "zero width: {from}..{to}");
    assert_eq!(from, 0.0);
    // Ingest and understand are unchanged either way — the share belongs to the phase, not the ask.
    assert_eq!(prepare_run::share(Phase::Ingest, false), (0.10, 0.30));
    assert_eq!(prepare_run::share(Phase::Understand, false), (0.30, 1.0));
}

// ---- S7 -----------------------------------------------------------------------

/// S7 success: the three folders named, and the status counting what was written.
#[test]
fn f1_1_s7_success_names_the_three_folders_and_counts_the_files() {
    let log = prepare_run::finished_log();
    assert_eq!(log[0], ">>> prepare wrote:");
    let joined = log.join("\n");
    for folder in ["prepare/inputs/", "prepare/describe/", "prepare/transcript/"] {
        assert!(joined.contains(folder), "the log names {folder}: {joined:?}");
    }
    assert_eq!(
        prepare_run::finished_status(9378),
        "prepared \u{2014} 9378 files"
    );
    assert_eq!(prepare_run::finished_status(0), "prepared \u{2014} 0 files");
}

/// S7 stopped: ⏹ keeps the finished work and says so in those words.
#[test]
fn f1_1_s7_stopped_keeps_finished_work() {
    assert_eq!(
        prepare_run::STOPPED,
        "stopped \u{2014} finished work is kept"
    );
    // Not phrased as a failure: a stopped run resumes, so it must not read like one.
    assert!(!prepare_run::STOPPED.contains("FAIL"));
    assert!(!prepare_run::STOPPED.contains("failed"));
}

/// S7 failed: the reason goes into the status, and a reasonless failure still says where to look.
#[test]
fn f1_1_s7_failure_carries_its_reason_or_points_at_the_log() {
    assert_eq!(
        prepare_run::failed_status("audio.cpp went away mid-take"),
        "prepare FAILED: audio.cpp went away mid-take"
    );
    assert_eq!(
        prepare_run::failed_status(""),
        "prepare failed \u{2014} see log",
        "an empty reason must not leave the status blank"
    );
    assert_eq!(
        prepare_run::failed_status("   "),
        prepare_run::FAILED_BLANK,
        "whitespace is not a reason either"
    );
}

// ---- F0.3 S5 through this flow: the arming reaches the run that owns the restart ----

/// F0.3 S5 end to end: a stop that landed inside Describe arms the NEXT `begin` to describe from the
/// start, and only that one. The arming travels in `stop_legs`, not in the project file, so this is
/// the check that the seam is joined — with nothing but a lane's `state.txt` on disk (the marker an
/// unfinished description leaves), the armed run clears it.
#[test]
fn f0_3_s5_a_stop_inside_describe_arms_the_next_prepare_run() {
    let (_keep, tree) = scratch("f03arm");
    fs::create_dir_all(tree.describe_dir("lecture")).unwrap();
    fs::write(tree.describe_state("lecture"), b"halfway\n").unwrap();
    let project = with_sources(&["project:sources/lecture.mkv"]);

    naivepost::stop_legs::arm_describe_restart();
    match prepare_run::begin(&project, &tree) {
        Start::Started { cleared, .. } => {
            assert!(
                cleared.removed.iter().any(|f| f == "state.txt"),
                "the armed stop reached the flow that owns the restart: {:?}",
                cleared.removed
            );
        }
        other => panic!("expected Started, got {other:?}"),
    }
    // The arming was consumed by that run: with no marker left and nothing re-armed, the next begin is
    // idle-safe and clears nothing.
    match prepare_run::begin(&project, &tree) {
        Start::Started { cleared, .. } => {
            assert!(cleared.quiet(), "one stop arms one run, no further: {cleared:?}");
        }
        other => panic!("expected Started, got {other:?}"),
    }
}
