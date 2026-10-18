//! §08-produce#6's legacy `produce` keys, folded into [`Produce`] on load.
//!
//! The encoder settings are project state: `params::produce()` spells `P.policy.gameVolume` from
//! `Produce::default().game_volume`, and every other setting is a serde default on [`Produce`]. A project
//! written by the Go prototype stored that same block under four spellings the current model has none of, and
//! §08 §6 records what each means:
//!
//! - `subs: "sidecar"` → subtitles "none in the video" (`Subtitles::None`): the old word named the files beside
//!   the video, which is exactly what "none in the video" keeps — an .srt beside it, nothing in the container;
//! - `subs_from` → dropped. It named where subtitle text was taken from, a question this app no longer asks
//!   (the cues are built on the produced clock), so it is ignored rather than guessed at or made an error;
//! - `game_vol`: ABSENT means the default (§4's 0.22), while a stored `0` means silence. That distinction is
//!   the whole reason this module exists: serde alone cannot tell "not in the file" from "written as zero", and
//!   folding a missing key to 0.0 would quiet every project that never had the key;
//! - `bare` → the negation of the blurred-edges tick: the prototype's name for leaving the edges bare.
//!
//! Lives in its own file because `project.rs` holds the rest of the migration and this rule is one function with
//! one raw view of it. Nothing here runs on save: the fold happens once, on load, and is reported like every
//! other legacy fold so a project says out loud what was read into it.

use serde::{Deserialize, Serialize};

use crate::project::{Produce, Subtitles};

/// The legacy spellings, as they appear inside the stored `produce` block. Every field is an `Option` so
/// "absent" stays distinguishable from "present and empty/zero" — which is precisely the question §6 answers.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RawProduce {
    /// `"sidecar"` is the only value §6 gives a meaning to; any other string is left alone rather than mapped.
    pub subs: Option<String>,
    /// Dropped: no current setting corresponds to it.
    pub subs_from: Option<serde_json::Value>,
    pub game_vol: Option<f64>,
    pub bare: Option<bool>,
    /// The CURRENT keys, read as plain words. Not legacy at all — this is how §6's two stored-value fallbacks
    /// reach a live read: an enum field cannot answer "the file said a word we do not offer", so the block is
    /// read a second time here and [`fold_stored_words`] decides what an unknown word means.
    pub subtitles: Option<String>,
    pub resolution: Option<String>,
}

impl RawProduce {
    /// Whether the block carried none of the names this raw view reads — the common case, and the one that must
    /// add no note.
    pub fn is_empty(&self) -> bool {
        self.subs.is_none()
            && self.subs_from.is_none()
            && self.game_vol.is_none()
            && self.bare.is_none()
            && self.subtitles.is_none()
            && self.resolution.is_none()
    }
}

/// Deserializer for the second read of the stored `produce` block: keeps only [`RawProduce`]'s names, so the
/// field fills for any shape the file has (absent block, current keys, legacy ones, both) and never fails a load.
/// Named `from_object` because it is attached to the `produce` key itself (`#[serde(rename = "produce")]`): one
/// object, two readers, each taking the half of it that is its own.
pub fn from_object<'de, D>(deserializer: D) -> Result<RawProduce, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let block = serde_json::Value::deserialize(deserializer)?;
    let Some(map) = block.as_object() else { return Ok(RawProduce::default()) };
    let only = READ_KEYS
        .iter()
        .filter_map(|key| map.get(*key).cloned().and_then(|value| keep_readable(key, value)))
        .collect::<serde_json::Map<String, serde_json::Value>>();
    Ok(serde_json::from_value(serde_json::Value::Object(only)).unwrap_or_default())
}

/// A value this module's fields can hold, or `None` (dropped from the second read) when it cannot: a
/// `game_vol` of `"loud"` has no reading here, and keeping it would fail the whole block.
fn keep_readable(key: &str, value: serde_json::Value) -> Option<(String, serde_json::Value)> {
    let ok = match key {
        // Read into an `Option<String>` / `Option<f64>` / `Option<bool>` by the field above.
        "subs" | "subs_from" | "subtitles" | "resolution" => true,
        "game_vol" => value.is_number() || value.is_null(),
        "bare" => value.is_boolean() || value.is_null(),
        _ => false,
    };
    ok.then(|| (key.to_string(), value))
}

/// The six names this raw view reads: §6's four legacy spellings plus the two current keys whose stored word
/// may be one the app no longer offers. Spelled once so the filter above and [`RawProduce`] cannot disagree.
const READ_KEYS: [&str; 6] = ["subs", "subs_from", "game_vol", "bare", "subtitles", "resolution"];

/// The stored `produce` block, read as plain JSON so the loader can keep the rows it understands when one
/// row's word is not one this app writes.
///
/// WHY this exists: `Produce`'s enum fields are strict serde enums, so a file saying `"subtitles": "sidcar"`
/// makes the whole `produce` object fail to deserialize — and because `Raw.produce` carries
/// `#[serde(default)]`, that failure silently hands back `Produce::default()`, resetting container, codec,
/// preset, crf and every tick in the same block over one misspelled word. §6 asks for the opposite: an unknown
/// subtitle mode reads as *none in the video* and an unoffered resolution as the default, each on its own, and
/// nothing else moves. So the block is read here as values first ([`read`]), every readable row is kept, and
/// only the two words §6 names get a fallback.
pub struct Stored {
    /// The settings the file could state, with §6's two fallbacks applied where a word was stale.
    pub settings: Produce,
    /// What was folded or fell back, in §6's order; empty for a current project.
    pub report: Vec<String>,
}

/// Read the stored `produce` object. Anything that is not an object (absent key, `null`, a list) is no
/// statement at all, so it answers with the defaults and says nothing.
pub fn read(block: Option<&serde_json::Value>) -> Stored {
    let mut settings = Produce::default();
    let mut report = Vec::new();
    let Some(map) = block.and_then(|value| value.as_object().cloned()) else {
        return Stored { settings, report };
    };
    // Each row on its own: one unreadable value costs that row and never its neighbours, which is the whole
    // point of reading the block this way instead of through the strict `Produce` deserializer.
    let word = |key: &str| map.get(key).and_then(|value| value.as_str());
    if let Some(found) = word("container").and_then(parse_container) {
        settings.container = found;
    }
    if let Some(found) = word("codec").and_then(parse_codec) {
        settings.codec = found;
    }
    if let Some(found) = word("preset").and_then(parse_preset) {
        settings.preset = found;
    }
    if let Some(found) = word("frame_rate").and_then(parse_frame_rate) {
        settings.frame_rate = found;
    }
    if let Some(found) = map.get("audio_kbps").and_then(|value| value.as_u64()) {
        settings.audio_kbps = found as u32;
    }
    if let Some(found) = map.get("crf").and_then(|value| value.as_u64()) {
        settings.crf = found as u32;
    }
    if let Some(found) = map.get("game_volume").and_then(|value| value.as_f64()) {
        settings.game_volume = found;
    }
    for (key, field) in [
        ("vfr", &mut settings.vfr),
        ("mono", &mut settings.mono),
        ("blurred_edges", &mut settings.blurred_edges),
    ] {
        if let Some(found) = map.get(key).and_then(|value| value.as_bool()) {
            *field = found;
        }
    }
    if let Some(list) = map.get("translate").and_then(|value| value.as_array()) {
        settings.translate = list
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect();
    }
    // §6's two fallbacks, each asked only of its own row. A known word was already taken by the loops above
    // (they parse the same spellings), so only an unknown one reaches the door — and the door is
    // `produce_details::stored_subtitle_mode` / `stored_resolution`, which is what makes them live rules
    // rather than constants a test reads back.
    if let Some(stored) = word("subtitles") {
        match parse_subtitles(stored) {
            Some(found) => settings.subtitles = found,
            None => {
                settings.subtitles = crate::produce_details::stored_subtitle_mode(false);
                report.push(format!(
                    "produce.subtitles {stored:?}: unknown, read as \"{}\"",
                    subtitles_label(settings.subtitles)
                ));
            }
        }
    }
    if let Some(stored) = word("resolution") {
        match parse_resolution(stored) {
            Some(found) => settings.resolution = found,
            None => {
                settings.resolution = crate::produce_details::stored_resolution(false);
                report.push(format!(
                    "produce.resolution {stored:?}: no longer offered, read as the default \"{}\"",
                    resolution_label(settings.resolution)
                ));
            }
        }
    }
    Stored { settings, report }
}

// The row labels §A spells, for a report line that says what a stale word became. Same three answers as
// `produce_screen::{SUBTITLE_CHOICES, RESOLUTIONS}`, in that order.
fn subtitles_label(mode: Subtitles) -> &'static str {
    match mode {
        Subtitles::BurnedIn => "burned in",
        Subtitles::TrackInFile => "track in file",
        Subtitles::None => "none in the video",
    }
}

fn resolution_label(resolution: crate::project::Resolution) -> &'static str {
    match resolution {
        crate::project::Resolution::P720 => "720p",
        crate::project::Resolution::P1080 => "1080p",
        crate::project::Resolution::Original => "original",
    }
}

// The stored spellings, spelled from `project.rs`'s own `#[serde(rename)]` attributes so this reader and the
// writer cannot drift: `Subtitles` is `rename_all = "lowercase"`, `Resolution` renames each variant by hand.
fn parse_subtitles(word: &str) -> Option<Subtitles> {
    match word {
        "burnedin" => Some(Subtitles::BurnedIn),
        "trackinfile" => Some(Subtitles::TrackInFile),
        "none" => Some(Subtitles::None),
        _ => None,
    }
}

fn parse_resolution(word: &str) -> Option<crate::project::Resolution> {
    match word {
        "720p" => Some(crate::project::Resolution::P720),
        "1080p" => Some(crate::project::Resolution::P1080),
        "original" => Some(crate::project::Resolution::Original),
        _ => None,
    }
}

fn parse_container(word: &str) -> Option<crate::project::Container> {
    match word {
        "mp4" => Some(crate::project::Container::Mp4),
        "mkv" => Some(crate::project::Container::Mkv),
        "webm" => Some(crate::project::Container::Webm),
        _ => None,
    }
}

fn parse_codec(word: &str) -> Option<crate::project::Codec> {
    match word {
        "h264" => Some(crate::project::Codec::H264),
        "h265" => Some(crate::project::Codec::H265),
        "vp9" => Some(crate::project::Codec::Vp9),
        _ => None,
    }
}

fn parse_preset(word: &str) -> Option<crate::project::Preset> {
    match word {
        "ultrafast" => Some(crate::project::Preset::Ultrafast),
        "veryfast" => Some(crate::project::Preset::Veryfast),
        "fast" => Some(crate::project::Preset::Fast),
        "medium" => Some(crate::project::Preset::Medium),
        "slow" => Some(crate::project::Preset::Slow),
        "veryslow" => Some(crate::project::Preset::VerySlow),
        _ => None,
    }
}

fn parse_frame_rate(word: &str) -> Option<crate::project::FrameRate> {
    match word {
        "source" => Some(crate::project::FrameRate::Source),
        "60" => Some(crate::project::FrameRate::F60),
        "30" => Some(crate::project::FrameRate::F30),
        "24" => Some(crate::project::FrameRate::F24),
        _ => None,
    }
}

/// Fold the legacy keys into the settings the current model holds, and say what was folded.
///
/// Order matters only for the report: the notes come out in the order §6 lists the keys, so a person reading
/// the log sees the spec's list rather than a hash order.
pub fn fold_legacy(settings: &mut Produce, legacy: &RawProduce) -> Vec<String> {
    let mut report = Vec::new();
    if let Some(subs) = legacy.subs.as_deref() {
        if subs == "sidecar" {
            settings.subtitles = Subtitles::None;
            report.push("produce.subs \"sidecar\": subtitles now \"none in the video\"".to_string());
        } else {
            report.push(format!("produce.subs {subs:?}: ignored, not a spelling this app knows"));
        }
    }
    if legacy.subs_from.is_some() {
        report.push("produce.subs_from: dropped, the cues are built on the produced clock".to_string());
    }
    // Present-and-zero is silence, which is what the file said; absent leaves the default alone, because the
    // default IS §4's answer for a project that never set the bed's level.
    if let Some(volume) = legacy.game_vol {
        settings.game_volume = volume;
        let said = if volume == 0.0 { "silence" } else { "the bed under the narration" };
        report.push(format!("produce.game_vol {volume}: {said}"));
    }
    if let Some(bare) = legacy.bare {
        settings.blurred_edges = !bare;
        report.push(format!(
            "produce.bare {bare}: blurred edges {}",
            if settings.blurred_edges { "on" } else { "off" }
        ));
    }
    report
}
