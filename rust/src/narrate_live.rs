//! F4.2's live half: the narration call over real HTTP.
//!
//! [`crate::narrate_call`] owns the two messages and the attempt loop, and [`crate::narrate_pass`]
//! owns every §3.8 tool answer — but nothing until now *telephoned*: `press_narrate_run`'s no-script
//! branch answered `Ok(vec![])`, so a press with a real server configured wrote nothing and reported
//! its own emptiness as if that were the answer. This module is that leg. It reads the Settings file on
//! every ask (so an address typed mid-run is honoured by the next request, §02-services#1), builds the
//! chat body from the pair `narrate_call::request` assembled, offers §3.8's six tools plus the web
//! tools (§F4.2: "thinking ON · web tools"), dials through [`crate::server_leg`] so each round trips
//! into a timed `requests.tsv` row, and reads the reply back.
//!
//! Two answer shapes both arrive from real servers, and both are handled here rather than in the rules
//! module:
//!
//! * **A content answer** already carrying the prototype's `{"entries":[{start,end,at,text,emotion}]}`
//!   document is handed straight back for [`narrate_call::drive`] to match against echoed bounds.
//! * **A tool-call answer** runs through [`tool_loop::run`], whose `call` closure dispatches to the
//!   EXISTING `narrate_pass` tools — not copies of them. The collected `write_line`/`leave_silent`
//!   calls become the entries string with each clip's OWN bounds echoed (`clips[clip-1].s/.e`), because
//!   the model answers in clip numbers and offsets and the app does the arithmetic (§00-principles).
//!
//! A `leave_silent(clip)` becomes an entry with empty text: silence is an ANSWER (the writer looked at
//! the clip and left it alone), while a clip with no entry at all fails the attempt — and
//! `drive`'s all-empty check still catches a whole reply that refused everything.
//!
//! A server that is down, or a reply that cannot be read, answers `Err` naming the address and the
//! model. Never `Ok` with nothing in it: that would let a dead box look like a writer with taste.

use crate::cut::Seg;
use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_request;
use crate::narrate_call::Request;
use crate::narrate_pass;
use crate::requests::{Request as TimedRow, Service};
use crate::server_leg;
use crate::services::{self, Kind};
use crate::settings::{self, Conf, Paths};
use crate::tool_loop::{self, Turn};
use crate::tools::Tool;
use serde_json::json;

/// The step name this job's timed rows carry. §3.8 is the section; `narrate` is what a person reads in
/// `requests.tsv` and in the run's per-service summary line (§09 §10).
pub const STEP: &str = "narrate";

/// The settings folder, re-read from the environment on every ask so a change made while a run is going
/// is honoured by its next request (§02-services#1).
fn current_paths() -> Option<Paths> {
    settings::from_environment()
}

/// What one round of the narration conversation came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    /// The answer text: either the entries document itself, or the entries built from the tool calls.
    pub answer: String,
    /// Whether this round asked for tools rather than answering outright. Recorded so a test can pin
    /// that the tool path really was taken over the wire, not only that an answer came back.
    pub used_tools: bool,
    /// How many times this call went out. More than one means the writer asked for tools and the
    /// conversation kept going; one means the answer arrived straight back.
    pub requests: u32,
}

/// The content of the LAST assistant message in a conversation, as a server's answer — never the
/// reasoning, per [`llm_request::Reply::answer`]'s rule.
fn assistant_content(reply: &llm_request::Reply) -> String {
    reply.answer().to_string()
}

/// One round of the narration call, live. `attempt` rides into the log so a retry reads as a retry,
/// and `thinking` is whatever `narrate_call::drive` decided this attempt gets (§F4.2: thinking starts
/// ON and is switched off after a call that wrote nothing) — this module obeys it rather than owning it.
///
/// every argument the leg needs is read HERE, per call: the address, the key and the model come from
/// the Settings file as it stands now, and the two messages come from the caller's already-built
/// [`narrate_call::Request`] so the pair a test pinned is the pair that goes out.
#[allow(clippy::too_many_arguments)]
pub fn ask_round(
    tree: &Tree,
    request: &Request,
    clips: &[Seg],
    transcript: &[crate::textfmt::SessionLine],
    narrator: &str,
    attempt: u32,
    thinking: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<Round, String> {
    let conf = match current_paths().as_ref().map(settings::read) {
        Some(Ok(conf)) => conf,
        Some(Err(reason)) => {
            // An unreadable settings file is reported the way a down server is: the run says why
            // rather than dialling a guessed address.
            return Err(format!("settings could not be read: {reason}"));
        }
        None => Conf::default(),
    };
    let model = conf.model.clone();
    let endpoint = services::llm_endpoint(&conf);
    let offered = offered_tools();

    // Shape of the first round: the two messages, thinking as told, tools offered. Later rounds grow
    // inside the tool loop, which pushes assistant and tool turns onto this same list.
    let messages = vec![
        Message { role: "system".into(), parts: vec![Part::Text(request.system.clone())] },
        Message { role: "user".into(), parts: vec![Part::Text(request.user.clone())] },
    ];

    // The loop's `ask` closure: build the body, dial, read one turn. Each dial writes its own timed
    // row, so a nine-round tool conversation leaves nine lines behind (§09 §10: a line per request).
    //
    // The round's answer is remembered HERE, at the dial, rather than read back out of the finished
    // conversation: a nudge round adds no answer of its own, and picking the content out of the
    // message list afterwards lost it behind the nudge — while §09 §10 says the row and the answer
    // belong to the reply that actually arrived.
    let mut requests = 0u32;
    let mut last_answer = String::new();
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<Turn, String> {
        requests += 1;
        let mode = if thinking { Mode::Thinking } else { Mode::Execute };
        let body = llm_request::body(&model, conversation, mode, Some(tools), false)?.to_string();
        let base = base_row(&model, attempt);
        let (_row, reply) = server_leg::call(
            tree,
            &base,
            services::Server::Llm,
            &endpoint,
            Kind::Chat,
            None,
            Some(&body),
            cancelled,
        );
        let sent = reply.map_err(|why| format!("{STEP} call to {model}: {why}"))?;
        if !sent.ok() {
            return Err(format!(
                "{STEP} call to {model} at {} answered {status}",
                endpoint.url,
                status = sent.status
            ));
        }
        let parsed: serde_json::Value = serde_json::from_str(&sent.body)
            .map_err(|_| format!("{STEP}: the server's answer is not JSON"))?;
        let reply = llm_request::parse_reply(&parsed);
        // `finish` closes the flow, and `tool_loop` reads it off the TURN, not off the call list — so
        // the leg says it here when the writer called it. Detected on the call NAMES rather than by
        // running them, so the closing round is not counted as one that wrote a line.
        let finishing = reply
            .tool_calls
            .iter()
            .any(|call| call.name == Tool::Finish.name());
        let answer = assistant_content(&reply);
        if !answer.is_empty() {
            last_answer = answer.clone();
        }
        Ok(Turn {
            content: answer,
            calls: reply.tool_calls,
            finish: finishing,
        })
    };

    // The loop's `call` closure: run one tool through the tools that ALREADY answer these (§3.8), and
    // remember the ones that write lines so the entries document can be built at the end.
    let mut collected: Vec<Picked> = Vec::new();
    let mut used_tools = false;
    let mut run_tool = |asked: &crate::exchanges::ToolCall| -> String {
        used_tools = true;
        run_narrate_tool(asked, clips, transcript, narrator, &mut collected)
    };

    // The loop's result is not read: the answer was taken at each dial (`last_answer`) and the tool
    // calls at each dispatch (`collected`), so nothing here needs the finished conversation.
    let _run = tool_loop::run(STEP, messages, &offered, &mut ask, &mut run_tool);

    // A content answer with no tool calls at all: the prototype's own shape, handed on untouched so
    // `drive`'s forward-only ±0.5 s match is what decides, exactly as for a scripted reply.
    if !used_tools {
        return Ok(Round {
            answer: last_answer,
            used_tools: false,
            requests,
        });
    }
    if collected.is_empty() {
        // Tools were called but none wrote a line or said "silent": the writer asked about emotions
        // and read the transcript and then stopped. That is no answer, and saying so beats handing
        // `drive` an empty document it would have to guess about.
        return Err(format!(
            "{STEP}: the reply used tools but wrote no line and left no clip silent"
        ));
    }
    Ok(Round { answer: entries_of(&collected, clips)?, used_tools: true, requests })
}

/// The tools this job is offered: §3.8's six plus the web tools the spec says ride on this call
/// ("thinking ON · web tools"). Taken from `tools::offered(Job::Narrate)`, which already appends
/// `web_search`/`web_read` for this job, minus the shared reading tools this item's flow never answers
/// (`get_context`/`get_events`/`get_frames` belong to other sections' rounds and would be offered
/// with no handler here). Listing them out rather than filtering keeps the offer honest: every name on
/// the wire has a function behind it in this module.
pub fn offered_tools() -> Vec<Tool> {
    vec![
        Tool::WriteLine,
        Tool::LeaveSilent,
        Tool::ListEmotions,
        Tool::GetLines,
        Tool::DescribeInsert,
        Tool::Finish,
        Tool::WebSearch,
        Tool::WebRead,
    ]
}

/// One line the writer asked for, before its clip's bounds are echoed back into it.
#[derive(Debug, Clone)]
struct Picked {
    clip: u32,
    at: f64,
    text: String,
    emotion: String,
}

/// Run one tool call through the §3.8 answers that already exist in [`narrate_pass`]. Args are read
/// as JSON, the way the wire sends them; a missing field takes the tool's own default rather than
/// failing the round, because each of those defaults is itself a rule the pass already states.
fn run_narrate_tool(
    asked: &crate::exchanges::ToolCall,
    clips: &[Seg],
    transcript: &[crate::textfmt::SessionLine],
    narrator: &str,
    collected: &mut Vec<Picked>,
) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&asked.args).unwrap_or_else(|_| json!({}));
    let number = |key: &str, fallback: f64| -> f64 {
        args.get(key)
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
            .unwrap_or(fallback)
    };
    let text = |key: &str| -> String {
        args.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string()
    };

    match asked.name.as_str() {
        "write_line" => {
            let clip = number("clip", 0.0) as u32;
            let at = number("at", 0.0);
            let line = text("text");
            let emotion = text("emotion");
            let pos = text("pos");
            let answer = narrate_pass::write_line(clips, clip, at, &line, &emotion, &pos);
            // Only a refusal leaves nothing collected: the tool's answer is what the model sees, and
            // a rejected line is not a line.
            if answer_is_ok(&answer) {
                collected.push(Picked {
                    clip,
                    at: placed_at(&answer, at),
                    text: line,
                    emotion,
                });
            }
            answer
        }
        "leave_silent" => {
            let clip = number("clip", 0.0) as u32;
            let answer = narrate_pass::leave_silent(clip);
            if answer_is_ok(&answer) {
                collected.push(Picked { clip, at: 0.0, text: String::new(), emotion: String::new() });
            }
            answer
        }
        "list_emotions" => narrate_pass::list_emotions(),
        "get_lines" => {
            // `from`/`to` are seconds relative to the clip the writer is working on; with no clip
            // named there is no anchor to read them against, and saying so is better than assuming 0.
            let Some(index) = (number("clip", 0.0) as usize).checked_sub(1) else {
                return crate::tools::error("get_lines needs the clip whose surroundings you want -- say \"clip\"");
            };
            let Some(seg) = clips.get(index) else {
                return crate::tools::error(
                    "get_lines needs the clip whose surroundings you want -- say \"clip\"",
                );
            };
            let from = seg.s + number("from", -10.0);
            let to = seg.s + number("to", 10.0);
            narrate_pass::get_lines(transcript, seg.s, from, to, narrator)
        }
        "describe_insert" => {
            let clip = number("clip", 0.0) as u32;
            narrate_pass::describe_insert(clips, clip)
        }
        "finish" => {
            // Which clips got an answer, and which of their lines will not fit: fitting is F4.3's
            // measurement and this headless leg has no render to measure against, so nothing is
            // claimed unfitted and `finish` answers on completeness alone.
            let answered: Vec<u32> = collected.iter().map(|p| p.clip).collect();
            narrate_pass::finish(&answered, &[])
        }
        other => crate::tools::error(&format!("{other} is not a tool of this call")),
    }
}

/// Whether a tool answer is a success rather than the `{}` error shape (same read `narrate_call` uses).
fn answer_is_ok(answer: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(answer).is_ok_and(|body| body.get("error").is_none())
}

/// The offset the tool actually placed the line at — the clamped one it answered with, not the one the
/// model asked for, so what lands in the record is what the tool said would play.
fn placed_at(answer: &str, asked_at: f64) -> f64 {
    serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|body| body.get("at").and_then(|v| v.as_f64()))
        .unwrap_or(asked_at)
}

/// Build the `{"entries":[…]}` document `narrate_call::drive` reads, echoing each clip's OWN bounds
/// beside the offset the writer gave. The echo is not decoration: drive matches an entry to a clip by
/// those bounds within [`crate::narrate_reply::ENTRY_MATCH_TOLERANCE_SECONDS`], so a clip number is
/// turned into session seconds HERE and never asked of the model (§00: models propose, the machine places).
/// Entries are emitted in clip order, because out-of-order clips are the thing drive refuses.
fn entries_of(picked: &[Picked], clips: &[Seg]) -> Result<String, String> {
    let mut ordered: Vec<&Picked> = picked.iter().collect();
    ordered.sort_by(|a, b| a.clip.cmp(&b.clip).then(a.at.total_cmp(&b.at)));
    let entries: Vec<serde_json::Value> = ordered
        .iter()
        .map(|p| {
            let index = (p.clip as usize).checked_sub(1)
                .ok_or_else(|| format!("clip {} is not one of this edit's clips", p.clip))?;
            let seg = clips
                .get(index)
                .ok_or_else(|| format!("clip {} is not one of this edit's clips", p.clip))?;
            Ok(json!({
                "start": seg.s,
                "end": seg.e,
                "at": p.at,
                "text": p.text,
                "emotion": p.emotion,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({ "entries": entries }).to_string())
}

/// The row every round of this call carries. Same eighteen-field shape as every other ask's row; the
/// timings are filled by `server_leg`, and `attempt` says which of the three this was.
fn base_row(model: &str, attempt: u32) -> TimedRow {
    TimedRow {
        started: String::new(),
        run: String::new(),
        step: STEP.to_string(),
        job: STEP.to_string(),
        service: Service::Llm,
        model: model.to_string(),
        kind: "chat".to_string(),
        attempt,
        ..Default::default()
    }
}
