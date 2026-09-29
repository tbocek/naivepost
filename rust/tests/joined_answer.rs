//! The fake textedit replies, built with serde_json so no hand-escaped JSON literals are needed.

use serde_json::json;

/// Wrap one reply body in the chat envelope a real server answers with, so the pass unwraps it
/// through `answer_of` exactly as it would live.
pub fn envelope(body: &str) -> String {
    json!({ "choices": [ { "message": { "content": body } } ] }).to_string()
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
/// `tests/joins_flow.rs` hand theirs: the bare `{"joined":"..."}` object, no envelope. For a
/// scenario that needs two different answers in one session without the cache getting between them.
pub fn answer(content: &str) -> String {
    json!({ "joined": content }).to_string()
}
