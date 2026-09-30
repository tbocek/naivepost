//! F5.6 S2: the real `youtube` call — §3.9's six tools answered by the rules that already exist.
//!
//! [`crate::produce_upload`] holds each tool's RULE (what a title fits, how far a picked frame really is,
//! what `finish` still needs). This module is the wire around them: it builds the request the way §3.9 and
//! §02-services#1 say (shipped `youtube` prompt as the system message, thinking ON, the job's tools
//! offered), dials the LLM through [`crate::server_leg`] with the settings re-read per round, runs the
//! conversation through [`crate::tool_loop`], and answers every call by CALLING its rule and returning the
//! result text §3.9 promises. The calls are then collected into [`upload::Answers`]; a model that used no
//! tools at all falls back to [`upload::peel`], which is §3.9's documented prose shape.

use crate::exchanges::{Message, Mode, Part, ToolCall};
use crate::layout::Tree;
use crate::llm_request;
use crate::produce_details::FrameAt;
use crate::produce_upload as upload;
use crate::project::Publish;
use crate::roles::Job;
use crate::server_leg;
use crate::services::{self, Kind, Server};
use crate::settings::{self, Conf, Paths};
use crate::tool_loop;
use crate::tools::Tool;

/// The step name on this call's rows in `requests.tsv` (§09 §10: one line per request).
pub const STEP: &str = "youtube";

/// What the call collected: the record the model would have written, plus what it said back.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Collected {
    /// The publish record as the tools left it — what `walk` then writes (S5) and draws from.
    pub publish: Publish,
    /// Whether any write tool ran at all. Prose-only replies leave this false, and the caller peels instead.
    pub wrote: bool,
    /// Every tool's answer text, in the order it was given, so a test can pin §3.9's wording.
    pub answers: Vec<String>,
}

/// The wording to ask with, through the settings read path so an edit here wins (§04-prepare#6). With no
/// settings folder at all, the shipped `spec/prompts/youtube.md` is what goes out.
fn wording(paths: Option<&Paths>, key: &str) -> String {
    let fallback = crate::join_ask::shipped_file(key).unwrap_or_default();
    match paths {
        Some(paths) => settings::prompt_text(paths, key, &fallback).unwrap_or(fallback),
        None => fallback,
    }
}

/// The SYSTEM for the upload call: the house rules cut to this job's section plus the shipped `youtube`
/// prompt. No User Context — §3.9's material is the brief, which already says what is in the video.
pub fn system(paths: Option<&Paths>) -> String {
    crate::prompt_assembly::system_message(
        &wording(paths, "system"),
        "youtube",
        &wording(paths, "youtube"),
        "",
        false,
        false,
    )
}

/// The two messages of one upload call: the assembled system prompt and the brief as the user turn.
pub fn messages(system: &str, brief: &str) -> Vec<Message> {
    vec![
        Message { role: "system".to_string(), parts: vec![Part::Text(system.to_string())] },
        Message { role: "user".to_string(), parts: vec![Part::Text(brief.to_string())] },
    ]
}

/// The body of one round: thinking ON (§F5.6 S2, and `prompts::THINKING_ON` lists `youtube`) and the job's
/// tools offered. `llm_request::body` is the one place the switch and the token cap are written.
pub fn body(model: &str, system: &str, brief: &str) -> Result<String, String> {
    let tools = crate::tools::offered(Job::UploadText);
    let body = llm_request::body(
        model,
        &messages(system, brief),
        Mode::Thinking,
        Some(&tools),
        false,
    )?;
    Ok(body.to_string())
}

/// The frames this call may choose from, as produced-clock moments, plus where each clip begins.
pub struct Frames<'a> {
    /// Every extracted frame on the produced clock, sorted.
    pub moments: &'a [f64],
    /// Each kept clip's start on the produced clock, 1-based `clip` numbers index into this.
    pub starts: &'a [f64],
}

/// Run one `youtube` call over the brief and answer its tools. Returns what the record looks like after the
/// conversation; `Err` names the model and the reason when the call itself did not happen (§Failure is
/// specific), which the caller reports as `words_failed` rather than writing anything.
pub fn call<FLog>(
    tree: &Tree,
    brief: &str,
    publish: &Publish,
    frames: &Frames,
    edited_by_hand: bool,
    cancelled: &dyn Fn() -> bool,
    mut log: FLog,
) -> Result<Collected, String>
where
    FLog: FnMut(&str),
{
    let paths = settings::from_environment();
    let system = system(paths.as_ref());
    let mut state = State {
        publish: publish.clone(),
        wrote: false,
        answers: Vec::new(),
        frames_moments: frames.moments.to_vec(),
        starts: frames.starts.to_vec(),
        edited_by_hand,
    };
    let offered = crate::tools::offered(Job::UploadText);
    // The first dial error, kept so a call that never got an answer can name the model and the reason
    // instead of reporting an empty success.
    let mut dial_error: Option<String> = None;
    // The prose the model spoke on a round that called no tool. `tool_loop` only records an assistant turn when
    // that round carried calls, so a §3.9 prose answer (three labelled lines, no calls) would otherwise be
    // heard by nobody but this closure — and it is the whole of what the record gets.
    let mut spoken: String = String::new();
    let mut conversation = messages(&system, brief);
    // One dial per round, each writing its own timed row: a five-round conversation leaves five lines behind.
    let mut ask = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        // Settings are re-read per round so an address edited mid-call lands on the next request (§1).
        let conf = match paths.as_ref().map(settings::read) {
            Some(Ok(conf)) => conf,
            Some(Err(reason)) => {
                let why = format!("settings could not be read: {reason}");
                dial_error.get_or_insert(why.clone());
                return Err(why);
            }
            None => Conf::default(),
        };
        let model = conf.model.clone();
        let mode = if crate::prompts::thinking("youtube") { Mode::Thinking } else { Mode::Execute };
        let sent_body = match llm_request::body(&model, conversation, mode, Some(tools), false) {
            Ok(sent_body) => sent_body.to_string(),
            Err(why) => {
                dial_error.get_or_insert(format!("{STEP} call to {model}: {why}"));
                return Err(format!("{STEP} call to {model}: {why}"));
            }
        };
        let endpoint = services::llm_endpoint(&conf);
        let base = server_leg::base_row(Server::Llm, &model, "chat", "", STEP, "upload");
        let (_row, reply) = server_leg::call(
            tree,
            &base,
            Server::Llm,
            &endpoint,
            Kind::Chat,
            None,
            Some(&sent_body),
            cancelled,
        );
        let sent = match reply {
            Ok(sent) => sent,
            Err(why) => {
                let why = format!("{STEP} call to {model}: {why}");
                dial_error.get_or_insert(why.clone());
                return Err(why);
            }
        };
        if !sent.ok() {
            let why = format!(
                "{STEP} call to {model} at {} answered {status}",
                endpoint.url,
                status = sent.status
            );
            dial_error.get_or_insert(why.clone());
            return Err(why);
        }
        let parsed: serde_json::Value = match serde_json::from_str(&sent.body) {
            Ok(parsed) => parsed,
            Err(_) => {
                let why = format!("{STEP}: the server's answer is not JSON");
                dial_error.get_or_insert(why.clone());
                return Err(why);
            }
        };
        let reply = llm_request::parse_reply(&parsed);
        let finishing = reply.tool_calls.iter().any(|call| call.name == Tool::Finish.name());
        let turn = tool_loop::Turn {
            content: reply.answer().to_string(),
            calls: reply.tool_calls.clone(),
            finish: finishing,
        };
        // The last quiet round wins; a round that said nothing alongside its calls does not clear what was said.
        if !turn.content.trim().is_empty() {
            spoken = turn.content.clone();
        }
        Ok(turn)
    };
    let mut run_tool = |asked: &ToolCall| -> String {
        let answer = state.run_tool(asked);
        if answer.is_empty() {
            "ok".to_string()
        } else {
            answer
        }
    };
    let run = tool_loop::run(STEP, std::mem::take(&mut conversation), &offered, &mut ask, &mut run_tool);
    for line in run.joined().lines() {
        log(line);
    }
    // A round that could not be made is the call's own failure, named: §Failure is specific says which
    // server and why, and the caller writes nothing (S5 keeps the thinking only for a FAILED DRAW).
    if let tool_loop::Outcome::Exhausted = run.outcome {
        // `take` rather than `is_some()` + `expect`: the loop only logs a failure when its ask returned one.
        if state.answers.is_empty() {
            if let Some(why) = dial_error.take() {
                return Err(why);
            }
        }
    }
    // The prose a round spoke but no tool wrote: kept on `spoken` because a quiet round's text never enters the
    // loop's messages (see above). Only when nothing was heard at all falls back to reading the conversation.
    if state.answers.is_empty() {
        let text = if spoken.trim().is_empty() { state.content_of(&run).unwrap_or_default() } else { spoken };
        state.wrote |= peel_into(&mut state.publish, &text);
    }
    Ok(Collected {
        publish: state.publish,
        wrote: state.wrote,
        answers: state.answers,
    })
}

/// Everything the tools act on, held across the rounds: the record being written and the frames to choose from.
struct State {
    publish: Publish,
    wrote: bool,
    answers: Vec<String>,
    frames_moments: Vec<f64>,
    starts: Vec<f64>,
    edited_by_hand: bool,
}

impl State {
    /// Answer one call by running its rule. Every arm returns §3.9's promised RESULT TEXT, because §5 asks
    /// each write to carry more than `ok` and the model has to see what was actually applied.
    fn run_tool(&mut self, asked: &ToolCall) -> String {
        let args: serde_json::Value =
            serde_json::from_str(&asked.args).unwrap_or(serde_json::Value::Null);
        let text = || string_of(&args, "text");
        match asked.name.as_str() {
            n if n == Tool::SetTitle.name() => {
                let box_ = self.publish.title_box.unwrap_or_default();
                let answer = upload::set_title(&self.publish, &text(), &box_, self.edited_by_hand);
                self.publish.thumb_title = text();
                self.publish.title = text();
                self.wrote = true;
                self.answers.push(answer.answer.clone());
                answer.answer
            }
            n if n == Tool::SetDescription.name() => {
                let answer = upload::set_description(&self.publish, &text());
                self.publish.description = text();
                self.wrote = true;
                self.answers.push(answer.clone());
                answer
            }
            n if n == Tool::PickFrame.name() => {
                let clip = usize_of(&args, "clip").unwrap_or(0);
                let offset = number_of(&args, "offset").unwrap_or(0.0);
                match upload::pick_frame(clip, offset, &self.starts, &self.frames_moments) {
                    Ok(picked) => {
                        // S3: the frame IS the thumbnail — one entry in the row, `own` true, nothing drawn.
                        self.publish.own = true;
                        self.publish.frames = vec![format!("frame:{:.1}", picked.taken)];
                        self.wrote = true;
                        self.answers.push(picked.answer.clone());
                        picked.answer
                    }
                    Err(why) => {
                        self.answers.push(why.clone());
                        why
                    }
                }
            }
            n if n == Tool::SetThumbnailInstruction.name() => {
                let negative = string_of(&args, "negative");
                let negative = if negative.is_empty() { None } else { Some(negative.as_str()) };
                match upload::set_thumbnail_instruction(self.publish.own, &text(), negative) {
                    Ok(answer) => {
                        self.publish.prompt = text();
                        if let Some(neg) = negative {
                            self.publish.negative = neg.to_string();
                        }
                        self.wrote = true;
                        self.answers.push(answer.clone());
                        answer
                    }
                    Err(why) => {
                        self.answers.push(why.clone());
                        why
                    }
                }
            }
            n if n == Tool::GetFrames.name() => {
                let clip = usize_of(&args, "clip").unwrap_or(0);
                let at = number_of(&args, "at");
                let span = clip_span(&self.starts, clip);
                let candidates =
                    crate::produce_upload_frames::get_frames(&self.frames_moments, span.0, span.1, at);
                let answer = format!(
                    "{} candidate frame(s) in clip {clip}: {}",
                    candidates.len(),
                    candidates
                        .iter()
                        .map(|at| format!("{:.1}", at))
                        .collect::<Vec<String>>()
                        .join(", ")
                );
                self.answers.push(answer.clone());
                answer
            }
            n if n == Tool::Finish.name() => {
                let missing = upload::finish_needs(&self.publish.thumb_title, &self.publish.description);
                let answer = if missing.is_empty() {
                    "finished".to_string()
                } else {
                    // S8 keeps the previous value for what is missing, so `finish` NAMES it rather than
                    // quietly shipping last run's words under this video.
                    format!("still missing: {}", missing.join(", "))
                };
                self.answers.push(answer.clone());
                answer
            }
            other => format!("no such tool for this job: {other}"),
        }
    }

    // The model's own words, if it left any on a round that called tools (the prose fallback's other material).
    // A quiet round's text is NOT here — the loop only records turns that carried calls — which is why `call`
    // keeps what it heard in its own `spoken`.
    fn content_of(&self, run: &tool_loop::Run) -> Option<String> {
        let text = run
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .map(|m| {
                m.parts
                    .iter()
                    .map(|part| match part {
                        Part::Text(text) => text.as_str(),
                        Part::Image(_) => "",
                    })
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
            .join("\n");
        (!text.trim().is_empty()).then_some(text)
    }
}

/// A clip's `(start, end)` on the produced clock; an unknown clip reads as the whole video rather than
/// nothing, so `get_frames` still shows the model something to look at.
fn clip_span(starts: &[f64], clip: usize) -> (f64, f64) {
    let Some(index) = clip.checked_sub(1) else {
        return (0.0, f64::MAX);
    };
    let Some(&start) = starts.get(index) else {
        return (0.0, f64::MAX);
    };
    let end = starts.get(index + 1).copied().unwrap_or(f64::MAX);
    (start, end)
}

/// Apply §3.9's prose fallback to the record: `TITLE:`/`THUMBNAIL:`/description off the front of the reply.
/// Returns whether anything landed.
fn peel_into(publish: &mut Publish, reply: &str) -> bool {
    let (title, thumbnail, description) = upload::peel(reply);
    let mut wrote = false;
    if let Some(title) = title {
        publish.thumb_title = title.clone();
        publish.title = title;
        wrote = true;
    }
    if let Some(thumbnail) = thumbnail {
        // A `frame:` inside the thumbnail line picks a frame; anything else is an instruction (§3.9's
        // folding rule lives in `details`, and a frame still beats an instruction here).
        match crate::produce_details::frame_line(thumbnail.trim()) {
            Some(FrameAt::Clip(clip, offset)) => {
                if let Ok(picked) = upload::pick_frame(clip, offset, &[], &[]) {
                    publish.own = true;
                    publish.frames = vec![format!("frame:{:.1}", picked.taken)];
                }
            }
            Some(FrameAt::Seconds(at)) => {
                publish.own = true;
                publish.frames = vec![format!("frame:{at:.1}")];
            }
            None => publish.prompt = thumbnail,
        }
        wrote = true;
    }
    if let Some(description) = description {
        publish.description = description;
        wrote = true;
    }
    wrote
}

fn string_of(args: &serde_json::Value, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// A numeric argument that may arrive as a number or as a quoted number — servers do both.
fn number_of(args: &serde_json::Value, key: &str) -> Option<f64> {
    let value = args.get(key)?;
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    value.as_str()?.trim().parse().ok()
}

/// A counting argument (`clip`), same two shapes as [`number_of`] but read as an index.
fn usize_of(args: &serde_json::Value, key: &str) -> Option<usize> {
    number_of(args, key).map(|number| number.max(0.0) as usize)
}
