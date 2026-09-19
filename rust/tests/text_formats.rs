//! §01-project-and-files#6-text-formats, part 1 — the row-oriented text formats: the
//! transcripts, the merged timeline, the event log with its `same` folding, the marks with
//! their 3- to 6-column history, and the finished words with their join markers.
//!
//! This item's text cites one parameter, P.policy.minTakeSeconds, which belongs to the
//! short-take silence of part 2; no `tool:*` is cited.

use std::path::{Path, PathBuf};

use naivepost::layout::Tree;
use naivepost::textfmt::{self, FrameEvent, Line, Retake, SessionLine};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-textfmt-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project folder inside a throwaway root.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// Plant a file's text at a path, making its directory.
fn plant(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

// --- transcript.tsv / transcript.fixed.tsv ------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_a_transcript_row_is_start_end_speaker_text_with_two_decimals()
{
    let (_root, t) = project("transcript");
    let path = t.transcript_tsv("lecture");
    textfmt::write_lines(
        &[Line { start: 1.005, end: 2.0, speaker: "1".to_string(), text: "We start".to_string() }],
        &path,
    )
    .unwrap();

    // The saved bytes are the whole claim: four tab-separated columns, times with two
    // decimals (§6's `%.2f`) — a whole number kept to 2.00, and 1.005 down to 1.00 because
    // that is what the digit actually is in binary (the same answer Go's %.2f gives).
    assert_eq!(read(&path), "1.00\t2.00\t1\tWe start\n");

    let back = textfmt::read_lines(&path).unwrap();
    assert_eq!(back[0].start, 1.0);
    assert_eq!(back[0].end, 2.0);
    assert_eq!((back[0].speaker.as_str(), back[0].text.as_str()), ("1", "We start"));
}

#[test]
fn sec_01_project_and_files_6_text_formats_times_are_written_with_two_decimals_and_never_reflowed() {
    let (_root, t) = project("decimals");
    // The fixed transcript is the same shape in another folder (§6's `*.fixed.tsv`).
    let path = t.transcript_src_dir("lecture").join("transcript.fixed.tsv");
    textfmt::write_lines(
        &[
            Line { start: 34.7, end: 65.0, speaker: "1".into(), text: "one line".into() },
            Line { start: 0.5, end: 0.25, speaker: "2".into(), text: "half a second".into() },
        ],
        &path,
    )
    .unwrap();

    let text = read(&path);
    assert!(text.contains("34.70\t65.00\t"), "{text}");
    assert!(text.contains("0.50\t0.25\t"), "{text}");
    assert_eq!(text.lines().count(), 2, "one row per line\n{text}");

    // Round-tripping changes nothing further: the same file written again is the same
    // bytes, which is what makes a step's output its own resume marker.
    let back = textfmt::read_lines(&path).unwrap();
    textfmt::write_lines(&back, &path).unwrap();
    assert_eq!(read(&path), text);
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_tab_inside_the_text_stays_with_the_text() {
    let (_root, t) = project("tabs");
    let path = t.transcript_tsv("lecture");
    textfmt::write_lines(
        &[Line { start: 1.0, end: 2.0, speaker: "1".into(), text: "left\tright".into() }],
        &path,
    )
    .unwrap();
    assert_eq!(read(&path), "1.00\t2.00\t1\tleft\tright\n");

    // Four columns read from the left; whatever is left over belongs to the text, tabs
    // included, so a table read out of a screen capture survives.
    let back = textfmt::read_lines(&path).unwrap();
    assert_eq!(back[0].text, "left\tright");

    // And more columns than that still say four things: only three is corruption.
    plant(&path, "1.00\t2.00\t1\ta\tb\n");
    assert_eq!(textfmt::read_lines(&path).unwrap()[0].text, "a\tb");
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_short_row_or_a_missing_file_is_handled_apart() {
    let (_root, t) = project("missing");
    let path = t.transcript_tsv("lecture");
    // No file: the step has not run, which is no rows and no error.
    assert!(textfmt::read_lines(&path).unwrap().is_empty());
    assert!(textfmt::read_session(&t.session_tsv()).unwrap().is_empty());
    assert!(textfmt::read_events(&t.events_tsv("lecture")).unwrap().is_empty());
    assert!(textfmt::read_retakes(&t.retakes_tsv()).unwrap().is_empty());

    // A row of three columns is a cut-off file, and reading it as no words would delete
    // speech from the page.
    plant(&path, "1.00\t2.00\t1\n");
    let err = textfmt::read_lines(&path).unwrap_err();
    assert!(err.contains("transcript.tsv"), "{err}");
    assert!(err.contains("expected start, end, speaker and text"), "{err}");

    // A time that is not a number names the row it failed on.
    plant(&path, "not-a-time\t2.00\t1\thello\n");
    let err = textfmt::read_lines(&path).unwrap_err();
    assert!(err.contains("not a time"), "{err}");
    assert!(err.contains("not-a-time"), "{err}");

    // Blank lines are not rows.
    plant(&path, "\n1.00\t2.00\t1\thello\n\n");
    assert_eq!(textfmt::read_lines(&path).unwrap().len(), 1);
}

// --- session.tsv ---------------------------------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_a_session_row_carries_the_source_and_speaker_or_event() {
    let (_root, t) = project("session");
    let path = t.session_tsv();
    textfmt::write_session(
        &[
            SessionLine {
                start: 1.5,
                end: 34.7,
                source: "lecture".into(),
                who: "1".into(),
                text: "spoken words".into(),
            },
            SessionLine {
                start: 40.0,
                end: 41.0,
                source: "lecture".into(),
                who: "EVENT".into(),
                text: "a slide changes".into(),
            },
        ],
        &path,
    )
    .unwrap();

    let text = read(&path);
    assert_eq!(
        text,
        "1.50\t34.70\tlecture\t1\tspoken words\n40.00\t41.00\tlecture\tEVENT\ta slide changes\n"
    );

    let back = textfmt::read_session(&path).unwrap();
    assert_eq!(back.len(), 2);
    // The word EVENT in the speaker's place is what marks a picture event.
    assert!(!back[0].is_event());
    assert!(back[1].is_event());
    assert_eq!(back[1].source, "lecture");

    // Five columns are required; four would put the text where the speaker is.
    plant(&path, "1.50\t34.70\tlecture\tspoken\n");
    let err = textfmt::read_session(&path).unwrap_err();
    assert!(err.contains("session.tsv"), "{err}");
}

// --- events.tsv ----------------------------------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_events_are_one_row_per_frame_as_written() {
    let (_root, t) = project("events");
    let path = t.events_tsv("lecture");
    textfmt::write_events(
        &[
            FrameEvent { start: 0.0, end: 1.0, text: "a title slide".into() },
            FrameEvent { start: 1.0, end: 2.0, text: "same".into() },
            FrameEvent { start: 2.0, end: 3.0, text: "the code scrolls".into() },
        ],
        &path,
    )
    .unwrap();

    // Writing keeps every frame it was given — the folding is a read-side rule, so the
    // file still says what each frame was stamped with.
    assert_eq!(
        read(&path),
        "0.00\t1.00\ta title slide\n1.00\t2.00\tsame\n2.00\t3.00\tthe code scrolls\n"
    );
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_same_row_extends_the_previous_row_when_read() {
    let (_root, t) = project("same");
    let path = t.events_tsv("lecture");
    plant(
        &path,
        "0.00\t1.00\ta title slide\n\
         1.00\t2.00\tsame\n\
         2.00\t3.00\tsame\n\
         3.00\t4.00\tthe code scrolls\n",
    );

    let events = textfmt::read_events(&path).unwrap();
    // Two frames of nothing changed extend the one event twice rather than adding rows:
    // what comes back is one event per thing that happened, dated to its last second.
    assert_eq!(events.len(), 2);
    assert_eq!((events[0].start, events[0].end), (0.0, 3.0));
    assert_eq!(events[0].text, "a title slide");
    assert_eq!((events[1].start, events[1].end), (3.0, 4.0));
    // A `same` row never survives the read.
    assert!(!events.iter().any(|e| textfmt::is_same(&e.text)));

    // The older spelling folds the same way, and so does punctuation around it.
    for spelling in ["(same)", "Same.", "same\"", "(Same)"] {
        plant(&path, &format!("0.00\t1.00\ta slide\n1.00\t2.00\t{spelling}\n"));
        let events = textfmt::read_events(&path).unwrap();
        assert_eq!(events.len(), 1, "{spelling} should fold");
        assert_eq!(events[0].end, 2.0, "{spelling}");
    }

    // A file that opens on a frame where nothing changed has not yet said what it was the
    // same as, so there is nothing to extend and the row is dropped.
    plant(&path, "0.00\t1.00\tsame\n1.00\t2.00\tthe code scrolls\n");
    let events = textfmt::read_events(&path).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!((events[0].start, events[0].end), (1.0, 2.0));

    // A file of nothing but `same` says nothing happened.
    plant(&path, "0.00\t1.00\tsame\n1.00\t2.00\tsame\n");
    assert!(textfmt::read_events(&path).unwrap().is_empty());

    // The predicate on its own, since the write side keeps `same` rows in.
    assert!(textfmt::is_same("same") && textfmt::is_same("(same)"));
    assert!(!textfmt::is_same("the same code as before"));
}

// --- retakes.tsv ---------------------------------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_a_mark_writes_five_columns_and_a_sixth_only_when_it_has_whole_takes()
{
    let (_root, t) = project("retakes");
    let path = t.retakes_tsv();
    textfmt::write_retakes(
        &[
            Retake {
                s: 12.5,
                e: 18.25,
                again: 20.0,
                to: 11.0,
                text: "said twice".into(),
                whole: String::new(),
            },
            Retake {
                s: 30.0,
                e: 33.0,
                again: 0.0,
                to: 33.0,
                text: "a whole take".into(),
                whole: "cam,cam2".into(),
            },
        ],
        &path,
    )
    .unwrap();

    let text = read(&path);
    // The mark with nothing to say about whole takes has no sixth column at all — §6 says
    // "empty or absent when none", and an empty field would be a third state.
    assert_eq!(
        text,
        "12.50\t18.25\t20.00\t11.00\tsaid twice\n30.00\t33.00\t0.00\t33.00\ta whole take\tcam,cam2\n"
    );

    let back = textfmt::read_retakes(&path).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(back[0].whole, "");
    assert_eq!(back[1].whole, "cam,cam2");
    assert_eq!((back[1].s, back[1].e, back[1].again, back[1].to), (30.0, 33.0, 0.0, 33.0));
    assert_eq!(textfmt::read_retakes(&path).unwrap(), back);
}

#[test]
fn sec_01_project_and_files_6_text_formats_three_to_six_column_mark_files_are_all_read() {
    let (_root, t) = project("mark-shapes");
    let path = t.retakes_tsv();

    // 6 columns: the shape this build writes.
    plant(&path, "1.00\t2.00\t3.00\t0.50\thello\tcam\n");
    let back = textfmt::read_retakes(&path).unwrap();
    assert_eq!(back[0].text, "hello");
    assert_eq!(back[0].whole, "cam");
    assert_eq!(back[0].to, 0.5);

    // 5 columns: the same mark with no whole takes named.
    plant(&path, "1.00\t2.00\t3.00\t0.50\thello\n");
    let back = textfmt::read_retakes(&path).unwrap();
    assert_eq!(back[0].text, "hello");
    assert_eq!(back[0].whole, "");
    assert_eq!(back[0].to, 0.5);

    // 4 columns: written before the edges were placed — its fourth column is the text and
    // the removal ends with the words, so `to` is `e`.
    plant(&path, "1.00\t2.00\t3.00\thello\n");
    let back = textfmt::read_retakes(&path).unwrap();
    assert_eq!(back[0].text, "hello");
    assert_eq!(back[0].to, 2.0);

    // 3 columns: before the removal's end and the text existed at all.
    plant(&path, "1.00\t2.00\t3.00\n");
    let back = textfmt::read_retakes(&path).unwrap();
    assert_eq!(back[0].text, "");
    assert_eq!((back[0].s, back[0].e, back[0].again, back[0].to), (1.0, 2.0, 3.0, 2.0));

    // A `to` of nought in the fifth-column shape means "the words alone", so it reads
    // back as the end of the mark rather than a removal ending at zero.
    plant(&path, "1.00\t2.00\t3.00\t0.00\thello\n");
    assert_eq!(textfmt::read_retakes(&path).unwrap()[0].to, 2.0);

    // A line too short to be a mark, or whose first three columns are not numbers, costs
    // that mark and not the file: these files are edited by hand.
    plant(&path, "1.00\t2.00\nnot\tnumbers\there\n1.00\t2.00\t3.00\tkept\n");
    let back = textfmt::read_retakes(&path).unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].text, "kept");

    // An empty file is no marks.
    plant(&path, "");
    assert!(textfmt::read_retakes(&path).unwrap().is_empty());
}

// --- final.txt -----------------------------------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_final_txt_marks_every_join_with_the_words_that_went() {
    let words: Vec<String> = ["hello", "world"].iter().map(|w| w.to_string()).collect();

    // `|cut N|` is a join where N words went; `|cut|` one where none did (§6).
    assert_eq!(textfmt::write_final(&words, &[3]), "hello |cut 3| world");
    assert_eq!(textfmt::write_final(&words, &[0]), "hello |cut| world");

    // Several joins, each with its own count, and the words as written.
    let three: Vec<String> = ["a", "b", "c"].iter().map(|w| w.to_string()).collect();
    assert_eq!(textfmt::write_final(&three, &[12, 0]), "a |cut 12| b |cut| c");

    // No joins at all — nothing was removed anywhere — is the words and nothing else.
    assert_eq!(textfmt::write_final(&words, &[]), "hello world");
    let one: Vec<String> = vec!["only".to_string()];
    assert_eq!(textfmt::write_final(&one, &[]), "only");

    // An empty video has no words and no marks.
    assert_eq!(textfmt::write_final(&[], &[]), "");
}

#[test]
fn sec_01_project_and_files_6_text_formats_the_join_markers_read_back_as_counts_not_words() {
    let (words, joins) = textfmt::read_final("hello |cut 3| world");
    assert_eq!(words, ["hello".to_string(), "world".to_string()]);
    assert_eq!(joins, [3]);

    let (words, joins) = textfmt::read_final("a |cut 12| b |cut| c");
    assert_eq!(words, ["a".to_string(), "b".to_string(), "c".to_string()]);
    // A bare `|cut|` is a join where nought words went — the same count the writer used.
    assert_eq!(joins, [12, 0]);

    let (words, joins) = textfmt::read_final("hello world");
    assert_eq!(words, ["hello".to_string(), "world".to_string()]);
    assert!(joins.is_empty());

    // Any field with a pipe in it is no word at all, however many fields the marker spans:
    // that is what keeps the prototype's longer mark from putting words on the page.
    let (words, _) = textfmt::read_final("hello |cut|whole take| world");
    assert_eq!(words, ["hello".to_string(), "world".to_string()]);

    // And what was written reads back as it was written.
    let three: Vec<String> = ["a", "b", "c"].iter().map(|w| w.to_string()).collect();
    let text = textfmt::write_final(&three, &[12, 0]);
    let (words, joins) = textfmt::read_final(&text);
    assert_eq!(words, three);
    assert_eq!(joins, [12, 0]);

    // final.txt is the file §6 means by this, and it is written where layout says.
    let (_root, t) = project("final");
    std::fs::create_dir_all(t.final_txt().parent().unwrap()).unwrap();
    std::fs::write(t.final_txt(), format!("{text}\n")).unwrap();
    let (words, joins) = textfmt::read_final(&read(&t.final_txt()));
    assert_eq!(words.join(" "), "a b c");
    assert_eq!(joins, [12, 0]);
}
