//! F3.10 S1's live half: the speed ask itself.

use crate::cut_speed_pass::{self, Call, GAP_SECONDS};
use crate::exchanges::{Message, Part};
use crate::layout::Tree;
use crate::llm_cache;
use crate::llm_request;
use crate::requests::{Request, Service};
use crate::roles::Job;
use crate::services;
use crate::settings::{self, Conf, Paths};
use crate::tools::Tool;
use std::path::PathBuf;

/// The step name this job's cache entries and timed rows carry.
pub const STEP: &str = "speed";

/// The answer shape §`spec/prompts/system.md`'s `speed` row spells, with the two rules that row states.
pub const SPEEDS_SYSTEM: &str = concat!(
    "Answer in JSON only: {\"speeds\":[{\"clip\":<n>,\"rate\":<x>}]} \u{2014} clip is the clip number from \
     the list, rate > 1. A clip that already has a caption cannot be sped up; leave it out.\n",
    "Strict JSON and nothing else: no prose, no code fence, no comment."
);

/// The SYSTEM for a speed call: the house rules cut to this job's section, the speed wording, and the
/// precedence rule only when there is a User Context to outrank anything (§09 §8 puts it last).
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

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6).
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

/// The gap two same-rate stretches may come before they are one run.
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
pub fn user_message(
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    total_seconds: f64,
) -> String {
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
    lines.push(
        "- Only rates above 1. Below 1 costs seconds instead of saving them, and is not this job."
            .to_string(),
    );
    lines.push(
        "- Never a timestamp and never a second count: a clip is one rate from end to end."
            .to_string(),
    );
    lines.push("- A clip marked \"captions\" plays at 1 whatever else is true: words on screen at 4 are gone before they are read.".to_string());
    lines.push(format!(
        "- Two stretches at the same rate nearer than {gap} s apart are one stretch, so give them one rate."
    ));
    lines.join("\n")
}

/// The chat body: thinking OFF (§4 does not list `speed` among the four prompts that think), and this job's
/// two tools offered — `set_clip_speed` for the work and `finish` to close the reply.
pub fn chat_body(model: &str, system: &str, user: &str) -> Result<String, String> {
    let messages = two_messages(system, user);
    let tools: Vec<Tool> = this_pass_tools();
    Ok(llm_request::body(
        STEP,
        model,
        &messages,
        crate::prompts::mode_of(STEP),
        Some(&tools),
        true,
    )?
    .to_string())
}

fn two_messages(system: &str, user: &str) -> Vec<Message> {
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

/// This pass's own two tools, from §3.7's job row: a call of another kind (a caption, a decoration) is not
/// offered because this ask cannot place it (§2 S4).
fn this_pass_tools() -> Vec<Tool> {
    crate::tools::offered(Job::ClipRules)
        .into_iter()
        .filter(|tool| matches!(tool, Tool::SetClipSpeed | Tool::Finish | Tool::GetFrames))
        .collect()
}

// NOTE: `prompt_cache_key` is not sent anywhere: §1's request contract (the table `tests/f3_9_s3_a_…` reads back
// on the wire) names model, the two messages, `tools` when the job has them, and the sampling five — no such
// field, and `llm_request::body` is the one place a chat request is built (§09 §5), so inventing a key here
// would be a field no server in §1 documents and no other ask sends.

/// Read a SPEEDS reply into the calls this pass places.
pub fn call_of_args(args: &serde_json::Value) -> Option<Call> {
    call_of(args)
}

pub fn parse_speeds(reply: &str) -> Vec<Call> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(reply.trim()) else {
        return Vec::new();
    };
    // A whole chat envelope is what comes off the wire; a bare SPEEDS document (or the same document under a
    // `content` key) is what sits in the cache, because §6 keeps the answer text and not the framing.
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
    let Ok(content) = serde_json::from_str::<serde_json::Value>(answer.content.trim()) else {
        return Vec::new();
    };
    content
        .get("speeds")
        .and_then(|speeds| speeds.as_array())
        .map(|speeds| speeds.iter().filter_map(call_of).collect())
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
    let rate = value
        .get("rate")
        .and_then(|rate| {
            rate.as_f64()
                .or_else(|| rate.as_str().and_then(|t| t.trim().parse::<f64>().ok()))
        })
        .filter(|rate| rate.is_finite())?;
    Some(Call { clip, rate })
}

/// What one speed ask came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Every rate the model asked for, in the order it asked.
    pub calls: Vec<Call>,
    /// The ask failed at the wire, so the caller should say why rather than treat the empty list as "the
    /// model wanted nothing sped up".
    pub failed: bool,
    /// The reason, naming the model and the fault.
    pub reason: String,
}

/// One live speed ask: build the two messages, honour the cache, dial, read the reply.
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
        Some(Err(reason)) => return failed(format!("settings could not be read: {reason}")),
        None => Conf::default(),
    };
    // No LLM address at all: name it, because "no usable answer" would blame the model for a server nobody
    // typed in.
    if services::server_url(&conf.server).is_none() {
        return failed("no LLM address is set \u{2014} nothing was asked".to_string());
    }
    ask_at_over(
        tree, &conf, context, clips, captioned, rows, lanes, attempt, cancel,
    )
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
    ask_at_over(
        tree,
        conf,
        context,
        clips,
        captioned,
        &[],
        &[],
        attempt,
        cancel,
    )
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
        return Asked {
            calls: Vec::new(),
            failed: false,
            reason: String::new(),
        };
    }
    let model = conf.model.clone();
    let total: f64 = clips.iter().map(|(_, length)| length).sum();
    let system = speeds_system(context);
    let user = user_message_over(clips, captioned, total, rows, lanes);
    // A retry asks with the same two messages, so it needs its own cache slot: without one the second round
    // would be served the reply that was just refused (§6).
    let parts = cache_parts(&system, &user, &model, attempt);
    if let Some(cached) = llm_cache::read(tree, STEP, &parts) {
        // §09 §10: the cached answer writes its own `cache` line, with no time on the wire; the row is
        // the record that this ask cost nothing, not a second count of the request that earned it.
        crate::request_timing::record_cached(tree, base_row(&model));
        return Asked {
            calls: parse_speeds(&cached),
            failed: false,
            reason: String::new(),
        };
    }
    let base = base_row(&model);
    let endpoint = services::llm_endpoint(conf);
    let offered = this_pass_tools();
    // How to build the body for one offer; `None` means no `tools` key, which is what §2 S5 resends to a server
    // that refused the field.
    let build = |offer: Option<&[Tool]>| {
        Ok(llm_request::body(
            STEP,
            &model,
            &two_messages(&system, &user),
            // §4's flag for this prompt, read from the one table (§10 §4: thinking off outside
            // textedit/cut/narrate/youtube).
            crate::prompts::mode_of(STEP),
            offer,
            true,
        )?
        .to_string())
    };
    // One live ask is ONE request on the wire.
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
                cancel,
                &crate::ui::window::log_line,
            )
        },
        &crate::ui::window::log_line,
    ) {
        Ok(sent) => sent,
        // The ask itself failed.
        Err(reason) => return failed(reason),
    };
    let reply = match llm_request::reply_from(&sent.content_type, &sent.body) {
        Ok(reply) => reply,
        Err(_) => {
            // Neither JSON nor a stream: nothing to place, read as "the model proposed nothing" — the same
            // ending a `{"speeds":[]}` answer has (§F3.10 does not fail a cut over an unreadable rate list).
            return Asked {
                calls: Vec::new(),
                failed: false,
                reason: String::new(),
            };
        }
    };
    if reply.tool_calls.is_empty() {
        if crate::llm_tools_offer::tools_for(&endpoint.url, &offered).is_none() {
            let text = reply.answer().to_string();
            if !text.trim().is_empty() {
                llm_cache::store(tree, STEP, &parts, &text);
            }
            return Asked {
                calls: parse_speeds(&text),
                failed: false,
                reason: String::new(),
            };
        }
        // §3's answered question on this item: the tools ARE on the wire, and a reply carrying a JSON document
        // instead of calling `set_clip_speed` has not answered.
        let went_out = std::time::Instant::now();
        let dial = |body: &str| {
            crate::llm_retry_leg::chat(
                STEP,
                tree,
                &base,
                &endpoint,
                body,
                cancel,
                &crate::ui::window::log_line,
            )
            .map(|sent| (sent.content_type, sent.body))
        };
        let second = match crate::clip_rules_tools_leg::ask_again_for_calls(
            STEP,
            Tool::SetClipSpeed.name(),
            two_messages(&system, &user),
            &reply,
            &crate::ui::window::log_line,
            &|conversation| {
                Ok(llm_request::body(
                    STEP,
                    &model,
                    conversation,
                    crate::prompts::mode_of(STEP),
                    Some(&offered),
                    true,
                )?
                .to_string())
            },
            &dial,
        ) {
            Ok(second) => second,
            Err(reason) => return failed(reason),
        };
        // That retry was a request, and §09 §10 wants every request on the record; the repair leg owns the dial,
        // so the row is written here (`chat` files rows only for what IT dials -- one line per request).
        crate::clip_rules_tools_leg::record_repair_row(tree, base_row(&model), went_out);
        let calls: Vec<Call> = second
            .tool_calls
            .iter()
            .filter(|call| call.name == Tool::SetClipSpeed.name())
            .filter_map(|call| {
                let args = serde_json::from_str::<serde_json::Value>(&call.args).ok()?;
                call_of_args(&args)
            })
            .collect();
        // §6: the ANSWERING round's envelope is what is kept, so an unchanged batch costs no socket.
        if crate::clip_rules_tools_leg::asks_for_anything(&second) {
            llm_cache::store(
                tree,
                STEP,
                &parts,
                &crate::clip_rules_tools_leg::answering_envelope(&second).to_string(),
            );
        }
        return Asked {
            calls,
            failed: false,
            reason: String::new(),
        };
    }
    // The refusal a captioned clip earns comes back as the tool's own sentence, so it has to be REACHED.
    let mut batch = crate::clip_rules_tools_leg::batch_of(clips);
    // The frames a model may aim at are only this far apart: §3.7's `get_frames` walks the batch's
    // grid, and a default 1 s spread would name pictures this session never extracted.
    for (clip, count) in captioned {
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
    // §2's loop from here, over the batch that already knows which clips carry captions.
    for line in crate::clip_rules_tools_leg::answer_calls(&reply.tool_calls, &mut batch) {
        crate::ui::window::log_line(&line);
    }
    // §6: the ANSWERING round's envelope is what is kept, so an unchanged batch costs no socket when the cut is
    // asked again.
    if crate::clip_rules_tools_leg::asks_for_anything(&reply) {
        llm_cache::store(
            tree,
            STEP,
            &parts,
            &crate::clip_rules_tools_leg::answering_envelope(&reply).to_string(),
        );
    }
    // A call this pass can already answer ENDS the ask: the model has its result and there is nothing to ask it
    // for, so no second request goes out (§F3.10 counts batches by rows in `requests.tsv`).
    Asked {
        calls: calls_out,
        failed: false,
        reason: String::new(),
    }
}

/// `stored_form` is gone: since §3's answer on this item the answer IS a call list, so what a pass caches is
/// the answering round's envelope (see [`crate::clip_rules_tools_leg::answering_envelope`]).
fn messages(context: &str, clips: &[(u32, f64)], captioned: &[(u32, usize)]) -> (String, String) {
    let total: f64 = clips.iter().map(|(_, length)| length).sum();
    (
        speeds_system(context),
        user_message(clips, captioned, total),
    )
}

/// The file one ask's answer is cached in — `Tree::cache_llm`'s layout and `llm_cache`'s key, reached through
/// `cache_parts`, the ONE key recipe the pass itself stores by, so the two cannot drift apart.
pub fn cache_file_for_test(
    tree: &Tree,
    context: &str,
    clips: &[(u32, f64)],
    captioned: &[(u32, usize)],
    model: &str,
    attempt: u32,
) -> PathBuf {
    let (system, user) = messages(context, clips, captioned);
    // A missing key is no file: `llm_cache::store` refuses to write one either (§6).
    let key = llm_cache::key(&cache_parts(&system, &user, model, attempt)).unwrap_or_default();
    tree.cache_llm(STEP, &key)
}

/// The deciding parts for one speed ask.
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

fn ask_slot(system: &str, attempt: u32) -> String {
    if attempt <= 1 {
        return system.to_string();
    }
    format!("{system}\u{21bb}{attempt}")
}

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

fn failed(reason: String) -> Asked {
    Asked {
        calls: Vec::new(),
        failed: true,
        reason,
    }
}
