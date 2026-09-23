//! §09-llm-and-tools#1-request-contract — what is sent to a chat server and what comes back, checked against
//! `naivepost::llm_request`.
//!
//! §1 is four bullets rather than numbered steps, so they are read here as S1..S4: where a request goes and what
//! it must carry (url, bearer key, model id); the body's shape; the response and its streaming form; and the
//! cancel context. One test per bullet, split where a bullet holds rules that break apart on their own.
//!
//! # Ids used
//!
//! §10 gives none of §1's eight values a `P.` row — it lists them under "the rest … LLM tokens 65536/8192" as
//! fixed in code — and the spec names no `tool:` id for the LLM itself (its only two are `tool:get_context` and
//! `tool:set_policy`, which are §2's). So each constant is cited by the bare `machine.` id its doc comment gives:
//! machine.topP, machine.topK, machine.minP, machine.presencePenalty, machine.thinkingTemperature,
//! machine.thinkingMaxTokens, machine.executeTemperature, machine.executeMaxTokens.

use serde_json::json;

use naivepost::checks;
use naivepost::degraded;
use naivepost::exchanges::{Message, Mode, Part};
use naivepost::llm_request as llm;
use naivepost::roles::{self, Job};
use naivepost::services::{self, Kind};
use naivepost::tools;

const ITEM: &str = "§09-llm-and-tools#1-request-contract";

fn message(role: &str, parts: Vec<Part>) -> Message {
    Message { role: role.to_string(), parts }
}

/// The whole body of one call, with nothing offered and no stream — what every test below reads a field out of.
fn body(mode: Mode) -> serde_json::Value {
    llm::body("qwen3", &[], mode, None, false).expect("a model is set")
}

// ---- S1: where it goes, the key, and the model id -----------------------------------

#[test]
fn sec_09_llm_and_tools_1_request_contract_s1_url_key_and_model_id() {
    assert_eq!(ITEM, "§09-llm-and-tools#1-request-contract");
    // `POST <server>/v1/chat/completions` — and the path is services::Kind::Chat's, so one table knows paths.
    let path = Kind::Chat.path(None);
    assert_eq!(path, "/v1/chat/completions");
    for (server, want) in [
        ("localhost:8731", "https://localhost:8731/v1/chat/completions"),
        ("http://box:8731", "http://box:8731/v1/chat/completions"),
        ("https://box:8731/", "https://box:8731/v1/chat/completions"),
    ] {
        assert_eq!(llm::chat_url(server).expect("a server is set"), want, "{server}");
        assert!(want.ends_with(&path), "{want} does not end in the Kind's own path");
    }
    // No server is as much of a dead end as no model: there is nothing to send to.
    assert!(llm::chat_url("   ").is_err());

    // Bearer key when set — and only when set, because a token that says "none" some servers read as a bad
    // request (services::authorization's reason, reused rather than restated).
    let headers = llm::headers("", false);
    assert!(headers.iter().all(|(name, _)| name != "authorization"), "{headers:?}");
    assert_eq!(headers[0], ("content-type".to_string(), "application/json".to_string()));
    let keyed = llm::headers("k1", false);
    assert!(keyed.contains(&("authorization".to_string(), "Bearer k1".to_string())), "{keyed:?}");
    assert_eq!(services::authorization("k1").as_deref(), Some("Bearer k1"));
    assert_eq!(services::authorization(""), None, "an empty key sends no header");

    // A streaming caller says what it will read; a plain one does not ask for a stream it will not parse.
    assert!(llm::headers("", true).contains(&("accept".to_string(), llm::EVENT_STREAM.to_string())));
    assert_eq!(llm::EVENT_STREAM, "text/event-stream");

    // Model id required, with §1's own sentence — and it is a different sentence from the settings dialog's.
    assert_eq!(llm::NO_MODEL, "no LLM model configured -- use the gear button");
    assert_eq!(llm::model_required("   ").unwrap_err(), llm::NO_MODEL);
    assert_eq!(llm::model_required("  qwen3  ").unwrap(), "qwen3", "typed with spaces is still the id");
    // Divergence pinned rather than fixed: two wordings of one problem live in the app, because they are asked
    // at different moments — a step about to run (§1's gear-button sentence) against a server being checked.
    let dialog = checks::llm_model_listed(&[], "").unwrap_err();
    assert!(dialog.contains("Fetch models and pick one"), "{dialog}");
    assert_ne!(dialog, llm::NO_MODEL);
}

// ---- S2: the body -------------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_1_request_contract_s2_mode_and_sampling() {
    assert_eq!(ITEM, "§09-llm-and-tools#1-request-contract");
    let thinking = body(Mode::Thinking);
    let execute = body(Mode::Execute);

    // machine.thinkingTemperature / machine.thinkingMaxTokens: reasoning gets full temperature and room.
    assert_eq!(thinking["temperature"], json!(1.0), "machine.thinkingTemperature");
    assert_eq!(thinking["max_tokens"], json!(65536), "machine.thinkingMaxTokens");
    assert_eq!(thinking["enable_thinking"], json!(true));
    // machine.executeTemperature / machine.executeMaxTokens: an executing call wants to be dull about it.
    assert_eq!(execute["temperature"], json!(0.6), "machine.executeTemperature");
    assert_eq!(execute["max_tokens"], json!(8192), "machine.executeMaxTokens");
    assert_eq!(execute["enable_thinking"], json!(false));

    // The sampling four ride whatever the mode is.
    for (label, sent) in [("thinking", &thinking), ("execute", &execute)] {
        assert_eq!(sent["top_p"], json!(0.95), "{label}: machine.topP");
        assert_eq!(sent["top_k"], json!(20), "{label}: machine.topK");
        assert_eq!(sent["min_p"], json!(0.0), "{label}: machine.minP");
        assert_eq!(sent["presence_penalty"], json!(0.0), "{label}: machine.presencePenalty");
        assert_eq!(sent["model"], json!("qwen3"), "{label}");
        // The switch twice, and both copies agree: llama.cpp reads one place, other servers the other.
        assert_eq!(sent["preserve_thinking"], json!(true), "{label}");
        let kwargs = &sent["chat_template_kwargs"];
        assert_eq!(kwargs["enable_thinking"], sent["enable_thinking"], "{label}: the two copies differ");
        assert_eq!(kwargs["preserve_thinking"], json!(true), "{label}");
    }

    // roles::thinking(job) is what picks the mode, and tools::offered(job) what fills `tools`.
    assert!(roles::thinking(Job::ModelCut), "a job that reasons");
    assert!(!roles::thinking(Job::CleanTranscript), "a job that fills in arguments");
    let mode = if roles::thinking(Job::ModelCut) { Mode::Thinking } else { Mode::Execute };
    assert_eq!(body(mode)["enable_thinking"], json!(true));
    assert!(!tools::offered(Job::ModelCut).is_empty(), "a web-tools job offers tools");
    assert!(tools::offered(Job::Asr).is_empty(), "an audio.cpp task has nothing to offer");
}

#[test]
fn sec_09_llm_and_tools_1_request_contract_s2_messages_tools_stream() {
    assert_eq!(ITEM, "§09-llm-and-tools#1-request-contract");
    // A text part and a picture: the picture travels as an `image_url` data URL, so the exchange page shows
    // exactly what the model saw. checks::base64's spelling is the one used.
    let url = llm::data_url(&checks::red_square_png(), "image/png");
    assert!(url.starts_with("data:image/png;base64,"), "{}", &url[..24]);
    assert_eq!(url, checks::vision_probe_url(), "the probe and a real frame are one dialect");
    let parts = llm::message_parts(&[Part::Text("look".into()), Part::Image(url.clone())]);
    assert_eq!(parts[0], json!({ "type": "text", "text": "look" }));
    assert_eq!(parts[1]["image_url"]["url"], json!(url));

    let messages = vec![message("user", vec![Part::Text("look".into()), Part::Image(url)])];
    let sent = llm::body("qwen3", &messages, Mode::Execute, None, false).expect("a model is set");
    assert_eq!(sent["messages"].as_array().expect("an array").len(), 1);
    let content = &sent["messages"][0]["content"];
    assert_eq!(content.as_array().expect("parts").len(), 2);
    assert_eq!(content[0]["type"], json!("text"));
    assert!(content[1]["image_url"]["url"].as_str().expect("a url").contains(";base64,"));

    // `tools` when offered — and omitted, not emptied, when nothing is: an empty array makes some servers
    // advertise no tools at all, which reads as a refusal rather than as nothing on offer.
    let empty_offer: Vec<tools::Tool> = vec![];
    assert!(body(Mode::Execute).get("tools").is_none(), "None offers no key");
    assert!(llm::body("qwen3", &[], Mode::Execute, Some(&empty_offer), false).expect("ok").get("tools").is_none());
    let offered = tools::offered(Job::ModelCut);
    let with = llm::body("qwen3", &[], Mode::Execute, Some(&offered), false).expect("ok");
    let list = with["tools"].as_array().expect("a tools array");
    assert_eq!(list.len(), offered.len());
    assert_eq!(list[0]["function"]["name"], json!(offered[0].name()));

    // `stream true` when the caller streams; absent otherwise (absent is false to every server in §1's table).
    let streamed = llm::body("qwen3", &[], Mode::Execute, None, true).expect("ok");
    assert_eq!(streamed["stream"], json!(true));
    assert!(body(Mode::Execute).get("stream").is_none(), "not streaming sends no key at all");

    // The model id is checked where the body is built, so no call site can forget it.
    assert_eq!(llm::body("", &[], Mode::Execute, None, false).unwrap_err(), llm::NO_MODEL);
}

// ---- S3: the response ---------------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_1_request_contract_s3_parse_reply() {
    assert_eq!(ITEM, "§09-llm-and-tools#1-request-contract");
    let reply = json!({
        "choices": [{
            "message": {
                "content": "cut at 12",
                "reasoning_content": "let me think about this for a long time, because reasoning is verbose",
                "tool_calls": [{ "function": { "name": "get_lines", "arguments": "{\"from\":1}" } }]
            },
            "finish_reason": "stop"
        }]
    });
    let parsed = llm::parse_reply(&reply);
    assert_eq!(parsed.finish_reason, "stop");
    assert_eq!(parsed.tool_calls.len(), 1);
    assert_eq!(parsed.tool_calls[0].name, "get_lines");
    assert_eq!(parsed.tool_calls[0].args, "{\"from\":1}");

    // Reasoning is kept apart and never returned as the answer — even when it dwarfs the content, which is the
    // case that would otherwise tempt a fallback.
    assert_eq!(parsed.answer(), "cut at 12");
    assert!(parsed.reasoning.len() > parsed.content.len() * 3);
    assert!(parsed.reasoning.starts_with("let me think"), "kept, just not as the answer");

    // A truncation is read by degraded's own rule, not restated here.
    let truncated = json!({ "choices": [{ "message": { "content": "half an" }, "finish_reason": degraded::FINISH_LENGTH }] });
    assert_eq!(llm::parse_reply(&truncated).finish_reason, "length");
    assert!(llm::parse_reply(&truncated).cut_off());
    assert!(!parsed.cut_off());

    // Malformed or empty answers read as empty rather than panicking: one server answering `null` must not take
    // a session down.
    let empty = llm::parse_reply(&json!({ "choices": [] }));
    assert_eq!(empty.answer(), "");
    assert_eq!(empty.tool_calls.len(), 0);
    let nulls = llm::parse_reply(&json!({ "choices": [{ "message": { "content": null, "tool_calls": null } }] }));
    assert_eq!(nulls.answer(), "");
    assert_eq!(llm::parse_reply(&json!({})).answer(), "", "an empty object answers nothing");

    // A finish reason at the top level still arrives, since servers put it in either place.
    let top = llm::parse_reply(&json!({ "choices": [{ "message": { "content": "x" } }], "finish_reason": "length" }));
    assert_eq!(top.finish_reason, "length");
    assert!(top.cut_off());
}

#[test]
fn sec_09_llm_and_tools_1_request_contract_s3_streaming() {
    assert_eq!(ITEM, "§09-llm-and-tools#1-request-contract");
    // Streaming only when the response is text/event-stream — a prefix match, because a server may append a charset.
    assert!(llm::is_event_stream("text/event-stream"));
    assert!(llm::is_event_stream("text/event-stream; charset=utf-8"));
    assert!(!llm::is_event_stream("application/json"));

    // Content and reasoning arrive in pieces and are kept apart.
    let text = llm::from_events(&[
        "data: {\"choices\":[{\"delta\":{\"content\":\"cut \"}}]}",
        ": keep-alive",
        "data: {\"choices\":[{\"delta\":{\"content\":\"at 12\",\"reasoning_content\":\"hmm \"}}]}",
        "not a data line at all",
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"long\"}}]}",
        "data: [DONE]",
    ]);
    assert_eq!(text.reply.answer(), "cut at 12");
    assert_eq!(text.reply.reasoning, "hmm long");
    assert!(text.complete(), "[DONE] ended it");

    // A tool call is split across chunks: the index picks the entry, the name arrives once, arguments append.
    let calls = llm::from_events(&[
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"get_lines\",\"arguments\":\"{\\\"from\\\"\"}}]}}]}",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"name\":\"finish\",\"arguments\":\"{}\"}}]}}]}",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":3}\"}}]}}]}",
        "data: [DONE]",
    ]);
    assert_eq!(calls.reply.tool_calls.len(), 2, "reassembled by index, not one merged call");
    assert_eq!(calls.reply.tool_calls[0].name, "get_lines");
    assert_eq!(calls.reply.tool_calls[0].args, "{\"from\":3}", "the pieces joined in arrival order");
    assert_eq!(calls.reply.tool_calls[1].name, "finish");
    assert_eq!(calls.reply.tool_calls[1].args, "{}");
    assert_eq!(calls.ended, llm::Ended::Done);

    // EOF without [DONE] keeps what arrived: the partial is the answer, not a failure.
    let cut = llm::from_events(&["data: {\"choices\":[{\"delta\":{\"content\":\"the first half of an \"}}]}"]);
    assert_eq!(cut.reply.answer(), "the first half of an ", "EOF keeps what arrived");
    assert!(!cut.complete());
    assert_eq!(cut.ended, llm::Ended::Eof);

    // A chunk carrying only a finish reason still says why the stream ended.
    let reason = llm::from_events(&[
        "data: {\"choices\":[{\"delta\":{\"content\":\"half an\"}}]}",
        "data: {\"choices\":[{\"finish_reason\":\"length\",\"delta\":{}}]}",
    ]);
    assert_eq!(reason.reply.answer(), "half an");
    assert_eq!(reason.reply.finish_reason, degraded::FINISH_LENGTH);
    assert!(reason.reply.cut_off());

    // Nothing at all answers nothing, and does not fail.
    let none = llm::from_events(&[]);
    assert_eq!(none.reply.answer(), "");
    assert_eq!(none.ended, llm::Ended::Eof);
}

// ---- S4: the cancel context ----------------------------------------------------------

#[test]
fn sec_09_llm_and_tools_1_request_contract_s4_rides_the_cancel_context() {
    assert_eq!(ITEM, "§09-llm-and-tools#1-request-contract");
    // §1's last bullet: every request rides the run's cancel context, so ⏹ aborts it. The rule is services' and
    // says yes for every kind — the rewrite's fix for a prototype that left five calls off the list, which is
    // why a ⏹ during a long synthesis used to leave the app waiting for a request it had already given up on.
    assert!(llm::rides_cancel());
    assert_eq!(llm::rides_cancel(), services::rides_cancel_context(Kind::Chat));
    for kind in [Kind::Chat, Kind::ListModels, Kind::Health, Kind::Speech, Kind::UnloadAll] {
        assert!(services::rides_cancel_context(kind), "{kind:?} must be cancellable too");
    }
}
