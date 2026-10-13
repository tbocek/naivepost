//! F3.11 S2 over a socket: a reply naming a clip outside the list costs the WHOLE reply, is said back, and
//! owes exactly one retry — never a third ask. One scenario per binary (`XDG_CONFIG_HOME`/cwd are
//! process-wide) — see `captions_wire_harness.rs`.
//!
//! The page's own door drives it (`ui::run_effects_pass`), so the rejection travels the route a ✨ Decorations
//! press takes: telephone → [`naivepost::cut_effects_pass::place`] → the status row and the log.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::effects_ask;
use naivepost::tools::clips::Clips;
use serde_json::json;

/// The User Context every ask here uses.
const CTX: &str = "Punch in on the score, hold the ending.";

/// Three clips of 20 s each at their session seconds — the batch the page would build from three kept segments.
fn batch() -> Clips {
    Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)])
}

/// The EFFECTS document in the chat envelope a real server answers with.
fn effects_json(entries: &[serde_json::Value]) -> String {
    envelope_of(&json!({ "fx": entries }).to_string())
}

fn fx(clip: u32, kind: &str, start: f64, end: f64) -> serde_json::Value {
    json!({"clip": clip, "kind": kind, "start": start, "end": end})
}

/// How many asks this fake has answered, counted by timed rows: §09 §10 writes one per REQUEST, so the count
/// is the wire count and reading it never waits on a request that must not arrive.
fn asked(fixture: &Fixture) -> usize {
    read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| line.split('\t').any(|field| field == "effects"))
        .count()
}

#[test]
fn f3_11_s2_live_effects_reply_naming_a_clip_outside_the_list_loses_the_whole_reply_and_retries() {
    let fixture = fixture("f311-s2-outside");
    // Round one names clip 9 of a three-clip batch beside a decoration that would have been fine; round two
    // answers about a clip that exists. Both replies are scripted, so the second one can only arrive if the
    // pass really dialled again.
    let llm = FakeLlm::start(vec![
        effects_json(&[fx(2, "zoom", 1.0, 4.0), fx(9, "stop", 2.0, 4.0)]),
        effects_json(&[fx(2, "zoom", 1.0, 4.0)]),
    ]);
    point_at(&llm.url());

    let batch = batch();
    let first = effects_ask::ask(&fixture.tree, CTX, &batch, &[], 1, &|| false);
    assert_eq!(first.calls.len(), 2, "both calls came back: the pass, not the reader, decides");
    assert_eq!(asked(&fixture), 1, "one request for one round");

    // S2 (`a clip outside the list rejects the whole reply`). Nothing is placed, and the sentence is the spec's
    // own, spelled by `cut_captions::clip_outside` so all three passes phrase it alike.
    let rejected = naivepost::cut_effects_pass::place(&batch, &first.calls);
    assert!(!rejected.usable(), "clip 9 makes the whole reply unusable");
    assert!(rejected.effects.is_empty(), "and NOTHING from it lands, not even the good zoom");
    let problem = rejected.fault.clone().expect("the fault is named");
    assert_eq!(problem, "clip 9 is not one of the clips given (1 to 3)", "the spec's wording, with the range");
    assert_eq!(
        naivepost::cut_effects_pass::rejected(&problem),
        ">>> effects rejected: clip 9 is not one of the clips given (1 to 3)",
        "the reason is said back before the call is put again"
    );

    // S4's retry budget: one more round, and a third never happens. Each round asks afresh, so the second ask
    // keys elsewhere in the cache rather than reading back what was just rejected.
    let second = effects_ask::ask(&fixture.tree, CTX, &batch, &[], 2, &|| false);
    assert_eq!(second.calls.len(), 1, "the retry got its own answer");
    assert_eq!(asked(&fixture), 2, "two rounds, two requests: {CTX}");
    let placed = naivepost::cut_effects_pass::place(&batch, &second.calls);
    assert!(placed.usable(), "the retried reply is usable");
    assert_eq!(placed.effects.len(), 1, "and it places the zoom the first reply also wanted");
    assert_eq!(placed.effects[0].kind, "zoom");

    // The give-up line exists for a reply that stays unusable after both rounds — see
    // `f3_11_s5_live_effects_two_rounds_then_the_give_up_line`; here the second round answered, so the pass
    // would report what landed rather than the failure.
    assert_eq!(naivepost::cut_effects_pass::placed(1), ">>> effects: 1 decoration(s)");
}
