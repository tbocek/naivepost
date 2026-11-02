//! §07-narrate#6 — the details a finished screen has to get right, confirmed against the code.
//!
//! Every message this page shows and every small rule behind it, as plain functions: unusable-reply wording ([`crate::narrate_reply`]), the Inputs row ([`clips_line`], [`stale_mark`], [`timeline_line`], [`inputs_tooltip`]),
//! ▶'s re-roll refusal ([`re_roll_refusal`]), sample and reference wording, added-file names, and the take band's statements. Four of §6's bullets are confirmed elsewhere:
//!
//! - the previous narration is copied aside **only once a valid one came back**, one generation deep — [`crate::narrate_data::keep_previous`], logged by [`previous_kept_log`];
//! - several entries on one clip play in `at` order, restored by [`crate::narration::Narration::sort`] on every read and write;
//! - `takes.json` is keyed by the **recording's base name, not the narrator's slot** — [`crate::narrate_data::take_base`], [`crate::narrate_data::save_takes`];
//! - changing the takes removes the built reference — [`crate::narrate_data::drop_reference`].

use crate::tools::mm_ss;

/// P.eng.narrationMaxExtendSeconds: how long past its own end a line may still be speaking when the cut moves on; [`crate::narrate_preview::tick`] owns the value.
pub const MAX_EXTEND_SECONDS: f64 = crate::narrate_preview::MAX_EXTEND_SECONDS;

/// P.machine.refWantSeconds: the length the automatic reference pick aims for, and the mark a hand-picked band is judged against in [`commit_status`].
/// Kept out of [`crate::params::narrate`] on purpose (§07-narrate#4 lists it among values a rule does not read yet).
pub const REFERENCE_WANTED_SECONDS: f64 = 14.0;

// ---- the Inputs row (F4.2) -------------------------------------------------------

/// The row's first half: `N clip(s) · mm:ss` of video, or the one sentence that says why there is none.
/// One clip reads "1 clip", not "1 clip(s)" (§6 confirms against the prototype's `plural`).
pub fn clips_line(clips: usize, seconds: f64) -> String {
    format!("{} \u{b7} {}", plural(clips, "clip"), mm_ss(seconds))
}

/// The whole Inputs row: the counts ▶ would fix go after a ⚠, and a missing timeline is named on the line itself; [`crate::narrate_screen::inputs_readout`] must match.
pub fn inputs_line(
    clips: usize,
    seconds: f64,
    unwritten_clips: usize,
    lines_off_cut: usize,
    has_timeline: bool,
    has_cut: bool,
) -> String {
    if !has_cut {
        return NO_CUT_LINE.to_string();
    }
    let mut line = clips_line(clips, seconds);
    let mark = stale_mark(unwritten_clips, lines_off_cut);
    if !mark.is_empty() {
        line.push_str(&format!(" \u{b7} \u{26a0} {mark}"));
    }
    if !has_timeline {
        line.push_str(" \u{b7} no timeline");
    }
    line
}

pub const NO_CUT_LINE: &str = "no cut yet \u{2014} build one on the Cut step";

/// The ⚠ half: what ▶ would fix, in the two counts it fixes them in.
pub fn stale_mark(unwritten_clips: usize, lines_off_cut: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    if unwritten_clips > 0 {
        parts.push(format!("{} unwritten", plural(unwritten_clips, "clip")));
    }
    if lines_off_cut > 0 {
        parts.push(format!("{} off the cut", plural(lines_off_cut, "line")));
    }
    parts.join(", ")
}

/// The transcript row of the tooltip: the ±4 s rule quoted with its number (§10). No lines at all moves the row to the main line (a missing input).
pub const CONTEXT_WINDOW_ID: &str = "P.machine.narrationContextSeconds";

pub fn timeline_line(lines: usize) -> Option<String> {
    if lines == 0 {
        return None;
    }
    let window = crate::narrate_pass::CONTEXT_SECONDS;
    Some(format!(
        "prepare/transcript/session.tsv — {lines} lines; the ones falling inside a clip (±{window:.0} s) go with that clip"
    ))
}

/// The voice row: named in the tooltip alone, because the picker beside it already says which voice.
pub fn voice_line(voice: &str, pitch_semitones: f64) -> String {
    format!("Spoken by {voice} at {pitch_semitones:+.1} semitones (narrate/voice_ref.wav)")
}

pub fn cut_detail(clips: usize, seconds: f64) -> String {
    format!(
        "cut/cut.json \u{2014} {clips} clips, {} of video to write for",
        mm_ss(seconds)
    )
}

/// The whole tooltip in §6's order: cut file, why it is stale and what fixes it, transcript with its window, session context verbatim, voice.
pub fn inputs_tooltip(
    clips: usize,
    seconds: f64,
    stale_why: &str,
    transcript_lines: usize,
    session_context: &str,
    voice: Option<(&str, f64)>,
) -> String {
    let mut rows: Vec<String> = Vec::new();
    if clips > 0 {
        rows.push(cut_detail(clips, seconds));
    }
    if !stale_why.is_empty() {
        rows.push(format!(
            "\u{26a0} {stale_why} \u{2014} \u{25b6} writes the narration again"
        ));
    }
    if let Some(row) = timeline_line(transcript_lines) {
        rows.push(row);
    }
    if !session_context.is_empty() {
        rows.push(format!(
            "Session context (Describe), sent with the narration:\n{session_context}"
        ));
    }
    if let Some((name, pitch)) = voice {
        rows.push(voice_line(name, pitch));
    }
    rows.join("\n\n")
}

// ---- re-roll, sample, reference (F4.3 / F4.7) ------------------------------------

/// §6 [F4.1]: the run log's continuation line (four leading spaces) naming where the overwritten narration is kept; written only once a valid one came back.
pub fn previous_kept_log(path: &str) -> String {
    // The spec names the file but not the wording; this line is the app's.
    format!("    the narration it replaced is kept at {path}")
}

/// §07-narrate#3-data: said when the current `narration.json` could not be read and the run started from the kept generation; naming the file keeps it from looking like a fresh session.
pub fn previous_fallback_log(path: &std::path::Path) -> String {
    format!(
        "    narration.json could not be read -- starting from the kept copy at {}",
        path.display()
    )
}

/// §6 (`Re-roll / sample refusals`): ↻ on a caption line; the captions-only voice has no takes to draw again.
pub fn re_roll_captions_refusal() -> &'static str {
    "no audio is chosen \u{2014} a caption has no take to re-roll"
}

/// §6: ↻ accepted; `line` is the row's index from 0, spoken as the number the listener hears next.
pub fn new_take_status(line: usize) -> String {
    format!("line {}: new take, speaking it", line + 1)
}

// §6: the four ways ▶ Sample refuses, each its own sentence; none opens a dialog.

/// Nothing chosen in the picker: there is no voice to speak anything.
pub fn pick_a_voice_refusal() -> &'static str {
    "pick a voice first"
}

/// The captions voice: it has no reference and never will, so there is nothing to sample.
pub fn no_voice_to_sample_refusal() -> &'static str {
    "no audio is chosen \u{2014} there is no voice to sample"
}

/// The sentence box is empty, so the only thing on hand to speak is nothing.
pub fn empty_sample_refusal() -> &'static str {
    "type a sample sentence to hear the voice"
}

/// A row's ▶ while its take is out on the wire (synthesis runs on a worker, §03-shell §1): so the row never reads as done before the take is heard.
pub fn speaking_line_status(line_no: usize) -> String {
    format!("speaking line {line_no}\u{2026} \u{23f9} gives up on it")
}

/// The last press has not finished: a second one would start a second synthesis of a line nobody heard yet.
pub fn busy_sample_status() -> &'static str {
    "still synthesizing the last sample\u{2026}"
}

/// §6 (`Re-roll / sample refusals`): what the status bar says while the sample is being made.
pub fn sample_status() -> &'static str {
    "synthesizing the sample\u{2026}"
}

// §6: the three states of a playing sample; the row's ▶ resumes or starts over, hence the icons.

/// The sample holds the player.
pub fn sample_playing_status() -> &'static str {
    "sample playing"
}

/// Paused mid-sample: ▶ picks up where it stopped, ⏹ would take the line from its beginning (§5).
pub fn sample_paused_status() -> &'static str {
    "sample paused \u{2014} \u{25b6} resumes, \u{23f9} starts over"
}

/// The sample is gone from the player; nothing else on the page changed.
pub fn sample_stopped_status() -> &'static str {
    "sample stopped"
}

/// §6: every sample logs voice, pitch and words **before** anything can go wrong (no output file to inspect). `take` is the row's re-roll count, absent
/// for a first take (`>>> sample:` against `>>> sample take 2:`).
pub fn sample_log(voice: &str, pitch_semitones: f64, take: Option<usize>, text: &str) -> String {
    let take = take.map_or(String::new(), |roll| format!(" take {}", roll + 1));
    format!(">>> sample{take}: {voice} at {pitch_semitones:+.1} semitones \u{2014} \"{text}\"")
}

/// §6: then what came back: file, size, and whether synthesis was just done or cached. `spoken_in` is how long it took, `None` for a cached one.
pub fn sample_file_log(file: &str, bytes: u64, spoken_in: Option<&str>) -> String {
    let spoken = spoken_in.map_or("spoken earlier".to_string(), |took| {
        format!("spoken in {took}")
    });
    format!("    sample: {file} ({} kB, {spoken})", bytes / 1024)
}

/// §6: a server answering a header and no audio leaves the room quiet while the player reports playing, so the log says how few bytes came.
pub fn sample_too_small_log(file: &str, bytes: u64) -> String {
    format!("!!! sample: {file} is {bytes} bytes \u{2014} no audio came back")
}

/// §6 (`Reference build`): the one line a built reference leaves, naming automatic or hand-picked; `hand_picked` nought means the automatic wording, whose tail is the word count.
pub fn reference_build_log(seconds: f64, words: usize, hand_picked: usize, base: &str) -> String {
    let how = if hand_picked > 0 {
        format!("{hand_picked} hand-picked take(s)")
    } else {
        format!("{words} words")
    };
    format!(">>> voice reference built: {seconds:.1} s, {how} from {base}")
}

/// A pasted recording that is not mono PCM yet (`tool:ffmpeg.pcm` converts it); this page only says why (§6 `"Add file…"`).
pub fn reference_convert_log(name: &str) -> String {
    format!("{name} is not mono PCM \u{2014} converting it with ffmpeg")
}

// ---- Add file (F4.7) -------------------------------------------------------------

/// §6 (`"Add file…"`): the name a pasted recording gets in the voices folder: its base sanitised ([`crate::narrate_data::sanitize_voice_id`]), **never overwriting**;
/// an index starting at `-2` appears only when the name is taken, since the file name IS the voice id in every cache key.
pub fn added_file_name(base_stem: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base_stem) {
        return base_stem.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base_stem}-{n}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

// ---- the take band's words (F4.5 / F4.6) -----------------------------------------

/// §6 (`Take band messages`): the band is empty, so the wave must be dragged across first; ＋ is the full-width plus glyph.
pub fn take_add_hint() -> &'static str {
    "drag across the wave first \u{2014} \u{ff0b} makes the selection a take"
}

/// §6: the other button, the full-width minus.
pub fn take_remove_hint() -> &'static str {
    "drag across the takes you want gone \u{2014} \u{ff0d} removes those seconds"
}

/// §6: － pressed over a stretch where nothing is picked — the selection was made, it just holds no take.
pub fn nothing_picked() -> &'static str {
    "nothing is picked in those seconds"
}

/// §6: the last take removed; the automatic pick resumes, and saying so keeps an accidental － from reading as a lost voice.
pub fn no_takes_left() -> &'static str {
    "no takes left \u{2014} the seconds are chosen for you again on the next line spoken"
}

/// §6: the walk finished by itself, every take played; silence with ⏹ still on the button would read as a hang.
pub fn takes_played() -> &'static str {
    "takes played"
}

/// §6: the walk ended because a hand stopped it, not because it ran out.
pub fn takes_stopped() -> &'static str {
    "stopped playing the takes"
}

/// The status line while takes play: how many, for how long, from where; a recording with nothing picked after it says so.
pub fn playing_takes(take_count: usize, total_seconds: f64, from_seconds: f64) -> String {
    if take_count == 0 {
        return format!(
            "playing the recording from {} \u{2014} nothing is picked after it",
            mm_ss(from_seconds)
        );
    }
    format!(
        "playing {take_count} take(s), {:.1} s from {}",
        total_seconds,
        mm_ss(from_seconds)
    )
}

/// §6 (`Take band messages`): the band after a ＋ or －. `what` is the takes or the recording they were taken from; the total decides whether this is a voice,
/// judged against [`REFERENCE_WANTED_SECONDS`]. The band stays a proposal until committed.
pub fn commit_status(what: &str, picked: usize, seconds: f64, wanted: f64) -> String {
    format!(
        "{what} \u{2014} {picked} take(s), {seconds:.1} s of reference ({wanted:.0} s is plenty). \
         Re-cut on the next line spoken."
    )
}

// ---- the preview's bound (F4.5) ---------------------------------------------------

// ---- helpers ---------------------------------------------------------------------

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}
