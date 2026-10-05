//! F1.10 S3's live half: the ask itself.
//!
//! [`crate::joins`] owns every rule that settles with no server — which seams exist, what a reply
//! means, the ceilings, the marks and both files. What it never did was *telephone*: on a session
//! with seams the pass stopped at its own brief line, so the answers that decide the cut arrived
//! from nobody. This module is that leg. It builds the two messages §F1.10 spells, dials the LLM
//! over HTTP through the same [`crate::llm_retry_leg`] every chat ask rides (which is [`crate::server_leg`]
//! plus §3's patience), and hands the reply
//! to [`crate::joins::answer_join`] so the tool's ceilings — not a copy of them — decide what
//! may go.
//!
//! The seam a test drives this through is the settings address: point `LLM_SERVER` (or the
//! `LLM_SERVER` line of `llm.conf`) at a local fake and the very same endpoint resolution a
//! typed address goes through reaches it. Nothing here takes a canned reply, and a server that is
//! down answers `Err` naming it, which the pass carries as a refusal — "A refusal still keeps the
//! stumble" (§F1.10), so the video keeps the stumble rather than losing words to a guess.
//!
//! Why the wording arrives as an argument to the builders rather than as something hunted for:
//! the shipped text lives in `spec/prompts/<key>.md` and is read through
//! [`crate::prompts::shipped_text`], so — exactly as [`crate::narrate_call`] does — the door reads
//! the two wordings from [`crate::settings::prompt_text`] with that shipped wording as the fallback,
//! which is what the settings box itself shows when this machine holds no edit.
//!
//! could not be read either way, and new code goes in a new file in a tree this size.

use crate::cancel_leg;
use crate::exchanges::{Message, Mode, Part};
use crate::joins::{self, Outcome};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::project::Project;
use crate::requests::{Request, Service};
use crate::roles::Job;
use crate::seam_retries::{self, Asking};
use crate::services;
use crate::settings::{self, Conf, Paths};
use crate::tool_loop;
use crate::tools::{self, Tool};
use crate::tools::textedit::{Join, SEAM_REACH_WORDS};
use crate::word_list::{self, Word};

/// The shipped wording of one prompt key: the fenced ``` block of `spec/prompts/<key>.md`.
/// The reader moved to [`crate::prompts::shipped_text`] once the Prepare bench started showing the
/// same wording; this stays as the name every ask module already dials through.
pub fn shipped_file(key: &str) -> Option<String> {
    crate::prompts::shipped_text(key)
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6).
/// With no settings folder at all, the shipped wording is what goes out.
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The first ``` fenced block of a markdown file: [`crate::prompts::fenced_block`], re-exported
/// under the name this module's callers learned first.
pub use crate::prompts::fenced_block;

/// The settings folder, read from this process's environment each time an ask is built so a change
/// made while a run is going is honoured by its next request (§02-services#1: settings are
/// re-read per request).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// What one seam's ask came to, and whether it is finished asking.
#[derive(Debug, Clone, PartialEq)]
pub struct Answered {
    /// What the answer means, in the pass's own terms.
    pub outcome: Outcome,
    /// Whether this seam may be asked again. `false` only when a retry is still worth making.
    pub settled: bool,
    /// The answer text, so the caller can log it as it arrived. Empty when nothing was answered.
    pub reply: String,
    /// The ask failed at the server rather than at the answer, so the join is NOT asked again:
    /// a down server answers the same way however often it is called, and `seam_retries` counts
    /// refusals of answers, not failures of the wire.
    pub failed: bool,
    /// Whether this answer came back from `cache/llm/textedit` instead of the model. §6's summary line
    /// ("N join(s) repaired, M from the cache") counts exactly this, so the pass cannot guess it from
    /// whether a reply arrived — a cached one arrives looking identical.
    pub from_cache: bool,
}

/// One seam's live ask: build the two messages, honour the cache, dial, read the reply.
///
/// `attempt` is which ask this is, 0-based: the first, then each retry. It selects the cache slot
/// ([`seam_retries::ask_slot`]) so a retry cannot be served by the reply that was just refused —
/// the whole reason `P.machine.seamRetries` needed a parameter round.
///
/// A cache hit returns without a socket being opened: the answer was paid for already, and §6 keys
/// on the exact request so an edited prompt misses instead.
#[allow(clippy::too_many_arguments)]
pub fn ask(
    tree: &Tree,
    user_context: &str,
    k: usize,
    n: usize,
    before: &[String],
    after: &[String],
    attempt: u32,
    cancelled: &dyn Fn() -> bool,
) -> Answered {
    let conf = match current_paths().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => {
            // An unreadable settings file is reported the way a down server is: the pass keeps the
            // stumble and says why, rather than dialling a guessed address.
            return Answered {
                outcome: Outcome::Refused(format!("settings could not be read: {reason}")),
                settled: true,
                reply: String::new(),
                failed: true,
                from_cache: false,
            };
        }
        None => Conf::default(),
    };
    ask_at(tree, &conf, user_context, k, n, before, after, attempt, cancelled)
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
#[allow(clippy::too_many_arguments)]
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    user_context: &str,
    k: usize,
    n: usize,
    before: &[String],
    after: &[String],
    attempt: u32,
    cancelled: &dyn Fn() -> bool,
) -> Answered {
    let model = conf.model.clone();
    let system = join_system(user_context);
    let user = joins::request(user_context, k, n, before, after);
    let parts = seam_parts(&system, &user, &model, attempt);
    if let Some(cached) = llm_cache::read(tree, joins::STEP, &parts) {
        // A reply served from the cache costs no wire and writes no row: the request that earned it
        // is already in `requests.tsv`, and a second row for the same answer would double-count a
        // call that did not happen (§09 §10).
        //
        // A cached TOOL answer is stored as the counts it came to, not as the calls: the round that
        // ended a tool flow says only "I am done", so the drops themselves live in earlier rounds and
        // a replay would have to re-dial them. Storing the outcome is what lets the next press cost no
        // wire at all, and it is the same shape the prose path always stored — a reply this door reads.
        let outcome = judge(&cached, before, after);
        return Answered { outcome, settled: true, reply: cached, failed: false, from_cache: true };
    }
    // No model configured: asking again answers the same way, so this settles. Checked before the
    // loop dials, because a body cannot even be built without one.
    if let Err(reason) = chat_body_with(&model, &messages0(&system, &user), None) {
        return Answered {
            outcome: Outcome::Refused(reason.clone()),
            settled: true,
            reply: String::new(),
            failed: true,
            from_cache: false,
        };
    }
    let base = base_row(&model);
    let endpoint = services::llm_endpoint(conf);
    let messages = vec![
        Message { role: "system".to_string(), parts: vec![Part::Text(system.clone())] },
        Message { role: "user".to_string(), parts: vec![Part::Text(user.clone())] },
    ];
    // §3.2: this job has tools and they are offered. `drop_words`, `keep_join`, `get_words` and
    // `finish` are `tools::offered(Job::Joins)` already and `Join` implements every one of them;
    // what was missing was the loop that runs them, so the model could only ever answer in the
    // prototype's prose shape. `None` here would be the bug, not a simplification.
    let offered: Vec<Tool> = tools::offered(Job::Joins);
    let mut join = Join::new(
        &before.iter().map(String::as_str).collect::<Vec<&str>>(),
        &after.iter().map(String::as_str).collect::<Vec<&str>>(),
    );
    // The body of the round that ENDED the flow — the pair this ask's cache entry stands for — and
    // whether the wire itself failed, which is not a refusal and spends no retry (§seam_retries).
    let mut final_body: Option<String> = None;
    let mut wire_error: Option<String> = None;
    // What the tool surface refused during the flow. A model that corrects itself is fine — the counts
    // that survived are what count — but a flow that ended having only ever been refused has no answer,
    // and `judge`'s prose path would otherwise read its empty content as "nothing left out".
    let mut refusals: Vec<String> = Vec::new();
    // Asks this seam has put on the wire, which is how S5 knows it is still on its FIRST round: the
    // field is only worth re-sending without once, and only before anything was answered.
    let mut rounds: u32 = 0;
    // Every call the loop ran, in order. A tool flow's cache entry is written from these (see below):
    // the round that ends the flow says only "I am done", so the drops live in the earlier rounds and
    // neither the last body nor `refusals` (accepted calls are not refusals) records them.
    let mut calls: Vec<String> = Vec::new();
    // One reply read the way every chat job reads it, with the flow's own bookkeeping: the round that
    // carries no calls (or the `finish` call) is the answer, whatever else the model wrote beside it.
    let mut turn_of = |sent: &crate::server_leg::Sent| -> Result<tool_loop::Turn, String> {
        let reply = llm_request::reply_from(&sent.content_type, &sent.body)
            .map_err(|why| format!("{}: {why}", joins::STEP))?;
        let finishing = reply.tool_calls.iter().any(|call| call.name == Tool::Finish.name());
        if finishing || reply.tool_calls.is_empty() {
            // A round that brings NO CALLS is the answer, whatever the model also wrote beside it; a
            // round that brings calls is not, even when a thinking model slips its `"joined"` text in
            // alongside them — those calls are what gets run, and running them and then letting the
            // same round's prose decide too would drop the seam's words twice.
            final_body = Some(sent.body.clone());
        }
        Ok(tool_loop::Turn {
            content: reply.content.clone(),
            calls: reply.tool_calls.clone(),
            finish: finishing,
        })
    };
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        // §2 S5: a server that already refused the `tools` field is asked without it, silently — the
        // same memory every other tool job shares, so one refusal costs this seam's later asks nothing.
        let offered = crate::llm_tools_offer::tools_for(&endpoint.url, tools);
        let body = chat_body_with(&model, conversation, offered.as_deref())?.to_string();
        let dial = || {
            crate::llm_retry_leg::chat(
                joins::STEP,
                tree,
                &base,
                &endpoint,
                &body,
                cancelled,
                &crate::ui::window::log_line,
            )
        };
        let sent = match dial() {
            Ok(sent) => {
                rounds += 1;
                sent
            }
            Err(why) => {
                // S5's other half: a first-round error that reads as a refusal of the FIELD is honoured
                // — said once, and the same ask goes out without the field. A dead server is not a
                // refusal and stays a transport failure, which spends none of the seam's retries.
                if offered.is_some()
                    && rounds == 0
                    && crate::llm_tools_offer::honour_refusal(&endpoint.url, &why, 1)
                {
                    crate::ui::window::log_line(&tool_loop::refused_log(joins::STEP, &why));
                    rounds += 1;
                    let bare = chat_body_with(&model, conversation, None)?.to_string();
                    let again = crate::llm_retry_leg::chat(
                        joins::STEP,
                        tree,
                        &base,
                        &endpoint,
                        &bare,
                        cancelled,
                        &crate::ui::window::log_line,
                    )
                    .inspect_err(|why| wire_error = Some(why.clone()))?;
                    return turn_of(&again);
                }
                wire_error = Some(why.clone());
                return Err(why);
            }
        };
        turn_of(&sent)
    };
    let mut run_tool = |asked: &crate::exchanges::ToolCall| -> String {
        // An accepted `drop_words` is remembered in its answer form, which is what a later press reads
        // out of the cache through the same `judge` door a prose answer goes through.
        let answer = live_call(asked, &mut join, &mut refusals);
        if asked.name == Tool::DropWords.name() && !tool_refused(&answer) {
            let gone_before = join.dropped_count(crate::tools::textedit::Side::Before);
            let gone_after = join.dropped_count(crate::tools::textedit::Side::After);
            calls.push(drop_answer(before, after, gone_before, gone_after));
        }
        answer
    };
    let run = tool_loop::run(joins::STEP, messages, &offered, &mut ask, &mut run_tool);
    let outcome = match run.outcome {
        tool_loop::Outcome::Exhausted => Outcome::Refused(
            "still calling tools after the round budget -- no answer from it".to_string(),
        ),
        tool_loop::Outcome::Finished { .. } => {
            // The order matters and is the whole §3.2 rule: what the MODEL CALLED is the answer. A
            // thinking server often puts its `"joined"` text in the same round as the `drop_words`
            // call, and reading the prose as well would charge for the stretch twice (the dedupe then
            // swallows the rest of the take). So: counts the tool accepted, else a refusal the tool
            // gave, else the prose answer of a round that made no calls at all.
            if join.kept()
                || join.dropped_count(crate::tools::textedit::Side::Before)
                    + join.dropped_count(crate::tools::textedit::Side::After)
                    > 0
            {
                outcome_of(&join)
            } else if !refusals.is_empty() && final_body.as_deref().map(empty_answer).unwrap_or(true) {
                // The last reason the TOOL gave, quoted unchanged: the rule that refused it lives in
                // `Join`, and a copy of its sentence here would be a second rule. Only when that last
                // round carried no prose of its own — a model that says something instead of calling is
                // answering, and `judge` below is the door that reads an answer.
                tool_refusal_outcome(&refusals)
            } else if let Some(body) = final_body.as_deref() {
                judge(&answer_of("application/json", body), before, after)
            } else {
                Outcome::Refused("answered with nothing to read".to_string())
            }
        }
    };
    // A transport failure is not a refusal of an answer: the join is not retried against a server that
    // is not there, and nothing is cached.
    if let Some(reason) = wire_error {
        return Answered { outcome: Outcome::Refused(reason), settled: true, reply: String::new(), failed: true, from_cache: false };
    }
    if outcome.usable() {
        // Cached only when usable (§F1.10): a refusal must go out again rather than be replayed from
        // the slot it failed in. A PROSE flow stores the round that ended it, unchanged; a TOOL flow
        // stores the answer its accepted calls came to, in that same prose shape, so one reader
        // (`judge`, in the cache branch above) answers both and a re-press costs no socket at all.
        let store_body = match calls.last() {
            Some(answer) => Some(answer.clone()),
            None => final_body.clone(),
        };
        if let Some(body) = store_body {
            llm_cache::store(tree, joins::STEP, &parts, &reply_envelope(&body));
        }
    }
    Answered { outcome, settled: true, reply: final_body.unwrap_or_default(), failed: false, from_cache: false }
}

/// The answer a set of accepted `drop_words` counts is worth, in the one shape this door reads back:
/// the joined text, spelled as the words kept (everything shown, minus what went). Storing THAT rather
/// than the raw calls is what lets a cached tool answer be read by `judge` with no second parser, and
/// it is honest about what the flow decided — the drops, not the round that announced them.
fn drop_answer(before: &[String], after: &[String], before_gone: usize, after_gone: usize) -> String {
    let mut kept: Vec<&str> = before
        .iter()
        .map(String::as_str)
        .take(before.len().saturating_sub(before_gone))
        .collect();
    kept.extend(after.iter().map(String::as_str).skip(after_gone));
    serde_json::json!({ "joined": kept.join(" ") }).to_string()
}

/// The name a side is answered under, the same one `Join` prints in its own tool replies.
fn side_name(side: crate::tools::textedit::Side) -> &'static str {
    match side {
        crate::tools::textedit::Side::Before => "before",
        crate::tools::textedit::Side::After => "after",
    }
}

/// A reply's envelope in the shape every chat job reads: what `ask_at` itself stores when a prose
/// answer wins, so a reply planted from outside lands in exactly the form the cache branch reads back.
/// Public for that reason — the private `envelope` a live ask uses is the same bytes.
pub fn reply_envelope(answer: &str) -> String {
    serde_json::json!({ "choices": [ { "message": { "content": answer } } ] }).to_string()
}

/// Read a reply the way every chat job reads it, then let the rules already written decide.
fn judge(reply: &str, before: &[String], after: &[String]) -> Outcome {
    let mut join = Join::new(
        &before.iter().map(String::as_str).collect::<Vec<&str>>(),
        &after.iter().map(String::as_str).collect::<Vec<&str>>(),
    );
    joins::answer_join(&mut join, reply, before, after)
}

/// The join's answer when a tool flow ended with the TOOL refusing everything and the last round
/// carried no prose of its own: the last reason, quoted unchanged. `Join` owns the rule that refused
/// it, so copying a sentence here would be a second rule — this only picks which one to say.
/// Public because the live path calls it and the test that pins the wording reaches it the same way.
pub fn tool_refusal_outcome(refusals: &[String]) -> Outcome {
    Outcome::Refused(
        refusals
            .last()
            .map(|reason| refusal_reason(reason))
            .unwrap_or_else(|| "answered with nothing to read".to_string()),
    )
}

/// Whether a reply body carries no answer text at all — the shape of a plain `finish` call, which
// means the model had nothing to say and the tool's own refusals are the whole of its answer.
fn empty_answer(body: &str) -> bool {
    answer_of("application/json", body).trim().is_empty()
}

/// The outcome a `Join` arrived at: `keep_join` wins, else the counts its accepted calls added up
/// to, else nothing was dropped and the seam stands as it was (`Kept`).
fn outcome_of(join: &Join) -> Outcome {
    let before_gone = join.dropped_count(crate::tools::textedit::Side::Before);
    let after_gone = join.dropped_count(crate::tools::textedit::Side::After);
    if join.kept() {
        Outcome::Kept
    } else if before_gone + after_gone > 0 {
        Outcome::Applied { before: before_gone, after: after_gone }
    } else {
        Outcome::Kept
    }
}

/// Run one tool call through the SAME door the live loop uses. Named `_for_test` because nothing in
/// the program reaches for it: the loop hands calls to its own closure, which wraps `run_one`.
pub fn call_for_test(
    asked: &crate::exchanges::ToolCall,
    join: &mut Join,
    refusals: &mut Vec<String>,
) -> String {
    live_call(asked, join, refusals)
}

/// The `Outcome` a `Join` arrived at, for the case that pins a tool answer against a prose answer.
pub fn outcome_for_test(join: &Join) -> Outcome {
    outcome_of(join)
}

/// The body a seam's ask would send, with this job's tools offered. The one thing a caller cannot get
/// from the live path without a socket: it proves the offer and the thinking flag ride together.
pub fn body_for_test(shown: &[String]) -> String {
    let messages = messages0("system wording", &shown.join(" "));
    let offered = tools::offered(Job::Joins);
    chat_body_with("test-model", &messages, Some(&offered))
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// The live loop's tool door: run one call, keep the refusal it produced if it produced one (the
/// model still gets the tool's own `{"error": …}` text back — §3.2's correction loop — and the caller
/// keeps the reason for the join's own refusal line). One pass over `run_one`, so an accepted drop is
/// never counted twice.
fn live_call(asked: &crate::exchanges::ToolCall, join: &mut Join, refusals: &mut Vec<String>) -> String {
    let answer = run_one(asked, join);
    if tool_refused(&answer) {
        refusals.push(answer.clone());
    }
    answer
}

/// Whether a tool's answer is a refusal — a `{"error": …}` reply the model can act on. Same question
/// `fix_call` asks of its tools, restated over one JSON read rather than reached into that module for:
/// the shape is `tools::error`'s, which is this file's own business.
fn tool_refused(answer: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|value| value.get("error").and_then(serde_json::Value::as_str).map(str::to_string))
        .is_some()
}

/// The reason a refusal gave, for putting in front of the model on the next try and for the log line.
fn refusal_reason(answer: &str) -> String {
    serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|value| value.get("error").and_then(serde_json::Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// One tool call answered. The TOOL decides what a legal drop is — the snap bound, the ceilings, one
/// stretch at the join — and its `{"error": …}` sentence goes back to the model, which is §3.2's
/// refusal loop. Re-deciding any of it here would be a second copy of a rule `Join` already owns.
fn run_one(asked: &crate::exchanges::ToolCall, join: &mut Join) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&asked.args).unwrap_or_else(|_| serde_json::json!({}));
    let side = |key: &str| match args.get(key).and_then(|v| v.as_str()) {
        Some("before") => Some(crate::tools::textedit::Side::Before),
        Some("after") => Some(crate::tools::textedit::Side::After),
        _ => None,
    };
    let count = || args.get("count").and_then(|v| v.as_u64()).map(|v| v as usize);
    match asked.name.as_str() {
        "drop_words" => match (side("side"), count()) {
            (Some(side), Some(count)) => join.drop_words(side, count),
            _ => tools::error("drop_words needs \"side\" (\"before\" or \"after\") and \"count\""),
        },
        "keep_join" => join.keep_join(),
        // §3.5's read: more words than the brief showed, for a seam that looked longer than the
        // extract. Reading is not an answer, so it never ends the flow.
        "get_words" => match (side("side"), count()) {
            (Some(side), Some(count)) => tools::ok(&serde_json::json!({
                "side": side_name(side),
                "words": join.get_words(side, count),
            })),
            _ => tools::error("get_words needs \"side\" (\"before\" or \"after\") and \"count\""),
        },
        "finish" => tools::ok(&serde_json::json!({
            "finished": true,
            "dropped_before": join.dropped_count(crate::tools::textedit::Side::Before),
            "dropped_after": join.dropped_count(crate::tools::textedit::Side::After),
        })),
        other => tools::error(&format!("{other} is not one of this job's tools")),
    }
}

/// SYSTEM for a join call: the house rules cut to textedit's sections, the job's own wording, and
/// the precedence rule only when there is a User Context to outrank anything (§09 §8 puts it last).
fn join_system(context: &str) -> String {
    let paths = current_paths();
    crate::prompt_assembly::system_message(
        &wording(paths.as_ref(), "system"),
        "textedit",
        &wording(paths.as_ref(), "textedit"),
        context,
        false,
        false,
    )
}

/// The chat body: thinking on (`joins::thinking_on`), with this job's tools offered when there are
/// any. `None` for the tools means the same request minus the `tools` key — what §2 S5 resends to a
/// server that refuses the field.
/// [`chat_body_with`] over the two starting messages, with no tools offered: the shape a §2 S5 resend
/// takes to a server that refuses the field. The live loop performs that resend itself (see `ask_at`),
/// so nothing calls this — kept out rather than carried under an `allow(dead_code)` that would let the
/// next reader believe it is the bare-body door.
fn messages0(system: &str, user: &str) -> Vec<Message> {
    vec![
        Message { role: "system".to_string(), parts: vec![Part::Text(system.to_string())] },
        Message { role: "user".to_string(), parts: vec![Part::Text(user.to_string())] },
    ]
}

/// [`chat_body`] over a conversation the tool loop has grown, with the offer it wants sent.
fn chat_body_with(
    model: &str,
    conversation: &[Message],
    offered: Option<&[Tool]>,
) -> Result<serde_json::Value, String> {
    llm_request::body(model, conversation, Mode::Thinking, offered, false)
}

/// The answer text out of a chat reply: `content`, never the reasoning. Routed by §1's rule — an
/// event-stream body is read as chunks, anything else as one JSON envelope.
fn answer_of(content_type: &str, reply: &str) -> String {
    llm_request::reply_from(content_type, reply)
        .map(|parsed| parsed.answer().to_string())
        // A body that is neither JSON nor a stream answers with nothing here; `judge` turns the empty
        // answer into the step's own degraded verdict, which is what it did before this door existed.
        .unwrap_or_default()
}

/// The deciding parts for one join ask, and so the cache key [`ask_at`] looks up. The retry's slot rides
/// in `state` — the texts, model and thinking flag are otherwise identical, and the slot is what keeps the
/// two asks apart (§6). Public because a caller that wants to plant or check an answer must reach the very
/// door the pass hashes: the bytes on the wire, the system prompt's wording included, decide the key, so
/// the caller passes the two finished messages rather than the seam's numbers (`joins::request` and
/// `join_system` are private, and re-deriving them here would let a planted reply drift from the ask).
pub fn seam_parts(
    system: &str,
    user: &str,
    model: &str,
    attempt: u32,
) -> Vec<serde_json::Value> {
    let state = seam_retries::ask_slot(system, attempt);
    let request = llm_cache::Request {
        system,
        user,
        state: &state,
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        model,
        thinking: joins::thinking_on(),
    };
    llm_cache::parts(&request)
}

/// A `Request` carrying what only the caller knows, for the timed row `server_leg::call` writes.
fn base_row(model: &str) -> Request {
    Request {
        started: String::new(),
        run: String::new(),
        step: "prepare".to_string(),
        job: "textedit".to_string(),
        service: Service::Llm,
        model: model.to_string(),
        kind: "chat".to_string(),
        ..Default::default()
    }
}

/// What the whole pass came to.
#[derive(Debug, Clone, Default)]
pub struct Pass {
    /// Per word of the input list: whether it survives into `final.txt`.
    pub kept: Vec<bool>,
    /// Every line the pass spoke, in order, for the run's page.
    pub logs: Vec<String>,
    /// Words removed at the seams, before dedupe.
    pub by_seams: usize,
    /// Words removed in all, seams and dedupe together — S6's first number.
    pub removed: usize,
    /// Separate dropped runs — S6's "stretch(es)".
    pub stretches: usize,
    /// Recordings this pass took out entire, for the yellow wash on Cut.
    pub whole: Vec<String>,
}

/// The whole join pass with the wire in it: every seam asked in order, each answer matched back,
/// then S4's dedupe, S5's marks and S6's two files written together.
///
/// The BEFORE and AFTER windows are taken off the words still standing, which is what makes a
/// later seam see a clean join: showing it what an earlier seam already removed made the
/// one-stretch check count that stretch here and refuse an answer that was right (join 22 of the
/// ETH lecture, fixed 2026-09-18).
pub fn run_seams(
    tree: &Tree,
    project: &Project,
    words: &[Word],
    times: &[(f64, f64)],
) -> Pass {
    let total = words.len();
    let mut kept = vec![true; total];
    // What was standing when the pass began, so the whole-take flag below can tell a word this pass
    // removed from one that was already gone.
    let started_kept: Vec<bool> = kept.clone();
    let mut pass = Pass { kept: kept.clone(), ..Default::default() };
    if total == 0 {
        return pass;
    }
    // S1: too few joinable words to hold a join. `press_joins` has already reported the count, so
    // this says nothing rather than printing the same sentence twice.
    if joins::too_few(words) {
        return pass;
    }
    let seams = joins::seams(words);
    let n = seams.len();
    let retries = seam_retries::SEAM_RETRIES;
    let cancelled = cancel_leg::cancel_check_now();
    let mut applied = 0usize;
    let mut from_cache = 0usize;
    for (index, seam) in seams.iter().enumerate() {
        let k = index + 1;
        let before = window_before(words, &kept, *seam);
        let after = window_after(words, &kept, *seam);
        if before.is_empty() || after.is_empty() {
            // Nothing standing on one side: there is no join left to ask about here.
            continue;
        }
        let mut asking = Asking::start(retries);
        loop {
            let answered = ask(
                tree,
                &project.context,
                k,
                n,
                &before,
                &after,
                asking.attempt,
                &cancelled,
            );
            match answered.outcome {
                Outcome::Applied { before: off_before, after: off_after } => {
                    take_off_end(&mut kept, *seam, off_before);
                    take_off_start(&mut kept, *seam, off_after);
                    applied += 1;
                    if answered.from_cache {
                        from_cache += 1;
                    }
                    pass.logs.push(applied_log(k, n, off_before, off_after));
                    break;
                }
                Outcome::Kept => {
                    if answered.from_cache {
                        from_cache += 1;
                    }
                    pass.logs.push(kept_log(k, n));
                    break;
                }
                Outcome::Refused(reason) => {
                    if answered.failed {
                        pass.logs.push(joins::refusal_line(k, &reason, "nothing removed there"));
                        break;
                    }
                    match asking.refused() {
                        Some(tail) => pass.logs.push(joins::refusal_line(k, &reason, tail)),
                        None => {
                            pass.logs.push(joins::refusal_line(k, &reason, "nothing removed there"));
                            break;
                        }
                    }
                }
            }
        }
    }
    pass.by_seams = kept.iter().filter(|keep| !**keep).count();
    // The seconds actually spoken, which F1.11 asks for when it decides whether a retake's onset is
    // clear of speech — read off the same word list the marks came from so the two never disagree.
    let spoken: Vec<(f64, f64)> = words.iter().map(|word| (word.start, word.end)).collect();
    // S4: a saying doubled straight across a cut loses its earlier copy.
    let written: Vec<String> = words.iter().map(|word| word.written.clone()).collect();
    let sources: Vec<String> = words.iter().map(|word| word.source.clone()).collect();
    pass.logs.extend(joins::dedupe_across(&written, times, &mut kept));
    // S5: the dropped runs become marks, and F1.11 places their edges on the sound — the words fence
    // both edges and each lane's cached envelope chooses where inside the fence. No envelope (no wave
    // cache for that recording) leaves the mark on its word times, which is the pass's documented
    // fallback, not a failure.
    let aligned: Vec<crate::edges::AlignedWord> = words
        .iter()
        .map(|word| crate::edges::AlignedWord { word: word.match_word.clone(), s: word.start, e: word.end })
        .collect();
    let held = crate::edges::load(tree, &joins::session_lanes(tree, words));
    let (marks, edge_notes) = joins::marks_for(times, &kept, &aligned, crate::edges::finder(&held), &spoken);
    pass.logs.extend(edge_notes);
    // Only words THIS pass removed count toward the flag: `kept` also carries words that were
    // already gone before the joins ran, and blaming those would hang a whole-take wash on a
    // recording no join touched.
    let removed_by_pass: Vec<bool> = kept
        .iter()
        .enumerate()
        .map(|(index, keep)| !*keep && started_kept[index])
        .collect();
    let flagged = joins::whole_takes(words, &removed_by_pass);
    for (base, start, end) in &flagged {
        pass.logs.push(joins::whole_take_log(base, *start, *end));
        pass.whole.push(base.clone());
    }
    // S6: both files, in one step, together.
    if let Err(error) = joins::write_pair(tree, &sources, &written, &kept, &pass.whole, &marks) {
        pass.logs.push(format!("!!! text edit: could not write the text files -- {error}"));
        pass.kept = kept;
        return pass;
    }
    pass.removed = kept.iter().filter(|keep| !**keep).count();
    pass.stretches = stretches(&kept);
    // The completion line is spoken only when this pass changed the text: each seam already said
    // "nothing removed there", and a line of two zeroes on top of that reads as a repair that
    // forgot what it did.
    if pass.removed > 0 {
        // §6's own summary of the asks, said before the arithmetic line so a reader learns where the
        // answers came from before being told what they cost. `applied` counts the seams repaired; a
        // reply read back from `cache/llm/textedit` is counted in both halves, which is the point.
        pass.logs.push(crate::tools::textedit::joins_repaired_log(applied, from_cache));
        pass.logs.push(joins::written_log(pass.removed, total, pass.stretches));
    }
    pass.kept = kept;
    pass
}

/// What every line this pass speaks starts with. One constant so a reader, and the pass's own
/// "did I speak at all" check, have one thing to look for.
pub const JOIN_PREFIX: &str = ">>> text edit:";

/// The `>>> text edit: join k of n: ...` line for an applied seam.
pub fn applied_log(k: usize, n: usize, before: usize, after: usize) -> String {
    format!("{JOIN_PREFIX} join {k} of {n}: {before} word(s) off BEFORE, {after} off AFTER")
}

/// The line for a `keep_join` answer: the whole answer, said as such.
pub fn kept_log(k: usize, n: usize) -> String {
    format!("{JOIN_PREFIX} join {k} of {n}: kept whole -- nothing removed there")
}

/// The BEFORE window: the last [`SEAM_REACH_WORDS`] surviving words before the seam.
pub fn window_before(words: &[Word], kept: &[bool], seam: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let end = seam.min(words.len());
    let mut index = end;
    while index > 0 && out.len() < SEAM_REACH_WORDS {
        index -= 1;
        if kept.get(index).copied().unwrap_or(true) {
            out.push(words[index].written.clone());
        }
    }
    out.reverse();
    out
}

/// The AFTER window: the first [`SEAM_REACH_WORDS`] surviving words from the seam on.
pub fn window_after(words: &[Word], kept: &[bool], seam: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let start = seam.min(words.len());
    let mut index = start;
    while index < words.len() && out.len() < SEAM_REACH_WORDS {
        if kept.get(index).copied().unwrap_or(true) {
            out.push(words[index].written.clone());
        }
        index += 1;
    }
    out
}

/// Take `count` surviving words off the end of the run before `seam`. Only surviving words are
/// counted, so a second join on the same seam cannot charge for the same word twice.
pub fn take_off_end(kept: &mut [bool], seam: usize, count: usize) {
    let edge = seam.min(kept.len());
    for _ in 0..count {
        match kept[..edge].iter().rposition(|keep| *keep) {
            Some(index) => kept[index] = false,
            None => break,
        }
    }
}

/// Take `count` surviving words off the start of the run after `seam`.
pub fn take_off_start(kept: &mut [bool], seam: usize, count: usize) {
    let start = seam.min(kept.len());
    for _ in 0..count {
        match kept[start..].iter().position(|keep| *keep) {
            Some(offset) => kept[start + offset] = false,
            None => break,
        }
    }
}

/// How many separate dropped runs there are — S6's "stretch(es)".
pub fn stretches(kept: &[bool]) -> usize {
    let mut out = 0usize;
    let mut inside = false;
    for keep in kept {
        if !*keep && !inside {
            out += 1;
            inside = true;
        } else if *keep {
            inside = false;
        }
    }
    out
}

/// The bare form a window's words are matched on, exposed so a caller can check a window against
/// the session list it came from.
pub fn bare(text: &str) -> String {
    word_list::bare(text)
}
