//! The fake textedit replies, built with serde_json so no hand-escaped JSON literals are needed.

use serde_json::json;

/// Wrap one reply body in the chat envelope a real server answers with, so the pass unwraps it
/// through `answer_of` exactly as it would live.
pub fn envelope(body: &str) -> String {
    json!({ "choices": [ { "message": { "content": body } } ] }).to_string()
}

/// One textedit answer to be served OVER THE WIRE: the words that go, quoted (§3.5), inside the chat envelope.
pub fn chat(drop_before: &str, drop_after: &str) -> String {
    envelope(&answer(drop_before, drop_after))
}

/// A reply whose content is NOT the quoted answer, for a scenario that wants to see the pass refuse it.
pub fn raw(content: &str) -> String {
    envelope(content)
}

/// One textedit answer handed to `joins::answer_join` DIRECTLY, as the rule tests in
/// `tests/suite/joins_flow.rs` hand theirs: the bare quote object, no envelope.
pub fn answer(drop_before: &str, drop_after: &str) -> String {
    json!({ "drop_before": drop_before, "drop_after": drop_after }).to_string()
}
