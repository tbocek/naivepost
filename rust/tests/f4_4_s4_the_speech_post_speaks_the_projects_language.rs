//! F4.4 S4 over a socket: the body that reaches the wire carries the project's language (not the
//! prototype's hard-coded "en"), the five top-level keys, and the emotion routed the way S6 says —
//! a known weighted tag as `emotion_vector` with alpha 1, anything else as words for the judge at
//! alpha 0.85. One scenario per binary; see the harness.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;

#[test]
fn f4_4_s4_the_speech_post_speaks_the_projects_language_not_the_prototypes() {
    let fixture = fixture("s4");
    write_reference(&fixture, b"RIFF fake reference");
    let audio = FakeAudio::start(Script::healthy());
    point_at(&audio.url());
    point_model_at("my-voice-model");

    // A known weighted tag: it resolves to eight floats, so it goes out exact.
    let outcome = speak(&fixture, "angry=1", "pl");
    assert!(outcome.wav().is_some(), "the line was spoken: {:?}", outcome.refused());

    let seen = audio.drain();
    let speech = seen
        .iter()
        .find(|seen| seen.path == "/v1/audio/speech")
        .expect("the speech call reached the server");
    assert_eq!(speech.method, "POST", "S4 is a POST");
    let body: serde_json::Value =
        serde_json::from_str(&speech.body_text()).expect("a JSON body went out");

    // The five keys §02-services §1 names, and nothing else standing in for them.
    let keys: Vec<String> = body
        .as_object()
        .expect("an object body")
        .keys()
        .cloned()
        .collect();
    for want in ["model", "input", "voice_ref", "language", "options"] {
        assert!(keys.iter().any(|k| k == want), "the body carries `{want}`: {keys:?}");
    }
    assert_eq!(body["model"], "my-voice-model", "the model the Settings name");
    assert_eq!(
        body["language"], "pl",
        "P.policy.ttsLanguage is the PROJECT's language, not the prototype's \"en\""
    );
    assert_eq!(
        body["voice_ref"], "/tmp/uploaded/voice_ref.wav",
        "voice_ref is the path the upload returned"
    );
    assert_eq!(
        body["input"], "The chain moved to proof of stake. (one)",
        "input is the line's own words"
    );

    // A weighted, known tag lands on the vector path with alpha 1 — the exact reading.
    let options = body["options"].as_object().expect("options is an object");
    assert_eq!(
        options.get("emotion_alpha").and_then(|v| v.as_str()),
        Some("1"),
        "a vector rides with alpha 1, not 0.85: {options:?}"
    );
    let vector = options
        .get("emotion_vector")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let parts: Vec<&str> = vector.split(',').collect();
    assert_eq!(parts.len(), 8, "eight floats, comma joined: {vector}");
    assert_eq!(parts[1], "1", "angry=1 lights axis 1 fully: {vector}");
    assert!(
        !options.contains_key("use_emotion_text"),
        "the vector path sends no judge request: {options:?}"
    );
    assert!(
        options.contains_key("seed"),
        "the seed travels so the take can be re-spoken: {options:?}"
    );
}
