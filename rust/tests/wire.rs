//! §09-llm-and-tools#1-request-contract, S3's second half: the response read off a real socket.
//!
//! `from_events` and `parse_reply` are unit-tested in the parent file. What unit tests cannot prove is the
//! ROUTING: §1 answers "Streaming only when the response is `text/event-stream`" — a server that streams is
//! framed from `data:` chunks, a server that answers JSON is read as one envelope. That decision is made from
//! a header the wire has to carry, so these tests dial a loopback server that sets the header, and read the
//! reply the way the program's chat door does.
//!
//! The env is process-wide, which is why this is a `mod` of the contract binary rather than a binary of its
//! own next to the others that re-point the settings.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use naivepost::layout::Tree;
use naivepost::llm_request as llm;
use naivepost::requests::Request;
use naivepost::server_leg;
use naivepost::services::{self, Kind, Server};

/// Answer the next request with `content_type` and `body`, and hand back the URL to dial.
///
/// One request per server: each test makes exactly one call, and the socket is shut as soon as the answer is
/// written so the client's read loop ends instead of waiting on a keep-alive.
fn server_answering(content_type: &str, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener.local_addr().expect("a local address").port();
    let header = content_type.to_string();
    let payload = body.to_string();
    std::thread::spawn(move || {
        let stream = match listener.incoming().next().map(|r| r.map_err(std::io::Error::other)) {
            Some(Ok(stream)) => stream,
            _ => return,
        };
        // The request has to be drained before the answer is useful to the client, and its body is read to the
        // declared length rather than a line, or the client sits waiting to finish writing.
        let mut stream = stream;
        let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
        let mut line = String::new();
        let mut length = 0usize;
        let _ = reader.read_line(&mut line);
        loop {
            let mut header_line = String::new();
            if reader.read_line(&mut header_line).unwrap_or(0) == 0 {
                break;
            }
            if header_line.trim().is_empty() {
                break;
            }
            if header_line.to_ascii_lowercase().starts_with("content-length:") {
                length = header_line.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
            }
        }
        let mut payload_bytes = vec![0u8; length];
        if length > 0 {
            let _ = reader.read_exact(&mut payload_bytes);
        }
        let framed = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {header}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        let _ = stream.write_all(framed.as_bytes());
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Both);
    });
    format!("http://127.0.0.1:{port}")
}

/// A project folder for the timed row `server_leg::call` writes.
fn project_dir(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-s9contract-{tag}.naivepost"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create the project folder");
    root
}

fn never() -> bool {
    false
}

/// The settings the dial reads: the model every scenario asks with, and the fake's address.
/// Built as `Conf::default()` then written out, because `Conf` keeps one private field and a struct literal
/// cannot use `..` past it — `settings::save` is the door every real caller writes through anyway.
fn conf_with(url: &str) -> naivepost::settings::Conf {
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "contract-model".to_string();
    conf.server = url.to_string();
    conf
}

/// Dial the chat endpoint of `url` and hand back what the PROGRAM made of the answer, through the one door
/// every chat caller reads replies from.
fn dial_and_read(url: &str, tag: &str) -> (naivepost::llm_request::Reply, String) {
    let tree = Tree::new(project_dir(tag)).expect("a .naivepost folder is a project");
    let endpoint = services::llm_endpoint(&conf_with(url));
    let base = Request {
        service: naivepost::requests::Service::Llm,
        model: "contract-model".to_string(),
        kind: "chat".to_string(),
        step: "settings".to_string(),
        ..Default::default()
    };
    let body = serde_json::json!({ "model": "contract-model", "messages": [] }).to_string();
    let (_row, sent) = server_leg::call(&tree, &base, Server::Llm, &endpoint, Kind::Chat, None, Some(&body), &never);
    let sent = sent.expect("the fake server answered over the wire");
    let content_type = sent.content_type.clone();
    (llm::reply_from(&content_type, &sent.body).expect("the answer reads"), content_type)
}

/// S3: a server that answers `application/json` is read as ONE envelope — content, reasoning kept apart,
/// tool calls and the finish reason all arrive.
#[test]
fn sec_09_llm_and_tools_1_request_contract_s3_json_answer_over_the_wire() {
    let envelope = serde_json::json!({
        "choices": [{
            "message": {
                "content": "the title writes itself",
                "reasoning_content": "let me weigh three titles first",
                "tool_calls": [
                    { "type": "function", "function": { "name": "set_title", "arguments": "{\"text\":\"a title\"}" } }
                ]
            },
            "finish_reason": "tool_calls"
        }]
    });
    let url = server_answering("application/json", &envelope.to_string());
    let (reply, content_type) = dial_and_read(&url, "json");
    assert_eq!(content_type, "application/json", "the wire's own header reached the door");
    assert_eq!(reply.answer(), "the title writes itself");
    assert_eq!(reply.reasoning, "let me weigh three titles first", "kept, but not as the answer");
    assert_ne!(reply.answer(), reply.reasoning, "§1: reasoning is never the answer");
    assert_eq!(reply.tool_calls.len(), 1);
    assert_eq!(reply.tool_calls[0].name, "set_title");
    assert_eq!(reply.tool_calls[0].args, "{\"text\":\"a title\"}");
    assert_eq!(reply.finish_reason, "tool_calls");
}

/// S3: the same program door, but the server answers `text/event-stream` — the reply is framed from `data:`
/// chunks, the tool call is reassembled across them, and `[DONE]` closes it.
#[test]
fn sec_09_llm_and_tools_1_request_contract_s3_event_stream_answer_over_the_wire() {
    let chunks = [
        "data: {\"choices\":[{\"delta\":{\"content\":\"A win nobody \",\"reasoning_content\":\"thinking \"}}]}",
        ": keep-alive",
        "data: {\"choices\":[{\"delta\":{\"content\":\"expected\",\"reasoning_content\":\"first\"}}]}",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c0\",\"function\":{\"name\":\"set_title\",\"arguments\":\"{\\\"text\\\"\"}}]}}]}",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":\\\"a title\\\"}\"}}]}}]}",
        "data: {\"choices\":[{\"finish_reason\":\"tool_calls\",\"delta\":{}}]}",
        "data: [DONE]",
        "",
    ]
    .join("\n");
    let url = server_answering("text/event-stream; charset=utf-8", &chunks);
    let (reply, content_type) = dial_and_read(&url, "stream");
    assert!(llm::is_event_stream(&content_type), "the header says stream: {content_type}");
    assert_eq!(reply.answer(), "A win nobody expected", "content joined across chunks");
    assert_eq!(reply.reasoning, "thinking first", "reasoning kept apart from the answer");
    assert_eq!(reply.tool_calls.len(), 1, "one call, reassembled by index across chunks");
    assert_eq!(reply.tool_calls[0].name, "set_title");
    assert_eq!(reply.tool_calls[0].args, "{\"text\":\"a title\"}", "the pieces joined in arrival order");
    assert_eq!(reply.finish_reason, "tool_calls");
    // The framing rule again, from the same door: a JSON body is NEVER framed, and a stream body is never
    // parsed as one object. Both would otherwise answer nothing.
    assert!(llm::reply_from("application/json", &chunks).is_err(), "framed lines are not one JSON object");
    let json_envelope = serde_json::json!({ "choices": [{ "message": { "content": "plain" } }] }).to_string();
    let streamed = llm::reply_from("text/event-stream", &json_envelope).expect("a stream answers, even with nothing in it");
    assert_eq!(streamed.answer(), "", "no `data:` line, no content — routing is by header, not by guesswork");
}

/// S3's last streaming clause over the wire: the connection closes without `[DONE]`, and what arrived IS the
/// answer rather than a failure.
#[test]
fn sec_09_llm_and_tools_1_request_contract_s3_eof_without_done_keeps_what_arrived() {
    let half = "data: {\"choices\":[{\"delta\":{\"content\":\"the first half of an \"}}]}\ndata: {\"choices\":[{\"delta\":{\"content\":\"answer that got cut\"}}]}\n";
    let url = server_answering("text/event-stream", half);
    let (reply, _) = dial_and_read(&url, "eof");
    assert_eq!(reply.answer(), "the first half of an answer that got cut", "EOF keeps what arrived");
    // The door hands back a Reply, so ended-ness is the stream's own question, asserted where the framing lives.
    let lines: Vec<&str> = half.lines().collect();
    let streamed = llm::from_events(&lines);
    assert!(!streamed.complete(), "no [DONE], so the answer is known to be a partial");
    assert_eq!(streamed.ended, llm::Ended::Eof);
    assert_eq!(streamed.reply.answer(), reply.answer(), "the same text the wire got");
    // And a body that is neither JSON nor framed says so by name — the reason every caller still pins.
    assert_eq!(
        llm::reply_from("application/json", "i am plain text").unwrap_err(),
        "the server's answer is not JSON"
    );
}
