//! F6.3 — `spec/09-llm-and-tools.md` §4, liveness and the gate.

use std::collections::{HashMap, HashSet};

pub const STALL_MINUTES: u64 = 5;

pub const WHOLE_MINUTES: u64 = 10;

pub const HEARTBEAT_SECONDS: u64 = 60;

pub const POLL_SECONDS: u64 = HEARTBEAT_SECONDS / 4;

pub const TAIL_BYTES: usize = 90;

pub const TAIL_KEPT_BYTES: usize = 360;

pub fn stall_seconds() -> u64 {
    static BOUND: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *BOUND.get_or_init(|| configured("NAIVEPOST_LLM_STALL_SECONDS").unwrap_or(STALL_MINUTES * 60))
}

/// The heartbeat's period the LIVE watch is measured against, in seconds.
pub fn heartbeat_seconds() -> u64 {
    static MINUTE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *MINUTE
        .get_or_init(|| configured("NAIVEPOST_LLM_HEARTBEAT_SECONDS").unwrap_or(HEARTBEAT_SECONDS))
}

/// One override read from the environment: set and parseable wins, anything else means "not configured".
fn configured(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse().ok()
}

/// §5's spin button range: 1 to 16.
pub const SLOTS_MIN: u32 = 1;

/// §5's spin button range: 1 to 16.
pub const SLOTS_MAX: u32 = 16;

pub fn clamped_slots(slots: u32) -> u32 {
    slots.clamp(SLOTS_MIN, SLOTS_MAX)
}

pub fn duration_spelling(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let (minutes, rest) = (seconds / 60, seconds % 60);
    if rest == 0 {
        return format!("{minutes}m0s");
    }
    format!("{minutes}m{rest}s")
}

pub fn size_spelling(bytes: usize) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // One decimal, rounded: multiply-then-divide on integers because the crate builds without float formatting
    // helpers in this path, and `(x * 10 / 1024)` is the rounding a reader expects from "1.5 kB".
    let tenths = bytes as u64 * 10 / 1024;
    format!("{}.{:01} kB", tenths / 10, tenths % 10)
}

pub fn go_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for char in text.chars() {
        match char {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            // Everything else — other control characters included — goes through as it stands: this quotes for a
            // human reading a log, not for a machine re-parsing it, and §4 asks only that the tail not break the
            // line it sits on.
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The heartbeat's tail: whitespace collapsed, then the last [`TAIL_BYTES`] bytes, then `…`.
pub fn tail_of(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<&str>>().join(" ");
    if collapsed.is_empty() {
        return String::new();
    }
    // Short enough to show whole: no ellipsis, because nothing was removed.
    if text.len() <= TAIL_BYTES {
        return collapsed;
    }
    // The last TAIL_BYTES bytes, then forward to a boundary: slicing at the raw number would panic mid-character.
    let mut start = collapsed.len() - TAIL_BYTES;
    while start < collapsed.len() && !collapsed.is_char_boundary(start) {
        start += 1;
    }
    while start < collapsed.len() && !collapsed.is_char_boundary(start) {
        start += 1;
    }
    format!("\u{2026}{}", &collapsed[start..])
}

/// What one poll decided about a running call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    /// Nothing to say this poll — under the heartbeat's minute, or nothing new and nothing wrong.
    Idle,
    /// The line for this minute.
    Heartbeat(String),
    /// The call is over: the log line, and the reason the caller reports.
    GaveUp { log: String, blame: String },
}

#[derive(Debug, Clone)]
pub struct Watch {
    pub step: String,
    /// The moment the request went on the wire — *after* the gate gave its slot, never the moment the step asked.
    pub started: u64,
    pub last_byte: Option<u64>,
    /// When a heartbeat was last said, so the first is a minute in and not immediately.
    pub last_said: u64,
    pub streamed: bool,
    pub stall_seconds: u64,
    pub thinking_bytes: usize,
    pub reply_bytes: usize,
    /// Kept for the tail only, and only [`TAIL_KEPT_BYTES`] of it. Reasoning is counted and shown, never returned.
    thinking: String,
    reply: String,
}

impl Watch {
    pub fn streamed(step: &str, now: u64) -> Self {
        Self::new(step, now, true)
    }

    pub fn unstreamed(step: &str, now: u64) -> Self {
        Self::new(step, now, false)
    }

    pub fn with_stall_seconds(mut self, seconds: u64) -> Self {
        self.stall_seconds = seconds;
        self
    }

    fn new(step: &str, now: u64, streamed: bool) -> Self {
        Self {
            step: step.to_string(),
            started: now,
            last_byte: None,
            last_said: now,
            streamed,
            stall_seconds: STALL_MINUTES * 60,
            thinking_bytes: 0,
            reply_bytes: 0,
            thinking: String::new(),
            reply: String::new(),
        }
    }

    pub fn byte_arrived(&mut self, now: u64) {
        // Deliberately not `last_said`: a token per second would otherwise push the heartbeat further away for-
        // ever, and the point of a minute's line is that it arrives whether or not anything does.
        self.last_byte = Some(now);
    }

    pub fn answered_at(&mut self, now: u64, text: &str, is_reasoning: bool) {
        self.byte_arrived(now);
        self.wrote(text, is_reasoning);
    }

    /// What those bytes turned out to be — the answer, or the reasoning sent alongside it.
    pub fn wrote(&mut self, text: &str, is_reasoning: bool) {
        let (counted, kept) = if is_reasoning {
            (&mut self.thinking_bytes, &mut self.thinking)
        } else {
            (&mut self.reply_bytes, &mut self.reply)
        };
        *counted += text.len();
        kept.push_str(text);
        // Trimmed here rather than in a method on `self` because both fields are being touched at once.
        if kept.len() > TAIL_KEPT_BYTES {
            let mut start = kept.len() - TAIL_KEPT_BYTES;
            while start < kept.len() && !kept.is_char_boundary(start) {
                start += 1;
            }
            *kept = kept[start..].to_string();
        }
    }

    /// How long nothing has arrived.
    pub fn quiet(&self, now: u64) -> u64 {
        // Nothing has ever arrived, so the call has been quiet for as long as it has been running.
        now.saturating_sub(self.last_byte.unwrap_or(self.started))
    }

    pub fn ever_arrived(&self) -> bool {
        self.last_byte.is_some()
    }

    /// The guard's look: four times a heartbeat, and it says at most one line a minute.
    pub fn poll(&mut self, now: u64) -> Poll {
        let quiet = self.quiet(now);
        if self.streamed && quiet >= self.stall_seconds {
            return Poll::GaveUp {
                log: format!(
                    ">>> {}: nothing for {} \u{2014} giving up",
                    self.step,
                    duration_spelling(quiet)
                ),
                blame: self.blame(now),
            };
        }
        if !self.streamed && now.saturating_sub(self.started) >= WHOLE_MINUTES * 60 {
            let over = now.saturating_sub(self.started);
            return Poll::GaveUp {
                log: format!(
                    ">>> {}: nothing for {} \u{2014} giving up",
                    self.step,
                    duration_spelling(over)
                ),
                // Nothing arrived and nothing can: the ceiling ended a call that was never going to stay quiet.
                blame: format!("no answer in {}", duration_spelling(over)),
            };
        }
        // The guard ticks from the last line *said*, not from the last byte — otherwise a call streaming at one
        // byte a minute would go a full silent minute without ever speaking, which is the opposite of what §4
        // wants. Bytes move `last_byte`; this moves `last_said`, and each has its own job.
        if now.saturating_sub(self.last_said) < heartbeat_seconds() {
            return Poll::Idle;
        }
        self.last_said = now;
        Poll::Heartbeat(self.heartbeat(now))
    }

    /// The minute's line, in one of §4's three shapes.
    fn heartbeat(&self, now: u64) -> String {
        let since = duration_spelling(now.saturating_sub(self.started));
        // Nothing at all yet — not a byte that turned into text. This is the shape an unstreamed call keeps for
        // its whole life, which is §4's point about X and Y staying 0.
        if self.reply_bytes == 0 && self.thinking_bytes == 0 {
            return format!(">>> {}: nothing yet, {since} in", self.step);
        }
        let head = format!(
            ">>> {}: {} thinking, {} reply, {since} in",
            self.step,
            size_spelling(self.thinking_bytes),
            size_spelling(self.reply_bytes)
        );
        // The answer is what gets quoted — it is what is being judged. Until there is one the reasoning is quoted
        // instead: a call six minutes in with nothing to show is exactly when what it is thinking is the only thing
        // worth knowing.
        let tail = match tail_of(&self.reply) {
            tail if !tail.is_empty() => tail,
            _ => tail_of(&self.thinking),
        };
        if tail.is_empty() {
            return head;
        }
        format!("{head} \u{2014} {}", go_quote(&tail))
    }

    pub fn blame(&self, now: u64) -> String {
        blame_at(
            now.saturating_sub(self.started),
            self.quiet(now),
            self.ever_arrived(),
            self.stall_seconds,
        )
    }
}

pub fn blame(ran_for: u64, quiet: u64, ever_arrived: bool) -> String {
    blame_at(ran_for, quiet, ever_arrived, STALL_MINUTES * 60)
}

pub fn blame_at(ran_for: u64, quiet: u64, ever_arrived: bool, bound_seconds: u64) -> String {
    let stall = duration_spelling(bound_seconds);
    if !ever_arrived {
        return format!("nothing arrived in {stall}");
    }
    format!(
        "stopped answering after {} -- nothing more for {}",
        duration_spelling(ran_for),
        duration_spelling(quiet)
    )
}

/// What asking for a slot answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acquisition {
    /// The slot is ours: put the request on the wire and start the [`Watch`] from this moment.
    Taken,
    Waiting {
        log: String,
    },
    /// ⏹ ended the wait before a slot came free.
    Cancelled,
}

/// One model's slots and who holds them — first come first served.
#[derive(Debug, Clone)]
pub struct Gate {
    model: String,
    slots: u32,
    held: Vec<String>,
    /// The queue, front to back. A `Vec` rather than a `VecDeque` because it is tiny and scanned by step name.
    queue: Vec<String>,
    /// Which steps have already been told they are waiting, so the line is said once per queueing spell.
    spoke: HashSet<String>,
    held_count: HashMap<String, usize>,
    admitted: HashSet<String>,
}

impl Gate {
    /// A gate named `model` (the server it holds, in this app) with `slots` turns ([`clamped_slots`]' range).
    pub fn new(model: &str, slots: u32) -> Self {
        Self {
            model: model.to_string(),
            slots: clamped_slots(slots),
            held: Vec::new(),
            queue: Vec::new(),
            spoke: HashSet::new(),
            held_count: HashMap::new(),
            admitted: HashSet::new(),
        }
    }

    /// How many requests the gate lets through from now on; whoever is on the wire stays there.
    pub fn set_slots(&mut self, slots: u32) {
        self.slots = clamped_slots(slots);
    }

    pub fn acquire(&mut self, step: &str) -> Acquisition {
        // A slot handed over by a release is collected here rather than re-bought below. The ordinary path
        // cannot see the hand-off: the asker already sits in `held`, which the gate reads as "waiting for
        // myself" and answers silently, so a step whose turn had arrived would wait forever behind itself.
        if self.admitted.remove(step) {
            return Acquisition::Taken;
        }
        // A free slot is only taken when nobody is in front of the asker: a step that arrived earlier keeps its
        // place against anyone who asks later, which is what "first come first served" has to mean when several
        // steps share one model.
        if self.queue.is_empty() && self.held.len() < self.slots as usize {
            self.held.push(step.to_string());
            *self.held_count.entry(step.to_string()).or_insert(0) += 1;
            return Acquisition::Taken;
        }
        if !self.queue.iter().any(|waiting| waiting == step) {
            self.queue.push(step.to_string());
        }
        // Already waiting: the line is this step's to say, and only once.
        Acquisition::Waiting {
            log: self.waiting_log(step),
        }
    }

    /// Who is on the wire.
    pub fn holders(&self) -> Vec<String> {
        self.held.clone()
    }

    pub fn release(&mut self, step: &str) {
        let at = self.held.iter().position(|holder| holder == step);
        let freed = at.is_some();
        // The released step's own uncollected promise is void: it got on the wire and is finished, so nothing
        // is owed to it. Without this a step served long ago would keep an old hand-off that its next ask could
        // spend as `Taken` when the gate is actually full again.
        self.admitted.remove(step);
        if let Some(at) = at {
            self.held.remove(at);
            // The count goes with the entry it counted. A step that still holds another slot stays on the wire
            // for the hand-off rule below — it is not "someone who finished".
            if let Some(n) = self.held_count.get_mut(step) {
                *n -= 1;
                if *n == 0 {
                    self.held_count.remove(step);
                }
            }
        }
        // Cleared only for whoever actually held a slot: a step still queued behind others has not finished being
        // told it waits, and clearing its flag here would let the same line print again on the next poll.
        if freed {
            self.spoke.remove(step);
            // The slot this just freed goes to whoever asked first, straight away — that is what "waits its turn"
            // means, and it cannot be decided later by whoever happens to ask next.
            if !self.queue.is_empty() {
                let next = self.queue.remove(0);
                self.spoke.remove(&next);
                // The hand-off is only a promise when there is somewhere to put it. At the last slot the queue's
                // head IS the only holder, so naming it in a later log would read "narrate is waiting for
                // narrate" — and the step cannot be given a second slot it never asked for. It learns of its
                // turn by finding itself holding the slot, which is what `holders()` already shows.
                let already_ours = self.held_count.contains_key(&next);
                if !already_ours && self.held.len() < self.slots as usize {
                    self.held.push(next.clone());
                    *self.held_count.entry(next.clone()).or_insert(0) += 1;
                    // Promised, not yet collected: the next ask from this step takes THAT slot.
                    self.admitted.insert(next);
                } else {
                    self.queue.insert(0, next);
                }
            }
        }
    }

    pub fn cancel_waiting(&mut self, step: &str) -> bool {
        let before = self.queue.len();
        self.queue.retain(|waiting| waiting != step);
        self.spoke.remove(step);
        self.queue.len() < before
    }

    /// Who is queued, front first — the order they will be admitted in.
    pub fn waiting(&self) -> Vec<String> {
        self.queue.clone()
    }

    fn waiting_log(&mut self, step: &str) -> String {
        if self.naming_itself(step) {
            return String::new();
        }
        if !self.spoke.insert(step.to_string()) {
            // Already said once for this queueing spell; repeating it every poll would bury the log.
            return String::new();
        }
        let holders = self.held.join(", ");
        format!(
            ">>> {step}: waiting for {} -- busy with {holders}",
            self.model
        )
    }

    fn naming_itself(&self, step: &str) -> bool {
        // Compared on the step's own name: `slot_gate::hold` gives every ask a `#N` of its own, so "Publish#4"
        // behind "Publish#3" is the same step behind itself. Compared whole, the two never matched and the
        // rule never held.
        let name = |holder: &str| holder.split('#').next().unwrap_or(holder).to_string();
        self.held.iter().any(|holder| name(holder) == name(step))
    }
}

/// How many of one step's requests may be on the wire at once.
pub fn in_flight(independent: bool, slots: u32) -> usize {
    if independent {
        clamped_slots(slots) as usize
    } else {
        1
    }
}

pub fn says_something(read: &[u8]) -> bool {
    let text = String::from_utf8_lossy(read);
    let mut events = 0;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Some(json) = line.strip_prefix("data:") else {
            return true;
        };
        let Ok(event) = serde_json::from_str::<serde_json::Value>(json.trim()) else {
            return true;
        };
        let delta = &event["choices"][0]["delta"];
        let blank = |key: &str| {
            delta[key]
                .as_str()
                .is_none_or(|said| said.trim().is_empty())
        };
        if !delta.is_object()
            || !blank("content")
            || !blank("reasoning_content")
            || !delta["tool_calls"].is_null()
        {
            return true;
        }
        events += 1;
    }
    events == 0
}
