use serde::{Deserialize, Serialize};

use crate::project::{Produce, Subtitles};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RawProduce {
    /// `"sidecar"` is the only value §6 gives a meaning to; any other string is left alone rather than mapped.
    pub subs: Option<String>,
    /// Dropped: no current setting corresponds to it.
    pub subs_from: Option<serde_json::Value>,
    pub game_vol: Option<f64>,
    pub bare: Option<bool>,
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

pub fn from_object<'de, D>(deserializer: D) -> Result<RawProduce, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let block = serde_json::Value::deserialize(deserializer)?;
    let Some(map) = block.as_object() else {
        return Ok(RawProduce::default());
    };
    let only = READ_KEYS
        .iter()
        .filter_map(|key| {
            map.get(*key)
                .cloned()
                .and_then(|value| keep_readable(key, value))
        })
        .collect::<serde_json::Map<String, serde_json::Value>>();
    Ok(serde_json::from_value(serde_json::Value::Object(only)).unwrap_or_default())
}

/// A value this module's fields can hold, or `None` (dropped from the second read) when it cannot: a
/// `game_vol` of `"loud"` has no reading here, and keeping it would fail the whole block.
fn keep_readable(key: &str, value: serde_json::Value) -> Option<(String, serde_json::Value)> {
    let ok = match key {
        "subs" | "subs_from" | "subtitles" | "resolution" => true,
        "game_vol" => value.is_number() || value.is_null(),
        "bare" => value.is_boolean() || value.is_null(),
        _ => false,
    };
    ok.then(|| (key.to_string(), value))
}

const READ_KEYS: [&str; 6] = [
    "subs",
    "subs_from",
    "game_vol",
    "bare",
    "subtitles",
    "resolution",
];

/// The stored `produce` block, read as plain JSON so the loader can keep the rows it understands when one
/// row's word is not one this app writes.
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
        "original" | "Original" => Some(crate::project::Resolution::Original),
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

pub fn fold_legacy(settings: &mut Produce, legacy: &RawProduce) -> Vec<String> {
    let mut report = Vec::new();
    if let Some(subs) = legacy.subs.as_deref() {
        if subs == "sidecar" {
            settings.subtitles = Subtitles::None;
            report
                .push("produce.subs \"sidecar\": subtitles now \"none in the video\"".to_string());
        } else {
            report.push(format!(
                "produce.subs {subs:?}: ignored, not a spelling this app knows"
            ));
        }
    }
    if legacy.subs_from.is_some() {
        report.push(
            "produce.subs_from: dropped, the cues are built on the produced clock".to_string(),
        );
    }
    // Present-and-zero is silence, which is what the file said; absent leaves the default alone, because the
    // default IS §4's answer for a project that never set the bed's level.
    if let Some(volume) = legacy.game_vol {
        settings.game_volume = volume;
        let said = if volume == 0.0 {
            "silence"
        } else {
            "the bed under the narration"
        };
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
