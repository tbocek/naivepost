//! §07-narrate#1-screen — what the Narrate page shows and what a press would do.
//!
//! The page renders this module and forwards to it; nothing here imports gtk, so which row is greyed,
//! what its tooltip says and where a take's seconds go can be answered without a display
//! (spec/00-principles.md §5). §1's numbered widgets are the inventory's rows in spec/inventory/narrate.md
//! §A, which the spec calls normative for tooltips.
//!
//! What this module does NOT own, and where it reads from instead:
//!
//! * the record — [`crate::narration::Entry`] / [`crate::narration::Narration]`, its `sort`, `has_line`,
//!   `is_silent` and the placement `at` — lives in [`crate::narration`], which is the file format;
//! * whether a line's end time is estimated comes from whether its take exists, so it is asked of
//!   [`crate::narration::tts_file`] by the caller and handed to [`status`] as `wav_exists`;
//! * the shell's one-line Inputs summary is [`crate::shell::inputs`] (pinned by other rounds' tests);
//!   [`inputs_readout`] below is the page's own fuller row, the same split §05-cut#1-screen set for Cut;
//! * ▶ and its refusal are [`crate::run::Step::Narrate`]'s, the clock face is [`crate::preview::clock`],
//!   `mm:ss` is [`crate::tools::mm_ss`] and a session second in a sentence is [`crate::cut_hear::scene_clock`];
//! * every number §10 lists by area lives once here, in the module whose rule uses it, and
//!   [`crate::params`] catalogues them.
//!
//! Fitting a line to its clip — extending, sliding, speeding — is F4.3's and belongs to the render. What
//! lives here is only what a row says about it (§1's two warnings), which is why [`fit`] takes the numbers
//! rather than computing them.

use crate::cut::Seg;
use crate::narration::Entry;
use crate::{cut_hear, preview, tools};

/// The toolbar's groups, left to right (§1's numbers 3–8: transport, preview volume, who speaks, the take
/// band's ＋ − ▶, the sample row, pitch). The window builds one box per entry in this order, which is what
/// makes the order a fact a test can hold rather than a fact about how a file happens to read.
///
/// The shot lays these out as two rows rather than one: §1's 3–6 (‹‹ ▶ ›› ＋, the slider and its clock, the shared
/// volume) sit under the picture, and 7–8 (who speaks, `Add file…` and the take band's ＋ − ▶) in the voice column
/// under that. The sample row (§1's 11–13: the sentence, ▶ ⏹ ⟳, pitch) is a third row below the band. Reading left
/// to right, top to bottom, that is this order — which is why the list runs transport → volume → voice → takes →
/// sample → pitch rather than grouping the two ＋ − ▶ pairs together.
pub const TOOLBAR_GROUPS: [&str; 6] = ["transport", "volume", "voice", "takes", "sample", "pitch"];

/// §1's Prototype note: the prototype's ＋ is the full-width sign U+FF0B, which only CJK fonts carry and
/// which draws as a missing-glyph box on a machine without one. The rewrite uses the named icon, which every
/// theme has — the transport's ＋ already did (inventory §A.4).
pub const ADD_LINE_ICON: &str = "list-add";

/// ‹‹ and ›› (§1's numbers 3 and 5): three seconds, snapped to the cut by the caller.
pub const BACK_SECONDS: f64 = 3.0;

/// Where a line added below another starts: half a second after the one above ends (inventory C.5). A line
/// butted flush against its neighbour reads as one word continuing, which is not what a second ＋ means.
pub const ADD_BELOW_SECONDS: f64 = 0.5;

/// §1 (`audition from 3 s ahead where those seconds are its own`): how far the row's ▶ reaches back. Three
/// seconds is enough to hear a line arrive rather than to have arrived, and it is the same reach as ‹‹.
pub const AUDITION_LEAD_SECONDS: f64 = 3.0;

/// A take's label ("4.2s") only fits in a band this wide; narrower and the words would be cut mid-digit.
pub const TAKE_LABEL_MIN_PX: f64 = 34.0;

/// P.eng.takeMinSeconds — the shortest hand-picked take. Shorter than this the model is cloned from a
/// breath, which is a noise rather than a voice.
pub const TAKE_MIN_SECONDS: f64 = 0.4;

/// P.eng.takeMinSeconds, read as a function so a caller cannot pick its own floor.
pub fn take_min() -> f64 {
    TAKE_MIN_SECONDS
}

/// §1 (`pitch −6..+6 semitones`): the slider's ends. Beyond these the reference stops being the person on
/// the recording, which is the point of the tooltip below.
pub const PITCH_MIN_SEMITONES: f64 = -6.0;

/// §1: the top of the same slider.
pub const PITCH_MAX_SEMITONES: f64 = 6.0;

/// Half a semitone: finer and the slider cannot be aimed by hand, coarser and the useful range is skipped.
pub const PITCH_STEP: f64 = 0.5;

/// §1 (`applied 400 ms after the last move`) — spec/10-parameters.md:133's `typing debounce 400 ms`. A shift
/// re-cuts the reference with rubberband, so applying it per pixel of a drag would rebuild it dozens of times.
pub const PITCH_APPLY_MS: u64 = 400;

/// The pitch slider's own limits, clamped rather than refused: a stored value outside them (an edited
/// `narrate/pitch.txt`) still means "shift it that way", and the reference is built at the nearest end.
pub fn clamp_pitch(semitones: f64) -> f64 {
    semitones.clamp(PITCH_MIN_SEMITONES, PITCH_MAX_SEMITONES)
}

/// Is there a shift at all? Zero writes no `narrate/pitch.txt` and no shifted copy — the reference is used as
/// cut, which keeps every take cached before the slider existed valid.
pub fn pitch_is_shifted(semitones: f64) -> bool {
    semitones != 0.0
}

// --- The take band (§1's number 10, inventory §A.7) --------------------------------------------------

/// spec/10-parameters.md:133 `Narrate take band: ruler 12 …`.
pub const BAND_RULER_PX: f64 = 12.0;

/// spec/10-parameters.md:133 `… lane 56 …`.
pub const BAND_LANE_PX: f64 = 56.0;

/// spec/10-parameters.md:133 `… max 200 px/s`. Shallower than the Cut page's 240 because this band is a few
/// minutes of one recording, not a session, and its wheel is for aiming at a sentence.
pub const BAND_MAX_PPS: f64 = 200.0;

/// spec/10-parameters.md:133 `… click slop 3 px`: under this a press-release is a click on the red bar.
pub const CLICK_SLOP_PX: f64 = 3.0;

/// F4.5's tick: how often the preview follows the players. Fast enough that a person never sees the picture
/// and the clock disagree, slow enough not to be a second clock of its own.
pub const TICK_MS: u64 = 100;

/// §1's slider (`Drag to seek; paused, the wheel steps a frame at a time`; inventory §A.4's `debounced 120 ms`):
/// how long a drag is quiet before the seek is issued. One seek per gesture rather than one per pixel — the cut is
/// played from a file and every seek reloads it, so an undebounced slider makes the drag itself stutter.
pub const SEEK_DEBOUNCE_MS: u64 = 120;

/// A take labelled with its duration alone (§1's `"4.2s"`): the seconds are the only thing two takes differ in.
pub fn take_label(seconds: f64) -> String {
    format!("{:.1}s", seconds)
}

/// What one press-release on the band means: a drag selects seconds, a click puts the red bar down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TakePress {
    SetBar,
    Select { from: f64, to: f64 },
}

/// A press that travelled less than [`CLICK_SLOP_PX`] is a click: the person aimed at a second, not at a span.
pub fn take_press(pressed_x: f64, released_x: f64, start: f64) -> TakePress {
    let travel = (released_x - pressed_x).abs();
    if travel <= CLICK_SLOP_PX {
        return TakePress::SetBar;
    }
    // Left to right, whichever way the pointer went: a take's seconds have no direction.
    let (near, far) = if released_x > pressed_x {
        (pressed_x, released_x)
    } else {
        (released_x, pressed_x)
    };
    TakePress::Select { from: start + near, to: start + far }
}

/// Does this selection reach [`TAKE_MIN_SECONDS`]? Asked before the take is made so the answer is the sentence
/// below rather than a take nobody wanted.
pub fn can_make_take(selection: (f64, f64)) -> bool {
    selection.1 - selection.0 >= TAKE_MIN_SECONDS
}

/// Would these seconds be dropped by the band's own clean-up? Asked before `－` so removing nothing says so.
pub fn take_seconds_too_short(seconds: f64) -> bool {
    seconds < TAKE_MIN_SECONDS
}

/// §1 / inventory §A.7 (`that is N s — a take has to be at least 0.4 s`).
pub fn take_too_short(seconds: f64) -> String {
    format!(
        "that is {seconds:.1} s \u{2014} a take has to be at least {:.1} s",
        TAKE_MIN_SECONDS
    )
}

/// The band's clean-up (inventory §A.7): sorted, no take shorter than [`TAKE_MIN_SECONDS`], touching or
/// overlapping takes merged. Sorting and merging are what let `－` split a take without leaving two edges at
/// the same second for the next drag to trip over.
fn clean(mut takes: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    takes.retain(|(from, to)| to - from >= TAKE_MIN_SECONDS);
    takes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(takes.len());
    for (from, to) in takes {
        match merged.last_mut() {
            // Touching counts as overlapping: two takes with no gap between them are one take.
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => merged.push((from, to)),
        }
    }
    merged
}

/// `＋`: these seconds join the takes. Kept only if they reach [`TAKE_MIN_SECONDS`] — the same floor a hand-picked
/// take has to clear, so a press of ＋ on a 0.2 s sliver cannot smuggle one in past [`can_make_take`].
pub fn add_takes(existing: &[(f64, f64)], span: (f64, f64)) -> Vec<(f64, f64)> {
    let mut takes = existing.to_vec();
    if can_make_take(span) {
        takes.push(span);
    }
    clean(takes)
}

/// `－`: these seconds come out of the takes, which splits a take when they fall inside it. Subtracting rather
/// than deleting whole takes is what lets a person keep a good sentence and drop one cough in it.
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

/// §1 (`With no takes the seconds are chosen for you.`): the band's own sentence about having none. It is a
/// promise rather than a warning because F4.6's automatic pick is a good answer most of the time.
pub fn band_status(takes: &[(f64, f64)]) -> &'static str {
    if takes.is_empty() {
        "With no takes the seconds are chosen for you."
    } else {
        ""
    }
}

// --- The voice picker (§1's numbers 7, 9, 11–13) --------------------------------------------------------

/// §1 (`unlabelled dropdown`): the id of the first row. `captions` is what short-circuits everything that
/// speaks (F4.8), so it is an id and not a label with no id behind it.
pub const CAPTIONS: &str = "captions";

/// One row of the dropdown: the id stored in `narrate/voice.txt` and the words on the row.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceOption {
    pub id: String,
    pub label: String,
}

/// §1 (`No audio — captions only; Narrator 1..4 — the recording tagged on Prepare; every .wav in the voices
/// folder`): the rows, in that order. `narrators` is how many slots Prepare has a recording for (slot 1
/// always, 2..4 when tagged), and `files` are the voices folder's .wav names. The dropdown itself stays
/// unlabelled — the shot shows it carrying the chosen voice as its own face (`Narrator 1 — 2026-09-16 18-43-01.mkv`)
/// with no label widget beside it — and `Add file…` sits to its right, so nothing here carries a label.
pub fn voice_options(narrators: usize, files: &[&str]) -> Vec<VoiceOption> {
    let mut options = vec![VoiceOption {
        id: CAPTIONS.to_string(),
        label: "No audio \u{2014} captions only".to_string(),
    }];
    for slot in 1..=narrators {
        options.push(VoiceOption {
            id: format!("narrator{slot}"),
            label: format!("Narrator {slot} \u{2014} cut from the recording"),
        });
    }
    options.extend(files.iter().map(|name| VoiceOption {
        id: (*name).to_string(),
        label: (*name).to_string(),
    }));
    options
}

/// §1 / inventory §A.6, the dropdown's tooltip quoted verbatim. It says what every line is made from, which
/// is the one thing a person picking a voice has to understand before they have written anything.
pub fn sample_tip() -> String {
    "Who speaks the narration. Every line is spoken by cloning the selected recording; switching voices keeps \
     what you already synthesized"
        .to_string()
}

/// The lines column's own empty state: what the page says where its rows go before anyone has written a
/// line. A blank list box reads as a panel that failed to fill, which is not what it means.
pub fn no_lines_note() -> &'static str {
    "no narration lines yet \u{2014} \u{ff0b} puts one at the playhead"
}

/// §1 (`Shift the reference recording before it is cloned — a different speaker, not the same one transposed`).
/// The distinction matters because the slider is next to a pitch control for the *video* on another page.
pub fn pitch_tip() -> String {
    "Shift the reference recording before it is cloned \u{2014} a different speaker, not the same one transposed"
        .to_string()
}

/// §1 (`Add file…` / inventory: `Copy one recording into the voices folder and use it`). Copies, because a
/// voice that lives inside the project keeps working when the recording is untagged or moved.
pub fn add_file_tip() -> String {
    "Copy one recording into the voices folder and use it".to_string()
}

/// §1's default sample sentence. Spoken in the selected voice, it is the shortest test that shows whether a
/// cloned voice will be listenable for minutes rather than seconds.
pub const DEFAULT_SAMPLE: &str = "This is the voice the narration will be spoken in. It should stay clear and \
                                  easy to follow for a couple of minutes.";

// --- The transport's clock (§1's number 5) ---------------------------------------------------------------

/// §1 (`clock "session · cut/length"`): where the line is on the recording, then where it is in the finished
/// video. Two clocks because the edit removed seconds and the person needs both to aim at a picture and at a
/// moment in the output. Built from [`tools::mm_ss`]; the tenths belong to [`preview::clock`] on the Cut page.
pub fn clock_line(session: f64, cut_at: f64, length: f64) -> String {
    format!(
        "{} \u{b7} {}/{}",
        tools::mm_ss(session),
        tools::mm_ss(cut_at),
        tools::mm_ss(length)
    )
}

/// §1 (`paused, wheel steps a frame`): while ▶ is running the wheel has nothing to step to, and stepping it
/// under a moving picture would fight the tick.
pub fn wheel_steps_a_frame(playing: bool) -> bool {
    !playing
}

// --- The text box (§1's number 20, inventory §B.4) --------------------------------------------------------

/// What a leading tag means. A placement is a caption and not an emotion because the viewer reads it; the two
/// cannot both be true of one line, which is why this is an enum and not two booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxRole {
    None,
    Caption,
    Emotion,
}

/// A box's text read apart: the words, the tag inside the brackets without its `@N`, the second the tag asked
/// for, and what the tag was.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxText {
    pub body: String,
    pub tag: String,
    pub at: Option<f64>,
    pub role: BoxRole,
}

/// §1 (`[emotion] words`, `[top|center|bottom] words` for captions; may carry the line's second in the tag as
/// `@N`). The five placement spellings are inventory §B.4's, `centre` and `middle` included: a person typing a
/// caption should not have to remember which of the four spellings this app chose.
pub fn parse_box(text: &str) -> BoxText {
    let trimmed = text.trim_start();
    let mut parsed = BoxText { body: text.to_string(), tag: String::new(), at: None, role: BoxRole::None };
    if !trimmed.starts_with('[') {
        return parsed;
    }
    let Some(end) = trimmed.find(']') else {
        return parsed;
    };
    let raw = &trimmed[1..end];
    parsed.body = trimmed[end + 1..].trim_start().to_string();

    // A trailing `@N` is the second, not part of a name: no emotion or placement is spelled with an @.
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
    let placement = matches!(first.as_str(), "top" | "center" | "centre" | "middle" | "bottom");
    // §1 / F4.7: a placement is a caption and clears the emotion, so an entry never carries both.
    if placement {
        parsed.role = BoxRole::Caption;
        parsed.tag = String::new();
    } else if !tag.is_empty() {
        parsed.role = BoxRole::Emotion;
    }
    parsed
}

/// The box as the page writes it back (§1: `write-only: the time field owns the number, the box never prints
/// it back`). So `@N` is dropped here even though [`parse_box`] reads it: printing it would put the same second
/// in two widgets, and only one of them would be the one that moved the line.
pub fn write_box(entry: &Entry) -> String {
    let tag = if !entry.pos.is_empty() {
        entry.pos.clone()
    } else {
        entry.emotion.clone()
    };
    if entry.text.is_empty() {
        // A deliberately silent clip's box stays empty rather than showing a bare tag nobody asked for.
        return String::new();
    }
    if tag.is_empty() {
        return entry.text.clone();
    }
    format!("[{tag}] {}", entry.text)
}

/// §1 (`[excited @65] Weee` moves the line, clamped out of the clip's last second`). The clamp is the same rule
/// as §5's `at never in the last second`: a line starting there has no room to be heard before the cut moves on.
/// Returns the placement **inside the clip** — so a tag naming a second outside it moves the line nowhere and says
/// so — and the `bool` is "the tag asked for a second that could not be kept", which the page reports rather than
/// hushes. `at` is the line's current placement, since a refused move leaves it exactly where it was.
pub fn tag_moves_line(clip_s: f64, clip_e: f64, requested: f64, at: f64) -> (f64, bool) {
    let last = (clip_e - 1.0).max(clip_s);
    if !(clip_s..=last).contains(&requested) {
        return (at, true);
    }
    (requested, false)
}

/// §1 / inventory §A.5 (`monospace, word wrap, 1–3 lines tall`): the box grows to three and then scrolls. A box
/// that grew forever would push the rows below it off the page while someone was typing in one of them.
pub fn tooltip_line_count(tall: bool) -> (i32, i32) {
    if tall {
        (1, 3)
    } else {
        (1, 1)
    }
}

/// §1 (`Tooltip explains the tag, judge vs weighted mixes, the eight base emotions`). P.policy.narrationEmotionTag
/// — the tag itself, which has no row of its own in §10 and is spelled by spec/prompts/system.md:72. The order is
/// that line's, so the tooltip and the prompt the model reads name the same eight in the same order.
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

/// §1's `the eight base emotions`, in spec/prompts/system.md:72's order.
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

// --- The row's status and warnings (§1's numbers 14, 15, 21) -------------------------------------------------

/// What the row says after its time: §1's `– end time`, `(~)` while estimated, `(no line …)` and `(caption …)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowStatus {
    Silent,
    Caption,
    Estimated(f64),
    Known(f64),
}

/// §1: which of the four. The estimated flag is not this module's guess at a speaking rate — it is whether the
/// take exists yet, which the caller asks of [`crate::narration::tts_file`] and hands over as `wav_exists`. A
/// caption never has a take, so it answers before that question rather than reading as an estimate forever.
pub fn status(entry: &Entry, wav_exists: bool) -> RowStatus {
    // The line starts where §4 stores it — the clip's head plus the placement inside it.
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

/// §1's four spellings. The dash is U+2013 as the inventory writes it and the tenths come from
/// [`preview::clock`], so a row and the Cut page's clock cannot disagree about what a second looks like.
pub fn status_line(status: RowStatus) -> String {
    match status {
        RowStatus::Silent => "(no line \u{2014} this clip plays on its own audio)".to_string(),
        RowStatus::Caption => "(caption \u{2014} the viewer reads it; never spoken)".to_string(),
        RowStatus::Known(end) => format!("\u{2013} {}", preview::clock(Some(end))),
        RowStatus::Estimated(end) => format!("\u{2013} {} (~)", preview::clock(Some(end))),
    }
}

/// How the row marks a line that does not fit: §1's two warnings. The arithmetic behind the numbers is F4.3's —
/// this is only what the row says about it, which is why [`fit`] is handed the measurements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fit {
    Fits,
    Tight { speech: f64, before: f64, next_line: bool },
    Overruns { past: f64, sped_up: bool },
}

/// §1's `⚠ ~N s of speech, M s before the clip ends|the next line` and
/// `⚠ this clip's lines run N s past it — the render will have them moved earlier[ and sped up]`.
pub fn fit_warning(fit: Fit) -> Option<String> {
    match fit {
        Fit::Fits => None,
        Fit::Tight { speech, before, next_line } => {
            let edge = if next_line { "the next line" } else { "the clip ends" };
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

/// §1 (`warnings … and the row marked red`): both warnings mark the row, since either one means the render will
/// not do what the row's time says. `Fits` is the only state that leaves the row its normal colour.
pub fn row_is_red(fit: Fit) -> bool {
    fit_warning(fit).is_some()
}

// --- The row's ▶ (§1's number 16, inventory C.4) -------------------------------------------------------------

/// What a press of the row's ▶ means. Six answers because the same button is asked to do six different things
/// depending on what it is sitting next to, and §1 wants each one said in its own words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Audition {
    Pause,
    OwnAudio { silent: bool },
    Caption { bare: bool },
    Busy,
    Speak { from: f64 },
}

/// §1 (`▶ speak this line — audition from 3 s ahead where those seconds are its own, then continue down the
/// cut; five other cases, each in its own words`). The lead-in is only "its own" when the line above has finished
/// speaking by then, so `prev_speaks_at` (the second the previous line's audio ends) sets the floor rather than a
/// fixed 3 s being subtracted and hoping.
pub fn audition(
    row_live: bool,
    blank: bool,
    caption: bool,
    busy: bool,
    line_no: usize,
    prev_speaks_at: f64,
    at: f64,
) -> (Audition, Option<String>) {
    // A press while this row is speaking is the same gesture as ⏸ over the transport: it stops.
    if row_live {
        return (Audition::Pause, None);
    }
    // §1's order puts a synthesis in flight ahead of the other refusals: the person is being told to wait, and
    // whether the line has words is not what is holding them up.
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
        Audition::Speak { from: (at - AUDITION_LEAD_SECONDS).max(prev_speaks_at) },
        None,
    )
}

/// §1's other half of the wordless and caption cases: when no recording covers the clip there is no game sound to
/// play either, so the sentence grows. Inventory C.4 gives these as one sentence plus a tail (`…and no recording
/// covers it`), which is why they extend the first rather than replace it.
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

/// §1 (`A clip no recording covers speaks alone off the tick`): with nothing under it the line is all there is to
/// hear, so it plays without waiting for a picture. The tick is F4.8's narration switch, not this module's state.
pub fn speaks_alone_off_the_tick(no_recording: bool) -> bool {
    no_recording
}

/// §1 (`"synthesizing… (first line after a cold start also loads the model)"`). The parenthetical is the point: a
/// first line takes far longer than every line after it, and without that sentence it looks like a hang. §1 gives
/// this one no line number, unlike its neighbours ("line N ready", "entry N — …"), so it takes none either.
pub fn speaking_line() -> String {
    "synthesizing\u{2026} (first line after a cold start also loads the model)".to_string()
}

/// §1 (`"entry N — no recording covers this clip, so the line plays on its own"`).
pub fn spoken_alone(entry_no: usize) -> String {
    format!(
        "entry {entry_no} \u{2014} no recording covers this clip, so the line plays on its own"
    )
}

/// §1 (`failure "synthesis failed — see log"`): named and local, per spec/00-principles.md. The log has the
/// server's reason; the status line only says where to look.
pub fn synthesis_failed() -> &'static str {
    "synthesis failed \u{2014} see log"
}

// --- Adding, moving and removing lines (§1's numbers 4, 17–19) -------------------------------------------------

/// §1's row order, which is what the window builds and what a test can hold: time entry (14), status with its end
/// time, `(~)` and warnings (15, 21), then ▶ speak this line (16), ↻ re-roll (17), ＋ add below (18), 🗑 remove (19) —
/// the four buttons right-aligned on the row's head — with the text box (20) below them. The shot draws exactly this,
/// in this order, on every one of its four rows.
pub const ROW_CONTROLS: [&str; 7] = ["time", "status", "speak", "re-roll", "add-below", "remove", "text"];

/// §1 / inventory C.5 (`within 1 s of an existing line → jump to it`): a press that close to a line means the
/// same line, not a second one on top of it. One second is roughly the shortest gap two lines can usefully have.
pub const ADD_NEAR_SECONDS: f64 = 1.0;

/// §C.2 (`chars / measured rate (default 15 chars/s…)`): how long a line runs when its take does not exist yet. The
/// page needs it for one thing — saying until when the seconds at the playhead are already spoken for.
pub const SPEECH_CHARS_PER_SECOND: f64 = 15.0;

/// §C.2's `tail` (P.eng.narrationTailSeconds): the silence after the last word, which is part of how long a line
/// holds the seconds. Without it "speaking until mm:ss" would name the last consonant rather than the end.
pub const SPEECH_TAIL_SECONDS: f64 = 0.2;

/// The clip a line was written against: the record stores the bounds, so that is what "the same clip" means here.
fn same_clip(clip: &Seg, entry: &Entry) -> bool {
    clip.s == entry.s && clip.e == entry.e
}

/// The clip the playhead stands on, half-open like every other range in the app.
fn clip_at<'a>(clips: &'a [Seg], at: f64) -> Option<&'a Seg> {
    clips.iter().find(|seg| seg.s <= at && at < seg.e)
}

/// How long a line runs when there is nothing to measure it by: §C.2's default of 15 characters a second plus its
/// tail (P.eng.narrationTailSeconds), which together are the only estimate available before its take exists — the
/// tail counts because "speaking until mm:ss" names the end of the line, not its last consonant. `0.0` for a caption
/// or a deliberate silence — neither speaks.
fn estimated_speech(entry: &Entry) -> f64 {
    if entry.text.is_empty() || !entry.pos.is_empty() {
        return 0.0;
    }
    entry.text.chars().count() as f64 / SPEECH_CHARS_PER_SECOND + SPEECH_TAIL_SECONDS
}

/// The second a line stops speaking, as far as the page can tell before its take exists: §C.2's 15 characters a
/// second until there is a wav to measure. A caption or a deliberate silence speaks for no time at all.
fn speech_end(entry: &Entry) -> f64 {
    entry.s + entry.at + estimated_speech(entry)
}

/// The line the playhead would speak over, if any: a spoken (not captioned, not silent) line **on the clip the
/// playhead stands on**, whose window holds that second. A line on another clip says nothing about these seconds —
/// the record stores the bounds it was written against, so those are what "the same clip" means here.
fn speaking_at<'a>(clip: &Seg, entries: &'a [Entry], at: f64) -> Option<&'a Entry> {
    entries.iter().find(|entry| {
        same_clip(clip, entry)
            && !entry.text.is_empty()
            && entry.pos.is_empty()
            && at >= entry.s + entry.at
            && at < line_end(entries, entry)
    })
}

/// Until when this line holds the clip's seconds: its own spoken length, cut short by the next line's start. Two
/// lines' windows never overlap (§5), so the earlier of the two is what a new line cannot be placed under.
fn line_end(entries: &[Entry], entry: &Entry) -> f64 {
    let started = entry.s + entry.at;
    let mut end = speech_end(entry);
    // The next line's second bounds this one's window. Entries arrive sorted, but a hand-edited file need not be, so
    // it is the minimum over the later ones rather than the first one found.
    for other in entries.iter().filter(|other| {
        other.s == entry.s && other.e == entry.e && other.s + other.at > started && !other.text.is_empty()
    }) {
        end = end.min(other.s + other.at);
    }
    end
}

/// §1's transport ＋: three reasons a line cannot start here, in F4.7's order — nothing to narrate, already
/// written here, and still speaking. `Ok` is the second the new line starts, on the clip it landed in.
pub fn add_at_playhead(at: f64, segs: &[Seg], entries: &[Entry]) -> Result<f64, String> {
    let Some(clip) = clip_at(segs, at) else {
        return Err(
            "the playhead is between clips \u{2014} the cut has nothing to narrate here".to_string(),
        );
    };
    if let Some(near) = entries.iter().find(|entry| (entry.s + entry.at - at).abs() < ADD_NEAR_SECONDS) {
        // Not a refusal the person has to argue with: the page jumps to the line instead, and says so.
        return Err(format!(
            "a line already starts here \u{2014} edit it, or move the playhead (it is at {})",
            preview::clock(Some(near.s + near.at))
        ));
    }
    // §1's `a line is speaking here until mm:ss — add after it`, and F4.7's flowchart asks the same of every line on
    // this clip rather than only the last: a second voice laid under one already speaking is what the branch refuses,
    // wherever in the clip it was pressed. Until when is the next line's second, or this line's estimate — §C.2's 15
    // chars/s while no take exists to measure — whichever comes first. Adding after an estimate is what puts two lines
    // on one another, so the refusal names the second to add after.
    let speaking = speaking_at(clip, entries, at);
    if let Some(speaking) = speaking {
        return Err(format!(
            "a line is speaking here until {} \u{2014} add after it",
            preview::clock(Some(line_end(entries, speaking)))
        ));
    }
    Ok(clip.s)
}

/// §1's row ＋ (`starting where its audio ends`): [`ADD_BELOW_SECONDS`] after the line above, refused when the clip
/// ends first — a line placed past its clip is one the render will move anyway, and silently. The bound is the same
/// last-second rule as [`tag_moves_line`]: a line starting inside the clip's final second has no room to be heard
/// before the cut moves on, so `audio_end + 0.5` has to land at least a second short of the clip's end.
pub fn add_below(audio_end: f64, clip_end: f64) -> Result<f64, String> {
    let at = audio_end + ADD_BELOW_SECONDS;
    if at > clip_end - 1.0 {
        return Err("no room after this line \u{2014} the clip ends first".to_string());
    }
    Ok(at)
}

/// §1's 🗑 (`remove this line — the video plays its own audio here instead`). The longer sentence, and the silent
/// marker behind it, are only owed when this was the clip's last line: otherwise the clip still speaks.
pub fn remove_line(clip_has_other_line: bool, clip_s: f64) -> String {
    if clip_has_other_line {
        return "line removed".to_string();
    }
    format!(
        "line removed \u{2014} the clip at {} plays its own audio",
        cut_hear::scene_clock(clip_s)
    )
}

/// §1's ↻ (§6: `"clip N has no line to re-roll"`): a re-roll draws again, and there is nothing to draw again of.
pub fn re_roll_refusal(clip_no: usize, has_line: bool) -> Option<String> {
    (!has_line).then(|| format!("clip {clip_no} has no line to re-roll"))
}

/// §6 (`re-rolling an already heard or edited narration needs the Replace tick`): ▶ refuses to overwrite a
/// take somebody has listened to or changed until the page is told that is what it is for. The refusal says
/// what to click, because nothing else clears it — and the tick never defaults on ([`crate::narrate_details`]).
pub fn replace_refusal(heard_or_edited: bool, replace_ticked: bool) -> Option<&'static str> {
    (heard_or_edited && !replace_ticked).then_some(
        "this narration was already heard or edited \u{2014} tick \u{201c}Replace\u{201d} to write another take over it",
    )
}

/// §1's time entry (`mm:ss.s, session clock`): the face is [`preview::clock`]'s, so a row and the transport agree.
pub fn time_field(shown: f64) -> String {
    preview::clock(Some(shown))
}

/// §1 (`a time in a gap is refused and written back`). The sentence names all three numbers a person needs: the
/// second they typed, the clip's bounds, and where the line stayed — so writing it back reads as an answer.
pub fn time_refused(requested: f64, clip_s: f64, clip_e: f64, at: f64) -> String {
    format!(
        "{} is outside the cut \u{2014} this line stays in its clip ({}\u{2013}{}), at {}",
        tools::mm_ss(requested),
        tools::mm_ss(clip_s),
        tools::mm_ss(clip_e),
        preview::clock(Some(at))
    )
}

// --- Narration off, and the two readouts (§1's numbers 1, 22) ----------------------------------------------------

/// §1 (`Off greys lines, preview and voice picker`). Greyed rather than removed, so the page does not reflow
/// under someone who is deciding. Produce hides **only** the game-volume slider: the shipped tooltip also promises
/// the subtitle choices go, which §1 calls stale (captions come from the transcript), so this list stops at three.
pub fn off_greys() -> [&'static str; 3] {
    ["lines", "preview", "voice"]
}

/// §1 (`Inputs: "N clip(s) · mm:ss[ · ⚠ N clip(s) unwritten, N line(s) off the cut][ · no timeline]"`). The two
/// counts are [`crate::shell::refit`]'s answers: a clip is unwritten when no entry sits on it and it is not
/// deliberately silent, and a line is off the cut when it sits on video the cut no longer has.
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

/// §1 (`Outputs: "narrate/ — narration.json, the voice reference and the synthesis cache"`). The folder button's
/// tooltip, which names the three things the page owns without pretending to know how many files are in them yet.
pub fn outputs_readout() -> &'static str {
    "narrate/ \u{2014} narration.json, the voice reference and the synthesis cache"
}
