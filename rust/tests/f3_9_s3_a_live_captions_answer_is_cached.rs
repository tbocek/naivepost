//! F3.9 over a socket, the caching side read the other way: a reply that WAS read is kept, and the
//! next identical ask is answered out of that slot without a wire or a second row in `requests.tsv`.
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

/// Whether anything under this step's name holds `needle`.
fn kept(fixture: &Fixture, needle: &str) -> bool {
    let mut stack = vec![cache_dir(fixture)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if std::fs::read_to_string(&path)
                .is_ok_and(|text| text.contains(needle))
            {
                return true;
            }
        }
    }
    false
}

#[test]
fn f3_9_s3_a_live_captions_answer_is_cached_so_the_next_identical_ask_opens_no_socket() {
    let fixture = fixture("cached");
    let clips = vec![(1u32, 10.0), (2u32, 10.0)];
    let answer = caption_json(&[(1, &[(0.5, 3.5, "The network is live")])]);
    // Only ONE scripted reply: were the second ask to reach this server it would be handed the same
    // text, so the test would pass for the wrong reason. The proof it did not is the row count.
    let llm = FakeLlm::start(vec![answer.clone()]);
    point_at(&llm.url());

    let first = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &[], 0, &|| false);
    assert!(!first.failed, "the ask was answered: {:?}", first.reason);
    assert_eq!(first.calls.len(), 1, "one caption came back over the socket");
    assert_eq!(first.calls[0].clip, 1, "and it was clip 1's, as the wire said");
    let _ = llm.next_seen();
    assert!(kept(&fixture, "The network is live"), "a readable answer is kept");
    // The stored bytes are the answer text, not the whole chat envelope: the cache holds what the pass
    // read (`content`), which is what makes a later ask answerable without a socket.
    assert!(!kept(&fixture, "\"choices\""), "the envelope is not what got cached: {:?}", cache_dir(&fixture));

    // Same settings, same prompt, same attempt: the slot answers it, so the fake sees no second
    // request and `requests.tsv` gains no second line.
    let second = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &[], 0, &|| false);
    assert!(
        !second.failed,
        "the cached answer is a whole answer: {:?}",
        second.reason
    );
    assert_eq!(second.calls, first.calls, "the cache handed back the same answer, word for word");
    let rows = read(&fixture.tree.requests_tsv());
    let data_rows = rows.lines().skip(1).filter(|line| !line.trim().is_empty()).count();
    assert_eq!(
        data_rows, 1,
        "the cache hit asked for no wire and so writes no second row: {rows}"
    );
}
