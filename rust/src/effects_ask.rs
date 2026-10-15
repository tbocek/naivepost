//! F3.11 S1's live half: the decorations ask itself.
//!
//! [`crate::cut_effects_pass`] owns every rule that settles with no server — the brief's layout, membership,
//! the per-call skips, the app's own defaults for an undersized decoration and the two log lines. What it
//! never did was *telephone*: `run_effects_pass` in the shell handed its own rules an empty list, so pressing
//! ✨ Decorations could only ever report "no usable answer" while a server sat there willing to name a zoom,
//! a stop and some volume. This module is that leg. It builds the two messages §F3.11 spells, dials the LLM
//! over HTTP through [`crate::llm_retry_leg`] (so each ask writes its timed `requests.tsv` row and rides §3's
//! patience), reads the EFFECTS reply back into [`cut_effects_pass::Call`]s, and hands those to
//! `crate::ui::cut_effects_pass`, which lets §3.7's `add_effect` tool — not a copy of it — decide what may
//! land.
//!
//! The seam a test drives this through is the settings address: point `LLM_SERVER` at a local fake and the
//! very same endpoint resolution a typed address goes through reaches it. Nothing here takes a canned reply,
//! and a server that is down answers `failed` naming it (§00-principles: failure is specific and local).
//!
//! Why it is a module of its own rather than more lines in `cut_effects_pass` or in `ui/window.rs`: the pass is
//! judgement and answers to no socket, and the window is a page past its size budget. `captions_ask` and
//! `speeds_ask` set the shape — two messages, the cache honoured, one dial, the reply read — and this copies
//! it with the three things that differ spelled out below: the offer is `add_effect` + `finish` only, thinking
//! is OFF (§4 does not list `effects` among the prompts that think), and the answer is a `{"fx":[…]}`
//! document whose entries carry CLIP-RELATIVE offsets, because a model asked for a session second would have
//! to compute one.

use crate::cut_effects_pass::{self, Call};
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
use crate::tools::clips::Clips;

/// The step name this job's cache entries and timed rows carry. §3.7 is one section for the three per-clip
/// passes, but the cache must tell a decoration answer from a caption or a speed answer apart, so each gets
/// its own step string. This one keeps the prototype's own name for the prompt, `effects`.
pub const STEP: &str = "effects";

/// The answer shape §`spec/prompts/system.md`'s `effects` row spells, with the rules that row and §F3.11
/// state. Only a fallback: the wording that actually goes out is `spec/prompts/effects.md` and the machine's
/// own edit of it, read through `wording` below into [`effects_system`].
pub const EFFECTS_SYSTEM: &str = concat!(
    "Answer in JSON only: {\"fx\":[{\"clip\":<n>,\"kind\":\"zoom|stop|volume\",\"start\":<sec>,\"end\":<sec>,\
     \"gain\":<x>}]} \u{2014} clip is the clip number from the list, seconds are from that clip's start and \
     stay inside it, gain belongs to volume alone (1 as recorded, 0 silent).\n",
    "Few and deliberate: about three or four across five minutes unless the context says otherwise. A zoom \
     runs two to four seconds. One stop per video. Volume for level, ducking or muting. Never zoom past \
     captions.\n",
    "Strict JSON and nothing else: no prose, no code fence, no comment."
);

/// The SYSTEM for a decorations call: the house rules cut to this job's section, the effects wording, and the
/// precedence rule only when there is a User Context to outrank anything (§09 §8 puts it last). Same assembly
/// [`crate::captions_ask`] and [`crate::speeds_ask`] build, because §8 assembles every job's system message
/// the one way.
pub fn effects_system(context: &str) -> String {
    let paths = current_paths();
    crate::prompt_assembly::system_message(
        &wording(paths.as_ref(), "system"),
        "effects",
        &wording(paths.as_ref(), "effects"),
        context,
        false,
        false,
    )
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6). With no
/// settings folder at all, the shipped wording is what goes out, and with no shipped wording either the
/// prompt's own one-line shape ([`EFFECTS_SYSTEM`]).
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

/// F3.11 S1 (`one call over every clip · told the captions and speeds`): the user message. The clip list with
/// its notes comes from [`cut_effects_pass::brief`], which already marks a captioned clip
/// `N caption(s) -- runs at 1` and a sped one `plays at Nx` — so the ask and the placement rules read one
/// picture of the cut rather than two that can disagree. After it, the footage total and the prompt rules
/// §F3.11 states, each stated as a rule the model can check before it answers rather than as a wish.
///
/// Every span asked for is CLIP-RELATIVE ("start"/"end" inside the clip): the standing rule forbids asking a
/// model for a timestamp it would have to compute, and a decoration is placed by the app from the clip number
/// plus these two offsets.
pub fn user_message(clips: &Clips, speeds: &[(u32, f64)]) -> String {
    user_message_over(clips, speeds, &[], &[])
}

/// §9 (`With more than one camera row or lane the cut brief names the rows and lanes`): the
/// ask with the session's camera rows and heard lanes named above the clip list. The pass
/// that holds a real cut calls this; the two-argument form stays for a caller with no cut.
pub fn user_message_over(
    clips: &Clips,
    speeds: &[(u32, f64)],
    rows: &[String],
    lanes: &[String],
) -> String {
    let total: f64 = clips.clips().iter().map(|clip| clip.length).sum();
    let count = clips.clips().len();
    let mut lines = vec![format!(
        "{} {:.0} seconds over {} clip(s).",
        crate::cut_speed_pass::FOOTAGE_HEADER,
        total,
        count
    )];
    lines.push(String::new());
    lines.push(cut_effects_pass::brief_over(clips, speeds, rows, lanes));
    lines.push(String::new());
    lines.push(
        "Decorate only what needs it, and answer with those decorations and nothing else \u{2014} a cut that \
         wants no decoration gets an empty list."
            .to_string(),
    );
    lines.push("- Few and deliberate: about three or four across five minutes of finished video, each with a \
                reason you could say out loud. The User Context outranks that count."
                .to_string());
    lines.push("- A zoom runs two to four seconds. One stop per video, a second or two on the beat the video \
                is about.".to_string());
    lines.push("- volume sets how loud those seconds are: 1 as recorded, 0 silent. Use it for a stretch \
                recorded too quiet or too loud, for ducking a background under a line that matters, and for \
                muting what the context says is not to be heard.".to_string());
    lines.push("- Every effect names its clip and its seconds FROM THAT CLIP'S START, inside the clip. Never \
                a session timestamp.".to_string());
    lines.push("- A clip marked \"captions\" has words the viewer is reading: do not zoom past them. A clip \
                that plays at a rate was kept to be skipped through \u{2014} nothing to zoom onto there.".to_string());
    lines.join("\n")
}

/// The chat body: thinking OFF (§4 does not list `effects` among the prompts that think), and this job's two
/// tools offered — `add_effect` for the work and `finish` to close the reply. `Job::ClipRules` is §3.7's job
/// row (captions, speed and decorations share one state), so the names come from the catalogue and the two
/// this item names are then asserted present rather than assumed: a model that is not told `add_effect`
/// exists answers in prose about pacing instead of asking for a zoom.
///
/// Only the name travels, as every chat ask sends it: §1 keeps each tool's parameter schema in §3, and a copy
/// written here would be a second answer to §3.7's question about `clip`, `kind`, `start`, `end` and `gain`.
pub fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    let tools: Vec<Tool> = this_pass_tools();
    let messages = two_messages(system, user);
    let body = llm_request::body(model, &messages, Mode::Execute, Some(&tools), true)?;
    // No `prompt_cache_key`: §1's request contract names model, the two messages, `tools` when the job has
    // them, and the sampling five — and `llm_request::body` is the one place a chat request is built (§09 §5).
    Ok(body.to_string())
}

/// The two messages every decoration ask carries.
fn two_messages(system: &str, user: &str) -> Vec<Message> {
    vec![
        Message { role: "system".to_string(), parts: vec![Part::Text(system.to_string())] },
        Message { role: "user".to_string(), parts: vec![Part::Text(user.to_string())] },
    ]
}

/// This pass's own tools, from §3.7's job row: `add_effect` for the work and `finish` as the model's own signal.
/// A call of another kind (a caption, a rate) is not offered because this ask cannot place it (§2 S4), and
/// `get_frames` stays out too — §F3.11 names exactly these two on the wire (`tests/f3_11_s1_…` counts them), and a
/// zoom is aimed at the clip's centre by the app's own default rather than by a frame the model asked to see.
/// One list, used for the body AND for any later round, so the wire can never disagree with itself.
fn this_pass_tools() -> Vec<Tool> {
    crate::tools::offered(Job::ClipRules)
        .into_iter()
        .filter(|tool| matches!(tool, Tool::AddEffect | Tool::Finish))
        .collect()
}

/// An `add_effect` call's arguments read as a decoration: the same `{clip, kind, start, end, gain?}` shape the
/// prose document's entries have, so [`call_of`] decides both spellings one way.
pub fn call_of_args(args: &serde_json::Value) -> Option<Call> {
    call_of(args)
}

/// Read an EFFECTS reply into the calls this pass places. Two spellings are accepted, because a model offered
/// tools answers with tools and one told to answer JSON answers with JSON, and both mean the same decoration:
///
/// - an `add_effect(clip, kind, start, end, gain?)` tool call, in the order it came;
/// - else the content's `{"fx":[{"clip":…,"kind":…,"start":…,"end":…,"gain":…}]}` document
///   (§`system.md`'s `effects` row).
///
/// An entry missing its clip, kind, start or end, or carrying a non-finite number, is SKIPPED rather than
/// losing the whole reply: §F3.11 costs the round only for a clip outside the list, and that is
/// [`cut_effects_pass::place`]'s judgement to make on the calls that did arrive. An unknown `kind` is passed
/// through as text for the same reason — `place` reports it as a skip, so the sentence the user reads is the
/// tool's and not a second vocabulary invented here. `{"fx":[]}` is an empty list and a whole answer.
pub fn parse_effects(reply: &str) -> Vec<Call> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(reply.trim()) else {
        return Vec::new();
    };
    // A whole chat envelope is what comes off the wire; a bare EFFECTS document (or the same document under a
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
        .filter(|call| call.name == Tool::AddEffect.name())
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            call_of(&args)
        })
        .collect();
    if !calls.is_empty() {
        return calls;
    }
    // No tool call: read the content as the EFFECTS document. Anything unreadable is no calls, which the pass
    // reads as "the model proposed nothing" — the same ending a `{"fx":[]}` answer has.
    let Ok(content) = serde_json::from_str::<serde_json::Value>(answer.content.trim()) else {
        return Vec::new();
    };
    content
        .get("fx")
        .and_then(|fx| fx.as_array())
        .map(|fx| fx.iter().filter_map(call_of).collect())
        .unwrap_or_default()
}

/// One `{clip, kind, start, end, gain?}` entry, from either spelling. A missing or non-finite number, or no
/// kind, is no call. `gain` stays an `Option`: §F3.11 makes an omitted field and a 1 the same ask, while an
/// explicit 0 is silence and must survive the trip.
fn call_of(value: &serde_json::Value) -> Option<Call> {
    let clip = value
        .get("clip")
        .and_then(|clip| clip.as_f64().or_else(|| clip.as_str().and_then(|t| t.trim().parse::<f64>().ok())))
        .filter(|n| n.is_finite() && *n >= 1.0)? as u32;
    let kind = value
        .get("kind")
        .and_then(|kind| kind.as_str())
        .map(str::trim)
        .filter(|kind| !kind.is_empty())?
        .to_string();
    let number = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|t| t.trim().parse::<f64>().ok())))
            .filter(|n| n.is_finite())
    };
    let start = number("start")?;
    let end = number("end")?;
    Some(Call {
        clip,
        kind,
        start,
        end,
        gain: number("gain"),
    })
}

/// What one decorations ask came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Every decoration the model asked for, in the order it asked. Membership, the unknown-kind skip and the
    /// ignored gain are NOT applied here — that is [`cut_effects_pass::place`]'s job, so one route decides and
    /// this one only reports.
    pub calls: Vec<Call>,
    /// The ask failed at the wire, so the caller should say why rather than treat the empty list as "the model
    /// wanted nothing". An empty EFFECTS document is NOT a failure.
    pub failed: bool,
    /// The reason, naming the model and the fault. Empty when nothing failed.
    pub reason: String,
}

/// One live decorations ask: build the two messages, honour the cache, dial, read the reply. Settings are
/// read HERE, per call, so an address changed while a run is going is honoured by the next ask
/// (§02-services#1).
///
/// `clips` is the batch the pass just built (lengths, captions and rates on it already) and `speeds` the rate
/// each clip plays at, so S1's brief can name both before any decoration is asked for.
pub fn ask(
    tree: &Tree,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_named(tree, context, clips, speeds, &[], &[], attempt, cancel)
}

/// [`ask`] with the session's camera rows and heard lanes named in the brief (§9).
#[allow(clippy::too_many_arguments)] // the ask carries the brief's rows and lanes beside its clips
pub fn ask_named(
    tree: &Tree,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
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
    ask_at_over(tree, &conf, context, clips, speeds, rows, lanes, attempt, cancel)
}

/// [`ask`] with the settings already read, for a caller that resolved them itself.
pub fn ask_at(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    ask_at_over(tree, conf, context, clips, speeds, &[], &[], attempt, cancel)
}

/// [`ask_at`] with the session's camera rows and heard lanes named in the brief (§9: the
/// passes know nothing of cameras and lanes until there is more than one of either).
#[allow(clippy::too_many_arguments)] // as [`ask_named`]: the brief's rows and lanes ride along
pub fn ask_at_over(
    tree: &Tree,
    conf: &Conf,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    rows: &[String],
    lanes: &[String],
    attempt: u32,
    cancel: &dyn Fn() -> bool,
) -> Asked {
    if clips.clips().is_empty() {
        // Nothing to ask about is not a failure and not worth a request.
        return Asked { calls: Vec::new(), failed: false, reason: String::new() };
    }
    let model = conf.model.clone();
    let system = effects_system(context);
    let user = user_message_over(clips, speeds, rows, lanes);
    // A retry asks with the same two messages, so it needs its own cache slot: without one the second round
    // would be served the reply that was just rejected (§6).
    let parts = cache_parts(&system, &user, &model, attempt);
    if let Some(cached) = llm_cache::read(tree, STEP, &parts) {
        // A reply served from the cache costs no wire and writes no row: the request that earned it is
        // already in `requests.tsv`, and a second row for the same answer would double-count a call that did
        // not happen (§09 §10).
        return Asked { calls: parse_effects(&cached), failed: false, reason: String::new() };
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
        // The ask itself failed. §F3.11's two rounds count rejections of ANSWERS, so a wire failure spends none of
        // them: a server that is not there answers the same way however often it is called.
        Err(reason) => return failed(reason),
    };
    let reply = match llm_request::reply_from(&sent.content_type, &sent.body) {
        Ok(reply) => reply,
        Err(_) => {
            // Neither JSON nor a stream: nothing to place, read as "the model proposed nothing" — the same ending
            // an empty EFFECTS document has (§F3.11 does not fail a cut over an unreadable decoration list).
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
        return Asked { calls: parse_effects(&text), failed: false, reason: String::new() };
    }
    // §2's loop from here, over the CLIP state the tools write into: `get_frames` is answered from it (a zoom has
    // to be aimed), `add_effect` lands in it, and each call is RUN before another request goes out — the same door
    // the loop uses, so an unknown kind or an empty span comes back as the tool's own sentence and is SAID here
    // (§F3.11 S3). Membership stays `cut_effects_pass::place`'s judgement (§F3.11 S2).
    let mut batch = clips.clone();
    for line in crate::clip_rules_tools_leg::answer_calls(&reply.tool_calls, &mut batch) {
        crate::ui::window::log_line(&line);
    }
    let calls_out: Vec<Call> = reply
        .tool_calls
        .iter()
        .filter(|call| call.name == Tool::AddEffect.name())
        .filter_map(|call| {
            let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
            call_of_args(&args)
        })
        .collect();
    // A call this pass can already answer ENDS the ask: the model has its result, so no second request goes out
    // (§F3.11 counts batches by rows in `requests.tsv`).
    Asked { calls: calls_out, failed: false, reason: String::new() }
}

/// The shape an answer is kept in: the call list for a tool answer, the plain document otherwise. See
/// [`parse_effects`], which reads both back.
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
fn messages(context: &str, clips: &Clips, speeds: &[(u32, f64)]) -> (String, String) {
    (effects_system(context), user_message(clips, speeds))
}

/// The file one ask's answer is cached in — `Tree::cache_llm`'s layout and `llm_cache`'s key, reached through
/// `cache_parts`, the ONE key recipe the pass itself stores by, so the two cannot drift apart. Named
/// `_for_test` because only a proof names the file: the pass never asks for a path, it just stores and reads.
/// A second hash recipe inside a test would be a rule able to disagree with the code.
pub fn cache_file_for_test(tree: &Tree, context: &str, clips: &Clips, speeds: &[(u32, f64)], model: &str, attempt: u32) -> PathBuf {
    let (system, user) = messages(context, clips, speeds);
    // A missing key is no file: `llm_cache::store` refuses to write one either (§6).
    let key = llm_cache::key(&cache_parts(&system, &user, model, attempt)).unwrap_or_default();
    tree.cache_llm(STEP, &key)
}

/// The deciding parts for one decorations ask. The retry's slot rides in `state` — the texts, model and
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
        thinking: false,
    };
    llm_cache::parts(&request)
}

/// A distinct cache slot per attempt of the same cut. The first ask uses the plain hash of the system text;
/// each retry appends its number, so a retry keys elsewhere than the rejection it is retrying rather than
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
