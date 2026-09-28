//! F5.4 S1 — which clip contributes cues, and which lines those cues are built from.
//!
//! §F5.4's first diamond asks about the CLIP: narration lines if it has any, else its own speech, and
//! that only where the clip is footage at all — never an insert, a freeze or a clip with no video. The
//! rule itself lives in [`crate::produce_subtitles::cues`]; this module answers the one question that
//! module cannot: what kind of clip the render is holding. It exists as its own file because
//! `produce_exec` builds the cue sheet and has no business classifying segments on the side.
//!
//! Everything here reads fields the render already filled in [`crate::produce_render::plan_clips`], so
//! there is no second opinion about what a segment is: `source` is `seg.ins` verbatim, and the same
//! `ins` spelling decides insert vs pasted stretch everywhere else in the render (`Seg::insert_asset`,
//! `Materials::source_file`).

use crate::narration::Entry;
use crate::produce_render::{Clip, Placed};
use crate::produce_subtitles::{self as subs, Source};

/// What one clip is, for the cue question. Each case reads one field of [`Clip`]:
/// - `source` non-empty → an inserted sound or picture, or a pasted `copy:<s>` stretch
///   ([`Source::Insert`]). Both say nothing the words of the session said: the asset is somebody
///   else's clip, and a pasted stretch was captioned where it was recorded.
/// - `on_screen <= 0.0` → nothing runs on screen to caption ([`Source::NoVideo`]). A freeze holds one
///   frame over running footage rather than being its own clip (§4), so the render hands no such clip
///   here today; the arm stays because S1 names a freeze explicitly and a later round may place one.
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

/// S1 (§F5.4): the clip's OWN aligned words, on the clip's own recording clock — the narrator mic
/// already excluded and the spellings already taken from the fixed transcript (`word_list`). Asked only
/// where the clip has no narration line of its own and is footage; [`crate::produce_clip_cues`] asks
/// that question, this answers where the words come from. An empty list means "this session has no
/// aligned words for this clip", which yields no cues rather than a guess.
pub fn words_for(
    clip: &Clip,
    list: &[crate::word_list::Word],
    narrator_sources: &[String],
) -> Vec<subs::Word> {
    if source_of(clip) != Source::Footage {
        return Vec::new();
    }
    // The clip's own seconds are cut-relative (the session clock), which is what the session word list
    // is timed on; `subs::Word` wants the same numbers, unshifted, because `on_clock` adds the clip's
    // place in the video afterwards.
    let start = clip.seg_s;
    let end = clip.seg_s + clip.on_screen * if clip.rate.abs() < f64::EPSILON { 1.0 } else { clip.rate };
    list.iter()
        .filter(|word| !narrator_sources.contains(&word.source))
        .filter(|word| word.end > start && word.start < end)
        .map(|word| subs::Word {
            text: word.written.clone(),
            s: word.start,
            e: word.end,
        })
        .collect()
}

/// S1: the cues one clip contributes, through the one door §F5.4 draws.
///
/// The branch order is `subs::cues`' own — it asks whether there are lines BEFORE it asks what the clip
/// is — so this function supplies both halves and adds no decision of its own. Two things make that more
/// than a pass-through:
/// - [`crate::produce_render::Placed`] carries seconds only (`fit` places timing, never text), so a
///   placed line's words and placement are looked up in the narration record by `entry_index`. Without
///   that lookup every cue would be empty, `subs::cues` would read "no lines" for a clip that has them,
///   and the clip's raw recognised words would land over its own written narration — the wrong-branch bug
///   this wiring is tested against.
/// - the seconds need converting back: `Placed.at` is clip-relative OUTPUT time and `subs::Cue` works in
///   the clip's own recording seconds until `on_clock` moves the track onto the video. A line with no
///   take keeps the half-second floor so `tidy` cannot fold a silent caption out of the sheet.
///
/// `placed` is the render's per-line placement for this clip; `entries` is the whole narration record,
/// indexed by `Placed::entry_index`; `words` are the clip's aligned words on its own recording clock,
/// narrator mic excluded and respelled from the fixed transcript.
pub fn build(
    clip: &Clip,
    placed: &[Placed],
    entries: &[Entry],
    words: &[subs::Word],
) -> Vec<subs::Cue> {
    let scale = if clip.rate.abs() < f64::EPSILON { 1.0 } else { clip.rate };
    let lines: Vec<Entry> = placed
        .iter()
        .filter_map(|line| {
            // `Placed` carries seconds only; the words and the placement live on the narration entry.
            let source = entries.get(line.entry_index)?;
            Some(Entry {
                s: line.at * scale,
                e: (line.at + line.speech.max(subs::MIN_SECONDS)) * scale,
                text: source.text.clone(),
                pos: source.pos.clone(),
                ..Default::default()
            })
        })
        .collect();
    subs::cues(source_of(clip), &lines, words, clip.rate)
}
