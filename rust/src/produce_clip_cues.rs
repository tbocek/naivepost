//! F5.4 S1 — which clip contributes cues, and which lines those cues are built from.
//!
//! §F5.4's first diamond asks about the CLIP: narration lines if it has any, else its own speech, and only
//! where it is footage (never an insert, a freeze or a clip with no video). The rule lives in
//! [`crate::produce_subtitles::cues`]; this answers what kind of clip the render holds, from fields
//! [`crate::produce_render::plan_clips`] already filled (`source` is `seg.ins` verbatim).

use crate::narrate_screen::SPEECH_CHARS_PER_SECOND;
use crate::narration::Entry;
use crate::produce_render::{Clip, Placed};
use crate::produce_subtitles::{self as subs, Source};

/// What one clip is, for the cue question. Each case reads one field of [`Clip`]:
/// - `source` non-empty → an inserted sound or picture, or a pasted `copy:<s>` stretch ([`Source::Insert`]);
///   neither says what the session's words said.
/// - `on_screen <= 0.0` → nothing on screen to caption ([`Source::NoVideo`]). The render hands no such clip
///   today (a freeze holds a frame over running footage, §4); the arm stays because S1 names a freeze.
/// - otherwise → footage, whose own speech is worth reading.
pub fn source_of(clip: &Clip) -> Source {
    if !clip.source.is_empty() {
        return Source::Insert;
    }
    if clip.on_screen <= 0.0 {
        return Source::NoVideo;
    }
    Source::Footage
}

/// S1 (§F5.4): the clip's OWN aligned words, on the clip's recording clock, narrator mic excluded and
/// spellings taken from the fixed transcript (`word_list`). Asked only where the clip has no narration line
/// and is footage ([`crate::produce_clip_cues`]). An empty list yields no cues rather than a guess.
pub fn words_for(
    clip: &Clip,
    list: &[crate::word_list::Word],
    narrator_sources: &[String],
) -> Vec<subs::Word> {
    if source_of(clip) != Source::Footage {
        return Vec::new();
    }
    let start = clip.seg_s;
    let end = clip.seg_s
        + clip.on_screen
            * if clip.rate.abs() < f64::EPSILON {
                1.0
            } else {
                clip.rate
            };
    list.iter()
        .filter(|word| !narrator_sources.contains(&word.source))
        .filter(|word| word.end > start && word.start < end)
        .map(|word| subs::Word {
            text: word.written.clone(),
            s: (word.start - start).max(0.0),
            e: (word.end - start).min(end - start),
        })
        .collect()
}

/// S1: the cues one clip contributes, through the one door §F5.4 draws.
///
/// The branch order is `subs::cues`' own (lines BEFORE clip kind), so this supplies both halves and decides
/// nothing. Two things make it more than a pass-through:
/// - [`crate::produce_render::Placed`] carries seconds only, so a placed line's words and placement are looked
///   up in the narration record by `entry_index`; without that, `subs::cues` would read "no lines" for a clip
///   that has them and the raw recognised words would land over its own written narration.
/// - `Placed.at` is clip-relative OUTPUT time while `subs::Cue` works in the clip's recording seconds until
///   `on_clock` moves the track onto the video. A line with no take keeps a floor so `tidy` cannot fold it out.
///
/// `placed` is the render's per-line placement for this clip; `entries` the whole narration record indexed by
/// `Placed::entry_index`; `words` the clip's aligned words on its recording clock.
pub fn build(
    clip: &Clip,
    placed: &[Placed],
    entries: &[Entry],
    words: &[subs::Word],
) -> Vec<subs::Cue> {
    let scale = if clip.rate.abs() < f64::EPSILON {
        1.0
    } else {
        clip.rate
    };
    let lines: Vec<Entry> = placed
        .iter()
        .filter_map(|line| {
            // `Placed` carries seconds only; the words and the placement live on the narration entry.
            let source = entries.get(line.entry_index)?;
            // No take to time it, as with a caption: up for as long as it takes to read, inside the clip.
            let read = source.text.chars().count() as f64 / SPEECH_CHARS_PER_SECOND;
            let room = (clip.on_screen - line.at).max(subs::MIN_SECONDS);
            let shown = if line.speech > 0.0 {
                line.speech
            } else {
                read.min(room)
            };
            Some(Entry {
                s: line.at * scale,
                e: (line.at + shown.max(subs::MIN_SECONDS)) * scale,
                text: source.text.clone(),
                pos: source.pos.clone(),
                ..Default::default()
            })
        })
        .collect();
    subs::cues(source_of(clip), &lines, words, clip.rate)
}
