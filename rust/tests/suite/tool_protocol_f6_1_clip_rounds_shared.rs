//! §F6.1's clip-round checks, shared: a loopback chat fake that answers a SCRIPTED LIST of bodies (one per
//! request, in order) and records every body it saw, plus the scenario root every live check needs.
//!
//! `tests/suite/captions_wire_harness.rs` answers every ask with the SAME scripted reply, which is right for the
//! single-request passes but cannot show a conversation: a second round gets the first round's call back, so the
//! loop never ends. Here each request spends the next reply, which is what makes "the loop actually ran" testable:
//! a round line only exists if a second request left the program.
//!
//! The scenario pins `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory — process-wide state — so each
//! scenario lives in its OWN test binary, exactly as the streaming round's files do.

#![allow(dead_code)] // every binary compiles this whole module; a helper it does not call is not a warning here

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

/// What the fake saw from one request.
pub struct Seen {
    pub body: String,
}

impl Seen {
    /// The request as JSON, parsed: the shape a check reads `messages` and `tools` out of.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("the ask sent a JSON body")
    }

    /// The `messages` array of a request: what the loop had grown by that round.
    pub fn messages(&self) -> Vec<serde_json::Value> {
        self.json()["messages"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// The tool names offered in this request.
    pub fn offered(&self) -> Vec<String> {
        self.json()["tools"]
            .as_array()
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every `role` in the request, in order.
    pub fn roles(&self) -> Vec<String> {
        self.messages()
            .iter()
            .filter_map(|message| message["role"].as_str().map(str::to_string))
            .collect()
    }
}

/// A loopback chat server that answers each request with the NEXT body of its script, recording every body it
/// saw. Once the script runs out it keeps answering with its last reply, so a check that expects the loop to stop
/// still gets an answer rather than a hang.
pub struct RoundsLlm {
    port: u16,
    received: Receiver<Seen>,
    script: Arc<Mutex<Vec<String>>>,
    sent: Arc<AtomicUsize>,
}

impl RoundsLlm {
    pub fn start(replies: Vec<String>) -> RoundsLlm {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let (sender, received) = mpsc::channel();
        let script = Arc::new(Mutex::new(replies));
        let sent = Arc::new(AtomicUsize::new(0));
        std::thread::spawn({
            let script = script.clone();
            let sent = sent.clone();
            move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let index = sent.fetch_add(1, Ordering::SeqCst);
                    let reply = {
                        let held = script.lock().expect("script lock");
                        held.get(index)
                            .cloned()
                            .unwrap_or_else(|| held.last().cloned().unwrap_or_default())
                    };
                    if let Some(seen) = serve(stream, &reply) {
                        let _ = sender.send(seen);
                    }
                }
            }
        });
        RoundsLlm {
            port,
            received,
            script,
            sent,
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The next request that reached the server.
    pub fn next(&self) -> Seen {
        self.received
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the ask reached the fake LLM")
    }

    /// Every request, waiting briefly for the first so a finished synchronous flow can be read whole. The count
    /// is how a check proves a second round happened, or that the loop stopped at its budget.
    pub fn drain(&self) -> Vec<Seen> {
        let mut seen = vec![self.next()];
        loop {
            match self
                .received
                .recv_timeout(std::time::Duration::from_millis(200))
            {
                Ok(more) => seen.push(more),
                Err(_) => return seen,
            }
        }
    }

    /// How many requests the fake answered.
    pub fn requests(&self) -> usize {
        self.sent.load(Ordering::SeqCst)
    }

    /// The script this fake holds — a check that scripted `N` rounds asserts it never saw more than `N` bodies
    /// for anything else.
    pub fn scripted(&self) -> usize {
        self.script.lock().expect("script lock").len()
    }
}

/// Answer one request with `Content-Length` framing and `Connection: close`, shutting the socket so the reader
/// sees EOF: the shape `tests/suite/tool_protocol_f6_1_refusal.rs` proves ureq reads cleanly.
fn serve(mut stream: TcpStream, answer: &str) -> Option<Seen> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            length = header
                .split(':')
                .nth(1)
                .unwrap_or("0")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).ok()?;
    }
    let framed = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
        answer.len()
    );
    let _ = stream.write_all(framed.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Some(Seen {
        body: String::from_utf8_lossy(&payload).to_string(),
    })
}

// ---- the scripted answers ---------------------------------------------------------------------

/// One tool call in the envelope a real server frames it with: `choices[0].message.tool_calls`, with
/// `content` null (which is exactly why the ask must read the calls and not the text).
pub fn tool_answer(name: &str, args: serde_json::Value) -> String {
    serde_json::json!({
        "choices": [{"message": {"content": null, "tool_calls": [{
            "id": "call_1",
            "type": "function",
            "function": {"name": name, "arguments": args.to_string()}
        }]}}]
    })
    .to_string()
}

/// Two calls in one round, in the order the model asked for them.
pub fn tool_answer_two(
    first: (&str, serde_json::Value),
    second: (&str, serde_json::Value),
) -> String {
    let call = |name: &str, args: serde_json::Value| {
        serde_json::json!({
            "id": "call", "type": "function",
            "function": {"name": name, "arguments": args.to_string()}
        })
    };
    serde_json::json!({
        "choices": [{"message": {"content": null, "tool_calls": [
            call(first.0, first.1),
            call(second.0, second.1),
        ]}}]
    })
    .to_string()
}

/// A chat envelope whose `content` is this prose document — what a round that answered in words looks like.
pub fn envelope_of(content: &str) -> String {
    serde_json::json!({ "choices": [ { "message": { "content": content } } ] }).to_string()
}

/// One caption of the CAPTIONS document: `(start, end, text)`.
pub type Caption = (f64, f64, &'static str);
/// One clip's entry of that document: its number and the captions asked for inside it.
pub type ClipCaptions = (u32, &'static [Caption]);

/// The CAPTIONS document §`prompts/system.md`'s `captions` row specifies, in that envelope.
pub fn caption_json(clips: &[ClipCaptions]) -> String {
    let entries: Vec<_> = clips
        .iter()
        .map(|(number, captions)| {
            serde_json::json!({
                "i": number,
                "fx": captions
                    .iter()
                    .map(|(start, end, text)| {
                        serde_json::json!({"start": start, "end": end, "text": text})
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    envelope_of(&serde_json::json!({ "clips": entries }).to_string())
}

/// This pass's own two tools, spelled the way the program filters them (`add_caption` + `finish`): a check names
/// them so a wire that offered the whole §3.7 catalogue is caught rather than accepted.
pub fn caption_tools() -> Vec<naivepost::tools::Tool> {
    naivepost::tools::offered(naivepost::roles::Job::ClipRules)
        .into_iter()
        .filter(|tool| {
            matches!(
                tool,
                naivepost::tools::Tool::AddCaption | naivepost::tools::Tool::Finish
            )
        })
        .collect()
}

/// This pass's own three tools, decorations spelled the way the program filters them.
pub fn effect_tools() -> Vec<naivepost::tools::Tool> {
    naivepost::tools::offered(naivepost::roles::Job::ClipRules)
        .into_iter()
        .filter(|tool| {
            matches!(
                tool,
                naivepost::tools::Tool::AddEffect
                    | naivepost::tools::Tool::GetFrames
                    | naivepost::tools::Tool::Finish
            )
        })
        .collect()
}

/// `P.eng.llmToolRounds`, read the way the program reads it, so a check that counts rounds counts the setting.
pub fn tool_rounds() -> u32 {
    naivepost::roles::TOOL_ROUNDS
}
