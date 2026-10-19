//! F4.1 S6 over a socket: the speaking half of ▶ Write and speak really dials audio.cpp and files the
//! takes it counts — no take on disk, no "spoken". One scenario per binary (the harness pins
//! process-wide env), so each step of the step is its own file; this one is "the wire was used".
//!
//! It drives [`naivepost::ui::narrate_run_speak`], the module `run_narrate_with` calls through, whose
//! per-line leg is the same door the row's ▶ uses (`narrate_page::press_line_speak` → `speak_leg::speak`).

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::narrate_run;
use naivepost::narration::Entry;
use naivepost::ui::narrate_run_speak;

/// Three clips' worth of narration: two with words, one deliberately blank (§S6 skips blanks).
fn record() -> Vec<Entry> {
    vec![
        Entry {
            s: 0.0,
            e: 20.0,
            at: 1.0,
            text: "Welcome back: lecture two is all about Ethereum.".into(),
            emotion: "calm".into(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 20.0,
            e: 40.0,
            at: 2.0,
            text: "".into(),
            emotion: String::new(),
            pos: String::new(),
            roll: 0,
        },
        Entry {
            s: 40.0,
            e: 60.0,
            at: 0.5,
            text: "First a quick look back at Bitcoin.".into(),
            emotion: "happy=0.6, calm=0.4".into(),
            pos: String::new(),
            roll: 0,
        },
    ]
}

/// The cache check the door passes: the real file under `narrate/tts/`, named by the key's digest.
fn cached_on_disk(fixture: &Fixture) -> impl Fn(&Entry) -> bool {
    let tree = fixture.tree.clone();
    move |entry| {
        let key = naivepost::narration::key_for(entry, Some("narrator1"));
        tree.tts_wav(&naivepost::narration::tts_file(&key)).is_file()
    }
}

#[test]
fn f4_1_s6_the_speaking_half_dials_audio_cpp() {
    let fixture = fixture("f41-s6-dial");
    write_reference(&fixture, b"RIFF fake reference, forty-odd bytes long, not empty");
    let audio = FakeAudio::start(Script::healthy());
    point_at(&audio.url());

    let entries = record();
    let run = narrate_run_speak::speak_lines(
        &entries,
        cached_on_disk(&fixture),
        |entry| narrate_run_speak::dial_line(&fixture.tree, "narrator1", "en", entry),
    );
    let (lines, spoken, cached) = (run.lines, run.spoken, run.cached);
    assert!(!run.stopped, "nothing stopped this pass");

    assert_eq!(spoken, 2, "both lines with words were spoken: {lines:?}");
    assert_eq!(cached, 0, "nothing was in the cache before this run");
    assert_eq!(lines.len(), entries.len(), "one verdict per line, blanks included");
    // The blank is neither spoken nor cached, so the summary cannot drift from what actually ran.
    assert_eq!(
        narrate_run::spoken_log(spoken, cached),
        "    narrate: 2 line(s) spoken, 0 already in the cache"
    );
    // A take exists on disk for EVERY counted line — that is the proof the server was asked and answered.
    let tts_home = fixture.tree.tts_wav("");
    let tts_home = tts_home.parent().expect("tts folder has a parent");
    let mut takes = 0usize;
    for line in &lines {
        if let narrate_run_speak::Line::Spoken(wav) = line {
            assert!(wav.is_file(), "the take the run counted is a real file: {}", wav.display());
            assert!(
                wav.starts_with(tts_home),
                "takes live under narrate/tts/, where the cache looks: {}",
                wav.display()
            );
            takes += 1;
        }
    }
    assert_eq!(takes, 2);
    // And the server saw the asks: one speech POST per spoken line, each after its own upload.
    let seen = audio.drain();
    let speeches = seen.iter().filter(|seen| seen.path == "/v1/audio/speech").count();
    assert_eq!(speeches, 2, "one /v1/audio/speech per spoken line: {} requests", seen.len());
    let uploads = seen.iter().filter(|seen| seen.path == "/v1/ui/upload").count();
    assert_eq!(uploads, 2, "F4.4 re-uploads the reference for every line");
    // The second pass over the same lines spends nothing: the cache the door checks is now warm.
    let second = narrate_run_speak::speak_lines(
        &entries,
        cached_on_disk(&fixture),
        |entry| narrate_run_speak::dial_line(&fixture.tree, "narrator1", "en", entry),
    );
    assert_eq!((second.spoken, second.cached), (0, 2), "a warm cache speaks nothing: {:?}", second.lines);
    assert_eq!(
        narrate_run::spoken_log(second.spoken, second.cached),
        "    narrate: 0 line(s) spoken, 2 already in the cache"
    );
    assert_eq!(
        audio.drain().iter().filter(|seen| seen.path == "/v1/audio/speech").count(),
        0,
        "the warm pass dialled the speech endpoint zero times"
    );
}
