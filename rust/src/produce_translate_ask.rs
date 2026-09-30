//! F5.4 S3's live half: the translation ask itself.
//!
//! [`crate::produce_translate`] owns every rule that settles with no server — the batch ranges, the
//! message wording, what a missing number costs, whether an answer earned a cache slot. What it never
//! did was *telephone*: the page handed the render a scripted reply or a hardcoded refusal, so a ticked
//! language could never reach a model however the settings were filled in. This module is that leg. It
//! builds the two messages §3.10 spells, dials the LLM over HTTP through [`crate::server_leg`] (so each
//! batch writes its timed `requests.tsv` row), reads the answer back into the numbered map
//! `produce_translate::Ask` expects, and returns `Err` naming the model when the wire fails — which the
//! walk already turns into "ship the original with a warning".
//!
//! The seam a test drives this through is the settings address: point `LLM_SERVER` at a local fake and
//! the same endpoint resolution a typed address goes through reaches it. Nothing here takes a canned
//! reply.
//!
//! Modelled on [`crate::captions_ask`] deliberately: two jobs, one shape for "build the messages, honour
//! the cache, dial, read the reply". Where they differ is spelled out below rather than papered over —
//! a translate call asks with thinking OFF (`Mode::Execute`, §02-services' table: an execute call is
//! temperature 0.6, 8192 tokens, `enable_thinking` false) and offers only `translate_line` and `finish`;
//! a captions answer is one JSON document, a translate answer is numbered lines or tool calls.

use std::collections::BTreeMap;

use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::produce_subtitles as subs;
use crate::produce_translate::Ask;
use crate::requests::{Request, Service};
use crate::server_leg;
use crate::services::{self, Kind, Server};
use crate::settings::{self, Conf, Paths};
use crate::tools::Tool;

/// The step name this job's cache entries and timed rows carry. `cache/llm/translate/<key>` — keyed on
/// the exact request, so an unchanged batch costs nothing and an edited cue misses (§00-principles).
pub const STEP: &str = "translate";

/// The settings folder, read from this process's environment for EVERY batch so an address changed
/// while a render is going is honoured by its next request (§02-services#1).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6). With
/// no settings folder at all, the shipped `spec/prompts/translate.md` is what goes out.
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The SYSTEM for one translate call: the house rules cut to this job's section plus the shipped
/// translate prompt. No User Context: §3.10's request is a batch of numbered lines and nothing else,
/// and the cue text is already the whole of what is to be translated.
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

/// The body of one batch's request: the system message, §3.10's numbered-lines message verbatim,
/// thinking off, and ONLY the two tools §3.10 lists. `llm_request::body` is the one place the
/// thinking switch and the token cap are written, so this cannot drift from §02-services' table.
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
    let body = llm_request::body(model, &messages, Mode::Execute, Some(&tools), false)?;
    Ok(body.to_string())
}

/// The deciding parts for one batch's cache slot: model, both messages, and the numbers asked for.
/// The numbers ride in the key because the re-ask sends the same wording over a different set, and
/// replaying the first answer to it would answer a question that was not asked.
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

/// One batch's answer, read two ways — both are real server behaviour and neither is invented here:
///
/// 1. `translate_line` TOOL CALLS, §3.10's contract: `{"n": <number>, "text": "..."}` per call.
/// 2. The numbered-line answer `spec/prompts/translate.md` actually asks for — `n<TAB>text`, one line
///    each — which is what a server that ignores the tool offer replies with.
///
/// Per §3.10 a number outside this batch, or an empty text, is REFUSED: the line simply stays missing,
/// which is what triggers `produce_translate`'s one re-ask rather than poisoning the track with it.
pub fn parse_answer(content_type: &str, reply: &str, numbers: &[usize]) -> BTreeMap<usize, String> {
    let model_reply = match llm_request::reply_from(content_type, reply) {
        Ok(reply) => reply,
        // Not a chat envelope at all: treat the whole body as the numbered answer, which is what a
        // plain-text server hands back. An event stream that framed nothing falls here too.
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
            .or_else(|| args.get("n").and_then(|n| n.as_str()).and_then(|s| s.trim().parse().ok()))
            .map(|n| n as usize);
        let text = args.get("text").and_then(|t| t.as_str()).unwrap_or_default();
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

/// `n<TAB>text`, one per line. A line whose number was not asked for, whose text is empty, or that
/// carries no number at all is dropped rather than guessed at.
pub fn parse_numbered(answer: &str, numbers: &[usize]) -> BTreeMap<usize, String> {
    let mut got: BTreeMap<usize, String> = BTreeMap::new();
    for line in answer.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        // Tab first (what the prompt asks for), then the first space: servers have been seen to answer
        // `1. text` or `1 text`, and renumbering a line to make it fit is what the prototype's repair
        // calls got wrong.
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

/// The model leg for the render: settings are read per batch (so an address edit lands on the next
/// call), the cache is honoured per batch, and every dial writes its timed row. `Err` names the model
/// and the reason — the walk ships that language's track as the original with the merge's warning
/// rather than failing the render over one server (§F5.4 S3, §00-principles).
pub fn ask(
    tree: &Tree,
    cancelled: &std::rc::Rc<dyn Fn() -> bool>,
) -> Box<Ask> {
    let tree = tree.clone();
    let cancelled = std::rc::Rc::clone(cancelled);
    Box::new(move |message: &str, numbers: &[usize]| {
        let conf = match current_paths().as_ref().map(settings::read) {
            Some(Ok(conf)) => conf,
            // An unreadable settings file is reported the way a down server is: the track keeps the
            // original and says why, rather than dialling a guessed address.
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
        // A cached answer costs no wire and writes no row: the request that earned it is already in
        // `requests.tsv` (§09 §10). Only a COMPLETE answer is ever stored, so a cache hit here is a
        // whole answer by construction.
        return Ok(parse_answer("application/json", &cached, numbers));
    }
    // No model configured is §02-services' own refusal, named, not a silent empty track.
    let body = body(&model, &system, message)?;
    let endpoint = services::llm_endpoint(conf);
    let (_row, reply) = server_leg::call(
        tree,
        &base_row(&model),
        Server::Llm,
        &endpoint,
        Kind::Chat,
        None,
        Some(&body),
        cancelled,
    );
    match reply {
        Ok(sent) => {
            let text = sent.body.clone();
            let got = parse_answer(&sent.content_type, &text, numbers);
            // S3: `cached only when complete`. A partial answer stored is a partial answer forever, so
            // the re-ask's chance is thrown away on every later run.
            if subs::cached(got.len() == numbers.len() && !numbers.is_empty()) {
                llm_cache::store(tree, STEP, &parts, &text);
            }
            Ok(got)
        }
        Err(reason) => Err(format!("the translation model {model:?} did not answer: {reason}")),
    }
}
