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
}

impl RawProduce {
    /// Whether the block carried none of the legacy keys — the common case, and the one that must add no note.
    pub fn is_empty(&self) -> bool {
        self.subs.is_none() && self.subs_from.is_none() && self.game_vol.is_none() && self.bare.is_none()
    }
}

/// Deserializer for the second read of the stored `produce` block: keeps only [`RawProduce`]'s four names, so the
/// field fills for any shape the file has (absent block, current keys, legacy ones, both) and never fails a load.
/// Named `from_object` because it is attached to the `produce` key itself (`#[serde(rename = "produce")]`): one
/// object, two readers, each taking the half of it that is its own.
pub fn from_object<'de, D>(deserializer: D) -> Result<RawProduce, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let block = serde_json::Value::deserialize(deserializer)?;
    let Some(map) = block.as_object() else { return Ok(RawProduce::default()) };
    let only = LEGACY_KEYS
        .iter()
        .filter_map(|key| map.get(*key).map(|value| ((*key).to_string(), value.clone())))
        .collect::<serde_json::Map<String, serde_json::Value>>();
    Ok(serde_json::from_value(serde_json::Value::Object(only)).unwrap_or_default())
}

/// The four names §6 records, spelled once so the filter above and [`RawProduce`] cannot disagree.
const LEGACY_KEYS: [&str; 4] = ["subs", "subs_from", "game_vol", "bare"];

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
