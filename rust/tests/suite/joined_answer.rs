//! The fake textedit replies, built with serde_json so no hand-escaped JSON literals are needed.

use serde_json::json;

/// Wrap one reply body in the chat envelope a real server answers with, so the pass unwraps it
/// through `answer_of` exactly as it would live.
pub fn envelope(body: &str) -> String {
    json!({ "choices": [ { "message": { "content": body } } ] }).to_string()
}

/// One chat reply that ENDS a §3.2 tool round: a `finish` call, spelled the way a server spells a
/// call. The join ask now offers its tools (it did not, once — that was the gap), and a tool round is
/// not an answer, so a test that wants the flow to stop needs this as the round after its answer.
pub fn finish() -> String {
    tool_calls_reply(vec![("finish", json!({}))])
}

/// One `drop_words` call, the shape §3.5 asks for: which side, how many words.
pub fn drop_words(side: &str, count: usize) -> String {
    tool_calls_reply(vec![(
        "drop_words",
        json!({ "side": side, "count": count }),
    )])
}

/// The chat envelope carrying calls rather than an answer. A server with `enable_thinking` answers
/// that way on a tool round: the reasoning text and the calls TOGETHER, which is why the prose answer
/// rides along here — a round that shows the app what it decided AND asks for the next round is what a
/// real thinking server sends, and the first ask is then the answer itself.
pub fn tool_calls_reply(calls: Vec<(&str, serde_json::Value)>) -> String {
    let spelled: Vec<serde_json::Value> = calls
        .into_iter()
        .enumerate()
        .map(|(n, (name, arguments))| {
            json!({
                "id": format!("call_{n}_{name}"),
                "type": "function",
                "function": { "name": name, "arguments": arguments.to_string() },
            })
        })
        .collect();
    // The answer text a call round may carry, so a scenario can send its joined text and its calls in
    // one reply the way a thinking server's single round does.
    json!({ "choices": [ { "message": { "content": "", "tool_calls": spelled } } ] }).to_string()
}

/// One round that answers with BOTH: the `drop_words` call and the `{"joined":…}` text, as a thinking
/// server's first round arrives — the model states its repair and calls the tool that applies it.
/// The app reads the CALL, so the prose beside it must not be charged a second time; the test that
/// uses this pins exactly that.
pub fn drop_words_with_answer(side: &str, count: usize, joined: &str) -> String {
    json!({ "choices": [ { "message": {
        "content": json!({ "joined": joined }).to_string(),
        "tool_calls": [ {
            "id": "call_drop",
            "type": "function",
            "function": {
                "name": "drop_words",
                "arguments": json!({ "side": side, "count": count }).to_string()
            }
        } ]
    } } ] })
    .to_string()
}

/// Whether a reply body is a `finish` call — what the fake answers "are you done?" with.
pub fn is_finish(body: &str) -> bool {
    body.contains("\"finish\"")
}

/// One textedit answer to be served OVER THE WIRE: the `{"joined":"..."}` object the job is asked
/// for, inside the chat envelope.
pub fn chat(content: &str) -> String {
    envelope(&json!({ "joined": content }).to_string())
}

/// A reply whose content is NOT a `{"joined":"..."}` answer, for a scenario that wants to see the
/// pass refuse it.
pub fn raw(content: &str) -> String {
    envelope(content)
}

/// One textedit answer handed to `joins::answer_join` DIRECTLY, as the rule tests in
/// `tests/suite/joins_flow.rs` hand theirs: the bare `{"joined":"..."}` object, no envelope. For a
/// scenario that needs two different answers in one session without the cache getting between them.
pub fn answer(content: &str) -> String {
    json!({ "joined": content }).to_string()
}
