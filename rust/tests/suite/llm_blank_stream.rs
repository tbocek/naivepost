//! A stream of blank text is not an answer on its way; anything else keeps the call alive.

use naivepost::llm_liveness::says_something;

fn event(field: &str, text: &str) -> String {
    format!(
        "data: {{\"choices\":[{{\"delta\":{{\"{field}\":{}}}}}]}}\n\n",
        serde_json::to_string(text).unwrap()
    )
}

#[test]
fn blank_text_says_nothing() {
    let read =
        event("content", "\n") + &event("content", "\n\n") + &event("reasoning_content", "  ");
    assert!(!says_something(read.as_bytes()));
}

#[test]
fn words_thinking_and_tool_calls_are_life() {
    assert!(says_something(event("content", "Hello").as_bytes()));
    assert!(says_something(event("reasoning_content", "Hmm").as_bytes()));
    assert!(says_something(
        b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0}]}}]}\n\n"
    ));
    let mixed = event("content", "\n") + &event("content", "word");
    assert!(says_something(mixed.as_bytes()));
}

#[test]
fn what_cannot_be_read_counts_as_life() {
    assert!(says_something(b"data: [DONE]\n\n"));
    assert!(says_something(b"data: {\"choices\":[{\"del"));
    assert!(says_something(
        b"{\"choices\":[{\"message\":{\"content\":\"x\"}}]}"
    ));
    assert!(says_something(b": keep-alive\n\n"));
}
