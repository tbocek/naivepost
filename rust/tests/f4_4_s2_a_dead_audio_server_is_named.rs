//! F4.4 S2 over a socket, case A: nothing answers `/health`, so the refusal names the server and
//! no line is asked of it. One scenario per binary (`XDG_CONFIG_HOME` and cwd are process-wide) —
//! see the harness.

#![allow(dead_code)]

#[path = "speak_tts_wire_harness.rs"]
mod harness;

use harness::*;

#[test]
fn f4_4_s2_a_server_that_does_not_answer_health_is_named() {
    let fixture = fixture("s2-dead");
    write_reference(&fixture, b"RIFF fake reference");
    // Hand out a port and drop the listener: dialling it answers nothing at all.
    let dead = {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind to hand out a port");
        listener.local_addr().expect("a local address").port()
    };
    point_at(&format!("http://127.0.0.1:{dead}"));

    let outcome = speak(&fixture, "calm", "en");
    let refused = outcome.refused().expect("nothing answers /health, so nothing can be spoken");
    assert!(
        refused.contains("/health") || refused.contains("audio.cpp"),
        "the refusal names the server and the probe that failed: {refused}"
    );
    assert!(outcome.wav().is_none(), "a dead server writes no take");
}
