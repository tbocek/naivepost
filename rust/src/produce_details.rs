//! §08-produce#6-details-confirmed-against-the-code-verification-pass — 08 §6, the verification pass.
//!
//! §6 is not a flow: it is the list of details that were checked line by line against the Go prototype
//! (`gui/produce.go`, `gui/publish.go`, `gui/subs.go`) and found to be load-bearing. Most of them already live
//! in the module whose flow owns them — [`crate::produce_render`], [`crate::render_fx`],
//! [`crate::produce_subtitles`], [`crate::produce_embed`], [`crate::produce_stamp`] — and this module does not
//! restate those: it links to them, or wraps one call so §6's rule has one address.
//!
//! What lives here is what no single flow owns: the refusals a planner makes while deciding what to encode, the
//! scratch names a render leaves behind between its own two passes, the ISO-639-2 answer for a language nobody
//! offered, the legacy `produce` keys a project still arrives with, and the publish rules that outlived the
//! prototype's single-file layout. Plain data and pure functions only — no window, no subprocess.
//!
//! Ids used: `P.policy.gameVolume`, `P.machine.thumbnailLongSide`, `P.policy.publishFrames`,
//! `P.eng.publishMaxFrames`, `tool:ffmpeg.encode`.

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

/// §6's own drift tolerance, in seconds: sound is only worth re-opening the read head for when it would land
/// audibly out of step, and 50 ms is where a person stops hearing an echo and starts hearing two sounds.
/// `machine.`-prefixed because §10 gives no `P.` row — nothing outside this module asks the question, so no
/// section has a reason to name it.
pub const DRIFT_SECONDS: f64 = 0.05;

/// A narration line with no session span (a card, a held frame) belongs to the clip whose bounds it was written
/// against, not to whichever clip it overlaps by half — there is no overlap to speak of when both are points.
/// The same tolerance as [`DRIFT_SECONDS`], and for the same reason: these bounds were typed or snapped, so a
/// rounding difference is not a different clip. `machine.`-prefixed, §10 has no row for it either.
pub const BOUNDS_TOLERANCE_SECONDS: f64 = 0.05;

/// The smallest drag that marks a box (`preview.` — §10 gives no `P.` id, and this is the preview's own
/// allowance). Under it a press reads as a click on the row behind the box, not as an attempt to mark one: the
/// prototype refused at 8 px and people relied on being able to tap inside a box they had just drawn.
pub const DRAG_MIN_PX: f64 = 8.0;

/// A first description line shorter than this *and* ending in ":" is a heading the model left behind, not a
/// sentence for the video's page (`machine.` — §10 has no row; only the publish reply parser reads it).
pub const DESCRIPTION_HEADING_CHARS: usize = 40;

/// How many labelled lines peel off a thumbnail instruction before what is left becomes the picture's own
/// words. Three because that is all the prototype ever peeled and a fourth label means the model answered with
/// a form, not a sentence (`machine.`; §10's `P.policy.publishFrames` counts images, not labels).
pub const LABELLED_LINES_MAX: usize = 3;

// ---- S1: clip planning refusals ------------------------------------------------------
//
// Each is one line of the render log and each names its own clip, because a person reads these after the video
// came out shorter than the cut and has to find the moment in the timeline. Wording is the prototype's.

/// "clip N copies footage at T s that falls in no recording — skipped": a pasted stretch points at session
/// seconds, and session seconds with no file behind them cannot be decoded.
pub fn copy_in_no_recording_log(no: usize, at: f64) -> String {
    format!("clip {no} copies footage at {} that falls in no recording \u{2014} skipped", tools::tenths(at))
}

/// "clip N copies past the end of <base> — shortened to X s": the paste runs off the file it belongs to, so the
/// clip keeps what the recording can still give it rather than failing.
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

/// An insert that keeps the sound under it but has no recording, or carries none of its own: the picture plays,
/// silently. Not an error — the person put a card there and may have meant it.
pub fn insert_plays_silent_log(no: usize) -> String {
    format!("clip {no} keeps the sound under it \u{2014} it plays silent")
}

/// "a stop at T s falls in no recording — its still is skipped": a freeze needs one frame to hold, and with no
/// file there is no frame to take.
pub fn stop_in_no_recording_log(at: f64) -> String {
    format!("a stop at {} falls in no recording \u{2014} its still is skipped", tools::tenths(at))
}

/// A clip with no line over it is not a failure; the log says so because "why does this clip still talk" is the
/// next question, and its answer is that nothing was ever written for it.
pub fn no_entry_keeps_own_audio_log(no: usize, at: f64) -> String {
    format!("clip {no} at {} has no narration entry \u{2014} it keeps its own audio", tools::tenths(at))
}

/// A line nobody synthesised still gets read by whoever asked for captions: the cue is real, the voice is not.
/// [`render::captions_only_warning`] answers the page-wide version of this; this is the per-line log line.
pub fn captioned_only_log(no: usize) -> String {
    format!("clip {no}: no synthesis for a line \u{2014} it is captioned only")
}

/// The camera's own refusal, with the rate the clip is asked to run at: `zoompan` samples a fixed grid, so a
/// variable-rate recording has to be told what a frame is before it can be magnified.
pub fn fixed_rate_log(clip: &str, fps: f64) -> String {
    format!("{clip}: the moving camera needs a fixed frame rate \u{2014} this clip is {fps:.2} fps")
}

/// The grid [`render_fx`]'s `zoompan` chain asks for, and what the log above reports when a recording's own rate
/// differs from it. Lives in [`render_fx::ZOOM_GRID_FPS`]; named here so §6's line has one source.
pub fn fixed_rate() -> f64 {
    render_fx::ZOOM_GRID_FPS
}

// ---- S1: the rules behind those lines ------------------------------------------------

/// A clip uses its recording's first audio stream as its own sound only when that stream is among the source's
/// selected `tracks`; otherwise the person switched it off and the clip is heard through the lane mix. An empty
/// list means nothing was switched off, which is how a project written before the row existed reads.
pub fn first_stream_is_own_sound(tracks: &[u32], stream: u32) -> bool {
    tracks.is_empty() || tracks.contains(&stream)
}

/// The same fact from a source record, so a caller never re-derives "first audio stream" as `0` at the call site.
pub fn own_sound_from_source(source: &project::Source) -> bool {
    first_stream_is_own_sound(&source.tracks, 0)
}

/// Where the sound comes from instead when [`first_stream_is_own_sound`] says no: the lane mix, which is what
/// [`crate::cut_hear`] already decides for a clip whose own audio is off.
pub fn heard_via_lane_mix(tracks: &[u32], stream: u32) -> bool {
    !first_stream_is_own_sound(tracks, stream) && cut_hear::MAX_GAIN > 0.0
}

/// A line with no session span matched by exact bounds within [`BOUNDS_TOLERANCE_SECONDS`]. The half-overlap
/// rule in [`render::matches`] cannot answer this: a card and its line are both points, and half of nothing is
/// nothing.
pub fn bounds_match(line_s: f64, line_e: f64, clip_s: f64, clip_e: f64) -> bool {
    (line_s - clip_s).abs() <= BOUNDS_TOLERANCE_SECONDS && (line_e - clip_e).abs() <= BOUNDS_TOLERANCE_SECONDS
}

/// The planned 1× read head opens only where the sound would drift by at least [`DRIFT_SECONDS`]; below that the
/// splice is inside one frame of what the person heard while cutting, and opening the head would dip audio that
/// never needed dipping. The run itself comes from [`render::read_head_plan`] and [`render_fx::read_head`].
pub fn drifts(planned: f64, actual: f64) -> bool {
    (planned - actual).abs() >= DRIFT_SECONDS
}

/// A card, a held frame, or a clip sitting on no recording closes the run: there is no continuous recording
/// under the sound to keep reading from. `source` is the cut's own spelling — `""` for footage.
pub fn run_closes(source_is_footage: bool, on_recording: bool) -> bool {
    !source_is_footage || !on_recording
}

/// The clip-relative moment a line arrives at when it has no synthesis and may not overrun the picture: its own
/// start, held to the next line's start or the clip's end — whichever comes first. Nothing is deleted, so the
/// cue survives with the voice missing ([`captioned_only_log`]).
pub fn held_cue_end(line_start: f64, next_line_start: Option<f64>, clip_end: f64) -> f64 {
    let until = next_line_start.unwrap_or(clip_end);
    until.max(line_start).min(clip_end)
}

/// S1's last rule as one call: what the render does to a spoken line. Time-stretch, resample, pan across the
/// output layout (a mono voice otherwise lands in one speaker), delay in whole milliseconds — every filter of it
/// is [`crate::produce_render`]'s audio chain and [`render_fx`]'s tempo chains; this names the four steps so §6's
/// sentence has a single test target. `hush` is the clip's own volume cues, in output seconds.
pub fn spoken_line_filters(clip: &render::Clip, settings: &Produce, lanes: usize, hush: &[(f64, f64)]) -> Vec<String> {
    render::audio_chain(clip, settings, lanes, hush)
}

// ---- S3: what produce/clips/ also holds ----------------------------------------------

/// The scratch folder's own name — [`crate::produce_data::SCRATCH`] / [`render::SCRATCH`], named here so §6's
/// list and §3's group have one spelling.
pub const SCRATCH: &str = render::SCRATCH;

/// What `produce/clips/` holds between the two ffmpeg passes: per-clip encodes, the burn cue files, one
/// generated title document per text cue, a written copy of any parameterised static card, a `.frames/` folder
/// per baked animation, and the stream-copy join. Every one is cleared at the start of every render —
/// [`render::clear_scratch`], which §5 makes the reason a rerun never reads its predecessor's leftovers.
pub const SCRATCH_KINDS: [&str; 6] = [
    "c%03d.<container>",
    "c%03d_<stamp>.srt",
    "c%03d_t%02d.svg",
    "<card copy>.svg",
    "<name>.frames/",
    "joined.<container>",
];

/// A clip's own burn cue file: the numbered clip plus the stamp of the cut it was built from, so a cue left by
/// an older render cannot be picked up by a newer clip with the same number.
pub fn clip_cues(clip_no: usize, cut_stamp: &str) -> String {
    format!("c{clip_no:03}_{cut_stamp}.srt")
}

/// One generated title document per text cue, numbered within its clip — an SVG because a text effect is drawn,
/// not typed, and the drawing has to survive being looped as an input.
pub fn cue_title(clip_no: usize, cue_no: usize) -> String {
    format!("c{clip_no:03}_t{cue_no:02}.svg")
}

/// A parameterised static card is written out with its query resolved, so ffmpeg reads a plain file and the
/// card's inputs are not re-parsed per frame. The stem keeps the card's own name.
pub fn card_copy(card_file: &str) -> String {
    let name = card_file.rsplit('/').next().unwrap_or(card_file);
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

/// §6 (`cleared at the start of every render`): the sweep is [`render::clear_scratch`] and it deletes files, not
/// the folder — a caller that removed the folder would break the paths the planner already handed ffmpeg.
pub fn clear(dir: &std::path::Path) -> Result<usize, String> {
    render::clear_scratch(dir)
}

// ---- S4: muxing the tracks -----------------------------------------------------------

/// §6's fallback language: a spoken language nobody offered in `SUBTITLE_LANGUAGES` has no ISO-639-2 tag to
/// carry, and `und` is what a player reads as "the file did not say". Its name then has to be the code itself,
/// upper-cased, because `de` alone in a menu reads like a mistake while `DE` reads like a label.
pub const UNDETERMINED_TAG: &str = "und";

/// The two strings one subtitle input is tagged with, so a player can name the track. A language in the list
/// answers with its own tag and name; anything else falls back as [`UNDETERMINED_TAG`] spells.
pub fn language_tag(spoken: &str, list: &[Language]) -> (String, String) {
    match list.iter().find(|language| language.code == spoken) {
        Some(language) => (language.tag.clone(), language.name.clone()),
        None => (UNDETERMINED_TAG.to_string(), spoken.to_uppercase()),
    }
}

/// The mux line itself is [`render::mux_command`]'s — one input per track, each mapped to its own subtitle
/// stream and tagged `language=`/`name=`. Named here so §6's sentence points at the command rather than at a
/// second copy of it. (`tool:ffmpeg.encode`.)
pub fn mux(video: &std::path::Path, audio: &std::path::Path, tracks: &[(&str, &std::path::Path)], settings: &Produce, out: &std::path::Path) -> Vec<String> {
    render::mux_command(video, audio, tracks, settings, out)
}

/// After a webm mux. The container refuses an `srt` stream outright, so the tracks are not lost — they were
/// always written beside the video, and this is the log line that says where to look.
pub const WEBM_SRT_LOG: &str = "webm cannot carry an srt track \u{2014} the subtitles are the files beside the video";

/// An unknown stored subtitle mode reads as *none in the video*. Guessing "burned in" would letter somebody's
/// picture on a run they pressed for something else entirely, and the row's own third choice is the one that
/// promises nothing.
pub fn stored_subtitle_mode(known: bool) -> Subtitles {
    if known { Subtitles::TrackInFile } else { Subtitles::None }
}

/// A stored resolution the page no longer offers falls back to the row's default rather than being passed to
/// ffmpeg as a word it has to fail on. The default is `Produce`'s own, so the two cannot drift apart.
pub fn stored_resolution(known: bool) -> project::Resolution {
    if known { project::Resolution::P1080 } else { Produce::default().resolution }
}

// ---- S5: legacy `produce` keys -------------------------------------------------------

/// §6's list of keys a project still arrives with that today's settings have no answer for. Dropped, not
/// migrated: the value they carried is recomputed from what the file still says.
pub const DROPPED_PRODUCE_KEYS: [&str; 1] = ["subs_from"];

/// `subs: "sidecar"` was the old spelling of "nothing in the video, the files beside it do the work" — which is
/// exactly what today's "none in the video" means, since an `.srt` is written beside the video whatever the row
/// says ([`render::sidecar_names`]).
pub fn legacy_subtitles(stored: &str) -> Option<Subtitles> {
    match stored {
        "burned" => Some(Subtitles::BurnedIn),
        "track" => Some(Subtitles::TrackInFile),
        // The one answer with no stream behind it, and any word this app has never written.
        _ => None,
    }
}

/// Absent `game_vol` is the default bed ([`Produce::default`]'s `game_volume`, `P.policy.gameVolume`); a stored
/// `0` is silence. The two must not collapse: JSON has no "absent" for a plain `f64`, so the caller passes what
/// it read and this decides, which is why the input is an `Option`.
pub fn legacy_game_volume(stored: Option<f64>) -> f64 {
    stored.unwrap_or_else(|| Produce::default().game_volume)
}

/// `bare` was "no blurred edges" before the tick was inverted. It reads once, as the negation of what the tick
/// says today, and then retires with the file.
pub fn legacy_bare(blurred_edges: bool) -> bool {
    !blurred_edges
}

/// One numbered answer line, split the way a model writes it: a tab, space, dot or colon after the number. The
/// whole rest of the line is the translation, dots inside it included — splitting on the first `.` would eat
/// "3. drei" and turn an abbreviation into a lost line.
pub fn answer_number(line: &str) -> Option<(usize, &str)> {
    let digits = line.trim_start().chars().take_while(char::is_ascii_digit).collect::<String>();
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

/// A reply's answers, keyed by number. The first answer for a number wins — a model that repeats one has
/// already answered it once, and the second version is usually the one it drifted into. An unreadable line is
/// skipped, never shifted onto the next number.
pub fn read_answers(reply: &str) -> BTreeMap<usize, String> {
    let mut got = BTreeMap::new();
    for line in reply.lines() {
        if let Some((number, value)) = answer_number(line) {
            got.entry(number).or_insert_with(|| value.to_string());
        }
    }
    got
}

// ---- S7: frame boxes -----------------------------------------------------------------

/// No aspect means the thumbnail is 16:9 at long side `P.machine.thumbnailLongSide`. Both halves are
/// [`screen::thumb_box`]'s already; this is §6's sentence about it in one call, so a test can pin the fallback
/// without inventing a picture to measure.
pub fn thumbnail_box_without_aspect() -> (u32, u32) {
    screen::thumb_box("", 0, 0)
}

/// A frame box derivable from nothing is the chosen height at 16:9 — and 1080 when even that is unset, which is
/// [`render_fx::TIER_SHORT_SIDE`], the same edge [`render_fx::frame_box`] names a tier by.
pub fn frame_box_from_nothing(height: i32) -> (i32, i32) {
    let high = if height > 0 { height } else { render_fx::TIER_SHORT_SIDE };
    let wide = even_side(high * 16 / 9);
    (wide, even_side(high))
}

/// Both sides are always even: every encoder this page offers is 4:2:0, which refuses a chroma sample sitting on
/// an odd edge. Rounded down for the same reason [`screen::thumb_box`] rounds down — rounding up would push a
/// side past the tier it was chosen for.
pub fn even_side(side: i32) -> i32 {
    side - side % 2
}

// ---- S6: publish ---------------------------------------------------------------------

/// The old base-by-index answer becomes the frame list's order: that frame moves to the front, because the first
/// frame is what the image model edits. An index naming no *other* frame moves nothing — including `0`, which
/// already names the front ([`project`] applies this on load; it is `pub(crate)` there, so §6's rule is stated
/// here once more rather than reached through a widened API).
pub fn apply_legacy_base(frames: &[String], index: usize) -> Vec<String> {
    if index == 0 || index >= frames.len() {
        return frames.to_vec();
    }
    let mut out = vec![frames[index].clone()];
    out.extend(frames.iter().enumerate().filter(|(n, _)| *n != index).map(|(_, frame)| frame.clone()));
    out
}

/// `title_off` is read once, on load, and never again: it was the old spelling of "the picture carries no words",
/// which today is a checkbox a person can change. Reading it twice would let a retired key overrule an edit.
pub fn title_off_reads_once(stored: Option<bool>, already_read: bool) -> Option<bool> {
    (!already_read).then_some(stored).flatten()
}

/// A one-line instruction that is nothing but a frame reference drew nothing and said nothing: the frame moved to
/// where it belongs and the instruction is cleared, so the row does not offer to edit a picture from no words.
pub fn clear_frame_only_instruction(instruction: &str) -> String {
    let lines: Vec<&str> = instruction.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.len() == 1 && frame_line(lines[0]).is_some() {
        return String::new();
    }
    instruction.to_string()
}

/// An older `<project>/publish/` folder wins over `produce/publish/` for as long as the new one does not exist —
/// [`crate::layout::Tree::publish_dir`] answers that, and nothing is migrated because a person's own folder is
/// not this app's to move.
pub fn publish_folder_wins(legacy_exists: bool, new_exists: bool) -> bool {
    !new_exists && legacy_exists
}

/// The deadlock breaker. The text gate asks for the upload text once the picture exists; with no picture, no
/// images and no instruction there is nothing that could ever produce one, so the gate opens again rather than
/// sitting shut forever.
pub const DEADLOCK_LOG: &str = "    publish: no picture, no images and no instruction \u{2014} asking for the upload text again";

/// Does the deadlock hold? All three have to be empty; one image is enough to draw from, and an instruction alone
/// is what a drawn-from-nothing picture needs.
pub fn asks_for_text_again(has_picture: bool, images: usize, instruction: &str) -> bool {
    !has_picture && images == 0 && instruction.trim().is_empty()
}

/// A picked frame says so in the log with the moment it came from, and the sentence is the promise: no model, no
/// GPU, this is the picture already shot.
pub fn chosen_frame_log(at_seconds: f64) -> String {
    format!(
        "    publish: the thumbnail is the frame at {}, as it is \u{2014} no model, no GPU",
        tools::mm_ss(at_seconds)
    )
}

/// Why the chosen frame did not work out, and what happened instead. The failure never turns into a drawn
/// thumbnail silently: the line names the reason first.
pub fn chosen_frame_failed_log(why: &str) -> String {
    format!("    publish: {why} -- the thumbnail is drawn instead")
}

/// Where the prototype's `frame:` line points. `frame: clip <n> +<seconds>` is that clip's start plus the offset;
/// a bare second or `mm:ss` is the produced clock. An unknown clip or a negative sum names **no** frame — the
/// prototype never fell back to "take the first one", because a wrong picture on a video's page is worse than a
/// missing one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameAt {
    /// A clip's start plus an offset into it, in clip number then seconds.
    Clip(usize, f64),
    /// A moment on the produced clock.
    Seconds(f64),
}

/// Parse one `frame:` line's value. Returns `None` for anything that is not a frame reference at all, which is how
/// [`clear_frame_only_instruction`] and the reply parser both know what they are looking at.
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

/// The clip's own start plus the offset, or `None` when the cut has no such clip — again, never a guess at the
/// first one. `starts` is every clip's start in the cut's order, 1-based on the clip number as the model writes it.
pub fn frame_moment(named: FrameAt, starts: &[f64]) -> Option<f64> {
    match named {
        FrameAt::Seconds(at) => Some(at),
        FrameAt::Clip(number, offset) => starts.get(number.checked_sub(1)?).map(|start| start + offset),
    }
}

/// `90`, `1:30`, `1:30.5` or a signed variant of either, as seconds. A bare number is seconds; a colon makes it
/// minutes and seconds, which is what a person types off a player's clock.
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

/// A `THUMBNAIL: frame: …` line is a frame, and whatever the model called its instruction is discarded: it was
/// describing a picture to edit, and there is no longer a picture to edit.
pub fn thumbnail_line_is_frame(line: &str) -> bool {
    match line.trim().strip_prefix("THUMBNAIL:") {
        Some(rest) => frame_line(rest.trim()).is_some(),
        None => false,
    }
}

/// Peel the labelled lines off a reply — `TITLE:`/`TEXT:` either way round, up to [`LABELLED_LINES_MAX`] of them,
/// quotes stripped. A whole-reply code fence is removed first because a model that fences one answer fences the
/// lot, and the fence would then read as part of the first label.
pub fn peel_labels(reply: &str) -> (Vec<String>, String) {
    let text = strip_whole_fence(reply);
    let mut labels = Vec::new();
    let mut rest = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if labels.len() < LABELLED_LINES_MAX {
            if let Some(value) = labelled_value(trimmed) {
                labels.push(value);
                continue;
            }
        }
        rest.push(line.to_string());
    }
    (labels, rest.join("\n"))
}

/// Drop a fence only when the *whole* reply is fenced: a model quoting an ffmpeg line inside prose keeps its
/// backticks, and eating them would edit somebody's answer.
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

/// `KEY: value` where the key is a single upper-case word, as either label spelling. Quotes around the value are
/// the model hedging, not part of the line.
fn labelled_value(line: &str) -> Option<String> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_uppercase() || c == '_') || key.len() < 4 {
        return None;
    }
    let value = value.trim().trim_matches('"').trim_matches('\'').trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// A first description line under [`DESCRIPTION_HEADING_CHARS`] that ends in ":" is a heading the model wrote for
/// itself ("Chapters:"), not a sentence for the page. Long enough, or not a heading, and it stays.
pub fn drops_description_heading(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.ends_with(':') && trimmed.chars().count() < DESCRIPTION_HEADING_CHARS
}

/// The candidate frames: the middles of three equal bands over the kept footage. Never the first or last frame of
/// a band (a cut's head is a black frame and its tail is where the person stopped caring), and never a moment an
/// insert covers, because that picture is not in the video at that point.
pub fn candidate_frames(kept: &[(f64, f64)], extracted: &[f64]) -> Vec<f64> {
    let mut wanted: Vec<f64> = Vec::new();
    for (from, to) in kept {
        let span = (to - from).max(0.0);
        for band in 0..LABELLED_LINES_MAX {
            let middle = from + span * (2.0 * band as f64 + 1.0) / (2.0 * LABELLED_LINES_MAX as f64);
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

/// Nothing to choose from either: the picture is drawn from words alone, and the log says so because the person
/// pressed a button expecting a frame of their own video.
pub const NO_FRAMES_LOG: &str = "    publish: no frames extracted either \u{2014} drawing from the instruction alone";

/// A base image that has gone missing is an error, not a fallback: the person pointed at that file and editing
/// something else would be a different picture than the one they agreed to.
pub fn base_gone_error(path: &str) -> String {
    format!("the base image is gone: {path}")
}

/// A reference image that has gone missing is skipped and logged: references are hints about style, and losing
/// one leaves a smaller but honest request.
pub fn reference_skipped_log(path: &str) -> String {
    format!("    publish: {path} is not there \u{2014} sent without it")
}

/// The request line: what size, what base, how many images, and how the base was cropped. Everything a person
/// needs to tell "it ignored my crop" from "the crop went off and came back wrong".
pub fn request_log(wide: u32, high: u32, base: &str, images: usize, percent: u32, cx: f64, cy: f64) -> String {
    format!(
        "    publish: {wide}x{high} editing {base}, {images} image(s) sent, base cropped to {percent}% of its width around {cx:.2},{cy:.2}"
    )
}

/// …and the version with nothing to edit.
pub const REQUEST_NO_IMAGES_LOG: &str = "    publish: drawn from the instruction alone, no images";

/// "Already drawn" needs *both* thumbnail files and a stamp match. One file is half a picture (the plain one is
/// what printing failed back to), and a stamp that differs means something the draw reads moved — a frame's
/// size@mtime, the instruction, the negative, the crop centre, the aspect, or whether the picture is a chosen
/// frame at all.
pub fn already_drawn(both_files: bool, stamp_matches: bool) -> bool {
    both_files && stamp_matches
}

/// The chosen-picture branch prints its words *before* the text-only gate, so ↻ Suggest picking a frame still
/// gets its title even on a project with no upload text yet. A printing failure keeps the plain picture rather
/// than losing the frame too ([`crate::produce_runs::reprint`] does the draw; this is the order rule).
pub fn prints_words_first(has_chosen_frame: bool, text_gate_closed: bool) -> bool {
    has_chosen_frame && !text_gate_closed
}

/// Printing failed: keep the plain picture. Losing the frame would turn a cosmetic failure into no thumbnail.
pub fn print_failed_keeps_plain(print_failed: bool) -> bool {
    print_failed
}

/// The title band is a box only while the picture carries words; with none there is nothing to place, and a
/// marked-but-empty box would read as a promise the render does not keep.
pub fn title_band_is_box(carries_words: bool) -> bool {
    carries_words
}

/// Remove keeps the printed line off for good — the difference between clearing the text (which re-seeds from
/// the title, [`screen::seed_title`]) and saying this picture has no words.
pub const REMOVE_IS_PERMANENT: &str = "Remove keeps it off for good; clearing the line only lets the next seed refill it";

/// The YouTube title never re-prints the picture: it is metadata, and a person fixing a typo in it should not
/// wait on an image model or lose a frame they chose.
pub fn youtube_title_redraws_picture() -> bool {
    false
}

// ---- S8: text boxes ------------------------------------------------------------------

/// Only the ✎ chip opens the dialog; a click anywhere else on the row drags or selects. The hint is §A's own
/// wording, so the promise and the box's behaviour are one sentence.
pub const TEXT_CHIP_HINT: &str = "Printed to fill the box you marked \u{2014} a longer line comes out smaller, and Enter starts a new line.";

/// Does this press open the dialog? Only ever the chip.
pub fn opens_text_dialog(is_chip: bool) -> bool {
    is_chip
}

/// A drag under [`DRAG_MIN_PX`] marks nothing, so tapping inside a box you already drew does not erase it.
pub fn drag_marks(moved_px: f64) -> bool {
    moved_px >= DRAG_MIN_PX
}

/// A box saved empty is not created, and an existing one emptied is removed: an empty text effect would be a box
/// in the list that draws nothing and has to be found before anyone can use the row again.
pub fn box_saved_empty(existed: bool) -> BoxSave {
    if existed { BoxSave::Removed } else { BoxSave::NotCreated }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSave {
    NotCreated,
    Removed,
}

/// Nothing to print means the plain bytes go through untouched — no re-encode, no watermark of a box nobody
/// filled, and no reason for the picture to change at all.
pub fn copies_plain_bytes(words: &str) -> bool {
    words.trim().is_empty()
}

// ---- S9: the <video> tag and the stamp -----------------------------------------------

/// The tracks come off disk, so a caption file deleted since the last run drops out of the page instead of
/// shipping a broken `<track>` — [`embed::list_vtt`] reads the folder, [`crate::produce_data::tag_rewritten`]
/// says why the tag is rewritten even when the encode was skipped.
pub fn tracks_from_disk(dir: &std::path::Path) -> Vec<String> {
    embed::list_vtt(dir)
}

/// The page's own file, beside the video — [`embed::paths`]' third answer.
pub fn page_path(tree: &crate::layout::Tree, container: project::Container) -> std::path::PathBuf {
    embed::paths(tree, container).2
}

/// Both web notes are [`embed::web_unplayable`]'s; named here so §6's two sentences have one test target.
pub fn web_note(container: project::Container, codec: project::Codec) -> Option<&'static str> {
    embed::web_unplayable(container, codec)
}

/// An unwritable poster is logged and the tag written without one: a page with no picture still plays, and
/// failing the run over a `.jpg` would throw away an encode that already worked. The reason comes from
/// [`embed::no_poster_reason`] and the line from [`embed::unwritable_log`].
pub fn poster_unwritable_log(path: &str) -> String {
    embed::unwritable_log(path)
}

/// Why there is no poster at all, in the log's own words.
pub fn poster_missing_reason() -> &'static str {
    embed::no_poster_reason()
}

/// §6's stamp rule in one question: an uncomputable or unwritable stamp never matches. The log line is the
/// app's own, spelled with the reason ffmpeg/glibc gave and the promise that nothing was lost but time.
pub fn stamp_unwritable_log(why: &str) -> String {
    format!("    produce: could not write the render stamp ({why}) \u{2014} the next \u{25b6} will encode again")
}

/// A stamp only ever lets a run skip work it can prove was done. Uncomputable (no settings to hash, no tree) or
/// unwritable (the write failed, so nothing was stored) both read as *not up to date*, which costs one encode
/// and never hides a change. Otherwise [`stamp::skip_encode`] answers.
pub fn stamp_matches(computable: bool, stored: Option<&str>, current: &str) -> bool {
    computable && stamp::skip_encode(stored, current)
}

/// …and the second half of §6's sentence: a stamp that could not be written never fails the run. The encode it
/// describes already succeeded, and the only cost is paying for it twice.
pub fn stamp_failure_fails_run() -> bool {
    false
}
