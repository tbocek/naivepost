//! F4.4 §09 §10 over a socket: every call one spoken line makes leaves its own timed row in
//! `requests.tsv` — health, models, upload and speech, all service `audio`, each with the time it
//! really spent on the wire. One scenario per binary; see the harness.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;

#[test]
fn f4_4_s7_every_call_its_own_timed_row() {
    let fixture = fixture("s7");
    write_reference(&fixture, b"RIFF fake reference");
    let audio = FakeAudio::start(Script::healthy());
    point_at(&audio.url());

    let outcome = speak(&fixture, "calm", "en");
    assert!(outcome.wav().is_some(), "the line was spoken: {:?}", outcome.refused());
    // Let the four legs' rows all be written before reading the file.
    audio.drain();

    let rows = audio_rows(&fixture);
    // Four calls went out for this one line; each is a row of its own (§09 §10: "a retry is a line
    // of its own", so a success is too).
    let kinds: Vec<&str> = rows.iter().map(|row| row.split('\t').nth(6).unwrap_or("")).collect();
    for want in ["health", "models", "upload", "speech"] {
        assert!(
            kinds.contains(&want),
            "a `{want}` row is missing from {} rows (kinds {kinds:?})",
            rows.len()
        );
    }
    assert_eq!(
        kinds.len(),
        4,
        "exactly the four legs, no extras: kinds {kinds:?}"
    );

    // Each row says which step and job it belongs to, and carries real timing.
    for row in &rows {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields[2], "speak", "step column: {row}");
        assert_eq!(fields[3], "narrate", "job column: {row}");
        assert_eq!(fields[4], "audio", "service column: {row}");
        assert_eq!(fields[16], "ok", "outcome column: {row}");
        assert!(
            !fields[14].is_empty() && fields[14].parse::<f64>().is_ok(),
            "on_wire_s filled for a request that really was on the wire: {row}"
        );
    }

    // The speech row knows which voice it dialled and how big the reply was.
    let speech = rows
        .iter()
        .find(|row| row.split('\t').nth(6) == Some("speech"))
        .expect("the speech row");
    let fields: Vec<&str> = speech.split('\t').collect();
    assert_eq!(fields[5], "index-tts2", "the model column names the voice: {speech}");
    let bytes = fields[9].parse::<i64>().unwrap_or_default();
    assert!(bytes >= 2400, "received_bytes holds the wav's size: {speech}");
}
