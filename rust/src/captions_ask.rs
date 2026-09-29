//! F3.9 S1–S6's live half: the captions ask itself.
//!
//! [`crate::cut_captions`] owns every rule that settles with no server — the batch ranges, the
//! message layout, membership, the floor, the fades and the skip sentence. What it never did was
//! *telephone*: `ask_captions` in the shell returned an empty list, so pressing ✐ Captions placed
//! nothing and the pass reported its own emptiness as if that were the answer. This module is that
//! leg. It builds the two messages §F3.9 spells, dials the LLM over HTTP through
//! [`crate::server_leg`] (so each ask writes its timed `requests.tsv` row), reads the CAPTIONS
//! reply back into [`cut_captions::Call`]s, and hands those to `cut_captions::place`, which lets
//! §3.7's `add_caption` tool — not a copy of it — decide what may land.
//!
//! The seam a test drives this through is the settings address: point `LLM_SERVER` at a local fake
//! and the very same endpoint resolution a typed address goes through reaches it. Nothing here takes
//! a canned reply, and a server that is down answers `Err` naming it (§00-principles: failure is
//! specific and local).
//!
//! Modelled on [`crate::join_ask`], deliberately: two jobs, one shape for "build the messages,
//! honour the cache, dial, read the reply", so a fix to that shape lands in both. Where they differ
//! is spelled out below rather than papered over — captions ask with tools offered, joins ask with
//! none; a caption answer is a JSON document, a join answer is words.

use crate::cut_captions::{self, Call};
use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::requests::{Request, Service};
use crate::server_leg;
use crate::services::{self, Kind, Server};
use crate::roles::Job;
use crate::settings::{self, Conf, Paths};
use crate::tools::Tool;

/// The step name this job's cache entries and timed rows carry. §3.7 is one section for the three
/// per-clip passes, but the cache must tell a caption answer from a decoration answer apart, so each
/// gets its own step string rather than sharing `"effects"`.
pub const STEP: &str = "captions";

/// What one batch's ask came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Every caption the model asked for, in the order it asked, as clip number + offsets inside the
    /// clip. Membership, the floor and the wordless check are NOT applied here — that is
    /// [`cut_captions::place`]'s job, so one route decides and this one only reports.
    pub calls: Vec<Call>,
    /// The ask failed at the wire or the answer was unreadable, so the caller should say why rather
    /// than treat the empty list as "the model wanted no captions". A `{clips:[]}` answer is NOT a
    /// failure: §F3.9's prompt calls it a whole answer.
    pub failed: bool,
    /// The reason, naming the model and the fault. Empty when nothing failed.
    pub reason: String,
}

/// The settings folder, read from this process's environment each time an ask is built so a change
/// made while a run is going is honoured by its next request (§02-services#1: settings are
/// re-read per request).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// One batch's live ask: build the two messages, honour the cache, dial, read the reply.
///
/// `attempt` selects the cache slot ([`ask_slot`]) so a retry cannot be served by the reply that
/// was just refused — the same reason F1.10's retries needed a slot. Settings are read HERE, per
/// call, so an address changed while a run is going is honoured by the next batch (§02-services#1).
#[allow(clippy::too_many_arguments)]
pub fn ask(
    tree: &Tree,
    user_context: &str,
    clips: &[(u32, f64)],
    words: &[(u32, f64, String)],
    attempt: u32,
    cancelled: &dyn Fn() -> bool,
) -> Asked {
    let conf = match current_paths().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => {
            // An unreadable settings file is reported the way a down server is: the pass keeps the
            // stumble and says why, rather than dialling a guessed address.
            return failed(format!("settings could not be read: {reason}"));
        }
        None => Conf::default(),
    };
    ask_at(tree, &conf, user_context, clips, words, attempt, cancelled)
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
#[allow(clippy::too_many_arguments)]
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    user_context: &str,
    clips: &[(u32, f64)],
    words: &[(u32, f64, String)],
    attempt: u32,
    cancelled: &dyn Fn() -> bool,
) -> Asked {
    let model = conf.model.clone();
    let system = captions_system(user_context);
    // Borrowed view for `cut_captions::message`, whose signature keeps the word texts borrowed.
    let shown: Vec<(u32, f64, &str)> = words
        .iter()
        .map(|(clip, offset, text)| (*clip, *offset, text.as_str()))
        .collect();
    let user = cut_captions::message(user_context, clips, &shown);
    let parts = cache_parts(&system, &user, &model, attempt);
    if let Some(cached) = llm_cache::read(tree, STEP, &parts) {
        // A reply served from the cache costs no wire and writes no row: the request that earned it
        // is already in `requests.tsv`, and a second row for the same answer would double-count a
        // call that did not happen (§09 §10).
        return match parse_captions(&cached) {
            Ok(calls) => Asked { calls, failed: false, reason: String::new() },
            Err(reason) => failed(reason),
        };
    }
    let body = match chat_body(&model, &system, &user) {
        Ok(body) => body,
        Err(reason) => return failed(reason),
    };
    let base = base_row(&model);
    let endpoint = services::llm_endpoint(conf);
    let (_row, reply) = server_leg::call(
        tree,
        &base,
        Server::Llm,
        &endpoint,
        Kind::Chat,
        None,
        Some(&body),
        cancelled,
    );
    match reply {
        Ok(sent) => {
            let text = answer_of(&sent.body);
            match parse_captions(&text) {
                Ok(calls) => {
                    // Only a readable answer is cached. A reply we could not read must go out again
                    // on the retry rather than being replayed from the slot it failed in.
                    llm_cache::store(tree, STEP, &parts, &text);
                    Asked { calls, failed: false, reason: String::new() }
                }
                Err(reason) => failed(reason),
            }
        }
        // The ask itself failed. `cut_captions::retries` counts refusals of ANSWERS, so a wire
        // failure spends none of them: a server that is not there answers the same way however
        // often it is called.
        Err(reason) => failed(reason),
    }
}

/// The batch's word lines as clip-relative offsets: each word whose session second falls inside kept
/// run `k` becomes `(k, second - run_start, word)`, with `k` counted from 1 because that is how the
/// message numbers its clips.
///
/// This is what keeps the model out of session seconds entirely (§00-principles: models propose, the
/// machine places). A word outside every kept run is dropped rather than sent unlabelled — it is not
/// in the video, so there is no clip to caption it under.
pub fn offsets_in_clips(
    runs: &[(f64, f64)],
    words: &[String],
    times: &[(f64, f64)],
) -> Vec<(u32, f64, String)> {
    let mut out: Vec<(u32, f64, String)> = Vec::new();
    for (index, (word, (start, _))) in words.iter().zip(times.iter()).enumerate() {
        if let Some((clip, run_start)) = clip_holding(runs, *start) {
            let _ = index;
            out.push((clip, start - run_start, word.clone()));
        }
    }
    out
}

/// Which kept run covers this session second, and where that run starts. Runs are expected sorted and
/// disjoint (they come from `cut_play::kept_runs` / `timeline::filmed_runs`, which merge touching
/// spans), so the first match is the only match.
fn clip_holding(runs: &[(f64, f64)], t: f64) -> Option<(u32, f64)> {
    runs.iter()
        .enumerate()
        .find(|(_, (start, end))| t >= *start && t < *end)
        .map(|(index, (start, _))| (index as u32 + 1, *start))
}

/// SYSTEM for a captions call: the house rules cut to this job's section, the shipped captions
/// wording, and the precedence rule only when there is a User Context to outrank anything
/// (§09 §8 puts it last). Same assembly the joins ask uses, with `captions_only` off — that flag
/// belongs to the narrate job, not this one.
fn captions_system(context: &str) -> String {
    let paths = current_paths();
    crate::prompt_assembly::system_message(
        &wording(paths.as_ref(), "system"),
        "captions",
        &wording(paths.as_ref(), "captions"),
        context,
        false,
        false,
    )
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6).
/// With no settings folder at all, the shipped wording is what goes out.
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The chat body: thinking on, and §3.7's two tools offered — `add_caption` for the work and
/// `finish` to close the reply. Offering them is not decoration: §1 asks that a tool conversation
/// sends the offer when the job has tools, and a model that is not told `add_caption` exists answers
/// in prose about captions instead of asking for them.
///
/// `Job::ClipRules` is §3.7's job row (captions, speed and decorations share one state), so the
/// offer is taken from the catalogue rather than written out here; the two this item names are then
/// asserted present rather than assumed.
fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    let offered = crate::tools::offered(Job::ClipRules);
    let tools: Vec<Tool> = offered
        .into_iter()
        .filter(|tool| matches!(tool, Tool::AddCaption | Tool::Finish))
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
    let body = llm_request::body(model, &messages, Mode::Thinking, Some(&tools), false)?;
    Ok(body.to_string())
}

/// The answer text out of a chat reply: `content`, never the reasoning.
fn answer_of(reply: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(reply) {
        Ok(parsed) => parsed,
        Err(_) => return String::new(),
    };
    llm_request::parse_reply(&parsed).answer().to_string()
}

/// Read the CAPTIONS document the prompt specifies into the calls this pass places:
/// `{"clips":[{"i":<n>,"fx":[{"start":..,"end":..,"text":".."}]}]}` (`spec/prompts/system.md`'s
/// `captions` row). Array order is kept, because a caption's order in its clip is information the
/// cut reads later and re-sorting it here would lose which line came first.
///
/// `{"clips":[]}` parses to an empty list and is NOT an error — the prompt says a context that asks
/// for nothing answers with an empty list and that is a whole answer. Anything that is not that one
/// field, or an `fx` entry missing a number or the text, is an error naming what was wrong: a reply
/// we cannot read is not the same thing as a reply that asked for nothing.
pub fn parse_captions(answer: &str) -> Result<Vec<Call>, String> {
    let value: serde_json::Value = serde_json::from_str(answer.trim())
        .map_err(|_| "the captions answer is not JSON".to_string())?;
    let clips = value
        .get("clips")
        .and_then(|clips| clips.as_array())
        .ok_or_else(|| "the captions answer has no \"clips\" list".to_string())?;
    let mut calls: Vec<Call> = Vec::new();
    for entry in clips {
        let clip = entry
            .get("i")
            .and_then(|number| number.as_u64())
            .map(|number| number as u32)
            .ok_or_else(|| "a clip entry in the captions answer has no clip number \"i\"".to_string())?;
        let fx = entry
            .get("fx")
            .and_then(|fx| fx.as_array())
            .ok_or_else(|| format!("clip {clip}'s answer has no \"fx\" list"))?;
        for caption in fx {
            let start = number(caption, "start")
                .ok_or_else(|| format!("clip {clip}: a caption has no \"start\""))?;
            let end = number(caption, "end")
                .ok_or_else(|| format!("clip {clip}: a caption has no \"end\""))?;
            let text = caption
                .get("text")
                .and_then(|text| text.as_str())
                .ok_or_else(|| format!("clip {clip}: a caption has no \"text\""))?;
            calls.push(Call { clip, start, end, text: text.to_string() });
        }
    }
    Ok(calls)
}

/// A JSON number read as f64. A string holding a number is accepted: servers have been seen to quote
/// them, and refusing a `"4.2"` would throw away a caption over spelling.
fn number(value: &serde_json::Value, key: &str) -> Option<f64> {
    let field = value.get(key)?;
    match field {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// The deciding parts for one captions ask. The retry's slot rides in `state` — the texts, model and
/// thinking flag are otherwise identical, and the slot is what keeps the two asks apart (§6).
fn cache_parts(system: &str, user: &str, model: &str, attempt: u32) -> Vec<serde_json::Value> {
    let state = ask_slot(system, attempt);
    let request = llm_cache::Request {
        system,
        user,
        state: &state,
        speech: "",
        context: "",
        images: Vec::new(),
        run_index: None,
        model,
        thinking: true,
    };
    llm_cache::parts(&request)
}

/// A distinct cache slot per attempt of the same batch. The first ask uses the plain hash of the
/// system text; each retry appends its number, so a retry keys elsewhere than the refusal it is
/// retrying rather than reading that reply back out of the cache.
fn ask_slot(system: &str, attempt: u32) -> String {
    if attempt == 0 {
        return system.to_string();
    }
    format!("{system}\u{21bb}{attempt}")
}

/// A `Request` carrying what only the caller knows, for the timed row `server_leg::call` writes.
fn base_row(model: &str) -> Request {
    Request {
        started: String::new(),
        run: String::new(),
        step: "cut".to_string(),
        job: STEP.to_string(),
        service: Service::Llm,
        model: model.to_string(),
        kind: "chat".to_string(),
        ..Default::default()
    }
}

/// One failure answer, with the reason kept beside it for the log.
fn failed(reason: String) -> Asked {
    Asked { calls: Vec::new(), failed: true, reason }
}
