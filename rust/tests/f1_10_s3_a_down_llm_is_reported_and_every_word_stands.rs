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
    // §3 (F6.2) gives a call that died in transit its one short retry, so the wire is now dialled THREE times for
    // this seam (the first ask and its seam_retries re-ask, one of them retried after 2 s) -- three only: the
    // ladder is for a mid-call death (§3's own line says "went away mid-call") and this endpoint has never
    // answered, so no 5 s-to-4 min wait is spent on a box nobody started. `seam_retries` still counts refusals of
    // ANSWERS and spends none of its retries on a dead wire, which is what keeps this bounded.
    let rows = read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| line.contains("chat"))
        .count();
    assert_eq!(rows, 3, "two asks of the seam, one of them retried once after 2 s -- no ladder, no storm: {rows}");
    let text = read(&fixture.tree.final_txt());
    assert!(
        text.contains("today") && text.contains("now"),
        "a down server takes no words out: {text}"
    );
    assert!(fixture.tree.retakes_tsv().exists(), "retakes.tsv still written");
}
