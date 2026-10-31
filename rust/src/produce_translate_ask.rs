//! F5.4 S3's live half: the translation ask itself.
//!
//! [`crate::produce_translate`] owns every rule that settles with no server; this is the leg that
//! *telephones*. It builds the two messages §3.10 spells, dials the LLM through [`crate::server_leg`] (each
//! batch writes its timed `requests.tsv` row), reads the answer into the numbered map `produce_translate::Ask`
//! expects, and returns `Err` naming the model when the wire fails (the walk ships the original with a warning).
//!
//! A test drives it by pointing `LLM_SERVER` at a local fake; nothing takes a canned reply. Modelled on
//! [`crate::captions_ask`], but a translate call asks with thinking OFF (`Mode::Execute`: temperature 0.6, 8192
//! tokens, `enable_thinking` false), offers only `translate_line` and `finish`, and its answer is numbered
//! lines or tool calls rather than one JSON document.

use std::collections::BTreeMap;

use crate::exchanges::{Message, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::produce_subtitles as subs;
use crate::produce_translate::Ask;
use crate::requests::{Request, Service};
use crate::services;
use crate::settings::{self, Conf, Paths};
use crate::tools::Tool;

/// The step name this job's cache entries and timed rows carry: `cache/llm/translate/<key>`, keyed on the
/// exact request (§00-principles).
pub const STEP: &str = "translate";

/// The settings folder, read from the environment for EVERY batch so an address changed mid-render is
/// honoured by the next request (§02-services#1).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// The wording to ask with, through the settings read path so an edit wins (§04-prepare#6); with no
/// settings folder, the shipped `spec/prompts/translate.md`.
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The SYSTEM for one translate call: the house rules cut to this job's section plus the shipped translate
/// prompt. No User Context: §3.10's request is numbered lines and nothing else.
pub fn system(paths: Option<&Paths>) -> String {
    crate::prompt_assembly::system_message(
        &wording(paths, "system"),
        "translate",
        &wording(paths, "translate"),
        "",
        false,
        false,
    )
}

/// The body of one batch's request: the system message, §3.10's numbered-lines message verbatim, thinking
/// off, and ONLY the two tools §3.10 lists. `llm_request::body` writes the thinking switch and token cap.
pub fn body(model: &str, system: &str, user: &str) -> Result<String, String> {
    let offered = crate::tools::offered(crate::roles::Job::Subtitles);
    let tools: Vec<Tool> = offered
        .into_iter()
        .filter(|tool| matches!(tool, Tool::TranslateLine | Tool::Finish))
        .collect();
    let messages = vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user.to_string())],
        },
    ];
    // §4: every chat call is streamed; the answer is read by response content type, so a plain-JSON fake still works.
    // Thinking is read from the prompt table ([`crate::prompts::mode_of`]) so it cannot drift from the catalogue.
    let body = llm_request::body(
        STEP,
        model,
        &messages,
        crate::prompts::mode_of(STEP),
        Some(&tools),
        true,
    )?;
    Ok(body.to_string())
}

/// The deciding parts for one batch's cache slot: model, both messages, and the numbers asked for. The
/// numbers ride in the key because the re-ask sends the same wording over a different set.
fn cache_parts(system: &str, user: &str, model: &str, numbers: &[usize]) -> Vec<serde_json::Value> {
    let asked = numbers
        .iter()
        .map(|number| number.to_string())
        .collect::<Vec<String>>()
        .join(",");
    let request = llm_cache::Request {
        system,
        user,
        state: &asked,
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        model,
        thinking: false,
    };
    llm_cache::parts(&request)
}

/// A `Request` carrying what only the caller knows, for the timed row `server_leg::call` writes.
fn base_row(model: &str) -> Request {
    Request {
        started: String::new(),
        run: String::new(),
        step: "produce".to_string(),
        job: STEP.to_string(),
        service: Service::Llm,
        model: model.to_string(),
        kind: "chat".to_string(),
        ..Default::default()
    }
}

/// One batch's answer, read two ways, both real server behaviour:
///
/// 1. `translate_line` TOOL CALLS, §3.10's contract: `{"n": <number>, "text": "..."}` per call.
/// 2. The numbered-line answer `spec/prompts/translate.md` asks for, `n<TAB>text` per line, which a server
///    that ignores the tool offer replies with.
///
/// Per §3.10 a number outside this batch, or an empty text, is REFUSED: the line stays missing, which
/// triggers `produce_translate`'s one re-ask.
pub fn parse_answer(content_type: &str, reply: &str, numbers: &[usize]) -> BTreeMap<usize, String> {
    let model_reply = match llm_request::reply_from(content_type, reply) {
        Ok(reply) => reply,
        // Not a chat envelope: treat the whole body as the numbered answer (a plain-text server; an event stream that framed nothing).
        Err(_) => return parse_numbered(reply, numbers),
    };
    let mut got: BTreeMap<usize, String> = BTreeMap::new();
    for call in model_reply
        .tool_calls
        .iter()
        .filter(|call| call.name == Tool::TranslateLine.name())
    {
        let args: serde_json::Value = match serde_json::from_str(&call.args) {
            Ok(args) => args,
            Err(_) => continue,
        };
        let number = args
            .get("n")
            .and_then(|n| n.as_u64())
            .or_else(|| {
                args.get("n")
                    .and_then(|n| n.as_str())
                    .and_then(|s| s.trim().parse().ok())
            })
            .map(|n| n as usize);
        let text = args
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or_default();
        if let Some(number) = number {
            if numbers.contains(&number) && !text.trim().is_empty() {
                got.insert(number, text.to_string());
            }
        }
    }
    if got.is_empty() {
        return parse_numbered(model_reply.answer(), numbers);
    }
    got
}

/// `n<TAB>text`, one per line. A line whose number was not asked for, whose text is empty, or that has no number is dropped.
pub fn parse_numbered(answer: &str, numbers: &[usize]) -> BTreeMap<usize, String> {
    let mut got: BTreeMap<usize, String> = BTreeMap::new();
    for line in answer.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let split = line.find('\t').or_else(|| line.find(' '));
        let (head, text) = match split {
            Some(at) => (&line[..at], line[at + 1..].trim()),
            None => continue,
        };
        let number: usize = match head.trim().trim_end_matches('.').parse() {
            Ok(number) => number,
            Err(_) => continue,
        };
        if numbers.contains(&number) && !text.is_empty() {
            got.insert(number, subs::keep_breaks(text).to_string());
        }
    }
    got
}

/// The model leg for the render: settings are read per batch, the cache is honoured per batch, and every
/// dial writes its timed row. `Err` names the model and the reason; the walk ships that language's track
/// as the original with the merge's warning (§F5.4 S3, §00-principles).
pub fn ask(tree: &Tree, cancelled: &std::rc::Rc<dyn Fn() -> bool>) -> Box<Ask> {
    let tree = tree.clone();
    let cancelled = std::rc::Rc::clone(cancelled);
    Box::new(move |message: &str, numbers: &[usize]| {
        let conf = match current_paths().as_ref().map(settings::read) {
            Some(Ok(conf)) => conf,
            // An unreadable settings file is reported like a down server: the track keeps the original and says why.
            Some(Err(reason)) => return Err(format!("settings could not be read: {reason}")),
            None => Conf::default(),
        };
        ask_at(&tree, &conf, message, numbers, cancelled.as_ref())
    })
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    message: &str,
    numbers: &[usize],
    cancelled: &dyn Fn() -> bool,
) -> Result<BTreeMap<usize, String>, String> {
    let model = conf.model.clone();
    let system = system(current_paths().as_ref());
    let parts = cache_parts(&system, message, &model, numbers);
    if let Some(cached) = llm_cache::read(tree, STEP, &parts) {
        // §09 §10: the cached answer writes its own `cache` line, with no time on the wire. Only a COMPLETE answer is stored.
        crate::request_timing::record_cached(tree, base_row(&model));
        return Ok(parse_answer("application/json", &cached, numbers));
    }
    // No model configured is §02-services' own refusal, named.
    let body = body(&model, &system, message)?;
    let endpoint = services::llm_endpoint(conf);
    let reply = crate::llm_retry_leg::chat(
        STEP,
        tree,
        &base_row(&model),
        &endpoint,
        &body,
        cancelled,
        &crate::ui::window::log_line,
    );
    match reply {
        Ok(sent) => {
            let text = sent.body.clone();
            let got = parse_answer(&sent.content_type, &text, numbers);
            // S3: `cached only when complete`; a stored partial answer is partial forever and throws away the re-ask's chance.
            if subs::cached(got.len() == numbers.len() && !numbers.is_empty()) {
                // §6's first irregularity: the file holds the RECONSTRUCTED numbered text (a replay answers `merge`
                // directly), and the gap-filling second call is never filed. The leg sees only a message and its numbers,
                // so the repair is recognised from its heading ([`subs::is_repair_request`]); `gap_fill_caches` takes the
                // SECOND-CALL flag, so a first call earns its slot and the repair never does.
                let second_call = subs::is_repair_request(message);
                if llm_cache::gap_fill_caches(second_call) {
                    let reconstructed: Vec<String> = numbers
                        .iter()
                        .filter_map(|number| {
                            got.get(number)
                                .map(|text| subs::numbered_one(*number, text))
                        })
                        .collect();
                    let stored = llm_cache::translate_store_value(&reconstructed.join("\n"), &text);
                    llm_cache::store(tree, STEP, &parts, &stored);
                }
            }
            Ok(got)
        }
        Err(reason) => Err(format!(
            "the translation model {model:?} did not answer: {reason}"
        )),
    }
}
