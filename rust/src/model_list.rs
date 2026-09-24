//! The model list and the Settings probes — `spec/09-llm-and-tools.md` §9, wording in
//! [`03-shell.md` F0.13](../spec/03-shell.md#f013-tests).
//!
//! Two things live here. A `GET /v1/models` answer becomes a dropdown: the ids it carries, a verdict
//! for the row, and what the Use button writes into the Model box. And the two LLM probes F0.13 sends
//! are described as data, so their numbers (a 60 s completion at 16 tokens with thinking off, a 120 s
//! vision probe on a red square) are pinned somewhere other than a request that has to be run to be
//! read.
//!
//! Nothing here opens a socket or builds a widget. The request's three parts come from [`request`];
//! the bodies and verdicts are [`crate::checks`]'s, which already owns the test prompts and the red
//! square. What this module adds is the list, because a completion cannot answer "what is on this
//! server" — only the list can.

use crate::checks;
use serde_json::Value;

/// §9's budget for `GET /v1/models`: fifteen seconds.
///
/// A model list is cheap — no weights load, no generation — so a slow list means the address is
/// wrong rather than the model being cold. It must not inherit the 60 s a completion gets
/// ([`checks::LLM_TEST_SECONDS`]): Fetch models is pressed while a person waits on the dialog, and
/// a minute of spinner reads as a hung app rather than a bad port. Distinct from
/// [`checks::HEALTH_SECONDS`], which budgets `/health`; both are 15 today but they are different
/// rules under different sections.
pub const LIST_MODELS_SECONDS: u64 = 15;

/// The endpoint the list comes from.
pub const ENDPOINT: &str = "/v1/models";

/// The whole request in one place: method, path and timeout.
///
/// Returned as a tuple rather than sent because this module does no I/O — the caller hands these to
/// its HTTP client and feeds the body back to [`parse_ids`].
pub fn request() -> (&'static str, &'static str, u64) {
    ("GET", ENDPOINT, LIST_MODELS_SECONDS)
}

/// The ids a `/v1/models` body lists, in the order the server gave them.
///
/// Entries with a missing, empty or non-string `id` are skipped rather than failing the whole list:
/// a server that grows a field should not take the dropdown down with it, and one junk entry out of
/// twenty is not a reason to tell the operator there is nothing to pick.
///
/// Not sorted. The operator's own listing order says something about the box — which model was
/// loaded first, which is the default — and a sorted list throws that away while making a long list
/// harder to scan against what the server prints.
pub fn parse_ids(body: &Value) -> Vec<String> {
    let Some(entries) = body.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether the list is worth showing.
///
/// An empty list is an error naming the endpoint, because that is the thing to go look at. This is
/// *not* [`checks::llm_model_listed`], which answers the later question of whether the configured
/// id is among them; this one only asks whether anything arrived at all.
pub fn list_verdict(ids: &[String]) -> Result<String, String> {
    if ids.is_empty() {
        return Err(format!("{ENDPOINT} answered with no models -- nothing to pick"));
    }
    Ok(format!("{} model(s) served", ids.len()))
}

/// What the Use button writes into Model.
///
/// §9: "Use copies the id into Model". The copy happens only for an exact member of the list — no
/// prefix matching, because ids share prefixes (the vision and text variants of one base name), and
/// writing half an id produces a completion that fails for a reason the operator cannot see.
pub fn use_choice(ids: &[String], chosen: &str) -> Result<String, String> {
    let chosen = chosen.trim();
    if chosen.is_empty() {
        return Err("no model picked -- choose one from the list".to_string());
    }
    if ids.iter().any(|id| id == chosen) {
        return Ok(chosen.to_string());
    }
    Err(format!("{chosen:?} is not one of the models {ENDPOINT} listed"))
}

/// One of F0.13's two LLM probes, as data.
///
/// Plain fields rather than a trait object: the point is that the numbers are readable without
/// sending anything, and each probe's pass condition is one call away in [`probe_passes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    /// The row's name, as F0.13 calls it.
    pub name: &'static str,
    /// How long the probe may take before it is called a failure.
    pub seconds: u64,
    /// The token ceiling the request sets, if it sets one. The vision probe leaves it unset: a
    /// one-word answer needs no cap beyond the server's own, and capping an image prompt cuts the
    /// answer off inside the reasoning.
    pub max_tokens: Option<u32>,
    /// Whether the probe sends a picture. `false` is the plain completion.
    pub vision: bool,
}

/// The two probes F0.13 runs against the LLM: one completion, then one red-square vision probe.
///
/// Both force thinking off the way [`checks::llm_test_body`] does. Thinking is the reason the text
/// probe forces it off — a thinking model spends the sixteen tokens on reasoning and answers
/// nothing, redding a row for a server that is fine — and the same applies to a vision probe whose
/// reply has to name a colour.
pub fn probes() -> [Probe; 2] {
    [
        Probe {
            name: "LLM",
            seconds: checks::LLM_TEST_SECONDS,
            max_tokens: Some(checks::LLM_TEST_MAX_TOKENS),
            vision: false,
        },
        Probe {
            name: "LLM vision",
            seconds: checks::VISION_TEST_SECONDS,
            max_tokens: None,
            vision: true,
        },
    ]
}

/// Whether a reply passes a probe.
///
/// The vision probe defers entirely to [`checks::vision_verdict`] — the colour check is that
/// function's rule and lives nowhere else. The completion probe passes on any non-blank answer: F0.13
/// reports what came back rather than judging its content, because for a text model arriving at all
/// *is* the verdict.
pub fn probe_passes(probe: &Probe, reply: &str) -> Result<String, String> {
    if probe.vision {
        return checks::vision_verdict(reply);
    }
    if reply.trim().is_empty() {
        return Err("the model answered nothing at all".to_string());
    }
    Ok(format!("answered {:?}", reply.trim()))
}
