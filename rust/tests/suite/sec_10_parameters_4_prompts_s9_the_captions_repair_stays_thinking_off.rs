//! F3.9 §3's repair over a socket: what a repaired captions ask sends. One scenario per binary
//! (`XDG_CONFIG_HOME`/cwd are process-wide), so this lives beside — not inside — the plain ask check.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;

use naivepost::captions_ask;

/// §3's repair lives behind [`ScriptedLlm`], a loopback chat fake that answers with the NEXT scripted reply
/// and then keeps answering with its LAST one. `harness::FakeLlm` collapses to its last entry as soon as ONE
/// is left, which cannot show "bad answer, then good answer" — and a repair is exactly that pair.
struct ScriptedLlm {
    port: u16,
    /// Every request that reached the fake, in order, so a check can read both bodies after the sync flow ends.
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl ScriptedLlm {
    fn start(replies: Vec<String>) -> ScriptedLlm {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
        let recording = seen.clone();
        let script = std::sync::Arc::new(std::sync::Mutex::new((replies, 0usize)));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let answer = {
                    let mut held = script.lock().expect("script lock");
                    let index = held.1;
                    held.1 += 1;
                    held.0
                        .get(index)
                        .cloned()
                        .or_else(|| held.0.last().cloned())
                        .unwrap_or_default()
                };
                let mut reader = BufReader::new(stream.try_clone().expect("clone the socket"));
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                let mut length = 0usize;
                loop {
                    let mut head = String::new();
                    if reader.read_line(&mut head).expect("read a header") == 0 {
                        break;
                    }
                    if head.to_ascii_lowercase().starts_with("content-length:") {
                        length = head.split(':').nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
                    }
                    if head.trim().is_empty() {
                        break;
                    }
                }
                let mut payload = vec![0u8; length];
                if length > 0 {
                    reader.read_exact(&mut payload).expect("read the body");
                }
                let framed = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    answer.len(),
                    answer
                );
                let _ = stream.write_all(framed.as_bytes());
                let _ = stream.flush();
                recording
                    .lock()
                    .expect("seen lock")
                    .push(String::from_utf8_lossy(&payload).to_string());
            }
        });
        ScriptedLlm { port, seen }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Every request body that reached the fake, waiting briefly for the first: the flow is synchronous, so a
    /// finished ask has already been answered by every entry this returns.
    fn drain(&self) -> Vec<String> {
        for _ in 0..400 {
            if !self.seen.lock().expect("seen lock").is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Two settled reads, so a second request still on its way cannot be missed.
        std::thread::sleep(std::time::Duration::from_millis(250));
        self.seen.lock().expect("seen lock").clone()
    }
}

/// §3's repair of a captions answer that was ALL reasoning. The first request is thinking-OFF like any other
/// captions ask (§4 does not list `captions` among the prompts that think), and the second — the one carrying
/// the correction — must be thinking-off too. `thinkAgain` turns thinking off for an empty answer, which is a
/// no-op here precisely because the ask never thought: before this round the repair leg was handed `true` and
/// the retry went out as a THINKING request, a mode §4 never gives this prompt.
#[test]
fn sec_10_parameters_4_prompts_s9_the_captions_repair_stays_thinking_off() {
    unsafe { std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000") };
    let fixture = fixture("repair_off");
    let (clips, words) = two_clips_with_words();
    // In order, then holding its LAST reply: the first answer is §3's `noAnswer` (reasoning, no content) and
    // the second is a real CAPTIONS document, so exactly one repair goes out.
    let llm = ScriptedLlm::start(vec![
        "{\"choices\":[{\"message\":{\"reasoning_content\":\"Let me weigh every line.\",\"content\":\"\"}}]}".to_string(),
        add_captions_json(&[(1, 0.4, 1.6, "the network")]),
    ]);
    point_at(&llm.url());

    let asked = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &words, 2, &|| false);
    assert!(!asked.failed, "the repaired answer landed: {}", asked.reason);
    assert_eq!(asked.calls.len(), 1, "the caption came from the SECOND answer: {:?}", asked.calls);

    let seen = llm.drain();
    assert_eq!(seen.len(), 2, "one repair, one extra request: {}", seen.len());
    for (at, body) in seen.iter().enumerate() {
        let sent: serde_json::Value = serde_json::from_str(body).expect("the request is JSON");
        assert_eq!(
            sent["enable_thinking"].as_bool(),
            Some(false),
            "request {at} went out thinking-off, as §4 gives this prompt"
        );
        assert_eq!(
            sent["chat_template_kwargs"]["enable_thinking"].as_bool(),
            Some(false),
            "and the template spelling agrees on request {at}"
        );
    }
}
