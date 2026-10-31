//! §02-services#1's last clause, and [`09 §10`](../spec/09-llm-and-tools.md): a web search and a page
//! read are requests the app sent outside, so each one is timed and appended to the project's
//! `requests.tsv` like an LLM call or an audio.cpp task — "one line per request, appended the moment it
//! ends, whether it succeeded, failed, was given up … or cancelled".

use crate::layout::Tree;
use crate::request_timing::{self, Timer};
use crate::requests::{self, Outcome, Request, Service};

/// The kind spelled for a `web_search`: three queries, one budget, one answer (§02 §2's web tools).
pub const KIND_SEARCH: &str = "search";

/// The kind spelled for a `web_read`: one page, clipped to `crate::web_tools::WEB_READ_MAX_BYTES`.
pub const KIND_READ: &str = "read";

pub fn session_tree() -> Option<Tree> {
    Tree::new(crate::startup::open_dir()).ok()
}

/// A web row carrying only what the caller knows, before the timing columns.
pub fn base_row(step: &str, kind: &str, asked: &str, answer: &str) -> Request {
    Request {
        started: requests::format_started(
            crate::server_leg::epoch_ms(),
            crate::server_leg::local_offset_secs(),
        ),
        step: step.to_string(),
        job: step.to_string(),
        service: Service::Web,
        kind: kind.to_string(),
        // What went out is the query or the URL; what came back is the text handed to the model, since
        // that is the size the conversation actually grew by.
        sent_bytes: Some(asked.len() as i64),
        received_bytes: Some(answer.len() as i64),
        ..Default::default()
    }
}

/// Time one finished web call and append its row.
pub fn record(
    step: &str,
    kind: &str,
    asked: &str,
    answer: &str,
    started_ms: i64,
    ended_ms: i64,
    outcome: Outcome,
) -> Option<Request> {
    let tree = session_tree()?;
    let base = base_row(step, kind, asked, answer);
    let mut timer = Timer::sent(started_ms);
    timer.first_byte(ended_ms);
    timer.finished(ended_ms);
    let mut row =
        request_timing::finish_row(&base, &timer, outcome, request_timing::next_attempt(None));
    // §10's run column, same rule as every other door: the open run's page name, empty outside a run.
    request_timing::stamp_run(&mut row);
    // Written and ignored on failure: a cache-less, log-less step is still a correct step.
    requests::record(&tree, &row).ok()?;
    Some(row)
}

/// The refusal's sentence, for the row's outcome column. `None` when the answer was content.
pub fn refusal_reason(answer: &str) -> Option<String> {
    if let Some(reason) = serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|why| why.as_str())
                .map(str::to_string)
        })
        .filter(|why| !why.is_empty())
    {
        return Some(reason);
    }
    ["web_search failed: ", "web_read failed: "]
        .iter()
        .find(|prefix| answer.starts_with(**prefix))
        .map(|prefix| answer[prefix.len()..].trim().to_string())
        .filter(|why| !why.is_empty())
}
