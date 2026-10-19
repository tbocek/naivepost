//! F4.4 S5 over a socket: only a 200 of at least MIN_WAV_BYTES is a take. A short body, however
//! confident its status, files nothing; a non-200 files nothing; a real reply lands at
//! `narrate/tts/‹hash›.wav` holding the bytes that crossed the wire. One scenario per binary.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;
use std::path::PathBuf;

/// The take folder, off the path a real take lands in (the Tree names files, not the folder).
fn take_folder(fixture: &Fixture) -> PathBuf {
    expected_take(fixture, "calm")
        .parent()
        .expect("a take path has a folder")
        .to_path_buf()
}

#[test]
fn f4_4_s5_a_short_or_non_200_reply_lands_no_file() {
    let fixture = fixture("s5");
    write_reference(&fixture, b"RIFF fake reference");
    // Three asks in order: a 900-byte 200, a 500, then a real wav. The script repeats its last
    // entry after it runs out, so these three are spent in order by three speaks.
    let audio = FakeAudio::start(Script::healthy().speech_with(vec![
        Reply { status: 200, body: wav_bytes(900) },
        Reply::json(500, r#"{"error":"out of memory"}"#),
        Reply { status: 200, body: wav_bytes(2400) },
    ]));
    point_at(&audio.url());

    // (a) 200 with 900 bytes: refused, and nothing filed.
    let short = speak_tagged(&fixture, "calm", "en", "short");
    let why = short.refused().expect("900 bytes is not a wav");
    assert!(
        why.contains("900") || why.contains("not a wav"),
        "the refusal says how short it was: {why}"
    );
    assert!(short.wav().is_none(), "a short reply writes no take");
    assert!(
        !take_folder(&fixture).exists()
            || std::fs::read_dir(take_folder(&fixture))
                .map(|entries| entries.count())
                .unwrap_or(0)
                == 0,
        "narrate/tts stays empty after a refused reply"
    );

    // (b) 500: refused with the server's own words beside the status.
    let failed = speak_tagged(&fixture, "calm", "en", "failed");
    let why = failed.refused().expect("a 500 is not a take");
    assert!(
        why.contains("500") && why.contains("out of memory"),
        "the refusal carries the status AND the server's message: {why}"
    );
    assert!(failed.wav().is_none(), "a 500 writes no file either");

    // (c) a real 2400-byte reply: written where the key says, holding exactly what came back.
    let good = speak_tagged(&fixture, "calm", "en", "good");
    assert!(good.refused().is_none(), "the good reply is a take: {:?}", good.refused());
    let landed = good.wav().expect("the take path");
    assert_eq!(
        landed,
        expected_take(&fixture, "calm").parent().unwrap().join(landed.file_name().unwrap()),
        "the take sits inside narrate/tts/"
    );
    assert_eq!(
        std::fs::read(landed).expect("readable"),
        wav_bytes(2400),
        "and holds the bytes the server sent, byte for byte"
    );
    let name = landed.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.ends_with(".wav") && name.len() == 20, "‹16 hex›.wav: {name}");
}
