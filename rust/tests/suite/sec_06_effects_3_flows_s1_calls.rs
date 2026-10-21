//! §06-effects §3 Flows S1, the TOOL-CALL scenario: a model that took the offer answers with `add_caption`
//! calls closed by `finish`, and that IS the answer -- one request, no repair, and `cut_captions::place` judges
//! the caption it asked for. Helpers and the other two scenarios: `sec_06_effects_3_flows_s1_shared.rs`.

#![allow(dead_code)]

#[path = "sec_06_effects_3_flows_s1_shared.rs"]
mod shared;

use naivepost::captions_ask;
use naivepost::cut_captions::{self, Reply};
use shared::{add_captions_json, clips, fixture, kept, point_at, rows_paid, FakeLlm};

#[test]
fn sec_06_effects_3_flows_s1_captions_tool_call_answer_places_the_caption() {
    let fixture = fixture("s1-calls");
    let llm = FakeLlm::start(vec![add_captions_json(&[(
        1,
        1.0,
        4.0,
        "The network is live",
    )])]);
    point_at(&llm.url());

    let answer = captions_ask::ask(
        &fixture.tree,
        "Caption every spoken line.",
        &clips(),
        &[],
        1,
        &|| false,
    );
    assert!(
        !answer.failed,
        "the call answer is the answer: {:?}",
        answer.reason
    );
    assert_eq!(
        answer.calls.len(),
        1,
        "one caption was asked for: {:?}",
        answer.calls
    );
    assert_eq!(
        answer.calls[0].text, "The network is live",
        "with the words the call carried"
    );
    assert_eq!(rows_paid(&fixture), 1, "one request, no repair spent");
    let _ = llm.next_seen();

    // The call travels to the same door a typed caption travels to: §3.7's `add_caption` decides what lands.
    match cut_captions::place(&clips(), &answer.calls) {
        Reply::Accepted(fx) => {
            assert_eq!(fx.len(), 1, "the caption placed");
            assert_eq!(
                fx[0].kind, "text",
                "a caption is a text record, proposed or typed"
            );
        }
        Reply::Rejected(reason) => {
            panic!("a caption inside the clip and over the floor must place: {reason}")
        }
    }
    assert!(
        kept(&fixture, "add_caption"),
        "§6: the round that answered is what got cached, so the next identical ask costs no socket"
    );
}
