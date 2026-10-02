//! The caller [`crate::exchanges`] never had: the door that turns a chat call's request body and
//! reply into one §7 exchange, files it in the run's page, and hands the app-log lines to whoever is
//! logging.
//!
//! It lives beside the leg rather than inside `exchanges` for the same reason `server_leg` sits apart
//! from `requests`: `exchanges` owns the FORMAT (page name, meta line, sections, log wording) and must
//! stay free of any socket or wire type; this module owns the reading of a real request and reply and
//! the keeping of the live page for the session. Everything it returns is already-worded text —
//! recording can never fail the call it records (§7), so nothing here hands back an error.

use std::cell::RefCell;

use serde_json::Value;

use crate::exchanges::{self, Call, Mode, Part, Message, RunLog, ToolCall};
use crate::layout::Tree;
use crate::llm_request;

/// Read one chat call out of what went out and what came back.
///
/// The request half comes from the JSON body the program built (`llm_request::body`): the mode is the
/// thinking switch's own flag, so the page says which job the model was doing by the same fact that
/// chose its temperature. The reply half is read through [`llm_request::reply_from`], the door every
/// chat route already uses, rather than re-parsed here: two readers would be two dialects of one
/// server's answer, and the cut-off rule would end up living twice.
///
/// A reply that is not JSON at all (a refused 400 whose body is prose, a proxy's HTML) still produces a
/// call: its `reply` is the body as it came, so the page shows what the server actually said rather than
/// an empty answer that looks like a model with nothing to say.
pub fn call_of(
    step: &str,
    model: &str,
    request_body: &str,
    reply_body: &str,
    content_type: &str,
    took_secs: u64,
    error: Option<String>,
) -> Call {
    let asked: Value = serde_json::from_str(request_body).unwrap_or(Value::Null);
    // `enable_thinking` appears twice in a real body (top level and `chat_template_kwargs`) and both
    // copies always agree, so either answers the question. A body with no switch at all is execute.
    let thinking = asked
        .get("enable_thinking")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let messages = asked
        .get("messages")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(message_of).collect())
        .unwrap_or_default();
    // Tool calls the model asked for come back on the REPLY; the results the app answered them with are
    // separate `role: "tool"` messages on the next request, so a single call's page shows the asks with
    // empty results and the following section shows the answers. Pairing them across requests would
    // mean guessing which later call belongs to this one, which is not what the page is for.
    let reply = llm_request::reply_from(content_type, reply_body).ok();
    let mut tool_calls: Vec<ToolCall> = reply
        .as_ref()
        .map(|answered| {
            answered
                .tool_calls
                .iter()
                .map(|asked| ToolCall {
                    name: asked.name.clone(),
                    args: asked.args.clone(),
                    result: String::new(),
                })
                .collect()
        })
        .unwrap_or_default();
    if tool_calls.is_empty() {
        tool_calls = asked_tool_results(&asked);
    }
    let answered = match content_type {
        // A call that got no reply at all has nothing to read.
        "" => llm_request::Reply::default(),
        // An event stream whose chunks never carried a `finish_reason` still has an answer: the
        // reassembled deltas. `reply_from` refuses to call non-JSON a reply, which is right for a step
        // parsing words it must trust, wrong for a page whose job is to show what the model said.
        _ if llm_request::is_event_stream(content_type) => {
            let lines: Vec<&str> = reply_body.lines().collect();
            llm_request::from_events(&lines).reply
        }
        _ => match llm_request::reply_from(content_type, reply_body) {
            Ok(reply) => reply,
            // Not a chat answer at all (a refusal in prose, a proxy's HTML): keep the server's own words as
            // the reply so the page can show them.
            Err(_) => llm_request::Reply {
                content: reply_body.to_string(),
                ..Default::default()
            },
        },
    };
    let cut_off = answered.cut_off();
    Call {
        step: step.to_string(),
        model: model.to_string(),
        mode: if thinking { Mode::Thinking } else { Mode::Execute },
        took_secs,
        // The row knows the reasoning split only when the server reported it; §8 prints it when known.
        thinking_secs: None,
        messages,
        reply: answered.content.clone(),
        reasoning: (!answered.reasoning.is_empty()).then(|| answered.reasoning.clone()),
        tool_calls,
        cut_off,
        error,
    }
}

/// One message of a request body as the page shows it. A plain string content is one text part; an
/// array carries typed parts, and an `image_url` becomes [`Part::Image`] because that variant already
/// holds the data URL the wire sent — so the page shows exactly the picture the model saw.
fn message_of(value: &Value) -> Option<Message> {
    let role = value.get("role")?.as_str()?.to_string();
    let parts = match value.get("content")? {
        Value::String(text) => vec![Part::Text(text.clone())],
        Value::Array(list) => list
            .iter()
            .filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => part.get("text").and_then(Value::as_str).map(|t| Part::Text(t.to_string())),
                Some("image_url") => part
                    .get("image_url")
                    .and_then(|url| url.get("url"))
                    .and_then(Value::as_str)
                    .map(|url| Part::Image(url.to_string())),
                _ => None,
            })
            .collect(),
        _ => return None,
    };
    Some(Message { role, parts })
}

/// The tool results sitting in a request body, for a reply that named no calls of its own (a retried
/// ask whose answer arrived as an event stream without the reassembled calls).
fn asked_tool_results(asked: &Value) -> Vec<exchanges::ToolCall> {
    let Some(list) = asked.get("messages").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
        .map(|message| exchanges::ToolCall {
            name: message.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
            args: String::new(),
            result: message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
        .collect()
}

/// The page stamp §7 names the file with: `MMDD-HHMMSS`, month first.
///
/// Built from the epoch the way [`crate::requests::format_started`] is (no date-time dependency), but
/// in the six-digit-per-field shape §7 spells rather than that function's sortable ISO form, so it is
/// written here once and shared by every caller.
fn stamp() -> String {
    let now = crate::server_leg::epoch_ms();
    let local = now + i64::from(crate::server_leg::local_offset_secs()) * 1000;
    let (days, millis) = (local.div_euclid(86_400_000), local.rem_euclid(86_400_000));
    // Howard Hinnant's civil-from-days, the same algorithm `requests` uses, so the two stamps cannot
    // disagree about what day it is.
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_day = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_day + era * 400;
    let day_of_year = day_of_era - (365 * year_of_day + year_of_day / 4 - day_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    let (hours, rest) = ((millis / 3_600_000) as u32, millis % 3_600_000);
    let (minutes, seconds) = ((rest / 60_000) as u32, (rest % 60_000) / 1000);
    format!("{month:02}{day:02}{hours:02}{minutes:02}{seconds:02}{year:04}")
}

thread_local! {
    /// The page this run is writing, open from its first call and closed when the next run starts.
    ///
    /// This holder is THE run's page, not a second bookkeeping copy: `start_run` closes it (§7's "a run
    /// is the calls between two queue resets") and every chat call files itself here, so a page holds
    /// every call of the run that opened it whatever step or thread made them.
    static RUN: RefCell<RunLog> = RefCell::new(RunLog::new());
}

/// File one call in the run's page and hand every line it produced to `log` — the two per-call lines
/// plus, on the run's first call, the once-per-run link (§7). Returns nothing: a page that could not
/// be written is already worded as a line by `exchanges`, and failing the call it records is not its
/// job (§7's last sentence).
///
/// The section goes on disk whole. A chat call reports here only once its reply has been read to the
/// end, so there is no half-arrived answer left to grow; a caller that really does append as chunks
/// land would drive [`Page::begin`] / [`Page::stream`] / [`Page::complete`] itself instead.
pub fn record(tree: &Tree, step: &str, call: &Call, log: &dyn Fn(&str)) {
    let stamp = stamp();
    let lines = RUN.with(|held| {
        let mut run = held.borrow_mut();
        // Only the FIRST call of a page owes the once-per-run link, and `record` hands it out with the
        // send line. Closing the page afterwards retires this call's own boundary (§7: a run is the
        // calls between two resets), so a call with no run open gets a page of its own instead of
        // soaking up the next run's calls into it.
        let open = run_open();
        let lines = run.record(tree, &stamp, step, call);
        if !open {
            run.reset();
        }
        lines
    });
    for line in lines {
        log(&line);
    }
}

/// Test seam: end the open page so a later check of this binary does not inherit it. The program's own
/// boundary is [`note_run_started`], which closes the previous run's page from `runqueue::start_run`.
pub fn reset_run_for_test() -> Option<String> {
    RUN_OPEN.with(|cell| cell.set(false));
    RUN.with(|held| held.borrow_mut().reset())
}

thread_local! {
    /// Whether a run stands. [`note_run_started`] raises it and [`note_run_ended`] lowers it, following
    /// `RunBar::running`, so "a run is open" means one thing app-wide. A call found outside one is still
    /// recorded, in a page that ends with itself: §7 would rather keep a stray exchange than drop it.
    static RUN_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is a run open right now? Read by [`record`] to decide whether the page it just opened belongs to a
/// run or stands alone.
fn run_open() -> bool {
    RUN_OPEN.with(|cell| cell.get())
}

/// Note for the recorder that a run has begun (§7's page boundary): the previous run's page is closed,
/// so this run's first call opens its own, and the return says whether anything was closed. Called
/// from [`crate::runqueue::start_run`], the one call every ▶ and ↻ makes, so no flow has to remember
/// it.
pub fn note_run_started() -> bool {
    RUN_OPEN.with(|cell| cell.set(true));
    RUN.with(|held| held.borrow_mut().reset().is_some())
}

/// Note for the recorder that the run ended, so a later stray call does not land on a finished run's
/// page. The page itself stays on disk — closing the bookkeeping does not retire the record. Called from
/// [`crate::runqueue::end_run`], where `RunBar::running` goes back to `None`.
pub fn note_run_ended() {
    RUN_OPEN.with(|cell| cell.set(false));
}
