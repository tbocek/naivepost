//! §01-project-and-files#6-text-formats, part 2 — `requests.tsv` and the ASR sidecars:
//! the eighteen timed columns of the request log, the four JSON documents a recording leaves
//! behind, the short-take silence that asks no server, `meta.env`, `.frames` and `scenes.tsv`.

use std::path::{Path, PathBuf};

use naivepost::layout::Tree;
use naivepost::requests::{self, Outcome, Request, SceneChange, Service, WordsDoc, Word};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-requests-{}-{tag}",
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

fn plant(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A row with something in every column. `first_byte_s` is a value the two decimals of
/// §6 can hold exactly, so a row read back from the file equals the row written to it —
/// which is what makes round-tripping an assertion rather than an approximation.

fn full() -> Request {
    Request {
        started: "2026-09-16 17:25:30.412".to_string(),
        run: "0916-172530".to_string(),
        step: "describe".to_string(),
        job: "batch 3".to_string(),
        service: Service::Llm,
        model: "gpt-x".to_string(),
        kind: "chat".to_string(),
        sent_bytes: Some(1200),
        images: Some(4),
        received_bytes: Some(9000),
        tokens_in: Some(10),
        tokens_out: Some(20),
        wait_s: Some(0.5),
        first_byte_s: Some(1.23),
        on_wire_s: Some(3.0),
        thinking_s: None,
        outcome: Outcome::Error("connection refused".to_string()),
        attempt: 2,
    }
}

// --- requests.tsv --------------------------------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_the_request_log_header_names_eighteen_columns() {
    let fields: Vec<&str> = requests::HEADER.split('\t').collect();
    assert_eq!(fields.len(), 18, "{}", requests::HEADER);
    // §6's order, spelled out: a column added later goes at the end and an old row simply
    // has nothing there.
    assert_eq!(
        fields,
        [
            "started",
            "run",
            "step",
            "job",
            "service",
            "model",
            "kind",
            "sent_bytes",
            "images",
            "received_bytes",
            "tokens_in",
            "tokens_out",
            "wait_s",
            "first_byte_s",
            "on_wire_s",
            "thinking_s",
            "outcome",
            "attempt"
        ]
    );
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_request_is_written_as_eighteen_tab_separated_fields() {
    let line = requests::write_line(&full());
    assert!(line.ends_with('\n'));
    // The whole row, in order: the service lower-case, `error <reason>` as one field, and
    // every second at two decimals.
    assert_eq!(
        line.trim_end(),
        "2026-09-16 17:25:30.412\t0916-172530\tdescribe\tbatch 3\tllm\tgpt-x\tchat\t1200\t4\t\
         9000\t10\t20\t0.50\t1.23\t3.00\t\terror connection refused\t2"
    );

    // And it reads back as the same request, which is what makes the file a record rather
    // than a printout.
    assert_eq!(requests::parse_line(&line).unwrap(), full());
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_cache_hit_has_no_time_on_the_wire() {
    let cache = Request {
        started: "2026-09-16 17:25:31.000".to_string(),
        step: "transcribe".to_string(),
        service: Service::Audio,
        kind: "asr".to_string(),
        outcome: Outcome::Cache,
        attempt: 1,
        ..Default::default()
    };
    let line = requests::write_line(&cache);
    // Empty fields, not zeros: a request that took no time and one that never went on the
    // wire are different facts.
    assert!(line.contains("\t\t\t\t\t\tcache\t1\n"), "{line}");

    let back = requests::parse_line(&line).unwrap();
    assert_eq!(back.on_wire_s, None);
    assert_eq!(back.wait_s, None);
    assert_eq!(back.first_byte_s, None);
    assert_eq!(back.thinking_s, None);
    assert_eq!(back.sent_bytes, None);
    assert_eq!(back.outcome, Outcome::Cache);

    // Seconds always carry two decimals — 1.234 became 1.23 above, and a whole number keeps
    // its two noughts.
    let timed = Request { on_wire_s: Some(3.0), ..cache.clone() };
    assert!(requests::write_line(&timed).contains("\t3.00\t"));
}

#[test]
fn sec_01_project_and_files_6_text_formats_the_outcome_column_is_one_of_the_named_answers() {
    // `error` carries a status or a reason, and the reason may hold spaces.
    for (text, outcome) in [
        ("ok", Outcome::Ok),
        ("cache", Outcome::Cache),
        ("stalled", Outcome::Stalled),
        ("cancelled", Outcome::Cancelled),
        ("error 500", Outcome::Error("500".to_string())),
        (
            "error connection refused",
            Outcome::Error("connection refused".to_string()),
        ),
    ] {
        let parsed: Outcome = text.parse().expect(text);
        assert_eq!(parsed, outcome);
        assert_eq!(parsed.to_string(), text);
    }

    // An outcome this build cannot name is refused rather than read as `ok`.
    let err = "nope".parse::<Outcome>().unwrap_err();
    assert!(err.contains("not an outcome"), "{err}");
    let line = requests::write_line(&Request {
        outcome: Outcome::Stalled,
        ..Default::default()
    });
    plant(
        &project("bad-outcome").0.join("demo.naivepost/requests.tsv"),
        "whatever",
    );
    let bad = full();
    let mut bad = bad;
    bad.outcome = Outcome::Ok;
    let row = requests::write_line(&bad).replace("\tok\t", "\tnope\t");
    let err = requests::parse_line(&row).unwrap_err();
    assert!(err.contains("not an outcome"), "{err}");
    assert!(!line.is_empty());
}

#[test]
fn sec_01_project_and_files_6_text_formats_the_log_is_appended_to_and_never_rewritten() {
    let (_root, t) = project("append");
    let path = t.requests_tsv();

    requests::record(&t, &full()).unwrap();
    let text = read(&path);
    // One header, then the row.
    assert_eq!(text.lines().count(), 2, "{text}");
    assert_eq!(text.lines().next().unwrap(), requests::HEADER);

    requests::record(&t, &full()).unwrap();
    let text = read(&path);
    // The header appears once however many rows follow: the file is appended to, never
    // rewritten.
    assert_eq!(text.matches(requests::HEADER).count(), 1, "{text}");
    assert_eq!(text.lines().count(), 3, "{text}");

    // A retry is a row of its own, and both rows read back in the order they were written.
    let mut retry = full();
    retry.attempt = 3;
    requests::record(&t, &retry).unwrap();
    let rows = requests::read(&t).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2].attempt, 3);
    assert_eq!(rows[0], full());

    // §1: files are 0644.
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );

    // A file with a hand-written line and no header is appended to untouched — repairing it
    // would rewrite history the file is the only record of.
    let (_root, hand) = project("hand-edited");
    plant(&hand.requests_tsv(), "an old row\n");
    requests::record(&hand, &full()).unwrap();
    let text = read(&hand.requests_tsv());
    assert!(text.starts_with("an old row\n"), "{text}");
    // The header is added above the new row so the columns are still named, and the old
    // line keeps its place.
    assert_eq!(text.lines().count(), 3, "{text}");
    assert_eq!(text.lines().nth(1).unwrap(), requests::HEADER);
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_new_project_starts_an_empty_log() {
    let (_root, t) = project("empty-log");
    // A missing file is no rows: "a new project starts an empty one" (§6).
    assert!(requests::read(&t).unwrap().is_empty());
    assert!(!t.requests_tsv().exists());

    // An empty file gets the header on its first row, so nothing about the columns is lost.
    plant(&t.requests_tsv(), "");
    requests::record(&t, &full()).unwrap();
    assert_eq!(read(&t.requests_tsv()).lines().next().unwrap(), requests::HEADER);

    // A row written by an older build has none of a column added since; the fields that are
    // there still mean what they say.
    let (_root, old) = project("old-row");
    plant(
        &old.requests_tsv(),
        "2026-01-01 00:00:00.000\trun\tcut\t\tllm\tgpt-x\tchat\n",
    );
    let rows = requests::read(&old).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].step, "cut");
    assert_eq!(rows[0].service, Service::Llm);
    // Nothing said about the missing columns, and an attempt that was never recorded reads
    // as the first one — a row from before retries existed *was* the first try.
    assert_eq!(rows[0].on_wire_s, None);
    assert_eq!(rows[0].attempt, 1);

    // A row too short to name its service is an error naming the file, not a silent row.
    let (_root, broken) = project("broken-row");
    plant(&broken.requests_tsv(), "only\tthree\n");
    let err = requests::read(&broken).unwrap_err();
    assert!(err.contains("requests.tsv"), "{err}");

    // And a service outside the four is refused rather than guessed.
    let (_root, wrong) = project("wrong-service");
    plant(
        &wrong.requests_tsv(),
        "a\tb\tc\td\tvideo\te\tchat\t\t\t\t\t\t\t\t\t\tok\t1\n",
    );
    let err = requests::read(&wrong).unwrap_err();
    assert!(err.contains("not a service"), "{err}");
}

#[test]
fn sec_01_project_and_files_6_text_formats_started_is_local_time_with_milliseconds() {
    // Pinned against an independent clock for the epoch, a negative offset, a millisecond
    // before midnight at a year boundary and one before a leap day.
    assert_eq!(requests::format_started(0, 0), "1970-01-01 00:00:00.000");
    assert_eq!(
        requests::format_started(1_789_000_000_412, 7200),
        "2026-09-10 02:26:40.412"
    );
    assert_eq!(
        requests::format_started(1_700_000_000_123, -12600),
        "2023-11-14 18:43:20.123"
    );
    assert_eq!(
        requests::format_started(1_735_689_600_000, 0),
        "2025-01-01 00:00:00.000"
    );
    assert_eq!(
        requests::format_started(1_709_683_199_999, 0),
        "2024-03-05 23:59:59.999"
    );
    // Before the epoch, still counting back through the millisecond.
    assert_eq!(
        requests::format_started(-86_400_001, 0),
        "1969-12-30 23:59:59.999"
    );
}

// --- the ASR sidecars ----------------------------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_words_json_is_the_asr_servers_own_document() {
    let (_root, t) = project("words");
    let doc = WordsDoc {
        text: "we start".to_string(),
        words: vec![
            Word { word: "we".to_string(), start_sample: 0, end_sample: 8000 },
            Word { word: "start".to_string(), start_sample: 8000, end_sample: 17000 },
        ],
    };
    requests::write_words(&t, "lecture", &doc).unwrap();

    // The server's keys, kept as they came back.
    let text = read(&t.words_json("lecture"));
    assert!(text.contains("\"text\":\"we start\""), "{text}");
    assert!(text.contains("\"word\":\"we\""), "{text}");
    assert!(text.contains("\"start_sample\":0"), "{text}");
    assert!(text.contains("\"end_sample\":17000"), "{text}");
    assert_eq!(requests::read_words(&t, "lecture").unwrap(), Some(doc.clone()));

    // `words.aligned.json` is the aligner's answer: the words and nothing else.
    let aligned = requests::AlignedDoc { words: doc.words.clone() };
    requests::write_aligned(&t, "lecture", &aligned).unwrap();
    let text = read(&t.words_aligned_json("lecture"));
    assert!(text.starts_with("{\"words\":["), "{text}");
    assert!(!text.contains("\"text\":"), "{text}");
    assert_eq!(requests::read_aligned(&t, "lecture").unwrap(), Some(aligned));
}

#[test]
fn sec_01_project_and_files_6_text_formats_turns_and_chunks_are_bare_arrays() {
    let (_root, t) = project("arrays");
    let turns = [requests::Turn { start_sample: 100, end_sample: 9000, speaker_id: 1 }];
    requests::write_turns(&t, "lecture", &turns).unwrap();

    // Not wrapped in an object: the file is the list.
    let text = read(&t.turns_json("lecture"));
    assert!(text.starts_with('['), "{text}");
    assert!(text.contains("\"speaker_id\":1"), "{text}");
    assert_eq!(requests::read_turns(&t, "lecture").unwrap(), turns);

    // asrchunks.json: the seconds each ASR request covered and exactly what came back.
    let chunks = [requests::Chunk { s: 0.0, e: 30.0, text: "the first thirty seconds".to_string() }];
    requests::write_chunks(&t, "lecture", &chunks).unwrap();
    let text = read(&t.asrchunks_json("lecture"));
    assert!(text.starts_with("[{"), "{text}");
    for key in ["\"s\":", "\"e\":", "\"text\":"] {
        assert!(text.contains(key), "{key} missing from {text}");
    }
    assert_eq!(requests::read_chunks(&t, "lecture").unwrap(), chunks);

    // An empty list is an empty array, not a null.
    requests::write_turns(&t, "silent", &[]).unwrap();
    assert_eq!(read(&t.turns_json("silent")), "[]");
    // And no file at all reads as no entries rather than an error.
    assert!(requests::read_turns(&t, "never-ran").unwrap().is_empty());
    assert!(requests::read_chunks(&t, "never-ran").unwrap().is_empty());
    assert_eq!(requests::read_words(&t, "never-ran").unwrap(), None);
}

#[test]
fn sec_01_project_and_files_6_text_formats_a_recording_under_the_short_take_bound_is_written_as_silence()
{
    // P.policy.minTakeSeconds — 2.0 s by default; a shorter recording is a start and a
    // stop, written up as silence with no server asked (§6).
    assert_eq!(requests::MIN_TAKE_SECONDS, 2.0);
    assert!(requests::is_short(1.999, requests::MIN_TAKE_SECONDS));
    // Exactly the bound is a take: the bound is where asking stops being worth it, and a
    // recording that reaches it has something in it.
    assert!(!requests::is_short(2.0, requests::MIN_TAKE_SECONDS));
    assert!(!requests::is_short(30.0, requests::MIN_TAKE_SECONDS));

    let (_root, t) = project("silence");
    requests::write_silence(&t, "blip").unwrap();

    // transcript.txt is a lone newline: no words, but the file exists so every later step
    // resumes rather than re-asking.
    assert_eq!(read(&t.transcript_txt("blip")), "\n");
    // words.json says only that nothing was heard — an empty `words` array would be a
    // second way to say the same thing, and the two could disagree.
    assert_eq!(read(&t.words_json("blip")), r#"{"text":""}"#);
    // turns.json is the empty list: nobody spoke.
    assert_eq!(read(&t.turns_json("blip")), "[]");
    // And there is no asrchunks.json, because no ASR request was ever made — a file here
    // would claim a request that never happened.
    assert!(!t.asrchunks_json("blip").exists());

    // The silence reads back through the same readers everything else uses.
    let words = requests::read_words(&t, "blip").unwrap().expect("written");
    assert_eq!(words.text, "");
    assert!(words.words.is_empty());
    assert!(requests::read_turns(&t, "blip").unwrap().is_empty());

    // §1: files are 0644.
    use std::os::unix::fs::PermissionsExt;
    for path in [t.transcript_txt("blip"), t.words_json("blip"), t.turns_json("blip")] {
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644,
            "{}",
            path.display()
        );
    }
}

// --- meta.env, .frames, scenes.tsv --------------------------------------------

#[test]
fn sec_01_project_and_files_6_text_formats_meta_env_is_key_value_lines_whose_existence_says_the_sources_were_read()
{
    let (_root, t) = project("meta");
    // Nothing read yet.
    assert!(!requests::sources_read(&t));

    // §1's keys: INTERVAL and SCALE always, the two video ones with footage, the two audio
    // ones when narrator 1 resolves. Order is kept, because the file is read by scripts.
    let pairs = [
        ("INTERVAL", "1.0"),
        ("SCALE", "720p"),
        ("VIDEO_FILE", "/media/a.mkv"),
        ("VIDEO_BASE", "a"),
        ("AUDIO_FILE", "=weird=path with spaces.wav"),
        ("AUDIO_BASE", "weird"),
    ];
    requests::write_meta(&t, &pairs.map(|(k, v)| (k.to_string(), v.to_string()))).unwrap();

    let text = read(&t.meta_env());
    assert_eq!(text.lines().next().unwrap(), "INTERVAL=1.0", "{text}");
    // Its existence is the whole answer to "were the sources read".
    assert!(requests::sources_read(&t));

    // Values verbatim: no quoting, no escaping, and a `=` in the value splits on the first
    // one only.
    let back = requests::read_meta(&t).unwrap();
    assert_eq!(back.len(), 6);
    assert_eq!(back[4], ("AUDIO_FILE".to_string(), "=weird=path with spaces.wav".to_string()));
    assert_eq!(back, pairs.map(|(k, v)| (k.to_string(), v.to_string())));

    // Blank lines and comments are not entries.
    plant(&t.meta_env(), "# a note for whoever opens this\n\nINTERVAL=1.0\n");
    assert_eq!(requests::read_meta(&t).unwrap().len(), 1);
    assert!(requests::sources_read(&t));

    // A line that is not KEY=VALUE is an error naming it, not a silently skipped entry.
    plant(&t.meta_env(), "INTERVAL\n");
    let err = requests::read_meta(&t).unwrap_err();
    assert!(err.contains("KEY=VALUE"), "{err}");

    // No file is no entries — and still "not read".
    let (_root, fresh) = project("meta-fresh");
    assert!(requests::read_meta(&fresh).unwrap().is_empty());
    assert!(!requests::sources_read(&fresh));
}

#[test]
fn sec_01_project_and_files_6_text_formats_the_frames_marker_holds_the_grid_and_the_scene_threshold()
{
    let (_root, t) = project("frames");
    requests::write_frames_marker(&t, "lecture", 0.25, 0.35).unwrap();

    // `<grid>|<scene threshold>` and nothing else — frames are always the video's own size
    // (§6), unlike the prototype's `.interval`, which held a scale name and had no scenes.
    assert_eq!(read(&t.frames_marker("lecture")), "0.25|0.35\n");
    assert_eq!(requests::read_frames_marker(&t, "lecture").unwrap(), Some((0.25, 0.35)));

    // The marker sits beside the frames it describes.
    assert_eq!(
        t.frames_marker("lecture"),
        t.frames_dir("lecture").join(".frames")
    );

    // No marker says the frames were never scanned.
    assert_eq!(requests::read_frames_marker(&t, "never").unwrap(), None);

    // A marker that is not two numbers is an error naming it.
    plant(&t.frames_marker("lecture"), "0.25\n");
    let err = requests::read_frames_marker(&t, "lecture").unwrap_err();
    assert!(err.contains("grid"), "{err}");
}

#[test]
fn sec_01_project_and_files_6_text_formats_scenes_tsv_is_a_time_and_a_score_per_scene_change() {
    let (_root, t) = project("scenes");
    requests::write_scenes(
        &t,
        "lecture",
        &[
            SceneChange { time: 34.7, score: 0.42 },
            SceneChange { time: 65.0, score: 1.0 },
        ],
    )
    .unwrap();

    // `time\tscore`, the time with two decimals like every other time in these files.
    assert_eq!(read(&t.scenes_tsv("lecture")), "34.70\t0.42\n65.00\t1\n");
    let back = requests::read_scenes(&t, "lecture").unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!((back[0].time, back[0].score), (34.7, 0.42));
    assert_eq!((back[1].time, back[1].score), (65.0, 1.0));

    // No file is no scene changes: a video of one shot has none.
    assert!(requests::read_scenes(&t, "never").unwrap().is_empty());
    requests::write_scenes(&t, "oneshot", &[]).unwrap();
    assert_eq!(read(&t.scenes_tsv("oneshot")), "");
    assert!(requests::read_scenes(&t, "oneshot").unwrap().is_empty());

    // A row that is not two numbers names the row it failed on.
    plant(&t.scenes_tsv("lecture"), "34.70\n");
    let err = requests::read_scenes(&t, "lecture").unwrap_err();
    assert!(err.contains("scenes.tsv"), "{err}");
}
