//! F6.2 §3's repair, second live scenario: the once-only rule over a wire.
//!
//! Split from `tests/llm_repair_f6_2.rs` because the harness is one-scenario-per-binary — `XDG_CONFIG_HOME`
//! and the working directory are process-wide, so a second fixture in the same target would read whichever
//! environment the other set last. Same fake, same door; only the script differs.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;

use naivepost::captions_ask;

/// §3's short retry is a real 2 s wait; the scale seam (`llm_retry_leg::nap_duration`) divides it so the
/// check sits through it in milliseconds. The log still names the seconds §3 spells.
fn init() {
    unsafe { std::env::set_var("NAIVEPOST_RETRY_SCALE", "1000") };
}

/// §3's repair is FIRST failure only. A server that answers with reasoning however often it is asked costs
/// two requests and then reports the step — it does not settle into a repair loop, and it does not get
/// confused with §F3.9's own out-of-batch retries, which count a different failure entirely.
///
/// One scenario per binary is the harness's rule (`XDG_CONFIG_HOME` and cwd are process-wide), so this is a
/// separate test TARGET sharing this file by include, not a second `#[test]` here.
#[test]
fn f6_2_s3_e_a_second_unusable_answer_ends_the_step_without_another_repair() {
    init();
    let fixture = fixture("repair_once");
    let (clips, words) = two_clips_with_words();
    // `content` empty, reasoning in its own field: §3's `noAnswer`, which is the only answer shape worth a
    // repair. A non-empty malformed answer is a different failure (the pass's own complaint about the ANSWER)
    // and correctly costs no extra request at all.
    let stubborn = "{\"choices\":[{\"message\":{\"reasoning_content\":\"still thinking\",\"content\":\"\"}}]}".to_string();
    let llm = FakeLlm::start(vec![stubborn.clone(), stubborn]);
    point_at(&llm.url());

    let asked = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &words, 0, &|| false);
    assert!(asked.failed, "two unusable answers are the step's failure, not a loop");
    assert!(asked.calls.is_empty(), "nothing was placed: {:?}", asked.calls);
    let seen = llm.drain();
    assert_eq!(seen.len(), 2, "the repair was spent once and not again: {}", seen.len());
    // And the reason still says what was wrong with the answer, so §F3.9's log line names a cause — and says
    // it was asked again, which is the whole difference between this failure and a plain one.
    assert!(
        asked.reason.contains("asked twice"),
        "the step says the repair was spent: {}",
        asked.reason
    );
    assert!(
        !asked.reason.is_empty(),
        "and never fails without a reason"
    );
    // §3's repair says so in the log, once per repair — the prototype logged nothing here, which is why a
    // repaired batch looked identical to a normal one. The wording has a home, so it gets pinned.
    assert!(
        naivepost::llm_repair_leg::repair_log("Captions", "why").contains("asked again"),
        "the repair line names what happened: {}",
        naivepost::llm_repair_leg::repair_log("Captions", "why")
    );
}
