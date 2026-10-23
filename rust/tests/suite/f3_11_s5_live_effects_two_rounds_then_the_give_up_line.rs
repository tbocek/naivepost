//! F3.11 S4/S5 over a socket: two rounds that both come back unusable, and then the spec's own give-up line —
//! exactly two requests, never a third. One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide) —
//! see `captions_wire_harness.rs`.

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::cut_effects_pass;
use naivepost::effects_ask;
use naivepost::tools::clips::Clips;
use serde_json::json;

const CTX: &str = "Punch in on the score, hold the ending.";

/// Three clips of 20 s each at their session seconds.
fn batch() -> Clips {
    Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)])
}

fn effects_json(entries: &[serde_json::Value]) -> String {
    add_effects_json(entries)
}

fn fx(clip: u32, kind: &str, start: f64, end: f64) -> serde_json::Value {
    json!({"clip": clip, "kind": kind, "start": start, "end": end})
}

fn asked(fixture: &Fixture) -> usize {
    read(&fixture.tree.requests_tsv())
        .lines()
        .filter(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            fields.contains(&"effects") && !fields.contains(&"cache")
        })
        .count()
}

#[test]
fn f3_11_s5_live_effects_two_rounds_then_the_give_up_line_is_said() {
    let fixture = fixture("f311-s5-give-up");
    // ONE scripted reply, and it stays unusable however often it is handed out: a clip number outside the
    // batch. The fake repeats its last reply, so a third ask would be answered too — which is exactly why the
    // timed-row count, not the answer text, is the proof that no third ask happened.
    let llm = FakeLlm::start(vec![effects_json(&[fx(7, "zoom", 1.0, 4.0)])]);
    point_at(&llm.url());

    let batch = batch();
    let mut placed: Option<Vec<naivepost::cut::Fx>> = None;
    let mut said: Vec<String> = Vec::new();
    for attempt in 1..=2u32 {
        // The same budget and the same doors the pass uses: each round asks afresh, the fault is said back, and
        // `cut_captions::retries` decides whether another round is owed. A third never is.
        let asked = effects_ask::ask(&fixture.tree, CTX, &batch, &[], attempt, &|| false);
        assert!(
            !asked.failed,
            "the wire answered both times: {:?}",
            asked.reason
        );
        let reply = cut_effects_pass::place(&batch, &asked.calls);
        if let Some(problem) = &reply.fault {
            said.push(cut_effects_pass::rejected(problem));
        }
        if reply.usable() {
            placed = Some(reply.effects.clone());
            break;
        }
        if !naivepost::cut_captions::retries(attempt) {
            break;
        }
    }

    assert!(
        placed.is_none(),
        "both rounds were rejected, so nothing lands"
    );
    assert_eq!(
        asked(&fixture),
        2,
        "two rounds, two requests — a third ask never happens"
    );
    assert_eq!(
        said,
        vec![
            ">>> effects rejected: clip 7 is not one of the clips given (1 to 3)".to_string(),
            ">>> effects rejected: clip 7 is not one of the clips given (1 to 3)".to_string(),
        ],
        "each rejection is said back before the call is put again"
    );
    // §F3.11's own sentence, spelled by the module: ASCII `--`, as this flow quotes it (unlike F3.10's em dash).
    assert_eq!(
        cut_effects_pass::no_answer(),
        "!!! effects: no usable answer -- the cut stands without them",
        "and that is what the status row and the log get"
    );
}
