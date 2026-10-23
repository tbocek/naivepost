//! The one reading of a streamed reply that §7's live page needs: the words, as they arrive.
//!
//! [`crate::llm_request::from_events`] is the app's reader of an event stream — it produces the answer a
//! step must trust, with its reasoning split, its tool-call fragments and its `[DONE]`/EOF verdict. This
//! module answers a smaller question for a different moment: a chunk of the raw body has just come off
//! the socket, and the run's exchange page wants the sentence the model has said SO FAR, before any of
//! that structure is complete. It therefore keeps only `choices[0].delta.content`, and nothing else.
//!
//! Two rules shape it. Bytes arrive in pieces the server did not align to lines (§9 §1: the request is
//! streamed because the answer is long), so a partial line waits for its rest rather than being parsed
//! half. And a line that cannot be read costs nothing: §7 ends with "recording never fails the call", so
//! a page that cannot decode a chunk simply shows less of it, while the step still gets its real answer
//! from `server_leg`'s own copy of the body.

/// The running text of one call's streamed reply.
#[derive(Default)]
pub struct Delta {
    /// A line the socket cut in half, kept until its newline lands. Bounded below so a server that never
    /// sends a newline cannot grow this without limit: the page is a convenience, the reply is not.
    pending: String,
}

/// Longest unterminated line tolerated. One SSE `data:` line carrying a whole base64 image would be
/// longer than this; a text delta never is, and dropping the excess costs the page a fragment, not the call.
const MAX_PENDING_BYTES: usize = 1 << 20;

impl Delta {
    /// Take the next raw bytes of the body and return the new words they carry, concatenated in the
    /// order their lines arrived. Empty when nothing completed — which is the ordinary answer for a
    /// chunk that ended mid-line, and for garbage.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        let mut text = String::new();
        // Bytes, not chars: cutting a UTF-8 sequence in half is normal on a socket, and `pending` holds
        // the incomplete tail until the rest arrives. Lossy conversion happens per LINE, once it is whole.
        let mut start = 0usize;
        for (at, byte) in bytes.iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            // Finish the line that started before this chunk, then take each complete line after it.
            self.pending
                .push_str(&String::from_utf8_lossy(&bytes[start..at]));
            text.push_str(&self.take_line());
            start = at + 1;
        }
        if start < bytes.len() {
            self.pending
                .push_str(&String::from_utf8_lossy(&bytes[start..]));
            if self.pending.len() > MAX_PENDING_BYTES {
                // Never terminated, so it will never be parsed: give up on it rather than grow forever.
                self.pending.clear();
            }
        }
        text
    }

    /// The words carried by whatever sits in `pending` plus `tail`, if the stream ends there. §1's EOF
    /// rule: a partial is the answer, so a connection that dies mid-sentence still puts its last words on
    /// the page instead of losing them to a missing newline.
    pub fn finish(&mut self, tail: &[u8]) -> String {
        self.pending.push_str(&String::from_utf8_lossy(tail));
        self.take_line()
    }

    /// One line out of `pending` and into the decoder.
    fn take_line(&mut self) -> String {
        let line = std::mem::take(&mut self.pending);
        content_of(&line)
    }
}

/// The words one SSE line carries: `data: {…}` → `choices[0].delta.content`. Everything else — a blank
/// keep-alive, a comment, `[DONE]`, JSON shaped differently, malformed JSON — answers empty. That is the
/// whole error policy here, and it is deliberate: see this module's header.
fn content_of(line: &str) -> String {
    let Some(payload) = line.trim().strip_prefix("data:") else {
        return String::new();
    };
    let payload = payload.trim();
    if payload.is_empty() || payload == crate::llm_request::DONE {
        return String::new();
    }
    let Ok(chunk) = serde_json::from_str::<serde_json::Value>(payload) else {
        return String::new();
    };
    chunk
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(|content| content.as_str())
        .unwrap_or_default()
        .to_string()
}
