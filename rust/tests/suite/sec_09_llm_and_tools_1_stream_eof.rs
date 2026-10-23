//! §09-llm-and-tools#1-request-contract S4, the early-end half: a stream that closes without `[DONE]` keeps
//! what arrived, and the reader says it knows the answer is a partial.
//!
//! The socket half of this clause — a server whose connection ends mid-stream, read through the program's own
//! chat door — lives in `tests/suite/wire.rs::…_s3_eof_without_done_keeps_what_arrived`. What that cannot show is the
//! OTHER thing §1 states: the reader reports HOW a stream ended, so a caller can tell "the model finished" from
//! "the wire died". A decorations ask cannot be that witness: an EFFECTS document cut in half parses to no
//! calls, which looks exactly like a model that proposed nothing. So this binary dials the fake through the same
//! `server_leg::call` every step uses and reads the stream's own verdict off the framing.

#[path = "sec_09_stream_shared.rs"]
mod stream;

use naivepost::llm_request as llm;
use naivepost::requests::{Request, Service};
use naivepost::services::{self, Kind, Server};

#[test]
fn sec_09_llm_and_tools_1_request_contract_s4_a_stream_that_ends_early_keeps_what_arrived() {
    let (_root, tree) = stream::fixture("early-end");
    let llm = stream::StreamLlm::start(stream::truncated_chunks());
    stream::point_at(&llm.url());
    let endpoint = services::llm_endpoint(&stream::conf());
    let base = Request {
        service: Service::Llm,
        model: stream::MODEL.to_string(),
        kind: "chat".to_string(),
        step: "effects".to_string(),
        ..Default::default()
    };
    let body =
        serde_json::json!({"model": stream::MODEL, "messages": [], "stream": true}).to_string();
    let (_row, sent) = server_leg_call(&tree, &base, &endpoint, &body);
    let sent = sent.expect("the fake answered over the wire");
    assert!(
        llm::is_event_stream(&sent.content_type),
        "the fake streamed: {}",
        sent.content_type
    );

    // The framing rule, on the bytes that actually arrived.
    let lines: Vec<&str> = sent.body.lines().collect();
    let got = llm::from_events(&lines);
    assert_eq!(
        got.ended,
        llm::Ended::Eof,
        "no [DONE], so the reader must say so"
    );
    assert!(
        !got.complete(),
        "a reply whose stream was cut is known to be a partial"
    );
    assert!(
        !got.reply.answer().is_empty(),
        "EOF keeps what arrived rather than throwing it away: {:?}",
        got.reply.answer()
    );
    // And the same bytes routed through the door every chat caller reads with answer, they do not fail: the
    // partial IS the reply a step gets.
    let reply =
        llm::reply_from(&sent.content_type, &sent.body).expect("a partial stream still answers");
    assert_eq!(
        reply.answer(),
        got.reply.answer(),
        "the door and the reader agree on what arrived"
    );
}

/// One call through the leg every step dials with, unwrapped from the liveness watch: this check is about the
/// framing, and the watch belongs to §F6.3's own tests.
fn server_leg_call(
    tree: &naivepost::layout::Tree,
    base: &Request,
    endpoint: &services::Endpoint,
    body: &str,
) -> (Request, Result<naivepost::server_leg::Sent, String>) {
    naivepost::server_leg::call(
        tree,
        base,
        Server::Llm,
        endpoint,
        Kind::Chat,
        None,
        Some(body),
        &|| false,
    )
}
