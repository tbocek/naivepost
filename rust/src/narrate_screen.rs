//! §07-narrate#1-screen: what the Narrate page shows and what a press would do; no gtk, so greying, tooltips and take
//! seconds are answerable without a display (spec/00-principles.md §5). §1's numbered widgets are spec/inventory/narrate.md §A.
//! The record is [`crate::narration`] (`wav_exists` for [`status`] comes from [`crate::narration::tts_file`]); the shell's one-line
//! summary is `crate::shell::inputs` and [`inputs_readout`] is the page's fuller row. Fitting a line to its clip is F4.3's
//! (the render); [`mirror_fit`] takes the numbers. Every §10 number lives once, in the module whose rule uses it ([`crate::params`] catalogues them).

use crate::cut::Seg;
use crate::narration::Entry;
use crate::{cut_hear, preview, tools};

/// The toolbar's groups in order (§1's 3–8): the order is a fact a test can hold. The shot lays them out as rows (3–6 under the
/// picture, 7–8 in the voice column, sample row below), which reads transport → volume → voice → takes → sample → pitch.
pub const TOOLBAR_GROUPS: [&str; 6] = ["transport", "volume", "voice", "takes", "sample", "pitch"];

/// §1's Prototype note: the prototype's full-width ＋ (U+FF0B) is CJK-only and draws as a missing-glyph box, so use the named icon.
pub const ADD_LINE_ICON: &str = "list-add";

/// The take row's −, same note (U+FF0D). `-symbolic` on purpose: the bare `list-remove` resolves to nothing in the shipped Adwaita theme.
pub const TAKE_REMOVE_ICON: &str = "list-remove-symbolic";

pub const BACK_SECONDS: f64 = 3.0;

/// Where a line added below another starts: half a second after the one above ends (inventory C.5).
pub const ADD_BELOW_SECONDS: f64 = 0.5;

/// §1 (`audition from 3 s ahead where those seconds are its own`): the row's ▶ reach, the same as ‹‹.
pub const AUDITION_LEAD_SECONDS: f64 = 3.0;

pub const TAKE_LABEL_MIN_PX: f64 = 34.0;

/// P.eng.takeMinSeconds: the shortest hand-picked take; shorter is a breath, not a voice to clone.
pub const TAKE_MIN_SECONDS: f64 = 0.4;

pub fn take_min() -> f64 {
    TAKE_MIN_SECONDS
}

/// §1 (`pitch −6..+6 semitones`): the slider's ends.
pub const PITCH_MIN_SEMITONES: f64 = -6.0;

pub const PITCH_MAX_SEMITONES: f64 = 6.0;

pub const PITCH_STEP: f64 = 0.5;

/// §1 (`applied 400 ms after the last move`), spec/10-parameters.md:133 `typing debounce 400 ms`: a shift re-cuts the reference with rubberband.
pub const PITCH_APPLY_MS: u64 = 400;

/// Clamped rather than refused: a stored value outside the limits still means "shift it that way".
pub fn clamp_pitch(semitones: f64) -> f64 {
    semitones.clamp(PITCH_MIN_SEMITONES, PITCH_MAX_SEMITONES)
}

/// §1: the number beside the slider, one decimal (`0.0`, `1.5`, `-6.0`; spec/img/07-narrate.png badge 13), clamped as [`clamp_pitch`].
pub fn pitch_readout(semitones: f64) -> String {
    format!("{:.1}", clamp_pitch(semitones))
}

/// spec/10-parameters.md:133 `Narrate take band: ruler 12 …`.
pub const BAND_RULER_PX: f64 = 12.0;

/// spec/10-parameters.md:133 `… lane 56 …`.
pub const BAND_LANE_PX: f64 = 56.0;

/// spec/10-parameters.md:133 `… max 200 px/s`; shallower than the Cut page's 240 (this band is one recording's few minutes).
pub const BAND_MAX_PPS: f64 = 200.0;

/// spec/10-parameters.md:133 `… click slop 3 px`: under this a press-release is a click on the red bar.
pub const CLICK_SLOP_PX: f64 = 3.0;

/// F4.5's tick. §4's engineering tick is ONE rate for the app: this is `preview::TICK_MS`, and `narrate.tickMs` catalogues the same number.
pub const TICK_MS: u64 = crate::preview::TICK_MS;

/// §1's slider (`Drag to seek …`, inventory §A.4 `debounced 120 ms`): one seek per gesture, as every seek reloads the cut's file.
pub const SEEK_DEBOUNCE_MS: u64 = 120;

pub fn take_label(seconds: f64) -> String {
    format!("{:.1}s", seconds)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TakePress {
    SetBar,
    Select { from: f64, to: f64 },
}

/// A press that travelled less than [`CLICK_SLOP_PX`] is a click. Left to right whichever way the pointer went. `start` is the
/// view's OFFSET in px ([`crate::narrate_band::x_to_second`]), not seconds.
pub fn take_press(pressed_x: f64, released_x: f64, start: f64) -> TakePress {
    let travel = (released_x - pressed_x).abs();
    if travel <= CLICK_SLOP_PX {
        return TakePress::SetBar;
    }
    let (near, far) = if released_x > pressed_x {
        (pressed_x, released_x)
    } else {
        (released_x, pressed_x)
    };
    TakePress::Select {
        from: crate::narrate_band::x_to_second(near, start, 1.0),
        to: crate::narrate_band::x_to_second(far, start, 1.0),
    }
}

pub fn can_make_take(selection: (f64, f64)) -> bool {
    selection.1 - selection.0 >= TAKE_MIN_SECONDS
}

/// §1 / inventory §A.7 (`that is N s — a take has to be at least 0.4 s`).
pub fn take_too_short(seconds: f64) -> String {
    format!(
        "that is {seconds:.1} s \u{2014} a take has to be at least {:.1} s",
        TAKE_MIN_SECONDS
    )
}

/// The band's clean-up (inventory §A.7): sorted, takes under [`TAKE_MIN_SECONDS`] dropped, touching or overlapping takes merged.
fn clean(mut takes: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    takes.retain(|(from, to)| to - from >= TAKE_MIN_SECONDS);
    takes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(takes.len());
    for (from, to) in takes {
        match merged.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => merged.push((from, to)),
        }
    }
    merged
}

/// `＋`: these seconds join the takes, kept only if they reach [`TAKE_MIN_SECONDS`] (the floor [`can_make_take`] enforces).
pub fn add_takes(existing: &[(f64, f64)], span: (f64, f64)) -> Vec<(f64, f64)> {
    let mut takes = existing.to_vec();
    if can_make_take(span) {
        takes.push(span);
    }
    clean(takes)
}

/// `－`: these seconds come out of the takes, splitting one when they fall inside it.
pub fn remove_takes(existing: &[(f64, f64)], span: (f64, f64)) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(existing.len() * 2);
    for (from, to) in existing {
        if *to <= span.0 || *from >= span.1 {
            out.push((*from, *to));
            continue;
        }
        if span.0 > *from {
            out.push((*from, span.0));
        }
        if span.1 < *to {
            out.push((span.1, *to));
        }
    }
    clean(out)
}

/// §1 (`With no takes the seconds are chosen for you.`): a promise, not a warning, as F4.6's automatic pick is usually good.
pub fn band_status(takes: &[(f64, f64)]) -> &'static str {
    if takes.is_empty() {
        "With no takes the seconds are chosen for you."
    } else {
        ""
    }
}

/// §1 (`unlabelled dropdown`): the first row's id; `captions` short-circuits everything that speaks.
pub const CAPTIONS: &str = "captions";

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceOption {
    pub id: String,
    pub label: String,
}

/// §1: the rows in order: `No audio — captions only`, `Narrator 1..4` (slot 1 always, 2..4 when Prepare tagged one), then
/// every .wav in the voices folder (`files`). The dropdown is unlabelled; `Add file…` sits to its right.
pub fn voice_options(narrators: usize, files: &[&str]) -> Vec<VoiceOption> {
    let mut options = vec![VoiceOption {
        id: CAPTIONS.to_string(),
        label: "No audio \u{2014} captions only".to_string(),
    }];
    for slot in 1..=narrators {
        options.push(VoiceOption {
            // Slot 1 is spelled `own` (inventory §voice ids, frozen): the id every take of the project's own voice is keyed without.
            id: if slot == 1 {
                crate::narrate_data::OWN_VOICE.to_string()
            } else {
                format!("narrator{slot}")
            },
            label: format!("Narrator {slot} \u{2014} everything they say in the cut"),
        });
    }
    options.extend(files.iter().map(|name| VoiceOption {
        id: (*name).to_string(),
        label: (*name).to_string(),
    }));
    options
}

/// §1 / inventory §A.6, the dropdown's tooltip verbatim.
pub fn sample_tip() -> String {
    "Who speaks the narration. Every line is spoken by cloning the selected recording; switching voices keeps \
     what you already synthesized"
        .to_string()
}

pub fn no_lines_note() -> &'static str {
    "no narration lines yet \u{2014} \u{ff0b} puts one at the playhead"
}

/// §1 (`Shift the reference recording before it is cloned — a different speaker, not the same one transposed`).
pub fn pitch_tip() -> String {
    "Shift the reference recording before it is cloned \u{2014} a different speaker, not the same one transposed"
        .to_string()
}

/// §1 (`Add file…`): copies, so the voice keeps working when the recording is untagged or moved.
pub fn add_file_tip() -> String {
    "Copy one recording into the voices folder and use it".to_string()
}

pub const DEFAULT_SAMPLE: &str =
    "This is the voice the narration will be spoken in. It should stay clear and \
                                  easy to follow for a couple of minutes.";

/// §1 (`clock "session · cut/length"`): the line on the recording, then in the finished video; tenths are [`preview::clock`]'s on the Cut page.
pub fn clock_line(session: f64, cut_at: f64, length: f64) -> String {
    format!(
        "{} \u{b7} {}/{}",
        tools::mm_ss(session),
        tools::mm_ss(cut_at),
        tools::mm_ss(length)
    )
}

/// What a leading tag means. A placement is a caption (the viewer reads it) and not an emotion; an enum because both cannot be true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxRole {
    None,
    Caption,
    Emotion,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxText {
    pub body: String,
    pub tag: String,
    pub at: Option<f64>,
    pub role: BoxRole,
}

/// §1 (`[emotion] words`, `[top|center|bottom] words`; the tag may carry `@N`). The five placement spellings are inventory §B.4's, `centre` and `middle` included.
pub fn parse_box(text: &str) -> BoxText {
    let trimmed = text.trim_start();
    let mut parsed = BoxText {
        body: text.to_string(),
        tag: String::new(),
        at: None,
        role: BoxRole::None,
    };
    if !trimmed.starts_with('[') {
        return parsed;
    }
    let Some(end) = trimmed.find(']') else {
        return parsed;
    };
    let raw = &trimmed[1..end];
    parsed.body = trimmed[end + 1..].trim_start().to_string();

    let (tag, at) = match raw.rfind('@') {
        Some(at_sign) => {
            let seconds = raw[at_sign + 1..].trim().parse::<f64>().ok();
            match seconds {
                Some(seconds) => (raw[..at_sign].trim(), Some(seconds)),
                None => (raw, None),
            }
        }
        None => (raw, None),
    };
    parsed.tag = tag.to_string();
    parsed.at = at;

    let first = tag.split('=').next().unwrap_or("").trim().to_lowercase();
    let placement = matches!(
        first.as_str(),
        "top" | "center" | "centre" | "middle" | "bottom"
    );
    if placement {
        parsed.role = BoxRole::Caption;
        parsed.tag = String::new();
    } else if !tag.is_empty() {
        parsed.role = BoxRole::Emotion;
    }
    parsed
}

/// The box as the page writes it back (§1: `write-only: the time field owns the number`); `@N` is dropped though [`parse_box`] reads it.
pub fn write_box(entry: &Entry) -> String {
    let tag = if !entry.pos.is_empty() {
        entry.pos.clone()
    } else {
        entry.emotion.clone()
    };
    if entry.text.is_empty() {
        return String::new();
    }
    if tag.is_empty() {
        return entry.text.clone();
    }
    format!("[{tag}] {}", entry.text)
}

/// §1 (`[excited @65] Weee` moves the line, clamped out of the clip's last second), §5's `at never in the last second`. Returns the placement
/// **inside the clip** and whether the tag's second could not be kept (reported, not hushed); `at` is the current placement, kept on a refused move.
pub fn tag_moves_line(clip_s: f64, clip_e: f64, requested: f64, at: f64) -> (f64, bool) {
    let last = (clip_e - 1.0).max(clip_s);
    if !(clip_s..=last).contains(&requested) {
        return (at, true);
    }
    (requested - clip_s, false)
}

/// §1 (`Tooltip explains the tag, judge vs weighted mixes, the eight base emotions`). P.policy.narrationEmotionTag has no §10 row;
/// spec/prompts/system.md:72 spells it, and the order is that line's.
pub fn row_tooltip() -> String {
    format!(
        "[emotion] words \u{2014} the emotion is sent to the voice as the delivery and never spoken; the field on \
         the left is when the line starts. A plain tag is read by a judge (\"angry\", \"surprised, happy\"); add \
         weights to skip it and set the mix exactly (\"[angry=1]\", \"[happy=0.8, surprised=0.4]\"). The eight it \
         mixes: {}. A tag may end in @N, the second the line starts; [top|center|bottom] words is a caption the \
         viewer reads and the voice never speaks.",
        EMOTIONS.join(", ")
    )
}

pub const EMOTIONS: [&str; 8] = [
    "happy",
    "angry",
    "sad",
    "afraid",
    "disgusted",
    "melancholic",
    "surprised",
    "calm",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowStatus {
    Silent,
    Caption,
    Estimated(f64),
    Known(f64),
}

/// §1: which of the four. Estimated means the take does not exist yet (`wav_exists`, asked by the caller); a caption never has one, so it answers first.
pub fn status(entry: &Entry, wav_exists: bool) -> RowStatus {
    let end = entry.s + entry.at;
    if entry.text.is_empty() {
        return RowStatus::Silent;
    }
    if !entry.pos.is_empty() {
        return RowStatus::Caption;
    }
    if wav_exists {
        RowStatus::Known(end)
    } else {
        RowStatus::Estimated(end)
    }
}

/// §1's four spellings. The dash is U+2013; tenths come from [`preview::clock`].
pub fn status_line(status: RowStatus) -> String {
    match status {
        RowStatus::Silent => "(no line \u{2014} this clip plays on its own audio)".to_string(),
        RowStatus::Caption => "(caption \u{2014} the viewer reads it; never spoken)".to_string(),
        RowStatus::Known(end) => format!("\u{2013} {}", preview::clock(Some(end))),
        RowStatus::Estimated(end) => format!("\u{2013} {} (~)", preview::clock(Some(end))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fit {
    Fits,
    Tight {
        speech: f64,
        before: f64,
        next_line: bool,
    },
    Overruns {
        past: f64,
        sped_up: bool,
    },
}

pub fn fit_warning(fit: Fit) -> Option<String> {
    match fit {
        Fit::Fits => None,
        Fit::Tight {
            speech,
            before,
            next_line,
        } => {
            let edge = if next_line {
                "the next line"
            } else {
                "the clip ends"
            };
            Some(format!(
                "\u{26a0} ~{speech:.0} s of speech, {before:.0} s before {edge}"
            ))
        }
        Fit::Overruns { past, sped_up } => {
            let remedy = if sped_up {
                "moved earlier and sped up"
            } else {
                "moved earlier"
            };
            Some(format!(
                "\u{26a0} this clip's lines run {past:.0} s past it \u{2014} the render will have them {remedy}"
            ))
        }
    }
}

/// §1 (`warnings … and the row marked red`): `Fits` is the only state that keeps the row's normal colour.
pub fn row_is_red(fit: Fit) -> bool {
    fit_warning(fit).is_some()
}

/// §F4.3 (`default 15 chars/s, clamped 8..28`): the band a speech estimate may be taken at, so a wild measurement cannot make a line look free or endless.
pub const CHAR_RATE_MIN: f64 = 8.0;
pub const CHAR_RATE_MAX: f64 = 28.0;

/// A rate clamped into [`CHAR_RATE_MIN`]..=[`CHAR_RATE_MAX`]; non-positive and non-finite rates fall back to the default.
pub fn char_rate_clamped(rate: f64) -> f64 {
    if !rate.is_finite() || rate <= 0.0 {
        return SPEECH_CHARS_PER_SECOND;
    }
    rate.clamp(CHAR_RATE_MIN, CHAR_RATE_MAX)
}

/// §F4.3: the rate a take MEASURED at (characters over its wav's seconds), through [`char_rate_clamped`]. `None` with nothing
/// to measure: no take, a caption, an empty line, or a zero-audio wav.
pub fn measured_char_rate(chars: usize, wav_seconds: f64) -> Option<f64> {
    if chars == 0 || !wav_seconds.is_finite() || wav_seconds <= 0.0 {
        return None;
    }
    Some(char_rate_clamped(chars as f64 / wav_seconds))
}

/// §F4.3's page-only half: on the Narrate page a line with no take is estimated as characters over the rate, so the row can warn.
/// In the RENDER the same line has length 0.0 ([`crate::produce_render::fit`]'s input), which this does not touch.
pub fn page_speech_seconds(chars: usize, rate: f64) -> f64 {
    chars as f64 / char_rate_clamped(rate)
}

/// F4.3 S1–S4 for the ROW: run the SAME ladder the render runs ([`crate::produce_render::fit`]), so a row cannot promise a remedy
/// the render would not apply. `clip_len` is the render's `room`; `lines` are that ONE clip's `(entry index, clip-relative at, speech)`.
/// The rung that fired is read out of the ladder's log strings: no log and slack → [`Fit::Fits`]; no log but the whole tail is
/// needed → [`Fit::Tight`]; the slide logged → [`Fit::Overruns`] with `sped_up: false`; the tempo rung logged → `sped_up: true`.
/// `past` is what the ladder could NOT remove at the clip's end, floored at 0.0.
pub fn mirror_fit(clip_len: f64, lines: &[(usize, f64, f64)]) -> Fit {
    if lines.is_empty() {
        return Fit::Fits;
    }
    let (placed, logs) = crate::produce_render::fit(lines, clip_len);
    let sped_up = logs.iter().any(|line| line.contains("sped up"));
    let slid = logs
        .iter()
        .any(|line| line.contains("moved") && line.contains("earlier"));
    if sped_up || slid {
        return Fit::Overruns {
            past: overrun_after(placed.iter(), clip_len).max(0.0),
            sped_up,
        };
    }
    drop(placed);
    let speech: f64 = lines.iter().map(|(_, _, speak)| speak).sum();
    // The PACKED run, as the render measures its need off packed starts; `at + speech` would disagree about where the last word finishes.
    let packed = crate::produce_render::packed(lines);
    let end = crate::produce_render::run_end(&packed);
    let slack = clip_len - (end + SPEECH_TAIL_SECONDS);
    if slack < 0.0 {
        // Two reasons a run lands tight, worded differently in §1: the CLIP ends before the last breath is spent, or a later line was
        // pushed off its placement by the line in front (packed start past `at`).
        let pushed = packed
            .iter()
            .zip(lines.iter())
            .any(|(landed, (_, at, _))| landed.at > *at + 1e-9);
        return Fit::Tight {
            speech,
            before: clip_len - end,
            next_line: pushed,
        };
    }
    Fit::Fits
}

/// What is left over the clip's end after the ladder's last rung (speed to [`crate::produce_render::MAX_TEMPO`], slide to the
/// lead), re-derived from `fit`'s placements so it cannot drift from what the render did.
fn overrun_after<'a>(
    placed: impl Iterator<Item = &'a crate::produce_render::Placed>,
    clip_len: f64,
) -> f64 {
    placed
        .map(|line| line.at + line.speech / line.tempo.max(1.0))
        .fold(0.0_f64, f64::max)
        + SPEECH_TAIL_SECONDS
        - clip_len
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Audition {
    Pause,
    OwnAudio { silent: bool },
    Caption { bare: bool },
    Busy,
    Speak { from: f64 },
}

/// §1 (`▶ speak this line — audition from 3 s ahead where those seconds are its own …`): the lead-in is "its own" only when the
/// line above has finished by then, so `prev_speaks_at` sets the floor.
pub fn audition(
    row_live: bool,
    blank: bool,
    caption: bool,
    busy: bool,
    line_no: usize,
    prev_speaks_at: f64,
    at: f64,
) -> (Audition, Option<String>) {
    if row_live {
        return (Audition::Pause, None);
    }
    // §1's order puts a synthesis in flight ahead of the other refusals.
    if busy {
        return (
            Audition::Busy,
            Some(format!(
                "still speaking line {line_no} for the first time \u{2014} \u{23f9} gives up on it"
            )),
        );
    }
    if blank {
        return (
            Audition::OwnAudio { silent: false },
            Some(format!(
                "clip {line_no} has no line \u{2014} playing it on its own audio"
            )),
        );
    }
    if caption {
        return (
            Audition::Caption { bare: false },
            Some(format!(
                "line {line_no} is a caption \u{2014} read, never spoken; playing its moment"
            )),
        );
    }
    (
        Audition::Speak {
            from: (at - AUDITION_LEAD_SECONDS).max(prev_speaks_at),
        },
        None,
    )
}

/// §1's other half of the wordless and caption cases: with no recording covering the clip there is no game sound either, so the sentence extends (inventory C.4).
pub fn audition_without_recording(case: Audition, line_no: usize) -> (Audition, Option<String>) {
    match case {
        Audition::OwnAudio { .. } => (
            Audition::OwnAudio { silent: true },
            Some(format!(
                "clip {line_no} has no line \u{2014} playing it on its own audio \u{2026}and no recording covers it"
            )),
        ),
        Audition::Caption { .. } => (
            Audition::Caption { bare: true },
            Some(format!(
                "line {line_no} is a caption \u{2014} read, never spoken; playing its moment \u{2026}and no \
                 recording covers its clip"
            )),
        ),
        other => (other, None),
    }
}

pub fn spoken_alone(entry_no: usize) -> String {
    format!("entry {entry_no} \u{2014} no recording covers this clip, so the line plays on its own")
}

/// §1's row order, which the window builds: time entry (14), status (15, 21), ▶ speak (16), ↻ re-roll (17), ＋ add below (18),
/// 🗑 remove (19) right-aligned on the head, then the text box (20) below.
pub const ROW_CONTROLS: [&str; 7] = [
    "time",
    "status",
    "speak",
    "re-roll",
    "add-below",
    "remove",
    "text",
];

/// §1 / inventory C.5 (`within 1 s of an existing line → jump to it`).
pub const ADD_NEAR_SECONDS: f64 = 1.0;

/// §C.2 (`chars / measured rate (default 15 chars/s…)`): how long a line runs with no take, used for "speaking until mm:ss".
pub const SPEECH_CHARS_PER_SECOND: f64 = 15.0;

/// §C.2's `tail` (P.eng.narrationTailSeconds): the silence after the last word, so "speaking until mm:ss" names the end, not the last consonant.
pub const SPEECH_TAIL_SECONDS: f64 = 0.2;

fn same_clip(clip: &Seg, entry: &Entry) -> bool {
    clip.s == entry.s && clip.e == entry.e
}

fn clip_at(clips: &[Seg], at: f64) -> Option<&Seg> {
    clips.iter().find(|seg| seg.s <= at && at < seg.e)
}

/// How long a line runs with nothing to measure: §C.2's 15 chars/s plus the tail. `0.0` for a caption or a deliberate silence.
fn estimated_speech(entry: &Entry) -> f64 {
    if entry.text.is_empty() || !entry.pos.is_empty() {
        return 0.0;
    }
    entry.text.chars().count() as f64 / SPEECH_CHARS_PER_SECOND + SPEECH_TAIL_SECONDS
}

fn speech_end(entry: &Entry) -> f64 {
    entry.s + entry.at + estimated_speech(entry)
}

/// The spoken (not captioned, not silent) line **on the clip the playhead stands on** whose window holds that second; the record stores the bounds it was written against.
fn speaking_at<'a>(clip: &Seg, entries: &'a [Entry], at: f64) -> Option<&'a Entry> {
    entries.iter().find(|entry| {
        same_clip(clip, entry)
            && !entry.text.is_empty()
            && entry.pos.is_empty()
            && at >= entry.s + entry.at
            && at < line_end(entries, entry)
    })
}

/// Until when this line holds the clip's seconds: its own spoken length, cut short by the next line's start (windows never overlap, §5).
fn line_end(entries: &[Entry], entry: &Entry) -> f64 {
    let started = entry.s + entry.at;
    let mut end = speech_end(entry);
    // The minimum over the later lines, since a hand-edited file need not be sorted.
    for other in entries.iter().filter(|other| {
        other.s == entry.s
            && other.e == entry.e
            && other.s + other.at > started
            && !other.text.is_empty()
    }) {
        end = end.min(other.s + other.at);
    }
    end
}

/// §1's transport ＋: refused when there is nothing to narrate, a line is already here, or one is still speaking (F4.7's order). `Ok` is the new line's second.
pub fn add_at_playhead(at: f64, segs: &[Seg], entries: &[Entry]) -> Result<f64, String> {
    let Some(clip) = clip_at(segs, at) else {
        return Err(
            "the playhead is between clips \u{2014} the cut has nothing to narrate here"
                .to_string(),
        );
    };
    if let Some(start) = near_line_start(at, entries) {
        return Err(format!(
            "a line already starts here \u{2014} edit it, or move the playhead (it is at {})",
            preview::clock(Some(start))
        ));
    }
    // §1's `a line is speaking here until mm:ss — add after it`: F4.7 asks this of every line on the clip, not only the last. Until
    // when is the next line's second or this line's estimate (§C.2), whichever is first; the refusal names the second to add after.
    let speaking = speaking_at(clip, entries, at);
    if let Some(speaking) = speaking {
        return Err(format!(
            "a line is speaking here until {} \u{2014} add after it",
            preview::clock(Some(line_end(entries, speaking)))
        ));
    }
    // §F4.7's free add is "a line HERE": the playhead second, never in the clip's last second (the bound §F4.7 states for a move).
    Ok(at.clamp(clip.s, (clip.e - 1.0).max(clip.s)))
}

/// The start of the line beginning within [`ADD_NEAR_SECONDS`] of `at` (§F4.7's node J: "within 1 s of a line → jump to it").
/// One function, so the refusal's sentence and the jump agree on which line was meant.
pub fn near_line_start(at: f64, entries: &[Entry]) -> Option<f64> {
    entries
        .iter()
        .enumerate()
        .filter(|(index, entry)| {
            crate::narrate_edit::silent_marker_on(entry.s, entries) != Some(*index)
        })
        .find(|(_, entry)| (entry.s + entry.at - at).abs() < ADD_NEAR_SECONDS)
        .map(|(_, entry)| entry.s + entry.at)
}

/// §1's row ＋ (`starting where its audio ends`): [`ADD_BELOW_SECONDS`] after the line above, refused when the clip ends first;
/// same last-second rule as [`tag_moves_line`], so `audio_end + 0.5` must land at least a second short of the clip's end.
pub fn add_below(audio_end: f64, clip_end: f64) -> Result<f64, String> {
    let at = audio_end + ADD_BELOW_SECONDS;
    if at > clip_end - 1.0 {
        return Err("no room after this line \u{2014} the clip ends first".to_string());
    }
    Ok(at)
}

/// §1's 🗑 (`remove this line — the video plays its own audio here instead`): the longer sentence is owed only when this was the clip's last line.
pub fn remove_line(clip_has_other_line: bool, clip_s: f64) -> String {
    if clip_has_other_line {
        return "line removed".to_string();
    }
    format!(
        "line removed \u{2014} the clip at {} plays its own audio",
        cut_hear::scene_clock(clip_s)
    )
}

pub fn re_roll_refusal(clip_no: usize, has_line: bool) -> Option<String> {
    (!has_line).then(|| format!("clip {clip_no} has no line to re-roll"))
}

pub fn time_field(shown: f64) -> String {
    preview::clock(Some(shown))
}

/// §1 (`a time in a gap is refused and written back`): names the typed second, the clip's bounds and where the line stayed.
pub fn time_refused(requested: f64, clip_s: f64, clip_e: f64, at: f64) -> String {
    format!(
        "{} is outside the cut \u{2014} this line stays in its clip ({}\u{2013}{}), at {}",
        tools::mm_ss(requested),
        tools::mm_ss(clip_s),
        tools::mm_ss(clip_e),
        preview::clock(Some(at))
    )
}

/// §1 (`Inputs: "N clip(s) · mm:ss[ · ⚠ N clip(s) unwritten, N line(s) off the cut][ · no timeline]"`). Counts are
/// [`crate::shell::refit`]'s: unwritten = no entry and not deliberately silent; off the cut = on video the cut no longer has.
pub fn inputs_readout(
    clips: usize,
    length: f64,
    unwritten: usize,
    off_cut: usize,
    has_timeline: bool,
    has_cut: bool,
) -> String {
    if !has_cut {
        return "no cut yet \u{2014} build one on the Cut step".to_string();
    }
    let mut line = format!("{} clip(s) \u{b7} {}", clips, tools::mm_ss(length));
    if unwritten > 0 || off_cut > 0 {
        line.push_str(&format!(
            " \u{b7} \u{26a0} {unwritten} clip(s) unwritten, {off_cut} line(s) off the cut"
        ));
    }
    if !has_timeline {
        line.push_str(" \u{b7} no timeline");
    }
    line
}

/// §1 (`Outputs: "narrate/ — narration.json, the voice reference and the synthesis cache"`): the page's Outputs READOUT wording.
pub fn outputs_readout() -> &'static str {
    "narrate/ \u{2014} narration.json, the voice reference and the synthesis cache"
}

/// The Outputs folder button's tooltip: what pressing it DOES, deliberately not [`outputs_readout`] (it looked like a copy of a tooltip).
pub const OUTPUTS_FOLDER_TIP: &str = "Open the narration folder";

/// §1 **8** ▶ (prototype `takeQueue`): the takes from the red bar `at` on, the one the bar stands inside trimmed to begin there; with
/// none after it, the recording from the bar to its end (`false`: not takes). Nothing when the bar is at or past the end.
pub fn take_queue(takes: &[(f64, f64)], at: f64, duration: f64) -> (Vec<(f64, f64)>, bool) {
    let queued: Vec<(f64, f64)> = takes
        .iter()
        .filter(|(_, end)| *end > at)
        .map(|(start, end)| (start.max(at), *end))
        .collect();
    if !queued.is_empty() {
        return (queued, true);
    }
    if at < 0.0 || at >= duration {
        return (Vec::new(), false);
    }
    (vec![(at, duration)], false)
}
