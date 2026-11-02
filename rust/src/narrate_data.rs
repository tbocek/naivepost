//! §07-narrate#3-data: the other files in `narrate/`, one rule per item: the previous record generation ([`keep_previous`]), the one-line settings ([`read_voice`]/[`write_voice`], [`read_pitch`]/[`write_pitch`]), the take set ([`load_takes`]/[`save_takes`]/[`clean_takes`]), the reference wavs and their invalidation ([`Tree::voice_ref_wav`], [`Tree::voice_ref_base_wav`], [`drop_reference`]), and the sample cache names ([`sample_path`]).
//! Not here: `narration.json`'s record ([`crate::narration`]), a take's seconds floor ([`crate::narrate_screen`]), building the reference (F4.6), speaking (F4.4), which voice made a take (F4.6/F4.7).
//! Where §3 is silent the rule follows the Go prototype in `go/`; items cite the function they mirror. Paths come from [`crate::layout::Tree`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::layout::Tree;
use crate::narrate_screen::{clamp_pitch, TAKE_MIN_SECONDS};
use crate::narration::Narration;

/// §07-narrate#3-data's list of what lives in `narrate/`, as data so a test can pin every `crate::layout` accessor to the spec.
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

/// `narration.prev.json`, one generation back, copied before a run overwrites the record (prototype `keepPrevNarration`). `Ok(false)` when there is no record yet. Never modifies `narration.json`.
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

/// What [`keep_previous`] left behind; `None` when absent or unparsable, which must not fail the run.
pub fn previous(tree: &Tree) -> Option<Narration> {
    let text = fs::read_to_string(tree.narration_prev_json()).ok()?;
    serde_json::from_str(&text).ok()
}

/// Where the run reads its starting record: `narrate/narration.json` if readable, else [`previous`], since a corrupt current file would answer "no lines" and the next save would overwrite the only readable copy. A MISSING file is a fresh session ([`crate::narration::load`]). Returns the record and whether the fallback was taken.
pub fn load_or_previous(tree: &Tree) -> Result<(Narration, bool), String> {
    if let Ok(record) = crate::narration::load(tree) {
        return Ok((record, false));
    }
    previous(tree).map(|record| (record, true)).ok_or_else(|| {
        format!(
            "{} cannot be read and neither can {}",
            tree.narration_json().display(),
            tree.narration_prev_json().display()
        )
    })
}

// --- voice.txt ----------------------------------------------------------------

/// The id `narrate/voice.txt` holds when absent: the session's own narrator (prototype `ownVoice`).
pub const OWN_VOICE: &str = "own";

/// Read `narrate/voice.txt`: the bare id ([`crate::narrate_screen::VoiceOption`]), trimmed; missing or blank reads [`OWN_VOICE`]. It lives in the project because the describe and transcript workers read it too.
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

/// The id as it stands, no newline, via `Tree::write_file` (§1's 0644).
pub fn write_voice(tree: &Tree, id: &str) -> Result<(), String> {
    let rel = relative(tree, &tree.voice_txt());
    tree.write_file(&rel, id.as_bytes())
}

// --- pitch.txt ----------------------------------------------------------------

/// `narrate/pitch.txt`: the semitones the reference is shifted by. Read clamped, not refused (P.eng.pitchRangeSemitones, built by `crate::params::narrate()` from [`crate::narrate_screen`]'s slider ends). Missing or unparsable reads 0.0.
pub fn read_pitch(tree: &Tree) -> f64 {
    match fs::read_to_string(tree.pitch_txt()) {
        Ok(text) => match text.trim().parse::<f64>() {
            Ok(semitones) => clamp_pitch(semitones),
            Err(_) => 0.0,
        },
        Err(_) => 0.0,
    }
}

/// One decimal: the slider's step and a rubberband shift's resolution.
pub fn write_pitch(tree: &Tree, semitones: f64) -> Result<(), String> {
    let rel = relative(tree, &tree.pitch_txt());
    tree.write_file(&rel, format!("{semitones:.1}").as_bytes())
}

// --- takes.json ---------------------------------------------------------------

/// One stretch of a recording in that file's own seconds; lowercase fields as `takes.json` spells them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub s: f64,
    pub e: f64,
}

/// Read `narrate/takes.json`: stretches keyed by RECORDING, not narrator slot. Missing is an empty map.
pub fn load_takes(tree: &Tree) -> Result<BTreeMap<String, Vec<Take>>, String> {
    let file = tree.takes_json();
    match fs::read_to_string(&file) {
        Ok(text) => serde_json::from_str(&text).map_err(|err| format!("{}: {err}", file.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(err) => Err(format!("{}: {err}", file.display())),
    }
}

/// [`clean_takes`] first, then store under [`take_base`], deleting the key when nothing survives. Two-space indent.
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

/// The key a recording's takes live under: its file stem, so re-tagging a slot never hands one person's takes to another (§07).
pub fn take_base(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Sorted, long enough, nothing overlapping or touching. Below [`TAKE_MIN_SECONDS`] (P.eng.takeMinSeconds, the floor `crate::narrate_screen` applies) a take is a breath, and a start before 0 is invalid. Overlaps are merged, not rejected: widening a take across its neighbour is one gesture.
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

/// The rate a voice reference is written at: `loudnorm` resamples to 192 kHz and above 48 kHz ffmpeg writes `WAVE_FORMAT_EXTENSIBLE`, which the audio server refuses as "unsupported WAV encoding".
pub const REF_SAMPLE_RATE: &str = "48000";

/// The level the voice reference is written at (§10 `P.eng.refLoudness`, "I -16, TP -1.5, LRA 7"; `params::narrate()` rows it from this constant). Single-pass `loudnorm` since takes come from different minutes, tight LRA for one speaker.
pub const REF_LOUDNESS: &str = "loudnorm=I=-16:TP=-1.5:LRA=7";

/// One stitched, levelled piece of the voice reference under `narrate/reference/`.
pub fn reference_part(tree: &Tree, index: usize) -> PathBuf {
    tree.reference_dir().join(format!("ref_part_{index}.wav"))
}

pub fn reference_list(tree: &Tree) -> PathBuf {
    tree.reference_dir().join("ref_parts.txt")
}

/// Invalidation shared by voice, pitch and takes: remove the built reference so the next speech rebuilds it. The served copy always goes; the base only when `keep_base` is false (the pitch slider keeps its base). A missing file is not a failure. Returns how many went.
pub fn drop_reference(tree: &Tree, keep_base: bool) -> usize {
    REFERENCE_TURN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut dropped = 0;
    if remove_if_there(&tree.voice_ref_wav()) {
        dropped += 1;
    }
    if !keep_base && remove_if_there(&tree.voice_ref_base_wav()) {
        dropped += 1;
    }
    dropped
}

static REFERENCE_TURN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn reference_turn() -> u64 {
    REFERENCE_TURN.load(std::sync::atomic::Ordering::SeqCst)
}

fn remove_if_there(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        // Nothing to drop is fine; anything else (a directory, a permission refusal) is still sitting there and must not be counted.
        Err(_) => false,
    }
}

// --- samples/*.wav ------------------------------------------------------------

/// The hash half of a sample cache file name: the first 6 bytes of the key's SHA-1 as lowercase hex. `crate::narration::tts_file` uses 8 bytes for `tts/`, so this is its own copy.
pub fn sample_hex(key: &str) -> String {
    let digest = Sha1::digest(key.as_bytes());
    let mut hex = String::with_capacity(12);
    for byte in &digest[..6] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// `narrate/samples/<voice>_<hash>.wav`; the name is `Tree::sample_wav`'s, this turns a key into its hash.
pub fn sample_path(tree: &Tree, voice: &str, key: &str) -> PathBuf {
    tree.sample_wav(voice, &sample_hex(key))
}

/// Paths go to `Tree::write_file` relative to the project (0644, creates the folder).
fn relative(tree: &Tree, path: &Path) -> PathBuf {
    path.strip_prefix(tree.dir())
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(path.file_name().unwrap_or_default()))
}
