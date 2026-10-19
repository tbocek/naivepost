//! F0.7 S3 over the wire, scenario 3: a refused proposal goes back to the model and its fix lands.
//!
//! The refusal is not scripted into anything: `policy::apply` refuses the unknown field, and the second
//! request carries that sentence because the leg appends it (S3's "error back to the model"). Two
//! requests reaching the fake is therefore the proof that the loop closed over the socket.
#![allow(dead_code)]

#[path = "policy_live_harness.rs"]
mod harness;

use harness::*;
use naivepost::project::{CutMode, MarkingPass};

#[test]
fn f0_7_s3_a_live_refusal_is_handed_back_and_the_second_round_lands() {
    let fixture = fixture("refused");
    // Round one invents a field; round two, told about it, answers with a real one.
    let llm = FakeLlm::start(vec![
        set_policy("markingStyle", "joins", "I meant the marking pass"),
        set_policy("markingPass", "joins", "the seams are what matter here"),
    ]);
    point_at(&llm.url());

    let run = derive_lively(&fixture, &fixture.context);

    let first = llm.next_seen();
    let second = llm.next_seen();
    assert_eq!(llm.drained(), 0, "exactly two requests went out");
    // Round two differs from round one by the refusal it was handed -- the sentence `policy::apply` wrote.
    assert!(
        !first.body.contains("is not a policy field"),
        "the first ask carried no refusal: {}",
        clip(&first.body)
    );
    assert!(
        second.body.contains("is not a policy field")
            && second.body.contains("markingStyle"),
        "the second ask carried the refusal naming the bad field: {}",
        clip(&second.body)
    );
    // The invented field changed nothing anywhere.
    assert_eq!(
        run.policy.marking_pass.value,
        MarkingPass::Joins,
        "the corrected proposal landed" // P.policy.markingPass
    );
    assert_eq!(run.policy.cut_mode.value, CutMode::Model, "untouched fields keep their default");
    // And the derivation reports both halves: one refusal said, one field set.
    assert_eq!(run.derived.refusals.len(), 1, "{:?}", run.derived.refusals);
    assert!(
        run.derived.refusals[0].contains("markingStyle"),
        "the refusal names what was wrong: {:?}",
        run.derived.refusals[0]
    );
    assert!(
        run.derived.applied.contains(&("markingPass", naivepost::policy::Applied::Set)),
        "{:?}",
        run.derived.applied
    );
}
