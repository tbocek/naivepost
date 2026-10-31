//! The one reading of a streamed reply that §7's live page needs: the words, as they arrive.
//!
//! [`crate::llm_request::from_events`] reads an event stream into the answer a step must trust; this keeps
//! only `choices[0].delta.content`, so the exchange page can show what the model has said SO FAR. Bytes
//! arrive unaligned to lines (§9 §1), so a partial line waits for its rest. A line that cannot be read costs
//! nothing (§7: "recording never fails the call"); the step still gets its answer from `server_leg`'s copy.

#[derive(Default)]
pub struct Delta {
    /// A line the socket cut in half, kept until its newline lands; bounded so a server that never sends one
    /// cannot grow it without limit.
    pending: String,
}

/// Longest unterminated line tolerated. A whole base64 image on one SSE line would exceed it; a text delta never does.
const MAX_PENDING_BYTES: usize = 1 << 20;

impl Delta {
    /// Take the next raw bytes of the body and return the new words they carry, in arrival order. Empty when
    /// nothing completed (a chunk ending mid-line, or garbage).
    pub fn push(&mut self, bytes: &[u8]) -> String {
        let mut text = String::new();
        // Bytes, not chars: a UTF-8 sequence cut in half is normal on a socket. Lossy conversion happens per LINE, once whole.
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
                // Never terminated, so it will never be parsed: give up rather than grow forever.
                self.pending.clear();
            }
        }
        text
    }

    /// The words carried by whatever sits in `pending` plus `tail`, if the stream ends there. §1's EOF rule: a
    /// partial is the answer, so a connection dying mid-sentence still puts its last words on the page.
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

/// The words one SSE line carries: `data: {…}` → `choices[0].delta.content`. Everything else (keep-alive,
/// comment, `[DONE]`, other or malformed JSON) answers empty, deliberately.
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
