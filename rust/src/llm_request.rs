//! §09-llm-and-tools#1-request-contract — what is sent to a chat server and what comes back.

use serde_json::{json, Value};

use crate::checks;
use crate::exchanges::{Message, Mode, Part, ToolCall};
use crate::llm_budget;
use crate::services::{self, Kind};
use crate::tools::Tool;

/// §1's own sentence for the one thing a request cannot do without.
pub const NO_MODEL: &str = "no LLM model configured -- use the gear button";

pub fn model_required(model: &str) -> Result<&str, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err(NO_MODEL.to_string());
    }
    Ok(model)
}

/// Where a chat request goes: `POST <server>/v1/chat/completions`. The path is [`Kind::Chat`]'s, so the table in
/// [`crate::services`] stays the one place that knows which kind takes which path. A server with no URL is the
/// same dead end as no model — nothing to send to.
pub fn chat_url(server: &str) -> Result<String, String> {
    let base =
        services::server_url(server).ok_or_else(|| format!("{server:?}: no server to ask"))?;
    Ok(format!("{base}{}", Kind::Chat.path(None)))
}

pub fn headers(key: &str, streaming: bool) -> Vec<(String, String)> {
    let mut out = vec![("content-type".to_string(), "application/json".to_string())];
    if let Some(bearer) = services::authorization(key) {
        out.push(("authorization".to_string(), bearer));
    }
    if streaming {
        out.push(("accept".to_string(), EVENT_STREAM.to_string()));
    }
    out
}

/// What a streaming response is recognised by, and what the request asks for. Compared by prefix
/// because a server is free to append `; charset=utf-8`.
pub const EVENT_STREAM: &str = "text/event-stream";

// ---- S2: the body -------------------------------------------------------------------

// The sampling four (§1 verbatim): `top_p 0.95, top_k 20, min_p 0, presence_penalty 0`. They are sent whatever
// the mode, because they describe how the model samples rather than how much it is allowed to think.
/// machine.topP — §1's `top_p 0.95`.
pub const TOP_P: f64 = 0.95;
/// machine.topK — §1's `top_k 20`.
pub const TOP_K: u32 = 20;
/// machine.minP — §1's `min_p 0`: no floor under a token's probability, so a rare-but-right word
/// survives.
pub const MIN_P: f64 = 0.0;
/// machine.presencePenalty — §1's `presence_penalty 0`: nothing is discouraged from repeating
/// itself, which matters because the pipeline asks for structured answers that legitimately repeat
/// field names.
pub const PRESENCE_PENALTY: f64 = 0.0;

/// machine.thinkingTemperature — §1's thinking mode runs at full temperature: reasoning that has to
/// reach a surprising connection is hurt by a greedy sample.
pub const THINKING_TEMPERATURE: f64 = 1.0;
/// machine.thinkingMaxTokens — §1's 65536, the ceiling on an answer plus everything thought about
/// it.
pub const THINKING_MAX_TOKENS: u32 = 65536;
/// machine.executeTemperature — §1's execute mode at 0.6: a call that fills in a tool's arguments
/// wants to be dull about it.
pub const EXECUTE_TEMPERATURE: f64 = 0.6;
/// machine.executeMaxTokens — §1's 8192: an executing call answers with tool calls and short text,
/// not prose.
pub const EXECUTE_MAX_TOKENS: u32 = 8192;

pub fn thinking_switch(mode: Mode) -> Value {
    let thinking = matches!(mode, Mode::Thinking);
    json!({
        "enable_thinking": thinking,
        "preserve_thinking": true,
        "chat_template_kwargs": { "enable_thinking": thinking, "preserve_thinking": true },
    })
}

/// A picture sent as a data URL, so the exchange page shows exactly what the model saw without
/// keeping a second copy of the frame ([`crate::exchanges`]'s reason for data URLs).
pub fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", checks::base64(bytes))
}

pub fn message_parts(parts: &[Part]) -> Vec<Value> {
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => json!({ "type": "text", "text": text }),
            Part::Image(url) => json!({ "type": "image_url", "image_url": { "url": url } }),
        })
        .collect()
}

pub fn body(
    step: &str,
    model: &str,
    messages: &[Message],
    mode: Mode,
    tools: Option<&[Tool]>,
    streaming: bool,
) -> Result<Value, String> {
    let model = model_required(model)?;
    // §09 §5: the cap is judged at the one place every chat request is built, so no call site can
    // forget it.
    llm_budget::check(step, llm_budget::chars(messages))?;
    let (temperature, max_tokens) = match mode {
        Mode::Thinking => (THINKING_TEMPERATURE, THINKING_MAX_TOKENS),
        Mode::Execute => (EXECUTE_TEMPERATURE, EXECUTE_MAX_TOKENS),
    };
    let mut body = json!({
        "model": model,
        "messages": messages.iter().map(|message| json!({
            "role": message.role,
            "content": message_parts(&message.parts),
        })).collect::<Vec<Value>>(),
        "top_p": TOP_P,
        "top_k": TOP_K,
        "min_p": MIN_P,
        "presence_penalty": PRESENCE_PENALTY,
        "temperature": temperature,
        "max_tokens": max_tokens,
    });
    // The switch merges in rather than nesting: its two copies sit at the top level and inside
    // `chat_template_kwargs`, not under a key of their own.
    let switch = thinking_switch(mode);
    for (key, value) in switch
        .as_object()
        .expect("thinking_switch builds an object")
    {
        body[key] = value.clone();
    }
    if let Some(tools) = tools.filter(|offered| !offered.is_empty()) {
        body["tools"] = json!(tools
            .iter()
            .map(|tool| json!({
                "type": "function",
                // The name the model calls and §3's arguments: without them a llama.cpp server's
                // grammar allows only `{}` ([`Tool::parameters`]).
                "function": { "name": tool.name(), "parameters": tool.parameters() },
            }))
            .collect::<Vec<Value>>());
    }
    if streaming {
        body["stream"] = json!(true);
    }
    Ok(body)
}

// ---- S3: the response ---------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
}

impl Reply {
    pub fn answer(&self) -> &str {
        &self.content
    }

    /// Whether the model stopped because it ran out of room, which is [`crate::degraded`]'s
    /// question to answer.
    pub fn cut_off(&self) -> bool {
        crate::degraded::cut_off(&self.finish_reason, !self.content.is_empty())
    }
}

/// A field that may be absent, `null`, or the wrong type: read as text, never a panic.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn finish(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(reason)) if !reason.is_empty() => Some(reason.clone()),
        _ => None,
    }
}

pub fn parse_reply(reply: &Value) -> Reply {
    let choice = reply.get("choices").and_then(|c| c.get(0));
    let message = choice.and_then(|c| c.get("message"));
    Reply {
        content: message
            .and_then(|m| m.get("content"))
            .map(text)
            .unwrap_or_default(),
        reasoning: message
            .and_then(|m| m.get("reasoning_content"))
            .map(text)
            .unwrap_or_default(),
        tool_calls: tool_calls(message.and_then(|m| m.get("tool_calls"))),
        finish_reason: finish(choice.and_then(|c| c.get("finish_reason")))
            .or_else(|| finish(reply.get("finish_reason")))
            .unwrap_or_default(),
    }
}

fn tool_calls(value: Option<&Value>) -> Vec<ToolCall> {
    let Some(list) = value.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    list.iter()
        .map(|call| {
            let function = call.get("function");
            ToolCall {
                name: function
                    .and_then(|f| f.get("name"))
                    .map(text)
                    .unwrap_or_default(),
                args: function
                    .and_then(|f| f.get("arguments"))
                    .map(text)
                    .unwrap_or_default(),
                result: String::new(),
            }
        })
        .collect()
}

pub fn is_event_stream(content_type: &str) -> bool {
    content_type
        .trim()
        .to_ascii_lowercase()
        .starts_with(EVENT_STREAM)
}

/// §1's response contract in one door: read a chat reply the way its `content-type` says. Every
/// chat route goes through here rather than parsing the body itself, because a server that streams
/// cannot otherwise be answered at all.
pub fn reply_from(content_type: &str, body: &str) -> Result<Reply, String> {
    if is_event_stream(content_type) {
        let lines: Vec<&str> = body.lines().collect();
        return Ok(from_events(&lines).reply);
    }
    let parsed: Value =
        serde_json::from_str(body).map_err(|_| "the server's answer is not JSON".to_string())?;
    Ok(parse_reply(&parsed))
}

/// What the event stream ended with: `[DONE]`, or the connection closing first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Done,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Streamed {
    pub reply: Reply,
    pub ended: Ended,
}

impl Streamed {
    /// §1's last streaming rule: EOF without `[DONE]` keeps what arrived. The partial is the answer — a run
    /// that lost its tail mid-sentence is dealt with by [`Reply::cut_off`] and the retry flow, not by throwing
    /// away everything that did arrive.
    pub fn complete(&self) -> bool {
        self.ended == Ended::Done
    }
}

pub fn from_events(lines: &[&str]) -> Streamed {
    let mut content = String::new();
    let mut reasoning = String::new();
    // Index order, not arrival order: a chunk may repeat an index after another index has appeared.
    let mut calls: Vec<(usize, ToolCall)> = Vec::new();
    for line in lines {
        let Some(payload) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload == DONE {
            return Streamed {
                reply: assemble(content, reasoning, calls),
                ended: Ended::Done,
            };
        }
        let Ok(chunk) = serde_json::from_str::<Value>(payload) else {
            continue;
        };
        let choice = chunk.get("choices").and_then(|c| c.get(0));
        // The reason is read before the delta: a server's last chunk usually carries the final
        // piece of text and its finish_reason together, and reading it only when no delta came
        // along would lose it.
        let reason = finish(choice.and_then(|c| c.get("finish_reason")));
        let delta = choice.and_then(|c| c.get("delta"));
        let Some(delta) = delta else { continue };
        if let Some(piece) = delta.get("content").and_then(|c| c.as_str()) {
            content.push_str(piece);
        }
        if let Some(piece) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
            reasoning.push_str(piece);
        }
        for call in delta
            .get("tool_calls")
            .and_then(|t| t.as_array())
            .map(|v| v.iter())
            .unwrap_or_default()
        {
            let index = call
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(calls.len() as u64) as usize;
            let slot = match calls.iter_mut().find(|(at, _)| *at == index) {
                Some((_, call)) => call,
                None => {
                    calls.push((
                        index,
                        ToolCall {
                            name: String::new(),
                            args: String::new(),
                            result: String::new(),
                        },
                    ));
                    &mut calls.last_mut().expect("just pushed").1
                }
            };
            let function = call.get("function");
            if let Some(name) = function
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
            {
                if !name.is_empty() {
                    slot.name = name.to_string();
                }
            }
            if let Some(piece) = function
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
            {
                slot.args.push_str(piece);
            }
        }
        if let Some(reason) = reason {
            let mut reply = assemble(content, reasoning, calls.clone());
            reply.finish_reason = reason;
            return Streamed {
                reply,
                ended: Ended::Eof,
            };
        }
    }
    Streamed {
        reply: assemble(content, reasoning, calls),
        ended: Ended::Eof,
    }
}

pub const DONE: &str = "[DONE]";

fn assemble(content: String, reasoning: String, mut calls: Vec<(usize, ToolCall)>) -> Reply {
    // Back in index order for the caller: the wire may interleave two calls, the model asked for
    // them in a row.
    calls.sort_by_key(|(index, _)| *index);
    Reply {
        content,
        reasoning,
        tool_calls: calls.drain(..).map(|(_, call)| call).collect(),
        finish_reason: String::new(),
    }
}

// ---- S4: the cancel context ----------------------------------------------------------

/// §1's last bullet: every request rides the run's cancel context, so ⏹ aborts it.
pub fn rides_cancel() -> bool {
    services::rides_cancel_context(Kind::Chat)
}
