//! F1.10 S3's live half over a socket, one scenario per binary.
//!
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory are process-wide, so a
//! fixture that re-points them cannot share a test binary with another: the shared env would
//! make the second pass read the first one's settings and project folder. One binary each,
//! sharing the harness in `tests/join_live_harness.rs`.
#![allow(dead_code)]

#[path = "join_live_harness.rs"]
mod harness;

use harness::*;
use serde_json::Value;
#[path = "joined_answer.rs"]
mod joined_answer;
use joined_answer::{chat, raw};

#[test]
fn f1_10_s3_a_live_join_call_carries_the_join_message_to_the_llm() {
    let fixture = fixture("msg");
    let llm = FakeLlm::start(vec![chat("the network is live today the network goes live now")]);
    point_at(&llm.url());
    let lines = press(&fixture.tree, &fixture.project);
    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "the LLM's chat path");
    assert!(
        seen.body.contains("JOIN 1 of 1."),
        "the join's number went out: {}",
        clip(&seen.body)
    );
    assert!(
        seen.body.contains("BEFORE (the end of the take that was interrupted):"),
        "BEFORE labelled as the spec labels it"
    );
    assert!(
        seen.body.contains("AFTER (the beginning of the take that follows):"),
        "AFTER labelled as the spec labels it"
    );
    assert!(
        seen.body.contains("LEAVE OUT ONE STRETCH, AT THE JOIN"),
        "the shipped textedit wording rode in the system message"
    );
    let value: serde_json::Value = serde_json::from_str(&seen.body).expect("a JSON body");
    // The switch rides in BOTH spellings (llama.cpp reads the top-level pair, other servers only
    // what sits inside chat_template_kwargs), so a call that set one would think on half the installs.
    assert_eq!(
        value.get("enable_thinking").and_then(Value::as_bool),
        Some(true),
        "the top-level thinking flag rode out"
    );
    assert_eq!(
        value
            .pointer("/chat_template_kwargs/enable_thinking")
            .and_then(Value::as_bool),
        Some(true),
        "the template-side thinking flag rode out too"
    );
    assert!(
        lines.iter().any(|line| line.starts_with(">>> text edit: briefing")),
        "the pass said its brief line: {lines:?}"
    );
    assert!(
        read(&fixture.tree.requests_tsv()).contains("textedit"),
        "the ask left its row in requests.tsv"
    );
}
