//! §09-llm-and-tools#1-request-contract — what is sent to a chat server and what comes back.
//!
//! Spec: `spec/09-llm-and-tools.md` §1. Its four bullets are read here as S1..S4 (where it goes and what it
//! must carry; the body's shape; the response and its streaming form; the cancel context), one section each.
//!
//! This is the contract, not a client: nothing here opens a socket. The app builds a request with [`body`],
//! reads a reply with [`parse_reply`] or [`from_events`], and hands both to whoever owns the wire — which is
//! what lets every rule below be tested without a server answering.
//!
//! # Why the numbers are here
//!
//! §1 names eight values and §10 gives none of them a `P.` row: they are fixed in code, listed under "the rest
//! … LLM tokens 65536/8192" as consequences of the servers rather than settings. Each therefore carries a bare
//! `machine.` id (§00's rule for a value §10 does not name) and lives once, below.

use serde_json::{json, Value};

use crate::checks;
use crate::exchanges::{Message, Mode, Part, ToolCall};
use crate::services::{self, Kind};
use crate::tools::Tool;

/// §1's own sentence for the one thing a request cannot do without. Two wordings of this exist in the app —
/// [`crate::checks::llm_model_listed`] answers the settings dialog's "Fetch models and pick one" — and both are
/// kept because they are asked at different moments: this one fires when a step is about to run with nothing
/// chosen, that one when a person checks what the server serves.
pub const NO_MODEL: &str = "no LLM model configured -- use the gear button";

/// There is no default model id to fall back to and none is invented here: guessing one would send a request
/// that either fails on someone else's machine or, worse, answers from a model nobody chose.
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
    let base = services::server_url(server).ok_or_else(|| format!("{server:?}: no server to ask"))?;
    Ok(format!("{base}{}", Kind::Chat.path(None)))
}

/// The two headers every chat request carries, and the third a streaming caller adds.
///
/// The key rides only when there is one: [`services::authorization`] answers `None` for an empty string because
/// a bearer token that says "none" is read as a bad request by some servers. `accept` is sent only when the
/// caller will read an event stream, so a non-streaming call is never handed `text/event-stream` it would have
/// to un-frame for nothing.
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

/// What a streaming response is recognised by, and what the request asks for. Compared by prefix because a
/// server is free to append `; charset=utf-8`.
pub const EVENT_STREAM: &str = "text/event-stream";

// ---- S2: the body -------------------------------------------------------------------

// The sampling four (§1 verbatim): `top_p 0.95, top_k 20, min_p 0, presence_penalty 0`. They are sent whatever
// the mode, because they describe how the model samples rather than how much it is allowed to think.
/// machine.topP — §1's `top_p 0.95`.
pub const TOP_P: f64 = 0.95;
/// machine.topK — §1's `top_k 20`.
pub const TOP_K: u32 = 20;
/// machine.minP — §1's `min_p 0`: no floor under a token's probability, so a rare-but-right word survives.
pub const MIN_P: f64 = 0.0;
/// machine.presencePenalty — §1's `presence_penalty 0`: nothing is discouraged from repeating itself, which
/// matters because the pipeline asks for structured answers that legitimately repeat field names.
pub const PRESENCE_PENALTY: f64 = 0.0;

/// machine.thinkingTemperature — §1's thinking mode runs at full temperature: reasoning that has to reach a
/// surprising connection is hurt by a greedy sample.
pub const THINKING_TEMPERATURE: f64 = 1.0;
/// machine.thinkingMaxTokens — §1's 65536, the ceiling on an answer plus everything thought about it.
pub const THINKING_MAX_TOKENS: u32 = 65536;
/// machine.executeTemperature — §1's execute mode at 0.6: a call that fills in a tool's arguments wants to be
/// dull about it.
pub const EXECUTE_TEMPERATURE: f64 = 0.6;
/// machine.executeMaxTokens — §1's 8192: an executing call answers with tool calls and short text, not prose.
pub const EXECUTE_MAX_TOKENS: u32 = 8192;

/// The thinking switch in the two places servers read it, plus `preserve_thinking true`.
///
/// §1's reason verbatim: llama.cpp reads one, other servers the other — so a request that sets only the
/// top-level flag silently thinks (or stops thinking) on half the installs this app runs on. Both copies always
/// agree; [`crate::checks::llm_test_body`] sends them the same way for the settings dialog's test call.
pub fn thinking_switch(mode: Mode) -> Value {
    let thinking = matches!(mode, Mode::Thinking);
    json!({
        "enable_thinking": thinking,
        "preserve_thinking": true,
        "chat_template_kwargs": { "enable_thinking": thinking, "preserve_thinking": true },
    })
}

/// A picture sent as a data URL, so the exchange page shows exactly what the model saw without keeping a second
/// copy of the frame ([`crate::exchanges`]'s reason for data URLs). The base64 is [`checks`]' standard-alphabet
/// one with padding — the same encoder the vision probe uses, so a picture and a probe are not two dialects.
pub fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", checks::base64(bytes))
}

/// One message's parts as the wire wants them: text, or `image_url` wrapping a data URL.
pub fn message_parts(parts: &[Part]) -> Vec<Value> {
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => json!({ "type": "text", "text": text }),
            // A stored image is already a data URL (that is what `Part::Image` holds), so it travels as it is.
            Part::Image(url) => json!({ "type": "image_url", "image_url": { "url": url } }),
        })
        .collect()
}

/// The whole request: model, messages, the sampling four, the mode's temperature / `max_tokens` / thinking
/// switch (twice), `tools` when offered, and `stream` when the caller streams.
///
/// `tools` is omitted rather than sent empty: an empty array makes some servers advertise no tools at all in
/// their reply, which reads as "the model declined" rather than "nothing was offered". An empty `model` is
/// [`NO_MODEL`], checked here rather than at every call site.
pub fn body(
    model: &str,
    messages: &[Message],
    mode: Mode,
    tools: Option<&[Tool]>,
    streaming: bool,
) -> Result<Value, String> {
    let model = model_required(model)?;
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
    for (key, value) in switch.as_object().expect("thinking_switch builds an object") {
        body[key] = value.clone();
    }
    if let Some(tools) = tools.filter(|offered| !offered.is_empty()) {
        body["tools"] = json!(tools
            .iter()
            .map(|tool| json!({
                "type": "function",
                // §2 owns each tool's parameter schema; §1 only asks that the offer is sent when there is one,
                // so this carries the name the model calls and nothing invented about its arguments.
                "function": { "name": tool.name() },
            }))
            .collect::<Vec<Value>>());
    }
    // `stream` is sent only when true. Absent means false to every server in §1's table, and a request that
    // spells `"stream": false` is a request whose intent a reader has to look up.
    if streaming {
        body["stream"] = json!(true);
    }
    Ok(body)
}

// ---- S3: the response ---------------------------------------------------------------

/// What came back, with the model's reasoning kept in its own field.
#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub content: String,
    /// `reasoning_content`, held apart from the answer.
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
}

impl Reply {
    /// The answer. Never [`Reply::reasoning`]: a model's reasoning is prose about the task, and returning it as
    /// the answer would have the app parsing its own scratchpad — the failure mode §1's "never returned as the
    /// answer" exists to stop.
    pub fn answer(&self) -> &str {
        &self.content
    }

    /// Whether the model stopped because it ran out of room, which is [`crate::degraded`]'s question to answer.
    pub fn cut_off(&self) -> bool {
        crate::degraded::cut_off(&self.finish_reason, !self.content.is_empty())
    }
}

/// A field that may be absent, `null`, or the wrong type: read as text, never a panic. A server that answers
/// `"content": null` has answered, and taking the run down over its spelling would turn one bad reply into a
/// lost session.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A finish reason, or nothing. `null` is a chunk that has not finished rather than a reason, so it reads as
/// empty where [`text`] would otherwise hand back the word "null".
fn finish(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(reason)) if !reason.is_empty() => Some(reason.clone()),
        _ => None,
    }
}

/// `choices[0].message.{content, reasoning_content, tool_calls}` and `finish_reason`.
///
/// The finish reason is read off the choice first and off the object's top level second: §1 lists it among the
/// message-level fields, and servers have been seen to put it at either place — an empty answer with no reason
/// would otherwise look like a refusal rather than a truncation.
pub fn parse_reply(reply: &Value) -> Reply {
    let choice = reply.get("choices").and_then(|c| c.get(0));
    let message = choice.and_then(|c| c.get("message"));
    Reply {
        content: message.and_then(|m| m.get("content")).map(text).unwrap_or_default(),
        reasoning: message.and_then(|m| m.get("reasoning_content")).map(text).unwrap_or_default(),
        tool_calls: tool_calls(message.and_then(|m| m.get("tool_calls"))),
        finish_reason: finish(choice.and_then(|c| c.get("finish_reason")))
            .or_else(|| finish(reply.get("finish_reason")))
            .unwrap_or_default(),
    }
}

/// A finished `tool_calls` array. The result field is empty here: it is filled when the app answers the call.
fn tool_calls(value: Option<&Value>) -> Vec<ToolCall> {
    let Some(list) = value.and_then(|v| v.as_array()) else { return Vec::new(); };
    list.iter()
        .map(|call| {
            let function = call.get("function");
            ToolCall {
                name: function.and_then(|f| f.get("name")).map(text).unwrap_or_default(),
                args: function.and_then(|f| f.get("arguments")).map(text).unwrap_or_default(),
                result: String::new(),
            }
        })
        .collect()
}

/// Is this response an event stream? Only that content type is read as one — a server that answers
/// `application/json` to a streaming request has answered with one object, and framing it would find no
/// `data:` line and answer nothing.
pub fn is_event_stream(content_type: &str) -> bool {
    content_type.trim().to_ascii_lowercase().starts_with(EVENT_STREAM)
}

/// What the event stream ended with: `[DONE]`, or the connection closing first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Done,
    Eof,
}

/// A stream read to its end, and how it ended.
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

/// Read `data:` lines as they arrived: concatenate `choices[0].delta`'s content and reasoning, reassemble tool
/// calls by index, stop at `[DONE]`.
///
/// A tool call is split across chunks — the first carries its id and name, later ones carry only an index and a
/// piece of `arguments` — so the index picks the entry, the name is set once, and the arguments append. Keying
/// by id instead would merge every call a server leaves unnamed, which llama.cpp does routinely.
pub fn from_events(lines: &[&str]) -> Streamed {
    let mut content = String::new();
    let mut reasoning = String::new();
    // Index order, not arrival order: a chunk may repeat an index after another index has appeared.
    let mut calls: Vec<(usize, ToolCall)> = Vec::new();
    for line in lines {
        let Some(payload) = line.trim().strip_prefix("data:") else { continue };
        let payload = payload.trim();
        if payload == DONE {
            return Streamed { reply: assemble(content, reasoning, calls), ended: Ended::Done };
        }
        let Ok(chunk) = serde_json::from_str::<Value>(payload) else { continue };
        let choice = chunk.get("choices").and_then(|c| c.get(0));
        // The reason is read before the delta: a server's last chunk usually carries the final piece of text and
        // its finish_reason together, and reading it only when no delta came along would lose it.
        let reason = finish(choice.and_then(|c| c.get("finish_reason")));
        let delta = choice.and_then(|c| c.get("delta"));
        let Some(delta) = delta else { continue };
        if let Some(piece) = delta.get("content").and_then(|c| c.as_str()) {
            content.push_str(piece);
        }
        if let Some(piece) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
            reasoning.push_str(piece);
        }
        for call in delta.get("tool_calls").and_then(|t| t.as_array()).map(|v| v.iter()).unwrap_or_default() {
            let index = call.get("index").and_then(Value::as_u64).unwrap_or(calls.len() as u64) as usize;
            let slot = match calls.iter_mut().find(|(at, _)| *at == index) {
                Some((_, call)) => call,
                None => {
                    calls.push((index, ToolCall { name: String::new(), args: String::new(), result: String::new() }));
                    &mut calls.last_mut().expect("just pushed").1
                }
            };
            let function = call.get("function");
            // Set once: a later chunk repeats the name or omits it, and appending would double it.
            if let Some(name) = function.and_then(|f| f.get("name")).and_then(|n| n.as_str()) {
                if !name.is_empty() {
                    slot.name = name.to_string();
                }
            }
            if let Some(piece) = function.and_then(|f| f.get("arguments")).and_then(|a| a.as_str()) {
                slot.args.push_str(piece);
            }
        }
        // A keep-alive or a reason-only chunk falls through to here either way.
        if let Some(reason) = reason {
            let mut reply = assemble(content, reasoning, calls.clone());
            reply.finish_reason = reason;
            return Streamed { reply, ended: Ended::Eof };
        }
    }
    Streamed { reply: assemble(content, reasoning, calls), ended: Ended::Eof }
}

/// The marker that ends a stream.
pub const DONE: &str = "[DONE]";

fn assemble(content: String, reasoning: String, mut calls: Vec<(usize, ToolCall)>) -> Reply {
    // Back in index order for the caller: the wire may interleave two calls, the model asked for them in a row.
    calls.sort_by_key(|(index, _)| *index);
    Reply { content, reasoning, tool_calls: calls.drain(..).map(|(_, call)| call).collect(), finish_reason: String::new() }
}

// ---- S4: the cancel context ----------------------------------------------------------

/// §1's last bullet: every request rides the run's cancel context, so ⏹ aborts it. The rule is
/// [`services::Kind::rides_cancel_context`]'s — true for every kind today, and explicitly every kind because
/// the prototype left five calls off the list, which is why a ⏹ during a long synthesis used to leave the app
/// waiting for a request it had already given up on.
pub fn rides_cancel() -> bool {
    services::rides_cancel_context(Kind::Chat)
}
