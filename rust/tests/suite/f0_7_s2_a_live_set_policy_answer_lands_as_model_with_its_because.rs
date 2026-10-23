//! F0.7 S2/S3 over the wire, scenario 2: a `set_policy` call read off the socket lands on the policy.
#![allow(dead_code)]

#[path = "policy_live_harness.rs"]
mod harness;

use harness::*;
use naivepost::project::{MarkingPass, Origin};

#[test]
fn f0_7_s2_a_live_set_policy_answer_lands_as_model_with_its_because() {
    let fixture = fixture("lands");
    // The model settles one field and quotes the context back, which is what S5's form shows as its reason.
    let llm = FakeLlm::start(vec![set_policy(
        "markingPass",
        "joins",
        "the speaker read a script and restarted after a stumble",
    )]);
    point_at(&llm.url());

    let run = derive_lively(&fixture, &fixture.context);

    assert_eq!(
        run.policy.marking_pass.value,
        MarkingPass::Joins,
        "the value the model called landed: {:?}",
        run.policy.marking_pass
    );
    assert_eq!(
        run.policy.marking_pass.origin,
        Origin::Model,
        "and it arrived with `source: model`, not as an unmarked edit"
    );
    assert_eq!(
        run.policy.marking_pass.because.as_deref(),
        Some("the speaker read a script and restarted after a stumble"),
        "S3 stores the reason beside the value"
    );
    // Read the STORED shape, the way `tests/suite/policy_derive.rs` does: a reason dropped on the way to JSON
    // would pass an in-memory check and fail here.
    let stored = stored(&run.policy, "markingPass");
    assert_eq!(stored["value"].as_str(), Some("joins"), "{stored}");
    assert_eq!(stored["source"].as_str(), Some("model"), "{stored}");
    assert!(
        stored["because"]
            .as_str()
            .unwrap_or_default()
            .contains("read a script"),
        "the reason survives the serialisation: {stored}"
    );
    // The derivation says what it did, and nothing was refused.
    assert!(
        run.derived.refusals.is_empty(),
        "{:?}",
        run.derived.refusals
    );
    assert!(
        run.derived
            .applied
            .contains(&("markingPass", naivepost::policy::Applied::Set)),
        "{:?}",
        run.derived.applied
    );
    // Untouched fields keep their defaults -- the model spoke to one field, so only one moved.
    assert!(
        run.policy.captions_pass.value,
        "P.policy.captionsPass stays at its default"
    );
    // And the answer went into the project cache, so the same context next time costs no request.
    let cache = fixture.tree.dir().join("cache/llm/policy");
    assert!(
        std::fs::read_dir(&cache)
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false),
        "{} holds the answer",
        cache.display()
    );
}
