//! F3.10 S4/S5 over a socket: two adjoining clips at one rate become ONE stretch, and the success line says
//! so with the spec's own bytes. One scenario per binary — see `captions_wire_harness.rs`.
//!
//! The merge is arithmetic the model must not be asked to do: it names a rate per clip, and the app folds same-rate
//! stretches nearer than P.policy.speedGapSeconds into one run so the render gets one ramp pair instead of two a
//! hair apart (which is what once came back as two ×4 runs a second apart — a stutter on screen, two badges on
//! the lane).

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::three_clips;
use naivepost::cut_speed_pass::{self, Call};
use naivepost::params;
use naivepost::speeds_ask;
use serde_json::json;

/// The SPEEDS document §`prompts/system.md`'s `speed` row specifies, in the chat envelope a real server
/// answers with, so the ask reads it through the same parser a live reply goes through.
fn speeds_json(speeds: &[(u32, f64)]) -> String {
    envelope_of(
        &json!({
            "speeds": speeds.iter().map(|(clip, rate)| json!({ "clip": clip, "rate": rate })).collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// No caption anywhere: both rates are ones the tool will apply, so the merge is the only thing left to test.
fn captioned() -> Vec<(u32, usize)> {
    Vec::new()
}

/// The batch as the Cut page hands it: three 20 s clips at 0, 20 and 40.
fn batch() -> naivepost::tools::clips::Clips {
    naivepost::tools::clips::Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)])
}

#[test]
fn f3_10_s5_live_speeds_same_rate_stretches_merge_and_the_line_is_said() {
    let fixture = fixture("s5-merged");
    // The same rate twice, on clips that touch: one stretch of 40 s, not two of 20 s.
    let llm = FakeLlm::start(vec![speeds_json(&[(1, 2.0), (2, 2.0)])]);
    point_at(&llm.url());

    let asked = speeds_ask::ask(&fixture.tree, "Run the waits fast.", &three_clips(), &captioned(), 1, &|| false);
    assert!(!asked.failed, "the ask was answered: {:?}", asked.reason);
    assert_eq!(asked.calls, vec![Call { clip: 1, rate: 2.0 }, Call { clip: 2, rate: 2.0 }]);

    // The gap the merge will use is the gap §10 catalogues — read from §05 §6, where this section's rows live.
    let gap = params::cut()
        .into_iter()
        .find(|row| row.id == "P.policy.speedGapSeconds")
        .expect("the gap is catalogued");
    assert_eq!(gap.spelled, "4", "// P.policy.speedGapSeconds");
    // The bound is "nearer than", and two clips that adjoin leave 0 s between them — inside it by definition,
    // which is what the merge below acts on rather than any number this test could restate.

    let clips = batch();
    let reply = cut_speed_pass::place(&clips, &asked.calls);
    assert!(reply.usable(), "a clean reply is usable: {:?}", reply.fault);
    assert_eq!(reply.effects.len(), 1, "two same-rate adjoining stretches merged into one, got {:?}", reply.effects);
    let merged = &reply.effects[0];
    assert_eq!(merged.kind, "speed", "a proposed speed is a speed effect");
    assert_eq!(merged.t, 0.0, "the stretch starts at clip 1's own session second");
    assert_eq!(merged.dur, 40.0, "and covers both clips: 20 + 20");
    assert_eq!(merged.rate, 2.0, "at the rate the tool reported applying");

    // S5's line: N counts CLIPS running fast, judged at each clip's middle, so a merged stretch still reads
    // as the two clips it sped. Em dash U+2014, spelled by the module rather than copied here.
    let footage = cut_speed_pass::footage_total(&clips);
    assert_eq!(footage, 60.0, "the footage total is the clips' own seconds before any rate shortens them");
    let said = cut_speed_pass::runs_fast(&clips, &reply.effects, footage);
    assert!(
        said.starts_with(">>> speed: 2 clip(s) run fast \u{2014} "),
        "the success line, em dash and all, got {said:?}"
    );
    assert!(said.contains("of video from") && said.contains("of footage"), "both clocks are named: {said:?}");
    // The seconds the line claims, derived from the app's own rate rule rather than typed in: each clip's own
    // length divided by the rate covering its middle. Clips 1 and 2 fall under the merged ×2 (10 s on screen
    // each); clip 3 keeps its 20 because no rate was asked for there. 40 s out of 60, padded as §F3.10 prints it.
    let stretched = cut_speed_pass::merge(&reply.effects);
    assert_eq!(stretched.len(), 1, "one stretch, read back through the merge rule itself");
    let on_screen: f64 = clips
        .clips()
        .iter()
        .map(|clip| {
            let rate = naivepost::cut_speed::rate_at(&stretched, clip.start + clip.length / 2.0);
            clip.length / rate
        })
        .sum::<f64>();
    assert_eq!(on_screen, 40.0, "two sped clips at 10 s each plus the untouched third");
    assert!(said.contains("00:40") && said.contains("01:00"), "the arithmetic the line reports: {said:?}");
}
