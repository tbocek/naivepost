//! F3.9 S4/S6 over a socket, the caching side: a reply that could not be read is never cached, so the
//! retry goes to the wire instead of being replayed from the slot it failed in.
//! One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide) -- see the harness.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::captions_ask;

/// The cache folder this step writes into, keyed on the exact request (§09 §4: a changed prompt misses).
fn cache_dir(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("session.naivepost/cache/llm/captions")
}

/// Whether anything under this step's name holds `needle`. Walked rather than counted: the claim is
/// about THIS scenario's answer, not about how many slots the folder happens to hold.
fn kept(fixture: &Fixture, needle: &str) -> bool {
    let mut stack = vec![cache_dir(fixture)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
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

#[test]
fn f3_9_s4_a_live_unreadable_captions_reply_is_never_cached_so_the_retry_asks_for_real() {
    let fixture = fixture("unreadable");
    let clips = vec![(1u32, 10.0), (2u32, 10.0)];
    // Not a call at all: a JSON document, which §3's answer on this item makes UNREADABLE for this pass. Every
    // ask gets the same unreadable answer, so if the first refusal were cached the retry would never reach the
    // fake server.
    let junk = envelope_of("{\"not\":\"the captions shape\"}");
    let llm = FakeLlm::start(vec![junk.clone(), junk]);
    point_at(&llm.url());

    let first = captions_ask::ask(
        &fixture.tree,
        "Caption every spoken line.",
        &clips,
        &[],
        0,
        &|| false,
    );
    assert!(
        first.failed,
        "an unreadable answer is a failure, not an empty request"
    );
    assert_eq!(
        first.reason,
        naivepost::clip_rules_tools_leg::no_calls_reason("captions", "add_caption"),
        "the reason names the step and the call the model never made"
    );
    // The prompt-shape rule behind this refusal still stands, even though the WIRE path no longer parses a
    // document: a `{}`-with-no-`clips` list is unreadable, not an empty answer. Reached now only from the cache
    // reader (a slot written before calls were the answer) and from a server that refused the `tools` field.
    assert!(
        captions_ask::parse_captions("{\"not\":\"the captions shape\"}").is_err(),
        "a document with no \"clips\" list is still not a whole answer"
    );
    let first_ask = llm.next_seen();
    assert_eq!(first_ask.method, "POST", "the ask really went out");
    assert!(
        !kept(&fixture, "captions shape"),
        "an unreadable answer leaves nothing in the cache"
    );

    // The retry: same junk, and it must be READ live again rather than replayed from the failed slot.
    let second = captions_ask::ask(
        &fixture.tree,
        "Caption every spoken line.",
        &clips,
        &[],
        1,
        &|| false,
    );
    assert!(
        second.failed,
        "the retry was refused too, as it must have been read live"
    );
    assert_eq!(
        second.reason,
        naivepost::clip_rules_tools_leg::no_calls_reason("captions", "add_caption"),
        "and by the same rule: {}",
        second.reason
    );
    let retry = llm.next_seen();
    assert_eq!(
        retry.method, "POST",
        "the retry went out over the wire rather than being replayed from the failed slot"
    );
    assert!(
        !kept(&fixture, "captions shape"),
        "still nothing cached from two refused answers"
    );
    // Two refused asks, and a row for every request the run made -- §09 §10 counts REQUESTS, not answers. Each
    // ask costs two of them here: the ask itself, then the one repair round §3's content repair owes an answer it
    // cannot use (the fake answers both with the same junk, which is why the retry too ends in a refusal).
    let rows = read(&fixture.tree.requests_tsv());
    let data_rows = rows
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .count();
    assert_eq!(data_rows, 4, "two asks, each with its repair round: {rows}");
}
