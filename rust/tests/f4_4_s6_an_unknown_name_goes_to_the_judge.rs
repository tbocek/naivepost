//! F4.4 S6 over a socket: an emotion name this client does not know is not an error — it goes to the
//! server's judge as words, at alpha 0.85, and the line is still spoken. One scenario per binary;
//! see the harness.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;

#[test]
fn f4_4_s6_an_unknown_name_goes_to_the_judge_rather_than_an_error() {
    let fixture = fixture("s6");
    write_reference(&fixture, b"RIFF fake reference");
    let audio = FakeAudio::start(Script::healthy());
    point_at(&audio.url());

    // `wistful` names no base, no kin and no blend; `smug` neither. Neither is refused.
    let outcome = speak(&fixture, "wistful=1, smug=0.3", "en");
    assert!(
        outcome.refused().is_none(),
        "an unknown emotion name never errors out: {:?}",
        outcome.refused()
    );
    let landed = outcome.wav().expect("the line was still spoken");
    assert!(landed.is_file(), "and its take is on disk at {}", landed.display());

    let seen = audio.drain();
    let speech = seen
        .iter()
        .find(|seen| seen.path == "/v1/audio/speech")
        .expect("the speech call reached the server");
    let body: serde_json::Value =
        serde_json::from_str(&speech.body_text()).expect("a JSON body went out");
    let options = body["options"].as_object().expect("options is an object");

    // The judge route, not the vector route.
    assert_eq!(
        options.get("use_emotion_text").and_then(|v| v.as_str()),
        Some("true"),
        "unknown names go up as text for the judge: {options:?}"
    );
    let said = options
        .get("emotion_text")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        said.contains("wistful") && said.contains("smug"),
        "both names reach the judge, weights stripped: {said}"
    );
    assert!(
        !said.contains('='),
        "weights are stripped off before the judge sees the words: {said}"
    );
    assert_eq!(
        options.get("emotion_alpha").and_then(|v| v.as_str()),
        Some("0.85"),
        "the judge blends at EMOTION_ALPHA, not at 1: {options:?}"
    );
    assert!(
        !options.contains_key("emotion_vector"),
        "no floats were invented for a name we do not have: {options:?}"
    );
}
