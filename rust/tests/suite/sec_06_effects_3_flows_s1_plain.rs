//! §06-effects §3 Flows S1, the NO-PLAIN-JSON scenario: a model that answers with the `{"clips":[…]}` document
//! instead of calling `add_caption` has NOT answered (§3's answered question on this item). The app asks again --
//! over the real seam, the correction in the conversation -- and when the answer is still a document the pass
//! says so by name and places nothing. Helpers and the other two scenarios:
//! `sec_06_effects_3_flows_s1_shared.rs`.

#![allow(dead_code)]

#[path = "sec_06_effects_3_flows_s1_shared.rs"]
mod shared;

use naivepost::captions_ask;
use naivepost::clip_rules_tools_leg;
use shared::{caption_json, clips, fixture, kept, log_said, point_at, rows_paid, FakeLlm};

#[test]
fn sec_06_effects_3_flows_s1_captions_plain_json_reply_is_unreadable_and_asked_again() {
    let fixture = fixture("s1-plain");
    // ONE scripted reply, so the retry is handed the same document: the proof it went out is the request count
    // and the timed rows, and the proof it did not help is the failure.
    let llm = FakeLlm::start(vec![caption_json(&[(
        1,
        &[(1.0, 4.0, "a document, not a call")],
    )])]);
    point_at(&llm.url());

    let answer = captions_ask::ask(
        &fixture.tree,
        "Caption every spoken line.",
        &clips(),
        &[],
        1,
        &|| false,
    );
    assert!(
        answer.failed,
        "a plain JSON document is not an answer: {:?}",
        answer.reason
    );
    assert_eq!(
        answer.reason,
        clip_rules_tools_leg::no_calls_reason("captions", "add_caption"),
        "the reason names the step and the call the model never made"
    );
    assert!(
        answer.calls.is_empty(),
        "nothing was placed from a reply that never called"
    );

    let seen = llm.drain();
    assert_eq!(
        seen.len(),
        2,
        "the ask-again went out over the wire: {} requests",
        seen.len()
    );
    let second = seen[1].body.clone();
    assert!(
        second.contains("Your answer failed validation"),
        "the correction rode back in the conversation (§3's retryTurn): {second}"
    );
    assert!(
        second.contains("not a tool call"),
        "and said what the model did wrong: {second}"
    );
    assert_eq!(
        rows_paid(&fixture),
        2,
        "§09 §10: the repair round is a request of its own in requests.tsv"
    );
    assert!(
        log_said(|line| line.starts_with(">>> captions:") && line.contains("asked again")),
        "the repair was said, not silently spent: {:?}",
        naivepost::ui::window_logs()
    );
    assert!(
        !kept(&fixture, "a document, not a call"),
        "an unreadable answer leaves nothing in the cache -- the next ask goes to the wire, not to the slot that failed"
    );
}
