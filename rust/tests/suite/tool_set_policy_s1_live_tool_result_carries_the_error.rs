//! tool:set_policy over the wire: a bad call comes back to the model AS THE TOOL RESULT.
//!
//! `tests/f0_7_s3_a_live_refusal_is_handed_back…` proves the refusal also rides in the next round's
//! user text. This proves the other half §3.11's result column asks for and directive B insists on:
//! the app validates inside the tool, so the second request carries a `tool` message whose content is
//! the error itself — naming the invented field — not only a paraphrase of it in prose.
#![allow(dead_code)]

#[path = "policy_live_harness.rs"]
mod harness;

use harness::*;
use naivepost::project::MarkingPass;

#[test]
fn tool_set_policy_s1_live_tool_result_carries_the_error() {
    let fixture = fixture("toolres");
    // Round one invents a field; round two is the model reading the verdict back and saying nothing more.
    let llm = FakeLlm::start(vec![
        set_policy("markingStyle", "joins", "I meant the marking pass"),
        chat("understood \u{2014} nothing else to set"),
    ]);
    point_at(&llm.url());

    let run = derive_lively(&fixture, &fixture.context);

    let first = llm.next_seen();
    // The refused proposal makes `policy::derive` re-ask (§3's loop), so a second request goes out. What
    // has to be in it is the ANSWER to the first call -- the tool result, naming the field it refused --
    // not only a paraphrase of it in prose. A server that answered `set_policy` with a success would
    // leave this sentence nowhere in the conversation: nothing else in it names a field.
    let second = llm.next_seen();
    assert_eq!(llm.drained(), 1, "exactly two requests went out");
    assert!(
        !first.body.contains("is not a policy field"),
        "the first ask carried no verdict yet: {}",
        clip(&first.body)
    );
    assert!(
        second.body.contains("is not a policy field") && second.body.contains("markingStyle"),
        "the refusal came back as the call's own tool result: {}",
        clip(&second.body)
    );
    // Nothing landed: the invented field is not a policy field, and the model never proposed a real one.
    assert_eq!(
        run.policy.marking_pass.value,
        MarkingPass::Retakes,
        "the refused call changed no field"
    );
    // S3's other door still works alongside it: the same sentence is in the refusals the loop keeps.
    assert!(
        run.derived
            .refusals
            .iter()
            .any(|say| say.contains("markingStyle")),
        "{:?}",
        run.derived.refusals
    );
}
