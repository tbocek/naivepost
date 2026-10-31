//! F4.2's live half: the narration call over real HTTP.

use crate::cut::Seg;
use crate::exchanges::{Message, Mode, Part};
use crate::layout::Tree;
use crate::llm_request;
use crate::narrate_call::Request;
use crate::narrate_pass;
use crate::requests::{Request as TimedRow, Service};
use crate::services;
use crate::settings::{self, Conf};
use crate::tool_loop::{self, Turn};
use crate::tools::Tool;
use serde_json::json;

/// The step name this job's timed rows carry. §3.8 is the section; `narrate` is what a person reads in
/// `requests.tsv` and in the run's per-service summary line (§09 §10).
pub const STEP: &str = "narrate";

/// What one round of the narration conversation came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    /// The answer text: either the entries document itself, or the entries built from the tool calls.
    pub answer: String,
    pub picks: Vec<Picked>,
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

/// One dialled response as the tool loop reads it. Shared by the ordinary leg and S5's resend-without-tools leg
/// so both shapes of answer are read the same way; `last_answer` is kept where it was because only the ordinary
/// leg needs it (the resend replaces the round that failed).
fn turn_of(sent: &crate::server_leg::Sent, last_answer: &mut String) -> Result<Turn, String> {
    let reply = llm_request::reply_from(&sent.content_type, &sent.body)
        .map_err(|why| format!("{STEP}: {why}"))?;
    // `finish` closes the flow, and `tool_loop` reads it off the TURN, not off the call list — so
    // the leg says it here when the writer called it. Detected on the call NAMES rather than by
    // running them, so the closing round is not counted as one that wrote a line.
    let finishing = reply
        .tool_calls
        .iter()
        .any(|call| call.name == Tool::Finish.name());
    let answer = assistant_content(&reply);
    if !answer.is_empty() {
        *last_answer = answer.clone();
    }
    Ok(Turn {
        content: answer,
        calls: reply.tool_calls,
        finish: finishing,
    })
}

/// One round of the narration call, live. `attempt` rides into the log so a retry reads as a retry,
/// and `thinking` is whatever `narrate_call::drive` decided this attempt gets (§F4.2: thinking starts
/// ON and is switched off after a call that wrote nothing) — this module obeys it rather than owning it.
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
    let conf = match settings::from_environment().as_ref().map(settings::read) {
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
    // §2: whether this step gets the web at all, said once with the reason. A machine with no browser is told
    // WHY its job has two fewer tools, and the offer drops them rather than promising a tool nothing answers.
    let offered = crate::llm_web_offer::offer(STEP, &conf.firefox, &offered_tools());
    let (offered, line) = offered;
    if let Some(line) = line {
        crate::ui::window::log_line(&line);
    }

    // Shape of the first round: the two messages, thinking as told, tools offered. Later rounds grow
    // inside the tool loop, which pushes assistant and tool turns onto this same list.
    let messages = vec![
        Message {
            role: "system".into(),
            parts: vec![Part::Text(request.system.clone())],
        },
        Message {
            role: "user".into(),
            parts: vec![Part::Text(request.user.clone())],
        },
    ];

    // The loop's `ask` closure: build the body, dial, read one turn. Each dial writes its own timed
    // row, so a nine-round tool conversation leaves nine lines behind (§09 §10: a line per request).
    let mut requests = 0u32;
    let mut last_answer = String::new();
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<Turn, String> {
        requests += 1;
        let mode = if thinking {
            Mode::Thinking
        } else {
            Mode::Execute
        };
        // S5: a server that already refused the `tools` field is asked without it and nothing more is said.
        let offered = crate::llm_tools_offer::tools_for(&endpoint.url, tools);
        // §4: every chat call is streamed; the reply is read by response content type
        // (`llm_request::reply_from`), so a plain-JSON fake still answers correctly.
        let body = llm_request::body(STEP, &model, conversation, mode, offered.as_deref(), true)?
            .to_string();
        let base = base_row(&model, attempt);
        let reply = crate::llm_retry_leg::dial(
            STEP,
            tree,
            &base,
            &endpoint,
            &body,
            cancelled,
            &crate::ui::window::log_line,
            &crate::llm_retry_leg::nap,
        );
        let sent = match reply.answer {
            Ok(sent) => sent,
            Err(why) => {
                // S5's other half: an error on the loop's FIRST round that is not a stop is read as the server
                // refusing the `tools` field, so the identical request goes out again without it. A refusal
                // already honoured (or a dead server) falls through to the plain failure below.
                if offered.is_some()
                    && requests == 1
                    && crate::llm_tools_offer::honour_refusal(&endpoint.url, &why, 1)
                {
                    crate::ui::window::log_line(&tool_loop::refused_log(STEP, &why));
                    requests += 1;
                    let bare = llm_request::body(STEP, &model, conversation, mode, None, true)?
                        .to_string();
                    let bare_reply = crate::llm_retry_leg::dial(
                        STEP,
                        tree,
                        &base,
                        &endpoint,
                        &bare,
                        cancelled,
                        &crate::ui::window::log_line,
                        &crate::llm_retry_leg::nap,
                    );
                    return bare_reply
                        .answer
                        .map(|sent| turn_of(&sent, &mut last_answer))
                        .map_err(|again| format!("{STEP} call to {model}: {again}"))?;
                }
                return Err(format!("{STEP} call to {model}: {why}"));
            }
        };
        if !sent.ok() {
            // The server's own message decides whether this is a refusal of the `tools` field; §3's wording is
            // what a schema-refusing server answers with, so the leg reads the body beside the status.
            let why = format!(
                "{STEP} call to {model} at {} answered {status}: {}",
                endpoint.url,
                sent.body.trim(),
                status = sent.status
            );
            // A status that says the field is what a schema-refusing server answers with; §1's own verdict on
            // the body decides whether that reads as a refusal or as a bad answer.
            if offered.is_some()
                && requests == 1
                && crate::llm_tools_offer::honour_refusal(&endpoint.url, &why, 1)
            {
                crate::ui::window::log_line(&tool_loop::refused_log(STEP, &why));
                requests += 1;
                let bare =
                    llm_request::body(STEP, &model, conversation, mode, None, true)?.to_string();
                let bare_reply = crate::llm_retry_leg::dial(
                    STEP,
                    tree,
                    &base,
                    &endpoint,
                    &bare,
                    cancelled,
                    &crate::ui::window::log_line,
                    &crate::llm_retry_leg::nap,
                );
                return bare_reply
                    .answer
                    .map(|sent| turn_of(&sent, &mut last_answer))
                    .map_err(|again| format!("{STEP} call to {model}: {again}"))?;
            }
            return Err(why);
        }
        turn_of(&sent, &mut last_answer)
    };

    // The loop's `call` closure: run one tool through the tools that ALREADY answer these (§3.8), and
    // remember the ones that write lines so the entries document can be built at the end.
    let mut collected: Vec<Picked> = Vec::new();
    let mut used_tools = false;
    let mut run_tool = |asked: &crate::exchanges::ToolCall| -> String {
        used_tools = true;
        // §3.1's three reads this leg has no answer of its own for go to the shared dispatcher.
        match asked.name.as_str() {
            "get_context" | "get_events" | "get_frames" => {
                if let Some(answer) = crate::tool_reads::dispatch(
                    &crate::tool_reads::Context {
                        // The User Context lives in the window's session (§10 §2), which this headless
                        // leg does not carry; an empty one answers as an empty field, not an error.
                        user_context: "",
                        job: crate::roles::Job::Narrate,
                        policy: &[],
                        lines: &[],
                        events: &[],
                        frames: &[],
                    },
                    asked,
                ) {
                    return answer;
                }
            }
            _ => {}
        }
        if let Some(answer) = crate::web_browser::dispatch(STEP, &conf.firefox, asked) {
            return answer;
        }
        run_narrate_tool(asked, clips, transcript, narrator, &mut collected)
    };

    // The loop's result is not read: the answer was taken at each dial (`last_answer`) and the tool
    // calls at each dispatch (`collected`), so nothing here needs the finished conversation.
    let run = tool_loop::run(STEP, messages, &offered, &mut ask, &mut run_tool);
    // §2 S2/S4's lines reach the user's log: "round i of 8 — the model asked for …" and the
    // "!!! still calling tools after 8 rounds" refusal are what tells a reader why a step answered nothing,
    // and they existed only in the returned Run before. Same sink every other headless flow writes to.
    for line in run.joined().lines() {
        crate::ui::window::log_line(line);
    }

    if !used_tools {
        return Ok(Round {
            answer: last_answer,
            picks: Vec::new(),
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
    Ok(Round {
        answer: entries_of(&collected, clips)?,
        picks: collected.clone(),
        used_tools: true,
        requests,
    })
}

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
#[derive(Debug, Clone, PartialEq)]
pub struct Picked {
    pub clip: u32,
    pub at: f64,
    pub text: String,
    pub emotion: String,
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
    let args: serde_json::Value = serde_json::from_str(&asked.args).unwrap_or_else(|_| json!({}));
    let number = |key: &str, fallback: f64| -> f64 {
        args.get(key)
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
            })
            .unwrap_or(fallback)
    };
    let text = |key: &str| -> String {
        args.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };

    match asked.name.as_str() {
        "write_line" => {
            let clip = number("clip", 0.0) as u32;
            let at = number("at", 0.0);
            let line = text("text");
            let emotion = text("emotion");
            let pos = text("pos");
            // \u00a712 #23, the half that belongs to the tool: the writer is told NOW whether this line will be
            // squeezed \u{2014} `write_line` answers that itself, in the render's own logged sentence.
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
                collected.push(Picked {
                    clip,
                    at: 0.0,
                    text: String::new(),
                    emotion: String::new(),
                });
            }
            answer
        }
        "list_emotions" => narrate_pass::list_emotions(),
        "get_lines" => {
            // `from`/`to` are seconds relative to the clip the writer is working on; with no clip
            // named there is no anchor to read them against, and saying so is better than assuming 0.
            let Some(index) = (number("clip", 0.0) as usize).checked_sub(1) else {
                return crate::tools::error(
                    "get_lines needs the clip whose surroundings you want -- say \"clip\"",
                );
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

fn entries_of(picked: &[Picked], clips: &[Seg]) -> Result<String, String> {
    let mut ordered: Vec<&Picked> = picked.iter().collect();
    ordered.sort_by(|a, b| a.clip.cmp(&b.clip).then(a.at.total_cmp(&b.at)));
    let entries: Vec<serde_json::Value> = ordered
        .iter()
        .map(|p| {
            let index = (p.clip as usize)
                .checked_sub(1)
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
