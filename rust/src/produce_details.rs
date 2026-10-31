//! §08-produce#6-details-confirmed-against-the-code-verification-pass — 08 §6, the verification pass.

use std::collections::BTreeMap;

use crate::cut_hear;
use crate::produce_embed as embed;
use crate::produce_render as render;
use crate::produce_screen as screen;
use crate::produce_stamp as stamp;
use crate::project::{self, Produce, Subtitles};
use crate::render_fx;
use crate::roles::Language;
use crate::tools;

pub const DRIFT_SECONDS: f64 = 0.05;

pub const BOUNDS_TOLERANCE_SECONDS: f64 = 0.05;

pub const DRAG_MIN_PX: f64 = 8.0;

pub const DESCRIPTION_HEADING_CHARS: usize = 40;

pub const LABELLED_LINES_MAX: usize = 3;

pub fn copy_in_no_recording_log(no: usize, at: f64) -> String {
    format!(
        "clip {no} copies footage at {} that falls in no recording \u{2014} skipped",
        tools::tenths(at)
    )
}

pub fn copy_past_end_log(no: usize, base: &str, seconds: f64) -> String {
    format!(
        "clip {no} copies past the end of {base} \u{2014} shortened to {}",
        tools::tenths(seconds)
    )
}

/// The sound-only half of the same problem: the picture exists, its recording does not.
pub fn sound_in_no_recording_log(no: usize, at: f64) -> String {
    format!(
        "clip {no} at {} falls in no recording \u{2014} its sound has no picture, skipped",
        tools::tenths(at)
    )
}

/// The sound-only half of running off the end.
pub fn sound_past_end_log(no: usize, base: &str, seconds: f64) -> String {
    format!(
        "clip {no} runs past the end of {base} \u{2014} shortened to {}",
        tools::tenths(seconds)
    )
}

pub fn insert_plays_silent_log(no: usize) -> String {
    format!("clip {no} keeps the sound under it \u{2014} it plays silent")
}

pub fn stop_in_no_recording_log(at: f64) -> String {
    format!(
        "a stop at {} falls in no recording \u{2014} its still is skipped",
        tools::tenths(at)
    )
}

pub fn no_entry_keeps_own_audio_log(no: usize, at: f64) -> String {
    format!(
        "clip {no} at {} has no narration entry \u{2014} it keeps its own audio",
        tools::tenths(at)
    )
}

pub fn captioned_only_log(no: usize) -> String {
    format!("clip {no}: no synthesis for a line \u{2014} it is captioned only")
}

pub fn fixed_rate_log(clip: &str, fps: f64) -> String {
    format!("{clip}: the moving camera needs a fixed frame rate \u{2014} this clip is {fps:.2} fps")
}

pub fn fixed_rate() -> f64 {
    render_fx::ZOOM_GRID_FPS
}

pub fn first_stream_is_own_sound(tracks: &[u32], stream: u32) -> bool {
    tracks.is_empty() || tracks.contains(&stream)
}

/// The same fact from a source record, so a caller never re-derives "first audio stream" as `0` at the call site.
pub fn own_sound_from_source(source: &project::Source) -> bool {
    first_stream_is_own_sound(&source.tracks, 0)
}

pub fn heard_via_lane_mix(tracks: &[u32], stream: u32) -> bool {
    !first_stream_is_own_sound(tracks, stream) && cut_hear::MAX_GAIN > 0.0
}

pub fn bounds_match(line_s: f64, line_e: f64, clip_s: f64, clip_e: f64) -> bool {
    (line_s - clip_s).abs() <= BOUNDS_TOLERANCE_SECONDS
        && (line_e - clip_e).abs() <= BOUNDS_TOLERANCE_SECONDS
}

pub fn drifts(planned: f64, actual: f64) -> bool {
    (planned - actual).abs() >= DRIFT_SECONDS
}

pub fn run_closes(source_is_footage: bool, on_recording: bool) -> bool {
    !source_is_footage || !on_recording
}

pub fn held_cue_end(line_start: f64, next_line_start: Option<f64>, clip_end: f64) -> f64 {
    let until = next_line_start.unwrap_or(clip_end);
    until.max(line_start).min(clip_end)
}

pub fn spoken_line_filters(
    clip: &render::Clip,
    settings: &Produce,
    _lanes: usize,
    _hush: &[(f64, f64)],
) -> Vec<String> {
    clip.lines
        .iter()
        .find(|line| line.speech > 0.0)
        .map(|line| render::voice_chain(line, settings))
        .unwrap_or_default()
}

pub const SCRATCH: &str = render::SCRATCH;

pub const SCRATCH_KINDS: [&str; 6] = [
    "c%03d.<container>",
    "c%03d_<stamp>.srt",
    "c%03d_t%02d.svg",
    "<card copy>.svg",
    "<name>.frames/",
    "joined.<container>",
];

pub fn clip_cues(clip_no: usize, cut_stamp: &str) -> String {
    format!("c{clip_no:03}_{cut_stamp}.srt")
}

pub fn cue_title(clip_no: usize, cue_no: usize) -> String {
    format!("c{clip_no:03}_t{cue_no:02}.svg")
}

pub fn card_copy(card_file: &str) -> String {
    let name = card_file.rsplit('/').next().unwrap_or(card_file);
    let name = name.strip_prefix("project:").unwrap_or(name);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{stem}-filled.svg")
}

/// A baked animation gets a folder of its own next to the file it was baked from.
pub fn animation_frames(name: &str) -> String {
    let clean = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{clean}.frames/")
}

/// The stream-copy join, before it is named `final.<container>` and moved out of the scratch folder.
pub fn joined(container: &str) -> String {
    format!("joined.{container}")
}

pub const UNDETERMINED_TAG: &str = "und";

pub fn language_tag(spoken: &str, list: &[Language]) -> (String, String) {
    // By code, whichever spelling arrived (`English` and `en` are one language), as `track_languages` reads it.
    let code = crate::roles::language_code(spoken);
    match list
        .iter()
        .find(|language| language.code.eq_ignore_ascii_case(&code))
    {
        Some(language) => (language.tag.clone(), language.name.clone()),
        None => (UNDETERMINED_TAG.to_string(), spoken.to_uppercase()),
    }
}

pub const WEBM_SRT_LOG: &str =
    "webm cannot carry an srt track \u{2014} the subtitles are the files beside the video";

pub fn stored_subtitle_mode(known: bool) -> Subtitles {
    if known {
        Subtitles::TrackInFile
    } else {
        Subtitles::None
    }
}

pub fn stored_resolution(known: bool) -> project::Resolution {
    if known {
        project::Resolution::P1080
    } else {
        Produce::default().resolution
    }
}

pub const DROPPED_PRODUCE_KEYS: [&str; 1] = ["subs_from"];

pub fn legacy_subtitles(stored: &str) -> Option<Subtitles> {
    match stored {
        "burned" => Some(Subtitles::BurnedIn),
        "track" => Some(Subtitles::TrackInFile),
        // The one answer with no stream behind it, and any word this app has never written.
        _ => None,
    }
}

pub fn legacy_game_volume(stored: Option<f64>) -> f64 {
    stored.unwrap_or_else(|| Produce::default().game_volume)
}

pub fn legacy_bare(blurred_edges: bool) -> bool {
    !blurred_edges
}

pub fn answer_number(line: &str) -> Option<(usize, &str)> {
    let digits = line
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    let rest = line.trim_start()[digits.len()..].trim_start();
    // The separator is a tab, space, dot or colon — and optional: `1 eins` and `1eins` differ only in whether a
    // digit follows, and a model that forgets the separator still means line 1.
    let value = match rest.strip_prefix(['.', ':']) {
        Some(value) => value.trim(),
        None if rest.starts_with(char::is_whitespace) => rest.trim(),
        None => rest,
    };
    if value.is_empty() {
        return None;
    }
    Some((digits.parse().ok()?, value))
}

pub fn read_answers(reply: &str) -> BTreeMap<usize, String> {
    let mut got = BTreeMap::new();
    for line in reply.lines() {
        if let Some((number, value)) = answer_number(line) {
            got.entry(number).or_insert_with(|| value.to_string());
        }
    }
    got
}

pub fn thumbnail_box_without_aspect() -> (u32, u32) {
    screen::thumb_box("", 0, 0)
}

pub fn frame_box_from_nothing(height: i32) -> (i32, i32) {
    let high = if height > 0 {
        height
    } else {
        render_fx::TIER_SHORT_SIDE
    };
    let wide = even_side(high * 16 / 9);
    (wide, even_side(high))
}

pub fn even_side(side: i32) -> i32 {
    side - side % 2
}

pub fn apply_legacy_base(frames: &[String], index: usize) -> Vec<String> {
    if index == 0 || index >= frames.len() {
        return frames.to_vec();
    }
    let mut out = vec![frames[index].clone()];
    out.extend(
        frames
            .iter()
            .enumerate()
            .filter(|(n, _)| *n != index)
            .map(|(_, frame)| frame.clone()),
    );
    out
}

pub fn title_off_reads_once(stored: Option<bool>, already_read: bool) -> Option<bool> {
    (!already_read).then_some(stored).flatten()
}

pub fn clear_frame_only_instruction(instruction: &str) -> String {
    let lines: Vec<&str> = instruction
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() == 1 && frame_line(lines[0]).is_some() {
        return String::new();
    }
    instruction.to_string()
}

pub fn publish_folder_wins(legacy_exists: bool, new_exists: bool) -> bool {
    !new_exists && legacy_exists
}

pub const DEADLOCK_LOG: &str = "    publish: no picture, no images and no instruction \u{2014} asking for the upload text again";

pub fn asks_for_text_again(has_picture: bool, images: usize, instruction: &str) -> bool {
    !has_picture && images == 0 && instruction.trim().is_empty()
}

pub fn chosen_frame_log(at_seconds: f64) -> String {
    format!(
        "    publish: the thumbnail is the frame at {}, as it is \u{2014} no model, no GPU",
        tools::mm_ss(at_seconds)
    )
}

pub fn chosen_frame_failed_log(why: &str) -> String {
    format!("    publish: {why} -- the thumbnail is drawn instead")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameAt {
    /// A clip's start plus an offset into it, in clip number then seconds.
    Clip(usize, f64),
    /// A moment on the produced clock.
    Seconds(f64),
}

pub fn frame_line(line: &str) -> Option<FrameAt> {
    let value = line.trim().strip_prefix("frame:")?.trim();
    if let Some(rest) = value.strip_prefix("clip") {
        let (number, offset) = rest.trim().split_once('+')?;
        let clip_no: usize = number.trim().parse().ok()?;
        let seconds = parse_seconds(offset.trim())?;
        // A negative sum is a frame before the clip starts; there is nothing to name.
        return (seconds >= 0.0).then_some(FrameAt::Clip(clip_no, seconds));
    }
    let seconds = parse_seconds(value)?;
    (seconds >= 0.0).then_some(FrameAt::Seconds(seconds))
}

pub fn frame_moment(named: FrameAt, starts: &[f64]) -> Option<f64> {
    match named {
        FrameAt::Seconds(at) => Some(at),
        FrameAt::Clip(number, offset) => starts
            .get(number.checked_sub(1)?)
            .map(|start| start + offset),
    }
}

fn parse_seconds(text: &str) -> Option<f64> {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, text.strip_prefix('+').unwrap_or(text)),
    };
    if let Some((minutes, seconds)) = rest.split_once(':') {
        let minutes: f64 = minutes.trim().parse().ok()?;
        let seconds: f64 = seconds.trim().parse().ok()?;
        return Some(sign * (minutes * 60.0 + seconds));
    }
    Some(sign * rest.trim().parse::<f64>().ok()?)
}

pub fn thumbnail_line_is_frame(line: &str) -> bool {
    match line.trim().strip_prefix("THUMBNAIL:") {
        Some(rest) => frame_line(rest.trim()).is_some(),
        None => false,
    }
}

pub fn peel_labels(reply: &str) -> (Vec<String>, String) {
    let text = strip_whole_fence(reply);
    let mut labels = Vec::new();
    let mut before = Vec::new();
    let mut after = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if labels.len() < LABELLED_LINES_MAX {
            if let Some(value) = labelled_value(trimmed) {
                labels.push(value);
                // What came before a labelled line is the model drafting aloud ("Let me correct the hashtag
                // list"), and the answer starts again here: only what follows the LAST label is the description.
                before.append(&mut after);
                continue;
            }
        }
        after.push(line.to_string());
    }
    // A reply that put its prose first and its labels last keeps that prose rather than none.
    let rest = if labels.is_empty() || after.iter().all(|l| l.trim().is_empty()) {
        before.into_iter().chain(after).collect::<Vec<String>>()
    } else {
        after
    };
    (labels, rest.join("\n"))
}

pub fn strip_whole_fence(reply: &str) -> String {
    let trimmed = reply.trim();
    if !trimmed.starts_with("```") || !trimmed.ends_with("```") {
        return reply.to_string();
    }
    let body = trimmed.trim_start_matches('`');
    let body = body.split('\n').skip(1).collect::<Vec<_>>().join("\n");
    match body.rsplit_once("```") {
        Some((head, _)) => head.to_string(),
        None => body,
    }
}

fn labelled_value(line: &str) -> Option<String> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_uppercase() || c == '_') || key.len() < 4 {
        return None;
    }
    let value = value.trim().trim_matches('"').trim_matches('\'').trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub fn drops_description_heading(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.ends_with(':') && trimmed.chars().count() < DESCRIPTION_HEADING_CHARS
}

pub fn candidate_frames(kept: &[(f64, f64)], extracted: &[f64]) -> Vec<f64> {
    let mut wanted: Vec<f64> = Vec::new();
    for (from, to) in kept {
        let span = (to - from).max(0.0);
        for band in 0..LABELLED_LINES_MAX {
            let middle =
                from + span * (2.0 * band as f64 + 1.0) / (2.0 * LABELLED_LINES_MAX as f64);
            if middle > *from && middle < *to {
                wanted.push(middle);
            }
        }
    }
    // Too few kept moments to choose from: hand back everything extracted rather than nothing, which is what
    // lets a first run of a short session still edit a real frame.
    if wanted.len() < LABELLED_LINES_MAX {
        return extracted.to_vec();
    }
    wanted
}

/// Is this extracted moment under an insert? Its picture comes from elsewhere, so it cannot stand for the video.
pub fn under_insert(at: f64, inserts: &[(f64, f64)]) -> bool {
    inserts.iter().any(|(from, to)| at >= *from && at < *to)
}

pub const NO_FRAMES_LOG: &str =
    "    publish: no frames extracted either \u{2014} drawing from the instruction alone";

pub fn base_gone_error(path: &str) -> String {
    format!("the base image is gone: {path}")
}

pub fn reference_skipped_log(path: &str) -> String {
    format!("    publish: {path} is not there \u{2014} sent without it")
}

pub fn request_log(
    wide: u32,
    high: u32,
    base: &str,
    images: usize,
    percent: u32,
    cx: f64,
    cy: f64,
) -> String {
    format!(
        "    publish: {wide}x{high} editing {base}, {images} image(s) sent, base cropped to {percent}% of its width around {cx:.2},{cy:.2}"
    )
}

/// …and the version with nothing to edit.
pub const REQUEST_NO_IMAGES_LOG: &str = "    publish: drawn from the instruction alone, no images";

pub fn already_drawn(both_files: bool, stamp_matches: bool) -> bool {
    both_files && stamp_matches
}

/// Printing failed: keep the plain picture. Losing the frame would turn a cosmetic failure into no thumbnail.
pub fn print_failed_keeps_plain(print_failed: bool) -> bool {
    print_failed
}

pub fn title_band_is_box(carries_words: bool) -> bool {
    carries_words
}

pub const REMOVE_IS_PERMANENT: &str =
    "Remove keeps it off for good; clearing the line only lets the next seed refill it";

pub fn youtube_title_redraws_picture() -> bool {
    false
}

pub const TEXT_CHIP_HINT: &str = "Printed to fill the box you marked \u{2014} a longer line comes out smaller, and Enter starts a new line.";

/// Does this press open the dialog? Only ever the chip.
pub fn opens_text_dialog(is_chip: bool) -> bool {
    is_chip
}

/// A drag under [`DRAG_MIN_PX`] marks nothing, so tapping inside a box you already drew does not erase it.
pub fn drag_marks(moved_px: f64) -> bool {
    moved_px >= DRAG_MIN_PX
}

pub fn box_saved_empty(existed: bool) -> BoxSave {
    if existed {
        BoxSave::Removed
    } else {
        BoxSave::NotCreated
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSave {
    NotCreated,
    Removed,
}

pub fn copies_plain_bytes(words: &str) -> bool {
    words.trim().is_empty()
}

/// The page's own file, beside the video — [`embed::paths`]' third answer.
pub fn page_path(tree: &crate::layout::Tree, container: project::Container) -> std::path::PathBuf {
    embed::paths(tree, container).2
}

pub fn stamp_unwritable_log(why: &str) -> String {
    format!("    produce: could not write the render stamp ({why}) \u{2014} the next \u{25b6} will encode again")
}

pub fn stamp_matches(computable: bool, stored: Option<&str>, current: &str) -> bool {
    computable && stamp::skip_encode(stored, current)
}

pub fn stamp_failure_fails_run() -> bool {
    false
}
