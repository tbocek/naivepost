//! §09-llm-and-tools#1-request-contract S4, the body half: every live chat ask asks for a stream.
//!
//! §1 lists `stream true` among what a request may carry and §4 says every chat call is streamed. These are
//! the doors whose body can be built without a socket; the doors that only build one inside a dial get the same
//! check over the fake in `sec_09_llm_and_tools_1_stream_effects.rs`. No process env here, so this binary can
//! hold as many doors as there are.

/// The `stream` key each door's body carries, read back rather than string-matched so a differently ordered
/// or prettified body still answers.
fn stream_of(body: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(body).expect("the body is JSON")["stream"].clone()
}

#[test]
fn sec_09_llm_and_tools_1_request_contract_s4_every_live_chat_ask_asks_for_a_stream() {
    const MODEL: &str = "stream-model";
    let doors: Vec<(&str, String)> = vec![
        (
            "effects",
            naivepost::effects_ask::chat_body(MODEL, "system", "user").expect("an effects body"),
        ),
        (
            "upload text",
            naivepost::produce_upload_call::body(MODEL, "system", "the brief")
                .expect("an upload body"),
        ),
        (
            "speeds",
            naivepost::speeds_ask::chat_body(MODEL, "system", "user").expect("a speeds body"),
        ),
        (
            "translate",
            naivepost::produce_translate_ask::body(MODEL, "system", "1. line")
                .expect("a translate body"),
        ),
    ];
    for (door, body) in &doors {
        assert_eq!(
            stream_of(body),
            serde_json::json!(true),
            "{door}: §4 says every chat call is streamed"
        );
    }
    // The join's body door is the one the tool loop grows its conversation on.
    let joined = naivepost::join_ask::body_for_test(&["one".to_string(), "two".to_string()]);
    assert_eq!(
        stream_of(&joined),
        serde_json::json!(true),
        "the join asks streamed: {joined}"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&joined).expect("a join body")["model"],
        serde_json::json!("test-model"),
        "its model is its own"
    );
}
