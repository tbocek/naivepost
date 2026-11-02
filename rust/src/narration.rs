//! The narration on disk: `narrate/narration.json` (spec/01-project-and-files.md §4).
//!
//! Pure data and file format — no UI here, so what a line says, where it sits and how
//! its spoken take is named are all testable without a window (spec/00-principles.md §5,
//! directive C).

use std::fs;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::cut::is_zero_f64;
use crate::cut::is_zero_i32;
use crate::layout;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    /// The clip's bounds, copied verbatim from the cut (§4).
    pub s: f64,
    pub e: f64,
    /// Seconds from the clip's start on the **recording's own clock**, rate ignored —
    /// [`output_seconds`] turns that into output seconds.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub at: f64,
    /// `""` is a line deleted on purpose — deliberately silent, and still an entry, so it survives
    /// a rebuild of the rows instead of coming back as a blank to fill.
    pub text: String,
    pub emotion: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pos: String,
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub roll: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Silent {
    pub s: f64,
    pub e: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Narration {
    // Always written: a narration with no lines writes `"entries": []`, which is a file
    // that says every clip was heard and nothing was to be said.
    pub entries: Vec<Entry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub silent: Vec<Silent>,
}

/// The performer this build speaks through and the blend delivered with it, both in the cache key
/// so a take from before them is never served as a current one.
const TTS_PREFIX: &str = "25e0.85";

/// The voice that speaks the project's own audio: it adds nothing to the cache key, so a project
/// narrated before the voice picker existed keeps every take it already had.
const OWN_VOICE: &str = "own";

/// The parts of a take's name after the prefix, in the order they go on: `[roll#]`,
/// then `[voice|]`, then `text|emotion`. Splitting them out keeps each salt added once and
/// in one order, so a roll and a voice together read `25e0.85|2#aria|…`.
fn key_body(entry: &Entry, voice: Option<&str>, emotion: &str) -> String {
    let mut body = format!("{}|{}", entry.text, emotion);
    if let Some(voice) = voice.filter(|v| *v != OWN_VOICE && *v != "narrator1") {
        body = format!("{voice}|{body}");
    }
    if entry.roll > 0 {
        // A line never re-rolled keeps the key it already had.
        body = format!("{}#{body}", entry.roll);
    }
    body
}

impl Narration {
    /// Playing order: by clip, then by placement within it (§4).
    pub fn sort(&mut self) {
        sort_entries(&mut self.entries);
    }

    pub fn is_silent(&self, s: f64, e: f64) -> bool {
        self.silent.iter().any(|q| q.s == s && q.e == e)
    }

    pub fn has_line(&self, s: f64, e: f64) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.s == s && entry.e == e)
    }
}

/// The clip's output seconds a line sits at: `at` is on the recording's own clock, so a
/// line stays on the same picture whatever speed the clip plays at, and the render
/// divides by the rate again (§4).
///
/// A rate of nought or less leaves it where its own clock puts it: rate 0 is a stop, a
/// still held on one frame, and dividing would put the line nowhere (or, worse, at minus
/// infinity's neighbourhood) for want of an answer this file was not given.
pub fn output_seconds(at: f64, rate: f64) -> f64 {
    if rate <= 0.0 {
        return at;
    }
    at / rate
}

/// The name of one take: everything that decides how this line comes back (§4).
pub fn tts_key(entry: &Entry, voice: Option<&str>, emotion_vector: Option<[f64; 8]>) -> String {
    let emotion = match emotion_vector {
        Some(vector) => format_vector(&vector),
        None => entry.emotion.clone(),
    };
    // The prefix stays first, so a take from before it is unreachable however the rest of the key
    // is salted.
    format!("{}|{}", TTS_PREFIX, key_body(entry, voice, &emotion))
}

pub fn key_for(entry: &Entry, voice: Option<&str>) -> String {
    tts_key(
        entry,
        voice,
        crate::narrate_tts::emotion_vector(&entry.emotion),
    )
}

fn format_vector(vector: &[f64; 8]) -> String {
    vector
        .iter()
        .map(|v| {
            let rounded = (v * 1000.0).round() / 1000.0;
            if rounded == 0.0 {
                return "0".to_string();
            }
            format!("{rounded}")
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn tts_file(key: &str) -> String {
    let digest = Sha1::digest(key.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

pub fn tts_seed(key: &str) -> u32 {
    let digest = Sha1::digest(key.as_bytes());
    u32::from_be_bytes([digest[8], digest[9], digest[10], digest[11]])
}

/// Lines in playing order: by clip, then by placement within it.
pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| a.s.total_cmp(&b.s).then(a.at.total_cmp(&b.at)));
}

/// The lines a voice speaks: every entry with words. A blank line is a deliberate silence and is
/// never spoken.
pub fn lines_to_speak(entries: &[Entry]) -> Vec<Entry> {
    entries
        .iter()
        .filter(|entry| !entry.text.trim().is_empty() && entry.pos.is_empty())
        .cloned()
        .collect()
}

pub fn has_lines(entries: &[Entry]) -> bool {
    entries.iter().any(|entry| !entry.text.trim().is_empty())
}

/// Read `narrate/narration.json`. A project that has never been narrated has no file, which is the
/// same state as a narration with no entries.
pub fn load(tree: &layout::Tree) -> Result<Narration, String> {
    let file = tree.narration_json();
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Narration::default()),
        Err(err) => return Err(format!("{}: {err}", file.display())),
    };
    let mut narration: Narration =
        serde_json::from_str(&text).map_err(|err| format!("{}: {err}", file.display()))?;
    // A hand-edited file may be out of playing order; it is put back here rather than left for
    // every reader to notice.
    narration.sort();
    Ok(narration)
}

pub fn save_held(held: &Narration, tree: &layout::Tree) -> Result<(), String> {
    let mut record = held.clone();
    record.silent = load(tree)?.silent;
    save(&record, tree)
}

/// The one function that writes `narration.json` (§4).
pub fn save(narration: &Narration, tree: &layout::Tree) -> Result<(), String> {
    let mut sorted = narration.clone();
    sorted.sort();
    let text = serde_json::to_string_pretty(&sorted).map_err(|err| err.to_string())?;
    let file = tree.narration_json();
    let rel = file
        .strip_prefix(tree.dir())
        .unwrap_or(std::path::Path::new("narrate/narration.json"));
    tree.write_file(rel, text.as_bytes())
}
