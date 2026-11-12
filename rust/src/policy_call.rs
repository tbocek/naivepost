//! F0.7 S2's live half: the derivation that actually telephones the LLM.

use crate::clip_pass_ask::wording;
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

pub const STEP: &str = "policy";

#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    /// Every `set_policy` the model called, in the order it called them, unedited: membership,
    /// range and the hand-set rule are [`policy::apply`]'s, so this only reports (§models propose).
    pub proposals: Vec<Proposal>,
    /// The round failed at the wire or the reply was unreadable, so the caller should say why
    /// rather than read an empty list as "the model wanted nothing".
    pub failed: bool,
    pub reason: String,
}

pub const FAULT_FIELD: &str = "llm";

impl Round {
    /// The one proposal a failure carries back. It names no real field on purpose, and the sentence
    /// that says what went wrong rides in its `because` (see [`FAULT_FIELD`]).
    fn failure(reason: String) -> Round {
        Round {
            proposals: Vec::new(),
            failed: true,
            reason,
        }
    }
}

fn session_tree() -> Option<Tree> {
    Tree::new(startup::open_dir()).ok()
}

pub fn answer_round(context: &str, refusals: &[String], policy: &Policy) -> Round {
    let conf = match settings::from_environment().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => {
            return Round::failure(format!("settings could not be read: {reason}"));
        }
        None => Conf::default(),
    };
    let tree = match session_tree() {
        Some(tree) => tree,
        None => return answer_at(None, &conf, context, refusals, policy),
    };
    answer_at(Some(&tree), &conf, context, refusals, policy)
}

pub fn answer_at(
    tree: Option<&Tree>,
    conf: &Conf,
    context: &str,
    refusals: &[String],
    policy: &Policy,
) -> Round {
    let req = policy::request(&crate::prompt_assembly::without_script(context));
    let model = conf.model.clone();
    let paths = settings::from_environment();
    let system = policy_system(paths.as_ref(), &req);
    let user = user_turn(&req, refusals, policy);
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
    let owned_tree = tree.cloned();
    let mut requests = 0u32;
    // The first failure's sentence, kept so a round that never got an answer can name the model and
    // the fault instead of reporting a blank silence (§failure is specific and local).
    let mut wire_failure: Option<String> = None;
    let mut answered = false;
    let mut proposals: Vec<Proposal> = Vec::new();
    let mut run_tool = |asked: &crate::exchanges::ToolCall| -> String {
        // Only `set_policy` proposes a value. A `get_context` or `finish` pushed here read as a
        // set_policy naming no field: refused, and the whole derivation asked a second time.
        if asked.name == "set_policy" {
            proposals.push(proposal_of(asked));
        }
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
                // S5's other half: an error on the loop's FIRST round that is not a stop is read as
                // the server again without the field, and the refusal is said once (§2 S5).
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
            // A batch of nothing but `set_policy` calls is the model saying it has finished
            // setting: end the flow here rather than letting §2's loop re-ask up to eight times for
            // a job with two tools.
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
        // Nobody answered at all -- every ask this leg tried came back as a failure or was never
        // sent.
        let reason =
            wire_failure.unwrap_or_else(|| "the server answered nothing at all".to_string());
        return Round::failure(format!("{STEP} call to {model}: {reason}"));
    }
    // Cache what the model actually said, so an unchanged context costs nothing next time (§6 keys
    // on the exact request; an edited context misses). Only a reply that produced calls is stored:
    // a round that errored must go out again rather than be replayed from the slot it failed in.
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

pub fn answer(req: &DerivationRequest, refusals: &[String]) -> Vec<Proposal> {
    let policy = live_policy();
    // A derivation whose first round could not be answered is over before it started: the model
    // that was missing on round one is no more reachable on round two, and re-asking spends §3's
    // whole patience ladder per round for one fault.
    if refusals.is_empty() {
        FAILED_DERIVATION.with(|flag| flag.set(false));
    }
    if FAILED_DERIVATION.with(|flag| flag.get()) {
        return Vec::new();
    }
    let round = answer_round(&req.context, refusals, &policy);
    if round.failed {
        FAILED_DERIVATION.with(|flag| flag.set(true));
        return vec![Proposal {
            field: FAULT_FIELD.to_string(),
            value: "unreachable".to_string(),
            because: round.reason.clone(),
        }];
    }
    round.proposals
}

thread_local! {
    static FAILED_DERIVATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The live policy handed to the round by the caller that owns one (`ui::window::derive_policy`). A
/// thread-local because the derivation callback's signature is the loop's and cannot carry it.
fn live_policy() -> Policy {
    LIVE_POLICY
        .with(|slot| slot.borrow().clone())
        .unwrap_or_default()
}

pub fn set_live_policy(policy: &Policy) {
    LIVE_POLICY.with(|slot| *slot.borrow_mut() = Some(policy.clone()));
}

pub fn note(what: &str) {
    crate::ui::window::note_startup(&[what.to_string()]);
}

thread_local! {
    static LIVE_POLICY: std::cell::RefCell<Option<Policy>> = const { std::cell::RefCell::new(None) };
}

/// Thinking off, read from the request rather than hard-coded: S2 says "thinking off" and
/// [`crate::prompts::thinking`] is the one table that knows whether the shipped `policy` prompt thinks.
fn mode_of(req: &DerivationRequest) -> Mode {
    if req.thinking {
        Mode::Thinking
    } else {
        // §09 §1's two modes are the whole thinking switch.
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

/// The user turn: the context the model is reading, assembled by §8's door
/// ([`crate::prompt_assembly::user_message`] — `policy` has no row in the per-job table, so its
/// system message keeps the whole house prompt and its user message gets the context block with no
/// speech rule), plus -- on a retry -- the sentences its last answer was refused for.
pub fn user_turn(req: &DerivationRequest, refusals: &[String], policy: &Policy) -> String {
    // The fields and their allowed values go with the first message: a model that had to call
    // get_context to learn them spent a whole request on it, every derivation.
    let fields = serde_json::Value::Array(fields_as_offered(policy)).to_string();
    let material = format!(
        "THE FIELDS you may set, each with its current value and the values it allows (an empty list: a number, or for language a code):\n{fields}\n\n\
         Set only the fields the context settles, all of them in one answer, and call finish when you are done. \
         get_context gives the context and this list again."
    );
    let mut out = crate::prompt_assembly::user_message(&req.context, req.prompt, &material);
    if !refusals.is_empty() {
        out.push_str("\n\nYour previous answer was refused:\n");
        for refusal in refusals {
            out.push_str(&format!("- {refusal}\n"));
        }
        out.push_str("Send set_policy again with the field names and values the list allows.");
    }
    out
}

fn chat_body(
    model: &str,
    messages: &[Message],
    mode: Mode,
    tools: Option<&[Tool]>,
) -> Result<String, String> {
    Ok(llm_request::body(STEP, model, messages, mode, tools, true)?.to_string())
}

fn all_proposals(calls: &[crate::exchanges::ToolCall]) -> bool {
    !calls.is_empty() && calls.iter().all(|call| call.name == "set_policy")
}

fn turn_of(sent: &crate::server_leg::Sent) -> Result<Turn, String> {
    let reply = llm_request::reply_from(&sent.content_type, &sent.body)
        .map_err(|why| format!("{STEP}: {why}"))?;
    Ok(Turn {
        content: reply.answer().to_string(),
        calls: reply.tool_calls,
        finish: false,
    })
}

/// A `set_policy` call as a proposal: three strings, exactly as the model typed them.
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
            // §3.11's result column is "ok or error (unknown field, out of range)", and directive B
            // says the app validates INSIDE the tool and hands the problem back as the tool result
            // -- so the verdict is computed here, by `policy::apply`, the one validator that knows
            // the catalogue.
            let mut scratch = policy.clone();
            match policy::apply(&mut scratch, &proposal) {
                Applied::Refused(say) => crate::tools::error(&say),
                Applied::Set => crate::tools::ok(&serde_json::json!({
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
        "finish" => crate::tools::ok(&serde_json::json!({"finished": true})),
        // §3.1's shared reads come through the one dispatcher, with this job's own context filled
        // in.
        "get_events" | "get_frames" | "get_lines" => crate::tool_reads::dispatch(
            &crate::tool_reads::Context {
                user_context: &req.context,
                // The policy job is not one of §3.1's read jobs, so it borrows the catalogue's own
                // name through the closest row and answers the fields itself (below).
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
        // P.policy.targetLengthSeconds — a number row, so the model gets the seconds as digits.
        "targetLengthSeconds" => format!("{}", policy.target_length_seconds.value),
        "language" => policy.language.value.clone(),
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

fn log_row(model: &str) -> Request {
    let mut row = base_row(Server::Llm, model, "POST", "", STEP, STEP);
    row.job = STEP.to_string();
    row
}

static NO_FOLDER: std::sync::OnceLock<Tree> = std::sync::OnceLock::new();

fn tree_holder(tree: &Option<Tree>) -> &Tree {
    match tree {
        Some(tree) => tree,
        None => NO_FOLDER.get_or_init(|| {
            let root = std::env::current_dir().unwrap_or_default();
            Tree::new(startup::session_dir(&root))
                .or_else(|_| Tree::new(root.join(startup::SESSION_NAME)))
                .expect("a session folder name is always a project name")
        }),
    }
}
