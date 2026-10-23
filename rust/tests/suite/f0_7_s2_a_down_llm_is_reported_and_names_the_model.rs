//! F0.7 S2 over the wire, scenario 4: a server that is not there gives an error, not a policy.
//!
//! Nothing may be invented here: with no model answering, every field keeps its §2 default and the status
//! line says which model was missing (§00-principles: failure is specific and local). The address is port
//! 1 on loopback -- privileged, so nothing here can bind it and nothing does, which means a dial to it is
//! refused the instant it is attempted. A held-but-unserved listener would NOT do: `connect` succeeds
//! against a listening socket's backlog, and this leg has no read timeout, so the call would sit waiting
//! for a reply that never comes.
#![allow(dead_code)]

#[path = "policy_live_harness.rs"]
mod harness;

use harness::*;
use naivepost::project::{CutMode, MarkingPass, Origin};

/// Port 1 is reserved and privileged: binding it needs root, and this runs as `dev`. It is therefore the
/// one loopback address that is guaranteed to refuse rather than accept-and-go-silent.
const NOTHING_THERE: &str = "http://127.0.0.1:1";

#[test]
fn f0_7_s2_a_down_llm_is_reported_and_names_the_model() {
    // Shorten §3's short patience through the seam `llm_retry_leg` itself reads, so the retry costs
    // milliseconds instead of seconds. The log still names the seconds the spec spells.
    scale_the_patience();
    let fixture = fixture("down");
    point_at(NOTHING_THERE);

    let run = derive_lively(&fixture, &fixture.context);

    // Every field stands at its default -- no half-derived policy, no invented style.
    assert_eq!(run.policy.marking_pass.value, MarkingPass::Retakes); // P.policy.markingPass
    assert_eq!(run.policy.cut_mode.value, CutMode::Model); // P.policy.cutMode
    for origin in [
        run.policy.marking_pass.origin,
        run.policy.cut_mode.origin,
        run.policy.captions_pass.origin,
        run.policy.speed_pass.origin,
        run.policy.decorations_pass.origin,
    ] {
        assert_eq!(
            origin,
            Origin::Default,
            "nothing claimed to have been derived: {origin:?}"
        );
    }
    // And the failure is said, naming the model it could not reach -- ONCE. One dead server is one fault,
    // so the derivation asks about it once rather than re-dialling per round.
    assert_eq!(run.derived.applied.len(), 0, "{:?}", run.derived.applied);
    assert_eq!(run.derived.refusals.len(), 1, "{:?}", run.derived.refusals);
    let said = &run.derived.refusals[0];
    assert!(
        said.contains("policy-model"),
        "the sentence names the model: {said}"
    );
    assert!(
        said.contains("not answering") || said.contains("no answer"),
        "and the fault: {said}"
    );
    // The status line is what a person reads: it carries that sentence rather than a generic failure.
    assert_eq!(
        run.derived.status(),
        *said,
        "the status line says who was missing"
    );
    // Each attempt left its row: the failed dial is visible in the request log, not silently retried.
    // What the row holds is the address that refused and the OS's own words for it, so those are what is
    // checked -- not a paraphrase of the reason.
    let tsv = read(&fixture.tree.requests_tsv());
    assert!(tsv.contains("policy"), "the ask is logged: {tsv}");
    assert!(
        tsv.contains("127.0.0.1:1"),
        "naming the address it tried: {tsv}"
    );
    assert!(tsv.contains("refused"), "with the fault it met: {tsv}");
}
