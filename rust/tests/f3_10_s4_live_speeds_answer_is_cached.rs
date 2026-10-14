//! F3.10's caching side over a socket: a speed answer that WAS read is kept, the next identical ask is
//! answered out of that slot with no wire and no second `requests.tsv` row, and the next ROUND is not.
//! One scenario per binary -- see `captions_wire_harness.rs`.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::three_clips;
use naivepost::cut_speed_pass::Call;
use naivepost::speeds_ask;
use serde_json::json;

/// The User Context, model id and first round every ask here uses, so the cache key the test names is the one
/// the program computed.
const CTX: &str = "Speed the dull parts.";
const MODEL: &str = "captions-model";
const ATTEMPT: u32 = 1;

/// Which clips carry a caption: clip 3, with its one caption — `N caption(s) -- runs at 1`, spelled as
/// §F3.10 spells it.
fn captioned() -> Vec<(u32, usize)> {
    vec![(3, 1)]
}

/// The batch the Cut page hands the pass: three 20 s clips at their session seconds, one caption on clip 3.
fn captioned_batch() -> naivepost::tools::clips::Clips {
    let mut batch = naivepost::tools::clips::Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)]);
    batch.add_caption(3, 1.0, 6.0, "a line already on screen");
    batch
}

/// The SPEEDS document §`prompts/system.md`'s `speed` row specifies, in the chat envelope a real server
/// answers with. The cache keeps the ANSWER text (§6 stores what the pass read, not the framing).
fn speeds_json(speeds: &[(u32, f64)]) -> String {
    envelope_of(
        &json!({
            "speeds": speeds.iter().map(|(clip, rate)| json!({ "clip": clip, "rate": rate })).collect::<Vec<_>>()
        })
        .to_string(),
    )
}

/// The file this ask's answer belongs in — named by the program's own keying, not by a second hash recipe.
fn slot(fixture: &Fixture) -> std::path::PathBuf {
    speeds_ask::cache_file_for_test(&fixture.tree, CTX, &three_clips(), &captioned(), MODEL, ATTEMPT)
}

/// Whether anything under this step's cache folder holds `needle`.
fn kept(fixture: &Fixture, needle: &str) -> bool {
    let mut stack = vec![slot(fixture).parent().unwrap().to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if std::fs::read_to_string(&path).is_ok_and(|text| text.contains(needle)) {
                return true;
            }
        }
    }
    false
}

/// The cache half of S4, read the other way: a second identical ask is answered out of the slot it earned,
/// so the socket stays shut and `requests.tsv` gains no row for a call that did not happen.
#[test]
fn f3_10_s4_live_speeds_answer_is_cached_so_the_next_identical_ask_opens_no_socket() {
    let fixture = fixture("s4-cached");
    // ONE scripted reply: were a later ask to reach this server it would be handed the same text and pass for
    // the wrong reason. What proves an ask did NOT go out is that the fake never saw it.
    let llm = FakeLlm::start(vec![speeds_json(&[(2, 3.0)])]);
    point_at(&llm.url());

    let first = speeds_ask::ask(&fixture.tree, CTX, &three_clips(), &captioned(), ATTEMPT, &|| false);
    assert_eq!(first.calls, vec![Call { clip: 2, rate: 3.0 }], "the ask went out and was answered");
    // One request so far, and its shape: the ask really telephoned the address the settings hold.
    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "on the LLM's chat path");

    // §6 keeps what the pass READ — the answer document — so a replay needs no envelope to unwrap and cannot
    // double-count a reply. This is the file the ask's own key names, so the test reads the real slot.
    let kept = std::fs::read_to_string(slot(&fixture)).expect("the answer was cached under its own key");
    assert!(kept.contains("\"speeds\""), "the slot holds the SPEEDS document: {kept}");
    assert!(!kept.contains("\"choices\""), "the framing is not what got cached: {kept}");

    // Same settings, same prompt, same round: the slot answers it. No socket, and no second timed row for a
    // call that did not happen (§09 §10 counts one row per request that went out).
    let second = speeds_ask::ask(&fixture.tree, CTX, &three_clips(), &captioned(), ATTEMPT, &|| false);
    assert_eq!(second.calls, first.calls, "the cached answer is the same rate");
    // Nothing new went out: §09 §10 writes one timed row per REQUEST, so the row count is the wire count, and
    // reading it never waits on a request that must not arrive.
    let rows_after_two = read(&fixture.tree.requests_tsv());
    assert_eq!(
        rows_after_two.lines().filter(|line| line.split('\t').any(|field| field == "speed")).count(),
        1,
        "the identical ask wrote no second row, so it dialled nobody: {rows_after_two}"
    );
    let rows = read(&fixture.tree.requests_tsv());
    assert_eq!(
        rows.lines().filter(|line| line.split('\t').any(|field| field == "speed")).count(),
        1,
        "one timed row for one request: {rows}"
    );

    // A DIFFERENT round of the same cut is a different question and keys elsewhere, so it dials again: §F3.10's
    // second round must not be served the reply that was just refused.
    let retry = speeds_ask::ask(&fixture.tree, CTX, &three_clips(), &captioned(), 2, &|| false);
    assert_eq!(retry.calls, first.calls, "the retry is answered afresh (the fake repeats; the socket proves it)");
    let rows_after_retry = read(&fixture.tree.requests_tsv());
    assert_eq!(
        rows_after_retry.lines().filter(|line| line.split('\t').any(|field| field == "speed")).count(),
        2,
        "the retry really dialled: one row per request that went out\n{rows_after_retry}"
    );
    assert_ne!(slot(&fixture), speeds_ask::cache_file_for_test(&fixture.tree, CTX, &three_clips(), &captioned(), MODEL, 2),
        "and the two rounds really do keep separate slots");
}
