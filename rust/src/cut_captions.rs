//! F3.9 Captions proposed by the model (after the cut) — `spec/06-effects.md` F3.9, steps S1–S5.

use crate::cut::Fx;
use crate::fx_text;
use crate::tools::clips::Clips;

pub const BATCH: usize = 5;

pub const CLIPS_HEADER: &str = "THE CLIPS, AND WHAT WAS SAID OVER EACH:";

pub fn batches(count: usize) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut first = 1u32;
    while (first as usize) <= count {
        let last = (first as usize + BATCH - 1).min(count) as u32;
        ranges.push((first, last));
        first = last + 1;
    }
    ranges
}

pub const KEEP_SWEARING_DEFAULT: bool = true;

pub const CLEANING_RULE: &str = "Clean the words as a subtitler would: no ehm, no ehh, no stutters \
                                (\"I I\" is \"I\"), no repeated words, sentence case, the swearing kept.";

const SWEARING_CLAUSE: &str = ", the swearing kept";

/// What the model is told about cleaning the words.
pub fn cleaning_rule(keep_swearing: bool) -> String {
    if keep_swearing {
        return CLEANING_RULE.to_string();
    }
    CLEANING_RULE.replace(SWEARING_CLAUSE, "")
}

pub fn clip_line(n: u32, length: f64) -> String {
    format!("CLIP {n}: {length:.1} s long")
}

pub fn offset_line(offset: f64, text: &str) -> String {
    format!("[+{offset:.1}s] {text}")
}

pub fn message(user_context: &str, clips: &[(u32, f64)], words: &[(u32, f64, &str)]) -> String {
    let given: Vec<u32> = clips.iter().map(|(n, _)| *n).collect();
    let mut lines: Vec<String> = Vec::new();
    if !user_context.trim().is_empty() {
        lines.push(user_context.trim().to_string());
        lines.push(String::new());
    }
    lines.push(CLIPS_HEADER.to_string());
    for (n, length) in clips {
        lines.push(clip_line(*n, *length));
        for (clip, offset, text) in words.iter().filter(|(clip, _, _)| given.contains(clip)) {
            if clip == n {
                lines.push(offset_line(*offset, text));
            }
        }
    }
    lines.join("\n")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub clip: u32,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// Every caption that survived S5's floor, as session-relative records.
    Accepted(Vec<Fx>),
    /// The clip number that gave the game away — the whole reply is thrown away and this is said to it.
    Rejected(String),
}

pub fn clip_outside(clip: u32, first: u32, last: u32) -> Option<String> {
    if (first..=last).contains(&clip) {
        return None;
    }
    Some(format!(
        "clip {clip} is not one of the clips given ({first} to {last})"
    ))
}

pub fn retries(runs: u32) -> bool {
    runs == 1
}

/// F3.9 S4 + S5 (`yes → placed · fades min 0.3, d/4` / `no → skipped`): the reply turned into effects.
pub fn place(clips_in: &[(u32, f64)], calls: &[Call]) -> Reply {
    let (first, last) = match (clips_in.first(), clips_in.last()) {
        (Some((first, _)), Some((last, _))) => (*first, *last),
        // Nothing was given, so nothing can be right about it. Named as a range of noughts because that is what
        // the model would have to be told: there were no clips to caption.
        _ => return Reply::Rejected("no clips were given (0 to 0)".to_string()),
    };
    for call in calls {
        if let Some(reason) = clip_outside(call.clip, first, last) {
            return Reply::Rejected(reason);
        }
    }

    // The tool's own state: a batch of clips with the batch's lengths, so its clamping is real clamping. Each
    // clip's session second comes from [`session_start`] — the sum of what came before — because this signature
    // carries lengths only; a caller that knows the real positions reads them through [`on_the_timeline_at`].
    let mut batch = Clips::new(
        &clips_in
            .iter()
            .map(|(n, _)| (*n, session_start(clips_in, *n), length_of(clips_in, *n)))
            .collect::<Vec<_>>(),
    );
    let mut placed = Vec::new();
    for call in calls {
        let answer = batch.add_caption(call.clip, call.start, call.end, &call.text);
        if !is_ok(&answer) {
            // S5's two reasons are silence; anything else is the reply being wrong about the batch.
            if is_skippable(&answer) {
                continue;
            }
            return Reply::Rejected(answer);
        }
        let Some((span, text)) = placed_caption(&answer) else {
            return Reply::Rejected(answer);
        };
        placed.push(on_the_timeline(clips_in, call.clip, span, &text));
    }
    Reply::Accepted(placed)
}

fn length_of(clips_in: &[(u32, f64)], clip: u32) -> f64 {
    clips_in
        .iter()
        .find(|(n, _)| *n == clip)
        .map(|(_, length)| *length)
        .unwrap_or_default()
}

pub fn fade(dur: f64) -> f64 {
    fx_text::FADE_SECONDS.min(dur / 4.0)
}

pub fn on_the_timeline(clips_in: &[(u32, f64)], clip: u32, span: (f64, f64), text: &str) -> Fx {
    let dur = span.1 - span.0;
    Fx {
        kind: "text".into(),
        t: session_start(clips_in, clip) + span.0,
        dur,
        trans: fade(dur),
        tout: fade(dur),
        text: text.to_string(),
        ..Default::default()
    }
}

pub fn session_start(clips_in: &[(u32, f64)], clip: u32) -> f64 {
    let mut seconds = 0.0f64;
    for (_, length) in clips_in.iter().take_while(|(n, _)| *n != clip) {
        seconds += length;
    }
    seconds
}

pub fn on_the_timeline_at(start: f64, span: (f64, f64), text: &str) -> Fx {
    let dur = span.1 - span.0;
    Fx {
        kind: "text".into(),
        t: start + span.0,
        dur,
        trans: fade(dur),
        tout: fade(dur),
        text: text.to_string(),
        ..Default::default()
    }
}

pub fn skipped(first: u32, last: u32) -> String {
    format!("!!! captions: clips {first}\u{2013}{last} skipped -- the cut stands without them")
}

fn is_ok(answer: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(answer).is_ok_and(|body| body.get("error").is_none())
}

fn is_skippable(answer: &str) -> bool {
    let Ok(body) = serde_json::from_str::<serde_json::Value>(answer) else {
        return false;
    };
    let Some(reason) = body.get("error").and_then(|e| e.as_str()) else {
        return false;
    };
    reason.contains("dropped") || reason.contains("no words in it")
}

/// What the tool reported placing: the clamped span and the words it will draw, as `(span, text)`.
fn placed_caption(answer: &str) -> Option<((f64, f64), String)> {
    let body = serde_json::from_str::<serde_json::Value>(answer).ok()?;
    let span = body.get("span")?.as_array()?;
    let from = span.first()?.as_f64()?;
    let to = span.get(1)?.as_f64()?;
    Some(((from, to), body.get("text")?.as_str()?.to_string()))
}
