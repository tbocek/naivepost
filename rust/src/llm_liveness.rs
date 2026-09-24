//! F6.3 — `spec/09-llm-and-tools.md` §4, liveness and the gate.
//!
//! Two separate questions get answered here, and keeping them apart is the point. *Liveness* asks whether a call
//! that is on the wire is still producing anything; *the gate* asks whether a call may go on the wire at all.
//! §4 says so outright: "The wait is cancellable by ⏹ and is not the stall watch's business: the watch starts
//! once the request is on the wire." A minute spent queueing behind another request is therefore never a minute
//! of "nothing yet" — and a call that has the slot is judged only by what comes back.
//!
//! Liveness is judged by **silence**, not by length: a streamed call may run as long as it takes, and is given up
//! on when nothing arrives for [`STALL_MINUTES`]. Length would be wrong twice over — a half-hour session's prompt
//! is tens of thousands of tokens the server must read before its first token, and a call that answers freely for
//! twenty minutes has done nothing wrong. An unstreamed call has no silence to measure, so it keeps the old
//! whole-call ceiling instead ([`WHOLE_MINUTES`]).
//!
//! # No clock, no threads
//!
//! Every moment is an argument — `now`, whole seconds — and [`Gate`] is plain state that methods mutate. A real
//! caller drives both from a ticker; this module only ever answers "what now, given these times". That keeps the
//! rules testable at 4:45 and 5:00 without waiting for either, which is the whole reason a stall rule can be
//! pinned by a test rather than hoped for.
//!
//! # Two models on one server are two slot counts
//!
//! §5's Settings text says so, and says why: "two models on one GPU share it however this is set". The spin
//! button counts *this app's* requests to *one model*; the server may hold more slots and serve other clients
//! with them. So [`Gate`] is built per model and knows nothing about its neighbours — [`shares_a_server`] exists
//! only so a caller can warn that two boxes point at one machine, never to merge their counts.
//!
//! # Ids used
//!
//! `P.eng.llmStallMinutes` (5) → [`STALL_MINUTES`], `P.eng.llmWholeMinutes` (10) → [`WHOLE_MINUTES`],
//! `P.eng.llmTailChars` (90 shown, 360 kept) → [`TAIL_BYTES`] and [`TAIL_KEPT_BYTES`], `P.machine.slots`
//! (default 1, one count per model) → [`DEFAULT_SLOTS`] with the seven counts in [`crate::settings::Slots`].

use std::collections::{HashMap, HashSet};

/// A streamed call may say nothing for this long before it is given up on (`P.eng.llmStallMinutes`, §4's first
/// bullet). Generous because the silence before the first token is real work, not a hang.
pub const STALL_MINUTES: u64 = 5;

/// The ceiling on a call nobody is streaming (`P.eng.llmWholeMinutes`). It cannot be a silence rule — there is no
/// silence to measure — so it stays the old deadline, kept for the short mechanical calls that are the only ones
/// made unstreamed. §4's note is that every chat call *is* streamed now: the prototype left two thinking jobs
/// (the upload text, and textedit) unstreamed, where they sat under this ceiling with no stall rule at all.
pub const WHOLE_MINUTES: u64 = 10;

/// How often a running call reports (§4: "Heartbeat every minute"). Often enough to see an answer going wrong
/// while it is still cheap to stop; rare enough that a ten-minute call is ten lines and not a scroll.
pub const HEARTBEAT_SECONDS: u64 = 60;

/// The guard looks four times per heartbeat (§4), so a stall is noticed on the poll at or after the bound — never
/// at 4:45, and never as late as 5:14.
pub const POLL_SECONDS: u64 = HEARTBEAT_SECONDS / 4;

/// How much of what just arrived goes in the heartbeat's tail (`P.eng.llmTailChars`, 90). Enough to recognise
/// prose, JSON, or a stuck model repeating itself.
pub const TAIL_BYTES: usize = 90;

/// How much text the watch holds on to (§10's row: "90 (360 kept)"). Comfortably more than the tail, so trimming
/// the tail is cheap and never needs the whole answer.
pub const TAIL_KEPT_BYTES: usize = 360;

/// A model's slot count when nothing says otherwise (`P.machine.slots`, default 1 — §5 spells that as "absent =
/// 1"). One row covers all seven models; each keeps its own value in [`crate::settings::Slots`].
pub const DEFAULT_SLOTS: u32 = 1;

/// §5's spin button range: 1 to 16.
pub const SLOTS_MIN: u32 = 1;

/// §5's spin button range: 1 to 16.
pub const SLOTS_MAX: u32 = 16;

/// A slot count forced into §5's range. Settings keeps what was typed (its own item's rule), so a caller builds
/// the gate through this: a conf left with 0 would make a gate nothing can ever enter, and a huge one a gate that
/// admits everything — both worse than the nearest end of the range the dialog offers.
pub fn clamped_slots(slots: u32) -> u32 {
    slots.clamp(SLOTS_MIN, SLOTS_MAX)
}

// ---- Spellings ----------------------------------------------------------------------

/// A duration as §4 prints it: `5m0s`, `10m0s`, `1m30s`, `90s`, `0s`. Go's `durOf` shape — seconds once there are
/// any, rounded to the second.
///
/// This deliberately differs from [`crate::llm_retry::wait_spelling`] ("1 min"): §3's prose spells its waits out
/// in words and §4 prints durations in its own examples, so each function follows the section whose line it
/// writes. Unifying them would make one of the two logs read wrong.
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

/// A size as the heartbeat spells it: bytes below a kibibyte, then one decimal in kB. The number a reader wants
/// from "X thinking, Y reply" is *roughly how much*, and 1536 B makes them do arithmetic.
pub fn size_spelling(bytes: usize) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // One decimal, rounded: multiply-then-divide on integers because the crate builds without float formatting
    // helpers in this path, and `(x * 10 / 1024)` is the rounding a reader expects from "1.5 kB".
    let tenths = bytes as u64 * 10 / 1024;
    format!("{}.{:01} kB", tenths / 10, tenths % 10)
}

/// Quote a string the way Go's `%q` does, which is what §4 means by "Go-quoted": surrounding double quotes, the
/// few escapes that matter in a log line, and every other character left as it was. A tail with a newline in it
/// would otherwise break the heartbeat into two lines and lose the `>>> ` prefix that makes it findable.
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

// ---- The tail -----------------------------------------------------------------------

/// The heartbeat's tail: whitespace collapsed, then the last [`TAIL_BYTES`] bytes, then `…`.
///
/// Collapse *before* cutting, which is the order §4's list gives and the only one that means what it says: cut
/// first and you keep 90 bytes of a run that was mostly spaces, so the reader sees whitespace where the model's
/// actual words should be. The cut counts bytes because that is what `P.eng.llmTailChars` bounds, and it steps
/// *forward* to a character boundary — a model writing dashes and em quotes puts multi-byte runes across the
/// cut, and printing half of one shows a replacement character exactly where the interesting text is.
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
    // Cut in the collapsed text rather than in the original, because the collapsed text is what gets shown —
    // cutting the original can start past the end of the string once its runs of spaces have gone.
    let mut start = collapsed.len() - TAIL_BYTES;
    while start < collapsed.len() && !collapsed.is_char_boundary(start) {
        start += 1;
    }
    while start < collapsed.len() && !collapsed.is_char_boundary(start) {
        start += 1;
    }
    format!("\u{2026}{}", &collapsed[start..])
}

// ---- The watch ----------------------------------------------------------------------

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

/// One call's liveness and its running commentary. Times are whole seconds supplied by the caller; nothing here
/// reads a clock, so the same sequence of `now` values replays identically in a test.
#[derive(Debug, Clone)]
pub struct Watch {
    pub step: String,
    /// The moment the request went on the wire — *after* the gate gave its slot, never the moment the step asked.
    pub started: u64,
    /// When a byte last arrived — `None` until one does, which is the only honest answer to "did anything ever
    /// come back". Silence from the start and silence after an answer are §4's two different sentences, and both
    /// look identical to a subtraction against `started`.
    pub last_byte: Option<u64>,
    /// When a heartbeat was last said, so the first is a minute in and not immediately.
    pub last_said: u64,
    pub streamed: bool,
    pub thinking_bytes: usize,
    pub reply_bytes: usize,
    /// Kept for the tail only, and only [`TAIL_KEPT_BYTES`] of it. Reasoning is counted and shown, never returned.
    thinking: String,
    reply: String,
}

impl Watch {
    /// A streamed call: judged by silence from `now`. The caller constructs this when the request goes on the wire
    /// (after [`Gate`]/[`Acquisition::Taken`]), which is what keeps a queue from looking like a stall.
    pub fn streamed(step: &str, now: u64) -> Self {
        Self::new(step, now, true)
    }

    /// An unstreamed call: never given up for silence, only past [`WHOLE_MINUTES`]. The guard still ticks, and X
    /// and Y stay 0 because nothing arrives until everything does.
    pub fn unstreamed(step: &str, now: u64) -> Self {
        Self::new(step, now, false)
    }

    fn new(step: &str, now: u64, streamed: bool) -> Self {
        Self {
            step: step.to_string(),
            started: now,
            last_byte: None,
            last_said: now,
            streamed,
            thinking_bytes: 0,
            reply_bytes: 0,
            thinking: String::new(),
            reply: String::new(),
        }
    }

    /// Something arrived. **Any** byte off the wire counts, including a keep-alive the event parser never turns
    /// into anything: both are proof the server is still there, which is the only thing being asked. This is why
    /// a call that streams nothing but pings for an hour is healthy and one that goes quiet for five minutes is
    /// not, whatever the model is doing in between.
    pub fn byte_arrived(&mut self, now: u64) {
        // Deliberately not `last_said`: a token per second would otherwise push the heartbeat further away for-
        // ever, and the point of a minute's line is that it arrives whether or not anything does.
        self.last_byte = Some(now);
    }

    /// A byte arrived at `now` and turned out to be this text. Same as [`Self::byte_arrived`] plus
    /// [`Self::wrote`], which is what a reader loop actually does; separate because the two facts are different —
    /// a keep-alive proves life and writes nothing.
    pub fn answered_at(&mut self, now: u64, text: &str, is_reasoning: bool) {
        self.byte_arrived(now);
        self.wrote(text, is_reasoning);
    }

    /// What those bytes turned out to be — the answer, or the reasoning sent alongside it.
    ///
    /// Bytes that turned into text were bytes off the wire first, so they are life exactly as a keep-alive is: the
    /// prototype's reader marks every read alive and `wrote` only adds what those bytes meant. Since this function
    /// takes no `now`, it refuses to *pretend* about a timestamp — it never moves the clock back, and a caller that
    /// wants the moment recorded calls [`Self::byte_arrived`] alongside, which is what a reader loop does anyway.
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

    /// Whether anything has ever arrived — the difference between a server that never answered and one that
    /// stopped, which §4 reports as two different sentences. Only "did a byte land after the call started" can
    /// answer it: `quiet(now)` is positive for a call that was silent from the first second too, and reading the
    /// silence as speech would report a server that never connected as one that stopped answering.
    pub fn ever_arrived(&self) -> bool {
        // Only "did a byte land after the call started" answers this, so it needs no `now`: `quiet(now)` is also
        // positive for a call that was silent from its first second, and reading that silence as speech would
        // report a server that never connected as one that stopped answering — §4's two sentences would collapse
        // into the wrong one. A caller whose bytes arrive without a timestamp (a test, or a reader that only sees
        // decoded text) calls [`Self::answered_at`] so the moment is recorded.
        self.last_byte.is_some()
    }


    /// The guard's look: four times a heartbeat, and it says at most one line a minute.
    ///
    /// Order matters. A streamed call that has gone quiet for [`STALL_MINUTES`] is over — checked first, so a call
    /// that stalls exactly when its minute comes round reports the give-up rather than a heartbeat nobody will act
    /// on. An unstreamed call skips the silence rule entirely and ends only at the ceiling.
    pub fn poll(&mut self, now: u64) -> Poll {
        let quiet = self.quiet(now);
        if self.streamed && quiet >= STALL_MINUTES * 60 {
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
        if now.saturating_sub(self.last_said) < HEARTBEAT_SECONDS {
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

    /// Why this call was given up, in one of §4's two sentences: nothing ever arrived, or it stopped answering
    /// after a while. The second names how long the call had been going — that is what tells a reader to blame the
    /// prompt's size rather than the connection — and it comes from the watch, which is the only thing holding both
    /// moments; [`blame`] spells the pair once `now` and the start are known.
    pub fn blame(&self, now: u64) -> String {
        blame(
            now.saturating_sub(self.started),
            self.quiet(now),
            self.ever_arrived(),
        )
    }
}

/// Why the caller says the call was given up (§4's two cancellations). `ever_arrived` separates a server that never
/// spoke from one that stopped. `ran_for` is how long the call had been going and `quiet` how long it had then said
/// nothing; the second sentence leads with the first, because that is what tells a reader to blame the prompt's size
/// rather than the connection — and both are needed, since "stopped answering after 5m0s" on a call that had been
/// running half an hour blames the wrong thing.
///
/// A ⏹ press is not this function's business: the caller reports its own stop, because only the run knows whether
/// the user ended it or the guard did.
pub fn blame(ran_for: u64, quiet: u64, ever_arrived: bool) -> String {
    let stall = duration_spelling(STALL_MINUTES * 60);
    if !ever_arrived {
        return format!("nothing arrived in {stall}");
    }
    format!(
        "stopped answering after {} -- nothing more for {}",
        duration_spelling(ran_for),
        duration_spelling(quiet)
    )
}

// ---- The gate -----------------------------------------------------------------------

/// What asking for a slot answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acquisition {
    /// The slot is ours: put the request on the wire and start the [`Watch`] from this moment.
    Taken,
    /// Every slot is busy; this is the line to log, or `String::new()` when the step is only queued behind its own
    /// earlier call and should say nothing.
    Waiting { log: String },
    /// ⏹ ended the wait before a slot came free.
    Cancelled,
}

/// One model's slots and who holds them — first come first served.
///
/// A request takes a slot before it goes on the wire and gives it back when the reply is in *or* the call is given
/// up; §4 is explicit that the hold covers the HTTP call only, so a step thinking between tool rounds holds
/// nothing. There is no re-entrancy magic here: `release` is the caller's job at both ends of the call, which is
/// also what makes a leak visible rather than silent.
#[derive(Debug, Clone)]
pub struct Gate {
    model: String,
    slots: u32,
    /// Who is on the wire now, in arrival order and with repeats — a step may hold two of four slots, and the log
    /// names every holder so a reader can see which one has been there longest.
    held: Vec<String>,
    /// The queue, front to back. A `Vec` rather than a `VecDeque` because it is tiny and scanned by step name.
    queue: Vec<String>,
    /// Which steps have already been told they are waiting, so the line is said once per queueing spell.
    spoke: HashSet<String>,
    /// How many slots each holder owes a release. A step may hold several (a gate of four, two held by `describe`),
    /// so "is this step still on the wire?" is a count and not a yes/no — and the hand-off decision below depends
    /// on it exactly: handing over to a step that already holds one would give it a second slot for one queued
    /// request, and leave the first unaccounted for.
    held_count: HashMap<String, usize>,
    /// The steps a [`Gate::release`] has handed a slot to but has not yet been collected. The promise stands for
    /// exactly one collect, and a step's own `release` voids it: an uncollected promise from a hand-off that has
    /// since been served would otherwise make a *later* ask come back `Taken` when it should be a new wait.
    admitted: HashSet<String>,
}

impl Gate {
    /// A gate for one model with `slots` of them ([`clamped_slots`]' range). Two models on one server get two
    /// gates; nothing here knows they share a machine.
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

    /// Ask for a slot. Free, with nobody ahead in the queue → [`Acquisition::Taken`] and nothing to log; otherwise
    /// the step joins (or stays where it already was) and gets its once-only sentence.
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
        Acquisition::Waiting { log: self.waiting_log(step) }
    }


    /// Who is on the wire.
    pub fn holders(&self) -> Vec<String> {
        self.held.clone()
    }

    /// Give a slot back. The first holder named is the one that finishes — matching how they were admitted, so a
    /// step holding two slots releases its own rather than a neighbour's. Then whoever waited longest goes in: §4's
    /// "waits its turn, first come first served" only means something if the hand-off is automatic, and doing it
    /// here means a caller cannot forget to ask. The step's once-only line is forgotten at this point too — if it
    /// has to wait again later, that is news worth saying once more.
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
                // The hand-off goes to the queue's head only when that step is not already on the wire. If it
                // holds a slot, handing it another would give one queued request two slots and leave the first
                // unaccounted for; if the gate is clamped to zero there is no room at all. Both go back to the
                // front of the queue, where the next release (or a grown slot count) tries again — so nobody is
                // ever handed a slot that does not exist, and nobody loses their place in line.
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



    /// ⏹ during a wait. Answers whether there was a wait to cancel, so a caller that stops after being `Taken`
    /// does not report cancelling something that never happened.
    pub fn cancel_waiting(&mut self, step: &str) -> bool {
        let before = self.queue.len();
        self.queue.retain(|waiting| waiting != step);
        self.spoke.remove(step);
        self.queue.len() < before
    }

    /// Whether the step is queued right now.
    pub fn is_waiting(&self, step: &str) -> bool {
        self.queue.iter().any(|waiting| waiting == step)
    }

    /// Who is queued, front first — the order they will be admitted in.
    pub fn waiting(&self) -> Vec<String> {
        self.queue.clone()
    }

    /// §4's line: `>>> ‹step›: waiting for ‹model› -- all N slot(s) busy (‹steps›)`, naming every holder so a reader
    /// can see how many slots are spoken for. Empty when the step is queued behind its own earlier call — see
    /// [`Self::naming_itself`].
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
            ">>> {step}: waiting for {} -- all {} slot(s) busy ({holders})",
            self.model, self.slots
        )
    }

    /// The prototype's rule, kept: a step queued behind its *own* earlier call waits silently. Naming the holder
    /// would name the asker, and "publish is waiting for publish" says less than silence — the reason it is
    /// waiting is already obvious from there being two of its requests in flight.
    fn naming_itself(&self, step: &str) -> bool {
        self.held.iter().any(|holder| holder == step)
    }


}

// ---- How many at once ---------------------------------------------------------------

/// How many of one step's requests may be on the wire at once.
///
/// Steps whose requests do not depend on each other MAY send up to N: describe chunks of different videos, fix
/// blocks, the retake pool's runs, subtitle translations, one TTS line per slot. A step whose requests depend on
/// the answers before them sends them in order whatever N is — the join pass (F1.10, `spec/04-prepare.md`), whose
/// every window leaves out what the joins before it took, and a tool conversation's rounds ([`crate::tool_loop`]).
/// So this returns 1 for the dependent case however big the slot count is: those extra slots belong to that
/// model's *other* users, not to a step that cannot use them.
pub fn in_flight(independent: bool, slots: u32) -> usize {
    if independent {
        clamped_slots(slots) as usize
    } else {
        1
    }
}

/// Do these two models live on one server? A caller warns with this; it never merges slot counts, because §5 says
/// plainly that two models on one GPU share it however the boxes are set.
///
/// Compared on scheme, host and port of the two servers' URLs — after [`crate::services::server_url`] has done
/// its scheme/slash work. A bare host reads as `https` (02 §20), so `llama:8080` and `http://llama:8080` are
/// two addresses, one reached over TLS and one not, and conflating them would point a warning's reader at the
/// wrong box.
pub fn shares_a_server(a: &str, b: &str) -> bool {
    // Compared on host *and* port. Two ports on one machine are two servers by agreement — the arrangement §1
    // describes — so they need no warning; one address wearing two prefixes is one server, and that is the case
    // worth naming, because it is one GPU being asked for two jobs at once. Two spellings of one machine
    // (`llama` and `10.0.0.4`) cannot be told apart here without DNS, so this never claims more than it can see.
    let host_and_port = |raw: &str| -> Option<String> {
        // services::server_url does the scheme and trailing-slash work, so `llama:8080` and `http://llama:8080/`
        // are one address here. The port is then added when the box leaves it off, because a URL without one means
        // its scheme's default — `http://host` and `http://host:80` are one server, and missing that would let the
        // common case slip past a warning that exists to be read.
        let url = crate::services::server_url(raw)?;
        // The scheme is part of the address kept below: a bare host reads as `https` (02 §20), so `llama:8080`
        // and `http://llama:8080` really are two addresses — one reached over TLS, one not — and conflating
        // them would send a warning's reader to the wrong box.
        let (scheme, after_scheme) = url.split_once("://")?;
        let host_and_maybe_port = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
        if host_and_maybe_port.is_empty() {
            return None;
        }
        let with_port = match host_and_maybe_port.rsplit_once(':') {
            Some((host, port)) if !port.is_empty() => host_and_maybe_port.to_string(),
            _ => format!("{}:{}", host_and_maybe_port, if scheme == "https" { 443 } else { 80 }),
        };
        Some(format!("{scheme}://{with_port}"))
    };
    match (host_and_port(a), host_and_port(b)) {
        (Some(one), Some(other)) => one == other,
        _ => false,
    }
}
