//! §01-project-and-files#6-text-formats, the `events.tsv` row: `start\tend\ttext` per frame,
//! times with two decimals, a `same` row extending the row before it WHEN READ.
//!
//! The point of this file is that the stage which APPENDS rows chunk by chunk
//! (`describe::events_rows` + `describe::write_chunk`) and the readers the live flows use
//! (`fix_transcripts::load_events` for the fix pass, `ui/produce_languages::read_all_events` for the
//! upload brief, `describe::read_events` for the resume) agree on one shape. They did not: the writer
//! emitted `at\ttext`, every reader refused it, and both live callers swallowed that error into an
//! empty picture context. Every file here is therefore written by the program's own writer and read
//! back by the program's own readers; only s4 plants bytes by hand, because it stands for a project
//! an older version wrote.
//!
//! §6 states no user action, so there is no widget test here — these are the format's rules.

use std::fs;

use crate::common;
use common::project_tree as project;
use naivepost::describe;
use naivepost::fix_transcripts as fix;
use naivepost::layout::Tree;
use naivepost::textfmt::{self, FrameEvent};
use naivepost::tools::describe::{Batch, Event};

/// A scratch project folder under the name Describe files its work in.
fn scratch(tag: &str) -> (std::path::PathBuf, Tree) {
    project(tag)
}

/// S1: what the describe stage writes, its readers parse. This is the round's whole complaint in one
/// test: the same bytes must satisfy the appender's shape and the fold-a-`same` reader's expectation.
#[test]
fn sec_01_project_and_files_6_text_formats_s1_what_the_describe_stage_writes_the_readers_parse() {
    let (_root, tree) = scratch("s1");
    let frames = [0.0, 1.0, 4.5];
    let events = [Event {
        frame: 2,
        text: "a slide changes".into(),
        calm: false,
    }];
    let rows = describe::events_rows(&frames, &events, true, 6.0);
    describe::write_chunk(&tree, "lecture", &rows, Some("On the slide.")).expect("the chunk lands");

    // §6's three columns, times with two decimals, one row per frame.
    assert_eq!(
        fs::read_to_string(tree.events_tsv("lecture")).unwrap(),
        "0.00\t1.00\tsame\n1.00\t4.50\ta slide changes\n4.50\t6.00\tsame\n"
    );

    // The reader that folds `same`: the event keeps the end the writer gave it, not its own second.
    let folded = textfmt::read_events(&tree.events_tsv("lecture")).unwrap();
    assert_eq!(
        folded.len(),
        1,
        "both `same` rows fold into the one real event: {folded:?}"
    );
    assert_eq!((folded[0].start, folded[0].end), (1.0, 6.0));
    assert_eq!(
        folded[0].text, "a slide changes",
        "the words arrive intact, not as a number"
    );

    // And through the fix pass's own door, on the same file.
    let via_fix = fix::load_events(&tree, "lecture");
    assert_eq!(
        via_fix, folded,
        "the fix pass reads exactly what the fold rule says"
    );
    assert_eq!(
        via_fix[0].end, 6.0,
        "the real end, not the frame's own second"
    );
}

/// S2: written per frame, folded when read (§6). Consecutive `same` rows extend the reading in front
/// of them, and the file's last row ends where the batch's coverage ended.
#[test]
fn sec_01_project_and_files_6_text_formats_s2_one_row_per_frame_and_same_extends_the_previous_row_when_read(
) {
    let (_root, tree) = scratch("s2");
    let frames = [0.0, 1.0, 2.0, 3.0];
    let events = [Event {
        frame: 1,
        text: "title slide".into(),
        calm: false,
    }];
    let rows = describe::events_rows(&frames, &events, true, 4.0);
    describe::write_chunk(&tree, "lecture", &rows, None).expect("chunk written");

    assert_eq!(
        fs::read_to_string(tree.events_tsv("lecture")).unwrap(),
        "0.00\t1.00\ttitle slide\n1.00\t2.00\tsame\n2.00\t3.00\tsame\n3.00\t4.00\tsame\n",
        "one row per frame, the `same` ones included: writing folds nothing"
    );

    let read = textfmt::read_events(&tree.events_tsv("lecture")).unwrap();
    assert_eq!(
        read.len(),
        1,
        "three `same` rows extend the one event: {read:?}"
    );
    assert_eq!(
        (read[0].start, read[0].end),
        (0.0, 4.0),
        "extended to the batch's own end"
    );
    assert_eq!(read[0].text, "title slide");
}

/// S3: the F1.7 S6 rule survives the shape change — a cold batch's FIRST frame is never written
/// `same`, because there is nothing behind it to be the same as; §6 force-writes `Calm; same view.`
/// there instead. In §6's columns now.
#[test]
fn sec_01_project_and_files_6_text_formats_s3_a_cold_batch_writes_the_calm_line_for_its_first_frame(
) {
    let (_root, tree) = scratch("s3");
    let frames = [0.0, 1.0];

    let cold = describe::events_rows(&frames, &[], false, 2.0);
    assert_eq!(
        cold,
        vec![
            format!("0.00\t1.00\t{}", Batch::SAME_WITHOUT_HISTORY),
            "1.00\t2.00\tsame".to_string()
        ],
        "frame 1 gets its own reading, not a `same` about a second nobody watched"
    );
    let warm = describe::events_rows(&frames, &[], true, 2.0);
    assert_eq!(warm, vec!["0.00\t1.00\tsame", "1.00\t2.00\tsame"]);

    // Both halves reach the file unchanged, and the cold file opens on a real reading instead of a
    // `same` that claims a fact about a second nobody watched.
    describe::write_chunk(&tree, "cold", &cold, None).unwrap();
    assert_eq!(
        fs::read_to_string(tree.events_tsv("cold")).unwrap(),
        format!(
            "0.00\t1.00\t{}\n1.00\t2.00\tsame\n",
            Batch::SAME_WITHOUT_HISTORY
        )
    );
    // Read back, the forced line IS the file's first event (it is not a `same`, so nothing folds it
    // away) and the `same` after it extends that event rather than adding one (§6's fold rule).
    let read_back = textfmt::read_events(&tree.events_tsv("cold")).unwrap();
    assert_eq!(
        read_back,
        vec![FrameEvent {
            start: 0.0,
            end: 2.0,
            text: Batch::SAME_WITHOUT_HISTORY.to_string()
        }],
        "the calm line reaches a reader as the opening reading"
    );
    describe::write_chunk(&tree, "warm", &warm, None).unwrap();
    assert_eq!(
        textfmt::read_events(&tree.events_tsv("warm"))
            .unwrap()
            .iter()
            .map(|e| (e.start, e.end))
            .collect::<Vec<_>>(),
        // §6's fold is "a `same` row extends the previous row", and `read_events` follows it exactly:
        // a file of nothing but `same` rows has no real reading to extend, so nothing comes back. The
        // resume does not read this folded — `describe::read_events` keeps every row (§F1.7 S2).
        Vec::<(f64, f64)>::new(),
        "a `same` extends a real reading in front of it, and this file holds none"
    );
    assert_eq!(
        describe::read_events(&tree, "warm")
            .iter()
            .map(|row| (row.at, row.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(0.0, "same"), (1.0, "same")],
        "the resume sees both frames: each frame's own second marks its chunk answered"
    );
}

/// S4: a project an older version wrote holds the two-column file (`start\ttext`, no end). Reading it
/// as an error would be swallowed by both live callers and silently empty the picture context they
/// send, so it reads: such a row ends where it started.
#[test]
fn sec_01_project_and_files_6_text_formats_s4_a_legacy_two_column_file_still_reads() {
    let (_root, tree) = scratch("s4");
    let path = tree.events_tsv("lecture");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "1.0\ta slide changes\n").unwrap();

    let read = textfmt::read_events(&path).unwrap();
    assert_eq!(
        read,
        vec![FrameEvent {
            start: 1.0,
            end: 1.0,
            text: "a slide changes".into()
        }]
    );
    assert_eq!(
        fix::load_events(&tree, "lecture"),
        read,
        "and the fix pass gets it too"
    );

    // A `same` after it still extends it, so the old file folds the way the new one does.
    fs::write(&path, "1.0\ta slide changes\n2.0\tsame\n").unwrap();
    let folded = textfmt::read_events(&path).unwrap();
    assert_eq!(folded.len(), 1);
    assert_eq!((folded[0].start, folded[0].end), (1.0, 2.0));

    // The describe stage's own resume reader takes the same shape.
    let rows = describe::read_events(&tree, "lecture");
    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        vec!["a slide changes", "same"]
    );

    // One field is still a broken row, named by file and reason: nothing is silently dropped.
    fs::write(&path, "1.0\n").unwrap();
    let err = textfmt::read_events(&path).unwrap_err();
    assert!(err.contains("events.tsv"), "{err}");
    assert!(err.contains("expected start, end and text"), "{err}");
}

/// S5: the resume marker is the row's START second (§6's per-frame row, F1.7 S2's skip rule), and the
/// rolling window handed to the model carries the readings' TEXT with no leading end number in it.
#[test]
fn sec_01_project_and_files_6_text_formats_s5_the_resume_skips_the_chunks_already_in_the_file() {
    let (_root, tree) = scratch("s5");
    // Eight frames, four per request: chunks start at index 0 (t=0) and index 4 (t=4).
    let frames: Vec<f64> = (0..8).map(|i| i as f64).collect();
    let first = describe::events_rows(
        &frames[..4],
        &[Event {
            frame: 2,
            text: "graph appears".into(),
            calm: false,
        }],
        true,
        4.0,
    );
    describe::write_chunk(&tree, "lecture", &first, Some("Graph.")).unwrap();

    let done = describe::read_events(&tree, "lecture");
    assert_eq!(
        describe::starts_already_logged(&done, &frames, describe::FRAMES_PER_REQ),
        vec![0],
        "the answered chunk is skipped, the untouched one runs again"
    );

    // Second chunk: now both starts have a row.
    let second = describe::events_rows(
        &frames[4..],
        &[Event {
            frame: 1,
            text: "the speaker points left".into(),
            calm: false,
        }],
        true,
        8.0,
    );
    describe::write_chunk(&tree, "lecture", &second, Some("Still the graph.")).unwrap();
    let done = describe::read_events(&tree, "lecture");
    assert_eq!(
        describe::starts_already_logged(&done, &frames, describe::FRAMES_PER_REQ),
        vec![0, 4]
    );

    // What the next request quotes as "Just before this:" is the readings themselves: §6's middle
    // column must never leak into them.
    let seeded = describe::seed_history(&done);
    assert_eq!(seeded.len(), 3, "the last three rows, oldest first");
    assert!(
        seeded
            .iter()
            .all(|text| !text.starts_with(char::is_numeric)),
        "no row begins with its own end: {seeded:?}"
    );
    assert_eq!(
        seeded,
        vec!["same", "same", "same"],
        "the window is the readings verbatim, `same` ones included: {seeded:?}"
    );
    // The event that opened the run is further back than the window, and still in the file the skip
    // rule reads — which is what makes the window a window rather than the whole log.
    assert!(done.iter().any(|row| row.text == "graph appears"));
}
