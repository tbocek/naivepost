//! F4.4 S3 over a socket: the reference goes up again for EVERY line, because a server path dies
//! with a restart — so nothing may be remembered between speaks. One scenario per binary; see the
//! harness.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;

#[test]
fn f4_4_s3_the_reference_uploads_itself_for_every_line() {
    let fixture = fixture("s3");
    write_reference(&fixture, b"RIFF fake reference, forty-odd bytes long, not empty");
    let audio = FakeAudio::start(Script::healthy());
    point_at(&audio.url());

    // Two lines, two speaks. Each must put the reference up on its own.
    let first = speak_tagged(&fixture, "calm", "en", "one");
    assert!(first.wav().is_some(), "the first line was spoken: {:?}", first.refused());
    let second = speak_tagged(&fixture, "calm", "en", "two");
    assert!(second.wav().is_some(), "the second line was spoken: {:?}", second.refused());

    let seen = audio.drain();
    let uploads: Vec<&Seen> = seen
        .iter()
        .filter(|seen| seen.path == "/v1/ui/upload")
        .collect();
    assert_eq!(
        uploads.len(),
        2,
        "S3 uploads for every line, never once and remembers: {} requests total",
        seen.len()
    );
    for upload in &uploads {
        assert_eq!(upload.method, "POST", "the upload is a POST");
        assert_eq!(
            upload.header("x-audiocpp-filename"),
            Some("voice_ref.wav"),
            "the raw body names the file it carries: {:?}",
            upload.headers
        );
        assert!(
            !upload.body.is_empty(),
            "the body is the reference's bytes, not a path"
        );
    }
    // And each line asked for speech after its own upload — the path used was the one that upload
    // answered, not one carried over from before.
    let speeches: Vec<&Seen> = seen
        .iter()
        .filter(|seen| seen.path == "/v1/audio/speech")
        .collect();
    assert_eq!(speeches.len(), 2, "each line posted its own speech call");
    let first_body: serde_json::Value = serde_json::from_str(&speeches[0].body_text())
        .expect("the speech body is JSON");
    let second_body: serde_json::Value = serde_json::from_str(&speeches[1].body_text())
        .expect("the second speech body is JSON");
    assert_eq!(
        first_body["voice_ref"],
        serde_json::json!("/tmp/uploaded/voice_ref.wav"),
        "the first line named the path its upload returned"
    );
    assert_ne!(
        first_body["input"], second_body["input"],
        "two different lines went out, each with its own fresh upload behind it"
    );
}
