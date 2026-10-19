//! §12-decisions#5's first clause, over a socket: "Every job now names its read tools as well as
//! write tools — before, only the policy job had one, so elsewhere the brief silently bounded what
//! the model could be right about."
//!
//! The catalogue (`tools::offered`) has listed reads for every job since §3 was written, but the
//! previous round's live tests proved the offer only for the two loops that ran (narrate, upload
//! text). This proves it for the §3.7 clip passes, on the wire: the ask dials the LLM address the
//! settings hold — the same seam a person types into the Settings box — and the test reads the
//! `tools` array off the request body the fake actually received. Nothing is injected into the
//! program's path; the reply travelled through a socket and was parsed by the client.
//!
//! One scenario per test binary, as every other live file here does: `XDG_CONFIG_HOME`,
//! `XDG_DATA_HOME` and the working directory are process-wide, so two fixtures in one binary would
//! each read whichever env the other set last.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::speeds_ask;
use serde_json::Value;

/// Three kept clips of 20 s each — the batch the Cut page hands the speed pass.
fn three_clips() -> Vec<(u32, f64)> {
    vec![(1u32, 20.0), (2u32, 20.0), (3u32, 20.0)]
}

/// The `tools` array of a request body, as the names the server was handed.
fn offered_names(body: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(body).expect("the chat body is JSON");
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("the request carried no tools array: {body}"));
    tools
        .iter()
        .map(|tool| {
            tool.pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("<unnamed>")
                .to_string()
        })
        .collect()
}

/// s1: the speed pass, live. It is a §3.7 job, so §3.7's read belongs on its request beside its own
/// write tool: `get_frames` is what lets the model aim rather than guess, and `set_clip_speed` is
/// the decision it is being asked for.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s1_the_offer_names_the_read_tools() {
    let fixture = fixture("s1-speeds");
    let llm = FakeLlm::start(vec![envelope_of(r#"{"speeds": []}"#)]);
    point_at(&llm.url());
    let asked = speeds_ask::ask(
        &fixture.tree,
        "Run the dull stretches fast.",
        &three_clips(),
        &[],
        1,
        &|| false,
    );
    assert!(!asked.failed, "the ask was answered: {:?}", asked.reason);
    let names = offered_names(&llm.next_seen().body);
    assert!(
        names.iter().any(|name| name == "get_frames"),
        "the speed pass offered {names:?}, which does not name §3.7's read tool get_frames"
    );
    assert!(
        names.iter().any(|name| name == "set_clip_speed"),
        "the speed pass offered {names:?}, which does not name its own write tool"
    );
}
