//! F3.9 S1/S2 over a socket, across a whole run: five kept clips go out as ONE batch and fifteen go
//! out as three, each request asking about its own five and no others. Settings re-read per request:
//! the second batch dials the address that is in the file when it starts, not the one the first used.
//! One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide).

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::captions_ask;

#[test]
fn f3_9_s1_a_fifteen_clip_run_asks_three_batches_over_the_wire_and_re_reads_the_address_each_time() {
    let fixture = fixture("batches");
    // Every kept clip of this run, ten seconds each, so the batch ranges are (1–5) (6–10) (11–15).
    let all: Vec<(u32, f64)> = (1..=15).map(|n| (n, 10.0)).collect();
    let ranges = cut_captions_ranges(all.len());
    assert_eq!(
        ranges,
        vec![(1, 5), (6, 10), (11, 15)],
        "P.machine.captionBatch is 5, so fifteen clips are three asks"
    );

    // First server: answers every batch with an empty list -- a whole answer, not a failure.
    let empty = FakeLlm::start(vec![caption_json(&[])]);
    point_at(&empty.url());
    let first_batch = &all[0..5];
    let asked = captions_ask::ask(&fixture.tree, "Caption nothing in these.", first_batch, &[], 1, &|| false);
    assert!(
        !asked.failed && asked.calls.is_empty(),
        "`{{clips:[]}}` is the prompt's own no-caption answer, not a wire failure: {:?}",
        asked.reason
    );
    let seen = empty.next_seen();
    assert!(
        seen.body.contains("CLIP 1: 10.0 s long") && seen.body.contains("CLIP 5: 10.0 s long"),
        "batch one carried clips 1..5"
    );
    assert!(
        !seen.body.contains("CLIP 6:"),
        "and nothing past its own range: the model is never shown the whole session"
    );

    // Second batch, asked after the settings were rewritten: it must dial the NEW address.
    let second = FakeLlm::start(vec![caption_json(&[])]);
    point_at(&second.url());
    let second_batch = &all[5..10];
    let asked = captions_ask::ask(&fixture.tree, "Caption nothing in these.", second_batch, &[], 1, &|| false);
    assert!(!asked.failed, "the second batch reached the new address: {:?}", asked.reason);
    let seen = second.next_seen();
    assert!(
        seen.body.contains("CLIP 6: 10.0 s long") && seen.body.contains("CLIP 10: 10.0 s long"),
        "batch two carried clips 6..10"
    );
    assert!(
        !seen.body.contains("CLIP 5:"),
        "the previous batch's clips did not ride along"
    );

    // requests.tsv holds one timed row per ask that went out -- two here, both captions, both LLM.
    let rows = read(&fixture.tree.requests_tsv());
    let data_rows: Vec<&str> = rows.lines().skip(1).filter(|line| !line.trim().is_empty()).collect();
    assert_eq!(
        data_rows.len(),
        2,
        "one row per live request, no more: {rows}"
    );
    for row in &data_rows {
        let fields: Vec<&str> = row.split('\t').collect();
        assert_eq!(fields[3], "captions", "job column: {row}");
        assert_eq!(fields[4], "llm", "service column: {row}");
        assert_eq!(fields[6], "chat", "kind column: {row}");
        assert_eq!(fields[16], "ok", "outcome column: {row}");
        // Timed: the wire fields are filled, so the row is the run's own stopwatch (§09 §10 -- how long
        // until the first byte, and how long on the wire in all). What went out in BYTES is left empty:
        // the leg measures the seconds and hands the byte count to a caller that counts it.
        assert!(
            !fields[13].is_empty(),
            "first_byte_s filled for a request that really answered: {row}"
        );
        assert!(
            !fields[14].is_empty(),
            "on_wire_s filled for a request that really was on the wire: {row}"
        );
    }
}

/// `cut_captions::batches` behind a name this file reads as §F3.9's S1.
fn cut_captions_ranges(count: usize) -> Vec<(u32, u32)> {
    naivepost::cut_captions::batches(count)
}
