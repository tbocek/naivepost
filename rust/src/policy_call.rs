//! F0.7 S2's live half: the derivation that actually telephones the LLM.
//!
//! [`crate::policy`] owns every rule that settles with no server — the catalogue, validation, both
//! origins, the round loop. What it never did was dial: the window fell back to a stand-in that
//! answered "could not reach the model", so a session with a User Context was never asked about it.
//! This module is that leg, modelled on [`crate::narrate_live`] (the one live leg already built on
//! [`crate::tool_loop`]): it builds the two messages S2 spells, offers `tool:get_context` and
//! `tool:set_policy`, dials over HTTP through [`crate::llm_retry_leg`] riding §3's patience and §2's
//! remembered `tools` refusal, runs each call through the answer that already exists, and hands the
//! `set_policy` calls back as [`policy::Proposal`]s so `policy::apply` — not a copy of it — decides
//! what may land.
//!
//! The seam a test drives this through is the settings address: point `LLM_SERVER` at a local fake and
//! the very same endpoint resolution a typed address goes through reaches it. Nothing here takes a
//! canned reply, and a server that is down answers with a refusal naming the model (§00-principles:
//! failure is specific and local), which `policy::derive` feeds back and the status line says out loud.
//!
//! One call to this module is ONE round of S2's loop. The rounds themselves stay in
//! [`crate::policy::derive`], which owns `DERIVE_ROUNDS` and the refusals — so the loop's cap is one
//! rule in one file and this leg cannot restart it.

use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::policy::{self, Applied, CatalogueRow, DerivationRequest, Proposal};
use crate::project::Policy;
use crate::requests::Request;
use crate::server_leg::base_row;
use crate::services::{self, Server};
use crate::settings::{self, Conf, Paths};
use crate::startup;
use crate::tool_loop::{self, Turn};
use crate::tools::Tool;

/// The step name this job's cache entries and timed `requests.tsv` rows carry. `policy` is the prompt
/// key too ([`policy::PROMPT`]), which is why one string serves both.
pub const STEP: &str = "policy";

/// What one derivation round came to: the model's proposals, or the reason there are none.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    /// Every `set_policy` the model called, in the order it called them, unedited: membership, range
    /// and the hand-set rule are [`policy::apply`]'s, so this only reports (§models propose).
    pub proposals: Vec<Proposal>,
    /// The round failed at the wire or the reply was unreadable, so the caller should say why rather
    /// than read an empty list as "the model wanted nothing". An answer with no calls IS a whole
    /// answer ([`policy::PROMPT`]'s own words: a default is a whole answer).
    pub failed: bool,
    /// The reason, naming the model and the fault. Empty when nothing failed.
    pub reason: String,
}

/// The field name a failed round's proposal carries: not a policy field, so `policy::derive` reads the
/// sentence back out of it and nothing else can mistake it for a value. The fault is that proposal's
/// `because` -- the round's own reason, naming the model and the fault -- NOT `policy::apply`'s "not a
/// policy field", which is right about a model that invented a field and wrong about a server that never
/// answered.
pub const FAULT_FIELD: &str = "llm";

impl Round {
    /// The one proposal a failure carries back. It names no real field on purpose, and the sentence that
    /// says what went wrong rides in its `because` (see [`FAULT_FIELD`]). Inventing a policy here would be
    /// the failure this whole item is about: the machine deciding what the model was supposed to say.
    fn failure(reason: String) -> Round {
        Round {
            proposals: Vec::new(),
            failed: true,
            reason,
        }
    }
}

/// The settings folder, read from this process's environment each time an ask is built so a change
/// made while a run is going is honoured by its next request (§02-services#1: settings are re-read
/// per request).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// The project folder the cache lives under: the working copy beside this process's root, which is
/// what the window's own log/readout paths use. `cache/llm` belongs to a project (§6), and a
/// derivation is about THIS session, so this is the folder whose cache makes a repeated context free.
fn session_tree() -> Option<Tree> {
    let root = std::env::current_dir().unwrap_or_default();
    Tree::new(startup::session_dir(&root)).ok()
}

/// S2's live round for this window's session, reading its own settings. The door the window calls.
///
/// `policy` is this session's live policy, handed in because `get_context` answers with the fields as
/// they stand: the model is told what it is being asked to change, which is the difference between a
/// derivation and a guess.
pub fn answer_round(context: &str, refusals: &[String], policy: &Policy) -> Round {
    let conf = match current_paths().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => {
            // An unreadable settings file is reported the way a down server is: nothing is invented,
            // and the sentence says which half of the machine was missing.
            return Round::failure(format!("settings could not be read: {reason}"));
        }
        None => Conf::default(),
    };
    let tree = match session_tree() {
        Some(tree) => tree,
        // No project folder to file the exchange page or the cache under: ask anyway. A derivation is
        // worth one request even in a folder that is not a project yet, and losing the cache is not a
        // reason to lose the answer.
        None => return answer_at(None, &conf, context, refusals, policy),
    };
    answer_at(Some(&tree), &conf, context, refusals, policy)
}

/// [`answer_round`] with the settings (and optionally the project folder) already resolved — the seam
/// a caller that read them itself uses, and the one a socket test points at a local fake.
pub fn answer_at(
    tree: Option<&Tree>,
    conf: &Conf,
    context: &str,
    refusals: &[String],
    policy: &Policy,
) -> Round {
    let req = policy::request(context);
    let model = conf.model.clone();
    let paths = current_paths();
    let system = policy_system(paths.as_ref(), &req);
    let user = user_turn(&req, refusals);
    let parts = cache_parts(&system, &user, &model, refusals.len());
    if let Some(tree) = tree {
        if let Some(cached) = llm_cache::read(tree, STEP, &parts) {
            // §09 §10: the cached answer writes its own `cache` line, with no time on the wire.
            crate::request_timing::record_cached(tree, log_row(&model));
            return Round {
                proposals: proposals_of(&cached),
                failed: false,
                reason: String::new(),
            };
        }
    }

    let endpoint = services::llm_endpoint(conf);
    let mode = mode_of(&req);
    // The cache/exchange home is optional, so the leg's `tree` argument is cloned into every dial from
    // one folder this function owns; a derivation with no project folder has nowhere to file to.
    let owned_tree = tree.cloned();
    let mut requests = 0u32;
    // The first failure's sentence, kept so a round that never got an answer can name the model and the
    // fault instead of reporting a blank silence (§failure is specific and local).
    let mut wire_failure: Option<String> = None;
    // Whether the dialled server ever answered once. `requests` counts asks this leg TRIED, including ones
    // that never reached a socket, so it cannot tell "nobody answered" from "answered, proposed nothing".
    let mut answered = false;
    let mut proposals: Vec<Proposal> = Vec::new();
    let mut run_tool = |asked: &crate::exchanges::ToolCall| -> String {
        proposals.push(proposal_of(asked));
        tool_answer(asked, &req, policy)
    };
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<Turn, String> {
        requests += 1;
        // §2 S5: a server that already refused the `tools` field is asked without it, silently.
        let offered = crate::llm_tools_offer::tools_for(&endpoint.url, tools);
        let body = match chat_body(&model, conversation, mode, offered.as_deref()) {
            Ok(body) => body,
            // No model configured: asking again answers the same way, and the sentence is §1's own.
            Err(why) => return Err(format!("{STEP} call to {model}: {why}")),
        };
        let base = log_row(&model);
        let reply = crate::llm_retry_leg::dial(
            STEP,
            tree_holder(&owned_tree),
            &base,
            &endpoint,
            &body,
            &crate::cancel_leg::cancel_check_now(),
            &note,
            &crate::llm_retry_leg::nap,
        );
        let sent = match reply.answer {
            Ok(sent) => sent,
            Err(why) => {
                wire_failure.get_or_insert_with(|| why.clone());
                // S5's other half: an error on the loop's FIRST round that is not a stop is read as the server
                // again without the field, and the refusal is said once (§2 S5).
                if offered.is_some()
                    && requests == 1
                    && crate::llm_tools_offer::honour_refusal(&endpoint.url, &why, 1)
                {
                    note(&tool_loop::refused_log(STEP, &why));
                    requests += 1;
                    let bare = chat_body(&model, conversation, mode, None)?;
                    let again = crate::llm_retry_leg::dial(
                        STEP,
                        tree_holder(&owned_tree),
                        &base,
                        &endpoint,
                        &bare,
                        &crate::cancel_leg::cancel_check_now(),
                        &note,
                        &crate::llm_retry_leg::nap,
                    );
                    return again
                        .answer
                        .map_err(|why| format!("{STEP} call to {model}: {why}"))
                        .and_then(|sent| turn_of(&sent));
                }
                return Err(format!("{STEP} call to {model}: {why}"));
            }
        };
        if !sent.ok() {
            let why = format!(
                "{STEP} call to {model} at {} answered {status}: {}",
                endpoint.url,
                sent.body.trim(),
                status = sent.status
            );
            wire_failure.get_or_insert_with(|| why.clone());
            if offered.is_some()
                && requests == 1
                && crate::llm_tools_offer::honour_refusal(&endpoint.url, &why, 1)
            {
                note(&tool_loop::refused_log(STEP, &why));
                requests += 1;
                let bare = chat_body(&model, conversation, mode, None)?;
                let again = crate::llm_retry_leg::dial(
                    STEP,
                    tree_holder(&owned_tree),
                    &base,
                    &endpoint,
                    &bare,
                    &crate::cancel_leg::cancel_check_now(),
                    &note,
                    &crate::llm_retry_leg::nap,
                );
                return again
                    .answer
                    .map_err(|why| format!("{STEP} call to {model}: {why}"))
                    .and_then(|sent| turn_of(&sent));
            }
            return Err(why);
        }
        answered = true;
        turn_of(&sent).map(|mut turn| {
            // A batch of nothing but `set_policy` calls is the model saying it has finished setting: end the
            // flow here rather than letting §2's loop re-ask up to eight times for a job with two tools.
            turn.finish = turn.finish || all_proposals(&turn.calls);
            turn
        })
    };

    let messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.clone())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user.clone())],
        },
    ];
    let run = tool_loop::run(STEP, messages, &req.tools, &mut ask, &mut run_tool);
    // §2's round lines belong to the user's log: they are what tells a reader why a derivation set
    // nothing, rather than leaving them guessing about a silent status line.
    for line in run.joined().lines() {
        note(line);
    }
    if !answered {
        // Nobody answered at all -- every ask this leg tried came back as a failure or was never sent.
        // Say which model and what happened rather than letting an empty proposal list stand in for
        // "the server is not there". A reply that came and proposed nothing is NOT this case: S2's prompt
        // says a default is a whole answer, so silence there is silence, not a fault.
        let reason =
            wire_failure.unwrap_or_else(|| "the server answered nothing at all".to_string());
        return Round::failure(format!("{STEP} call to {model}: {reason}"));
    }
    // Cache what the model actually said, so an unchanged context costs nothing next time (§6 keys on
    // the exact request; an edited context misses). Only a reply that produced calls is stored: a
    // round that errored must go out again rather than be replayed from the slot it failed in.
    if let Some(tree) = tree {
        let text = write_proposals(&proposals);
        if !text.is_empty() && text != "[]" {
            llm_cache::store(tree, STEP, &parts, &text);
        }
    }
    let _ = requests;
    Round {
        proposals,
        failed: false,
        reason: String::new(),
    }
}

/// The door shaped the way [`crate::policy::derive`] asks to be answered: the request it was built
/// with comes back in (its context is this session's User Context) and the refusals so far ride along,
/// so the model is told what it got wrong -- carried into the retry's user turn by [`user_turn`], which
/// is S3's "error back to the model". `policy` is the session's live policy, handed through
/// [`set_live_policy`] because the derivation callback's signature is [`policy::derive`]'s and cannot
/// carry a second argument; the request itself stays the pure wire that `policy::request` builds.
pub fn answer(req: &DerivationRequest, refusals: &[String]) -> Vec<Proposal> {
    let policy = live_policy();
    // A derivation whose first round could not be answered is over before it started: the model that was
    // missing on round one is no more reachable on round two, and re-asking spends §3's whole patience
    // ladder per round for one fault. The window installs this function as its own answer (see
    // `ui::window::live_policy_answer`), so the rule belongs here rather than in each caller.
    if refusals.is_empty() {
        FAILED_DERIVATION.with(|flag| flag.set(false));
    }
    if FAILED_DERIVATION.with(|flag| flag.get()) {
        return Vec::new();
    }
    let round = answer_round(&req.context, refusals, &policy);
    if round.failed {
        FAILED_DERIVATION.with(|flag| flag.set(true));
        // The fault as a refusal, said in the app's own words. No field was proposed, so there is nothing
        // for `policy::apply` to refuse and nothing invented to make it look like there was: dressing the
        // failure up as a call about a non-existent field made the status line read "llm is not a policy
        // field", which loses the one thing §failure-is-specific asks for -- the model and its fault.
        // `policy::derive` reads it back through [`Round::refusal`].
        return vec![Proposal {
            field: FAULT_FIELD.to_string(),
            value: "unreachable".to_string(),
            because: round.reason.clone(),
        }];
    }
    round.proposals
}

thread_local! {
    /// Whether this derivation's dial already failed (see [`answer`]). Reset by the first round of a
    /// derivation, which is the round whose failure means there is nothing to derive from.
    static FAILED_DERIVATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The live policy handed to the round by the caller that owns one (`ui::window::derive_policy`).
/// A thread-local because the derivation callback's signature is the loop's and cannot carry it.
fn live_policy() -> Policy {
    LIVE_POLICY
        .with(|slot| slot.borrow().clone())
        .unwrap_or_default()
}

/// Say which policy the next round should answer `get_context` with. Called by the window immediately
/// before it runs a derivation, so what the session holds and the model's view of it cannot drift.
pub fn set_live_policy(policy: &Policy) {
    LIVE_POLICY.with(|slot| *slot.borrow_mut() = Some(policy.clone()));
}

/// The derivation's own log sink: the window's log line, reached from a headless leg the same way every
/// other live flow writes to it. `policy_call`'s dial and its round lines both go here.
pub fn note(what: &str) {
    crate::ui::window::note_startup(&[what.to_string()]);
}

thread_local! {
    /// The policy the next round answers `get_context` from (see [`set_live_policy`]).
    static LIVE_POLICY: std::cell::RefCell<Option<Policy>> = const { std::cell::RefCell::new(None) };
}

/// Thinking off, read from the request rather than hard-coded: S2 says "thinking off" and
/// [`crate::prompts::thinking`] is the one table that knows whether the shipped `policy` prompt thinks.
fn mode_of(req: &DerivationRequest) -> Mode {
    if req.thinking {
        Mode::Thinking
    } else {
        // §09 §1's two modes are the whole thinking switch; every non-thinking job in the app takes
        // this one, and the body then carries `enable_thinking false` in both spellings.
        Mode::Execute
    }
}

/// SYSTEM: the house rules cut to this job's sections, the shipped `policy` wording, and the
/// precedence rule only when there is a User Context to outrank anything (§09 §8 puts it last).
fn policy_system(paths: Option<&Paths>, req: &DerivationRequest) -> String {
    crate::prompt_assembly::system_message(
        &wording(paths, "system"),
        req.prompt,
        &wording(paths, req.prompt),
        &req.context,
        false,
        false,
    )
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6). With
/// no settings folder at all, the shipped wording is what goes out — read through
/// [`crate::join_ask::shipped_file`], the one reader of `spec/prompts/<key>.md`'s fenced block.
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The user turn: the context the model is reading, assembled by §8's door ([`crate::prompt_assembly::user_message`]
/// — `policy` has no row in the per-job table, so its system message keeps the whole house prompt and its user
/// message gets the context block with no speech rule), plus -- on a retry -- the sentences its last answer
/// was refused for. S3's "error back to the model" is what this appends; the first request stays exactly
/// what S2 describes, cache key included.
pub fn user_turn(req: &DerivationRequest, refusals: &[String]) -> String {
    let material = "Set only the fields it settles. Call get_context if you want the fields and their ranges again, \
         and finish when you are done.";
    let mut out = crate::prompt_assembly::user_message(&req.context, req.prompt, material);
    if !refusals.is_empty() {
        out.push_str("\n\nYour previous answer was refused:\n");
        for refusal in refusals {
            out.push_str(&format!("- {refusal}\n"));
        }
        out.push_str("Send set_policy again with the field names and values the list allows.");
    }
    out
}

/// The chat body: the loop's messages, the thinking flag the request carries, and the tools offered
/// through the loop's own catalogue. `llm_request::body` builds the `tools` array from the tools it is
/// given, so `DerivationRequest::tool_schemas()` and this cannot drift apart.
fn chat_body(
    model: &str,
    messages: &[Message],
    mode: Mode,
    tools: Option<&[Tool]>,
) -> Result<String, String> {
    Ok(llm_request::body(STEP, model, messages, mode, tools, true)?.to_string())
}

/// Whether a round's calls mean the derivation is over: every one of them was a `set_policy`, which is
/// the only call that settles anything here.
///
/// This is what closes S2's loop after an answer rather than asking for another one. §2's quiet-round
/// rule ("a round with no tool calls and no `finish` is asked once to call `finish`") cannot do it on
/// its own: a reply that DOES call `set_policy` is not a quiet round, so the shared loop keeps going --
/// eight rounds of re-asking for a job whose whole surface is two tools and whose prompt says "a default
/// is a whole answer". A `get_context` in the batch means the model still wants information, so that
/// round goes back out; a batch of settled proposals does not.
fn all_proposals(calls: &[crate::exchanges::ToolCall]) -> bool {
    !calls.is_empty() && calls.iter().all(|call| call.name == "set_policy")
}

/// One dialled response as the tool loop reads it. There is no `finish` in this job's surface (S2 names
/// two tools), so a round with no calls is the loop's own quiet-round question to answer, not this leg's.
fn turn_of(sent: &crate::server_leg::Sent) -> Result<Turn, String> {
    let reply = llm_request::reply_from(&sent.content_type, &sent.body)
        .map_err(|why| format!("{STEP}: {why}"))?;
    Ok(Turn {
        content: reply.answer().to_string(),
        calls: reply.tool_calls,
        finish: false,
    })
}

/// A `set_policy` call as a proposal: three strings, exactly as the model typed them. A call whose
/// arguments are not JSON still becomes a proposal — with empty fields — because `policy::apply` is
/// what refuses a bad one, and it refuses by naming what was wrong (§failure is specific).
fn proposal_of(call: &crate::exchanges::ToolCall) -> Proposal {
    let args: serde_json::Value =
        serde_json::from_str(&call.args).unwrap_or_else(|_| serde_json::json!({}));
    let field = |key: &str| -> String {
        match args.get(key) {
            Some(serde_json::Value::String(text)) => text.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        }
    };
    Proposal {
        field: field("field"),
        value: field("value"),
        because: field("because"),
    }
}

/// The other of S2's two tools: `get_context` answers with the context the model is reading and the
/// five fields it may set, spelled as the catalogue spells them. Answering it is what makes a read
/// round possible — without an answer the model asks for the fields, gets silence, and the round cap
/// ends the derivation.
pub fn tool_answer(
    asked: &crate::exchanges::ToolCall,
    req: &DerivationRequest,
    policy: &Policy,
) -> String {
    match asked.name.as_str() {
        "set_policy" => {
            let proposal = proposal_of(asked);
            // §3.11's result column is "ok or error (unknown field, out of range)", and directive B says
            // the app validates INSIDE the tool and hands the problem back as the tool result -- so the
            // verdict is computed here, by `policy::apply`, the one validator that knows the catalogue.
            // It runs against a throwaway copy: answering is this function's job, applying stays
            // `policy::derive`'s loop, which still sees every proposal through `run_tool` and feeds its
            // refusals back as the next round's user text (S3).
            let mut scratch = policy.clone();
            match policy::apply(&mut scratch, &proposal) {
                Applied::Refused(say) => crate::tools::error(&say),
                Applied::Set => crate::tools::ok(&serde_json::json!({
                    // Echo what was recorded, so the model can see the app read its call correctly.
                    "recorded": true,
                    "field": proposal.field,
                    "value": proposal.value,
                })),
            }
        }
        "get_context" => crate::tools::ok(&serde_json::json!({
            "userContext": req.context,
            "fields": fields_as_offered(policy),
        })),
        // The shipped wording ends with "Call finish when you are done", so the model may well call it.
        // Answering it (rather than falling through to the unknown-tool error) is what lets §2's loop close
        // on the model's own signal instead of on our guess about it.
        "finish" => crate::tools::ok(&serde_json::json!({"finished": true})),
        // §3.1's shared reads come through the one dispatcher, with this job's own context filled in.
        "get_events" | "get_frames" | "get_lines" => crate::tool_reads::dispatch(
            &crate::tool_reads::Context {
                user_context: &req.context,
                // The policy job is not one of §3.1's read jobs, so it borrows the catalogue's own name
                // through the closest row and answers the fields itself (below).
                job: crate::roles::Job::ModelCut,
                policy: &[],
                lines: &[],
                events: &[],
                frames: &[],
            },
            asked,
        )
        .unwrap_or_else(|| crate::tools::error("this call answers set_policy and get_context only")),
        other => crate::tools::error(&format!(
            "{other} is not one of this call's tools \u{2014} call set_policy(field, value, because), or get_context"
        )),
    }
}

/// The catalogue as `get_context` hands it: each field's name, the `P.policy.*` id it answers to, the
/// value it holds right now, and the values it takes. Built from [`policy::derived_fields`] so the
/// model is offered exactly what `policy::apply` accepts — one fact, not two catalogues.
fn fields_as_offered(policy: &Policy) -> Vec<serde_json::Value> {
    policy::derived_fields()
        .iter()
        .map(|row: &CatalogueRow| {
            serde_json::json!({
                "name": row.field,
                "id": row.id,
                "values": row.values,
                "current": current_value(policy, row),
            })
        })
        .collect()
}

/// The value a catalogue row holds in this policy, spelled the way the model is being asked to spell it.
fn current_value(policy: &Policy, row: &CatalogueRow) -> String {
    match row.field {
        "markingPass" => match policy.marking_pass.value {
            crate::project::MarkingPass::Joins => "joins",
            crate::project::MarkingPass::None => "none",
            crate::project::MarkingPass::Retakes => "retakes",
        }
        .to_string(),
        "cutMode" => match policy.cut_mode.value {
            crate::project::CutMode::Words => "words",
            crate::project::CutMode::Model => "model",
        }
        .to_string(),
        "captionsPass" => on_off(policy.captions_pass.value),
        "speedPass" => on_off(policy.speed_pass.value),
        "decorationsPass" => on_off(policy.decorations_pass.value),
        // P.policy.targetLengthSeconds — a number row, so the model gets the seconds as digits; `0` is §10's
        // "none" and stays 0 rather than an empty string, which would read as a field with no value.
        "targetLengthSeconds" => format!("{}", policy.target_length_seconds.value),
        _ => String::new(),
    }
}

fn on_off(value: bool) -> String {
    if value {
        "on".to_string()
    } else {
        "off".to_string()
    }
}

/// The proposals as stored text: the mirror of [`proposals_of`], written by hand for the same reason
/// ([`Proposal`] carries no serde derives on purpose).
fn write_proposals(proposals: &[Proposal]) -> String {
    let entries: Vec<String> = proposals
        .iter()
        .map(|proposal| {
            format!(
                "{{\"field\":{},\"value\":{},\"because\":{}}}",
                serde_json::Value::from(proposal.field.clone()),
                serde_json::Value::from(proposal.value.clone()),
                serde_json::Value::from(proposal.because.clone())
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Cached proposals out of the stored text (the JSON [`answer_at`] wrote). A stored answer that does not
/// read as proposals is treated as no proposals, and the caller's own refusal rules apply.
///
/// Hand-read rather than `serde_json::from_str` because [`Proposal`] is the wire's three strings and
/// carries no `Deserialize`: adding one so a cache can be read would put a storage concern on the type
/// every validator takes, and this reader is six lines.
fn proposals_of(stored: &str) -> Vec<Proposal> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stored) else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return Vec::new();
    };
    let text = |entry: &serde_json::Value, key: &str| -> String {
        entry
            .get(key)
            .and_then(|field| field.as_str())
            .unwrap_or_default()
            .to_string()
    };
    entries
        .iter()
        .map(|entry| Proposal {
            field: text(entry, "field"),
            value: text(entry, "value"),
            because: text(entry, "because"),
        })
        .collect()
}

/// The deciding parts for one derivation. `state` carries the refusals count so a retry keys elsewhere
/// than the answer that was refused (`seam_retries::ask_slot`'s reason, restated for this job).
/// The deciding parts for one derivation. `state` carries how many refusals the round was asked with,
/// so a retry keys elsewhere than the answer that was refused (the reason `seam_retries::ask_slot`
/// exists): replaying the refused reply would re-refuse it and burn the round cap on a loop of one.
/// `thinking` is read from the request's own flag rather than written `false` here, so the cache cannot
/// key one way while the body goes out another.
fn cache_parts(system: &str, user: &str, model: &str, refusals: usize) -> Vec<serde_json::Value> {
    let state = if refusals == 0 {
        String::new()
    } else {
        format!("|refused {refusals}")
    };
    let request = llm_cache::Request {
        system,
        user,
        state: &state,
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        model,
        thinking: crate::prompts::thinking(STEP),
    };
    llm_cache::parts(&request)
}

/// The step and job this job's timed `requests.tsv` row and exchange page are filed under, beside the
/// model that answered. `base_row` already writes the timestamp and the service; this is what this call
/// adds to it -- the derivation is the one ask whose step and job are the same word.
fn log_row(model: &str) -> Request {
    let mut row = base_row(Server::Llm, model, "POST", "", STEP, STEP);
    row.job = STEP.to_string();
    row
}

/// A fallback project folder for a dial that has none: the working copy beside this process's root, the
/// same place [`session_tree`] reads. `dial` needs a folder to file the exchange page in, and a
/// derivation asked outside any project still gets its page rather than being skipped for the lack.
static NO_FOLDER: std::sync::OnceLock<Tree> = std::sync::OnceLock::new();

fn tree_holder(tree: &Option<Tree>) -> &Tree {
    match tree {
        Some(tree) => tree,
        None => NO_FOLDER.get_or_init(|| {
            let root = std::env::current_dir().unwrap_or_default();
            // `Tree::new` refuses a folder that does not end in `.naivepost`; the working copy always does,
            // and if even that cannot be named the session's own parent is the closest honest answer.
            Tree::new(startup::session_dir(&root))
                .or_else(|_| Tree::new(root.join(startup::SESSION_NAME)))
                .expect("a session folder name is always a project name")
        }),
    }
}
