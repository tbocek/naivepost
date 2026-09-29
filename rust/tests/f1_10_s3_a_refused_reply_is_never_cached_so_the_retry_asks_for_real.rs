//! F1.10 S3's live half over a socket, one scenario per binary.
//!
//! `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the working directory are process-wide, so a
//! fixture that re-points them cannot share a test binary with another: the shared env would
//! make the second pass read the first one's settings and project folder. One binary each,
//! sharing the harness in `tests/join_live_harness.rs`.
#![allow(dead_code)]

#[path = "join_live_harness.rs"]
mod harness;

use harness::*;
#[path = "joined_answer.rs"]
mod joined_answer;
use joined_answer::{chat, raw};

#[test]
fn f1_10_s3_a_refused_reply_is_never_cached_so_the_retry_asks_for_real() {
    // Two stretches left out ("today" sits inside the removal with "the" kept on either side), so
    // this reply is refused. Served over the wire, twice, because a retry must not be answered out
    // of the slot the first refusal was refused from.
    let refused = raw("the today the network goes live now");
    let fixture = fixture("nocache");
    let llm = FakeLlm::start(vec![refused.clone(), refused]);
    point_at(&llm.url());
    let _ = press(&fixture.tree, &fixture.project);
    let arrived = 1 + llm.drained();
    assert!(
        arrived >= 2,
        "the refused reply was not served back from cache: {arrived} request(s) arrived"
    );
    let cache = fixture.root.join("session.naivepost/cache/llm/textedit");
    let stored = std::fs::read_dir(&cache).map(|d| d.count()).unwrap_or(0);
    assert_eq!(stored, 0, "no cache file left behind by a refused answer");
}
