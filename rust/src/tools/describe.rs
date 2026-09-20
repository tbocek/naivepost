//! Describe — spec/02-services.md §3.2, one chunk of frames at a time.
//!
//! The model is shown frames and asked for one EVENT per frame plus a running STATE. What it gets
//! back from `record_event` is the frame it landed on and that frame's session second, because the
//! number it sent is an index into this batch and only the app knows which second that is.
//!
//! The prototype's cleanups of a prose reply are kept here rather than in a prompt: they are about
//! what a model actually writes — an offset rounded to the nearest half second, a description with
//! the `EVENT:` label missing, an answer across two lines — and each one used to be silent. They
//! stay silent to the model too (§3.2's "all invisible to the model"); what changes is that they are
//! now code with names, so a wrong snap can be found.

use serde_json::json;

/// How many of the previous EVENT lines ride along with each request — what [`Batch::with_history`]
/// stands for, and the reason a batch's first frame may be answered "same" at all.
/// P.machine.describeRecentEvents
pub const RECENT_EVENTS: usize = 3;

/// Speech context per side per source: how many segments either side of the chunk join the brief.
/// P.machine.describeCtxSegs
pub const CTX_SEGS: usize = 2;

/// …and how far back or forward each side looks for them. P.machine.describeCtxWindowSeconds
pub const CTX_WINDOW_SECONDS: f64 = 10.0;

/// One chunk of frames: the seconds the batch was stamped with, in order, and the interval between
/// them (`P.project.frameInterval`, the same value the request's own stamps were built from).
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    /// Session seconds of the frames in this batch, oldest first. The model numbers them 1..=n.
    frames: Vec<f64>,
    interval: f64,
    /// What has been recorded so far, in the order it arrived.
    events: Vec<Event>,
    /// The running STATE, if any.
    state: Option<String>,
    /// Whether this batch has anything before it to be "same" as — the previous batches' EVENT lines.
    /// §3.2's finish rule needs exactly this and nothing more.
    has_history: bool,
}

/// One recorded event: which frame of the batch it landed on and what was said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// 1..=n as stamped — the number the model sent, not an index.
    pub frame: usize,
    pub text: String,
    pub calm: bool,
}

impl Batch {
    /// A batch of `count` frames starting at session second `start`, `interval` apart.
    ///
    /// The seconds are computed rather than passed in because the model's stamps and these have to
    /// agree by construction: two sources of the same arithmetic is how a frame ends up named by
    /// one second and written down at another.
    pub fn new(start: f64, interval: f64, count: usize) -> Batch {
        Batch {
            frames: (0..count).map(|i| start + i as f64 * interval).collect(),
            interval,
            events: Vec::new(),
            state: None,
            has_history: false,
        }
    }

    /// A batch that continues a recording with EVENT lines already behind it.
    pub fn with_history(mut self) -> Self {
        self.has_history = true;
        self
    }

    /// The frames of this batch, as the request stamped them.
    pub fn frames(&self) -> &[f64] {
        &self.frames
    }

    /// `record_event(frame, text, calm)` — ok naming the frame the event landed on and its session
    /// second; a frame recorded twice replaces what was there; an error when `frame` is not in this
    /// batch.
    ///
    /// The frame number is 1-based because that is how the request stamped it ("FRAME 1 of 4"), and
    /// out-of-range is refused rather than clamped: a model that says frame 9 of 4 has lost track of
    /// the batch, and quietly filing its words on the last frame would invent an event at a second
    /// nobody described.
    pub fn record_event(&mut self, frame: usize, text: &str, calm: bool) -> String {
        if frame == 0 || frame > self.frames.len() {
            return crate::tools::error(&format!(
                "frame {frame} is not in this batch -- it runs 1..={}",
                self.frames.len()
            ));
        }
        let at = self.frames[frame - 1];
        let replaced = self
            .events
            .iter()
            .position(|event| event.frame == frame)
            .inspect(|_| {
                // Replacing is the documented behaviour, so say it happened: a model that recorded
                // twice and gets a plain ok has no reason to think its first words were dropped.
            });
        let text = flatten(text);
        let event = Event {
            frame,
            text: text.clone(),
            calm,
        };
        match replaced {
            Some(at_index) => self.events[at_index] = event,
            None => self.events.push(event),
        }
        crate::tools::ok(&json!({
            "frame": frame,
            "at": at,
            "replaced": replaced.is_some(),
            "text": text,
            "calm": calm,
        }))
    }

    /// `set_state(text)` — ok; empty text clears the state rather than leaving the old one.
    ///
    /// Clearing matters: a state that outlives what it describes is read forward through every
    /// later chunk and becomes the description of a scene nobody watched.
    pub fn set_state(&mut self, text: &str) -> String {
        let text = flatten(text);
        if text.trim().is_empty() {
            self.state = None;
            return crate::tools::ok(&json!({"state": null, "cleared": true}));
        }
        self.state = Some(text.clone());
        crate::tools::ok(&json!({"state": text}))
    }

    /// The running STATE, if one is set.
    pub fn state(&self) -> Option<&str> {
        self.state.as_deref()
    }

    /// What has been recorded, in the order it arrived.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// What has been recorded, ordered by frame — how `events.tsv` is written.
    pub fn events_sorted(&self) -> Vec<&Event> {
        let mut out: Vec<&Event> = self.events.iter().collect();
        out.sort_by_key(|event| event.frame);
        out
    }

    /// `speech_around(from, to)` — every line spoken in that stretch, from EVERY recording.
    ///
    /// Overlap, not containment: a sentence that starts before the window and runs into it is still
    /// spoken there, and dropping it loses the word the event was about. And no per-side cap — §3.2
    /// says outright this is not the brief's two-per-side extract, which cut a lecture's aside away
    /// from the slide change that prompted it.
    pub fn speech_around<'a>(
        &self,
        from: f64,
        to: f64,
        lines: impl IntoIterator<Item = &'a Spoken>,
    ) -> Vec<&'a Spoken> {
        let (from, to) = if from <= to { (from, to) } else { (to, from) };
        lines
            .into_iter()
            .filter(|line| line.start < to && line.end > from)
            .collect()
    }

    /// `finish` — the frames still without an event, plus the batch's first frame when it was left
    /// "same" with no history to be the same as.
    ///
    /// The second half is not tidiness: a batch whose first second says nothing has nothing to be
    /// resumed from, which is why the prototype force-wrote `Calm; same view.` there in silence.
    /// Here it comes back as a frame still to answer, so the model can say what the first frame is
    /// instead of having it written for it.
    pub fn finish(&self) -> Vec<Missing> {
        let answered: Vec<usize> = self.events.iter().map(|event| event.frame).collect();
        let mut missing: Vec<Missing> = self
            .frames
            .iter()
            .enumerate()
            .filter(|(index, _)| !answered.contains(&(index + 1)))
            .map(|(index, at)| Missing {
                frame: index + 1,
                at: *at,
                reason: if index == 0 && !self.has_history {
                    MissingReason::FirstFrameWithNoHistory
                } else {
                    MissingReason::NoEvent
                },
            })
            .collect();
        // A first frame that WAS answered with "same" and has no history is the same problem: the
        // word points at nothing. §3.2 asks for it either way, so it is named here too.
        if let Some(first) = self.frames.first() {
            let said_same = self
                .events
                .iter()
                .any(|event| event.frame == 1 && is_same(&event.text));
            if said_same && !self.has_history {
                missing.push(Missing {
                    frame: 1,
                    at: *first,
                    reason: MissingReason::FirstFrameWithNoHistory,
                });
            }
        }
        missing.sort_by_key(|missing| missing.frame);
        missing.dedup_by_key(|missing| missing.frame);
        missing
    }

    /// The line §3.2 says the prototype force-wrote for a batch's first frame with no history. Kept
    /// because a caller may still want to write it; [`Batch::finish`] is what asks instead.
    pub const SAME_WITHOUT_HISTORY: &'static str = "Calm; same view.";

    /// The interval between this batch's frames, which is the half-width an offset may be off by and
    /// still be snapped (see [`parse_event_line`]).
    pub fn interval(&self) -> f64 {
        self.interval
    }
}

/// One line of transcript from any recording, as `speech_around` sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Spoken {
    /// Which recording said it — the field the brief's extract threw away by taking two per side.
    pub source: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A frame `finish` is still asking about.
#[derive(Debug, Clone, PartialEq)]
pub struct Missing {
    pub frame: usize,
    pub at: f64,
    pub reason: MissingReason,
}

/// Why a frame is missing: an ordinary gap, or the batch's opening second with nothing before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingReason {
    NoEvent,
    /// The batch's first frame, left "same" with no history to be the same as.
    FirstFrameWithNoHistory,
}

/// What happened to one prose line — recorded, or dropped, and for which of three reasons.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseOutcome {
    /// The offset was within half an interval of a frame; that frame is what it meant. `at` is the
    /// frame's session second, so a caller can see the drift rather than only the landing.
    Snapped { frame: usize, at: f64, text: String },
    /// No label at all before the STATE line: filed as the description, which is what such a reply
    /// plainly was (six chunks in one recording did this in a row).
    UnlabeledDescription(String),
    /// One answer written across several lines, flattened to one.
    Flattened { frame: usize, at: f64, text: String },
    /// Gone: an offset that does not parse, or one further from every frame than half an interval.
    Dropped(DropReason),
}

/// Why a line was dropped. Both are silent to the model — §3.2 says so — but not to whoever is
/// reading why an event is missing from the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// The offset could not be read as a number.
    UnparseableOffset,
    /// Nothing within half an interval: closer to no frame than to any one of them.
    TooFarFromEveryFrame,
}

/// Where one prose line's offset landed on the batch.
#[derive(Debug, Clone, PartialEq)]
pub enum Placement {
    /// Some frame was within half an interval; `frame` is 1-based and `drift` how far off it was.
    Frame { frame: usize, at: f64, drift: f64 },
    /// No frame was close enough to be meant.
    TooFar(f64),
}

/// The frame an offset belongs to: the nearest one within half an interval of this batch's frame
/// grid, measured from the batch's first frame (which is what `EVENT [+2s]` counts from).
pub fn place_offset(offset: f64, interval: f64, frames: &[f64]) -> Placement {
    // At exactly half an interval the nearest frame is still unique, so the snap says something;
    // past it "nearest" stops being a fact and the line is dropped. The tolerance is one ulp of the
    // bound rather than a fixed epsilon because 0.5 s and 1.0 s are exact in binary while their
    // difference from 0.51 s is not, and a drift the spec drops must not snap because a bit went the
    // wrong way. One ulp cannot admit a real drift: the smallest interval this app extracts at is
    // 0.25 s, half of which is 125 ms, or 2^49 ulps wide.
    let base = frames.first().copied().unwrap_or_default();
    let half = interval / 2.0;
    let tolerance = (half * f64::EPSILON).max(f64::MIN_POSITIVE);
    let mut best: Option<(usize, f64, f64)> = None;
    for (index, at) in frames.iter().enumerate() {
        let drift = (offset - (at - base)).abs();
        let nearer = best.is_none_or(|(_, _, best)| drift < best);
        if drift <= half + tolerance && nearer {
            best = Some((index + 1, *at, drift));
        }
    }
    match best {
        Some((frame, at, drift)) => Placement::Frame { frame, at, drift },
        None => Placement::TooFar(offset),
    }
}

/// One line of the prototype's prose reply, cleaned up the way it cleaned up — with the reasons
/// named instead of swallowed.
///
/// `interval` is this batch's frame interval and `frames` its session seconds in order; an offset
/// within half an interval of a frame snaps to it, and further off is dropped without a word. The
/// bound is inclusive at exactly half: at that distance the nearest frame is still unique, so the
/// snap says something, while past it "nearest" stops being a fact.
///
/// Offsets are measured against the batch's own first frame, which is what `EVENT [+2s]` means in a
/// reply — seconds from where the batch starts, not absolute session time. The stamps the model was
/// shown (`[+0.0s]`, `[+1.0s]`, …) are the same arithmetic, so its number and this one land on the
/// same frame without either side converting.
///
/// Offsets are measured against the batch's own first frame, which is what `EVENT [+2s]` means in a
/// reply — seconds from where the batch starts, not absolute session time. The stamps the model was
/// shown (`[+0.0s]`, `[+1.0s]`, …) are the same arithmetic, so its number and this one land on the
/// same frame without either side converting.
pub fn parse_event_line(line: &str, interval: f64, frames: &[f64]) -> ParseOutcome {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return ParseOutcome::Dropped(DropReason::UnparseableOffset);
    }

    // The STATE half is taken off first: it belongs to the running state, and leaving it in would
    // file the state inside an event.
    let (event_part, state) = split_state(line);

    // A stamped line is "EVENT [+2s]: text"; anything else where a stamp should be is unparseable.
    let body = match event_part.find("EVENT") {
        Some(at) => &event_part[at + "EVENT".len()..],
        None => {
            // No label at all. The words before the STATE line are the description — that is what
            // they were, every time this happened.
            let text = flatten(event_part);
            if !text.is_empty() {
                return ParseOutcome::UnlabeledDescription(state.unwrap_or(text));
            }
            return ParseOutcome::Dropped(DropReason::UnparseableOffset);
        }
    };
    let Some((offset_text, text)) = bracketed_offset(body) else {
        // "EVENT" with no readable `[...]` after it: the same case as a bad number, and the reason
        // is the same — there is no moment to attach the words to.
        return ParseOutcome::Dropped(DropReason::UnparseableOffset);
    };
    let Ok(offset) = offset_text.trim().trim_end_matches('s').trim().parse::<f64>() else {
        return ParseOutcome::Dropped(DropReason::UnparseableOffset);
    };

    // Nearest frame, and only if it is close enough to be meant.
    let (frame, at) = match place_offset(offset, interval, frames) {
        Placement::Frame { frame, at, .. } => (frame, at),
        Placement::TooFar(_) => return ParseOutcome::Dropped(DropReason::TooFarFromEveryFrame),
    };

    // One answer written over several lines is one answer. The prototype flattened it; the flattening
    // stays because a two-line event reads as two events in the log the cut reads.
    let multi = text.contains('\n');
    let text = flatten(text);
    if multi {
        return ParseOutcome::Flattened { frame, at, text };
    }
    ParseOutcome::Snapped { frame, at, text }
}

/// The line as a prose reply wrote it: one line, single-spaced, trailing space gone. A newline in
/// the middle would be a second event in `events.tsv`, which is why this is flattening rather than
/// trimming.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split a reply at its `STATE:` label, returning the half before it and the state after.
fn split_state(reply: &str) -> (&str, Option<String>) {
    match reply.find("STATE:") {
        Some(at) => (
            &reply[..at],
            Some(flatten(&reply[at + "STATE:".len()..])),
        ),
        None => (reply, None),
    }
}

/// The `[...]` offset of an `EVENT [...] : text` line and the text after the colon.
fn bracketed_offset(body: &str) -> Option<(&str, &str)> {
    let start = body.find('[')?;
    let inner = &body[start + 1..];
    let end = inner.find(']')?;
    let offset = &inner[..end];
    let rest = inner[end + 1..].find(':').map(|colon| &inner[end + 1 + colon + 1..])?;
    Some((offset, rest))
}

/// A frame's line saying nothing changed. Same reading as the prototype: quotes and the trailing
/// full stop are decoration, "same" is the word.
pub fn is_same(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    let t = t.trim_matches(|c| c == '.' || c == '"' || c == '\'');
    t == "same" || t == "same view" || t.ends_with("; same") || t.ends_with("; same view")
}
