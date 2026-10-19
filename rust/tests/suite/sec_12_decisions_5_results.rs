//! §12-decisions#5's second clause: "Every write tool's result now carries what the app made of the
//! item, not just 'ok'."
//!
//! The spec's §3 tables say what each result must hold — a caption clamped into its clip with its
//! fades, a rate as applied with the on-screen length, an effect as placed with the box a zoom used,
//! a segment's snapped edges against the running footage, a retake mark's removed stretch against
//! the running share, a seam drop's exact words. §3's reason is the same in every row: the model is
//! choosing numbers it cannot check, so a bare `ok` leaves it believing a placement that never
//! happened.
//!
//! Each case here goes through `clip_rules_tools_leg::answer`, `cutpass::Plan`, `tools::retakes::Marks`
//! or `join_ask`'s own door — the same entry points the live loops dispatch a call through — and then
//! re-reads the app's OWN state to prove the numbers in the result are that state's, not a copy of
//! what the model asked for. That is what makes the check about the result rather than about the echo.
//!
//! s2's read-tool half (a read answered from real app state) lives with these because both doors are
//! the same `answer` call: `get_frames` and `cut_status` are answered here too.

use naivepost::exchanges::ToolCall;
use naivepost::tools::clips::Clips;
use naivepost::tools::cutpass::Plan;
use naivepost::tools::retakes::{Line, Marks, Trim};
use naivepost::tools::textedit::Join;
use naivepost::ui::cut_suggest_model_run;
use naivepost::clip_rules_tools_leg;
use serde_json::Value;

/// Call one tool through `name`'s own door, so the test exercises the dispatch the loop uses.
fn call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        name: name.to_string(),
        args: args.to_string(),
        result: String::new(),
    }
}

/// Whether the answer is an acceptance rather than one of §3.1's error sentences. A result body is
/// the fields themselves with no wrapper, so `ok` is never a key to compare; an error arrives as
/// `{"error": "..."}` and carries none of them.
fn accepted(answer: &Value) -> bool {
    answer.get("error").is_none()
}

/// Three kept clips of 20 s each, the batch the Cut page hands the §3.7 passes.
fn batch() -> Clips {
    Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0), (3, 40.0, 20.0)])
}

/// s3: `add_caption` answers with the caption AS PLACED, and the clamp is the proof: a span run past
/// the clip's end comes back inside it, with the fade the render will use.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s3_a_write_result_carries_what_the_app_made_add_caption() {
    let mut batch = batch();
    // Clip 2 runs 0..20 in the clip's own space; 18..40 is a model counting from the session, so the
    // answer has to say what the clip can actually show rather than repeat the ask.
    let answer: Value =
        serde_json::from_str(&clip_rules_tools_leg::answer(
            &call("add_caption", serde_json::json!({"clip": 2, "start": 18.0, "end": 40.0, "text": "  a line  "})),
            &mut batch,
        ))
        .expect("the tool answers with JSON");
    assert!(accepted(&answer), "the call was accepted: {answer}");
    assert_eq!(answer["span"], serde_json::json!([18.0, 20.0]), "clamped into the clip");
    assert_eq!(answer["seconds"], serde_json::json!(2.0), "the on-clip length");
    assert_eq!(answer["clamped"], serde_json::json!(true), "the answer says it moved");
    assert_eq!(answer["text"], serde_json::json!("a line"), "the text as stored, trimmed");
    // The fade is a number the app chose (half the caption at worst), not one the model sent.
    let fade = answer["fade"].as_f64().expect("the result names the fade");
    assert!(fade > 0.0 && fade <= 1.0, "a fade the render can use: {fade}");
    // And it is the SAME span the pass will place, read back off the batch's own state.
    let placed = batch.captions_of(2);
    assert_eq!(placed.len(), 1, "the batch holds the one caption the result named");
    assert!(
        (placed[0].span.0 - 18.0).abs() < 0.01 && (placed[0].span.1 - 20.0).abs() < 0.01,
        "the stored caption is the clamped span, not the asked one: {:?}",
        placed[0].span
    );
    assert!(
        (placed[0].fade - fade).abs() < 0.001,
        "the fade in the result is the fade stored on the caption: {:?}",
        placed[0].fade
    );
}

/// s3: `set_clip_speed` answers with the rate AS APPLIED plus the on-screen length, and the proof is
/// the captioned clip the tool refuses — the prompt's own rule, said rather than silently dropped.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s3_a_write_result_carries_what_the_app_made_set_clip_speed() {
    let mut batch = batch();
    batch.add_caption(3, 1.0, 6.0, "a line already on screen");
    let clean: Value = serde_json::from_str(&clip_rules_tools_leg::answer(
        &call("set_clip_speed", serde_json::json!({"clip": 1, "rate": 2.0})),
        &mut batch,
    ))
    .expect("JSON");
    assert!(accepted(&clean));
    assert_eq!(clean["asked"], serde_json::json!(2.0), "what the model asked");
    assert_eq!(clean["applied"], serde_json::json!(2.0), "what will be applied");
    assert_eq!(clean["on_screen"], serde_json::json!(10.0), "20 s at 2x is 10 s on screen");

    // The same tool, the same call, over a caption: refused IN THE RESULT, per call.
    let refused: Value = serde_json::from_str(&clip_rules_tools_leg::answer(
        &call("set_clip_speed", serde_json::json!({"clip": 3, "rate": 2.0})),
        &mut batch,
    ))
    .expect("JSON");
    assert!(
        refused.get("error").and_then(Value::as_str).is_some(),
        "a rate over a caption is refused, not dropped: {refused}"
    );
}

/// s3: `add_effect` answers with the effect as placed — clamped, faded, and for a zoom the BOX the
/// app actually used, which is §3.7's answer to a prototype that showed the pass no pictures.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s3_a_write_result_carries_what_the_app_made_add_effect() {
    let mut batch = batch();
    let answer: Value = serde_json::from_str(&clip_rules_tools_leg::answer(
        &call(
            "add_effect",
            serde_json::json!({"clip": 1, "kind": "zoom", "start": 4.0, "end": 60.0}),
        ),
        &mut batch,
    ))
    .expect("JSON");
    assert!(accepted(&answer));
    assert_eq!(answer["kind"], serde_json::json!("zoom"));
    assert_eq!(answer["span"], serde_json::json!([4.0, 20.0]), "clamped into the clip");
    assert_eq!(answer["clamped"], serde_json::json!(true));
    // §3.7: "for a zoom the box used". A zoom without a box gets the frame's middle, and the answer
    // has to say so — that number is what the render will crop by.
    let box_used = answer.get("box").filter(|v| !v.is_null()).cloned();
    assert!(box_used.is_some(), "the result names the box the zoom used: {answer}");
    assert!(
        answer.get("fade").is_some(),
        "the result names the app's fade: {answer}"
    );
}

/// s3: `add_segment` answers with the segment as it BECAME — snapped edges, how far each moved, what
/// a mark will take, whether it survives the scene floor, and the running footage against the window.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s3_a_write_result_carries_what_the_app_made_add_segment() {
    let mut plan = Plan::new(120.0, 60.0).with_snap_points(vec![4.0, 20.0]);
    let answer: Value = serde_json::from_str(&cut_suggest_model_run::answer_call(
        &mut plan,
        &call("add_segment", serde_json::json!({"start": 4.3, "end": 20.4, "why": "the point"})),
    ))
    .expect("JSON");
    assert!(accepted(&answer));
    assert!(answer.get("span").is_some(), "the edges as placed: {answer}");
    assert!(answer.get("moved").is_some(), "how far each edge moved: {answer}");
    assert!(answer.get("mark_seconds").is_some(), "what a mark will take out: {answer}");
    assert!(answer.get("survives_min_scene").is_some(), "the floor verdict: {answer}");
    // The running total is the plan's own reading, so the result and `cut_status` cannot disagree.
    let status: Value =
        serde_json::from_str(&cut_suggest_model_run::answer_call(&mut plan, &call("cut_status", serde_json::json!({}))))
            .expect("JSON");
    assert_eq!(
        status["footage"], answer["footage"],
        "add_segment's running footage IS the plan's status, not a number of its own"
    );
    assert_eq!(status["window"], answer["window"], "and so is the target window");
}

/// s2 and s3 together for the retake hunt: `mark_abandoned` answers with the stretch that will ACTUALLY
/// be removed (trimmed to the words the later take repeats) plus the running share of speech marked,
/// and §3.4's `get_lines` read comes back with the pause before every line, including the short stops
/// the brief leaves out.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s2_s3_the_retake_read_and_mark() {
    // Five lines of 10 s each, 60 s of speech, so one 10 s mark is a sixth of it.
    let lines: Vec<Line> = (1..=6u32)
        .map(|n| Line {
            n,
            start: (n as f64 - 1.0) * 10.0,
            end: n as f64 * 10.0,
        })
        .collect();
    let mut marks = Marks::new(lines, 60.0);
    let mark: Value = serde_json::from_str(&marks.mark_abandoned_at(2, 3, None, Trim::none(), 0.0))
        .expect("JSON");
    assert!(accepted(&mark), "the mark was accepted: {mark}");
    assert!(mark.get("removed").is_some(), "the stretch that will be removed: {mark}");
    assert!(mark.get("seconds").is_some(), "its length in seconds: {mark}");
    let share = mark["speech_marked"].as_f64().expect("the running share");
    assert!(share > 0.0 && share <= 1.0, "a share of the session's speech: {share}");
    assert_eq!(
        share,
        marks.finish().share,
        "the share in the result is the same reading `finish` will report"
    );

    // s2: the read. §3.4 wants every pause, including the ones under the threshold.
    let read = naivepost::retakes_ask::lines_answer_for_test(&marks, 1, 4);
    assert!(
        read.contains("pause"),
        "the read names the pause before each line: {read}"
    );
}

/// s3: `drop_words` answers with the exact words that would go and the sentence left reading across
/// the join — §3.5's replacement for a prototype that derived the counts by matching backwards.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s3_a_write_result_carries_what_the_app_made_drop_words() {
    let mut join = Join::new(
        &["the", "network", "is", "live", "today"],
        &["the", "network", "goes", "live", "now"],
    );
    let mut refusals: Vec<String> = Vec::new();
    let answer: Value = serde_json::from_str(&naivepost::join_ask::call_for_test(
        &call("drop_words", serde_json::json!({"side": "before", "count": 3})),
        &mut join,
        &mut refusals,
    ))
    .expect("JSON");
    assert!(accepted(&answer), "the drop was accepted: {answer}");
    assert_eq!(answer["count"], serde_json::json!(3), "the count as charged");
    let words: Vec<&str> = answer["words"]
        .as_array()
        .expect("the words that would go")
        .iter()
        .map(|w| w.as_str().expect("a word"))
        .collect();
    assert_eq!(words, vec!["is", "live", "today"], "the exact words, in the order spoken");
    assert!(
        answer.get("across_the_join").is_some(),
        "the sentence left reading across the join: {answer}"
    );
    // Same state the logic test checks: the drop is charged on the Join, not just described.
    assert_eq!(
        join.dropped_count(naivepost::tools::textedit::Side::Before),
        3,
        "the counts the tool accepted are the counts the seam carries"
    );
}

/// s2: a read the model is owed, answered from the app rather than stubbed. `get_frames` over a clip
/// batch names the clip-relative offsets every other number in the request uses — the read §3.7 adds
/// so a zoom can be aimed at something instead of at the middle of the frame.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s2_a_read_call_is_answered_from_the_app_get_frames() {
    let mut batch = batch();
    let frames: Value = serde_json::from_str(&clip_rules_tools_leg::answer(
        &call("get_frames", serde_json::json!({"clip": 2})),
        &mut batch,
    ))
    .expect("JSON");
    let list = frames["frames"].as_array().expect("a frame list");
    assert!(!list.is_empty(), "the clip's frames came back: {frames}");
    assert!(
        list.iter().all(|f| f["clip"] == serde_json::json!(2)),
        "every frame is stamped with the clip it belongs to: {frames}"
    );
    // A clip outside the batch is §3.1's error sentence, not an empty list and not a hang.
    let outside: Value = serde_json::from_str(&clip_rules_tools_leg::answer(
        &call("get_frames", serde_json::json!({"clip": 99})),
        &mut batch,
    ))
    .expect("JSON");
    assert!(
        outside.get("error").and_then(Value::as_str).is_some(),
        "a clip not in this batch is refused: {outside}"
    );
}

/// s2: `cut_status` at any time gives the reading `finish_cut` would — footage, window, segment count
/// against its floor and ceiling, and the marks and dead air still to come out.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s2_a_read_call_is_answered_from_the_app_cut_status() {
    let mut plan = Plan::new(120.0, 60.0);
    plan.set_pending(5.0, 3.0);
    let status: Value =
        serde_json::from_str(&cut_suggest_model_run::answer_call(&mut plan, &call("cut_status", serde_json::json!({}))))
            .expect("JSON");
    assert!(accepted(&status), "the read answered: {status}");
    for key in ["footage", "window", "segments", "min_segments", "max_segments", "marks_pending", "dead_air_pending"] {
        assert!(status.get(key).is_some(), "cut_status names {key}: {status}");
    }
    assert_eq!(status["marks_pending"], serde_json::json!(5.0), "the marks still to come out");
    assert_eq!(status["dead_air_pending"], serde_json::json!(3.0), "the dead air still owed");
}

/// s2: the shared reads, answered for the cut job from the plan in hand rather than left silent. A
/// read that answers nothing is what §3 calls the worst failure: the model stops trying.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s2_a_read_call_is_answered_from_the_app_shared_reads() {
    let mut plan = Plan::new(30.0, 20.0);
    let frames: Value = serde_json::from_str(&cut_suggest_model_run::answer_call(
        &mut plan,
        &call("get_frames", serde_json::json!({"from": 0.0, "to": 5.0})),
    ))
    .expect("JSON");
    let list = frames["frames"].as_array().expect("a frame list");
    assert_eq!(list.len(), 5, "one frame a second over the footage the plan runs on: {frames}");

    // A read the job cannot carry an answer for is refused in words, never silently dropped.
    let lines: Value = serde_json::from_str(&cut_suggest_model_run::answer_call(
        &mut plan,
        &call("get_lines", serde_json::json!({"from": 1, "to": 3})),
    ))
    .expect("JSON");
    assert!(
        lines.get("error").and_then(Value::as_str).is_some(),
        "an unanswerable read says so: {lines}"
    );
}

/// s2: the joins read. §3.5's `get_words` answers with more of the seam than the brief showed, and
/// §3.1's `get_context` with the User Context the job was asked under — both from the seam in hand.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s2_a_read_call_is_answered_from_the_app_joins_reads() {
    let mut join = Join::new(&["the", "network", "is"], &["today"]);
    let mut refusals: Vec<String> = Vec::new();
    let words: Value = serde_json::from_str(&naivepost::join_ask::call_for_test(
        &call("get_words", serde_json::json!({"side": "before", "count": 3})),
        &mut join,
        &mut refusals,
    ))
    .expect("JSON");
    assert!(accepted(&words), "the read answered: {words}");
    assert_eq!(
        words["words"],
        serde_json::json!(["the", "network", "is"]),
        "the read names the seam's own words: {words}"
    );
    // A read is not an answer: it must not end the flow, so nothing was refused and nothing was kept.
    assert!(refusals.is_empty(), "a read is not a refusal: {refusals:?}");
    assert!(!join.kept(), "a read left the flow open");
}

/// §3.1's `get_context` is in EVERY job's offer (P.eng.contextEverywhere), so the joins loop has to be
/// able to answer it. A read that answers nothing is §3's worst failure: the model stops trying.
/// And a read the offer NAMES but a seam has nothing for must say so honestly: a join ask carries no
/// pictures, so `get_frames` gets `tool_reads`' empty-batch sentence, not `run_one`'s claim that the
/// tool is not one of this job's tools.
#[test]
fn sec_12_decisions_5_gaps_this_audit_closed_s2_a_read_call_is_answered_from_the_app_joins_get_context() {
    let before: Vec<String> = ["one", "two", "three"].iter().map(|w| w.to_string()).collect();
    let after: Vec<String> = ["four"].iter().map(|w| w.to_string()).collect();
    let context = naivepost::join_ask::read_for_test(
        &call("get_context", serde_json::json!({})),
        "keep the pacing brisk",
        &before,
        &after,
    )
    .expect("a shared read is answered here, not passed through as a job tool");
    let answer: Value = serde_json::from_str(&context).expect("JSON");
    assert!(
        answer.get("error").is_none(),
        "a read the offer lists for this job is answered, not refused: {context}"
    );
    assert_eq!(
        answer["job"],
        serde_json::json!("joins"),
        "the answer says which job it was read for: {answer}"
    );

    // The read the offer names but a seam has nothing for: refused in words, and named as the empty
    // batch it is rather than as a tool this job does not have.
    let frames = naivepost::join_ask::read_for_test(
        &call("get_frames", serde_json::json!({"clip": 1})),
        "keep the pacing brisk",
        &before,
        &after,
    )
    .expect("the read is answered here too");
    let why: Value = serde_json::from_str(&frames).expect("JSON");
    let said = why["error"].as_str().unwrap_or("");
    assert!(said.contains("no frames"), "an empty frame batch says so: {frames}");
    assert!(
        !said.contains("not one of this job's tools"),
        "a read the offer lists is never reported as an unknown tool: {frames}"
    );
}
