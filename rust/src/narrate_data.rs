//! §07-narrate#3-data — the other files in `narrate/`.
//!
//! §3 lists files rather than prose, so each item below is one file's rule: the previous
//! generation of the record ([`keep_previous`]), the two one-line settings ([`read_voice`] /
//! [`write_voice`], [`read_pitch`] / [`write_pitch`]), the take set ([`load_takes`] /
//! [`save_takes`] / [`clean_takes`]), the two reference wavs and their invalidation
//! ([`served_reference`], [`base_reference`], [`drop_reference`]), and the sample cache's names
//! ([`sample_path`]).
//!
//! It does NOT own: `narration.json`'s record — its format, sorting and TTS key/file/seed are
//! [`crate::narration`]'s; a take's seconds rules (the floor is [`crate::narrate_screen`]'s, and
//! this module only applies it); building the reference (F4.6); speaking a line (F4.4); or which
//! voice a take was made with (F4.6/F4.7).
//!
//! Where §3 is silent the rule follows the Go prototype in `gui/` — AGENT.md names it the
//! read-only source of observed behaviour — and each item cites the function it mirrors
//! (`keepPrevNarration`, `voiceKey`/`ownVoice`, `pitchST`/`setPitchST`, `cleanTakes`,
//! `setTakesFor`, `shiftRef`, `sampleWav`). Plain logic only: no widget, page or thread, and
//! every path comes from [`crate::layout::Tree`] rather than being spelled out here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::layout::Tree;
use crate::narrate_screen::{clamp_pitch, TAKE_MIN_SECONDS};
use crate::narration::Narration;

/// §07-narrate#3-data's own list of what lives in `narrate/`, in the spec's order. Held as
/// data so a test can pin every accessor in `crate::layout` against the spec's words: a rename
/// there fails this round rather than turning up as a missing file months later.
pub const DATA_FILES: [&str; 9] = [
    "narration.json",
    "narration.prev.json",
    "voice.txt",
    "pitch.txt",
    "takes.json",
    "voice_ref_base.wav",
    "voice_ref.wav",
    "tts/*.wav",
    "samples/*.wav",
];

// --- narration.prev.json ------------------------------------------------------

/// §07-narrate#3-data: `narration.prev.json`, one generation back — the copy a run makes
/// before it overwrites the record, for "I did not mean that" (prototype `keepPrevNarration`).
///
/// `Ok(false)` when there is no record yet: nothing to keep, and not a problem. `Ok(true)` once
/// a copy exists. One generation deep — the file is overwritten, never appended to. This reads
/// `narration.json` and never modifies it; writing the record stays [`crate::narration::save`].
pub fn keep_previous(tree: &Tree) -> Result<bool, String> {
    let from = tree.narration_json();
    let bytes = match fs::read(&from) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(format!("{}: {err}", from.display())),
    };
    let to = tree.narration_prev_json();
    fs::write(&to, bytes).map_err(|err| format!("{}: {err}", to.display()))?;
    Ok(true)
}

/// What [`keep_previous`] left behind, if anything. `None` when the file is absent or
/// unparsable: a half-written previous generation must never fail the run that is about to
/// overwrite the current one.
pub fn previous(tree: &Tree) -> Option<Narration> {
    let text = fs::read_to_string(tree.narration_prev_json()).ok()?;
    serde_json::from_str(&text).ok()
}

// --- voice.txt ----------------------------------------------------------------

/// §07-narrate#3-data: the id `narrate/voice.txt` holds when the file is not there — the
/// session's own narrator (prototype `ownVoice`, gui/narrate_voice.go:32).
pub const OWN_VOICE: &str = "own";

/// §07-narrate#3-data: read `narrate/voice.txt`. The id is what
/// [`crate::narrate_screen::VoiceOption`] carries, written bare and read trimmed; a missing or
/// blank file reads [`OWN_VOICE`]. It lives in the project because it is part of every step's
/// input — the narrator's name reaches the describe and transcript workers too — so it has to
/// survive the app closing (prototype `voiceKey`'s read).
pub fn read_voice(tree: &Tree) -> String {
    match fs::read_to_string(tree.voice_txt()) {
        Ok(text) => {
            let id = text.trim();
            if id.is_empty() {
                OWN_VOICE.to_string()
            } else {
                id.to_string()
            }
        }
        Err(_) => OWN_VOICE.to_string(),
    }
}

/// The other half of [`read_voice`]: the id as it stands, no newline. Written through
/// `Tree::write_file` so §1's 0644 applies to a file created here too.
pub fn write_voice(tree: &Tree, id: &str) -> Result<(), String> {
    let rel = relative(tree, &tree.voice_txt());
    tree.write_file(&rel, id.as_bytes())
}

/// `narrate/voice.txt`'s other half of the deal with `narrate/samples/`: an id is pasted into
/// those file names and read back out of this file, so a slash in it would write somewhere else
/// entirely. Letters, digits, `-`, `_` and `.` survive; everything else becomes `-`; both ends
/// lose the separators they gained; nothing left over means `"voice"` (prototype
/// `sanitizeVoiceID`).
pub fn sanitize_voice_id(raw: &str) -> String {
    let mapped: String = raw
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '-',
        })
        .collect();
    let trimmed = mapped.trim_matches(|c| c == '-' || c == '_' || c == '.');
    if trimmed.is_empty() {
        return "voice".to_string();
    }
    trimmed.to_string()
}

// --- pitch.txt ----------------------------------------------------------------

/// §07-narrate#3-data: `narrate/pitch.txt`, the semitones the reference is shifted by. Read
/// clamped rather than refused, because a stored value outside the range (an edited file, or a
/// project carried from a build with different ends) should still mean "shift it that way" —
/// P.eng.pitchRangeSemitones, whose row `crate::params::narrate()` builds from
/// [`crate::narrate_screen`]'s slider ends. Missing or unparsable reads 0.0 (prototype
/// `pitchST`).
pub fn read_pitch(tree: &Tree) -> f64 {
    match fs::read_to_string(tree.pitch_txt()) {
        Ok(text) => match text.trim().parse::<f64>() {
            Ok(semitones) => clamp_pitch(semitones),
            Err(_) => 0.0,
        },
        Err(_) => 0.0,
    }
}

/// The other half: one decimal, which is the slider's own step and all the resolution a
/// rubberband pitch shift answers to (prototype `setPitchST`'s `FormatFloat(.., 'f', 1, 64)`).
pub fn write_pitch(tree: &Tree, semitones: f64) -> Result<(), String> {
    let rel = relative(tree, &tree.pitch_txt());
    tree.write_file(&rel, format!("{semitones:.1}").as_bytes())
}

// --- takes.json ---------------------------------------------------------------

/// One stretch of a recording, in that file's own seconds. Lowercase field names because that
/// is how `takes.json` spells them (prototype `voiceTake`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub s: f64,
    pub e: f64,
}

/// §07-narrate#3-data: read `narrate/takes.json` — hand-picked stretches keyed by RECORDING,
/// not by narrator slot. A missing file is an empty map: nobody has picked anything yet
/// (prototype `takesRead1`).
pub fn load_takes(tree: &Tree) -> Result<BTreeMap<String, Vec<Take>>, String> {
    let file = tree.takes_json();
    match fs::read_to_string(&file) {
        Ok(text) => serde_json::from_str(&text).map_err(|err| format!("{}: {err}", file.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(err) => Err(format!("{}: {err}", file.display())),
    }
}

/// The other half: [`clean_takes`] first, then store under [`take_base`] of the recording, and
/// delete the key when nothing survives — an empty list is no takes, not a take of no seconds
/// (prototype `setTakesFor`). Two-space indent, as the prototype's `MarshalIndent` wrote it.
pub fn save_takes(tree: &Tree, recording: &str, takes: &[Take]) -> Result<(), String> {
    let mut all = load_takes(tree)?;
    let cleaned = clean_takes(takes);
    if cleaned.is_empty() {
        all.remove(recording);
    } else {
        all.insert(recording.to_string(), cleaned);
    }
    let text = serde_json::to_string_pretty(&all).map_err(|err| err.to_string())?;
    let rel = relative(tree, &tree.takes_json());
    tree.write_file(&rel, text.as_bytes())
}

/// The key a recording's takes live under: its file stem. Takes belong to the recording rather
/// than to the narrator slot pointing at it, so re-tagging a slot never hands one person's
/// takes to another (§07's takes row; prototype `takesFor`).
pub fn take_base(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The shape everything else may assume: sorted, long enough, and with nothing overlapping or
/// touching. Below [`TAKE_MIN_SECONDS`] (P.eng.takeMinSeconds — the same floor
/// `crate::narrate_screen` applies when a press is made, not a second copy of it) a take is a
/// breath rather than a voice, and a take starting before 0 names seconds that do not exist.
///
/// Overlaps are merged rather than rejected because they are how a set is edited — widening a
/// take by dragging across its neighbour is one gesture, not an error — and a reference cut from
/// overlapping takes would say the same syllable twice (prototype `cleanTakes`).
pub fn clean_takes(takes: &[Take]) -> Vec<Take> {
    let mut kept: Vec<Take> = takes
        .iter()
        .filter(|take| take.e - take.s >= TAKE_MIN_SECONDS && take.s >= 0.0)
        .copied()
        .collect();
    kept.sort_by(|a, b| a.s.total_cmp(&b.s));
    let mut out: Vec<Take> = Vec::with_capacity(kept.len());
    for take in kept {
        match out.last_mut() {
            Some(last) if take.s <= last.e => last.e = last.e.max(take.e),
            _ => out.push(take),
        }
    }
    out
}

// --- voice_ref_base.wav, voice_ref.wav ---------------------------------------

/// The rate a voice reference is written at. `loudnorm` resamples to 192 kHz, and above 48 kHz
/// ffmpeg writes `WAVE_FORMAT_EXTENSIBLE` (format tag 0xFFFE) instead of a plain PCM header —
/// which the audio server refuses as "unsupported WAV encoding". 48 kHz keeps the header plain
/// and is at or above the rate of anything the reference is cut from, so no sample of the take
/// is thrown away to get there. (Prototype `refRate`, gui/narrate_voice.go:214.)
pub const REF_SAMPLE_RATE: &str = "48000";

/// The level the voice reference is written at, so the model hears the narrator at a sane
/// loudness: a clone is only as loud as its source, and a quietly spoken take level-matched
/// against loudnorm'd game audio comes back as somebody murmuring. Single-pass (dynamic)
/// `loudnorm`, because the takes come from different minutes of the session, and a tight LRA
/// because it is one person talking. §10 files this as `P.eng.refLoudness`
/// ("I -16, TP -1.5, LRA 7"), and `params::narrate()` rows it from this constant.
/// (Prototype `refLoud`, gui/narrate_voice.go:208.)
pub const REF_LOUDNESS: &str = "loudnorm=I=-16:TP=-1.5:LRA=7";

/// §07-narrate#3-data: the file the audio server is handed. One fixed path for every request,
/// which is why the unshifted base is never given to it and why a pitch shift writes a copy
/// here instead — at 0 semitones the copy is byte-identical to the base (prototype `shiftRef`).
pub fn served_reference(tree: &Tree) -> PathBuf {
    tree.voice_ref_wav()
}

/// §07-narrate#3-data: the unshifted reference, rebuilt from the recording or the picked voice.
/// Kept separate from [`served_reference`] so changing the pitch costs a re-shift and not a
/// re-cut (prototype `refBase`).
pub fn base_reference(tree: &Tree) -> PathBuf {
    tree.voice_ref_base_wav()
}

/// Is there a base with no shifted copy in front of it? That is the state a shift rebuilds
/// from, and the one [`drop_reference`] leaves the pitch slider in.
pub fn reference_needs_shift(base: &Path, served: &Path) -> bool {
    base.exists() && !served.exists()
}

/// The invalidation the three settings share: the built reference is an answer to a question,
/// and each of them changes the question — the voice, the pitch, or the takes it was cut from.
/// So they remove it and let the next thing spoken rebuild it (prototype `setVoice`,
/// `setPitchST`, `setTakesFor`). The served copy always goes; the base goes only when
/// `keep_base` is false — the pitch slider keeps its base and re-shifts it, while a take set or
/// a slot/captions voice makes the base itself wrong. A file that was never there is not a
/// failure: an error here would fail a setting change over nothing. Returns how many went.
pub fn drop_reference(tree: &Tree, keep_base: bool) -> usize {
    let mut dropped = 0;
    if remove_if_there(&served_reference(tree)) {
        dropped += 1;
    }
    if !keep_base && remove_if_there(&base_reference(tree)) {
        dropped += 1;
    }
    dropped
}

fn remove_if_there(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        // Nothing to drop is the state the caller wanted anyway; anything else (a directory, a
        // permission refusal) is a file still sitting there, which the count must not claim.
        Err(_) => false,
    }
}

// --- samples/*.wav ------------------------------------------------------------

/// §07-narrate#3-data: the hash half of a sample cache file's name — the first 6 bytes of the
/// key's SHA-1 as lowercase hex, twelve digits. `crate::narration::tts_file` is the same digest
/// cut to eight bytes for `tts/`; the sample cache wants six, so this is its own copy of the
/// loop rather than a parameter added to a file that has no reason to know about samples
/// (prototype `sampleWav`).
pub fn sample_hex(key: &str) -> String {
    let digest = Sha1::digest(key.as_bytes());
    let mut hex = String::with_capacity(12);
    for byte in &digest[..6] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// `narrate/samples/<voice>_<hash>.wav`: hearing a voice again is instant and comparing two is
/// a click each, so the file is named by who speaks and what they read. The name itself is
/// `Tree::sample_wav`'s; this only turns a key into its hash (prototype `sampleWav`).
pub fn sample_path(tree: &Tree, voice: &str, key: &str) -> PathBuf {
    tree.sample_wav(voice, &sample_hex(key))
}

/// Paths are handed to `Tree::write_file` relative to the project, which is what makes them
/// 0644 and creates their folder; every path this module reads came out of the same `Tree`.
fn relative(tree: &Tree, path: &Path) -> PathBuf {
    path.strip_prefix(tree.dir())
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(path.file_name().unwrap_or_default()))
}
