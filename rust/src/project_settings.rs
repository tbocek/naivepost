//! §10-parameters#3-project-settings-tab-controls — the project's own tab controls.
//!
//! `spec/10-parameters.md` §3 lists what one project says about itself, edited on the
//! project's tabs rather than in `llm.conf`: Freq, language, narration on/off, copy
//! sources, the User Context; the encoder settings; the publish state; the aspect.
//!
//! §10's intro files values in three homes (`P.policy` §2, `P.machine` §1, `P.eng` §5).
//! §3 is the fourth, and it spells exactly one id with a `P.` prefix —
//! `P.project.frameInterval`, Freq — leaving the rest of its rows nameless. AGENT.md's
//! tuning-number rule gives those a bare `project.` prefix, so [`listed`] carries one
//! `P.` row and the rest bare ones: only the first answers to
//! [`params::Family::Project`]. Inventing further `P.project.*` ids would claim that §10
//! had written rows it did not.
//!
//! This module holds no numbers and no defaults of its own. Every answer here is read from
//! the owner that already holds it — [`crate::prepare`] for the Freq stops,
//! [`crate::project::Project::default`] and [`crate::project::Produce::default`] for the
//! stored values, [`crate::produce_screen`] for the encoder rows' option lists,
//! [`crate::fx_aspect`] for the shape words, [`crate::publish`] for whether the upload
//! text exists — so this is a catalogue over them, not a second copy that can drift.

use crate::{fx_aspect, prepare, produce_screen, project::Project};

/// The tab controls in §3's own order: the five project controls, then the thirteen
/// encoder rows (§A's list, taken whole from [`produce_screen::SETTINGS_ROWS`] rather
/// than retyped here), then the publish state, then the aspect.
///
/// `no_narration` is the project's flag: with narration off the game-volume row is not offered at all
/// (F4.8 — "Produce: no game-volume slider"), which [`produce_screen::settings_rows_shown`] decides.
pub fn rows_for(no_narration: bool) -> Vec<&'static str> {
    let mut rows = vec![
        "Freq",
        "Language",
        "Narration",
        "Copy into project",
        "User Context",
    ];
    rows.extend(produce_screen::settings_rows_shown(no_narration));
    rows.push("Publish");
    rows.push("Aspect");
    rows
}

/// Every row the tab can hold, for a narrated project — the full list, before F4.8 takes the
/// game-volume row away from one that has no narration.
pub fn rows() -> Vec<&'static str> {
    rows_for(false)
}

/// What the control shows before this project has an opinion. Read from the stored
/// default each time, never from a literal typed here.
///
/// Two of these read inverted, because the file does: Narration *on* is
/// `no_narration == false`, and "copy sources" *on* is `reference_sources == false`
/// (§01: the flag stores the negation on purpose).
pub fn default_for(row: &str) -> Option<String> {
    let empty = Project::default();
    match row {
        "Freq" => Some(prepare::freq_label(prepare::FREQ_DEFAULT)),
        "Language" => Some(empty.language.clone()),
        "Narration" => Some(tick(!empty.no_narration)),
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
        // The seven stops §3 lists, labelled by [`prepare::freq_label`] so the dropdown
        // and this list cannot spell a stop differently.
        "Freq" => Some(freq_labels()),
        "Narration" | "Copy into project" | "Frame timing" | "Channels" | "Frame edges" => {
            Some(TICK_VALUES.iter().map(|v| (*v).to_string()).collect())
        }
        other => produce_screen::options(other).map(|list| list.iter().map(|v| (*v).to_string()).collect()),
    }
}

/// Where this control writes, so a reader can follow the value back to one field instead
/// of finding two places that could hold it. The two inversions are named here as well as
/// in [`default_for`]: the tick reads on while the key reads `no_narration`.
pub fn stored_in(row: &str) -> &'static str {
    if row == "Freq" {
        return "project.interval";
    }
    if row == "Language" {
        return "project.language";
    }
    // The tick is narration on; the field is narration off.
    if row == "Narration" {
        return "project.no_narration (inverted)";
    }
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
///
/// Only Freq carries a `P.` id, because only Freq has one in §3. The others take the
/// bare `project.` prefix AGENT.md prescribes for a value §10 gives no `P.` id, and
/// therefore report [`params::Family::Other`] — the same treatment `machine.jpegQuality`
/// and `preview.playTickMs` get elsewhere in the catalogue.
pub fn listed() -> Vec<crate::params::Param> {
    let empty = Project::default();
    let encode_defaults = produce_screen::defaults();
    let mut params = vec![
        // The one `P.` row §3 spells. Stops and default both come from prepare, whose
        // doc comment already names this id; nothing here repeats 0.25 or 1.0.
        crate::params::Param {
            id: "P.project.frameInterval",
            spelled: stops_spelled(),
            from: "prepare::FREQ_STOPS / FREQ_DEFAULT",
        },
        param(
            "project.language",
            empty.language.clone(),
            "project::Project::default — language",
        ),
        param(
            "project.narration",
            tick(!empty.no_narration),
            "project::Project::default — no_narration (inverted)",
        ),
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

/// The Freq stops as labels, in stop order, printed by [`prepare::freq_label`]: a
/// fraction of a second bare, a second and up with its unit.
fn freq_labels() -> Vec<String> {
    prepare::FREQ_STOPS.iter().map(|stop| prepare::freq_label(*stop)).collect()
}

/// The stop list as §3 writes it: `0.25, 0.5, 1, 2, 3, 4, 5` — whole seconds without
/// the `.0`, built from [`prepare::FREQ_STOPS`] so the numbers stay single-sourced.
fn stops_spelled() -> String {
    prepare::FREQ_STOPS
        .iter()
        .map(|stop| {
            if *stop == stop.trunc() {
                format!("{}", *stop as i64)
            } else {
                format!("{stop}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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

/// The row label as the bare id's tail. Three rows are named for the field they write
/// rather than for their label, so a reader can line the id up with `Produce`:
/// "Quality (CRF)" → `crf`, "Frame timing" → `vfr`, "Frame edges" → `blurredEdges`.
/// The rest take their label's own words (`Container` → `container`, "Game audio" →
/// `gameAudio`) — the label is what the tab shows, and the field it writes is spelled by
/// [`stored_in`].
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
