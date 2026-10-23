//! §04-prepare.md §5, bullets 1 and 2 — the two rules about FILES rather than about models:
//!
//! * *"Every stage skips when its output exists; resume markers written last and whole"* — a marker is a
//!   promise that the work behind it finished, so two things have to hold: it must be written after the
//!   work (`.frames` last, `frames::run` S5), and it must never exist in halves. A truncated marker is
//!   worse than none: the next ▶ reads it, believes it, and skips the stage whose output was never
//!   written. So every file Prepare writes goes out as `<name>.part` beside its destination and is
//!   renamed onto the name (§1 already spells `.part` for a source copy in flight); these tests are what
//!   pins that, because nothing else can see an interrupted write.
//! * *"Server's own documents never rewritten; derived files live beside them"* — `inputs/<src>/transcript.tsv`
//!   is what the ASR answered and stays byte-for-byte; the fixer's answer goes to `transcript.fixed.tsv`,
//!   which sits in the same tree beside it.
//!
//! No window here: both bullets are about bytes on disk, so each check builds a project folder of its own
//! and reads back what the program wrote through the program's own accessors.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::common;
use common::temp_dir_created as temp_dir;
use naivepost::fix_transcripts;
use naivepost::frames::{self, GRID, SCENE_THRESHOLD};
use naivepost::layout::Tree;
use naivepost::requests;
use naivepost::textfmt::{self, Line};

/// The lane every scene here uses. A microphone's base name, so the fixed file is the commentary one.
const SOURCE: &str = "mic.wav";
const BASE: &str = "mic";

/// A project folder of its own, with the frame directory the frames stage expects to find.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(dir.join("inputs/frames/mic.wav")).expect("the folders");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

/// Every `*.part` left under a folder, recursively — an interrupted writer's only visible trace, and the
/// thing that must never survive a call that returned Ok.
fn parts_under(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(parts_under(&path));
        } else if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            if name.ends_with(".part") {
                found.push(path.to_string_lossy().into_owned());
            }
        }
    }
    found.sort();
    found
}

/// §5 bullet 1: the `.frames` marker lands whole — exact bytes, readable by the reader that decides the
/// skip — and leaves no `.part` behind anywhere.
#[test]
fn sec_04_prepare_5_rules_b1_a_marker_lands_whole_and_leaves_no_part() {
    let (root, tree) = project("b1-whole");
    requests::write_frames_marker(&tree, SOURCE, GRID, SCENE_THRESHOLD).expect("the marker writes");

    let marker = tree.frames_marker(SOURCE);
    assert_eq!(
        fs::read_to_string(&marker).expect("readable"),
        format!("{GRID}|{SCENE_THRESHOLD}\n"),
        "the marker holds `<grid>|<threshold>` and its newline, whole"
    );
    assert_eq!(
        marker.metadata().expect("mode").permissions().mode() & 0o777,
        0o644,
        "§1's file mode lands on the real name, not only on the part"
    );
    assert!(
        parts_under(&root).is_empty(),
        "a completed write leaves no in-flight file: {:?}",
        parts_under(&root)
    );

    // And the whole file is what makes the stage skip: the reader parses it, and the match is the skip.
    let found = requests::read_frames_marker(&tree, SOURCE).expect("parses");
    assert_eq!(
        found,
        Some((GRID, SCENE_THRESHOLD)),
        "read back what was written"
    );
    assert!(
        frames::marker_matches(found, GRID, SCENE_THRESHOLD),
        "so the next run skips this stage"
    );
}

/// §5 bullet 1, the half a plain `fs::write` cannot give: when a write fails, the file that WAS there is
/// still there, unchanged, and the failure leaves no `.part` to be mistaken for work.
#[test]
fn sec_04_prepare_5_rules_b1_a_failed_write_leaves_the_real_file_alone() {
    let (root, tree) = project("b1-failed");
    let good = [("INTERVAL".to_string(), "0.25".to_string())];
    requests::write_meta(&tree, &good).expect("first write");
    let meta = tree.meta_env();
    let before = fs::read_to_string(&meta).expect("readable");
    assert_eq!(before, "INTERVAL=0.25\n", "the good bytes are on disk");

    // Make the destination's directory refuse a new file, then ask for the same write again. The rename
    // target's parent is unwritable, so the part cannot even be created.
    let inputs = meta.parent().expect("a parent");
    fs::set_permissions(inputs, PermissionsExt::from_mode(0o500)).expect("closed to writes");
    let outcome = requests::write_meta(&tree, &[("INTERVAL".to_string(), "9.99".to_string())]);
    // Open it again whatever happened: a folder nobody can write is not a folder a later cleanup sees.
    fs::set_permissions(inputs, PermissionsExt::from_mode(0o755)).expect("open again");

    assert!(
        outcome.is_err(),
        "a write into a closed folder must say so, not pretend: {outcome:?}"
    );
    assert!(
        outcome
            .expect_err("the refusal")
            .contains(&meta.display().to_string()),
        "the error names the file it could not write"
    );
    assert_eq!(
        fs::read_to_string(&meta).expect("still readable"),
        before,
        "the previous whole file stands: a failed write never touches the name"
    );
    assert!(
        parts_under(&root).is_empty(),
        "and a failed write cleans its own part up: {:?}",
        parts_under(&root)
    );
}

/// §5 bullet 1's other half: the marker comes LAST. A frames pass that dies partway has pictures on disk
/// and no marker, so the next run extracts instead of skipping over a half-done job.
#[test]
fn sec_04_prepare_5_rules_b1_marker_written_last_after_the_frames() {
    let (root, tree) = project("b1-last");
    let log = scene_log();

    // First chunk fails: some of the grid was asked for, none of it is vouched for.
    let mut halfway = Extract::default();
    let first = frames::run(
        &tree,
        SOURCE,
        BASE,
        60.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        &log,
        |at| halfway.run(at, true),
    );
    assert!(
        first.is_err(),
        "a failed extraction is the stage's failure: {first:?}"
    );
    assert!(
        !halfway.calls.lock().unwrap().is_empty(),
        "extraction had begun: the pass reached at least one chunk before it died"
    );
    assert!(
        !tree.frames_marker(SOURCE).exists(),
        "no marker while the stage is half done — a half marker is a skip over work never done"
    );
    assert!(
        parts_under(&root).iter().all(|p| !p.contains(".frames")),
        "no half marker either"
    );

    // Now the same pass to success: the marker appears, and the second call answers skipped off it.
    let mut done = Extract::default();
    let out = frames::run(
        &tree,
        SOURCE,
        BASE,
        60.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        &log,
        |at| done.run(at, false),
    )
    .expect("a finished pass is not a failure");
    assert!(!out.skipped, "the first pass that ran was not a skip");
    assert!(
        tree.frames_marker(SOURCE).is_file(),
        "the marker is written last, after the frames"
    );

    let again = frames::run(
        &tree,
        SOURCE,
        BASE,
        60.0,
        0.0,
        SCENE_THRESHOLD,
        GRID,
        8,
        &log,
        |at| done.run(at, false),
    )
    .expect("a skip is not a failure");
    assert!(again.skipped, "the next run skips off the marker");
    assert_eq!(
        again.logs,
        vec![frames::already_log(BASE, GRID, SCENE_THRESHOLD)],
        "and says so, naming the two numbers it matched"
    );
}

/// Counts the chunks it was handed, the way `tests/suite/frames.rs`'s helper does; `fail_first` makes
/// the very first chunk answer ffmpeg's exit-1 so the pass stops mid-way.
#[derive(Default)]
struct Extract {
    calls: std::sync::Mutex<Vec<Vec<f64>>>,
}

impl Extract {
    fn run(&mut self, at: &[f64], fail_first: bool) -> Result<usize, String> {
        let index = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(at.to_vec());
            calls.len()
        };
        if fail_first && index == 1 {
            return Err("ffmpeg exited 1".into());
        }
        Ok(at.len())
    }
}

/// ffmpeg's `metadata=print` report for a short clip, as in `tests/suite/frames.rs`: a change around
/// 1.0 s and another at 5.0 s, quiet elsewhere.
fn scene_log() -> String {
    [
        "frame:0    pts:0       pts_time:0",
        "lavfi.scd.score=0.118",
        "frame:4    pts:1600    pts_time:1.0",
        "tag:lavfi.scd.score=58.310",
        "frame:6    pts:2400    pts_time:1.4",
        "tag:lavfi.scd.score=12.900",
        "frame:20   pts:8000    pts_time:5.0",
        "tag:lavfi.scd.score=77.400",
    ]
    .join("\n")
}

/// §5 bullet 2: the derived files sit in the same tree as the server's own documents — beside them, not
/// somewhere else — and writing a derived one leaves the server's bytes alone.
#[test]
fn sec_04_prepare_5_rules_b2_a_derived_file_lives_beside_the_servers_own_one() {
    let (_root, tree) = project("b2-beside");
    // The ASR's own documents, as the server answered them.
    let doc = requests::WordsDoc {
        text: "the words as heard".into(),
        words: vec![requests::Word {
            word: "the".into(),
            start_sample: 0,
            end_sample: 8000,
        }],
    };
    requests::write_words(&tree, SOURCE, &doc).expect("words.json");
    requests::write_chunks(
        &tree,
        SOURCE,
        &[requests::Chunk {
            s: 0.0,
            e: 60.0,
            text: "x".into(),
        }],
    )
    .expect("asrchunks.json");
    let lines = vec![Line {
        start: 0.0,
        end: 1.0,
        speaker: "NARRATOR".into(),
        text: "as heard".into(),
    }];
    textfmt::write_lines(&lines, &tree.transcript_tsv(SOURCE)).expect("transcript.tsv");

    let words_before = fs::read(tree.words_json(SOURCE)).expect("bytes");
    let tsv_before = fs::read(tree.transcript_tsv(SOURCE)).expect("bytes");

    // The fixer's answer for the same recording: a microphone, so commentary, and no subtitles.
    let fixed = vec![Line {
        start: 0.0,
        end: 1.0,
        speaker: "NARRATOR".into(),
        text: "respelled".into(),
    }];
    let written =
        fix_transcripts::write_fixed(&tree, SOURCE, false, true, &fixed).expect("fixed files");
    assert_eq!(
        written,
        vec![tree.commentary_fixed_tsv(SOURCE)],
        "one file, and it is the derived one"
    );

    let derived = tree.commentary_fixed_tsv(SOURCE);
    assert!(derived.is_file(), "the derived file exists");
    // "Beside": inside the same prepare/ tree, one directory level per kind, sharing the session root.
    let prepare = tree.prepare_dir();
    assert!(
        derived.starts_with(&prepare),
        "{derived:?} is not under {prepare:?}"
    );
    assert!(
        tree.words_json(SOURCE).starts_with(&prepare),
        "so is the server's own document"
    );
    assert_ne!(
        derived.parent(),
        tree.words_json(SOURCE).parent(),
        "in its own kind of folder"
    );
    assert_eq!(
        derived
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        Some(SOURCE.to_string()),
        "the derived file is filed under the SAME source name as the document it derives from"
    );

    // Writing the derived file rewrote nothing of the server's.
    assert_eq!(
        fs::read(tree.words_json(SOURCE)).expect("bytes"),
        words_before,
        "words.json untouched"
    );
    assert_eq!(
        fs::read(tree.transcript_tsv(SOURCE)).expect("bytes"),
        tsv_before,
        "transcript.tsv untouched"
    );
    assert!(
        !tree.subtitles_srt(SOURCE).exists(),
        "an audio recording gets no subtitles: there is nothing to caption over it"
    );
}

/// §5 bullet 2 with the fix pass actually running: the ASR's file is byte-identical afterwards while the
/// fixer's file differs from it — the pass changed TEXT, into a different file.
#[test]
fn sec_04_prepare_5_rules_b2_the_servers_own_document_is_never_rewritten_by_a_fix() {
    let (_root, tree) = project("b2-never");
    let heard = vec![
        Line {
            start: 0.0,
            end: 1.0,
            speaker: "NARRATOR".into(),
            text: "teh words as herd".into(),
        },
        Line {
            start: 1.0,
            end: 2.0,
            speaker: "NARRATOR".into(),
            text: "anoither line".into(),
        },
    ];
    textfmt::write_lines(&heard, &tree.transcript_tsv(SOURCE)).expect("the ASR's file");
    let before = fs::read(tree.transcript_tsv(SOURCE)).expect("bytes");

    let respelled = vec![
        Line {
            start: 0.0,
            end: 1.0,
            speaker: "NARRATOR".into(),
            text: "the words as heard".into(),
        },
        Line {
            start: 1.0,
            end: 2.0,
            speaker: "NARRATOR".into(),
            text: "another line".into(),
        },
    ];
    fix_transcripts::write_fixed(&tree, SOURCE, false, false, &respelled)
        .expect("the fixer's file");

    assert_eq!(
        fs::read(tree.transcript_tsv(SOURCE)).expect("bytes"),
        before,
        "the server's own document is never rewritten: transcript.tsv is byte-for-byte what the ASR sent"
    );
    let fixed_bytes =
        fs::read(tree.transcript_fixed_tsv(SOURCE)).expect("the fixer wrote its own file");
    assert_ne!(
        fixed_bytes, before,
        "the fixed text is DIFFERENT — and lives in the other file"
    );
    let read_back = textfmt::read_lines(&tree.transcript_fixed_tsv(SOURCE)).expect("readable");
    assert_eq!(
        read_back, respelled,
        "every row kept its times and its speaker; only the text moved"
    );
}
