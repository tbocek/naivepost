//! F4.7 — editing the lines: where a new one goes, how a moved one lands, what a delete leaves behind,
//! and what a typed box means.
//!
//! Every rule here is pure: it takes the entries, the clips and what was typed, and returns either the
//! second to use or the sentence to say. The page (`crate::ui::narrate_page`) holds the widgets and
//! the status bar; nothing in this module touches GTK, so each branch below is reachable from a test
//! without a window.
//!
//! What already existed elsewhere is reused rather than restated: the clock faces come from
//! [`crate::preview::clock`] / [`crate::tools::mm_ss`], the last-second bound from
//! [`crate::narrate_screen::tag_moves_line`], the emotion vocabulary from
//! [`crate::narrate_pass::emotion_known`], and the refusal sentence from
//! [`crate::narrate_screen::time_refused`].

use crate::cut::Seg;
use crate::narration::{Entry, Silent};
use crate::{narrate_pass, narrate_screen, preview, tools};

/// §F4.7 (row ＋): the gap after a row that has words — "previous audio's end + 0.5 s".
pub const BELOW_WORDED_SECONDS: f64 = 0.5;

/// §F4.7 (row ＋): the flat gap after a wordless row — "else flat +1.2 s". §10 gives this number no
/// `P.` id, so it lives with the rule that reads it rather than in a table nothing consults.
pub const BELOW_WORDLESS_SECONDS: f64 = 1.2;

/// Which gap a row asks for: a row with words waits half a second after its own audio, a wordless row
/// gets the flat 1.2 s because there is no audio end to measure from. One function for both cases
/// because the row decides which it needs; the caller never picks the constant itself.
pub fn below_gap(entry: &Entry) -> f64 {
    if entry.text.trim().is_empty() {
        BELOW_WORDLESS_SECONDS
    } else {
        BELOW_WORDED_SECONDS
    }
}

/// §F4.7 (row ＋): place the next line `gap` after the previous audio ended, refused when the clip
/// ends first — the same last-second bound as [`narrate_screen::add_below`], since a line starting in
/// the clip's final second has nothing left to be heard over.
pub fn add_below(audio_end: f64, clip_end: f64, gap: f64) -> Result<f64, String> {
    let at = audio_end + gap;
    if at > clip_end - 1.0 {
        return Err("no room after this line \u{2014} the clip ends first".to_string());
    }
    Ok(at)
}

/// §F4.7 (transport ＋): the index of the clip's ONLY entry when that entry is the empty
/// "deliberately silent" marker — no words, no placement, no delivery. That marker moves to the
/// playhead and becomes the line, rather than a second row being added beside it. Anything else on
/// the clip (a spoken line, a caption, or more than one entry) answers `None`.
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
    Moved { at: f64 },
    /// The typed second falls outside the cut: keep the old position, say so.
    Refused { said: String },
}

/// §F4.7 (time field): move a line into another clip. The typed second has to fall inside the target;
/// anywhere else is a gap and comes back refused with [`narrate_screen::time_refused`]'s sentence,
/// which names the typed second, the clip's bounds and where the line stayed — so the write-back reads
/// as an answer rather than as a silently reverted box.
pub fn move_line(entry: &Entry, target: &Seg, requested: f64) -> MoveResult {
    if !(target.s..target.e).contains(&requested) {
        return MoveResult::Refused {
            said: narrate_screen::time_refused(requested, entry.s, entry.e, entry.s + entry.at),
        };
    }
    // Never in the clip's last second: the clamp is the same one the `@N` tag obeys, taken through
    // `tag_moves_line` so the bound is stated once in this tree.
    let offset = requested - target.s;
    let (clamped, _) = narrate_screen::tag_moves_line(target.s, target.e, offset, entry.at);
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

/// §F4.7 (🗑): the clip is remembered as deliberately silent only when the removed entry was its LAST
/// line. With a sibling still on the clip the clip still speaks, and marking it silent would mute a line
/// that is sitting right there.
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
    /// True when the box was emptied and the old emotion is what survives.
    pub kept_old_emotion: bool,
}

/// The three placements §1 lists for a caption, matched case-insensitively.
const PLACEMENTS: [&str; 3] = ["top", "center", "bottom"];

/// §F4.7 (**Text**): `[tag] words`; a placement tag clears the emotion; emptying a box keeps the old
/// emotion. Leading `[...]` tags are read off the front, in order, until one is not a tag:
/// - a placement word sets `pos` and leaves `emotion` EMPTY — the placement tag CLEARS the emotion,
///   because a caption is read and not spoken, so a delivery left behind would describe a voice that
///   will not be used;
/// - anything else the voice vocabulary knows becomes the emotion;
/// - an unknown tag stays as TYPED TEXT rather than refusing the edit: a person mid-sentence who types
///   `[wait for it]` should not lose their words to a validator.
///
/// `@N` inside a tag carries the line's own second and sets `moves_to`; the clamp out of the clip's
/// last second is the call site's job, through [`narrate_screen::tag_moves_line`], because it needs
/// the clip the line is on and this function does not have it.
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
            // `[]` says nothing: leave it as text rather than eating the person's characters.
            break;
        }
        if PLACEMENTS.contains(&name.to_ascii_lowercase().as_str()) {
            parsed.pos = name.to_ascii_lowercase();
        } else if !name.is_empty() && narrate_pass::emotion_known(name) {
            parsed.emotion = name.to_string();
        } else if moved.is_none() {
            // Unknown and not a move: un-consume it so the tag stays visible in the words.
            break;
        }
        if moved.is_some() {
            parsed.moves_to = moved;
        }
        rest = tail;
    }
    parsed.words = rest.trim().to_string();
    // §F4.7: "emptying a box keeps the old emotion". The placement survives too: turning a caption
    // back into speech (or the reverse) by deleting the words would change what the render burns into
    // the picture from an edit that said nothing about placement.
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

/// §F4.7 (↻): "roll + 1, a new take spoken; the old wav stays". The previous file is never removed —
/// [`crate::produce_stamp::wav_of`] keys the take on the roll, so the old one is exactly what an undo
/// reads back, and deleting it here would make a re-roll irreversible.
pub fn reroll(roll: i32) -> i32 {
    roll + 1
}

/// The clip whose span holds this second, if any — a gap between clips answers `None`, which is the
/// case [`move_line`] refuses. Kept here so the page's move seam has one way to name "the clip under
/// this second" and does not re-derive it at each call.
pub fn clip_holding(segs: &[Seg], at: f64) -> Option<Seg> {
    segs.iter().find(|seg| seg.s <= at && at < seg.e).cloned()
}

/// Read one field value back off the clock face the time field prints (`m:ss.s`, or a bare number of
/// seconds). A widget-format concern, so it lives with the rule module rather than being restated in
/// the page: the rules themselves work in seconds and never see text.
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
        Silent { s: self.s, e: self.e }
    }
}

/// §F4.7 (🗑 and the time field): which clip an edit emptied, and whether the record already knows.
/// `None` when other lines still sit on the clip — it still speaks, so marking it silent would mute a
/// line that is sitting right there.
///
/// The page cannot answer "already marked" itself: its `NarrateState` carries entries, not the file's
/// `silent` list, so a delete after a delete would otherwise write the same clip twice. This reads the
/// record through [`crate::narration::load`] — the file's own reader, not a second format parser.
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
