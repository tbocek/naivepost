//! F6.2 \u00a73's repair, first live scenario: a repaired captions ask over a wire.
//!
//! Split from `tests/suite/llm_repair_f6_2.rs` because the harness is one-scenario-per-binary — `XDG_CONFIG_HOME`
//! and the working directory are process-wide, so a second fixture in the same target would read whichever
//! environment the other set last.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;

use naivepost::captions_ask;
use naivepost::llm_retry;

/// \u00a73's short retry is a real 2 s wait; the scale seam (`llm_retry_leg::nap_duration`) divides it so the
/// check sits through it in milliseconds. The log still names the seconds \u00a73 spells.
fn init() {
    unsafe { std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000") };
}

/// The live half: ✐ Captions' prose path against a fake whose FIRST answer is all reasoning and whose
/// second is a real CAPTIONS document. Before this round the first answer was fatal — the identical request
/// would have earned the identical non-answer — so the assertions are about the SECOND request's bytes: it
/// carries the correction, and the batch lands. Which way the thinking switch sits on either of them is §4's
/// business and another test's assertion; what this one pins is the repair turn itself.
#[test]
fn f6_2_s3_d_a_live_all_reasoning_captions_answer_is_asked_again_repaired_and_lands() {
    init();
    let fixture = fixture("repair_lands");
    let (clips, words) = two_clips_with_words();
    // A fake that spends its script IN ORDER and then keeps answering with its LAST entry (the shared harness
    // fake collapses to its last reply as soon as one is left, which is right for a plain retry but cannot
    // show "bad answer, then good answer" — and §3's repair is exactly that pair).
    let llm = ScriptedLlm::start(vec![
        // `reasoning_content` carries the thinking and `content` holds NOTHING: that is §3's `noAnswer`, and
        // the shape a reasoning server actually answers with (see the notes above the assertions below).
        format!(
            "{{\"choices\":[{{\"message\":{{\"reasoning_content\":\"{reasoning}\",\"content\":\"\"}}}}]}}",
            reasoning = "Let me consider every spoken line of this session."
        ),
        caption_json(&[(1, &[(0.4, 1.6, "the network")])]),
    ]);
    point_at(&llm.url());
    let asked = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &words, 0, &|| false);
    assert!(!asked.failed, "the repaired answer landed: {}", asked.reason);
    assert_eq!(asked.calls.len(), 1, "the caption came from the SECOND answer: {:?}", asked.calls);
    assert_eq!(asked.calls[0].text, "the network");

    // §3's repair is ONE extra request, and it is the SECOND one on the wire. Its bytes are what a check can
    // pin: `retryTurn` adds the correction as an extra turn — as a `tool` result, because captions offer tools
    // (§3's "with tools these become tool results") — and `thinkAgain` turns thinking off for an answer that
    // held nothing. The empty answer itself is NOT echoed back: some servers refuse a conversation holding a
    // blank assistant turn. Before this round the second request was an UNCHANGED resend of the non-answer, so
    // every assertion below fails against the old code.
    let seen = llm.drain();
    assert_eq!(seen.len(), 2, "one repair, one extra request: {}", seen.len());
    for one in &seen {
        assert!(
            !one.body.contains("Let me consider"),
            "the rejected reasoning was never echoed back into a later request: {}",
            one.body
        );
    }
    let repaired = seen[1].parsed();
    let messages = repaired["messages"]
        .as_array()
        .expect("the repair carries its messages")
        .clone();
    let roles: Vec<&str> = messages.iter().filter_map(|m| m["role"].as_str()).collect();
    assert_eq!(
        roles,
        vec!["system", "user", "tool"],
        "mid-round, so the correction arrives as a tool result and no blank assistant turn was added: {roles:?}"
    );
    let correction = messages[2]["content"][0]["text"]
        .as_str()
        .expect("the correction is a text part");
    assert!(
        correction.starts_with("Your answer failed validation:"),
        "§3's correction opens the turn: {correction}"
    );
    assert!(
        correction.contains(llm_retry::NO_ANSWER),
        "and names WHICH repair it was, in the wording the model was tuned on: {correction}"
    );
    assert!(
        correction.ends_with("Return corrected strict JSON only."),
        "and closes it the way §3 spells: {correction}"
    );
    assert_eq!(
        repaired["enable_thinking"].as_bool(),
        Some(false),
        "`thinkAgain`: an empty answer retries without thinking: {repaired}"
    );
    assert_eq!(
        seen[0].parsed()["enable_thinking"].as_bool(),
        Some(false),
        "§4 gives the captions prompt no thinking, so neither ask on this pair thinks (see \
         `sec_10_parameters_4_prompts_s9_the_captions_repair_stays_thinking_off`)"
    );
}

/// Read a recorded request body as JSON without every assertion repeating the parse.
trait Parsed {
    fn parsed(&self) -> serde_json::Value;
}

impl Parsed for harness::Seen {
    fn parsed(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("the fake records a JSON chat body")
    }
}

/// A loopback chat fake that answers each request with the NEXT scripted reply and, once the script runs
/// out, keeps answering with its LAST one. `captions_wire_harness::FakeLlm` collapses to its last entry as
/// soon as ONE is left, which is right for a plain retry but cannot show "bad answer then good answer" — and
/// §3's repair is precisely that pair, so this check needs its ordering spelled out.
struct ScriptedLlm {
    port: u16,
    /// Every request that reached the fake, in order. A `Vec` rather than a channel so a check can read the
    /// conversation after the synchronous flow returned, with no race about which request is "next".
    seen: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

impl ScriptedLlm {
    fn start(replies: Vec<String>) -> ScriptedLlm {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let port = listener.local_addr().expect("a local address").port();
        let seen: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>> = Default::default();
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
                recording.lock().expect("seen lock").push((String::from_utf8_lossy(&payload).to_string(), answer));
            }
        });
        ScriptedLlm { port, seen }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Every request that reached the fake, waiting briefly for the first to show up: the flow is
    /// synchronous, so a finished ask has already been answered by every entry this returns.
    fn drain(&self) -> Vec<crate::suite::llm_repair_live_f6_2::harness::Seen> {
        for _ in 0..400 {
            if !self.seen.lock().expect("seen lock").is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Two settled reads, so a second request that is still on its way cannot be missed.
        std::thread::sleep(std::time::Duration::from_millis(250));
        self.seen
            .lock()
            .expect("seen lock")
            .iter()
            .map(|(body, reply)| harness::Seen {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                body: body.clone(),
                reply: reply.clone(),
            })
            .collect()
    }
}
