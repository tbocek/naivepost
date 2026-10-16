//! The live `GET /v1/models` the Settings dialog's Fetch models button asks for —
//! [`spec/09-llm-and-tools.md` §9](../spec/09-llm-and-tools.md#9-model-list-and-tests).
//!
//! §9 names one request: `GET /v1/models`, fifteen seconds. [`crate::model_list`] describes it
//! ([`crate::model_list::request`] is its method, path and budget) and parses whatever comes back, but
//! nothing used to send it: the live provider left `"fetch-models"` to the fallback, so a real session's
//! press answered "nothing wired to probe fetch-models" and never touched the socket. This file is that
//! missing leg — nothing more. The verdict wording stays in `model_list`, the badge and log stay in
//! `ui::settings`.
//!
//! Why it lives in its own file rather than beside the other rows in [`crate::settings_probe`]: the row
//! there already carries the audio and sd routing, and one bounded GET with its own timeout rule reads
//! better as three functions than as another arm in that match.

use std::time::Duration;

use crate::model_list;
use crate::services::{Endpoint, Kind, Server};

/// §9's budget for this call, as a duration the socket can be given.
fn budget() -> Duration {
    Duration::from_secs(model_list::LIST_MODELS_SECONDS)
}

/// Ask one LLM server what it serves and hand back its body, exactly as it arrived.
///
/// The three parts come from [`model_list::request`] rather than being repeated here, so the endpoint
/// string and the fifteen-second bound cannot drift from what [`crate::model_list::parse_ids`]' tests
/// pin. It rides the same leg every other request does (`server_leg`, bearer from the endpoint's key,
/// the live ⏹ switch), so a stop pressed while the list is coming ends it like anything else.
///
/// WHY this call carries a client timeout when [`crate::services::timeout_for`] answers `None` for
/// [`Kind::ListModels`]: §1 leaves the ordinary model listing unbounded because a catalogue read is not
/// the slow thing on those servers, but §9 puts fifteen seconds on THIS one because a person is standing
/// at the dialog waiting for the dropdown. A list is cheap — no weights load, no generation — so fifteen
/// silent seconds means the address is wrong, and saying so beats a spinner nobody can explain.
pub fn models_at(endpoint: &Endpoint) -> Result<String, String> {
    let (method, path, seconds) = model_list::request();
    // The description is the caller's contract: if §9 ever changes the verb or the path, this says so
    // out loud instead of quietly sending something else.
    assert_eq!(
        (method, path),
        ("GET", model_list::ENDPOINT),
        "the list request must stay the one §9 names"
    );
    assert_eq!(seconds, model_list::LIST_MODELS_SECONDS);
    let cancelled = crate::cancel_leg::cancel_check_now();
    crate::server_leg::send_budget(
        Server::Llm,
        endpoint,
        Kind::ListModels,
        None,
        None,
        Some(budget()),
        &cancelled,
    )
    .map(|sent| sent.body)
}
