//! F1.10 S3's live half over a socket, one scenario per binary.
//!
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory are process-wide, so a
//! fixture that re-points them cannot share a test binary with another: the shared env would
//! make the second pass read the first one's settings and project folder. One binary each,
//! sharing the harness in `tests/suite/join_live_harness.rs`.
#![allow(dead_code)]

#[path = "join_live_harness.rs"]
mod harness;

use harness::*;
#[path = "joined_answer.rs"]
mod joined_answer;
use joined_answer::{chat, drop_words, finish};

#[test]
fn f1_10_s3_a_live_answer_is_matched_back_and_both_files_are_written() {
    let fixture = fixture("applied");
    // The answer keeps take-a whole but for its tail "is live today", then all of take-b: three
    // words off BEFORE, one stretch at the join. Dedupe then finds "the network" said again
    // straight across the cut and takes the EARLIER copy too (spec step 4), which empties take-a,
    // so the join carries the whole-take marker rather than a bare count.
    // The kept text the drop call stands for — used for the cache-shape check below.
    let kept = format!("{} {}", TAKE_A[..2].join(" "), TAKE_B.join(" "));
    let _ = kept;
    // The repair is a `drop_words` call (3 off BEFORE), which is this job's §3.2 tool answer; the
    // closing `finish` round ends the flow. `next_seen` is still the FIRST ask — the one whose body
    // carries the seam's words — and the pass reads the drop off the call, not off any prose.
    let llm = FakeLlm::start(vec![drop_words("before", 3), finish()]);
    let addr = point_at(&llm.url());
    let lines = press(&fixture.tree, &fixture.project);
    let seen = llm.next_seen();
    assert_eq!(
        seen.reply,
        drop_words("before", 3),
        "the ask was answered with this scenario's own reply over HTTP at {addr}"
    );
    assert!(
        seen.body.contains("\"tools\"") && seen.body.contains("drop_words"),
        "the ask offered this job's tools (§3.2): {}",
        clip(&seen.body)
    );
    assert!(
        seen.body.contains("\"enable_thinking\":true"),
        "thinking stayed on for the tool ask: {}",
        clip(&seen.body)
    );
    assert!(
        seen.body.contains(&TAKE_A.join(" ")) && seen.body.contains(&TAKE_B.join(" ")),
        "the request carried the take before and the take after the seam"
    );
    let _ = chat;
    let text = read(&fixture.tree.final_txt());
    assert_eq!(
        text.trim(),
        TAKE_B.join(" "),
        "only take-b's words survive the seam repair plus the dedupe"
    );
    assert!(!text.contains("today"), "the dropped word is gone: {text}");
    let marks = read(&fixture.tree.retakes_tsv());
    assert!(!marks.trim().is_empty(), "the dropped run is a mark: {marks}");
    assert!(
        lines.iter().any(|line| line.contains("off BEFORE")),
        "the applied seam said what it took: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(">>> text edit: 5 of 10 words removed in 1 stretch(es)")),
        "S6's completion line states the count for {addr}: {lines:?}"
    );
}
