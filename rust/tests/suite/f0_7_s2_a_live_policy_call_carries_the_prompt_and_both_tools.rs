//! F0.7 S2 over the wire, scenario 1: what the derivation actually sends.
//!
//! One binary per scenario because `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory are
//! process-wide (see `policy_live_harness.rs`). The reply comes from a socket; nothing is injected.
#![allow(dead_code)]

#[path = "policy_live_harness.rs"]
mod harness;

use harness::*;
use serde_json::Value;

#[test]
fn f0_7_s2_a_live_derivation_call_carries_the_policy_prompt_and_both_tools() {
    let fixture = fixture("prompt");
    // A plain answer with no tool calls: S2's prompt says a default is a whole answer, so this settles
    // the round after one request and keeps the assertion about what went OUT.
    let llm = FakeLlm::start(vec![chat("nothing here settles a field")]);
    point_at(&llm.url());

    let run = derive_lively(&fixture, &fixture.context);
    let seen = llm.next_seen();

    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "the LLM's chat path");
    // The shipped `spec/prompts/policy.md` wording rode in the system message -- its own sentence, not
    // a paraphrase of it.
    assert!(
        seen.body
            .contains("set the editing policy fields it speaks to"),
        "the policy prompt went out: {}",
        clip(&seen.body)
    );
    assert!(
        seen.body.contains("Leave every other field alone"),
        "the whole of the shipped wording, not the first line"
    );
    // The User Context itself: the model cannot derive from a context it was not given.
    assert!(
        seen.body.contains("A lecture read from a script"),
        "the session's User Context went out: {}",
        clip(&seen.body)
    );
    // Both of S2's tools offered, and only those two.
    let body: Value = serde_json::from_str(&seen.body).expect("a JSON body");
    let tools = body
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
        .collect();
    assert!(
        names.contains(&"get_context"),
        "get_context offered: {names:?}"
    );
    assert!(
        names.contains(&"set_policy"),
        "set_policy offered: {names:?}"
    );
    assert_eq!(names.len(), 2, "S2 offers exactly two tools: {names:?}");
    // Thinking OFF, in BOTH spellings: llama.cpp reads the top-level pair, other servers only what sits
    // inside chat_template_kwargs, so a request that set one thinks on half the installs.
    assert_eq!(
        body.get("enable_thinking").and_then(Value::as_bool),
        Some(false),
        "thinking off at the top level"
    );
    assert_eq!(
        body.pointer("/chat_template_kwargs/enable_thinking")
            .and_then(Value::as_bool),
        Some(false),
        "thinking off in the template copy too"
    );
    // And the ask left its timed row behind (§09 §10: one line per request that went out).
    assert!(
        read(&fixture.tree.requests_tsv()).contains("policy"),
        "the derivation logged its request: {}",
        read(&fixture.tree.requests_tsv())
    );
    // An answer that named no field changed nothing: silence is silence, not a failure.
    assert!(
        run.derived.applied.is_empty() && run.derived.refusals.is_empty(),
        "no proposal, nothing applied and nothing refused: {:?}",
        run.derived
    );
}
