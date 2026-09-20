//! F1.3 Per source: audio → text → word times → speakers → segments — spec/04-prepare.md.
//!
//! The steps live in [`naivepost::transcribe`]; ▶ only hands it ffmpeg's length, the ASR box and the
//! diarizer. So these checks assert what the log says, which files exist afterwards and how words and
//! turns become segments — never that a server was actually asked (spec/00-principles.md §5).

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use naivepost::layout::Tree;
use naivepost::requests::{self, AlignedDoc, Turn, Word, WordsDoc};
use naivepost::textfmt;
use naivepost::transcribe::{self, Align};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = PathBuf::from(format!(
        "/tmp/naivepost-transcribe-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project folder with the one source this flow reads, so "already on disk" is a real file rather
/// than a path that merely looks plausible.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    // S1's decode, as the caller leaves it: the folders under `inputs/` are made by whoever writes
    // into them, so the file that stands for "the audio is ready" needs its parent first.
    fs::create_dir_all(tree.input_dir(SOURCE)).unwrap();
    fs::write(tree.voice16k_wav(SOURCE), b"pcm").unwrap();
    (root, tree)
}

const SOURCE: &str = "mic.wav";

fn word(text: &str, start_sample: u64, end_sample: u64) -> Word {
    Word {
        word: text.to_string(),
        start_sample,
        end_sample,
    }
}

/// Words at whole seconds, which keeps a test's arithmetic in seconds rather than samples.
fn words_at(spans: &[(&str, f64, f64)]) -> Vec<Word> {
    spans
        .iter()
        .map(|(text, start, end)| word(text, (start * 16000.0) as u64, (end * 16000.0) as u64))
        .collect()
}

fn turn(start: f64, end: f64, speaker: u32) -> Turn {
    Turn {
        start_sample: (start * 16000.0) as u64,
        end_sample: (end * 16000.0) as u64,
        speaker_id: speaker,
    }
}

fn turns(spans: &[(f64, f64, u32)]) -> Vec<Turn> {
    spans.iter().map(|(s, e, id)| turn(*s, *e, *id)).collect()
}

/// `glue_turns` over seconds, since a test's turns are written in seconds and the diarizer's answer
/// is samples.
fn spoken(spans: &[(f64, f64, u32)]) -> Vec<(f64, f64, String)> {
    transcribe::glue_turns(&turns(spans))
}

/// S1: the audio is mono 16 kHz — that is the rate every sidecar's samples are counted in — and its
/// length is said out loud before anything is asked of a server.
#[test]
fn f1_3_s1_the_audio_is_mono_16khz_and_said_out_loud() {
    assert_eq!(transcribe::SAMPLE_RATE, 16_000);
    assert_eq!(transcribe::seconds(16000), 1.0);
    assert_eq!(transcribe::seconds(8000), 0.5);
    assert_eq!(transcribe::seconds(0), 0.0);

    assert_eq!(
        transcribe::audio_log(SOURCE, 12.34),
        ">>> [mic.wav] 12.3 s of audio"
    );

    // S1's file is the caller's decode; this module only asks whether it is there yet.
    let (_root, tree) = project("s1");
    assert!(transcribe::has_audio(&tree, SOURCE));
    assert!(!transcribe::has_audio(&tree, "later.wav"));
}

/// S2 (`P.policy.minTakeSeconds`): a take under two seconds is a start and a stop. The same files an
/// empty recording gets, and no server is asked about a third of a second of nothing.
#[test]
fn f1_3_s2_a_short_take_is_written_up_as_silence_without_asking_anything() {
    // P.policy.minTakeSeconds
    assert_eq!(requests::MIN_TAKE_SECONDS, 2.0);
    assert!(requests::is_short(1.999, requests::MIN_TAKE_SECONDS));
    assert!(!requests::is_short(2.0, requests::MIN_TAKE_SECONDS));
    assert!(transcribe::is_short_take(1.2));

    let (_root, tree) = project("s2");
    let asked = AtomicBool::new(false);
    let passage = transcribe::run(
        &tree,
        SOURCE,
        1.2,
        "nemotron-asr",
        Align::None,
        || {
            asked.store(true, Ordering::SeqCst);
            Ok(WordsDoc::default())
        },
        || Ok(vec![]),
    )
    .expect("a short take is not a failure");

    assert_eq!(passage.logs[0], ">>> [mic.wav] 1.2 s of audio");
    assert_eq!(
        passage.logs[1],
        ">>> [mic.wav] 1.2 s long -- a start/stop, not a take: written up as silence"
    );
    assert_eq!(passage.logs.last().unwrap(), ">>> [mic.wav] 0 segments");
    assert!(!asked.load(Ordering::SeqCst), "no ASR for a start/stop");

    // Silence spelled the way §6 spells it, so every later step resumes rather than re-asking.
    assert_eq!(fs::read_to_string(tree.transcript_txt(SOURCE)).unwrap(), "\n");
    assert_eq!(fs::read_to_string(tree.words_json(SOURCE)).unwrap(), r#"{"text":""}"#);
    assert_eq!(fs::read_to_string(tree.turns_json(SOURCE)).unwrap(), "[]");
    // No asrchunks.json: it would claim an ASR request that never happened.
    assert!(!tree.asrchunks_json(SOURCE).exists());

    // And the two transcripts exist anyway — empty, because there is nothing to read.
    assert!(tree.transcript_tsv(SOURCE).exists());
    assert!(tree.transcript_srt(SOURCE).exists());
    assert_eq!(textfmt::read_lines(&tree.transcript_tsv(SOURCE)).unwrap(), vec![]);
    assert_eq!(transcribe::srt_text(&[]), "");
}

/// S3: `words.json` is this stage's resume marker, so its existence — not its contents — decides
/// whether the ASR is asked. An ASR that heard nothing writes empty text, and reading that as "not
/// done" would ask again on every run forever.
#[test]
fn f1_3_s3_asr_is_skipped_when_words_json_exists() {
    let (_root, tree) = project("s3-done");
    requests::write_words(
        &tree,
        SOURCE,
        &WordsDoc {
            text: "hello there".into(),
            words: words_at(&[("hello", 0.0, 0.4), ("there", 0.5, 0.9)]),
        },
    )
    .unwrap();

    let asked = AtomicBool::new(false);
    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::None,
        || {
            asked.store(true, Ordering::SeqCst);
            Ok(WordsDoc::default())
        },
        || Ok(vec![]),
    )
    .expect("a finished ASR is not a failure");

    assert_eq!(passage.logs[1], ">>> [mic.wav] ASR already done");
    assert!(!asked.load(Ordering::SeqCst), "the answer was on disk");

    // Absent, it is asked — and named with the model that answered.
    let (_root, tree) = project("s3-asked");
    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::None,
        || {
            Ok(WordsDoc {
                text: "hello there".into(),
                words: words_at(&[("hello", 0.0, 0.4), ("there", 0.5, 0.9)]),
            })
        },
        || Ok(vec![]),
    )
    .expect("the ASR answered");

    assert_eq!(passage.logs[1], ">>> [mic.wav] ASR (nemotron-asr)");
    // Written whole, since it is the marker: the next pass must not ask again.
    let saved = requests::read_words(&tree, SOURCE).unwrap().expect("words.json");
    assert_eq!(saved.text, "hello there");
}

/// S4: an aligner that fails while the ASR timed its own words costs precision, not the recording —
/// a warning, and the run carries on.
#[test]
fn f1_3_s4_a_failed_alignment_stands_on_the_asr_times_when_it_has_them() {
    let (_root, tree) = project("s4-stood");
    requests::write_words(
        &tree,
        SOURCE,
        &WordsDoc {
            text: "hello there".into(),
            words: words_at(&[("hello", 0.0, 0.4), ("there", 0.5, 0.9)]),
        },
    )
    .unwrap();

    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::Failed("connect refused".into()),
        || Ok(WordsDoc::default()),
        || Ok(vec![]),
    )
    .expect("the ASR's times are enough to carry on");

    let warning = passage
        .logs
        .iter()
        .find(|line| line.starts_with("!!! "))
        .expect("a warning about the aligner");
    assert!(warning.contains("align: connect refused"), "{warning}");
    assert!(warning.contains("the ASR's own times stand"), "{warning}");
    assert_eq!(passage.logs.last().unwrap(), ">>> [mic.wav] 1 segments");

    // The plain decision, spelled out.
    let doc = WordsDoc {
        text: "hello".into(),
        words: words_at(&[("hello", 0.0, 0.4)]),
    };
    assert_eq!(
        transcribe::align_outcome(Some("nope".into()), !doc.words.is_empty(), &doc.text),
        transcribe::Alignment::Stood
    );
}

/// S4: an aligner that fails with nothing else to time the words by is a dead end, and it names the
/// model rather than "a missing aligner" — which would send the user to the wrong setting.
#[test]
fn f1_3_s4_a_failed_alignment_without_word_times_is_a_plain_error() {
    let (_root, tree) = project("s4-fatal");
    requests::write_words(
        &tree,
        SOURCE,
        &WordsDoc {
            text: "hello there".into(),
            words: vec![],
        },
    )
    .unwrap();

    let err = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "whisper-large",
        Align::Failed("model not loaded".into()),
        || Ok(WordsDoc::default()),
        || Ok(vec![]),
    )
    .expect_err("words nobody can time cannot be transcribed");

    assert!(err.contains("align: model not loaded"), "{err}");
    assert!(err.contains("whisper-large"), "the model is named: {err}");
    assert!(err.contains("cannot be transcribed without it"), "{err}");
    // And the pass stopped where it was: no half-built transcripts.
    assert!(!tree.transcript_tsv(SOURCE).exists());
}

/// S4: with no words at all there is nothing to align, so a failing aligner says nothing — the
/// silence case has its own line and two lines about one nothing would read as trouble.
#[test]
fn f1_3_s4_no_transcript_means_nothing_to_align() {
    assert_eq!(
        transcribe::align_outcome(Some("nope".into()), false, ""),
        transcribe::Alignment::Skipped
    );
    assert_eq!(
        transcribe::align_outcome(Some("nope".into()), false, "   \n"),
        transcribe::Alignment::Skipped
    );
    // No error at all is the ordinary answer.
    assert_eq!(transcribe::align_outcome(None, false, ""), transcribe::Alignment::Aligned);

    let (_root, tree) = project("s4-silent");
    requests::write_words(&tree, SOURCE, &WordsDoc { text: String::new(), words: vec![] }).unwrap();

    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::Failed("model not loaded".into()),
        || Ok(WordsDoc::default()),
        || Ok(vec![]),
    )
    .expect("nothing to align is not a failure");
    assert!(
        passage.logs.iter().all(|line| !line.starts_with("!!! ")),
        "silent, as the spec says: {:?}",
        passage.logs
    );

    // No aligner served at all is the same quiet answer — and a different reason, which is why they
    // are two cases rather than one.
    let (_root, tree) = project("s4-none");
    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::None,
        || Ok(WordsDoc { text: "hi".into(), words: words_at(&[("hi", 0.0, 0.3)]) }),
        || Ok(vec![]),
    )
    .expect("a session with no aligner is a working setup");
    assert!(passage.logs.iter().all(|line| !line.contains("align")), "{:?}", passage.logs);

    // And an answer already on disk is read instead of asked for again.
    let (_root, tree) = project("s4-aligned");
    requests::write_words(
        &tree,
        SOURCE,
        &WordsDoc { text: "hi".into(), words: words_at(&[("hi", 0.0, 0.3)]) },
    )
    .unwrap();
    requests::write_aligned(
        &tree,
        SOURCE,
        &AlignedDoc { words: words_at(&[("hi", 0.1, 0.35)]) },
    )
    .unwrap();
    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::Failed("would have been asked".into()),
        || Ok(WordsDoc::default()),
        || Ok(vec![]),
    )
    .expect("the answer was on disk");
    assert!(passage.logs.iter().all(|line| !line.starts_with("!!! ")), "{:?}", passage.logs);
    // The aligner's times are the ones used: 0.1 s, not the ASR's 0.0.
    let lines = textfmt::read_lines(&tree.transcript_tsv(SOURCE)).unwrap();
    assert_eq!(lines[0].start, 0.1);
}

/// S5: `turns.json` is diarization's marker, so its existence — again not its contents, since `[]` is
/// what "nobody spoke" is written as — decides whether the server is asked.
#[test]
fn f1_3_s5_diarization_is_skipped_when_turns_json_exists() {
    let (_root, tree) = project("s5-done");
    fs::write(
        tree.turns_json(SOURCE),
        serde_json::to_string(&turns(&[(0.0, 2.0, 1)])).unwrap(),
    )
    .unwrap();
    requests::write_words(
        &tree,
        SOURCE,
        &WordsDoc { text: "hello".into(), words: words_at(&[("hello", 0.5, 0.9)]) },
    )
    .unwrap();

    let asked = AtomicBool::new(false);
    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::None,
        || Ok(WordsDoc::default()),
        || {
            asked.store(true, Ordering::SeqCst);
            Ok(turns(&[(0.0, 2.0, 0)]))
        },
    )
    .expect("the turns were already there");

    assert!(!asked.load(Ordering::SeqCst), "diarization was not asked");
    // And the speaker on disk is the one used: speaker_id 1, not what a new answer would have said.
    let lines = textfmt::read_lines(&tree.transcript_tsv(SOURCE)).unwrap();
    assert_eq!(lines[0].speaker, "SPEAKER_01");
    assert_eq!(passage.logs.last().unwrap(), ">>> [mic.wav] 1 segments");

    // Absent, it is asked — and its answer is left on disk for the next pass.
    let (_root, tree) = project("s5-asked");
    requests::write_words(
        &tree,
        SOURCE,
        &WordsDoc { text: "hello".into(), words: words_at(&[("hello", 0.5, 0.9)]) },
    )
    .unwrap();
    transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::None,
        || Ok(WordsDoc::default()),
        || Ok(turns(&[(0.0, 2.0, 2)])),
    )
    .expect("diarization answered");
    assert_eq!(requests::read_turns(&tree, SOURCE).unwrap(), turns(&[(0.0, 2.0, 2)]));
}

/// S6: a segment ends at a speaker change, at a silence over `mergeGap`, or at the length cap — the
// three things that make subtitles readable rather than one long line.
#[test]
fn f1_3_s6_words_become_segments_at_a_speaker_change_a_gap_or_12s() {
    let nobody: Vec<(f64, f64, String)> = vec![];

    // A speaker change.
    let two_voices = spoken(&[(0.0, 1.0, 0), (1.0, 2.0, 1)]);
    let lines = transcribe::segments(
        &words_at(&[("mine", 0.1, 0.4), ("yours", 1.1, 1.4)]),
        &two_voices,
    );
    assert_eq!(lines.len(), 2);
    assert_eq!((lines[0].speaker.as_str(), lines[1].speaker.as_str()), ("SPEAKER_00", "SPEAKER_01"));

    // A silence over 0.7 s, and one that is not: the bound is `>`, so exactly 0.7 stays together.
    let lines = transcribe::segments(
        &words_at(&[("one", 0.0, 0.4), ("two", 1.2, 1.5)]),
        &nobody,
    );
    assert_eq!(lines.len(), 2, "0.8 s of silence ends a segment");
    let lines = transcribe::segments(
        &words_at(&[("one", 0.0, 0.4), ("two", 1.2, 1.5)]),
        &nobody,
    );
    assert_eq!(lines.len(), 2, "0.8 s of silence ends a segment");
    // The bound itself is `>`, not `>=`: exactly 0.7 s of pause keeps the phrase in one subtitle.
    let lines = transcribe::segments(
        &words_at(&[("one", 0.0, 1.4), ("two", 2.05, 2.5)]),
        &nobody,
    );
    assert_eq!(lines.len(), 1, "650 ms of pause does not: {:?}", lines);

    // And the cap, however unbroken the speech is.
    let spoken: Vec<(String, f64, f64)> = (0..25)
        .map(|n| (format!("w{n}"), n as f64 * 0.5, n as f64 * 0.5 + 0.3))
        .collect();
    let words: Vec<Word> = spoken
        .iter()
        .map(|(text, start, end)| word(text, (start * 16000.0) as u64, (end * 16000.0) as u64))
        .collect();
    let lines = transcribe::segments(&words, &nobody);
    assert!(lines.len() >= 2, "12 s of unbroken speech is capped: {:?}", lines);
    assert!(
        lines.iter().all(|line| line.end - line.start <= transcribe::MERGE_MAX_LEN + 0.001),
        "{:?}",
        lines
    );
}

/// S6: the ASR stretches a word's end across the silence after it, so an end is worth nothing past two
/// seconds after its start — unstretched, one such word would hold its segment open forever.
#[test]
fn f1_3_s6_a_long_word_end_is_clamped_to_two_seconds() {
    let nobody: Vec<(f64, f64, String)> = vec![];
    let lines = transcribe::segments(&words_at(&[("stretched", 0.0, 9.0)]), &nobody);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].start, 0.0);
    assert_eq!(lines[0].end, 2.0, "clamped to start + 2 s");

    let (start, end) = transcribe::word_span(&words_at(&[("stretched", 1.0, 30.0)])[0]);
    assert_eq!((start, end), (1.0, 3.0));
}

/// S6: who said a word — the turn it overlaps most, else the nearest turn within 1 s, else one voice
/// when diarization found none, else "?" for the space between turns.
#[test]
fn f1_3_s6_speaker_by_greatest_overlap_then_the_nearest_turn() {
    // The diarizer's times are samples at [`transcribe::SAMPLE_RATE`], which `glue_turns` converts.
    // Speaker 1 first on purpose: the winner is whoever holds most of the word, not whoever is asked
    // about first.
    let overlapping = spoken(&[(4.0, 6.0, 1), (0.0, 5.0, 0)]);
    assert_eq!(overlapping.len(), 2, "different speakers are never glued");

    // The turn sharing the most time wins, whichever came first: 0.8 s of one against 1.6 s of the
    // other inside a word spanning their overlap.
    assert_eq!(transcribe::speaker_of(4.2, 5.8, &overlapping), "SPEAKER_01");
    assert_eq!(transcribe::speaker_of(0.2, 0.9, &overlapping), "SPEAKER_00");

    // A word between two turns takes the nearer one when it is within 1 s: 0.2 s from the first and
    // 0.8 s from the second here, so the ordering of the list must not decide it either.
    let apart = spoken(&[(2.4, 3.0, 1), (0.0, 1.0, 0)]);
    assert_eq!(transcribe::speaker_of(1.2, 1.3, &apart), "SPEAKER_00");
    // ...and nobody's when it is further than that.
    let far = spoken(&[(0.0, 1.0, 0), (5.0, 6.0, 1)]);
    assert_eq!(transcribe::speaker_of(3.0, 3.2, &far), "?");

    // Such a word keeps the running speaker rather than being shown as a name — 0.4 s after the
    // previous word, so it is still the same subtitle.
    let lines = transcribe::segments(
        &words_at(&[("first", 0.1, 0.4), ("stray", 0.8, 1.0)]),
        &far,
    );
    assert_eq!(lines.len(), 1, "a stray word joins the phrase before it: {lines:?}");
    assert_eq!(lines[0].speaker, "SPEAKER_00");
    assert_eq!(lines[0].text, "first stray");

    // And when the pause does start a new subtitle, that line carries the previous voice too rather
    // than a question mark: it is who was speaking, and "?" would be shown as somebody's name.
    let lines = transcribe::segments(
        &words_at(&[("first", 0.1, 0.4), ("stray", 3.0, 3.2)]),
        &far,
    );
    assert_eq!(lines.len(), 2, "2.6 s of silence ends a subtitle: {lines:?}");
    assert_eq!(lines[1].speaker, "SPEAKER_00");

    // No turns at all is a recording diarization found no voices in, whose ASR heard words anyway:
    // one voice, named as a diarized session would name it.
    let nobody: Vec<(f64, f64, String)> = vec![];
    assert_eq!(transcribe::speaker_of(0.0, 1.0, &nobody), "SPEAKER_00");
    let lines = transcribe::segments(&words_at(&[("hello", 0.0, 0.4)]), &nobody);
    assert_eq!(lines[0].speaker, "SPEAKER_00");

    // A segment that opened on an unplaceable word is named by the first word that could be placed.
    // A segment opened on an unplaceable word is named by the first word that could be placed — and
    // the gap keeps it a segment of its own. The turns are 0-1 s and 5-6 s here, so "stray" at 3 s is
    // nobody's (2 s from either) while "placed" at 5.1 s belongs to the second voice.
    let lines = transcribe::segments(
        &words_at(&[("stray", 3.0, 3.2), ("placed", 5.1, 5.4)]),
        &far,
    );
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0].speaker, "?", "nothing before it to keep: {lines:?}");
    assert_eq!(lines[1].speaker, "SPEAKER_01");
}

/// S5/S6: the diarizer splits where a speaker breathed. Gluing same-speaker turns first is what stops
/// one word of one sentence being given to two people.
#[test]
fn f1_3_s6_same_speaker_turns_close_together_are_one_turn() {
    let glued = spoken(&[(0.0, 1.0, 0), (1.4, 2.0, 0)]);
    assert_eq!(glued.len(), 1, "0.4 s apart is one turn: {glued:?}");
    assert_eq!(glued[0], (0.0, 2.0, "SPEAKER_00".to_string()));

    // Past the bound they are two turns — and a word in neither is nobody's.
    let glued = spoken(&[(0.0, 1.0, 0), (1.6, 2.0, 0)]);
    assert_eq!(glued.len(), 2, "0.6 s apart is two turns: {glued:?}");

    // Different speakers are never glued, however close.
    let glued = spoken(&[(0.0, 1.0, 0), (1.0, 2.0, 1)]);
    assert_eq!(glued.len(), 2);

    // Samples become seconds at the one rate everything else uses.
    let glued = spoken(&[(0.0, 1.5, 3)]);
    assert_eq!(glued[0], (0.0, 1.5, "SPEAKER_03".to_string()));
}

/// S6: the two transcript files come off one list of segments, which is what keeps them agreeing, and
/// the count that ends the log is that list's length.
#[test]
fn f1_3_s6_the_two_transcript_files_and_the_count() {
    let (_root, tree) = project("s6-files");
    requests::write_words(
        &tree,
        SOURCE,
        &WordsDoc {
            text: "one two".into(),
            words: words_at(&[("one", 0.0, 0.4), ("two", 1.2, 1.5)]),
        },
    )
    .unwrap();
    fs::write(tree.turns_json(SOURCE), "[]").unwrap();

    let passage = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron-asr",
        Align::None,
        || Ok(WordsDoc::default()),
        || Ok(vec![]),
    )
    .expect("the pass finished");

    assert_eq!(passage.logs.last().unwrap(), ">>> [mic.wav] 2 segments");
    assert_eq!(transcribe::segments_log(SOURCE, 2), ">>> [mic.wav] 2 segments");

    // The tsv reads back through the same reader Cut uses, times at two decimals.
    let lines = textfmt::read_lines(&tree.transcript_tsv(SOURCE)).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!((lines[0].start, lines[0].end), (0.0, 0.4));
    assert_eq!(lines[0].speaker, "SPEAKER_00");
    let tsv = fs::read_to_string(tree.transcript_tsv(SOURCE)).unwrap();
    assert!(tsv.starts_with("0.00\t0.40\tSPEAKER_00\tone\n"), "{tsv}");

    // The srt: number, times, speaker tag, text — and no tag for a word nobody was named for.
    let srt = fs::read_to_string(tree.transcript_srt(SOURCE)).unwrap();
    assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:00,400\n[SPEAKER_00] one\n\n"), "{srt}");
    assert!(srt.contains("\n2\n00:00:01,200 --> 00:00:01,500\n[SPEAKER_00] two\n"), "{srt}");

    let unnamed = transcribe::srt_text(&[textfmt::Line {
        start: 0.0,
        end: 1.0,
        speaker: "?".into(),
        text: "whoever".into(),
    }]);
    assert_eq!(unnamed, "1\n00:00:00,000 --> 00:00:01,000\nwhoever\n\n");

    // Timecodes past an hour, since the srt is what a person opens and a segment rarely ends inside
    // the first minute of a lecture.
    let long = transcribe::srt_text(&[textfmt::Line {
        start: 3661.5,
        end: 3671.25,
        speaker: "SPEAKER_00".into(),
        text: "late".into(),
    }]);
    assert!(long.contains("01:01:01,500 --> 01:01:11,250"), "{long}");
}
