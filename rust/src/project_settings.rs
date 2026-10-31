//! §10-parameters#3-project-settings-tab-controls — the project's own tab controls.

use crate::{fx_aspect, produce_screen, project::Project};

/// The tab controls in §3's own order: the two project controls, then the thirteen
/// encoder rows (§A's list, taken whole from [`produce_screen::SETTINGS_ROWS`] rather
/// than retyped here), then the publish state, then the aspect.
pub fn rows() -> Vec<&'static str> {
    let mut rows = vec!["Copy into project", "User Context"];
    rows.extend(produce_screen::SETTINGS_ROWS.iter().copied());
    rows.push("Publish");
    rows.push("Aspect");
    rows
}

/// What the control shows before this project has an opinion. Read from the stored
/// default each time, never from a literal typed here.
pub fn default_for(row: &str) -> Option<String> {
    let empty = Project::default();
    match row {
        "Copy into project" => Some(tick(!empty.reference_sources)),
        // Empty means nothing is sent with the request, which is still the default's answer.
        "User Context" => Some(empty.context.clone()),
        "Publish" => Some(NOT_WRITTEN.to_string()),
        "Aspect" => Some(fx_aspect::SOURCE.to_string()),
        // The encoder rows are §A's, and their defaults are Produce's fields spelled by
        // the page that edits them.
        other => produce_screen::defaults()
            .into_iter()
            .find(|(name, _)| *name == other)
            .map(|(_, value)| value.to_string()),
    }
}

/// The choices a control offers, or `None` where it has none: Language is a free entry,
/// the User Context a text view, Publish a state rather than a selection, and Aspect is
/// offered by the Cut form ([`fx_aspect::ASPECTS`]) rather than by this tab.
pub fn values_for(row: &str) -> Option<Vec<String>> {
    match row {
        "Copy into project" | "Frame timing" | "Channels" | "Frame edges" => {
            Some(TICK_VALUES.iter().map(|v| (*v).to_string()).collect())
        }
        other => produce_screen::options(other)
            .map(|list| list.iter().map(|v| (*v).to_string()).collect()),
    }
}

/// Where this control writes, so a reader can follow the value back to one field instead
/// of finding two places that could hold it. The inversion is named here as well as in
/// [`default_for`]: the tick reads on while the key reads `reference_sources`.
pub fn stored_in(row: &str) -> &'static str {
    // The tick is copy into project; the field is reference-in-place.
    if row == "Copy into project" {
        return "project.reference_sources (inverted)";
    }
    if row == "User Context" {
        return "project.context";
    }
    if row == "Publish" {
        return PUBLISH_FILE;
    }
    if row == "Aspect" {
        return "cut.aspect";
    }
    if let Some(at) = produce_screen::SETTINGS_ROWS.iter().position(|r| *r == row) {
        return ENCODED_FIELDS[at];
    }
    ""
}

/// The §10 §3 catalogue: one row per tab control, in [`rows`]' order.
pub fn listed() -> Vec<crate::params::Param> {
    let empty = Project::default();
    let encode_defaults = produce_screen::defaults();
    let mut params = vec![
        param(
            "project.copySources",
            tick(!empty.reference_sources),
            "project::Project::default — reference_sources (inverted)",
        ),
        param(
            "project.context",
            context_spelling(&empty.context),
            "project::Project::default — context",
        ),
    ];

    for row in produce_screen::SETTINGS_ROWS.iter().copied() {
        // The spelling is the encoder page's own, so the row cannot disagree with what
        // the tab shows; `from` names that page and the row it read.
        let spelled = encode_defaults
            .iter()
            .find(|(name, _)| *name == row)
            .map(|(_, value)| (*value).to_string())
            .unwrap_or_default();
        params.push(param(
            Box::leak(format!("project.{}", id_tail(row)).into_boxed_str()),
            spelled,
            Box::leak(format!("produce_screen::defaults — {row}").into_boxed_str()),
        ));
    }

    params.push(param(
        "project.publish",
        NOT_WRITTEN.to_string(),
        "publish::is_written — publish.json absent",
    ));
    params.push(param(
        "project.aspect",
        fx_aspect::SOURCE.to_string(),
        "fx_aspect::SOURCE — cut.aspect empty",
    ));
    params
}

fn param(id: &'static str, spelled: String, from: &'static str) -> crate::params::Param {
    crate::params::Param { id, spelled, from }
}

/// A tick as §10 spells a boolean: `on` / `off`, the words §3 uses for
/// "narration on/off" and §A uses for the three encoder ticks.
fn tick(on: bool) -> String {
    if on {
        "on".to_string()
    } else {
        "off".to_string()
    }
}

/// An empty User Context catalogued as what it is rather than as a blank cell: §3 lists
/// the context with no default value, and the app sends nothing when it is empty.
fn context_spelling(context: &str) -> String {
    if context.is_empty() {
        "empty (nothing sent)".to_string()
    } else {
        context.to_string()
    }
}

fn id_tail(row: &str) -> String {
    match row {
        "Quality (CRF)" => "crf".to_string(),
        "Frame timing" => "vfr".to_string(),
        "Frame edges" => "blurredEdges".to_string(),
        other => {
            let words: Vec<&str> = other.split_whitespace().collect();
            let mut out = words[0].to_lowercase();
            for word in &words[1..] {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    out.push(first.to_ascii_uppercase());
                    out.push_str(chars.as_str());
                }
            }
            out
        }
    }
}

/// A tick's two values, off first — the order every tick in this app draws them.
const TICK_VALUES: [&str; 2] = ["off", "on"];

/// The publish state's file: presence is the answer (§5), so the row names the file
/// rather than a field.
const PUBLISH_FILE: &str = "publish.json";

/// What an unwritten upload text reads as. Kept as words because "none" would read like a
/// value chosen rather than work not yet done.
const NOT_WRITTEN: &str = "not written";

/// The field each encoder row writes, index-aligned with [`produce_screen::SETTINGS_ROWS`].
const ENCODED_FIELDS: [&str; produce_screen::SETTINGS_ROWS.len()] = [
    "produce.container",
    "produce.codec",
    "produce.preset",
    "produce.resolution",
    "produce.frame_rate",
    "produce.audio_kbps",
    "produce.subtitles",
    "produce.translate",
    "produce.game_volume",
    "produce.crf",
    "produce.vfr",
    "produce.mono",
    "produce.blurred_edges",
];
