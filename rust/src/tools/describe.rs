//! Describe — spec/02-services.md §3.2, one chunk of frames at a time.

use serde_json::json;

pub const RECENT_EVENTS: usize = 3;

pub const CTX_SEGS: usize = 2;

/// …and how far back or forward each side looks for them. P.machine.describeCtxWindowSeconds
pub const CTX_WINDOW_SECONDS: f64 = 10.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    /// Session seconds of the frames in this batch, oldest first. The model numbers them 1..=n.
    frames: Vec<f64>,
    interval: f64,
    /// What has been recorded so far, in the order it arrived.
    events: Vec<Event>,
    /// The running STATE, if any.
    state: Option<String>,
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

    pub const SAME_WITHOUT_HISTORY: &'static str = "Calm; same view.";

    pub fn interval(&self) -> f64 {
        self.interval
    }
}

pub const SPEECH_HEADINGS: [&str; 3] = [
    "--- context before (do not describe) ---",
    "--- spoken during these frames ---",
    "--- context after (do not describe) ---",
];

/// What a section with nothing in it says, in the two places that use `(none)`.
pub const NO_SPEECH: &str = "(none)";

pub const NO_SPEECH_DURING: &str = "(no speech during these frames)";

pub fn speech_block(before: &[String], during: &[String], after: &[String]) -> String {
    let section = |heading: &str, lines: &[String], empty: &str| {
        let mut out = format!("{heading}\n");
        if lines.is_empty() {
            out.push_str(empty);
            out.push('\n');
        } else {
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    };
    let mut out = section(SPEECH_HEADINGS[0], before, NO_SPEECH);
    out.push_str(&section(SPEECH_HEADINGS[1], during, NO_SPEECH_DURING));
    out.push_str(&section(SPEECH_HEADINGS[2], after, NO_SPEECH));
    out
}

pub fn no_event_line(reply: &str) -> String {
    let text = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    let head: String = text.chars().take(120).collect();
    let head = if text.chars().count() > 120 {
        format!("{}…", head.trim_end())
    } else {
        head
    };
    format!("(no event line: {head})")
}

pub const OLD_SHAPE_FRAME: usize = 1;

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
    Snapped {
        frame: usize,
        at: f64,
        text: String,
    },
    UnlabeledDescription(String),
    /// One answer written across several lines, flattened to one.
    Flattened {
        frame: usize,
        at: f64,
        text: String,
    },
    /// Gone: an offset that does not parse, or one further from every frame than half an interval.
    Dropped(DropReason),
}

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

pub fn place_offset(offset: f64, interval: f64, frames: &[f64]) -> Placement {
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

/// named instead of swallowed.
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
    let Ok(offset) = offset_text
        .trim()
        .trim_end_matches('s')
        .trim()
        .parse::<f64>()
    else {
        return ParseOutcome::Dropped(DropReason::UnparseableOffset);
    };

    // Nearest frame, and only if it is close enough to be meant.
    let (frame, at) = match place_offset(offset, interval, frames) {
        Placement::Frame { frame, at, .. } => (frame, at),
        Placement::TooFar(_) => return ParseOutcome::Dropped(DropReason::TooFarFromEveryFrame),
    };

    let multi = text.contains('\n');
    let text = flatten(text);
    if multi {
        return ParseOutcome::Flattened { frame, at, text };
    }
    ParseOutcome::Snapped { frame, at, text }
}

fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split a reply at its `STATE:` label, returning the half before it and the state after.
fn split_state(reply: &str) -> (&str, Option<String>) {
    match reply.find("STATE:") {
        Some(at) => (&reply[..at], Some(flatten(&reply[at + "STATE:".len()..]))),
        None => (reply, None),
    }
}

/// The `[...]` offset of an `EVENT [...] : text` line and the text after the colon.
fn bracketed_offset(body: &str) -> Option<(&str, &str)> {
    let start = body.find('[')?;
    let inner = &body[start + 1..];
    let end = inner.find(']')?;
    let offset = &inner[..end];
    let rest = inner[end + 1..]
        .find(':')
        .map(|colon| &inner[end + 1 + colon + 1..])?;
    Some((offset, rest))
}

pub fn is_same(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    let t = t.trim_matches(|c| c == '.' || c == '"' || c == '\'');
    t == "same" || t == "same view" || t.ends_with("; same") || t.ends_with("; same view")
}
