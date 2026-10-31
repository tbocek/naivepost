//! The live `GET /v1/models` the Settings dialog's Fetch models button asks for.

use std::time::Duration;

use crate::model_list;
use crate::services::{Endpoint, Kind, Server};

/// §9's budget for this call, as a duration the socket can be given.
pub fn budget() -> Duration {
    let scale = std::env::var("NAIVEPOST_RETRY_SCALE")
        .ok()
        .and_then(|word| word.parse::<u64>().ok())
        .filter(|scale| *scale > 1)
        .unwrap_or(1);
    let seconds = model_list::LIST_MODELS_SECONDS / scale;
    Duration::from_secs(seconds.max(1))
}

/// One `GET /v1/models`, logged when there is a tree to log into, bounded by §9's fifteen seconds
/// either way.
pub fn models_ask(
    endpoint: &Endpoint,
    tree: &Option<crate::layout::Tree>,
) -> Result<String, String> {
    let (method, path, seconds) = model_list::request();
    // The description is the caller's contract: if §9 ever changes the verb or the path, this says
    // so out loud instead of quietly sending something else.
    assert_eq!(
        (method, path),
        ("GET", model_list::ENDPOINT),
        "the list request must stay the one §9 names"
    );
    assert_eq!(seconds, model_list::LIST_MODELS_SECONDS);
    let cancelled = crate::cancel_leg::cancel_check_now();
    match tree {
        // The §9 bound has to bite on the logged path too.
        Some(tree) => {
            let base = crate::server_leg::base_row(
                Server::Llm,
                "",
                Kind::ListModels.method(),
                "",
                "settings",
                "",
            );
            let reply = crate::server_leg::call_budget(
                tree,
                &base,
                Server::Llm,
                endpoint,
                Kind::ListModels,
                None,
                None,
                Some(budget()),
                &cancelled,
            )
            .1?;
            Ok(reply.body)
        }
        None => crate::server_leg::send_budget(
            Server::Llm,
            endpoint,
            Kind::ListModels,
            None,
            None,
            Some(budget()),
            &cancelled,
        )
        .map(|sent| sent.body),
    }
}
