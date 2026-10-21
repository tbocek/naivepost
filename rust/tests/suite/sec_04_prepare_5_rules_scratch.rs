//! §04-prepare.md §5 bullet 1, its second half: *"scratch dirs survive failure, go on success"*.
//!
//! Every stage that cuts audio to ask about it writes its pieces into a scratch folder of its own —
//! `asr/`, `align/`, `diar/` under `prepare/inputs/<source>/`, and `stems/<lane>.work` beside the split's
//! products. Both halves matter and neither is decoration:
//!
//! * **survives failure** — `asr/c03.wav` with its `.plan` is how a refused chunk is compared against the
//!   audio it was cut from without asking the server again; deleting on the way out would throw away the
//!   only evidence of what was asked.
//! * **goes on success** — once the answer is on disk the pieces are the stage's own mess, and leaving them
//!   means a later resume reads last run's chunks as this run's work.
//!
//! The keep-on-failure half falls out of WHERE the removal is called (after the marker write, so a failed
//! pass returns before it), which is why each check here drives both a failing and a whole successful pass
//! through the program's own door and asserts the folder both times. No window, no socket: every stage here
//! takes an `ask`/`diarize` closure — the seam the real ▶ also uses (`spec/00-principles.md` §5).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use naivepost::align;
use naivepost::asr::{self, Answer, Silence};
use naivepost::layout::Tree;
use naivepost::requests::{self, Turn, Word, WordsDoc};
use naivepost::separate_media as media;
use naivepost::transcribe::{self, Align};
use crate::common;
use common::temp_dir_created as temp_dir;

const SOURCE: &str = "mic.wav";

/// A project folder with S1's decode in place, so "the audio is ready" is a file rather than a guess.
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = temp_dir(tag);
    let dir = root.join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    fs::create_dir_all(tree.input_dir(SOURCE)).unwrap();
    fs::write(tree.voice16k_wav(SOURCE), b"pcm").unwrap();
    (root, tree)
}

/// Everything in a folder, sorted — printed in the assertion message so a wrong leftover names itself.
fn names(dir: &Path) -> Vec<String> {
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Loud for `duration`: no silences, so every piece is worth asking about.
fn loud(_: f64) -> Vec<Silence> {
    Vec::new()
}

/// One aligner-answer word, in the clip's own seconds.
fn aligned_word(text: &str, from: f64, to: f64) -> serde_json::Value {
    json!({"word": text, "start": from, "end": to})
}

/// §5 bullet 1, ASR: `asr/` keeps its chunks when a chunk is refused and is gone when the answer landed.
#[test]
fn sec_04_prepare_5_rules_s1_the_asr_scratch_goes_on_success_and_survives_failure() {
    // Failure half: the only chunk refuses, so its cut and plan must still be there to read.
    let (_root, tree) = project("s1-asr-fails");
    let err = asr::run(&tree, SOURCE, 60.0, "nemotron", "en", &loud(60.0), |_file| {
        Err("the box hung up".to_string())
    })
    .expect_err("a refusal reaches the caller");
    assert_eq!(err, "the box hung up");
    assert!(
        tree.asr_scratch(SOURCE).is_dir(),
        "survives failure: `asr/` is the record of what was asked"
    );
    assert!(
        names(&tree.asr_scratch(SOURCE)).iter().any(|n| n.ends_with(".plan")),
        "survives failure: the refused chunk's plan is there to read: {:?}",
        names(&tree.asr_scratch(SOURCE))
    );

    // Success half: the same stage, everything answered, leaves nothing of itself behind.
    let (_root, tree) = project("s1-asr-ok");
    asr::run(&tree, SOURCE, 60.0, "nemotron", "en", &loud(60.0), |_| {
        Ok(Answer { text: "one take".into(), words: vec![] })
    })
    .expect("every chunk answered");
    assert!(
        tree.words_json(SOURCE).is_file(),
        "the answer is the marker that lets the scratch go"
    );
    assert!(
        !tree.asr_scratch(SOURCE).exists(),
        "goes on success: {:?}",
        names(&tree.asr_scratch(SOURCE))
    );
}

/// §5 bullet 1, alignment: `align/` keeps each window's note when a window fails and goes when
/// `words.aligned.json` is written whole.
#[test]
fn sec_04_prepare_5_rules_s1_the_align_scratch_goes_on_success_and_survives_failure() {
    let aligners = ["qwen3-aligner".to_string()];

    // Failure half: every window refuses. `f1_3_run_path.rs` proves the same thing across a whole run;
    // here it is the stage on its own, so a regression names the stage.
    let (_root, tree) = project("s1-align-fails");
    let err = align::run(
        &tree,
        SOURCE,
        60.0,
        "en",
        &loud(60.0),
        &[],
        &[],
        "one two three",
        &aligners,
        |_, _, _| Err("no room for that much audio".to_string()),
    )
    .expect_err("an aligner that cannot fit the audio is a failure");
    assert!(err.contains("no room"), "{err}");
    assert!(
        !tree.words_aligned_json(SOURCE).exists(),
        "no answer, so no marker: the skip rule must not fire on a failed pass"
    );
    let kept = names(&tree.align_scratch(SOURCE));
    assert!(
        kept.iter().any(|n| n.ends_with(".plan")),
        "survives failure: the refused window's note is what a person compares against its audio: {kept:?}"
    );

    // Success half: windows answered, `words.aligned.json` written whole, `align/` gone.
    let (_root, tree) = project("s1-align-ok");
    align::run(
        &tree,
        SOURCE,
        60.0,
        "en",
        &loud(60.0),
        &[],
        &[],
        "one two three",
        &aligners,
        |_, _, _| Ok(json!({"words": [aligned_word("hello", 0.5, 1.0)]})),
    )
    .expect("the aligner placed the window");
    assert!(
        tree.words_aligned_json(SOURCE).is_file(),
        "the marker lands first — that is what the cleanup waits for"
    );
    assert!(
        !tree.align_scratch(SOURCE).exists(),
        "goes on success: {:?}",
        names(&tree.align_scratch(SOURCE))
    );
}

/// §5 bullet 1, diarization: `diar/` stays when the diarizer gives up and goes with `turns.json`.
/// Driven through `transcribe::run`, whose `diarize` closure is the very seam the real ▶ hands the
/// diarizer to, so this is the run path and not a helper called for its own sake.
#[test]
fn sec_04_prepare_5_rules_s1_the_diar_scratch_goes_on_success_and_survives_failure() {
    let spoken = || WordsDoc {
        text: "hello there".into(),
        words: vec![Word { word: "hello".into(), start_sample: 0, end_sample: 8000 }],
    };

    // Failure half: the diarizer's failure is fatal (bullet 5), and the scratch it wrote on the way out
    // is what says which window gave up.
    let (_root, tree) = project("s1-diar-fails");
    requests::write_words(&tree, SOURCE, &spoken()).expect("S3's answer is on disk");
    fs::create_dir_all(tree.diar_scratch(SOURCE)).expect("the diarizer's scratch");
    fs::write(tree.diar_scratch(SOURCE).join("d0000.wav.plan"), "-ss 0 -t 90\n").expect("its note");
    let err = transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron",
        |_| Align::None,
        || Ok(spoken()),
        || Err("the diarizer refused every window".to_string()),
    )
    .expect_err("no turns is fatal, not an empty session");
    assert!(err.contains("diarizer"), "{err}");
    assert!(
        !tree.turns_json(SOURCE).exists(),
        "no answer, so no marker"
    );
    assert!(
        tree.diar_scratch(SOURCE).is_dir(),
        "survives failure: `diar/` holds the window that failed"
    );

    // Success half: turns written whole, `diar/` gone.
    let (_root, tree) = project("s1-diar-ok");
    requests::write_words(&tree, SOURCE, &spoken()).expect("S3's answer is on disk");
    fs::create_dir_all(tree.diar_scratch(SOURCE)).expect("the diarizer's scratch");
    fs::write(tree.diar_scratch(SOURCE).join("d0000.wav.plan"), "-ss 0 -t 90\n").expect("its note");
    transcribe::run(
        &tree,
        SOURCE,
        30.0,
        "nemotron",
        |_| Align::None,
        || Ok(spoken()),
        || Ok(vec![Turn { start_sample: 0, end_sample: 16000, speaker_id: 1 }]),
    )
    .expect("the diarizer answered");
    assert!(
        tree.turns_json(SOURCE).is_file(),
        "the marker lands first — that is what the cleanup waits for"
    );
    assert!(
        !tree.diar_scratch(SOURCE).exists(),
        "goes on success: {:?}",
        names(&tree.diar_scratch(SOURCE))
    );
}

/// §5 bullet 1, the voice split: `stems/<lane>.work` is cleared on ENTRY (so parts of an earlier take can
/// never be joined into this one's file) and removed on SUCCESS only. The split's own legs need ffmpeg and
/// a separation server, so what is checked here is the two doors the flow calls — `clear_work` at the top
/// and the same `remove_dir_all` the success path runs — over a work dir of our own.
#[test]
fn sec_04_prepare_5_rules_s1_the_split_work_dir_survives_failure_and_goes_on_success() {
    let (_root, tree) = project("s1-split");
    let lane = "lecture.wav";
    let work = media::work_dir(&tree, lane);
    assert!(
        work.starts_with(tree.stems_dir()),
        "the scratch sits beside the split's products, not among the sources: {}",
        work.display()
    );

    // Cleared on entry: parts left by a killed run are gone before anything is decoded.
    fs::create_dir_all(&work).expect("the work dir");
    fs::write(work.join("p07.wav"), b"part of a dead run").expect("a stale part");
    media::clear_work(&work).expect("absent or present, entry-clearing never fails");
    assert!(
        !work.exists(),
        "cleared on entry: a stale part would be joined into this take's file"
    );
    // Absent is the normal case, not a warning: the door answers Ok either way.
    media::clear_work(&work).expect("nothing to clear is not an error");

    // Survives failure: a split that stops mid-way keeps its parts (they are what a person reads and what
    // the resume rule looks at), which is exactly what NOT calling the removal means.
    fs::create_dir_all(&work).expect("the work dir");
    fs::write(media::decoded_file(&work, lane), b"pcm").expect("the decode");
    assert!(
        work.is_dir() && media::decoded_file(&work, lane).is_file(),
        "survives failure: the decode and the parts are the evidence"
    );

    // Goes on success: the same call the success path makes (§F1.2, after `separate::finish`).
    let _ = std::fs::remove_dir_all(&work);
    assert!(
        !work.exists(),
        "goes on success: the join is on disk, so the work dir is the stage's own mess"
    );
}
