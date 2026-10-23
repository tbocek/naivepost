//! F4.1 S6, the refusal half: a line the engine will not speak is said with the reason it gave and
//! writes no take, and the lines after it still run (§Failure-is-specific: name the model and the
//! reason where it failed, carry on elsewhere). One scenario per binary — see the harness's note.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::narrate_run::{self, Speak};
use naivepost::narration::Entry;
use naivepost::ui::narrate_run_speak;

fn line(s: f64, e: f64, text: &str) -> Entry {
    Entry {
        s,
        e,
        at: 1.0,
        text: text.into(),
        emotion: "calm".into(),
        pos: String::new(),
        roll: 0,
    }
}

/// A server that answers every speech POST with an error: nothing may be filed as a take, and each
// line's own refusal must reach the caller so the log can name it.
#[test]
fn f4_1_s6_a_refusal_is_said_and_writes_no_take() {
    let fixture = fixture("f41-s6-refuse");
    write_reference(
        &fixture,
        b"RIFF fake reference, forty-odd bytes long, not empty",
    );
    let audio = FakeAudio::start(Script::healthy().speech_with(vec![Reply::json(
        500,
        r#"{"error":"no memory for a take"}"#,
    )]));
    point_at(&audio.url());

    let entries = vec![
        line(
            0.0,
            20.0,
            "Welcome back: lecture two is all about Ethereum.",
        ),
        line(20.0, 40.0, ""),
        line(40.0, 60.0, "First a quick look back at Bitcoin."),
    ];
    let run = narrate_run_speak::speak_lines(
        &entries,
        |entry| {
            let key = naivepost::narration::key_for(entry, Some("narrator1"));
            fixture
                .tree
                .tts_wav(&naivepost::narration::tts_file(&key))
                .is_file()
        },
        |entry| narrate_run_speak::dial_line(&fixture.tree, "narrator1", "en", entry),
    );
    let (lines, spoken, cached) = (run.lines, run.spoken, run.cached);
    assert!(!run.stopped, "a refusal is not a stop");
    assert_eq!(spoken, 0, "nothing was spoken: {lines:?}");
    assert_eq!(cached, 0);
    let refusals: Vec<&str> = lines
        .iter()
        .filter_map(narrate_run_speak::refused)
        .collect();
    assert_eq!(
        refusals.len(),
        2,
        "each line with words got its own refusal: {lines:?}"
    );
    for why in &refusals {
        assert!(
            !why.trim().is_empty(),
            "a refusal always carries the sentence the step answered with"
        );
    }
    let tts_home = fixture.tree.tts_wav("");
    let tts = tts_home.parent().expect("tts folder has a parent");
    let written = std::fs::read_dir(tts)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(
        written,
        0,
        "a refused reply never becomes a take: {written} files in {}",
        tts.display()
    );
    // Both lines still asked: the first refusal does not stop the pass walking the rest.
    assert_eq!(
        audio.hits("/v1/audio/speech"),
        2,
        "the pass carried on past the first refusal"
    );
    // And the summary the run logs says zero spoken, which is the truth of it.
    assert_eq!(
        narrate_run::spoken_log(spoken, cached),
        "    narrate: 0 line(s) spoken, 0 already in the cache"
    );
}

/// The classification itself still decides per line, and this module agrees with it: blank → neither,
/// cached → cached, otherwise dialled. Pinned here because this module turns those three verdicts into
/// the run's counts, and a drift between the two would show up as a wrong "N spoken".
#[test]
fn f4_1_s6_verdicts_match_the_classification() {
    let entries = vec![
        line(0.0, 20.0, "one"),
        line(20.0, 40.0, "   "),
        line(40.0, 60.0, "three"),
    ];
    let decisions = narrate_run::speak_pass("narrator1", &entries, |entry| entry.s == 40.0);
    assert_eq!(
        decisions,
        vec![Speak::Synthesize, Speak::SkippedBlank, Speak::SkippedCached],
        "blank before cache, cache before the wire"
    );
    let dialed = std::cell::RefCell::new(Vec::new());
    let run = narrate_run_speak::speak_lines(
        &entries,
        |entry| entry.s == 40.0,
        |entry| {
            dialed.borrow_mut().push(entry.s);
            Ok(std::path::PathBuf::from("/tmp/take.wav"))
        },
    );
    let (lines, spoken, cached) = (run.lines, run.spoken, run.cached);
    assert_eq!(
        dialed.borrow().as_slice(),
        &[0.0],
        "only the uncached line reached the leg"
    );
    assert_eq!((spoken, cached), (1, 1), "{lines:?}");
}
