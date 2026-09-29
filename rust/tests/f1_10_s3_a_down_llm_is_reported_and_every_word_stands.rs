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
    let dead = FakeLlm::dead_port();
    point_at(&format!("http://127.0.0.1:{dead}"));
    let fixture = fixture("down");
    let lines = press(&fixture.tree, &fixture.project);
    assert!(
        lines.iter().any(|line| line.contains("llm") && line.contains("not answering")),
        "the failure names the server that is down: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("nothing removed there")),
        "the join ends with nothing removed: {lines:?}"
    );
    // No retry storm: a server that is not there answers the same way however often it is called,
    // and seam_retries counts refusals of ANSWERS, not failures of the wire.
    let rows = read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| line.contains("chat"))
        .count();
    assert_eq!(rows, 1, "one ask, not a retry storm");
    let text = read(&fixture.tree.final_txt());
    assert!(
        text.contains("today") && text.contains("now"),
        "a down server takes no words out: {text}"
    );
    assert!(fixture.tree.retakes_tsv().exists(), "retakes.tsv still written");
}
