//! F3.9 S4 over a socket: a clip number outside this batch loses the WHOLE reply, the retry goes out
//! for real (it reaches the wire again), and if the second answer is clean the captions land.
//! One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide).

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::captions_ask;
use naivepost::cut_captions::{self, Reply};

/// The batch this scenario argues about: clips 6 to 10, ten seconds each, so "clip 3" is a clip that
/// exists somewhere but not here.
fn batch() -> Vec<(u32, f64)> {
    (6..=10).map(|n| (n, 10.0)).collect()
}

#[test]
fn f3_9_s4_a_live_out_of_batch_clip_is_asked_once_more_and_the_retry_lands() {
    let fixture = fixture("retry");
    let clips = batch();
    // First answer names a clip outside the batch; the second stays inside it.
    let bad = caption_json(&[(3, &[(1.0, 4.0, "a caption on a clip that is not here")])]);
    let good = caption_json(&[(7, &[(1.0, 4.0, "a caption on clip seven")])]);
    let llm = FakeLlm::start(vec![bad, good]);
    point_at(&llm.url());

    let first = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &[], 1, &|| false);
    assert!(!first.failed, "the reply came back over the wire: {:?}", first.reason);
    match cut_captions::place(&clips, &first.calls) {
        Reply::Rejected(problem) => assert_eq!(
            problem,
            "clip 3 is not one of the clips given (6 to 10)",
            "the refusal names the clip and the batch's own range"
        ),
        Reply::Accepted(fx) => panic!("an out-of-batch clip must reject the whole reply, got {fx:?}"),
    }
    let refused = llm.next_seen();
    assert!(
        refused.body.contains("CLIP 6: 10.0 s long") && refused.body.contains("CLIP 10: 10.0 s long"),
        "the ask carried this batch's five clips"
    );

    // The retry: attempt 2 goes out AGAIN over the same seam, and a clean answer survives placement.
    let second = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &[], 2, &|| false);
    assert!(!second.failed, "the retry reached the server too: {:?}", second.reason);
    let retry = llm.next_seen();
    assert_eq!(retry.method, "POST", "the retry was a real request, not a re-read of the first");
    match cut_captions::place(&clips, &second.calls) {
        Reply::Accepted(fx) => {
            assert_eq!(fx.len(), 1, "the retry placed exactly the caption it asked for");
            assert_eq!(fx[0].kind, "text", "a caption is a text record, proposed or typed");
            // `session_start` sums the lengths BEFORE the caption's own clip. The batch handed to
            // `place` carries clips 6 to 10 and nothing before them, so clip 7 sits at 10.0 (clip 6's
            // length) plus the 1.0 offset the model gave = 11.0. A batch is a window on the session,
            // not the whole of it; a caller that knows the real positions goes through
            // `on_the_timeline_at` instead of trusting the sum (§F3.9's own note on a sped-up clip).
            assert!( (fx[0].t - 11.0).abs() < 1e-9, "session second computed here: {}", fx[0].t);
        }
        Reply::Rejected(problem) => panic!("the retry's answer was inside the batch: {problem}"),
    }
    assert!(
        cut_captions::retries(1) && !cut_captions::retries(2),
        "one retry owed after one rejection, none after two"
    );
}
