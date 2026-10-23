//! §04-prepare.md §5, bullets 3, 4 and 5 — the rules about ANSWERS, marks and missing speech.
//!
//! These are the clauses §5 states in one line each and the rest of the spec spells across F1.4–F1.10; the
//! behaviour itself lives in `llm_cache`, `fix_transcripts`, `tools::retakes`, `tools::textedit`,
//! `transcribe`, `asr` and `diar_scan`, and each of those has its own rule file. What this file owns is the
//! §5 wording itself: one test per clause, reading the program's own sentence or its own file, so a
//! reworded refusal or a cache that started storing junk fails somewhere that names §5.
//!
//! No window, no Application, no socket: every clause below settles in a function or a file in a private
//! temp folder.

use std::fs;
use std::path::PathBuf;

use crate::common;
use common::temp_dir_created as temp_dir;
use naivepost::asr;
use naivepost::diar_scan;
use naivepost::diarize;
use naivepost::fix_transcripts;
use naivepost::layout::Tree;
use naivepost::llm_cache::{self, Stored};
use naivepost::retakes;
use naivepost::textfmt::{self, Line};
use naivepost::tools::retakes::{Line as MarkedLine, Marks, Trim};
use naivepost::tools::textedit::{self, Side};
use naivepost::transcribe;
use serde_json::json;

/// A project folder of §1's shape, unique per test so parallel tests cannot read each other's files.
fn tree(tag: &str) -> Tree {
    let dir: PathBuf = temp_dir(tag);
    Tree::new(dir.join("session.naivepost")).expect("a .naivepost folder is a project")
}

// ---- bullet 3: only usable answers are cached ----------------------------------------

/// §5: *"Only usable answers are cached"*. An empty or whitespace reply is not an answer — it is a model
/// that refused, timed out, or answered nothing — and filing it would make every later run replay the
/// silence. The cache's own gate is [`llm_cache::usable`]; these are the two doors that consult it.
#[test]
fn sec_04_prepare_5_rules_b3_only_a_usable_answer_reaches_the_cache() {
    let tree = tree("b3-cache");
    let parts = vec![json!("the system prompt"), json!("the material")];

    let written = llm_cache::store(&tree, "textedit", &parts, "a real answer");
    assert_eq!(written, Stored::Written, "a usable answer is filed");
    let filed = tree.cache_llm("textedit", &llm_cache::key(&parts).expect("a key"));
    assert_eq!(fs::read_to_string(&filed).expect("filed"), "a real answer");

    // The same question with an unusable answer: no new file, and `usable` is the door that said no.
    for junk in ["", "   ", "\n\t "] {
        assert!(!llm_cache::usable(junk), "{junk:?} is not an answer");
        let refused = llm_cache::store(&tree, "textedit", &parts, junk);
        assert_eq!(
            refused,
            Stored::RefusedEmpty,
            "{junk:?} must be refused, not filed"
        );
    }
    // A refusal leaves nothing on disk: not a file for the refusal, and not one for the parts.
    let step_dir = tree.cache_llm("empty-step", &llm_cache::key(&parts).expect("a key"));
    assert_eq!(
        llm_cache::store_at(&tree, "empty-step", "deadbeef", ""),
        Stored::RefusedEmpty,
        "store_at consults the same gate"
    );
    assert!(!step_dir.exists(), "an unusable reply wrote no file at all");
    assert!(
        !tree.cache_llm("empty-step", "deadbeef").exists(),
        "and no empty file either"
    );
    // Parts that cannot be keyed write nothing either: nothing to find the answer by again.
    assert_eq!(
        llm_cache::store(&tree, "textedit", &[], "answer"),
        Stored::RefusedNoKey
    );
}

// ---- bullet 3: the fix pass changes text only ----------------------------------------

/// §5: *"the fix pass changes text only"*. A fixed row keeps its place on the clock and its speaker; a
/// respelling that moved a time would move a cut, and one that moved a speaker would mislabel the video.
/// `textfmt::Line` has no field for anything else, which is the rule in type form — the test pins that the
/// writer really leaves the other three alone.
#[test]
fn sec_04_prepare_5_rules_b3_the_fix_pass_changes_text_only() {
    let tree = tree("b3-text");
    let heard = [
        Line {
            start: 0.0,
            end: 1.5,
            speaker: "NARRATOR".into(),
            text: "teh first".into(),
        },
        Line {
            start: 2.0,
            end: 3.25,
            speaker: "1".into(),
            text: "sekon line".into(),
        },
    ];
    let fixed = vec![
        Line {
            start: 0.0,
            end: 1.5,
            speaker: "NARRATOR".into(),
            text: "the first".into(),
        },
        Line {
            start: 2.0,
            end: 3.25,
            speaker: "1".into(),
            text: "second line".into(),
        },
    ];
    fix_transcripts::write_fixed(&tree, "cam.mkv", true, false, &fixed).expect("the fixer writes");

    let read_back = textfmt::read_lines(&tree.transcript_fixed_tsv("cam.mkv")).expect("readable");
    assert_eq!(read_back.len(), fixed.len());
    for (before, after) in heard.iter().zip(&read_back) {
        assert_eq!(after.start, before.start, "start second moved: {after:?}");
        assert_eq!(after.end, before.end, "end second moved: {after:?}");
        assert_eq!(after.speaker, before.speaker, "speaker moved: {after:?}");
        assert_ne!(after.text, before.text, "only the text was meant to change");
    }
    assert_eq!(
        read_back, fixed,
        "the respelled rows are what landed, times and speakers intact"
    );
}

// ---- bullet 3: the retake pass answers in line numbers -------------------------------

/// §5: *"the retake pass answers in line numbers"*. The model is shown numbered lines and answers
/// `mark_abandoned(from, to, again)` in those numbers; converting them to seconds is the app's arithmetic
/// (§00-principles: a model is never asked for a timestamp it would have to compute). This is the
/// conversion, one mark, in both directions of the range.
#[test]
fn sec_04_prepare_5_rules_b3_a_retake_answer_in_line_numbers_becomes_seconds() {
    // Five spoken lines, one second each, gapless: line n spans [n-1, n].
    let lines: Vec<MarkedLine> = (1..=5)
        .map(|n| MarkedLine {
            n,
            start: f64::from(n) - 1.0,
            end: f64::from(n),
        })
        .collect();
    let mut marks = Marks::new(lines, 5.0);

    // The model's own answer shape: line numbers, `again` a line number too.
    let outcome = marks.mark_abandoned(2, 3, Some(5), Trim::none());
    assert!(
        !tool_refused(&outcome),
        "a mark inside the session is accepted: {outcome}"
    );
    let mark = &marks.marks()[0];
    assert_eq!(
        (mark.from, mark.to),
        (2, 3),
        "the mark keeps the LINE numbers the model sent"
    );
    assert_eq!(mark.again, Some(5), "and `again` is a line number too");
    // The seconds those two lines span, which is what the file and the cut use.
    assert_eq!(
        mark.removed,
        (1.0, 3.0),
        "lines 2..3 are seconds 1.0..3.0: {:?}",
        mark.removed
    );
    assert_eq!(
        marks.marked_seconds(),
        2.0,
        "the marked total is the seconds, not the line count"
    );

    // A line number outside the session is refused rather than guessed at: a model that invents a line
    // has misread the brief, and answering for it would mark arbitrary audio.
    let bogus = marks.mark_abandoned(2, 99, None, Trim::none());
    assert!(
        tool_refused(&bogus),
        "line 99 of a five-line session is refused: {bogus}"
    );
    assert_eq!(marks.marks().len(), 1, "and nothing was added");
}

/// Does a tool's answer say "refused"? `tools::error` serialises `{"error": "…"}`, so a refusal is
/// recognised by that shape rather than by a prefix that could be reworded.
fn tool_refused(answer: &str) -> bool {
    answer.starts_with("{\"error\"")
}

// ---- bullet 3: the join pass can only delete -----------------------------------------

/// §5: *"the join pass can only delete"*. `tool:textedit`'s whole tool surface is `get_words`,
/// `drop_words`, `keep_join` and `finish` — there is no call that inserts, replaces or rewrites a word, so
/// an answer cannot put a word into the video that was never said. This pins that the ONLY mutating verb
/// removes words from what was shown, and that even it is bounded to what was shown.
#[test]
fn sec_04_prepare_5_rules_b3_a_join_answer_can_only_delete() {
    let before: Vec<&str> = "the first take trailed off".split(' ').collect();
    let after: Vec<&str> = "and the next one begins here".split(' ').collect();
    let mut join = textedit::Join::new(&before, &after);

    // Shown words are all it can talk about; a drop removes the words NEAREST the join, which is the
    // rule that keeps a cut from reaching past the stumble it is repairing.
    let shown = join.shown_count(Side::Before) + join.shown_count(Side::After);
    join.drop_words(Side::Before, 2);
    let taken = join.words_taken(Side::Before);
    assert_eq!(
        taken,
        vec!["trailed", "off"],
        "the two words before the join are what go: {taken:?}"
    );
    assert_eq!(
        join.dropped_count(Side::Before),
        2,
        "and the count is what it removed"
    );
    assert!(
        join.words_taken(Side::After).is_empty(),
        "the other side stands until it is dropped too"
    );
    // Asking for more than was shown is refused — the answer cannot reach past the window it was given,
    // which is the same boundary from the other side: it can delete at most what it saw.
    let too_many = join.drop_words(Side::After, shown + 10);
    assert!(
        tool_refused(&too_many),
        "dropping more than was shown must be refused: {too_many}"
    );
    assert!(
        too_many.contains("only"),
        "and say how many were shown: {too_many}"
    );
    // A kept join keeps every shown word: the pass's other answer is to change nothing.
    let mut kept = textedit::Join::new(&before, &after);
    kept.keep_join();
    assert!(kept.kept(), "keep_join is the no-change answer");
    assert_eq!(
        kept.dropped_count(Side::Before),
        0,
        "and it deletes nothing"
    );
}

// ---- bullet 3: "prefer the later saying" is the matcher's direction -------------------

/// §5: *"'prefer the later saying' is the direction of the match, not a request to the model"*. The dedupe
/// rule behind a doubled saying is the app's own arithmetic — `tool:textedit`'s `dedupe_joins` walks the
/// kept words either side of a cut and un-keeps the EARLIER copy, because what was said last is what the
/// speaker meant to keep — and the note it leaves names which one went. The model is never asked to choose;
/// asking it would make a coin flip out of something the word list already settles.
#[test]
fn sec_04_prepare_5_rules_b3_the_later_saying_is_the_direction_of_the_match() {
    let note = textedit::dedupe_note(12.0, "and that is the point");
    assert_eq!(
        note,
        "12: \"and that is the point\" said again straight after the cut -- the earlier one goes",
        "§6's wording, and it names WHICH copy goes"
    );

    // The direction in code: `a b | c | a b` — the run at the cut is the earlier copy of what follows it,
    // and it is the earlier one whose kept flags go, never the later one's.
    let words: Vec<String> = ["a", "b", "c", "a", "b"]
        .iter()
        .map(|w| (*w).to_string())
        .collect();
    let times: Vec<(f64, f64)> = (0..5).map(|i| (f64::from(i), f64::from(i) + 0.5)).collect();
    let mut kept = vec![true, true, false, true, true];
    let notes = textedit::dedupe_joins(&words, &times, &mut kept);
    assert_eq!(
        kept,
        vec![false, false, false, true, true],
        "the copy BEFORE the cut was un-kept; the later copy stands: {kept:?}"
    );
    assert_eq!(notes.len(), 1, "one note, naming the doubled saying");
    assert!(
        notes[0].contains("the earlier one goes"),
        "the note says which went: {notes:?}"
    );
    // The later copy's words are all still kept, which is the whole direction of the match.
    assert!(
        kept[3] && kept[4],
        "what was said last is what the video keeps"
    );
}

// ---- bullet 4: marks are markings; an empty file means "asked, none" ------------------

/// §5: *"Marks are markings, never deletions; an empty marks file means 'asked, none'"*. Two halves,
/// both about files: a refused pass and a nothing-found pass still WRITE `retakes.tsv`, empty, because the
/// file's presence is the only thing that tells the next step the pass ran; and a marking pass never
/// touches the text files it is about — the words stay in the transcript, always.
#[test]
fn sec_04_prepare_5_rules_b4_an_empty_marks_file_means_asked_none() {
    let tree = tree("b4-marks");
    // The text the marks are ABOUT, on disk before the pass.
    let session_txt = tree.session_txt();
    fs::create_dir_all(session_txt.parent().expect("a parent")).expect("the folder");
    fs::write(
        &session_txt,
        "00:00-00:01 | mm:ss] NARRATOR: the words as said\n",
    )
    .expect("session.txt");
    let before = fs::read(&session_txt).expect("bytes");

    // S6's ceiling refusal: the pass refuses EVERYTHING and still owes the file. Nine spoken lines is
    // enough to hold a retake (§F1.9 S1's gate), so the ceiling is what answers here.
    let spoken: Vec<MarkedLine> = (1..=9)
        .map(|n| MarkedLine {
            n,
            start: f64::from(n),
            end: f64::from(n) + 1.0,
        })
        .collect();
    let refused = retakes::stage_marking(true, &spoken, &[], 90.0, 100.0);
    assert_eq!(
        refused,
        vec![retakes::ceiling_refusal(90.0, 100.0)],
        "the refusal is the whole log"
    );
    assert!(
        refused[0].contains("refused, nothing is marked"),
        "and it says nothing was marked: {}",
        refused[0]
    );
    retakes::write_marks(&tree, &[], &[], &[]).expect("the empty file is written on purpose");
    assert!(
        tree.retakes_tsv().is_file(),
        "§F1.9: the empty file IS written, not skipped"
    );
    assert_eq!(
        fs::read_to_string(tree.retakes_tsv()).expect("readable"),
        "",
        "and it is EMPTY — the marks list is empty, not the file missing"
    );
    // Read back as the "asked, none" it means: an Ok of nothing, distinguishable from never-asked only
    // by the file existing.
    assert_eq!(
        retakes::read_marks(&tree.retakes_tsv()).expect("parses"),
        Vec::<textfmt::Retake>::new(),
        "asked, none"
    );
    assert!(
        tree.retakes_tsv().try_exists().expect("exists"),
        "which is what tells Cut the pass ran"
    );

    // And the words it marked are all still there: marking never deletes text.
    assert_eq!(
        fs::read(&session_txt).expect("bytes"),
        before,
        "session.txt untouched by a marking pass"
    );

    // S1's too-few-lines path says the same thing in its own sentence, and owes the same file.
    let three: Vec<MarkedLine> = (1..=3)
        .map(|n| MarkedLine {
            n,
            start: f64::from(n),
            end: f64::from(n) + 1.0,
        })
        .collect();
    assert!(
        retakes::too_few(&three),
        "three spoken lines cannot hold a retake"
    );
    assert!(
        retakes::empty_marks_log().contains("nothing to mark"),
        "{}",
        retakes::empty_marks_log()
    );
}

// ---- bullet 5: no aligner + no word times is a hard failure; no speech is not ---------

/// §5: *"no ASR word times and no aligner is a hard failure named plainly"*. The sentence names the model
/// that answered and names the remedy, because the fix is a setting the person can reach — and §6 spells
/// it as a hard failure of that source, not a warning beside an empty file.
#[test]
fn sec_04_prepare_5_rules_b5_no_word_times_and_no_aligner_is_named_plainly() {
    let plain = transcribe::no_word_times("whisper.cpp", false);
    assert!(
        plain.contains("whisper.cpp"),
        "it names the ASR that answered without times: {plain}"
    );
    assert!(
        plain.contains("timed no words"),
        "and what is missing: {plain}"
    );
    assert!(
        plain.contains("no aligner is registered"),
        "and that no aligner exists: {plain}"
    );
    assert!(plain.contains("register one"), "and the remedy: {plain}");
    // The aligner-was-asked variant points at the line above instead of repeating a reason it does not
    // know; both spellings exist because the remedy differs.
    let variant = transcribe::no_word_times("qwen3-asr", true);
    assert!(
        variant.contains("the aligner left no times either"),
        "{variant}"
    );
    assert!(
        !variant.contains("register one"),
        "the aligner is registered here, so that is not the fix"
    );

    // A hard failure arrives as an `Err`, through the pass that decides alignment: an ASR with no word
    // times and no aligner stops the source rather than writing an untimed transcript.
    let failure: Result<(), String> = Err(transcribe::no_word_times("whisper.cpp", false));
    assert!(failure.is_err(), "it is a failure, not a logged warning");
}

/// §5: *"diarization failure is fatal after the ladder"*. Every rung refusing is the model being asked
/// for more reference than any window can hold: `diar_scan::run` walks all three and then returns `Err`,
/// naming the model and the last refusal's two numbers. One rung succeeding is not fatal at all.
#[test]
fn sec_04_prepare_5_rules_b5_diarization_is_fatal_only_after_the_whole_ladder() {
    assert_eq!(
        diarize::ladder().len(),
        3,
        "the ladder §5 says must be exhausted first"
    );

    // Every rung refuses: fatal, and named.
    let crowded = || {
        let mut turns = speaks(0, 1.0, 8.0, 6.0, 4);
        for slot in 1..7 {
            turns.extend(speaks(slot, 2.0 + f64::from(slot), 8.0, 6.0, 4));
        }
        turns
    };
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f64>::new()));
    let mut scan = {
        let asked = asked.clone();
        move |windows: &[(f64, f64)]| {
            asked
                .lock()
                .unwrap()
                .push(windows.iter().map(|(f, t)| t - f).fold(0.0f64, f64::max));
            Ok(windows.iter().map(|_| crowded()).collect())
        }
    };
    let mut place = |_: (f64, f64), _: f64| Ok(vec![]);
    let outcome = diar_scan::run(
        600.0,
        "sortformer-diar",
        "talk",
        &mut scan,
        &mut place,
        |_| (),
    );
    let error = match outcome {
        Ok(rung) => panic!(
            "a room no window can anchor must not succeed: {:?}",
            rung.window
        ),
        Err(error) => error,
    };
    assert!(
        error.contains("sortformer-diar"),
        "it names the model: {error}"
    );
    assert!(error.contains("anchor too long"), "and the reason: {error}");
    assert_eq!(
        asked.lock().unwrap().len(),
        diarize::ladder().len(),
        "and it was asked about EVERY rung before failing: {:?}",
        asked.lock().unwrap()
    );

    // One rung serving is the same call answering Ok: fatal only after the ladder ran out.
    let mut scan_ok =
        |windows: &[(f64, f64)]| Ok(windows.iter().map(|_| two_voices()).collect::<Vec<_>>());
    let mut place_ok = |_: (f64, f64), _: f64| Ok(vec![]);
    let rung = diar_scan::run(
        600.0,
        "sortformer-diar",
        "talk",
        &mut scan_ok,
        &mut place_ok,
        |_| (),
    )
    .expect("a room the top rung can hold is not a failure");
    assert_eq!(rung.window, 90.0, "the first rung served");
}

/// §5: *"no speech is a real case at every layer"*. Three layers, three sentences, none of them an error:
/// ASR heard nothing (an empty transcript), diarization found no voice to anchor (an empty `turns.json`),
/// and the fixer reads a missing transcript as a recording with nothing in it. Each answers Ok with its
/// own line, because a silent screen capture is an ordinary thing to import.
#[test]
fn sec_04_prepare_5_rules_b5_no_speech_is_a_real_case_at_every_layer() {
    // Layer 1: the ASR. §F1.4's S4 sentence, and nothing failing around it.
    let heard = asr::no_speech_log("slides");
    assert_eq!(
        heard, ">>> [slides] no speech found -- an empty transcript",
        "§F1.4 S4's wording"
    );
    assert!(
        heard.starts_with(">>> "),
        "it is a log line, not a failure line (!!!): {heard}"
    );

    // Layer 2: diarization. `plan` answers NoVoice and the scan returns Ok with no turns — the empty
    // turns.json is the resume marker that says "nobody spoke".
    assert_eq!(
        diarize::no_voice_log("slides"),
        ">>> [slides] diarization: no voice long enough to anchor -- no turns"
    );
    let mut scan_none =
        |windows: &[(f64, f64)]| Ok(windows.iter().map(|_| vec![]).collect::<Vec<_>>());
    let mut place_none = |_: (f64, f64), _: f64| Ok(vec![]);
    let rung = diar_scan::run(
        600.0,
        "sortformer-diar",
        "slides",
        &mut scan_none,
        &mut place_none,
        |_| (),
    )
    .expect("no voice is an answer, not a failure");
    assert!(rung.turns.is_empty(), "and the answer is no turns at all");

    // Layer 3: the fixer. A source with no transcript file is a recording with nothing heard, so reading
    // it answers Ok with no lines rather than an Err about a missing file.
    let tree = tree("b5-silent");
    let lines = fix_transcripts::load_transcript(&tree, "never_recorded.wav")
        .expect("an absent transcript is a real case, not an error");
    assert!(
        lines.is_empty(),
        "nothing was heard, so nothing to fix: {lines:?}"
    );
}

/// A turn in seconds, which is how a test thinks.
fn turn(slot: u32, from: f64, to: f64) -> naivepost::requests::Turn {
    naivepost::requests::Turn {
        start_sample: (from * naivepost::transcribe::SAMPLE_RATE as f64) as u64,
        end_sample: (to * naivepost::transcribe::SAMPLE_RATE as f64) as u64,
        speaker_id: slot,
    }
}

/// One voice speaking `seconds` every `every` seconds from `from` — long enough to count as a voice.
fn speaks(
    slot: u32,
    from: f64,
    every: f64,
    seconds: f64,
    times: usize,
) -> Vec<naivepost::requests::Turn> {
    (0..times)
        .map(|at| {
            turn(
                slot,
                from + f64::from(at as u32) * every,
                from + f64::from(at as u32) * every + seconds,
            )
        })
        .collect()
}

/// A two-voice room any rung can anchor.
fn two_voices() -> Vec<naivepost::requests::Turn> {
    let mut turns = speaks(0, 1.0, 8.0, 6.0, 4);
    turns.extend(speaks(1, 3.0, 8.0, 6.0, 4));
    turns
}
