//! §07-narrate#6 — the details a finished screen has to get right, confirmed against the code.
//!
//! Every message this page shows and every small rule behind it lives here as a plain function: what
//! an unusable reply says ([`crate::narrate_reply`]), how the Inputs row reads
//! ([`clips_line`], [`stale_mark`], [`timeline_line`], [`inputs_tooltip`]), why ▶ refuses to re-roll
//! ([`re_roll_refusal`]), what a refused sample or a rebuilt reference says, how an added file is named,
//! and what the take band states while it plays. The widgets stay thin; the wording is testable.
//!
//! Four of §6's bullets are confirmed elsewhere and only pointed at from here, so this module doc is the
//! one place a reader finds the whole list:
//!
//! - the previous narration is copied aside **only once a valid one came back**, one generation deep and
//!   never appended to — [`crate::narrate_data::keep_previous`], whose log line is [`previous_kept_log`];
//! - several entries on one clip play in `at` order, put back by [`crate::narration::Narration::sort`] on
//!   every read and write;
//! - `takes.json` is keyed by the **recording's base name, not the narrator's slot**, so re-tagging never
//!   moves anyone's takes — [`crate::narrate_data::take_base`] and [`crate::narrate_data::save_takes`];
//! - changing the takes removes the built reference — [`crate::narrate_data::drop_reference`] — because a
//!   reference nobody can say which seconds made it is worse than none.
//!
//! Ids cited: `P.machine.narrationContextSeconds` (the ±4 s transcript rule),
//! `P.eng.narrationMaxExtendSeconds` (how long a preview holds for one line, [`MAX_EXTEND_SECONDS`]),
//! `P.machine.refWantSeconds` ([`REFERENCE_WANTED_SECONDS`], catalogued elsewhere on purpose — see there),
//! `tool:ffmpeg.pcm` (a pasted take becomes mono PCM, [`reference_convert_log`]). The take band's own
//! seconds live in [`crate::narrate_rules`] and [`crate::narrate_screen`] already.

use crate::tools::mm_ss;

/// P.eng.narrationMaxExtendSeconds: how long past its own end a line may still be speaking when the cut
/// moves on. Named here because §6 lists the preview's hold among the details to confirm; what acts on it
/// is [`crate::narrate_preview::tick`], which owns the value.
pub const MAX_EXTEND_SECONDS: f64 = crate::narrate_preview::MAX_EXTEND_SECONDS;

/// P.machine.refWantSeconds: the length the automatic reference pick aims for, and the mark a hand-picked
/// band is judged against in [`commit_status`]'s "14 s is plenty". The row stays out of
/// [`crate::params::narrate`] on purpose — §07-narrate#4's comment there lists this id among the five
/// automatic-reference values a rule does not read yet, and cataloguing it here would silently undo that.
pub const REFERENCE_WANTED_SECONDS: f64 = 14.0;

/// §5 (`the Replace tick is remembered per project, never on by default`): a re-roll overwrites a take
/// somebody may already have listened to, so the page starts with it off every time the project opens.
pub fn replace_tick_default() -> bool {
    false
}

// ---- the Inputs row (F4.2) -------------------------------------------------------

/// The row's first half: `N clip(s) · mm:ss` of video, or the one sentence that says why there is none.
/// §6 confirms the shape against the prototype (`plural(n, "clip")`), so one clip reads "1 clip", not
/// "1 clip(s)" — the parenthesised spelling in §1 is the pattern, not the string on screen.
pub fn clips_line(clips: usize, seconds: f64) -> String {
    format!("{} \u{b7} {}", plural(clips, "clip"), mm_ss(seconds))
}

/// The whole Inputs row: the counts ▶ would fix go after a ⚠, and a missing timeline is named on the line
/// itself rather than hidden in the tooltip. [`crate::narrate_screen::inputs_readout`] draws the same row
/// from the shell's answers; this is the wording it has to match.
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

/// The one sentence that says why there is nothing to write for.
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

/// The transcript row of the tooltip: the ±4 s rule quoted with its own number (§10), so a reader can
/// check it. No lines at all means the row goes on the main line instead — that is a missing input.
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

/// The cut row of the tooltip: the file, how many clips, how much video.
pub fn cut_detail(clips: usize, seconds: f64) -> String {
    format!(
        "cut/cut.json \u{2014} {clips} clips, {} of video to write for",
        mm_ss(seconds)
    )
}

/// The whole tooltip, in the order §6 lists it: the cut file, why it is stale and what fixes it, the
/// transcript with its window, the session context verbatim, the voice.
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
        rows.push(format!("\u{26a0} {stale_why} \u{2014} \u{25b6} writes the narration again"));
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

/// Why ▶ refuses to replace a narration someone has already listened to or edited: the take is only
/// overwritten once the page admits it was heard. The refusal says what to click; it is the page's own
/// sentence ([`crate::narrate_screen::replace_refusal`]), named here so §6 pins one wording.
pub fn re_roll_refusal(heard_or_edited: bool, tick_checked: bool) -> Option<&'static str> {
    crate::narrate_screen::replace_refusal(heard_or_edited, tick_checked)
}

/// §6 [F4.1]: the run log's continuation line naming where the narration that was overwritten is kept —
/// written only once a valid narration came back, which is why it follows [`crate::narrate_data::keep_previous`].
/// Four leading spaces because it hangs under the step's own line rather than starting one.
pub fn previous_kept_log(path: &str) -> String {
    format!("    the narration it replaced is kept at {path}")
}

/// §6 (`Re-roll / sample refusals`): ↻ on a clip whose line is a caption — with the captions-only voice
/// nothing has a take, so there is nothing to draw again.
pub fn re_roll_captions_refusal() -> &'static str {
    "no audio is chosen \u{2014} a caption has no take to re-roll"
}

/// §6 (`Re-roll / sample refusals`): ↻ accepted — the row's number, spoken because it is what the listener
/// is about to hear instead of the take they had. `line` is the row's index from 0.
pub fn new_take_status(line: usize) -> String {
    format!("line {}: new take, speaking it", line + 1)
}

// §6 (`Re-roll / sample refusals`): the four ways ▶ Sample refuses, each its own sentence because each
// wants a different click. None of them is an error and none opens a dialog — the status bar alone says it.

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

/// The last press has not finished: a second one would start a second synthesis of a line nobody heard yet.
pub fn busy_sample_status() -> &'static str {
    "still synthesizing the last sample\u{2026}"
}

/// §6 (`Re-roll / sample refusals`): what the status bar says while the sample is being made — named in
/// §6 so it never reads as done.
pub fn sample_status() -> &'static str {
    "synthesizing the sample\u{2026}"
}

// §6 (`Re-roll / sample refusals`): the three states of a sample that is playing. They are the row's ▶'s
// meaning, and the row's ▶ is what resumes or starts over — hence the icons in the sentence.

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

/// §6: every sample logs which voice, at what pitch, in what words, **before** anything can go wrong — a
/// sample is the one thing on this page with no output file to inspect afterwards. `take` is the row's
/// re-roll count, and its absence from the string (`>>> sample:` against `>>> sample take 2:`) is the
/// point: a line's first take is not worth naming.
pub fn sample_log(voice: &str, pitch_semitones: f64, take: Option<usize>, text: &str) -> String {
    let take = take.map_or(String::new(), |roll| format!(" take {}", roll + 1));
    format!(">>> sample{take}: {voice} at {pitch_semitones:+.1} semitones \u{2014} \"{text}\"")
}

/// §6: then what came back — the file, its size, and whether the synthesis was just done or heard earlier
/// from the cache. `spoken_in` is the caller's reading of that: how long it took, or nothing for a cached one.
pub fn sample_file_log(file: &str, bytes: u64, spoken_in: Option<&str>) -> String {
    let spoken = spoken_in.map_or("spoken earlier".to_string(), |took| format!("spoken in {took}"));
    format!("    sample: {file} ({} kB, {spoken})", bytes / 1024)
}

/// §6: a server that answers with a header and no audio is the failure this page cannot show — the player
/// takes the file, reports itself playing, and the room stays quiet. So the log says how few bytes came.
pub fn sample_too_small_log(file: &str, bytes: u64) -> String {
    format!("!!! sample: {file} is {bytes} bytes \u{2014} no audio came back")
}

/// §6 (`Reference build`): the one line a built reference leaves. It names which of the two kinds it is —
/// an automatic reference that sounds wrong is a ranking to overrule by hand, a hand-picked one is seconds
/// to re-pick — so `hand_picked` nought means the automatic wording, whose own tail is the word count.
pub fn reference_build_log(seconds: f64, words: usize, hand_picked: usize, base: &str) -> String {
    let how = if hand_picked > 0 {
        format!("{hand_picked} hand-picked take(s)")
    } else {
        format!("{words} words")
    };
    format!(">>> voice reference built: {seconds:.1} s, {how} from {base}")
}

/// A pasted recording that is not mono PCM yet — `tool:ffmpeg.pcm`: ffmpeg makes the mono 16-bit wav, this
/// page only says why (§6 `"Add file…"`).
pub fn reference_convert_log(name: &str) -> String {
    format!("{name} is not mono PCM \u{2014} converting it with ffmpeg")
}

// ---- Add file (F4.7) -------------------------------------------------------------

/// §6 (`"Add file…"`): the name a pasted recording gets in the voices folder — its sanitised base
/// ([`crate::narrate_data::sanitize_voice_id`] is what sanitises it), and **never overwriting**: only when
/// that name is taken does an index appear, starting at `-2`, because the file name *is* the voice id in
/// every project's cache keys and silently rewriting one would rewrite its cached audio everywhere.
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

/// §6 (`Take band messages`): the band is empty, so the wave has to be dragged across first — ＋ on its own
/// would have nothing to make a take of. Full-width plus because that is the button's glyph.
pub fn take_add_hint() -> &'static str {
    "drag across the wave first \u{2014} \u{ff0b} makes the selection a take"
}

/// §6: the other button, with the full-width minus that removes seconds rather than adding them.
pub fn take_remove_hint() -> &'static str {
    "drag across the takes you want gone \u{2014} \u{ff0d} removes those seconds"
}

/// §6: － pressed over a stretch where nothing is picked — the selection was made, it just holds no take.
pub fn nothing_picked() -> &'static str {
    "nothing is picked in those seconds"
}

/// §6: the last take removed. The band is empty again, so the automatic pick resumes on its own; saying so
/// is what keeps an accidental － from reading as a lost voice.
pub fn no_takes_left() -> &'static str {
    "no takes left \u{2014} the seconds are chosen for you again on the next line spoken"
}

/// §6: nothing has been picked in this session at all, which is a different sentence from
/// [`no_takes_left`] — there is no hand-picked band to have just emptied.
pub fn no_takes_yet() -> &'static str {
    "no takes yet \u{2014} drag across the wave and press \u{ff0b}"
}

/// §6: the walk finished by itself, every take played. Worth saying because the alternative — silence with
/// ⏹ still on the button — reads as a hang.
pub fn takes_played() -> &'static str {
    "takes played"
}

/// §6: the walk ended because a hand stopped it, not because it ran out.
pub fn takes_stopped() -> &'static str {
    "stopped playing the takes"
}

/// The status line while takes play: how many, for how long, from where. A recording with nothing picked
/// after it says so — that is the case a listener would otherwise misread as a resume.
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

/// §6 (`Take band messages`): the band after a ＋ or －. `what` is what the sentence is about — the takes,
/// or the recording they were taken out of — and the total is in it because it is the number that decides
/// whether this is a voice: [`REFERENCE_WANTED_SECONDS`] is what the automatic pick aims for, so it is the
/// fair mark to be judged against. The band stays a proposal until it is committed.
pub fn commit_status(what: &str, picked: usize, seconds: f64, wanted: f64) -> String {
    format!(
        "{what} \u{2014} {picked} take(s), {seconds:.1} s of reference ({wanted:.0} s is plenty). \
         Re-cut on the next line spoken."
    )
}

// ---- the preview's bound (F4.5) ---------------------------------------------------

/// §6 (`Preview`): a gap is held only while the speaking line is within its clip's end plus
/// [`MAX_EXTEND_SECONDS`]; past that the picture moves on and the line is simply late. The value is
/// P.eng.narrationMaxExtendSeconds and [`crate::narrate_preview::tick`] is what acts on it — this is here
/// so §6's bound is pinned next to the page's other wording.
pub fn preview_hold_bound() -> f64 {
    MAX_EXTEND_SECONDS
}

// ---- helpers ---------------------------------------------------------------------

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}
