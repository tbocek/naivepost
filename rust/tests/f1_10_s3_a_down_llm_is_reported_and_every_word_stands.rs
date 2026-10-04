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
fn f1_10_s3_a_down_llm_is_reported_and_every_word_stands() {
    // The listener is dropped BEFORE the pass runs, so every dial meets a clean connection REFUSED: with it
    // held open, the first dial would CONNECT into its full accept queue and fail on the read instead, and
    // which of the two shapes wins depends on how fast that read times out against §3's 2 s wait -- which
    // patiences apply (§3's classification keys on transport death) should not be a race. A port released
    // here could in principle be taken by another process between the drop and the asks; if that happened
    // the failure is still a transport error at the same address, so the count below stays bounded either way.
    let dead = harness::FakeLlm::dead_listener();
    let dead_port = dead.local_addr().expect("the dead port has an address").port();
    point_at(&format!("http://127.0.0.1:{dead_port}"));
    drop(dead);
    let fixture = fixture("down");
    let lines = press(&fixture.tree, &fixture.project);
    // The count is ONE failed seam (2 rows) rather than one per seam because a transport death on a server
    // that has never answered spends no ladder at all, and the pass stops asking once it has said the
    // failure: §3's one retry, and `seam_retries` spends none of its retries on a wire that never answered.
    assert!(
        lines.iter().any(|line| line.contains("llm") && line.contains("not answering")),
        "the failure names the server that is down: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("nothing removed there")),
        "the join ends with nothing removed: {lines:?}"
    );
    // §3 (F6.2) gives this branch ONE retry after 2 s, so the wire is dialled TWICE for this seam and never a
    // third: a server this app has never had an answer from is not "gone away mid-call", so no rung of the 5 s-to-4
    // min ladder is spent on a box nobody started. `seam_retries` counts refusals of ANSWERS and spends none of its
    // retries on a dead wire, which is what keeps this bounded.
    let rows = read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| line.contains("chat"))
        .count();
    assert_eq!(rows, 2, "§3's short patience is one retry after 2 s: two asks, no ladder, no storm: {rows}");
    let text = read(&fixture.tree.final_txt());
    assert!(
        text.contains("today") && text.contains("now"),
        "a down server takes no words out: {text}"
    );
    assert!(fixture.tree.retakes_tsv().exists(), "retakes.tsv still written");
}
