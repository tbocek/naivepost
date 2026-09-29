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
fn f1_10_s3_settings_are_re_read_per_request() {
    // Ask once over a live address, then change the settings and ask again: the SECOND ask must go
    // to the address that is in the file NOW (§02-services#1: settings are re-read per request).
    // The first press is answered by a refusal rather than an applied repair on purpose -- an
    // applied answer would end the seam, and there would be nothing left to ask about the second time.
    let fixture = fixture("reread");
    let live = FakeLlm::start(vec![raw("not an answer at all")]);
    point_at(&live.url());
    let first = press(&fixture.tree, &fixture.project);
    let _ = live.next_seen();
    assert!(
        first.iter().any(|line| line.contains("asking once more")),
        "the first ask went out and was refused: {first:?}"
    );
    let dead = FakeLlm::dead_port();
    point_at(&format!("http://127.0.0.1:{dead}"));
    let lines = press(&fixture.tree, &fixture.project);
    assert!(
        lines
            .iter()
            .any(|line| line.contains(&dead.to_string()) && line.contains("not answering")),
        "the second ask dialled the address now in the settings: {lines:?}"
    );
}
