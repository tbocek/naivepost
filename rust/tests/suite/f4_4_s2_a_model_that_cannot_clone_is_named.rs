//! F4.4 S2 over a socket, case B: the server answers and is healthy but serves no model that can
//! clone a voice — refused in the verdict's own words, with no upload and no speech call. One
//! scenario per binary; see the harness.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;

#[test]
fn f4_4_s2_a_model_that_cannot_clone_is_named_and_no_line_is_asked_of_it() {
    let fixture = fixture("s2-noclone");
    write_reference(&fixture, b"RIFF fake reference");
    // Healthy, answering, serving only models with no cloning ability: exactly the case S2
    // distinguishes from "is there a model on that port".
    let audio = FakeAudio::start(Script::healthy().models_with(vec![Reply::json(
        200,
        r#"{"data":[{"id":"whisper-large","family":"whisper","task":"asr"}]}"#,
    )]));
    point_at(&audio.url());

    let outcome = speak(&fixture, "calm", "en");
    let refused = outcome
        .refused()
        .expect("a server serving only ASR models cannot narrate");
    assert!(
        refused.contains("clone") || refused.contains("whisper-large"),
        "the refusal says why in the verdict's own words: {refused}"
    );

    // The wire: health and models were asked (that is how it found out), the upload and the speech
    // call never were.
    let paths: Vec<String> = audio
        .drain()
        .into_iter()
        .map(|seen| format!("{} {}", seen.method, seen.path))
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("/health")),
        "S2 asked /health: {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("/v1/models")),
        "S2 asked /v1/models: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("/v1/ui/upload")),
        "an unclonable server was never uploaded to: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("/v1/audio/speech")),
        "and never asked to speak: {paths:?}"
    );
}
