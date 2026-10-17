//! §02-services#1's last clause, and [`09 §10`](../spec/09-llm-and-tools.md): a web search and a page
//! read are requests the app sent outside, so each one is timed and appended to the project's
//! `requests.tsv` like an LLM call or an audio.cpp task — "one line per request, appended the moment it
//! ends, whether it succeeded, failed, was given up … or cancelled".
//!
//! The browser legs (`crate::web_browser`) launch a process rather than dial a socket, so no leg in
//! `server_leg` sees them and nothing else wrote their rows. This module is that missing door: it owns
//! the two kind names, builds the row, times it from the milliseconds the caller measured around the
//! ladder or the fetch, and files it.
//!
//! # What a web row says, and what it leaves out
//!
//! * `service` is [`Service::Web`], the fourth of [`request_timing::OUTSIDE_SERVICES`], which is what
//!   puts these rows into the run's per-service summary ("…; web 3 in 12s").
//! * `model` stays EMPTY. §09 §10 asks for "the service and the model", and a DuckDuckGo query or a page
//!   fetch has none: inventing `"firefox"` there would claim a model where the spec names a binary, and
//!   the binary's name belongs to the machine settings, not to a column every other service reads as a
//!   model id.
//! * A refusal is still a row with [`Outcome::Error`]: a bot wall, an empty page and a browser that could
//!   not start all went out (or tried to) and cost time, which is exactly what the file exists to show.
//! * No folder, no row. Recording never fails the work ([`00-principles.md`] rule 8), and a derivation or
//!   a tool call asked outside any project has nowhere to file, so the row is dropped rather than the
//!   step refused.

use crate::layout::Tree;
use crate::request_timing::{self, Timer};
use crate::requests::{self, Outcome, Request, Service};

/// The kind spelled for a `web_search`: three queries, one budget, one answer (§02 §2's web tools).
pub const KIND_SEARCH: &str = "search";

/// The kind spelled for a `web_read`: one page, clipped to [`crate::web_tools::WEB_READ_MAX_BYTES`].
pub const KIND_READ: &str = "read";

/// The session folder the headless legs file under — the same door [`crate::policy_call`] uses for its
/// cache and exchange page: the working copy beside this process's root. `None` when that path is not a
/// project folder at all, which is the case this module answers by writing nothing.
pub fn session_tree() -> Option<Tree> {
    let root = std::env::current_dir().unwrap_or_default();
    Tree::new(crate::startup::session_dir(&root)).ok()
}

/// A web row carrying only what the caller knows, before the timing columns.
///
/// Same shape as [`crate::server_leg::base_row`] — `run` empty outside a run, as every live door passes
/// it — with the service pinned to [`Service::Web`] and no model, for the reasons in the module note.
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
///
/// The caller holds the milliseconds because the browser legs measure around their own process:
/// `started_ms` is when the ladder or the fetch began, `ended_ms` when the answer was in hand. There is
/// no slot wait here — a web call takes no model slot (§09 §4 counts slots per MODEL) — so the timer is
/// [`Timer::sent`], whose wait column is zero and whose first byte is the answer itself.
///
/// Returns the row that was written, or `None` when there is no project folder to write it in.
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
    let row = request_timing::finish_row(
        &base,
        &timer,
        outcome,
        request_timing::next_attempt(None),
    );
    // Written and ignored on failure: a cache-less, log-less step is still a correct step.
    requests::record(&tree, &row).ok()?;
    Some(row)
}

/// The refusal's sentence, for the row's outcome column. `None` when the answer was content.
///
/// Two spellings exist and both mean "nothing arrived that the model can use": [`crate::tools::error`]'s
/// `{"error":"…"}` (a bad argument list) and the web legs' own `web_search failed: …` / `web_read failed: …`
/// sentences (§02 §2), which is what a bot wall, an empty page and a browser that could not start come back
/// as. Content is a hit list (`Results for …`, or `No results for any of the three queries.` — the ladder
/// ran and answered, so that one is an outcome, not a failure) or a page's text.
pub fn refusal_reason(answer: &str) -> Option<String> {
    if let Some(reason) = serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|value| value.get("error").and_then(|why| why.as_str()).map(str::to_string))
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
