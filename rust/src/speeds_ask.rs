//! F3.10 S1's live half: the speed ask itself.
//!
//! [`crate::cut_speed_pass`] owns every rule that settles with no server — the brief's layout, membership,
//! the caption refusal, rate ≈ 1, the ramps, the gap merge and the two log lines. What it never did was
//! *telephone*: `run_speeds_pass` in the shell handed its own rules an empty list, so pressing ⏩ Speeds
//! could only ever report "no usable answer" while a server sat there willing to name rates. This module is
//! that leg. It builds the two messages §F3.10 spells, dials the LLM over HTTP through
//! [`crate::llm_retry_leg`] (so each ask writes its timed `requests.tsv` row and rides §3's patience), reads
//! the SPEEDS reply back into [`cut_speed_pass::Call`]s, and hands those to
//! `crate::ui::cut_speeds_pass`, which lets §3.7's `set_clip_speed` tool — not a copy of it — decide
//! what may land.
//!
//! The seam a test drives this through is the settings address: point `LLM_SERVER` at a local fake and the
//! very same endpoint resolution a typed address goes through reaches it. Nothing here takes a canned reply,
//! and a server that is down answers `Err` naming it (§00-principles: failure is specific and local).
//!
//! Why it is a module of its own rather than more lines in `cut_speed_pass` or in `ui/window.rs`: the pass is
//! judgement and answers to no socket, and the window is a page. `captions_ask` and `join_ask` set the shape
//! — two messages, the cache honoured, one dial, the reply read — and this copies it with the three things
//! that differ spelled out below: the offer is `set_clip_speed` + `finish` only, thinking is OFF (§4 lists
//! speed among the prompts that do not think), and the answer is a `{"speeds":[…]}` document.

use crate::cut_speed_pass::{self, Call, GAP_SECONDS};
use std::path::PathBuf;
use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::requests::{Request, Service};
use crate::roles::Job;
use crate::services;
use crate::settings::{self, Conf, Paths};
use crate::tools::Tool;

/// The step name this job's cache entries and timed rows carry. §3.7 is one section for the three per-clip
/// passes, but the cache must tell a speed answer from a caption answer apart, so each gets its own step
/// string rather than sharing `"effects"`.
pub const STEP: &str = "speed";

/// The answer shape §`spec/prompts/system.md`'s `speed` row spells, with the two rules that row states.
/// Only a fallback: the wording that actually goes out is `spec/prompts/speed.md` and the machine's own edit
/// of it, read through `wording` below into [`speeds_system`].
pub const SPEEDS_SYSTEM: &str = concat!(
    "Answer in JSON only: {\"speeds\":[{\"clip\":<n>,\"rate\":<x>}]} \u{2014} clip is the clip number from \
     the list, rate > 1. A clip that already has a caption cannot be sped up; leave it out.\n",
    "Strict JSON and nothing else: no prose, no code fence, no comment."
);

/// The SYSTEM for a speed call: the house rules cut to this job's section, the speed wording, and the
/// precedence rule only when there is a User Context to outrank anything (§09 §8 puts it last). Same
/// assembly [`crate::captions_ask`] builds, because §8 assembles every job's system message the one way.
pub fn speeds_system(context: &str) -> String {
    let paths = current_paths();
    crate::prompt_assembly::system_message(
        &wording(paths.as_ref(), "system"),
        "speed",
        &wording(paths.as_ref(), "speed"),
        context,
        false,
        false,
    )
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6). With no
/// settings folder at all, the shipped wording is what goes out, and with no shipped wording either the
/// prompt's own one-line shape ([`SPEEDS_SYSTEM`]).
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The settings folder, read from this process's environment each time an ask is built so a change made
/// while a run is going is honoured by its next request (§02-services#1: settings are re-read per request).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// The gap two same-rate stretches may come before they are one run. Read from §10's catalogue where this
/// section's rows live (`params::cut`, §05 §6 — `params::find` searches Prepare's list only), so the ask
/// quotes the number and [`crate::cut_speed_pass`]'s merge applies it: one value, said and enforced in
/// step. With no row to read the constant itself is the answer, never a second guess at it.
fn gap_seconds() -> f64 {
    crate::params::cut()
        .into_iter()
        .find(|row| row.id == "P.policy.speedGapSeconds")
        .and_then(|row| row.spelled.trim().parse::<f64>().ok())
        .filter(|gap| gap.is_finite() && *gap > 0.0)
        .unwrap_or(GAP_SECONDS)
}

/// F3.10 S1: the user message — every clip with its own seconds, which of them already carry captions, the
/// footage the cut comes to, and the rules a rate has to respect.
///
/// `clips` is `(number, length)` in play order, numbered from 1, exactly the list
/// [`crate::ui::cut_speeds_pass`] builds from the kept segments. The seconds are always the clip's OWN:
/// a model shown a session second would have to compute one, which is the standing rule's first
/// prohibition, and a captioned clip is marked in its own line so the refusal in S2 is a rule the model was
/// told before it broke it rather than a surprise it gets afterwards.
pub fn user_message(clips: &[(u32, f64)], captioned: &[(u32, usize)], total_seconds: f64) -> String {
    user_message_over(clips, captioned, total_seconds, &[], &[])
}

/// §9 (`With more than one camera row or lane the cut brief names the rows and lanes`): the
/// speed ask with the session's camera rows and heard lanes named above the clip list.
pub fn user_message_over(
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    total_seconds: f64,
    rows: &[String],
    lanes: &[String],
) -> String {
    let gap = gap_seconds();
    let mut lines = vec![format!(
        "{} {:.0} seconds over {} clip(s).",
        cut_speed_pass::FOOTAGE_HEADER,
        total_seconds,
        clips.len()
    )];
    lines.extend(crate::effect_overlap::brief_lines(rows, lanes));
    lines.push(String::new());
    lines.push(crate::cut_captions::CLIPS_HEADER.to_string());
    for (number, length) in clips {
        let mut line = crate::cut_captions::clip_line(*number, *length);
        if let Some((_, count)) = captioned.iter().find(|(clip, _)| clip == number) {
            line.push_str(&format!(", {count} caption(s) -- runs at 1"));
        }
        lines.push(line);
    }
    lines.push(String::new());
    lines.push(format!(
        "Total video {total_seconds:.0} s of footage. Speed only what would bore, and answer with the clips \
         you would speed and nothing else \u{2014} a cut with nothing dull in it gets no rates at all."
    ));
    lines.push("- Only rates above 1. Below 1 costs seconds instead of saving them, and is not this job.".to_string());
    lines.push("- Never a timestamp and never a second count: a clip is one rate from end to end.".to_string());
    lines.push("- A clip marked \"captions\" plays at 1 whatever else is true: words on screen at 4 are gone before they are read.".to_string());
    lines.push(format!(
        "- Two stretches at the same rate nearer than {gap} s apart are one stretch, so give them one rate."
    ));
    lines.join("\n")
}

/// The chat body: thinking OFF (§4 does not list `speed` among the four prompts that think), and this job's
/// two tools offered — `set_clip_speed` for the work and `finish` to close the reply.
///
/// Offering them is not decoration: §1 asks that a tool conversation sends the offer when the job has tools,
/// and a model that is not told `set_clip_speed` exists answers in prose about dullness instead of asking for
/// a rate. `Job::ClipRules` is §3.7's job row (captions, speed and decorations share one state), so the names
/// come from the catalogue and the two this item names are then asserted present rather than assumed.
///
/// Only the name travels, as every chat ask sends it: §1 keeps each tool's parameter schema in §3, and a copy
/// written here would be a second answer to §3.7's question about `clip` and `rate`.
pub fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    let messages = two_messages(system, user);
    let tools: Vec<Tool> = this_pass_tools();
    Ok(llm_request::body(model, &messages, Mode::Execute, Some(&tools), true)?.to_string())
}

/// The two messages every speed ask carries: the SYSTEM rules and the USER clips/rows.
fn two_messages(system: &str, user: &str) -> Vec<Message> {
    vec![
        Message { role: "system".to_string(), parts: vec![Part::Text(system.to_string())] },
        Message { role: "user".to_string(), parts: vec![Part::Text(user.to_string())] },
    ]
}

/// This pass's own two tools, from §3.7's job row: a call of another kind (a caption, a decoration) is not
/// offered because this ask cannot place it (§2 S4). One list, used for the body AND for the loop, so the wire
/// and the loop can never disagree about what was on offer.
fn this_pass_tools() -> Vec<Tool> {
    crate::tools::offered(Job::ClipRules)
        .into_iter()
        .filter(|tool| matches!(tool, Tool::SetClipSpeed | Tool::Finish))
        .collect()
}

// NOTE: `prompt_cache_key` is not sent anywhere: §1's request contract (the table `tests/f3_9_s3_a_…` reads back
// on the wire) names model, the two messages, `tools` when the job has them, and the sampling five — no such
// field, and `llm_request::body` is the one place a chat request is built (§09 §5), so inventing a key here
// would be a field no server in §1 documents and no other ask sends.

/// Read a SPEEDS reply into the calls this pass places. Two spellings are accepted, because a model offered
/// tools answers with tools and one told to answer JSON answers with JSON, and both mean the same rate:
///
/// - a `set_clip_speed(clip, rate)` tool call, in the order it came;
/// - else the content's `{"speeds":[{"clip":<n>,"rate":<x>}]}` document (§`system.md`'s `speed` row).
///
/// An entry with no clip number, no rate, or a rate that is not a number is SKIPPED rather than losing the
/// whole reply: unlike F3.9, §F3.10 makes only a clip number outside the batch cost the round, and that is
/// [`cut_speed_pass::place`]'s judgement to make on the calls that did arrive. `{"speeds":[]}` is an empty
/// list and a whole answer — a cut with nothing dull in it is not a failed run.
/// A `set_clip_speed` call's arguments read as a rate: the same `{clip, rate}` pair the prose document's entries
/// carry, so one rule decides both spellings.
pub fn call_of_args(args: &serde_json::Value) -> Option<Call> {
    call_of(args)
}

pub fn parse_speeds(reply: &str) -> Vec<Call> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(reply.trim()) else {
        return Vec::new();
    };
    // A whole chat envelope is what comes off the wire; a bare SPEEDS document (or the same document under a
    // `content` key) is what sits in the cache, because §6 keeps the answer text and not the framing. Both
    // shapes parse here so the wire reader and the cache reader are one rule that cannot disagree.
    let answer = match value.get("choices") {
        Some(_) => llm_request::parse_reply(&value),
        None => llm_request::Reply {
            content: value
                .get("content")
                .and_then(|content| content.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| reply.trim().to_string()),
            ..Default::default()
        },
    };
    let calls: Vec<Call> = answer
        .tool_calls
        .iter()
        .filter(|call| call.name == Tool::SetClipSpeed.name())
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            call_of(&args)
        })
        .collect();
    if !calls.is_empty() {
        return calls;
    }
    // No tool call: read the content as the SPEEDS document. Anything unreadable is no calls, which the
    // pass reads as "the model proposed nothing" — the same ending a `{"speeds":[]}` answer has.
    let Ok(content) = serde_json::from_str::<serde_json::Value>(answer.content.trim()) else {
        return Vec::new();
    };
    content
        .get("speeds")
        .and_then(|speeds| speeds.as_array())
        .map(|speeds| speeds.iter().filter_map(call_of).collect())
        .unwrap_or_default()
}

/// One `{clip, rate}` pair, from either spelling. A missing or non-finite number is no call.
fn call_of(value: &serde_json::Value) -> Option<Call> {
    let clip = value
        .get("clip")
        .and_then(|clip| clip.as_f64().or_else(|| clip.as_str().and_then(|t| t.trim().parse::<f64>().ok())))
        .filter(|n| n.is_finite() && *n >= 1.0)? as u32;
    let rate = value
        .get("rate")
        .and_then(|rate| rate.as_f64().or_else(|| rate.as_str().and_then(|t| t.trim().parse::<f64>().ok())))
        .filter(|rate| rate.is_finite())?;
    Some(Call { clip, rate })
}

/// What one speed ask came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Every rate the model asked for, in the order it asked. Membership, the caption refusal and rate ≈ 1
    /// are NOT applied here — that is [`cut_speed_pass::place`]'s job, so one route decides and this one
    /// only reports.
    pub calls: Vec<Call>,
    /// The ask failed at the wire, so the caller should say why rather than treat the empty list as "the
    /// model wanted nothing sped up". An empty SPEEDS document is NOT a failure.
    pub failed: bool,
    /// The reason, naming the model and the fault. Empty when nothing failed.
    pub reason: String,
}

/// One live speed ask: build the two messages, honour the cache, dial, read the reply. Settings are read
/// HERE, per call, so an address changed while a run is going is honoured by the next ask (§02-services#1).
///
/// `captioned` names which clips already carry captions — the ask must say so (S1), and §3.7's tool refuses
/// a fast rate over them (S2) — while `clips` stays `(number, length)` pairs because a speed is asked for
/// over a whole clip and has no offsets to line up.
pub fn ask(
    tree: &Tree,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_named(tree, context, clips, captioned, &[], &[], attempt, cancel)
}

/// [`ask`] with the session's camera rows and heard lanes named in the brief (§9: the passes
/// know nothing of cameras and lanes until there is more than one of either to tell apart).
#[allow(clippy::too_many_arguments)] // the ask carries the brief's rows and lanes beside its clips
pub fn ask_named(
    tree: &Tree,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    rows: &[String],
    lanes: &[String],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    let conf = match current_paths().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        // An unreadable settings file is reported the way a down server is: the pass keeps the stumble and
        // says why, rather than dialling a guessed address.
        Some(Err(reason)) => return failed(format!("settings could not be read: {reason}")),
        None => Conf::default(),
    };
    // No LLM address at all: name it, because "no usable answer" would blame the model for a server nobody
    // typed in. §02-services#1: an empty box means the loopback default, which is what `llm_endpoint` answers.
    if services::server_url(&conf.server).is_none() {
        return failed("no LLM address is set \u{2014} nothing was asked".to_string());
    }
    ask_at_over(tree, &conf, context, clips, captioned, rows, lanes, attempt, cancel)
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_at_over(tree, conf, context, clips, captioned, &[], &[], attempt, cancel)
}

/// [`ask_at`] with the session's camera rows and heard lanes named in the brief (§9).
#[allow(clippy::too_many_arguments)] // as [`ask_named`]: the brief's rows and lanes ride along
pub fn ask_at_over(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    rows: &[String],
    lanes: &[String],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    if clips.is_empty() {
        // Nothing to ask about is not a failure and not worth a request.
        return Asked { calls: Vec::new(), failed: false, reason: String::new() };
    }
    let model = conf.model.clone();
    let total: f64 = clips.iter().map(|(_, length)| length).sum();
    let system = speeds_system(context);
    let user = user_message_over(clips, captioned, total, rows, lanes);
    // A retry asks with the same two messages, so it needs its own cache slot: without one the second round
    // would be served the reply that was just refused (§6). The fault itself travels in the conversation.
    let parts = cache_parts(&system, &user, &model, attempt);
    if let Some(cached) = llm_cache::read(tree, STEP, &parts) {
        // A reply served from the cache costs no wire and writes no row: the request that earned it is
        // already in `requests.tsv`, and a second row for the same answer would double-count a call that
        // did not happen (§09 §10).
        return Asked { calls: parse_speeds(&cached), failed: false, reason: String::new() };
    }
    let base = base_row(&model);
    let endpoint = services::llm_endpoint(conf);
    let offered = this_pass_tools();
    // How to build the body for one offer; `None` means no `tools` key, which is what §2 S5 resends to a server
    // that refused the field. The cache slot and the timed row belong to whichever body goes out first.
    let build = |offer: Option<&[Tool]>| {
        Ok(llm_request::body(
            &model,
            &two_messages(&system, &user),
            Mode::Execute,
            offer,
            true,
        )?
        .to_string())
    };
    // One live ask is ONE request on the wire. This dial IS §2's first round and honours S5 itself: a refusal of
    // the `tools` FIELD gets the identical body again without it, said once, remembered so no later ask re-offers.
    let sent = match crate::llm_tool_refusal_leg::dial_honouring_refusal(
        STEP,
        &endpoint.url,
        Some(&offered),
        &build,
        &|body: &str| {
            crate::llm_retry_leg::chat(STEP, tree, &base, &endpoint, body, cancel, &crate::ui::window::log_line)
        },
        &crate::ui::window::log_line,
    ) {
        Ok(sent) => sent,
        // The ask itself failed. §F3.10's two rounds count refusals of ANSWERS, so a wire failure spends none of
        // them: a server that is not there answers the same way however often it is called.
        Err(reason) => return failed(reason),
    };
    let reply = match llm_request::reply_from(&sent.content_type, &sent.body) {
        Ok(reply) => reply,
        Err(_) => {
            // Neither JSON nor a stream: nothing to place, read as "the model proposed nothing" — the same
            // ending a `{"speeds":[]}` answer has (§F3.10 does not fail a cut over an unreadable rate list).
            return Asked { calls: Vec::new(), failed: false, reason: String::new() };
        }
    };
    if reply.tool_calls.is_empty() {
        // Round 1 answered in prose, which is the whole ask: one request, one timed row.
        let text = reply.answer().to_string();
        if !text.trim().is_empty() {
            // Only an answer with words in it is cached: an empty one would make every later ask replay the
            // nothing, and a step that could still be answered later would never be asked again.
            llm_cache::store(tree, STEP, &parts, &stored_form(&text));
        }
        return Asked { calls: parse_speeds(&text), failed: false, reason: String::new() };
    }
    // The refusal a captioned clip earns comes back as the tool's own sentence, so it has to be REACHED. The batch
    // the loop answers its own calls from is filled with the captions here, so every rate the model asked for is
    // tried against them and each refusal is said in this step's log (§F3.10 S2). Every rate still travels in
    // `Asked`, because `cut_speed_pass::place` keeps making the placement judgement on the calls that arrived.
    let mut batch = crate::clip_rules_tools_leg::batch_of(clips);
    for (clip, count) in captioned {
        // One placeholder line per caption the caller reported: `set_clip_speed` refuses any rate above 1 over a
        // clip that carries one, whatever the words are, and this pass is not shown the texts.
        for index in 0..*count {
            batch.add_caption(*clip, 0.0, 0.5, &format!("caption {index}"));
        }
    }
    // The calls above are RUN against the batch (the tool's own refusal sentence for a rate over a captioned clip
    // is logged, §F3.10 S2) and then they END the ask: the model has its result, so no second request goes out.
    let calls_out: Vec<Call> = reply
        .tool_calls
        .iter()
        .filter(|call| call.name == Tool::SetClipSpeed.name())
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            call_of_args(&args)
        })
        .collect();
    // §2's loop from here, over the batch that already knows which clips carry captions. The calls above are
    // RUN first — results produced and said — before another request goes out: a rate over a captioned clip is
    // answered with the tool's own refusal sentence (§F3.10 S2), and one such call is an ending, not a round.
    for line in crate::clip_rules_tools_leg::answer_calls(&reply.tool_calls, &mut batch) {
        crate::ui::window::log_line(&line);
    }
    // A call this pass can already answer ENDS the ask: the model has its result and there is nothing to ask it
    // for, so no second request goes out (§F3.10 counts batches by rows in `requests.tsv`).
    Asked { calls: calls_out, failed: false, reason: String::new() }
}
/// The shape an answer is kept in: the call list for a tool answer, the plain document otherwise. See
/// [`parse_speeds`], which reads both back.
fn stored_form(answer: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(answer.trim()) else {
        return answer.trim().to_string();
    };
    let parsed = llm_request::parse_reply(&value);
    if parsed.tool_calls.is_empty() && !parsed.content.trim().is_empty() {
        return parsed.content.trim().to_string();
    }
    answer.trim().to_string()
}

/// One ask's two messages, so the file its answer is cached in (`cache_file_for_test`) is named from the very
/// texts the request is made of.
fn messages(context: &str, clips: &[(u32, f64)], captioned: &[(u32, usize)]) -> (String, String) {
    let total: f64 = clips.iter().map(|(_, length)| length).sum();
    (speeds_system(context), user_message(clips, captioned, total))
}

/// The file one ask's answer is cached in — `Tree::cache_llm`'s layout and `llm_cache`'s key, reached through
/// `cache_parts`, the ONE key recipe the pass itself stores by, so the two cannot drift apart. Named
/// `_for_test` because only a proof names the file: the pass never asks for a path, it just stores and reads.
/// A second hash recipe inside a test would be a rule able to disagree with the code.
pub fn cache_file_for_test(tree: &Tree, context: &str, clips: &[(u32, f64)], captioned: &[(u32, usize)], model: &str, attempt: u32) -> PathBuf {
    let (system, user) = messages(context, clips, captioned);
    // A missing key is no file: `llm_cache::store` refuses to write one either (§6).
    let key = llm_cache::key(&cache_parts(&system, &user, model, attempt)).unwrap_or_default();
    tree.cache_llm(STEP, &key)
}

/// The deciding parts for one speed ask. The retry's slot rides in `state` — the texts, model and thinking
/// flag are otherwise identical, and the slot is what keeps the two asks apart (§6).
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
        thinking: false,
    };
    llm_cache::parts(&request)
}

/// A distinct cache slot per attempt of the same cut. The first ask uses the plain hash of the system text;
/// each retry appends its number, so a retry keys elsewhere than the refusal it is retrying rather than
/// reading that reply back out of the cache.
fn ask_slot(system: &str, attempt: u32) -> String {
    if attempt <= 1 {
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
