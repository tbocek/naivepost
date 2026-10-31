//! F4.7 — editing the lines: where a new one goes, how a moved one lands, what a delete leaves behind,
//! and what a typed box means.

use crate::cut::Seg;
use crate::narration::{Entry, Silent};
use crate::{narrate_pass, narrate_screen, preview, tools};

/// §F4.7 (row ＋): the gap after a row that has words — "previous audio's end + 0.5 s".
pub const BELOW_WORDED_SECONDS: f64 = 0.5;

/// §F4.7 (row ＋): the flat gap after a wordless row — "else flat +1.2 s". §10 gives this number no
/// `P.` id, so it lives with the rule that reads it rather than in a table nothing consults.
pub const BELOW_WORDLESS_SECONDS: f64 = 1.2;

pub fn below_gap(entry: &Entry) -> f64 {
    if entry.text.trim().is_empty() {
        BELOW_WORDLESS_SECONDS
    } else {
        BELOW_WORDED_SECONDS
    }
}

pub fn add_below(audio_end: f64, clip_end: f64, gap: f64) -> Result<f64, String> {
    let at = audio_end + gap;
    if at > clip_end - 1.0 {
        return Err("no room after this line \u{2014} the clip ends first".to_string());
    }
    Ok(at)
}

pub fn silent_marker_on(clip_s: f64, entries: &[Entry]) -> Option<usize> {
    let on_clip: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.s == clip_s)
        .map(|(index, _)| index)
        .collect();
    if on_clip.len() != 1 {
        return None;
    }
    let marker = &entries[on_clip[0]];
    let is_marker = marker.text.is_empty() && marker.pos.is_empty() && marker.emotion.is_empty();
    is_marker.then_some(on_clip[0])
}

/// Where a move landed: the new offset into the target clip, or the reason it did not move at all.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveResult {
    /// Adopted the target clip's bounds; `at` is the offset into it.
    Moved {
        at: f64,
    },
    Refused {
        said: String,
    },
}

pub fn move_line(entry: &Entry, target: &Seg, requested: f64) -> MoveResult {
    if !(target.s..target.e).contains(&requested) {
        return MoveResult::Refused {
            said: narrate_screen::time_refused(requested, entry.s, entry.e, entry.s + entry.at),
        };
    }
    // Never in the clip's last second: the clamp is the same one the `@N` tag obeys, taken through
    // `tag_moves_line` so the bound is stated once in this tree.
    let (clamped, _) = narrate_screen::tag_moves_line(target.s, target.e, requested, entry.at);
    MoveResult::Moved { at: clamped }
}

/// §F4.7 (time field): "moved this line to the clip at m:ss.s — it now starts at m:ss.s". Both
/// seconds are named because the person typed one number and needs to see which two the app used.
pub fn moved_said(target: &Seg, at: f64) -> String {
    format!(
        "moved this line to the clip at {} \u{2014} it now starts at {}",
        tools::mm_ss(target.s),
        preview::clock(Some(target.s + at))
    )
}

pub fn delete_leaves_clip_silent(entries: &[Entry], removed: &Entry) -> bool {
    !entries
        .iter()
        .any(|other| other.s == removed.s && other.at != removed.at)
}

/// What one typed text box means: the words to keep, the delivery to speak with, where a caption sits,
/// and the second a `[tag @N]` asked the line to move to.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedLine {
    pub words: String,
    pub emotion: String,
    pub pos: String,
    /// Seconds a `[tag @N]` asked the line to move to, if the tag carried one.
    pub moves_to: Option<f64>,
    pub kept_old_emotion: bool,
}

/// The three placements §1 lists for a caption, matched case-insensitively.
const PLACEMENTS: [&str; 3] = ["top", "center", "bottom"];

pub fn parsed_text(raw: &str, old_emotion: &str) -> ParsedLine {
    let mut parsed = ParsedLine::default();
    let mut rest = raw.trim_start();
    while let Some(after_open) = rest.strip_prefix('[') {
        let Some(close) = after_open.find(']') else {
            break;
        };
        let body = after_open[..close].trim();
        let tail = after_open[close + 1..].trim_start();
        let (name, moved) = match body.split_once('@') {
            Some((name, seconds)) => (name.trim(), parse_seconds(seconds.trim())),
            None => (body, None),
        };
        if name.is_empty() && moved.is_none() {
            break;
        }
        if PLACEMENTS.contains(&name.to_ascii_lowercase().as_str()) {
            parsed.pos = name.to_ascii_lowercase();
        } else if !name.is_empty() && narrate_pass::emotion_known(name) {
            parsed.emotion = name.to_string();
        } else if moved.is_none() {
            break;
        }
        if moved.is_some() {
            parsed.moves_to = moved;
        }
        rest = tail;
    }
    parsed.words = rest.trim().to_string();
    if parsed.words.is_empty() {
        parsed.emotion = old_emotion.to_string();
        parsed.kept_old_emotion = true;
    }
    parsed
}

/// A plain seconds value as typed inside a tag (`@12`, `@12.5`). Non-numeric is no move at all.
fn parse_seconds(text: &str) -> Option<f64> {
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>().ok()
}

pub fn reroll(roll: i32) -> i32 {
    roll + 1
}

pub fn clip_holding(segs: &[Seg], at: f64) -> Option<Seg> {
    segs.iter().find(|seg| seg.s <= at && at < seg.e).cloned()
}

pub fn parse_clock(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if !text.contains(':') {
        return text.parse::<f64>().ok();
    }
    let (minutes, seconds) = text.split_once(':')?;
    let minutes: f64 = minutes.trim().parse().ok()?;
    let seconds: f64 = seconds.trim().parse().ok()?;
    if !(0.0..60.0).contains(&seconds) || minutes < 0.0 {
        return None;
    }
    Some(minutes * 60.0 + seconds)
}

/// A clip the edit left with nothing to say, in the two forms the record needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmptiedClip {
    /// The clip's bounds on the session clock — what a status sentence names.
    pub s: f64,
    pub e: f64,
    /// Whether this clip is already remembered as deliberately silent in `narration.json`.
    pub already_marked: bool,
}

impl EmptiedClip {
    /// The marker for `narration.json`'s `silent` list.
    pub fn marker(&self) -> Silent {
        Silent {
            s: self.s,
            e: self.e,
        }
    }
}

/// §F4.7 (🗑 and the time field): which clip an edit emptied, and whether the record already knows.
pub fn emptied_clip(tree: &crate::layout::Tree, clip_s: f64, clip_e: f64) -> Option<EmptiedClip> {
    let record = crate::narration::load(tree).ok()?;
    if record
        .entries
        .iter()
        .any(|entry| entry.s == clip_s && entry.e == clip_e)
    {
        return None;
    }
    Some(EmptiedClip {
        s: clip_s,
        e: clip_e,
        already_marked: record
            .silent
            .iter()
            .any(|silent| silent.s == clip_s && silent.e == clip_e),
    })
}
