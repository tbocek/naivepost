//! F3.10 S2 over a socket: a fast rate the model asked for over a captioned clip is REFUSED, per call,
//! and the refusal is said. One scenario per binary -- see `captions_wire_harness.rs`.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::three_clips;
use naivepost::speeds_ask;
use serde_json::json;
use naivepost::cut_speed_pass::Call;

/// Which clips carry a caption: clip 3, with its one caption — `N caption(s) -- runs at 1`, spelled as
/// §F3.10 spells it.
fn captioned() -> Vec<(u32, usize)> {
    vec![(3, 1)]
}

/// The fake answers every ask with an OpenAI envelope, so that is the shape this helper builds — not the bare
/// document (§1: `choices[0].message.content` is the answer, and the ask reads it that way off the wire).
fn speeds_json(speeds: &[(u32, f64)]) -> String {
    envelope_of(
        &json!({
            "speeds": speeds.iter().map(|(clip, rate)| json!({ "clip": clip, "rate": rate })).collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// The batch the Cut page hands the pass: three 20 s clips at their session seconds, one caption on clip 3.
fn captioned_batch() -> naivepost::tools::clips::Clips {
    let mut batch = naivepost::tools::clips::Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)]);
    batch.add_caption(3, 1.0, 6.0, "a line already on screen");
    batch
}

/// A real server answers a tool conversation with a whole envelope whose `message` carries the call rather
/// than prose, so that is the shape this reply has.
fn tool_answer(clip: u32, rate: f64) -> String {
    json!({
        "choices": [{"message": {"content": null, "tool_calls": [{
            "id": "call_1",
            "type": "function",
            "function": {"name": "set_clip_speed", "arguments": json!({"clip": clip, "rate": rate}).to_string()}
        }]}}]
    })
    .to_string()
}

/// S2: the refusal reaches the caller. A model that offers `set_clip_speed` over a captioned clip is answered
/// by the tool's own sentence, not by a silent drop, and the rate that could not be refused is untouched.
#[test]
fn f3_10_s2_live_speeds_reply_refused_on_a_captioned_clip_is_said() {
    let fixture = fixture("s2-refused");
    // A tool call is the shape a tool conversation answers with, so this is the reply a live model sends. It
    // goes in the envelope a server frames it with — a bare `{"tool_calls":…}` is not a chat reply, and §1
    // reads only `choices[0].message.tool_calls`.
    let llm = FakeLlm::start(vec![tool_answer(3, 2.0)]);
    point_at(&llm.url());

    let asked = speeds_ask::ask(&fixture.tree, "Run the dull stretches fast.", &three_clips(), &captioned(), 1, &|| false);
    let seen_reply = llm.next_seen().reply;    assert!(!asked.failed, "the ask was answered: {:?}", asked.reason);
    // The ask reports what the model ASKED for: the refusal is the tool's judgement, made where the captions
    // are known, so it is `cut_speed_pass::place` that refuses — not a filter that quietly drops the rate.
    assert_eq!(asked.calls, vec![Call { clip: 3, rate: 2.0 }], "the ask reports the model's own call -- the server answered {seen_reply}");

    // The batch the Cut page hands the pass: three 20 s clips, one caption on clip 3.
    let batch = captioned_batch();
    let reply = naivepost::cut_speed_pass::place(&batch, &asked.calls);
    assert!(reply.usable(), "a per-call refusal does not cost the round: {:?}", reply.fault);
    assert!(reply.effects.is_empty(), "nothing lands over a captioned clip, got {:?}", reply.effects);
    let said: Vec<String> = reply
        .answers
        .iter()
        .filter_map(|answer| match answer {
            naivepost::cut_speed_pass::Answer::Refused(reason) => Some(reason.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(said.len(), 1, "one refusal, said aloud: {said:?}");
    assert!(said[0].contains("cannot be sped up"), "the tool names the rule it refused: {said:?}");
    assert!(said[0].contains("clip 3"), "and names the clip it refused: {said:?}");
}
