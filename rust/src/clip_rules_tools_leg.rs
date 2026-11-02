//! §2's tool loop for the three per-clip jobs of §3.7 — captions, speed and decorations.

use crate::exchanges::{Message, Part, ToolCall};
use crate::tool_loop;
use crate::tools::clips::Clips;
use crate::tools::Tool;

/// Run one §3.7 ask as a tool conversation.
type Ask<'a> = &'a mut (dyn FnMut(&[Message], &[Tool]) -> Result<tool_loop::Turn, String> + 'a);

/// One answered round: what the model was shown, and its calls with each result already written on them.
#[derive(Clone)]
pub struct Round {
    pub messages: Vec<Message>,
    pub turn: tool_loop::Turn,
}

/// [`run_over_batch`] with the FIRST round's answer already in hand — which is how all three cut passes work:
pub fn run_over_batch_from(
    step: &str,
    first: &tool_loop::Turn,
    messages: Vec<Message>,
    offered: &[Tool],
    batch: &mut Clips,
    ask: Ask<'_>,
) -> Run {
    if offered.is_empty() {
        return Run {
            run: tool_loop::Run {
                messages,
                logs: vec![crate::tools::exhausted_log(step)],
                outcome: tool_loop::Outcome::Finished {
                    asked_finish: false,
                },
                announced: Vec::new(),
            },
            calls: Vec::new(),
            rounds: Vec::new(),
        };
    }
    // Every call the model made, in the order it made them.
    let mut asked: Vec<ToolCall> = Vec::new();
    // Every round as it ran: the conversation at that moment plus the answered calls.
    let mut rounds: Vec<Round> = Vec::new();
    // Whether the round now being asked is the one whose answer the caller already dialled.
    let mut owed = !first.calls.is_empty() || !first.content.trim().is_empty();
    let mut probe = |conversation: &[Message], tools: &[Tool]| -> Result<tool_loop::Turn, String> {
        let mut turn = if owed {
            owed = false;
            first.clone()
        } else {
            ask(conversation, tools)?
        };
        asked.extend(turn.calls.iter().cloned());
        // Remember what this round was SHOWN and what it ANSWERED.
        rounds.push(Round {
            messages: conversation.to_vec(),
            turn: turn.clone(),
        });
        // `finish` is this job's own end signal (§2 S3) and the loop reads it off the Turn, so set it here, at
        // the one place that sees the real calls: a round that called `finish` beside its work tool (exactly how
        // a model closes an `add_caption` answer) ENDED the job.
        turn.finish = turn.finish
            || turn
                .calls
                .iter()
                .any(|call| call.name == Tool::Finish.name());
        Ok(turn)
    };
    let run = tool_loop::run(step, messages, offered, &mut probe, |call| {
        answer(call, batch)
    });
    // Write each round's ANSWERED calls back into the round that made them.
    let assistants: Vec<usize> = run
        .messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == "assistant")
        .map(|(at, _)| at)
        .collect();
    let last = rounds.len();
    for (index, round) in rounds.iter_mut().enumerate() {
        let shown = round.messages.len();
        // This round's own turn: the first assistant turn that arrived after it was asked.
        let mine = assistants
            .iter()
            .copied()
            .filter(|at| *at >= shown)
            .min()
            .unwrap_or(usize::MAX);
        let end = assistants
            .iter()
            .copied()
            .filter(|at| *at > mine)
            .min()
            .unwrap_or(run.messages.len());
        if mine == usize::MAX || mine >= run.messages.len() {
            continue;
        }
        let window = &run.messages[mine..end.min(run.messages.len())];
        if let Some(Part::Text(spoken)) = window.first().and_then(|turn| turn.parts.first()) {
            round.turn.content = spoken.clone();
        }
        let mut answers = window
            .iter()
            .filter(|message| message.role == "tool")
            .filter_map(|message| match message.parts.first() {
                Some(Part::Text(text)) => Some(text.clone()),
                _ => None,
            });
        for call in &mut round.turn.calls {
            if let Some(result) = answers.next() {
                call.result = result;
            }
        }
        round.turn.finish = round
            .turn
            .calls
            .iter()
            .any(|call| call.name == Tool::Finish.name())
            || (round.turn.calls.is_empty() && index + 1 == last);
    }
    Run {
        run,
        calls: asked,
        rounds,
    }
}

/// Answer a whole round's calls against the batch and return the §7 notes for them, in order — the lines a step
/// logs so a refusal (a rate over a captioned clip, §F3.10 S2) is SAID rather than quietly dropped.
pub fn answer_calls(calls: &[ToolCall], batch: &mut Clips) -> Vec<String> {
    calls.iter().map(|call| call_note_of(call, batch)).collect()
}

/// One answered call as §7 shows it.
fn call_note_of(call: &ToolCall, batch: &mut Clips) -> String {
    let mut done = call.clone();
    done.result = answer(call, batch);
    tool_loop::call_note(&done)
}

/// A finished §3.7 conversation: the loop's own record, plus the calls it saw and the rounds it ran.
pub struct Run {
    pub run: tool_loop::Run,
    /// Every call the model made, reads included, in the order it made them.
    pub calls: Vec<ToolCall>,
    /// One entry per request the loop made, each holding the conversation that round was shown and its ANSWERED
    /// calls.
    pub rounds: Vec<Round>,
}

impl Run {
    /// The model's last spoken text — the prose answer of a round that made no calls, the shape F3.9, F3.10 and
    /// F3.11 were written against.
    pub fn prose(&self) -> Option<String> {
        self.run
            .messages
            .iter()
            .filter(|message| message.role == "assistant")
            .filter_map(|message| {
                message.parts.iter().find_map(|part| match part {
                    crate::exchanges::Part::Text(text) if !text.trim().is_empty() => {
                        Some(text.clone())
                    }
                    _ => None,
                })
            })
            .next_back()
    }

    /// This job's own calls only, in the order asked: a `get_frames` read is not an answer, and the passes count
    /// what was asked for, not what was looked at.
    pub fn job_calls(&self, job: &[Tool]) -> Vec<ToolCall> {
        self.calls
            .iter()
            .filter(|call| job.iter().any(|tool| tool.name() == call.name))
            .cloned()
            .collect()
    }

    /// Whether the loop ended by running out of rounds (§2 S4) rather than by an answer.
    pub fn exhausted(&self) -> bool {
        self.run.outcome == tool_loop::Outcome::Exhausted
    }
}

/// Answer one §3.7 call against the batch: the read first, then this job's three tools, then `finish`.
pub fn answer(call: &ToolCall, batch: &mut Clips) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&call.args).unwrap_or_else(|_| serde_json::json!({}));
    let number = |key: &str| {
        args.get(key).and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|t| t.trim().parse::<f64>().ok()))
        })
    };
    match call.name.as_str() {
        // §3.7's read: the clip's frames, so a zoom can be aimed.
        "get_frames" => frames_of(batch, number("clip").map(|n| n as u32), number("at")),
        "add_caption" => match (
            number("clip").map(|n| n as u32),
            number("start"),
            number("end"),
            args.get("text").and_then(|text| text.as_str()),
        ) {
            (Some(clip), Some(start), Some(end), Some(text)) => {
                batch.add_caption(clip, start, end, text)
            }
            _ => crate::tools::error("add_caption needs \"clip\", \"start\", \"end\" and \"text\""),
        },
        "set_clip_speed" => match (number("clip").map(|n| n as u32), number("rate")) {
            (Some(clip), Some(rate)) => batch.set_clip_speed(clip, rate),
            _ => crate::tools::error("set_clip_speed needs \"clip\" and \"rate\""),
        },
        "add_effect" => match (
            number("clip").map(|n| n as u32),
            args.get("kind").and_then(|kind| kind.as_str()),
            number("start"),
            number("end"),
        ) {
            (Some(clip), Some(kind), Some(start), Some(end)) => {
                // An omitted gain and a 1 are the same ask (§F3.11); an explicit 0 is silence and must survive.
                let gain = number("gain").filter(|value| *value != 1.0);
                batch.add_effect(clip, kind, start, end, gain, None)
            }
            _ => crate::tools::error("add_effect needs \"clip\", \"kind\", \"start\" and \"end\""),
        },
        // §3.7's `finish` for these three jobs IS the batch, which is what the caller reads back off `Clips`;
        // saying so in words tells the model the conversation is over rather than leaving it guessing.
        "finish" => "{\"finished\":true}".to_string(),
        _ => crate::tools::error(&format!("{} is not one of this job's tools", call.name)),
    }
}

/// `get_frames` over a clip batch: the stamped offsets of one clip, or of every clip when none is named.
fn frames_of(batch: &Clips, clip: Option<u32>, at: Option<f64>) -> String {
    let wanted: Vec<u32> = match clip {
        Some(clip) => vec![clip],
        None => batch.clips().iter().map(|candidate| candidate.n).collect(),
    };
    let mut frames: Vec<serde_json::Value> = Vec::new();
    for number in wanted {
        for frame in batch.clip_frames(number, at) {
            frames.push(serde_json::json!({ "clip": frame.clip, "at": frame.at }));
        }
    }
    if frames.is_empty() {
        return crate::tools::error(&match clip {
            Some(clip) => format!("clip {clip} is not in this batch"),
            None => "this batch has no clips to show".to_string(),
        });
    }
    serde_json::json!({ "frames": frames }).to_string()
}

/// §3's reading of these three calls, as answered in `spec/QUESTIONS.md`: the tools ARE on the wire, and
/// "a reply that only carries a JSON document instead of tool calls `is` unreadable, asking again".
pub fn no_calls_problem(tool_name: &str) -> String {
    format!("the answer is a JSON document, not a tool call -- call `{tool_name}` (or `finish`) instead")
}

/// The failure a person reads when even the retry answered in prose.
pub fn no_calls_reason(step: &str, tool_name: &str) -> String {
    format!("{step}: no tool call in the answer -- asked again and got a JSON document instead of `{tool_name}`")
}

/// The log sink `ask_repair_after` will take.
fn log_through<'a>(log: &'a (dyn Fn(&str) + 'a)) -> Box<dyn Fn(&str) + 'a> {
    Box::new(move |line: &str| log(line))
}

/// Ask once more, and only once, for a tool call.
#[allow(clippy::too_many_arguments)] // three closures, each owned by the caller's own wire shape
#[allow(clippy::type_complexity)] // the two closures ARE the caller's wire shape; naming them elsewhere helps nothing
pub fn ask_again_for_calls(
    step: &str,
    tool_name: &str,
    messages: Vec<crate::exchanges::Message>,
    first: &crate::llm_request::Reply,
    log: &dyn Fn(&str),
    build: &dyn Fn(&[crate::exchanges::Message]) -> Result<String, String>,
    dial: &dyn Fn(&str) -> Result<(String, String), String>,
) -> Result<crate::llm_request::Reply, String> {
    // `retry_turn`'s `tools_active` is false on purpose: the model made NO call, so it is not mid-round and the
    // correction arrives as a `user` turn, which is what a model that answered with a document expects.
    let verdict = |again: &crate::llm_request::Reply| {
        again
            .tool_calls
            .is_empty()
            .then(|| no_calls_problem(tool_name))
    };
    let repaired = crate::llm_repair_leg::ask_repair_after(
        step,
        &log_through(log),
        messages,
        false,
        false,
        &verdict,
        first,
        dial,
        &|messages, _thinking| build(messages),
    )?;
    let reply = repaired.reply()?;
    if reply.tool_calls.is_empty() {
        return Err(no_calls_reason(step, tool_name));
    }
    Ok(reply)
}

/// The chat envelope a round answered with, rebuilt for the cache.
pub fn answering_envelope(reply: &crate::llm_request::Reply) -> serde_json::Value {
    let said: Vec<String> = reply
        .tool_calls
        .iter()
        .map(|call| format!("{}({})", call.name, call.args))
        .collect();
    serde_json::json!({
        "choices": [{ "message": {
            "content": said.join(" "),
            "tool_calls": reply.tool_calls.iter().map(|call| serde_json::json!({
                "function": { "name": call.name, "arguments": call.args }
            })).collect::<Vec<_>>(),
        } }]
    })
}

/// Whether an answer carries any work call at all -- `finish` and a read (`get_frames`) aside.
pub fn asks_for_anything(reply: &crate::llm_request::Reply) -> bool {
    reply
        .tool_calls
        .iter()
        .any(|call| call.name != Tool::Finish.name() && call.name != Tool::GetFrames.name())
}

/// The one timed row `llm_retry_leg::chat` would have written for a request an ask sent WITHOUT riding that
/// door -- which is exactly the ask-again round: the repair leg owns how the retry is built and dialled, so it
/// cannot also go through the patience ladder, and §09 §10 still wants the request on the record ("every request
/// to any of them is timed and written to the project's `requests.tsv`"; a refused answer is still a request the
/// run paid for).
pub fn record_repair_row(
    tree: &crate::layout::Tree,
    mut base: crate::requests::Request,
    went_out: std::time::Instant,
) {
    let sent_ms = crate::server_leg::epoch_ms();
    let mut timer =
        crate::request_timing::Timer::sent(sent_ms - went_out.elapsed().as_millis() as i64);
    timer.finished(sent_ms);
    crate::request_timing::stamp_run(&mut base);
    let row = crate::request_timing::finish_row(&base, &timer, crate::requests::Outcome::Ok, 1);
    let _ = crate::requests::record(tree, &row);
}

/// The batch a §3.7 ask was built about, in the shape [`Clips`] wants.
pub fn batch_of(clips: &[(u32, f64)]) -> Clips {
    Clips::new(
        &clips
            .iter()
            .map(|(n, length)| (*n, 0.0, *length))
            .collect::<Vec<_>>(),
    )
}
