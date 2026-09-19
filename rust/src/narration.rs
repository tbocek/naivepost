//! The narration on disk: `narrate/narration.json` (spec/01-project-and-files.md §4).
//!
//! Pure data and file format — no UI here, so what a line says, where it sits and how
//! its spoken take is named are all testable without a window (spec/00-principles.md §5,
//! directive C).

use std::fs;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::layout;

/// One line of narration over one clip.
///
/// Which keys the prototype omits (`gui/narrate.go:116-136`) is what this writes: §4 is
/// written against that file — it cites three of its read paths — and its example shows
/// `"pos": "", "roll": 0` as an illustration of the keys, not as bytes. A file written by
/// this build therefore leaves a zero `at`, an empty `pos` and a zero `roll` out, which
/// reads back identically, and keeps old files byte-identical the way §4 asks of the
/// emotion tag.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    /// The clip's bounds, copied verbatim from the cut (§4). They are what a line is
    /// attached to; nothing here re-derives them.
    pub s: f64,
    pub e: f64,
    /// Seconds from the clip's start on the **recording's own clock**, rate ignored —
    /// [`output_seconds`] turns that into output seconds. Zero is the head of the clip,
    /// which is also what every entry written before the field existed means.
    #[serde(skip_serializing_if = "is_zero_f64", default)]
    pub at: f64,
    /// `""` is a line deleted on purpose — deliberately silent, and still an entry, so
    /// it survives a rebuild of the rows instead of coming back as a blank to fill.
    pub text: String,
    /// A delivery tag ("calm"), or the 8-float vector a caller resolved one into.
    pub emotion: String,
    /// Caption placement: "top", "center", or `""` for the bottom.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pos: String,
    /// How many times this line has been asked for a different take. It moves nothing
    /// but the seed, by salting the cache key ([`tts_key`]).
    #[serde(skip_serializing_if = "is_zero_i32", default)]
    pub roll: i32,
}

/// A clip whose last line was deleted on purpose. Bounds and nothing else — which is
/// why this is not a [`crate::cut::Seg`]: it carries no asset, no lane, no rate, and
/// writing one of those would say something about the cut that this file does not know.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Silent {
    pub s: f64,
    pub e: f64,
}

/// The narration, whole.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Narration {
    // Always written: a narration with no lines writes `"entries": []`, which is a file
    // that says every clip was heard and nothing was to be said.
    pub entries: Vec<Entry>,
    /// Omitted when empty, like the prototype's `omitempty`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub silent: Vec<Silent>,
}

/// The performer this build speaks through and the blend delivered with it, both in the
/// cache key so a take from before them is never served as a current one. Fixed on
/// purpose: §4 asks for a key "deliberately stable across versions", so changing either
/// means a new prefix, not a new value here.
const TTS_PREFIX: &str = "25e0.85";

/// The voice that speaks the project's own audio: it adds nothing to the cache key, so
/// a project narrated before the voice picker existed keeps every take it already had.
const OWN_VOICE: &str = "own";

/// The parts of a take's name after the prefix, in the order they go on: `[roll#]`,
/// then `[voice|]`, then `text|emotion`. Splitting them out keeps each salt added once and
/// in one order, so a roll and a voice together read `25e0.85|2#aria|…`.
fn key_body(entry: &Entry, voice: Option<&str>, emotion: &str) -> String {
    let mut body = format!("{}|{}", entry.text, emotion);
    if let Some(voice) = voice.filter(|v| *v != OWN_VOICE) {
        body = format!("{voice}|{body}");
    }
    if entry.roll > 0 {
        // A line never re-rolled keeps the key it already had.
        body = format!("{}#{body}", entry.roll);
    }
    body
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

impl Narration {
    /// Playing order: by clip, then by placement within it (§4). Everything downstream
    /// leans on it — a line's window runs to the next entry — and a hand-edited time can
    /// break it, so both [`load`] and [`save`] put it back.
    pub fn sort(&mut self) {
        self.entries.sort_by(|a, b| {
            a.s.partial_cmp(&b.s).unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.at
                        .partial_cmp(&b.at)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
    }

    /// Is this clip marked silent — its last line deleted on purpose, kept by bounds?
    pub fn is_silent(&self, s: f64, e: f64) -> bool {
        self.silent.iter().any(|q| q.s == s && q.e == e)
    }

    /// Does this clip have a line at all? A silent marker means nothing once a line is
    /// on the clip again, which is what lets the marker become the line.
    pub fn has_line(&self, s: f64, e: f64) -> bool {
        self.entries.iter().any(|entry| entry.s == s && entry.e == e)
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
///
/// `25e0.85|[roll#][voiceKey|]text|emotion` — the prefix names the performer and the
/// blend that reached it, the roll is the user saying "same line, different draw", and a
/// voice other than [`OWN_VOICE`] must not serve the old speaker from cache nor throw its
/// lines away. `emotion` is the tag verbatim unless a vector is given, in which case the
/// vector keys it: a stronger blend, or a tag whose mapping has moved, is a different
/// performance of the same words.
///
/// The named-tag-to-vector mapping itself is 07-narrate's business, not this file's —
/// that is why the vector arrives as an argument rather than being looked up here.
pub fn tts_key(
    entry: &Entry,
    voice: Option<&str>,
    emotion_vector: Option<[f64; 8]>,
) -> String {
    let emotion = match emotion_vector {
        Some(vector) => format_vector(&vector),
        None => entry.emotion.clone(),
    };
    // The prefix stays first, so a take from before it is unreachable however the rest
    // of the key is salted.
    format!("{}|{}", TTS_PREFIX, key_body(entry, voice, &emotion))
}

/// The eight axes the engine mixes, joined as the key wants them. Rounded to three
/// decimals and written shortest-form: a blend scaled by a weight lands on numbers like
/// 0.16499999999999998, and this string is part of a filename.
fn format_vector(vector: &[f64; 8]) -> String {
    vector
        .iter()
        .map(|v| {
            // Round half away from zero, as the prototype's math.Round does: Rust's
            // f64::round already differs from Go's on the halves this can land on.
            let rounded = (v * 1000.0).round() / 1000.0;
            if rounded == 0.0 {
                // -0.0 would print as "-0", and a zero axis has no sign.
                return "0".to_string();
            }
            format!("{rounded}")
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The cache file holding that take: the first 8 bytes of the key's SHA-1, lowercase hex,
/// no extension — the caller adds `.wav` and the directory.
pub fn tts_file(key: &str) -> String {
    let digest = Sha1::digest(key.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// The take's random seed, cut from the same digest as its filename: one filename, one
/// seed, and only what moves the filename moves it. Left alone, the engine draws a fresh
/// seed per request (§4: bytes 8..11 of the digest, big-endian).
pub fn tts_seed(key: &str) -> u32 {
    let digest = Sha1::digest(key.as_bytes());
    u32::from_be_bytes([digest[8], digest[9], digest[10], digest[11]])
}

/// Read `narrate/narration.json`. A project that has never been narrated has no file,
/// which is the same state as a narration with no entries.
pub fn load(tree: &layout::Tree) -> Result<Narration, String> {
    let file = tree.narration_json();
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Narration::default()),
        Err(err) => return Err(format!("{}: {err}", file.display())),
    };
    let mut narration: Narration =
        serde_json::from_str(&text).map_err(|err| format!("{}: {err}", file.display()))?;
    // A hand-edited file may be out of playing order; it is put back here rather than
    // left for every reader to notice.
    narration.sort();
    Ok(narration)
}

/// The one function that writes `narration.json` (§4).
pub fn save(narration: &Narration, tree: &layout::Tree) -> Result<(), String> {
    let mut sorted = narration.clone();
    sorted.sort();
    let text = serde_json::to_string_pretty(&sorted).map_err(|err| err.to_string())?;
    // The path comes from layout, so there is one spelling of it; only its prefix inside
    // the project is taken back off, which is what lets write_file create the directory
    // and set §1's 0644.
    let file = tree.narration_json();
    let rel = file
        .strip_prefix(tree.dir())
        .unwrap_or(std::path::Path::new("narrate/narration.json"));
    tree.write_file(rel, text.as_bytes())
}
