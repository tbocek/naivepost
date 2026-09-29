//! F4.4 S1 over a socket: with no reference on disk the line is refused BEFORE any request goes out.
//! One scenario per binary (`XDG_CONFIG_HOME` and cwd are process-wide) — see the harness.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;

#[test]
fn f4_4_s1_no_reference_refuses_before_the_server_is_asked() {
    let fixture = fixture("s1");
    // A server that would answer everything, so a refusal can only come from S1, not from a dead box.
    let audio = FakeAudio::start(Script::healthy());
    point_at(&audio.url());

    let outcome = speak(&fixture, "calm", "en");
    let refused = outcome.refused().expect("no voice_ref.wav means nothing can be cloned");
    assert!(
        refused.contains("voice_ref.wav"),
        "the refusal names the missing reference: {refused}"
    );
    assert!(
        refused.contains("F4.6"),
        "and says where to build it: {refused}"
    );
    assert!(outcome.wav().is_none(), "nothing was written");

    // The wire proves the order: S1 refused before ANY dial, so the fake saw zero requests at all —
    // not even /health.
    let seen = audio.drain();
    assert_eq!(
        seen.len(),
        0,
        "S1's refusal must not touch the server; it saw {:?}",
        seen.iter().map(|s| format!("{} {}", s.method, s.path)).collect::<Vec<_>>()
    );
}
