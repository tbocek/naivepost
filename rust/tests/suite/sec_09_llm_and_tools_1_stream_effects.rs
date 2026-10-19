//! §09-llm-and-tools#1-request-contract S4, the read-back half: a chunked answer arrives over a real socket
//! and a real door returns the REASSEMBLED answer.
//!
//! `from_events` is unit-tested in `tests/suite/llm_request_contract.rs` and its routing over a socket in
//! `tests/suite/wire.rs`. What neither proves is that a live door — cache, retry leg, prompt assembly and all — sends
//! `"stream": true` and then reads the chunks back as one answer. This binary holds exactly ONE scenario,
//! because the settings address and the working directory are process-wide (see `sec_09_stream_shared.rs`).

#[path = "sec_09_stream_shared.rs"]
mod stream;

use stream::{decorated_clips, effects_chunks, never_stops};

#[test]
fn sec_09_llm_and_tools_1_request_contract_s4_a_streamed_answer_is_read_back() {
    let (_root, tree) = stream::fixture("effects");
    let llm = stream::StreamLlm::start(effects_chunks());
    stream::point_at(&llm.url());
    let held = decorated_clips();
    let asked = naivepost::effects_ask::ask_at(
        &tree,
        &stream::conf(),
        "context",
        &held,
        &[],
        1,
        &never_stops(),
    );
    let seen = llm.next();
    let body: serde_json::Value = serde_json::from_str(&seen.body).expect("the request is JSON");
    assert_eq!(body["stream"], serde_json::json!(true), "the ask went out streamed: {}", seen.body);
    assert_eq!(body["model"], serde_json::json!(stream::MODEL), "the settings' model asks");
    assert!(
        !asked.failed,
        "a chunked answer must read as an answer, not a wire failure: {}",
        asked.reason
    );
    assert_eq!(
        asked.calls.len(),
        2,
        "the EFFECTS document split across chunks came back whole: {:?}",
        asked.calls
    );
    assert_eq!(asked.calls[0].kind, "zoom", "the first decoration is the first one asked for");
    assert_eq!(asked.calls[1].kind, "volume");
    assert_eq!(asked.calls[1].clip, 2, "the second decoration keeps its own clip");
    assert_eq!(asked.calls[1].gain, Some(0.5), "a number split mid-token still parses");
}
