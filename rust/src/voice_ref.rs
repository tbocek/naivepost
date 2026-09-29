//! F4.6 Choose the voice and build the reference — `spec/07-narrate.md` F4.6, steps S1–S5.
//!
//! Who speaks is one choice with three kinds of answer (captions only, a narrator slot cut from the
//! session's own recording, a file in the voices folder), and every answer has to become the same two
//! files the speech server clones from: `narrate/voice_ref_base.wav` (levelled) and
//! `narrate/voice_ref.wav` (pitch-shifted). This module holds those rules as plain functions; the page
//! (`crate::ui::narrate_page`) draws them and forwards the presses.
//!
//! What this module does **not** own:
//! - where the files live or when they are thrown away — [`crate::narrate_data`] owns both paths, the
//!   loudness/rate constants and [`crate::narrate_data::drop_reference`];
//! - the picker's option list and the take seconds' floor — [`crate::narrate_screen`]'s
//!   `voice_options`, `TAKE_MIN_SECONDS`, `add_takes` / `remove_takes`;
//! - speaking anything ([`crate::narrate_tts`], F4.4) — this only builds what it clones from.
//!
//! A documented decision (§F4.6 is silent on it): the automatic pick takes its diarization turns and
//! transcript words as parameters the caller already loaded rather than opening `turns.json` here.
//! Reaching into a file format this module does not own would give it a second reason to change, and
//! the caller that has the session open can hand the turns over cheaply.

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::narrate_data;
use crate::narrate_screen::CAPTIONS;

// --- the numbers §10 lists for this flow -----------------------------------------------------

/// P.eng.refMinTakeSeconds: the shortest stretch the automatic pick will use. Below this the model
/// clones from too little audio and the voice wavers between lines.
pub const REF_MIN_TAKE_SECONDS: f64 = 5.0;

/// P.machine.refWantSeconds: how much reference the automatic pick aims at. Kept pointed at
/// `narrate_details`' existing constant so one number answers both the aim and the "14 s is plenty"
/// status line.
pub const REF_WANT_SECONDS: f64 = crate::narrate_details::REFERENCE_WANTED_SECONDS;

/// P.machine.refTakeMax: the most pieces the automatic pick may stitch together. More than three and
/// the reference is a patchwork whose seams the clone inherits.
pub const REF_TAKE_MAX: usize = 3;

/// P.eng.refMinWordsPerSecond: how dense a turn must be to be worth cloning. A sparse turn is the
/// diarizer guessing over music or a door slam.
pub const REF_MIN_WORDS_PER_SECOND: f64 = 1.5;

/// How far another speaker has to stay clear of a candidate take. The spec gives 2 s; it keeps the
/// clone from hearing the other person's last word inside the reference.
pub const OTHER_SPEAKER_CLEAR_SECONDS: f64 = 2.0;

/// The rate ceiling a picked wav must respect to be readable by the speech server (§F4.6 S3).
const READABLE_MAX_RATE: u32 = 48_000;

/// `WAVE_FORMAT_EXTENSIBLE` — the header tag ffmpeg writes above 48 kHz or for non-standard channel
/// masks, which the speech server refuses outright.
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

// --- S1: picking the voice -------------------------------------------------------------------

/// What a voice choice resolved to. `Chosen` carries the sentence S1 says out loud; `Refused` carries
/// the spec's refusal verbatim, which the page prints unchanged rather than paraphrasing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceChoice {
    /// Captions only: nothing is spoken, and S1 wants that said rather than left inferred from a grey
    /// control.
    Silent(String),
    /// The voice is usable; `said` is the switch report.
    Chosen { id: String, said: String },
    /// The voice cannot be used; `why` is the sentence to print.
    Refused(String),
}

/// S1: resolve a picked voice against what Prepare actually tagged and what the voices folder holds.
///
/// `narrators` is how many narrator slots have a recording, `voice_files` the names in the voices
/// folder, `dir` the voices folder as it is shown in the "no longer in DIR" sentence.
pub fn pick_voice(
    id: &str,
    narrators: usize,
    voice_files: &[String],
    dir: &str,
) -> VoiceChoice {
    if id == CAPTIONS {
        return VoiceChoice::Silent(SILENT_SWITCH.to_string());
    }
    if let Some(slot) = narrator_slot(id) {
        if slot < 1 || slot > narrators {
            return VoiceChoice::Refused(untagged_narrator(slot));
        }
        return VoiceChoice::Chosen {
            id: id.to_string(),
            said: narrator_switch(slot),
        };
    }
    if voice_files.iter().any(|name| name == id) {
        return VoiceChoice::Chosen {
            id: id.to_string(),
            said: file_switch(id),
        };
    }
    VoiceChoice::Refused(vanished_voice(id, dir))
}

/// Which narrator slot an id names; `None` for captions or a file name.
pub fn narrator_slot(id: &str) -> Option<usize> {
    let digits = id.strip_prefix("narrator")?;
    digits.parse::<usize>().ok()
}

/// S1's refusal when a slot has no recording tagged on the Prepare step.
pub fn untagged_narrator(slot: usize) -> String {
    format!(
        "narrator {slot} is not tagged on the Prepare step \u{2014} tag a recording, or pick another voice"
    )
}

/// S1's refusal when a voices-folder file has gone missing since it was chosen.
pub fn vanished_voice(name: &str, dir: &str) -> String {
    format!("voice {name} is no longer in {dir} \u{2014} pick another")
}

/// S4: which setting changed, so the invalidation knows whether the base survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changed {
    /// A different voice: the base was cut from something else, so both files go.
    Voice,
    /// The pitch slider: only the shifted copy is wrong; the base re-shifts.
    Pitch,
    /// The take set: the base was cut from those seconds, so both files go.
    Takes,
}

/// S4's invalidation rule in one call: the built reference answers a question, and all three settings
/// change it. Delegates to [`crate::narrate_data::drop_reference`], which keeps the base for a pitch
/// change and drops it otherwise — this exists so the page says WHICH thing changed rather than passing
/// a bare bool that reads backwards at the call site.
pub fn invalidate_for_change(tree: &Tree, what: Changed) -> usize {
    let keep_base = matches!(what, Changed::Pitch);
    narrate_data::drop_reference(tree, keep_base)
}

/// S1's report for captions only.
pub const SILENT_SWITCH: &str =
    "no audio \u{2014} the narration is written and timed, never spoken";

/// S1's report for a narrator slot.
pub fn narrator_switch(slot: usize) -> String {
    format!("voice: narrator {slot}'s \u{2014} it is re-cut from the recording on the next line spoken")
}

/// S1's report for a voices-folder file.
pub fn file_switch(name: &str) -> String {
    format!("voice: {name} \u{2014} \u{25b6} beside the sample plays it")
}

/// S1's report when installing a picked file failed; the reason goes to the log.
pub const INSTALL_FAILED: &str = "could not install that voice \u{2014} see log";

// --- S2: choosing the seconds the reference is cut from --------------------------------------

/// One candidate piece for the automatic reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub start: f64,
    pub end: f64,
}

impl Piece {
    pub fn seconds(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

/// What the automatic pick decided.
#[derive(Debug, Clone, PartialEq)]
pub enum ReferencePlan {
    /// Hand-picked takes win outright: no cap, no diarization needed (S2).
    HandPicked(Vec<Piece>),
    /// Pieces chosen from the diarized turns, within the wanted length and the piece cap.
    Automatic(Vec<Piece>),
    /// No usable reference; `why` is the spec's sentence.
    Unavailable(String),
}

/// S2: decide what the reference is cut from, against the session's own Prepare output.
///
/// `takes` are the hand-picked takes (empty when nobody picked any); `turns` the diarized turns of the
/// narrator's recording; `words_in` the count of transcript words falling inside each candidate window,
/// paired with that window. Turns are given in seconds rather than samples because the caller has
/// already converted them — see the module doc.
///
/// The one thing this rule does NOT answer is whether a recording was tagged at all: only
/// `naivepost.json` says that, and it is asked by [`recut_for_voice`] before this runs, because
/// "run Prepare" would be wrong advice for a project that never named a recording to run it on.
pub fn plan_reference(
    takes: &[(f64, f64)],
    turns: &[(f64, f64, u32)],
    words_in: impl Fn(f64, f64) -> usize,
    narrator_speaker: u32,
) -> ReferencePlan {
    if !takes.is_empty() {
        return ReferencePlan::HandPicked(
            takes.iter().map(|&(start, end)| Piece { start, end }).collect(),
        );
    }
    if turns.is_empty() {
        return ReferencePlan::Unavailable(NO_DIARIZATION.to_string());
    }

    // Solo stretches first: a window of the narrator's turns with no other speaker inside it, and at
    // least OTHER_SPEAKER_CLEAR_SECONDS of clearance on each side.
    let mut candidates: Vec<Piece> = Vec::new();
    for &(start, end, speaker) in turns {
        if speaker != narrator_speaker {
            continue;
        }
        let clear = |other: &(f64, f64, u32)| {
            other.2 != narrator_speaker
                && other.1 > start - OTHER_SPEAKER_CLEAR_SECONDS
                && other.0 < end + OTHER_SPEAKER_CLEAR_SECONDS
        };
        if turns.iter().any(clear) {
            continue;
        }
        if end - start < REF_MIN_TAKE_SECONDS {
            continue;
        }
        let words = words_in(start, end);
        if (words as f64) < (end - start) * REF_MIN_WORDS_PER_SECOND {
            continue;
        }
        candidates.push(Piece { start, end });
    }
    if candidates.is_empty() {
        return ReferencePlan::Unavailable(NO_CLEAN_STRETCH.to_string());
    }

    // Longest first, so few pieces carry the wanted length; stop at the cap.
    candidates.sort_by(|a, b| {
        b.seconds()
            .partial_cmp(&a.seconds())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut chosen: Vec<Piece> = Vec::new();
    let mut total = 0.0;
    for piece in candidates {
        if chosen.len() >= REF_TAKE_MAX || total >= REF_WANT_SECONDS {
            break;
        }
        total += piece.seconds();
        chosen.push(piece);
    }
    chosen.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ReferencePlan::Automatic(chosen)
}

/// S2's refusal when there is no diarization to choose from at all.
pub const NO_DIARIZATION: &str = "no diarization for X -- run Prepare, or pick the seconds by hand under the video";

/// S2's refusal when diarization exists but nothing clean and long enough turned up.
pub const NO_CLEAN_STRETCH: &str = "no clean solo stretch found for the voice reference";

/// S2's refusal naming the slot when nothing is tagged for it.
pub fn nothing_tagged(slot: usize) -> String {
    format!("nothing is tagged as narrator {slot} on the Prepare step")
}

// --- S3: the real level-and-shift leg -------------------------------------------------------

/// Where the two reference files landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    pub base: PathBuf,
    pub served: PathBuf,
    /// The sentences worth printing while building (a re-cut notice, a rename).
    pub said: Vec<String>,
}

/// The ffmpeg argument vector that levels the reference: mono, 48 kHz pcm, `REF_LOUDNESS`. Built as a
/// vector so the shape of the call is testable without running anything; the running stays with
/// [`crate::subprocess`].
pub fn level_args(input: &str, out: &str) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        input.to_string(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        narrate_data::REF_SAMPLE_RATE.to_string(),
        "-af".into(),
        narrate_data::REF_LOUDNESS.to_string(),
        "-c:a".into(),
        "pcm_s16le".into(),
        out.to_string(),
    ]
}

/// The ffmpeg argument vector that shifts the pitch with formants kept (`rubberband`'s `formant=y`).
/// `semitones` is the slider's value; the ratio is the linear factor rubberband expects.
pub fn shift_args(input: &str, out: &str, semitones: f64) -> Vec<String> {
    let ratio = 2f64.powf(semitones / 12.0);
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        input.to_string(),
        "-af".into(),
        format!("rubberband=pitch={ratio:.6}:formant=y"),
        "-c:a".into(),
        "pcm_s16le".into(),
        out.to_string(),
    ]
}

/// S3: read a wav header far enough to know whether the speech server can read it. Returns
/// `Some((format_tag, sample_rate))` for a RIFF/WAVE file, `None` when the header is not there or
/// not a wav at all (which is also a file the server cannot read).
pub fn wav_header(path: &Path) -> Option<(u16, u32)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 28 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    // The fmt chunk follows the 12-byte RIFF header in every writer we meet; scanning for the tag
    // keeps odd padding between chunks from being misread.
    let fmt_at = (12..bytes.len().saturating_sub(4))
        .find(|&i| &bytes[i..i + 4] == b"fmt ")?;
    let body = fmt_at + 8;
    if bytes.len() < body + 16 {
        return None;
    }
    let tag = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
    let rate = u32::from_le_bytes([bytes[body + 4], bytes[body + 5], bytes[body + 6], bytes[body + 7]]);
    Some((tag, rate))
}

/// Whether a picked wav needs re-cutting before the server will read it: no readable header, the
/// extensible format tag, or a rate above 48 kHz.
pub fn needs_re_cut(path: &Path) -> bool {
    match wav_header(path) {
        None => true,
        Some((WAVE_FORMAT_EXTENSIBLE, _)) => true,
        Some((_, rate)) => rate > READABLE_MAX_RATE,
    }
}

/// S3's notice that a picked file is being cut again, naming the file.
pub fn recut_notice(file: &str) -> String {
    format!("voice reference {file} is not a wav the server reads -- cutting it again")
}

/// F4.6 S2+S3 as one call: the reference the next spoken line clones from, cut from the seconds
/// [`plan_reference`] chose, levelled to the base and shifted to the served file. This is what S1's
/// "it is re-cut from the recording on the next line spoken" is made of, and the reason a voice pick
/// that invalidated `voice_ref.wav` still ends up speaking in the newly chosen voice.
///
/// `turns` and `words_in` come from the caller because they are the session's Prepare output
/// (see the module doc): `speaker_turns` loads them for the real flow. `ffmpeg` is the Settings
/// seam, so a test points it at a script and no live ffmpeg is needed.
///
/// `Err` carries the spec's own refusal sentence (S2's `nothing_tagged`, `NO_DIARIZATION` or
/// `NO_CLEAN_STRETCH`) or a failure naming the program that was run.
pub fn recut_for_voice(
    tree: &Tree,
    ffmpeg: &str,
    source: &str,
    turns: &[(f64, f64, u32)],
    words_in: impl Fn(f64, f64) -> usize,
) -> Result<Built, String> {
    // S3's repair runs BEFORE anything is cut: an older project holds `voice_ref.wav` with no base,
    // and that lone file is the reference's history. Cutting over it would lose it.
    let mut said = Vec::new();
    if let Some(renamed) = adopt_existing_reference(tree) {
        said.push(format!(
            "voice reference had no base; renamed {} to {}",
            narrate_data::served_reference(tree).display(),
            renamed.display()
        ));
    }

    let base = narrate_data::base_reference(tree);
    let served = narrate_data::served_reference(tree);
    // S2's first question is not about diarization at all but about the project: the spec's "nothing
    // is tagged as narrator N on the Prepare step" is answered of the recording this build was asked
    // to cut from, and it comes BEFORE the rule because the rule cannot see the project file.
    if nothing_is_tagged(tree, source) {
        return Err(nothing_tagged(FLOW_SLOT));
    }
    let plan = plan_reference(&takes_of(tree, source), turns, words_in, FLOW_SLOT as u32);
    let pieces = match plan {
        ReferencePlan::HandPicked(pieces) | ReferencePlan::Automatic(pieces) => pieces,
        ReferencePlan::Unavailable(why) => return Err(why),
    };

    if pieces.len() == 1 {
        // One stretch needs no stitching: level it straight onto the base.
        let piece = &pieces[0];
        let args = level_piece_args(source, piece.start, piece.end, &base.display().to_string());
        run_ffmpeg(ffmpeg, &args)?;
    } else {
        // Each piece levelled on its own, so the loudness target is met per stretch rather than across
        // silences, then one concat of the levelled parts. The list file's paths are quoted because
        // a recording named `take, 2.wav` would otherwise split on its own comma.
        let mut parts: Vec<PathBuf> = Vec::new();
        for (index, piece) in pieces.iter().enumerate() {
            let part = part_path(tree, index);
            let args = level_piece_args(source, piece.start, piece.end, &part.display().to_string());
            run_ffmpeg(ffmpeg, &args)?;
            parts.push(part);
        }
        let list = list_path(tree);
        let body: String = parts
            .iter()
            .map(|part| format!("file '{}'\n", part.display()))
            .collect();
        std::fs::write(&list, body).map_err(|err| format!("{}: {err}", list.display()))?;
        let args = concat_args(&list.display().to_string(), &base.display().to_string());
        run_ffmpeg(ffmpeg, &args)?;
    }

    // S3's second half: the served copy is the base at the slider's pitch. A zero shift still writes
    // it so the two files agree byte-for-byte, the same rule `build` follows.
    let semitones = narrate_data::read_pitch(tree);
    if semitones.abs() < f64::EPSILON {
        copy_file(&base, &served)?;
    } else {
        let shift = shift_args(&base.display().to_string(), &served.display().to_string(), semitones);
        run_ffmpeg(ffmpeg, &shift)?;
    }
    Ok(Built { base, served, said })
}

/// The one thing `recut_for_voice` needs from the project that the diarization cannot say: whether a
/// recording was tagged at all. `None` means the session's own recording — the one the flow is cutting
/// from, named by the caller — carries a narrator slot. "nothing tagged" is asked the way the spec
/// means it, of the recording in hand, and is exactly S2's own sentence.
fn nothing_is_tagged(tree: &Tree, source: &str) -> bool {
    let Ok(project) = crate::project::load(tree.dir()) else {
        // No project to read is not "untagged": there is nothing to blame, and the diarization answer
        // below is the honest one.
        return false;
    };
    !project
        .sources
        .iter()
        .any(|project_source| project_source.narrator != 0 && same_file(&project_source.path, source))
}

/// The slot this flow cuts for. §1 makes the first tagged recording the one the narration is spoken
/// in, so the automatic reference is always built for narrator 1; the same number appears in the
/// `nothing_tagged` sentence the refusal carries.
const FLOW_SLOT: usize = 1;

/// Whether two paths name the same recording, by file name: `takes.json` and the prepare folders are
/// keyed by stem for the same reason, so the comparison here stays on the same identity the rest of
/// the flow uses rather than on a string that differs by one trailing slash.
fn same_file(a: &str, b: &str) -> bool {
    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    !a.is_empty() && !b.is_empty() && name(a) == name(b)
}

/// The hand-picked takes this recording owns, as seconds. `takes.json` is keyed by RECORDING, so the
/// lookup is the source's own stem; no file, no key, or an unreadable one all mean "nobody picked
/// anything", which sends the plan to S2's automatic branch rather than failing the build.
fn takes_of(tree: &Tree, source: &str) -> Vec<(f64, f64)> {
    let key = narrate_data::take_base(Path::new(source));
    narrate_data::load_takes(tree)
        .ok()
        .and_then(|all| all.get(&key).cloned())
        .map(|takes| takes.iter().map(|take| (take.s, take.e)).collect())
        .unwrap_or_default()
}

/// One stitched piece's working file, beside the two the server reads.
fn part_path(tree: &Tree, index: usize) -> PathBuf {
    narrate_data::reference_part(tree, index)
}

/// The concat list that names the parts in playing order.
fn list_path(tree: &Tree) -> PathBuf {
    narrate_data::reference_list(tree)
}

/// S3's level pass over ONE stretch of the source: [`level_args`]' shape with `-ss`/`-to` in front of
/// the input, so the piece is cut as it is levelled and each one meets the loudness target on its own
/// seconds rather than averaged across the gaps between pieces.
pub fn level_piece_args(input: &str, start: f64, end: f64, out: &str) -> Vec<String> {
    let mut args = level_args(input, out);
    let at = args.iter().position(|a| a == "-i").unwrap_or(0);
    args.splice(
        at..at,
        [
            "-ss".to_string(),
            format!("{start:.3}"),
            "-to".to_string(),
            format!("{end:.3}"),
        ],
    );
    args
}

/// S3's stitching pass: ffmpeg's own concat demuxer over the list file, streams copied (the parts are
/// already mono 48 kHz pcm from the level pass, so there is nothing left to re-encode).
pub fn concat_args(list: &str, out: &str) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list.to_string(),
        "-c".into(),
        "copy".into(),
        out.to_string(),
    ]
}

/// S3: build the reference from `source` through the configured ffmpeg binary.
///
/// `ffmpeg` is the program path the app resolves from Settings (`settings.ffmpeg`, else PATH) — the
/// same seam every other ffmpeg user in this tree uses, so a test points it at a fake script and the
/// program never knows the difference. `semitones` is the pitch slider; 0 still writes the served
/// copy so the two files agree byte-for-byte at zero shift (§3's `shiftRef`).
///
/// Every failure names the program and the reason and comes back `Err`: no silent fallback.
pub fn build(
    tree: &Tree,
    source: &str,
    ffmpeg: &str,
    semitones: f64,
) -> Result<Built, String> {
    let base = narrate_data::base_reference(tree);
    let served = narrate_data::served_reference(tree);
    let mut said = Vec::new();

    let stage_source = source.to_string();
    if needs_re_cut(Path::new(source)) {
        said.push(recut_notice(source));
    }

    let level = level_args(&stage_source, &base.display().to_string());
    run_ffmpeg(ffmpeg, &level)?;

    if semitones.abs() < f64::EPSILON {
        copy_file(&base, &served)?;
    } else {
        let shift = shift_args(&base.display().to_string(), &served.display().to_string(), semitones);
        run_ffmpeg(ffmpeg, &shift)?;
    }
    Ok(Built { base, served, said })
}

/// S3's last sentence: a project holding `voice_ref.wav` with no base renames the former to the base.
/// Returns the renamed-to path, or `None` when there was nothing to do.
pub fn adopt_existing_reference(tree: &Tree) -> Option<PathBuf> {
    let served = narrate_data::served_reference(tree);
    let base = narrate_data::base_reference(tree);
    if served.exists() && !base.exists() {
        std::fs::rename(&served, &base).ok()?;
        return Some(base);
    }
    None
}

/// S5's key for one sample: the voice and the words, and nothing else.
///
/// The spec caches the sample "per voice and text", and the sample is not a line of the narration — it
/// has no clip bounds, no delivery tag and no re-roll count that mean anything. `narration::tts_key`
/// takes an `Entry` and would fold all three into the digest, so re-rolling or moving a line would
/// re-synthesise a sample whose words and voice never changed. The Entry here is a carrier for the text
/// alone; the voice rides in `Option` so a switch to another voice misses the cache, which is the
/// whole point of the key.
pub fn sample_key(voice: &str, text: &str) -> String {
    let carrier = crate::narration::Entry {
        text: text.to_string(),
        ..Default::default()
    };
    crate::narration::tts_key(&carrier, Some(voice), None)
}

/// Where one sample lives: `narrate/samples/<voice>_<hash>.wav`, named by who speaks and what they
/// read, so hearing a voice again is instant and a different voice never overwrites another's take.
pub fn sample_file(tree: &Tree, voice: &str, text: &str) -> PathBuf {
    narrate_data::sample_path(tree, voice, &sample_key(voice, text))
}

/// S5: the same sample bytes, if this voice and text were spoken before. A zero-length file is no
/// sample at all — an interrupted write must not be replayed as silence.
pub fn sample_cached(tree: &Tree, voice: &str, text: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(sample_file(tree, voice, text)).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(bytes)
}

/// S5's last step: file the take under the sample's own name. Returns nothing — the caller keeps the
/// bytes it read; this only says whether they could be written where they belong.
pub fn store_sample(
    tree: &Tree,
    voice: &str,
    text: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let file = sample_file(tree, voice, text);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    std::fs::write(&file, bytes).map_err(|err| format!("{}: {err}", file.display()))?;
    Ok(())
}


/// Run ffmpeg and turn any failure into a named error, the way the other legs do. The output folder is
/// made first: the reference's working files live under `narrate/reference/`, which a fresh session
/// has not created, and an ffmpeg told to write where no folder is fails with `nonexistent directory`
/// rather than telling anyone which part of the build broke.
fn run_ffmpeg(ffmpeg: &str, args: &[String]) -> Result<(), String> {
    if let Some(out) = args.last() {
        if let Some(parent) = Path::new(out).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::run(ffmpeg, &borrowed, |_| {}).map_err(|why| {
        format!("{}: {why}", crate::subprocess::shown(ffmpeg, &borrowed))
    })?;
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    let bytes =
        std::fs::read(from).map_err(|err| format!("{}: {err}", from.display()))?;
    if let Some(parent) = to.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(to, bytes).map_err(|err| format!("{}: {err}", to.display()))
}
