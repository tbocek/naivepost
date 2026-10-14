//! F3.10 S3 over a socket: a rate of about 1 is silence, and an empty SPEEDS list is a whole answer.
//! One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide) — see `captions_wire_harness.rs`.
//!
//! Both endings look like "nothing happened" from the outside and are opposite inside: a rate that means the
//! footage's own speed is not an answer at all (§F3.10's flowchart sends it to `ignored`, not to `refused`),
//! while `{}`-with-no-clips is the model saying this cut has nothing dull in it — which §F3.10's prompt calls a
//! complete reply and therefore must not be reported as a failure.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::three_clips;
use naivepost::cut_speed_pass::{self, Answer, Call};
use naivepost::speeds_ask;
use serde_json::json;

/// The SPEEDS document §`prompts/system.md`'s `speed` row specifies, wrapped in the chat envelope a real
/// server answers with, so the ask reads it through the same parser a live reply goes through.
fn speeds_json(speeds: &[(u32, f64)]) -> String {
    envelope_of(
        &json!({
            "speeds": speeds.iter().map(|(clip, rate)| json!({ "clip": clip, "rate": rate })).collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// No clip carries a caption here: a rate near 1 must read as silence on a plain clip, where nothing else
/// could explain why no effect appeared.
fn captioned() -> Vec<(u32, usize)> {
    Vec::new()
}

/// The batch as the Cut page hands it: three 20 s clips at their session seconds, none captioned.
fn batch() -> naivepost::tools::clips::Clips {
    naivepost::tools::clips::Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)])
}

#[test]
fn f3_10_s3_live_speeds_rate_near_one_is_silence_and_an_empty_answer_is_a_whole_answer() {
    let fixture = fixture("s3-neutral");
    // 1.0 exactly, and 1.005 — both inside the pass's own neutral window (effects.neutralRateTolerance, a
    // hundredth either side of 1). Either one says "leave this clip alone".
    let llm = FakeLlm::start(vec![speeds_json(&[(1, 1.0), (2, 1.005)])]);
    point_at(&llm.url());

    let asked = speeds_ask::ask(&fixture.tree, "Speed what drags.", &three_clips(), &captioned(), 1, &|| false);
    assert!(!asked.failed, "the ask was answered: {:?}", asked.reason);
    // The ask reports what the model ASKED for, rates included. Deciding that a rate says nothing is the
    // pass's judgement, made once, rather than something the wire quietly filters out on the way in.
    assert_eq!(asked.calls, vec![Call { clip: 1, rate: 1.0 }, Call { clip: 2, rate: 1.005 }]);

    let reply = cut_speed_pass::place(&batch(), &asked.calls);
    assert!(reply.usable(), "a rate of about 1 is not an unusable reply: {:?}", reply.fault);
    assert!(reply.effects.is_empty(), "and nothing is placed over it, got {:?}", reply.effects);
    assert_eq!(
        reply.answers,
        vec![Answer::Ignored, Answer::Ignored],
        "every one of them is IGNORED -- silence, not a refusal: {:?}",
        reply.answers
    );
    // The tolerance itself, named by the rule that reads it. Half as fast is slow motion and MEANS something,
    // so the window sits around 1 rather than being any rate under it.
    assert!(
        !cut_speed_pass::is_neutral(0.5) && cut_speed_pass::is_neutral(1.005),
        "the neutral window is around 1, not everything below it"
    );

    // An empty list is the other ending: the model looked and found nothing dull. That is a whole answer, so
    // the ask must NOT report a failure — only a wire fault does that — and the give-up line the pass prints
    // when it truly has nothing is the spec's own bytes, em dash U+2014 and all.
    let empty = speeds_ask::parse_speeds(&envelope_of(r#"{"speeds":[]}"#));
    assert!(empty.is_empty(), "an empty SPEEDS document parses to no calls");
    assert_eq!(
        cut_speed_pass::no_answer(),
        "!!! speed: no usable answer \u{2014} every clip plays at 1",
        "the give-up line, spelled by the module, em dash and all"
    );
}
