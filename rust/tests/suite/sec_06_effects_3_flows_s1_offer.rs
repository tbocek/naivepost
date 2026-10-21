//! §06-effects §3 Flows S1, the OFFER scenario: the captions request names the tools §F3.9's **Tools** line
//! lists (`add_caption`, `finish`) -- the offer is what makes the answer shape a fair demand. Helpers and the
//! other two scenarios: `sec_06_effects_3_flows_s1_shared.rs`.

#![allow(dead_code)]

#[path = "sec_06_effects_3_flows_s1_shared.rs"]
mod shared;

use shared::{clips, fixture, finish_json, point_at, rows_paid, FakeLlm};
use naivepost::captions_ask;

#[test]
fn sec_06_effects_3_flows_s1_captions_offer_present_on_the_wire() {
    let fixture = fixture("s1-offer");
    let llm = FakeLlm::start(vec![finish_json()]);
    point_at(&llm.url());

    let answer = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips(), &[], 1, &|| false);
    assert!(!answer.failed, "a `finish` answer is a whole answer: {:?}", answer.reason);
    assert!(answer.calls.is_empty(), "and it asked for no caption");

    let seen = llm.next_seen();
    let body: serde_json::Value = serde_json::from_str(&seen.body).expect("the ask sent JSON");
    let offered: Vec<&str> = body["tools"]
        .as_array()
        .expect("the captions ask offers its tools")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    assert!(offered.contains(&"add_caption"), "add_caption is on the wire: {offered:?}");
    assert!(offered.contains(&"finish"), "finish is on the wire: {offered:?}");
    // §09 §10 wrote its row for the request (§10 counts every request, whatever the answer was).
    assert!(rows_paid(&fixture) >= 1, "the ask left its timed row: {}", rows_paid(&fixture));
}
