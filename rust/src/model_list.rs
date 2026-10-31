//! The model list and the Settings probes — `spec/09-llm-and-tools.md` §9, wording in
//! [`03-shell.md` F0.13](../spec/03-shell.md#f013-tests).

use crate::checks;
use serde_json::Value;

/// §9's budget for `GET /v1/models`: fifteen seconds.
pub const LIST_MODELS_SECONDS: u64 = 15;

/// The endpoint the list comes from.
pub const ENDPOINT: &str = "/v1/models";

/// The whole request in one place: method, path and timeout.
pub fn request() -> (&'static str, &'static str, u64) {
    ("GET", ENDPOINT, LIST_MODELS_SECONDS)
}

/// The ids a `/v1/models` body lists, in the order the server gave them.
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
pub fn list_verdict(ids: &[String]) -> Result<String, String> {
    if ids.is_empty() {
        return Err(format!(
            "{ENDPOINT} answered with no models -- nothing to pick"
        ));
    }
    Ok(format!("{} model(s) served", ids.len()))
}

/// What the Use button writes into Model.
pub fn use_choice(ids: &[String], chosen: &str) -> Result<String, String> {
    let chosen = chosen.trim();
    if chosen.is_empty() {
        return Err("no model picked -- choose one from the list".to_string());
    }
    if ids.iter().any(|id| id == chosen) {
        return Ok(chosen.to_string());
    }
    Err(format!(
        "{chosen:?} is not one of the models {ENDPOINT} listed"
    ))
}

/// One of F0.13's two LLM probes, as data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    /// The row's name, as F0.13 calls it.
    pub name: &'static str,
    /// How long the probe may take before it is called a failure.
    pub seconds: u64,
    pub max_tokens: Option<u32>,
    /// Whether the probe sends a picture. `false` is the plain completion.
    pub vision: bool,
}

/// The two probes F0.13 runs against the LLM: one completion, then one red-square vision probe.
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
pub fn probe_passes(probe: &Probe, reply: &str) -> Result<String, String> {
    if probe.vision {
        return checks::vision_verdict(reply);
    }
    if reply.trim().is_empty() {
        return Err("the model answered nothing at all".to_string());
    }
    Ok(format!("answered {:?}", reply.trim()))
}
