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
use joined_answer::answer;

#[test]
fn f1_10_s3_a_refused_live_answer_is_asked_once_more_and_the_retry_lands() {
    use naivepost::joins::{self, Outcome};
    use naivepost::seam_retries::{Asking, SEAM_RETRIES};
    use naivepost::tools::textedit::Join;

    let fixture = fixture("retry");
    // Ask #1 goes over the wire and is refused: "today" sits inside the removal with a kept word on
    // either side, which is two stretches, not one at the join.
    let refused = raw("the today the network goes live now");
    point_at(&{
        let llm = FakeLlm::start(vec![refused]);
        // Keep the fake alive for the whole scenario so the ask has somewhere to go.
        let url = llm.url();
        std::mem::forget(llm);
        url
    });
    let lines = press(&fixture.tree, &fixture.project);
    assert!(
        lines.iter().any(|line| line.contains("asking once more")),
        "the refusal says the join gets asked again: {lines:?}"
    );
    // The retry is the second ask of the same seam, in its own slot, and this answer is one stretch
    // at the join: the seam ends repaired rather than refused.
    let mut asking = Asking::start(SEAM_RETRIES);
    let first_tail = asking.refused().expect("one retry is owed after a refusal");
    assert_eq!(first_tail, "asking once more");
    assert_ne!(
        naivepost::seam_retries::ask_slot("base", 0),
        naivepost::seam_retries::ask_slot("base", 1),
        "the retry keys elsewhere than the refusal it is retrying"
    );
    let kept = format!("{} {}", TAKE_A[..2].join(" "), TAKE_B.join(" "));
    let mut join = Join::new(
        &TAKE_A.iter().map(|w| *w).collect::<Vec<&str>>(),
        &TAKE_B.iter().map(|w| *w).collect::<Vec<&str>>(),
    );
    let before: Vec<String> = TAKE_A.iter().map(|w| w.to_string()).collect();
    let after: Vec<String> = TAKE_B.iter().map(|w| w.to_string()).collect();
    match joins::answer_join(&mut join, &answer(&kept), &before, &after) {
        Outcome::Applied { before: gone, after: 0 } => assert_eq!(gone, 3, "three off BEFORE"),
        other => panic!("the retry's answer must be applied, got {other:?}"),
    }
}
