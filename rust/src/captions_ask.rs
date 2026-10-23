//! F3.9 S1–S6's live half: the captions ask itself.
//!
//! [`crate::cut_captions`] owns every rule that settles with no server — the batch ranges, the
//! message layout, membership, the floor, the fades and the skip sentence. What it never did was
//! *telephone*: `ask_captions` in the shell returned an empty list, so pressing ✐ Captions placed
//! nothing and the pass reported its own emptiness as if that were the answer. This module is that
//! leg. It builds the two messages §F3.9 spells, dials the LLM over HTTP through
//! [`crate::llm_retry_leg`] (so each ask writes its timed `requests.tsv` row and rides §3's patience), reads the CAPTIONS
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
use crate::exchanges::{self, Message, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::requests::{Request, Service};
use crate::roles::Job;
use crate::services;
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
        // §09 §10: a reply served from the cache writes its own line, marked `cache` with no time on
        // the wire, so the file shows what a re-run saved. Nothing goes out here; the row only says so.
        crate::request_timing::record_cached(tree, base_row(&model));
        return match read_back(&cached) {
            Ok(calls) => Asked {
                calls,
                failed: false,
                reason: String::new(),
            },
            Err(reason) => failed(reason),
        };
    }
    let base = base_row(&model);
    let endpoint = services::llm_endpoint(conf);
    // Only what this pass answers: `add_caption` for the work and `finish` as its own signal (§2 S3's end). The
    // SAME list goes into the request body and into the loop, so the model is never handed a tool whose result
    // this ask cannot place (speeds and decorations are their own passes).
    //
    // §3 does say every job names its read tools as well as its write tools, and `get_frames` is §3.7's read for
    // this job. It stays out ON PURPOSE: this pass makes ONE request (§F3.9 counts a batch by the rows it writes
    // in `requests.tsv`), so a model that called a read would spend a round this step has no budget for and the
    // batch would be charged twice. The asks that CAN place a read are the speed and decoration passes, and those
    // run the loop (`clip_rules_tools_leg::run_over_batch`) over the full `tools::offered(Job::ClipRules)` list,
    // `get_frames` included.
    let offered: Vec<Tool> = crate::tools::offered(Job::ClipRules)
        .into_iter()
        .filter(|tool| matches!(tool, Tool::AddCaption | Tool::Finish))
        .collect();
    // How to build the body for one offer. `None` means "no `tools` key at all", which is what §2 S5 resends to a
    // server that refused the field; the cache slot and the timed row belong to whichever body goes out first.
    let build = |offer: Option<&[Tool]>| {
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
        // §4 lists captions as thinking-OFF, so the mode comes from the prompt table rather than a named
        // `Mode` here: this call used to send `Mode::Thinking` for a job §4 says does not think.
        Ok(llm_request::body(
            STEP,
            &model,
            &messages,
            crate::prompts::mode_of(STEP),
            offer,
            true,
        )?
        .to_string())
    };
    // One live ask is ONE request on the wire (§F3.9 counts batches by rows in `requests.tsv`). This dial IS §2's
    // first round, and it honours S5 itself: a refusal of the `tools` FIELD gets the identical body again without
    // the field, said once, and [`crate::llm_tools_offer`] remembers it so no later ask re-offers.
    let sent = match crate::llm_tool_refusal_leg::dial_honouring_refusal(
        STEP,
        &endpoint.url,
        Some(&offered),
        &build,
        &|body: &str| {
            crate::llm_retry_leg::chat(
                STEP,
                tree,
                &base,
                &endpoint,
                body,
                cancelled,
                &crate::ui::window::log_line,
            )
        },
        &crate::ui::window::log_line,
    ) {
        Ok(sent) => sent,
        // The ask itself failed. `cut_captions::retries` counts refusals of ANSWERS, so a wire failure spends
        // none of them: a server that is not there answers the same way however often it is called.
        Err(reason) => return failed(reason),
    };
    let reply = match llm_request::reply_from(&sent.content_type, &sent.body) {
        Ok(reply) => reply,
        Err(reason) => return failed(format!("{STEP}: {reason}")),
    };
    if reply.tool_calls.is_empty() {
        // A server that no longer gets a `tools` field (§2 S5 remembered its refusal, so the body went out
        // without one) CANNOT be blamed for answering in prose: the document is the only shape left to it, and
        // `build(None)` is what §2 S5's own resend looks like. This is the sole exception, and it is silent --
        // the refusal was already said once, on the round that earned it.
        if crate::llm_tools_offer::tools_for(&endpoint.url, &offered).is_none() {
            return match parse_captions(reply.answer()) {
                Ok(calls) => Asked {
                    calls,
                    failed: false,
                    reason: String::new(),
                },
                Err(problem) => failed(problem),
            };
        }
        // §3's answered question about these three passes: the tools ARE on the wire, and a reply that carries a
        // JSON document instead of calling `add_caption` is UNREADABLE, not an answer. This branch used to read
        // such a document (and repair an unparseable one); both are gone, because a model that was offered
        // `add_caption` and answered with prose has not answered. Asking again, with the correction in the
        // conversation, is what §3's repair is for: ONE extra request, never two, and a second prose answer is
        // this pass's failure, said by name rather than quietly placed.
        let went_out = std::time::Instant::now();
        let dial = |body: &str| {
            crate::llm_retry_leg::chat(
                STEP,
                tree,
                &base,
                &endpoint,
                body,
                cancelled,
                &crate::ui::window::log_line,
            )
            .map(|sent| (sent.content_type, sent.body))
        };
        let second = match crate::clip_rules_tools_leg::ask_again_for_calls(
            STEP,
            Tool::AddCaption.name(),
            repair_messages(&system, &user),
            &reply,
            &crate::ui::window::log_line,
            &|conversation| chat_body_conversation(&model, conversation, &offered),
            // §3's patience ladder runs INSIDE this dial, so a repair whose retry had to wait for the server
            // to come back leaves those wait rows here and nowhere else (§09 §10 writes one per REQUEST).
            &dial,
        ) {
            Ok(second) => second,
            Err(reason) => return failed(reason),
        };
        // That retry was a request too, and §09 §10 wants every request on the record: this ask cannot route it
        // through `llm_retry_leg::chat` itself (the repair leg owns the dial), so the row is written here.
        // One line for one request -- `llm_retry_leg::chat` files its own rows only for what it dials, so
        // nothing is counted twice.
        crate::clip_rules_tools_leg::record_repair_row(tree, base_row(&model), went_out);
        // The retry answered by calling: its `add_caption` calls ARE the answer.
        let calls: Vec<Call> = second
            .tool_calls
            .iter()
            .filter_map(|call| {
                let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
                call_of_args(&args)
            })
            .collect();
        // The retry answered by calling: its `add_caption` calls ARE the answer. What gets cached is the
        // ANSWERING round's text (§6: an unchanged batch must not open a socket again), which for a tool answer
        // is the call list re-spelled as the envelope the round arrived in — the one shape `read_back` reads.
        if asks_for_anything(&second) {
            llm_cache::store(tree, STEP, &parts, &chat_envelope(&second).to_string());
        }
        return Asked {
            calls,
            failed: false,
            reason: String::new(),
        };
    }
    // §2's loop from here: the clip batch is the state the tools write into, so a model that asks `get_frames`
    // gets frames and a caption call lands in the same batch the prose document would have filled. The grown
    // conversation rides on every later round (§2 S2), each with its own timed row -- rounds only happen because
    // the model asked, so a plain answer still costs the one request above.
    let mut batch = crate::clip_rules_tools_leg::batch_of(clips);
    // The frames a model may aim at are only this far apart: §3.7's `get_frames` walks the batch's
    // grid, and a default 1 s spread would name pictures this session never extracted.
    batch.set_frame_interval(crate::project::frame_interval(tree.dir()));
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
    let mut calls_out: Vec<Call> = reply
        .tool_calls
        .iter()
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            call_of_args(&args)
        })
        .collect();
    // The reply above IS the loop's first round: it is already on the wire and in `requests.tsv`, so feeding it
    // back is what keeps a call answer at ONE request (§2 S5's plain-call shape) instead of dialling the identical
    // body again for an answer already in hand.
    let first = crate::tool_loop::Turn {
        content: reply.content.clone(),
        calls: reply.tool_calls.clone(),
        finish: reply
            .tool_calls
            .iter()
            .any(|call| call.name == Tool::Finish.name()),
    };
    let looped = crate::clip_rules_tools_leg::run_over_batch_from(
        STEP,
        &first,
        messages,
        &offered,
        &mut batch,
        &mut |conversation, tools| {
            let body = chat_body_conversation(&model, conversation, tools)?;
            let sent = crate::llm_retry_leg::chat(
                STEP,
                tree,
                &base,
                &endpoint,
                &body,
                cancelled,
                &crate::ui::window::log_line,
            )?;
            let round = llm_request::reply_from(&sent.content_type, &sent.body)
                .map_err(|reason| format!("{STEP}: {reason}"))?;
            // A round's own calls are part of the answer, not only of the conversation: F3.9 places every caption
            // the model asked for, wherever in the rounds it asked.
            calls_out.extend(round.tool_calls.iter().filter_map(|call| {
                let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
                call_of_args(&args)
            }));
            Ok(crate::tool_loop::Turn {
                content: round.content.clone(),
                calls: round.tool_calls.clone(),
                finish: round
                    .tool_calls
                    .iter()
                    .any(|call| call.name == Tool::Finish.name()),
            })
        },
    );
    for line in &looped.run.logs {
        crate::ui::window::log_line(line);
    }
    // §7's live half for a tool flow: ONE section per request this ask made, each showing what the model was shown,
    // what it asked for, and what each tool answered. `call_of` could not do this — it reads a reply's calls blind
    // to their answers — so the loop files its own rounds, already answered by the one door that ran them.
    // The FIRST round is skipped: that request was filed by `llm_retry_leg::chat` as it went out, and filing it a
    // second time would print the same exchange twice.
    if !looped.rounds.is_empty() {
        for round in looped.rounds.iter().skip(1) {
            if round.turn.calls.is_empty() {
                // A round that asked nothing is the answer, and the dialling leg already filed that request on the
                // page when it went out; filing it again would print the same exchange twice.
                continue;
            }
            let filed = page_round(STEP, &model, round);
            crate::exchange_leg::record(tree, STEP, &filed, &crate::ui::window::log_line);
        }
    }
    if looped.run.outcome == crate::tool_loop::Outcome::Exhausted {
        // §2 S4: the step gets no answer from this call. The wording is §2's own, not a paraphrase.
        crate::ui::window::log_line(&crate::tools::exhausted_log(STEP));
        return failed(crate::tools::exhausted_log(STEP));
    }
    // The answer is the call list. Prose beside it is the model talking, not asking: a round with calls is not an
    // answer (§2), and an empty list after the rounds simply means the model asked for nothing.
    // The caption a TOOL answer put in its `content` is not the answer either -- the calls are the only thing
    // `read_back` reads back, and caching that envelope is what makes a re-run cost no socket (§6).
    if asks_for_anything(&reply) {
        llm_cache::store(tree, STEP, &parts, &chat_envelope(&reply).to_string());
    }
    Asked {
        calls: calls_out,
        failed: false,
        reason: String::new(),
    }
}

/// One answered round as §7 shows it: the conversation that round was shown, its calls with their results, and
/// the prose the model said alongside them. Built by [`crate::exchange_leg::round_of`], so a tool flow's page is
/// drawn through the same door every other exchange is.
fn page_round(
    step: &str,
    model: &str,
    round: &crate::clip_rules_tools_leg::Round,
) -> exchanges::Call {
    crate::exchange_leg::round_of(step, model, &round.messages, &round.turn)
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

/// A `add_caption` call's arguments read as a caption: the same `{clip,start,end,text}` entry the prose document
/// carries, so one rule decides both spellings.
pub fn call_of_args(args: &serde_json::Value) -> Option<Call> {
    let text = args.get("text").and_then(|text| text.as_str())?.to_string();
    let number = |key: &str| {
        args.get(key).and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|t| t.trim().parse::<f64>().ok()))
        })
    };
    Some(Call {
        clip: number("clip")?.max(1.0) as u32,
        start: number("start")?,
        end: number("end")?,
        text,
    })
}

/// The two messages of one captions ask, rebuilt for §3's repair path: the SAME system and user the first
/// request carried, because a repair adds to the conversation rather than replacing it.
fn repair_messages(system: &str, user: &str) -> Vec<Message> {
    vec![
        Message {
            role: "system".to_string(),
            parts: vec![Part::Text(system.to_string())],
        },
        Message {
            role: "user".to_string(),
            parts: vec![Part::Text(user.to_string())],
        },
    ]
}

/// The chat body for one round of a tool conversation: the grown message list beside the offer the loop was
/// given (§2 S2's "append the assistant turn · one `tool` message per call"). The body carries `"stream": true`
/// (§1), which `llm_request::body` sets for every mode.
fn chat_body_conversation(
    model: &str,
    conversation: &[Message],
    tools: &[Tool],
) -> Result<String, String> {
    // The mode is §4's flag for this prompt (see [`crate::prompts::mode_of`]), not a hard-coded `Mode`.
    Ok(llm_request::body(
        STEP,
        model,
        conversation,
        crate::prompts::mode_of(STEP),
        Some(tools),
        true,
    )?
    .to_string())
}

/// The timed row shape this ask's rounds write: `started` and `run` belong to whoever owns the run, and a step
/// that stands alone leaves them blank (the same shape `llm_retry_leg::chat` fills in).
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

/// The cache slot parts: the exact request, so an unchanged batch costs nothing and an edited prompt misses (§09 §10).
fn cache_parts(system: &str, user: &str, model: &str, attempt: u32) -> Vec<serde_json::Value> {
    vec![
        serde_json::Value::String(system.to_string()),
        serde_json::Value::String(user.to_string()),
        serde_json::Value::String(model.to_string()),
        serde_json::Value::from(attempt),
    ]
}

/// An ask that did not answer: F3.9's pass keeps its stumble and says why, rather than failing the cut.
fn failed(reason: String) -> Asked {
    Asked {
        calls: Vec::new(),
        failed: true,
        reason,
    }
}

/// Read a cached captions answer back. Two shapes, because the answer's shape changed with the tools being put
/// on the wire: an ENVELOPE is what a tool answer stores (its `add_caption` calls are the answer), a bare
/// document is what an older slot holds (§6 keeps every step's answer, and a re-run must still cost nothing).
pub fn read_back(cached: &str) -> Result<Vec<Call>, String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(cached.trim()) else {
        return parse_captions(cached);
    };
    if value.get("choices").is_some() {
        let answer = llm_request::parse_reply(&value);
        let calls: Vec<Call> = answer
            .tool_calls
            .iter()
            .filter_map(|call| {
                let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
                call_of_args(&args)
            })
            .collect();
        if !calls.is_empty() {
            return Ok(calls);
        }
        // An envelope whose message holds a document is the shape a live reply arrives in; read the document.
        return parse_captions(answer.answer());
    }
    parse_captions(cached)
}

/// Whether an answer carries any caption call at all -- `finish` aside. The shared door is
/// [`crate::clip_rules_tools_leg::asks_for_anything`]; this one only has to ignore `finish`, because the
/// captions pass offers nothing else, and it reads clearer at the call site.
fn asks_for_anything(reply: &llm_request::Reply) -> bool {
    reply
        .tool_calls
        .iter()
        .any(|call| call.name != Tool::Finish.name())
}

/// The chat envelope a round answered with, rebuilt for the cache — the shared door, so all three §3.7 passes
/// keep what they read in one shape.
fn chat_envelope(reply: &llm_request::Reply) -> serde_json::Value {
    crate::clip_rules_tools_leg::answering_envelope(reply)
}

/// Read the CAPTIONS document the prompt specifies into the calls this pass places:
/// `{"clips":[{"i":<n>,"fx":[{"start":..,"end":..,"text":".."}]}]}` (`spec/prompts/system.md`'s
/// `captions` row). Array order is kept, because a caption's order in its clip is information the
/// cut reads later and re-sorting it here would lose which line came first.
///
/// `"clips":[]}` is an empty list and a whole answer, and so is a document naming no clip at all (§F3.9's
/// prompt calls an empty answer complete). Anything whose `fx` entry is missing a number or the text is an
/// error naming what was wrong: a reply we cannot read is not the same thing as a reply that asked for nothing.
///
/// NOT the live answer shape any more: §3's answer on this item says a reply that carries a JSON document
/// instead of calling `add_caption` is unreadable, so the wire path no longer calls this. Two readers still
/// need it — [`read_back`], because a slot written before that rule holds a document and must still cost
/// nothing to re-run, and a server that refused the `tools` field, which has no other shape to answer in.
pub fn parse_captions(answer: &str) -> Result<Vec<Call>, String> {
    let value: serde_json::Value = serde_json::from_str(answer.trim())
        .map_err(|_| "the captions answer is not JSON".to_string())?;
    let clips = value
        .get("clips")
        .and_then(|clips| clips.as_array())
        .ok_or_else(|| "the captions answer has no \"clips\" list".to_string())?;
    // `{"clips":[]}` is the prompt's own whole no-caption answer; a document that is NOT that shape is unreadable,
    // which is a different thing and says so (§F3.9).
    let mut calls: Vec<Call> = Vec::new();
    for entry in clips {
        let clip = entry
            .get("i")
            .and_then(|number| number.as_u64())
            .map(|number| number as u32)
            .ok_or_else(|| {
                "a clip entry in the captions answer has no clip number \"i\"".to_string()
            })?;
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
            calls.push(Call {
                clip,
                start,
                end,
                text: text.to_string(),
            });
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
