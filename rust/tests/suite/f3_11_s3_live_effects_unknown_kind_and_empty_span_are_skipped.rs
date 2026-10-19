//! F3.11 S3 over a socket: an unknown kind and an empty span are SKIPPED — the reply stays usable, the good
//! decoration lands, each skip is SAID, and neither costs the round. One scenario per binary
//! (`XDG_CONFIG_HOME`/cwd are process-wide) — see `captions_wire_harness.rs`.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::cut_effects_pass;
use naivepost::effects_ask;
use naivepost::tools::clips::Clips;
use serde_json::json;

const CTX: &str = "Punch in on the score, hold the ending.";

/// Three clips of 20 s each at their session seconds.
fn batch() -> Clips {
    Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)])
}

fn effects_json(entries: &[serde_json::Value]) -> String {
    envelope_of(&json!({ "fx": entries }).to_string())
}

fn fx(clip: u32, kind: &str, start: f64, end: f64) -> serde_json::Value {
    json!({"clip": clip, "kind": kind, "start": start, "end": end})
}

fn asked(fixture: &Fixture) -> usize {
    read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            fields.contains(&"effects") && !fields.contains(&"cache")
        })
        .count()
}

#[test]
fn f3_11_s3_live_effects_unknown_kind_and_empty_span_are_skipped_and_said() {
    let fixture = fixture("f311-s3-skips");
    // One reply, three lines: a kind this pass does not place, a span that is empty inside its clip, and one
    // decoration that is perfectly usable.
    let llm = FakeLlm::start(vec![effects_json(&[
        fx(1, "blur", 2.0, 5.0),
        fx(2, "zoom", 7.0, 7.0),
        fx(3, "stop", 1.0, 3.0),
    ])]);
    point_at(&llm.url());

    let batch = batch();
    let first = effects_ask::ask(&fixture.tree, CTX, &batch, &[], 1, &|| false);
    assert_eq!(first.calls.len(), 3, "the reader passes all three through: only `place` judges them");
    let reply = cut_effects_pass::place(&batch, &first.calls);

    // S3 (`unknown kind or end ≤ start → skipped`): the reply is NOT unusable, so no retry is owed.
    assert!(reply.usable(), "a bad line skips itself instead of throwing the answer away");
    assert!(reply.fault.is_none(), "no fault: nothing here is about a clip outside the list");
    assert_eq!(reply.effects.len(), 1, "only the good stop lands");
    assert_eq!(reply.effects[0].kind, "speed", "a stop is recorded as a rate-0 speed (§06#1's table has no stop kind)");
    assert_eq!(reply.effects[0].rate, 0.0, "and its rate says so");

    // Each skip is SAID. Dropping them silently would leave the user counting bars on the lane, finding fewer
    // than the reply proposed, with no reason printed anywhere.
    let said: Vec<String> = reply
        .answers
        .iter()
        .filter_map(|answer| match answer {
            cut_effects_pass::Answer::Refused(reason) => Some(reason.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(said.len(), 2, "both bad lines answer with a reason, got {said:?}");
    assert!(
        said.iter().any(|reason| reason.contains("\"blur\" is not an effect this pass places")),
        "the unknown kind names itself: {said:?}"
    );
    assert!(
        said.iter().any(|reason| reason.contains("the span is empty after clamping it into the clip")),
        "the empty span says why it is nothing: {said:?}"
    );

    // And the round was NOT retried: skips are unwelcome, not unusable. One request, one timed row.
    assert_eq!(asked(&fixture), 1, "one ask, because a skip costs no round");
}
