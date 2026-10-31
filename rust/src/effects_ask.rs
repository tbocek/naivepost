//! F3.11 S1's live half: the decorations ask itself.

use crate::clip_pass_ask;
use crate::cut_effects_pass::{self, Call};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::roles::Job;
use crate::services;
use crate::settings::{self, Conf, Paths};
use crate::tools::clips::Clips;
use crate::tools::Tool;
use std::path::PathBuf;

pub const STEP: &str = "effects";

pub const EFFECTS_SYSTEM: &str = concat!(
    "Answer in JSON only: {\"fx\":[{\"clip\":<n>,\"kind\":\"zoom|stop|volume\",\"start\":<sec>,\"end\":<sec>,\
     \"gain\":<x>}]} \u{2014} clip is the clip number from the list, seconds are from that clip's start and \
     stay inside it, gain belongs to volume alone (1 as recorded, 0 silent).\n",
    "Few and deliberate: about three or four across five minutes unless the context says otherwise. A zoom \
     runs two to four seconds. One stop per video. Volume for level, ducking or muting. Never zoom past \
     captions.\n",
    "Strict JSON and nothing else: no prose, no code fence, no comment."
);

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

fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

pub fn user_message(clips: &Clips, speeds: &[(u32, f64)]) -> String {
    user_message_over(clips, speeds, &[], &[])
}

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

pub fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    let tools: Vec<Tool> = this_pass_tools();
    let messages = clip_pass_ask::two_messages(system, user);
    let body = llm_request::body(
        STEP,
        model,
        &messages,
        crate::prompts::mode_of(STEP),
        Some(&tools),
        true,
    )?;
    // No `prompt_cache_key`: §1's request contract names model, the two messages, `tools` when the job has
    // them, and the sampling five — and `llm_request::body` is the one place a chat request is built (§09 §5).
    Ok(body.to_string())
}

/// This pass's own tools, from §3.7's job row: `add_effect` for the work and `finish` as the model's own signal.
fn this_pass_tools() -> Vec<Tool> {
    crate::tools::offered(Job::ClipRules)
        .into_iter()
        .filter(|tool| matches!(tool, Tool::AddEffect | Tool::Finish | Tool::GetFrames))
        .collect()
}

pub fn call_of_args(args: &serde_json::Value) -> Option<Call> {
    call_of(args)
}

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
            // A slot written by `answering_envelope` carries each call's arguments as a STRING, so this is the
            // only read path; a non-string argument (a server that sent an object) would print with its own
            // braces and fail to parse here, dropping every decoration in the reply.
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

fn call_of(value: &serde_json::Value) -> Option<Call> {
    let clip = value
        .get("clip")
        .and_then(|clip| {
            clip.as_f64()
                .or_else(|| clip.as_str().and_then(|t| t.trim().parse::<f64>().ok()))
        })
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
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|t| t.trim().parse::<f64>().ok()))
            })
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

pub type Asked = clip_pass_ask::Asked<Call>;

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
        Some(Err(reason)) => return Asked::failed(format!("settings could not be read: {reason}")),
        None => Conf::default(),
    };
    // No LLM address at all: name it, because "no usable answer" would blame the model for a server nobody
    // typed in. §02-services#1: an empty box means the loopback default, which is what `llm_endpoint` answers.
    if services::server_url(&conf.server).is_none() {
        return Asked::failed("no LLM address is set \u{2014} nothing was asked".to_string());
    }
    ask_at_over(
        tree, &conf, context, clips, speeds, rows, lanes, attempt, cancel,
    )
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
    ask_at_over(
        tree,
        conf,
        context,
        clips,
        speeds,
        &[],
        &[],
        attempt,
        cancel,
    )
}

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
        return Asked::answered(Vec::new());
    }
    clip_pass_ask::run(
        tree,
        conf,
        cancel,
        &clip_pass_ask::Pass {
            step: STEP,
            tool: Tool::AddEffect,
            tools: this_pass_tools(),
            model: &conf.model,
            system: &effects_system(context),
            user: &user_message_over(clips, speeds, rows, lanes),
            attempt,
            batch: clips.clone(),
            parse: &parse_effects,
            call_of_args: &call_of_args,
        },
    )
}

fn messages(context: &str, clips: &Clips, speeds: &[(u32, f64)]) -> (String, String) {
    (effects_system(context), user_message(clips, speeds))
}

pub fn cache_file_for_test(
    tree: &Tree,
    context: &str,
    clips: &Clips,
    speeds: &[(u32, f64)],
    model: &str,
    attempt: u32,
) -> PathBuf {
    let (system, user) = messages(context, clips, speeds);
    // A missing key is no file: `llm_cache::store` refuses to write one either (§6).
    let key = llm_cache::key(&clip_pass_ask::cache_parts(&system, &user, model, attempt))
        .unwrap_or_default();
    tree.cache_llm(STEP, &key)
}
