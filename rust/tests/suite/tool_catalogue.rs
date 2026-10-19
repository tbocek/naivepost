//! §02-services.md §3 tool catalogue, rewrite directive B: the cut pass's five tools (§3.6) and the clips
//! pass's four (§3.7).
//!
//! Every check is on the JSON these tools answer with, because that string is the whole interface: the model
//! never sees a Rust type, only `{"error":"…"}` or the bare object `tools::ok` serialises.
//!
//! Fixtures use a target of 0 unless the test is about the target itself: with no target there is no window
//! to satisfy and the count floor drops to one, so a test about snapping or captions never has to solve
//! `finish_cut`'s arithmetic by accident. Every fixture length carries its own sanity assertion, phrased in
//! the module's formula, so a wrong fixture fails at the fixture.

use naivepost::tools::clips::{Clips, CAPTION_MIN_SECONDS, DEFAULT_ZOOM_BOX, FADE_SECONDS, KINDS};
use naivepost::tools::cutpass::{
    self, Plan, MAX_RATE, MIN_CLIP_SECONDS, MIN_SCENE_SECONDS, SHORT_TARGET_SECONDS,
};

/// The error sentence out of a tool's reply.
fn error(reply: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(reply).expect("a tool always answers JSON");
    value["error"]
        .as_str()
        .expect("a refusal is an error, never a silent ok")
        .to_string()
}

/// The body of a successful reply. `tools::ok` serialises it bare — no `{"ok": …}` wrapper on the wire.
fn ok(reply: &str) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(reply).expect("a tool always answers JSON");
    assert!(value["error"].is_null(), "refused: {reply}");
    value
}

/// A pair of numbers out of a reply, compared as floats — `Value` has no `PartialEq<f64>`.
fn pair(value: &serde_json::Value) -> (f64, f64) {
    (
        value[0].as_f64().expect("a pair"),
        value[1].as_f64().expect("a pair"),
    )
}

/// A number out of a reply.
fn num(value: &serde_json::Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key} is not a number in {value}"))
}

/// A twelve-minute session with no target: nothing to satisfy but the tools' own rules.
const SESSION: f64 = 720.0;
/// The six-minute cut the window and count tests aim at.
const TARGET: f64 = 360.0;

/// A fresh no-target plan. Named `a_plan` and not `plan` because most tests hold one in a variable called
/// `plan`, whose binding would shadow this function mid-test.
fn a_plan() -> Plan {
    Plan::new(SESSION, 0.0)
}

/// A stretch that even the fastest rate cannot shorten into what the render makes — under
/// `MIN_CLIP_SECONDS` on screen at `MAX_RATE` — while still surviving the minimum scene `add_segment`
/// reports on. Midway between the two bounds, so both hold with room.
fn clampable() -> f64 {
    (MIN_SCENE_SECONDS + MIN_CLIP_SECONDS * MAX_RATE) / 2.0
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s1_add_segment_reports_what_the_segment_became() {
    let mut plan = a_plan();
    // Nothing to snap to, so the edges stand where they were sent and each moved zero seconds.
    let placed = ok(&plan.add_segment(60.0, 150.0, "the demo starts here"));
    assert_eq!(pair(&placed["span"]), (60.0, 150.0));
    assert_eq!(pair(&placed["moved"]), (0.0, 0.0));
    assert_eq!(num(&placed, "seconds"), 90.0);
    assert_eq!(num(&placed, "mark_seconds"), 0.0);
    assert_eq!(placed["survives_min_scene"], serde_json::json!(true));

    // The running total is reported against the window: the model chooses the whole cut one segment at a
    // time and cannot add up what it has already asked for. With no target there is no window.
    assert_eq!(num(&placed, "footage"), 90.0);
    assert_eq!(pair(&placed["window"]), (0.0, 0.0));

    // A second segment sent before the first: kept in timeline order, not arrival order, because the cut is
    // a timeline and `remove_segment` names segments by their start.
    ok(&plan.add_segment(0.0, 45.0, "cold open"));
    let starts: Vec<f64> = plan.segments().iter().map(|s| s.start).collect();
    assert_eq!(starts, vec![0.0, 60.0]);
    assert_eq!(plan.segments()[1].why, "the demo starts here");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_add_segment_refuses_a_range_with_no_footage() {
    let mut plan = Plan::new(100.0, 0.0);
    // Past the end: the error carries the mm:ss reading of the model's own number.
    let problem = error(&plan.add_segment(90.0, 120.0, "later"));
    assert!(problem.contains("02:00 is past the end"), "{problem}");
    assert!(problem.contains("runs to 01:40"), "{problem}");

    // Ends before it starts — a different complaint, because there is no range to reason about.
    let problem = error(&plan.add_segment(80.0, 20.0, "backwards"));
    assert!(problem.contains("ends at 00:20 and starts at 01:20"), "{problem}");
    assert!(plan.segments().is_empty(), "a refusal adds nothing");

    // No footage under an edge. The caller says where the gaps are; this module only refuses to spend a
    // segment's length on seconds nobody filmed.
    let mut gapped = a_plan().with_gaps(vec![(200.0, 400.0)], vec![]);
    let problem = error(&gapped.add_segment(190.0, 260.0, "across the gap"));
    assert!(problem.contains("no footage under it"), "{problem}");
    assert!(problem.contains("a gap from 03:20 to 06:40"), "{problem}");
    // Beside a gap is fine — only an edge inside one, or a range spanning it, is refused.
    assert!(!gapped.add_segment(400.0, 500.0, "after").contains("\"error\""));
    let problem = error(&gapped.add_segment(100.0, 700.0, "all of it"));
    assert!(problem.contains("no footage under it"), "{problem}");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s3_a_refusal_quotes_the_model_s_own_seconds() {
    let mut plan = a_plan();
    ok(&plan.add_segment(100.0, 200.0, "first"));
    // Overlapping an added segment: the message says which one, in the same reading the model used.
    let problem = error(&plan.add_segment(150.0, 260.0, "second"));
    assert!(problem.contains("overlaps"), "{problem}");
    assert!(problem.contains("already added at 01:40 to 03:20"), "{problem}");

    // Touching is not overlapping: this pass joins takes, and a cut lands on a frame.
    assert!(!plan.add_segment(200.0, 300.0, "third").contains("\"error\""));
    assert_eq!(plan.segments().len(), 2);

    // Past an hour the minutes keep counting rather than rolling into a clock face: 62:05 says where the
    // second hour is, 02:05 would not.
    assert_eq!(cutpass::clock(3725.0), "62:05");
    let problem = error(&Plan::new(100.0, 0.0).add_segment(400.0, 500.0, "past the end"));
    assert!(problem.contains("08:20 is past the end"), "{problem}");
    // A negative number has no reading on a timeline; it clamps to 00:00 rather than printing "-00:03".
    assert_eq!(cutpass::clock(-3.0), "00:00");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s4_a_short_segment_is_accepted_and_reported() {
    let mut plan = a_plan();
    // Under the minimum scene: refused by `finish_cut`, not by `add_segment`. A model that has just chosen a
    // moment learns more from being told it is short than from a rejection it cannot act on.
    let short = MIN_SCENE_SECONDS / 2.0;
    let placed = ok(&plan.add_segment(10.0, 10.0 + short, "a flicker"));
    assert_eq!(pair(&placed["span"]), (10.0, 10.0 + short));
    assert_eq!(placed["survives_min_scene"], serde_json::json!(false));

    // Exactly at the bound survives: it is a floor, not a minimum-strictly-above.
    let placed = ok(&plan.add_segment(100.0, 100.0 + MIN_SCENE_SECONDS, "one beat"));
    assert_eq!(placed["survives_min_scene"], serde_json::json!(true));

    // And it is what `finish_cut` complains about, named by where it starts.
    let problem = error(&plan.finish_cut());
    assert!(problem.contains("runs 0.5 s on screen"), "{problem}");
    assert!(problem.contains("shortest stretch this cut keeps"), "{problem}");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s5_set_speed_clamps_and_always_says_why() {
    let mut plan = a_plan();
    ok(&plan.add_segment(0.0, 120.0, "a beat"));
    // Over the ceiling: applied at P.policy.maxSpeedRate, and told.
    let reply = ok(&plan.set_speed(0.0, MAX_RATE * 2.0));
    assert_eq!(num(&reply, "applied"), MAX_RATE);
    assert!(
        reply["reason"].as_str().unwrap().contains("fastest"),
        "{}",
        reply["reason"]
    );

    // A stretch short enough that even the ceiling renders it under the shortest clip the render makes.
    let seconds = clampable();
    assert!(
        seconds >= MIN_SCENE_SECONDS && seconds / MAX_RATE < MIN_CLIP_SECONDS,
        "fixture must survive the scene and trip the clamp: {seconds}"
    );
    let mut brief = a_plan();
    ok(&brief.add_segment(0.0, seconds, "short"));
    let reply = ok(&brief.set_speed(0.0, MAX_RATE));
    assert!(
        (num(&reply, "applied") - seconds / MIN_CLIP_SECONDS).abs() < 1e-9,
        "{reply}"
    );
    assert!(num(&reply, "on_screen") >= MIN_CLIP_SECONDS - 1e-9);
    assert!(
        reply["reason"].as_str().unwrap().contains("shortest clip"),
        "{}",
        reply["reason"]
    );

    // A rate at or below 1 is applied exactly with no reason: this pass only shortens, and slowing footage
    // down lengthens it, so the shortest-clip floor cannot bite.
    for rate in [1.0, 0.9, 0.5] {
        let reply = ok(&plan.set_speed(0.0, rate));
        assert_eq!(num(&reply, "applied"), rate);
        assert!(reply["reason"].is_null(), "{reply}");
    }

    // A named segment that is not there says what the cut does hold.
    let problem = error(&plan.set_speed(SESSION - 60.0, 2.0));
    assert!(problem.contains("no segment starting at"), "{problem}");
    assert!(problem.contains("the cut holds 1 segment(s)"), "{problem}");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s6_remove_segment_names_a_segment_by_its_start() {
    // A minute apart: `remove_segment` matches a start within the snap tolerance, so timestamps this far
    // apart can never be confused for one another.
    let six = |mut plan: Plan| -> Plan {
        for index in 0..6u32 {
            let start = index as f64 * 60.0;
            ok(&plan.add_segment(start, start + 50.0, "a beat"));
        }
        plan
    };
    let mut plan = six(a_plan());
    let removed = ok(&plan.remove_segment(120.0));
    assert_eq!((num(&removed, "start"), num(&removed, "end")), (120.0, 170.0));
    assert_eq!(removed["segments_left"], 5);

    // Gone from the list, so a second call has nothing to remove — and says how many are left.
    let problem = error(&plan.remove_segment(120.0));
    assert!(problem.contains("no segment starting at"), "{problem}");
    assert!(problem.contains("the cut holds 5 segment(s)"), "{problem}");

    // An empty cut says so rather than listing nothing. Farthest-first, so no earlier start is ever asked
    // for twice.
    let mut empty = six(a_plan());
    for index in (0..6u32).rev() {
        ok(&empty.remove_segment(index as f64 * 60.0));
    }
    let problem = error(&empty.remove_segment(0.0));
    assert!(problem.contains("nothing yet"), "{problem}");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s7_cut_status_and_finish_cut_are_one_reading() {
    let mut built = a_plan();
    for index in 0..6u32 {
        let start = index as f64 * 100.0;
        ok(&built.add_segment(start, start + 90.0, "a beat"));
    }
    let reading = built.status();
    assert_eq!(reading.segments, 6);
    // None sped up, so on screen is their own length.
    assert!((reading.footage - 6.0 * 90.0).abs() < 1e-9);

    // The tool's own words carry the same numbers the struct holds.
    let status = ok(&built.status_json());
    assert_eq!(num(&status, "footage"), reading.footage);
    assert_eq!(pair(&status["window"]), reading.window);
    assert_eq!(status["segments"], reading.segments as u64);
    assert_eq!(status["min_segments"], reading.min_segments as u64);
    assert_eq!(status["max_segments"], reading.max_segments as u64);

    // `cut_status` is "the reading finish_cut would give, at any time": when one has no problem to list, the
    // other answers with those same numbers.
    let finished = ok(&built.finish_cut());
    assert_eq!(num(&finished, "footage"), reading.footage);

    // An empty plan reports honestly instead of claiming a cut is ready.
    let problem = error(&a_plan().finish_cut());
    assert!(problem.contains("0 segments is under the"), "{problem}");
    // And `cut_status` on the same plan still answers, with the pending work it was told about.
    let mut told = a_plan();
    told.set_pending(12.0, 4.0);
    let status = ok(&told.status_json());
    assert_eq!(num(&status, "marks_pending"), 12.0);
    assert_eq!(num(&status, "dead_air_pending"), 4.0);
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s8_the_window_and_the_counts_follow_the_parameters() {
    // P.machine.footageWindow: ×[0.6, 1.2] up to a short target — a format, a promise — else ×[0.6, 1.5],
    // a wish; the ceiling × maxSpeedRate either way, because that is what the speed pass could squeeze in.
    let (low, high) = cutpass::footage_window(SHORT_TARGET_SECONDS);
    assert_eq!(
        (low, high),
        (SHORT_TARGET_SECONDS * 0.6, SHORT_TARGET_SECONDS * 1.2 * MAX_RATE)
    );
    let (low, high) = cutpass::footage_window(TARGET);
    assert_eq!((low, high), (TARGET * 0.6, TARGET * 1.5 * MAX_RATE));
    // No target: no window to satisfy, so length never blocks a finish.
    assert_eq!(cutpass::footage_window(0.0), (0.0, 0.0));

    // P.machine.suggestMinSegments = min(1 + ⌊target/30⌋, 4): the floor is what a short target asks for, so
    // a 25 s cut is not refused for having one segment.
    assert_eq!(cutpass::min_segments(25.0), 1);
    assert_eq!(cutpass::min_segments(90.0), 4);
    assert_eq!(cutpass::min_segments(TARGET), 4);

    // P.machine.suggestMaxSegments = max(⌊target/5⌋, 40): a floor of forty, so the bound catches a runaway
    // answer (548 segments) and not a cut that genuinely is many short moments.
    assert_eq!(cutpass::max_segments(TARGET), 72);
    assert_eq!(cutpass::max_segments(60.0), 40);

    // A plan with the count the target asks for and barely any footage is inside the bounds on one axis and
    // outside on the other, so exactly one problem comes back — the length one, which changes what to add.
    let mut thin = Plan::new(SESSION, TARGET);
    for index in 0..cutpass::min_segments(TARGET) as u32 {
        let start = index as f64 * 60.0;
        // Long enough to survive the minimum scene, so this plan's only fault is how little it keeps.
        ok(&thin.add_segment(start, start + MIN_SCENE_SECONDS * 2.0, "brief"));
    }
    let status = thin.status();
    assert_eq!(status.segments, status.min_segments);
    let problems = cutpass::cut_problems(&status);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("kept is under"), "{problems:?}");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s9_too_much_footage_is_a_problem_too() {
    // The ceiling is the floor's own arithmetic — ×0.6 of the target times everything MAX_RATE can shorten —
    // so each segment holds a quarter of it on screen (its length is a quarter times MAX_RATE): six are double
    // the ceiling and four land exactly on it, which counts as inside. Speeding up cannot rescue the six: the
    // ceiling already assumed every second ran at MAX_RATE, so dropping is the only fix. The session is sized
    // FROM the segments, so none can fall past its end, and a step wider than a segment means none overlap.
    let (low, high) = cutpass::footage_window(TARGET);
    let each = high * MAX_RATE / 4.0;
    assert!(
        low > 0.0 && six_at_max(each) > high && four_at_max(each) <= high,
        "fixture must be over with six and inside with four: {}",
        six_at_max(each)
    );
    let step = each * 1.5;
    let mut over = Plan::new(step * 6.0 + each, TARGET);
    for index in 0..6u32 {
        ok(&over.add_segment(index as f64 * step, index as f64 * step + each, "a long take"));
    }
    let status = over.status();
    assert!(status.footage > high, "{status:?}");
    let problems = cutpass::cut_problems(&status);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("is over the"), "{problems:?}");
    assert!(problems[0].contains("drop some footage"), "{problems:?}");

    // Speeding it up cannot rescue it — this is what the ceiling already assumed.
    let starts: Vec<f64> = over.segments().iter().map(|s| s.start).collect();
    for start in &starts {
        ok(&over.set_speed(*start, MAX_RATE));
    }
    assert!(over.finish_cut().contains("is over the"), "{}", over.finish_cut());

    // Four of them at that rate are under the ceiling and still past the count's floor: a cut the arithmetic
    // can land. Dropping stops complaining at exactly that point.
    for start in starts.iter().take(2) {
        ok(&over.remove_segment(*start));
    }
    let status = over.status();
    assert!(status.segments >= status.min_segments, "{status:?}");
    assert!(!over.finish_cut().contains("\"error\""), "{}", over.finish_cut());
}

/// Six segments of `each` seconds, on screen at the fastest rate this cut plays footage.
fn six_at_max(each: f64) -> f64 {
    6.0 * each / MAX_RATE
}

/// The same keep list with two dropped — what `remove_segment` leaves behind.
fn four_at_max(each: f64) -> f64 {
    4.0 * each / MAX_RATE
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s10_edges_snap_and_report_how_far_they_moved() {
    // The caller names where an edge may land — word edges, silences, visual cuts — and how far is near
    // enough. P.policy.snapToleranceSeconds.
    let mut plan = a_plan().with_snap_points(vec![100.0, 200.0, 300.0]);
    let placed = ok(&plan.add_segment(98.0, 204.0, "near enough"));
    assert_eq!(pair(&placed["span"]), (100.0, 200.0));
    // The move is reported per edge: a model that cannot see it will send the same edges again next round.
    assert_eq!(pair(&placed["moved"]), (2.0, -4.0));

    // Further than tolerance, and the edge stands where it was put.
    let placed = ok(&plan.add_segment(310.0, 420.0, "far from any point"));
    assert_eq!(pair(&placed["span"]), (310.0, 420.0));
    assert_eq!(pair(&placed["moved"]), (0.0, 0.0));

    // A snap may move an edge, not reverse the segment: both edges pulled onto one point would leave nothing,
    // so they stand.
    let mut tight = a_plan().with_snap_points(vec![100.0]);
    let placed = ok(&tight.add_segment(98.0, 102.0, "both ends on one word"));
    assert_eq!(pair(&placed["span"]), (98.0, 102.0));

    // A tighter tolerance leaves the same edge alone: the caller's number decides, not a private one.
    let mut strict = a_plan().with_snap_points(vec![100.0, 200.0]);
    strict.set_snap_tolerance(1.0);
    let placed = ok(&strict.add_segment(98.0, 204.0, "outside the tighter tolerance"));
    assert_eq!(pair(&placed["span"]), (98.0, 204.0));

    // A mark over a segment is reported as the seconds the cut will take out of it.
    let mut marked = a_plan().with_gaps(vec![], vec![(120.0, 135.0)]);
    let placed = ok(&marked.add_segment(100.0, 200.0, "over a mark"));
    assert_eq!(num(&placed, "mark_seconds"), 15.0);
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s11_add_caption_places_and_reports_the_fade() {
    let mut clips = Clips::new(&[(1, 0.0, 30.0)]);
    let placed = ok(&clips.add_caption(1, 5.0, 8.0, "  hello world  "));
    assert_eq!(pair(&placed["span"]), (5.0, 8.0));
    assert_eq!(num(&placed, "seconds"), 8.0 - 5.0);
    assert_eq!(num(&placed, "fade"), FADE_SECONDS);
    // The words as sent, trimmed: this is what will be drawn.
    assert_eq!(placed["text"], "hello world");
    assert_eq!(placed["clamped"], serde_json::json!(false));

    // An unknown clip says how many there are; an empty caption says it has no words.
    let problem = error(&clips.add_caption(7, 0.0, 3.0, "later"));
    assert!(problem.contains("not in this batch"), "{problem}");
    assert!(problem.contains("1 clip"), "{problem}");
    let problem = error(&clips.add_caption(1, 1.0, 4.0, "   "));
    assert!(problem.contains("no words"), "{problem}");
    assert_eq!(clips.captions_of(1).len(), 1, "a refusal adds nothing");

    // A caption shorter than the floor is refused with the seconds left for it and the clip's own space.
    let problem = error(&clips.add_caption(1, 29.9, 30.0, "too late"));
    assert!(problem.contains("leaves 0.10 s"), "{problem}");
    assert!(problem.contains(&CAPTION_MIN_SECONDS.to_string()), "{problem}");
    assert!(problem.contains("0..30.00"), "the clip's own space: {problem}");

    // Exactly at the floor is placed, with a fade that cannot eat a third of it.
    let placed = ok(&clips.add_caption(1, 0.0, CAPTION_MIN_SECONDS, "at the floor"));
    assert_eq!(num(&placed, "fade"), (CAPTION_MIN_SECONDS / 2.0).min(FADE_SECONDS));
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s12_a_caption_at_the_edge_is_clamped_not_dropped() {
    let mut clips = Clips::new(&[(1, 0.0, 30.0)]);
    // Counting from the session instead of from the clip: the part inside is kept and said so, rather than
    // the whole caption being thrown away for a number the clip cannot show.
    let placed = ok(&clips.add_caption(1, 25.0, 40.0, "still talking"));
    assert_eq!(pair(&placed["span"]), (25.0, 30.0));
    assert_eq!(placed["clamped"], serde_json::json!(true));

    // Before the start, the same rule: clamped up to zero and reported.
    let placed = ok(&clips.add_caption(1, -5.0, 3.0, "before the start"));
    assert_eq!(pair(&placed["span"]), (0.0, 3.0));
    assert_eq!(placed["clamped"], serde_json::json!(true));

    // Entirely outside has nothing to clamp to: it leaves no seconds at all and is refused.
    let problem = error(&clips.add_caption(1, 40.0, 45.0, "outside"));
    assert!(problem.contains("leaves 0.00 s"), "{problem}");
    assert_eq!(clips.captions_of(1).len(), 2);
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s13_a_captioned_clip_is_never_spd_up_invisibly() {
    let mut clips = Clips::new(&[(1, 0.0, 30.0), (2, 30.0, 30.0)]);
    // The prompt's own rule, enforced again: the model asked for a speed and must be able to see that it did
    // not happen, or it believes its speed is in the cut.
    ok(&clips.add_caption(1, 1.0, 4.0, "captioned"));
    let problem = error(&clips.set_clip_speed(1, 1.2));
    assert!(problem.contains("has a caption"), "{problem}");
    assert!(problem.contains("cannot be sped up"), "{problem}");
    assert_eq!(clips.clips()[0].rate, 1.0, "a refusal changes nothing");

    // Slowing down is allowed — the rule is about speeding a captioned clip up.
    assert_eq!(num(&ok(&clips.set_clip_speed(1, 0.9)), "applied"), 0.9);

    // A clip without captions takes the rate, reported as applied plus the on-screen length.
    let reply = ok(&clips.set_clip_speed(2, MAX_RATE));
    assert_eq!(num(&reply, "applied"), MAX_RATE);
    assert!((num(&reply, "on_screen") - 30.0 / MAX_RATE).abs() < 1e-9);
    assert!(reply["reason"].is_null(), "{reply}");

    // Over the ceiling it is clamped and told why; an unknown clip is refused.
    let reply = ok(&clips.set_clip_speed(2, MAX_RATE * 3.0));
    assert_eq!(num(&reply, "applied"), MAX_RATE);
    assert!(reply["reason"].as_str().unwrap().contains("fastest"));
    let problem = error(&clips.set_clip_speed(9, 1.2));
    assert!(problem.contains("not in this batch"), "{problem}");
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s14_add_effect_places_the_three_kinds() {
    let mut clips = Clips::new(&[(1, 0.0, 30.0)]);
    // A zoom with a box gets that box; the span is reported in the clip's own seconds.
    let placed = ok(&clips.add_effect(1, "zoom", 5.0, 8.0, None, Some((0.4, 0.3))));
    assert_eq!(placed["kind"], "zoom");
    assert_eq!(pair(&placed["span"]), (5.0, 8.0));
    assert_eq!(placed["box"], serde_json::json!([0.4, 0.3]));
    assert_eq!(num(&placed, "fade"), FADE_SECONDS);

    // No box: the frame's middle, and said so — a zoom aimed nowhere in particular is visible in the reply
    // instead of being a guess the model has to discover in the render.
    let placed = ok(&clips.add_effect(1, "zoom", 10.0, 12.0, None, None));
    assert_eq!(pair(&placed["box"]), (DEFAULT_ZOOM_BOX.0, DEFAULT_ZOOM_BOX.1));

    // A stop holds the frame; volume carries its gain.
    assert_eq!(
        num(&ok(&clips.add_effect(1, "stop", 20.0, 21.0, None, None)), "gain"),
        0.0
    );
    let placed = ok(&clips.add_effect(1, "volume", 0.0, 30.0, Some(0.2), None));
    assert_eq!(num(&placed, "gain"), 0.2);
    assert!(placed["box"].is_null(), "only a zoom has a box");

    // A gain of 1 is a no-op and says so rather than adding an effect that changes nothing.
    let problem = error(&clips.add_effect(1, "volume", 0.0, 5.0, Some(1.0), None));
    assert!(problem.contains("changes nothing"), "{problem}");

    // An unknown kind is refused with the words it does take — never quietly mapped onto another.
    let problem = error(&clips.add_effect(1, "blur", 0.0, 5.0, None, None));
    assert!(problem.contains("not an effect this pass places"), "{problem}");
    for kind in KINDS {
        assert!(problem.contains(kind), "{kind} is listed: {problem}");
    }

    // An empty span after clamping is refused, naming the clip's own space; a span running off the end is
    // clamped and reported.
    let problem = error(&clips.add_effect(1, "zoom", 40.0, 50.0, None, None));
    assert!(problem.contains("empty"), "{problem}");
    assert!(problem.contains("0..30.00"), "{problem}");
    let placed = ok(&clips.add_effect(1, "stop", 28.0, 40.0, None, None));
    assert_eq!(pair(&placed["span"]), (28.0, 30.0));
    assert_eq!(placed["clamped"], serde_json::json!(true));
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s15_get_frames_walks_the_clip_s_own_seconds() {
    let mut clips = Clips::new(&[(1, 120.0, 5.0), (2, 200.0, 30.0)]);
    let step = 1.0;
    clips.set_frame_interval(step);

    // Every frame of a clip, named by the clip and its offset inside it — the space every other number this
    // pass sends is in. The session's own second comes along so the caller can find the picture.
    let frames = clips.clip_frames(1, None);
    assert_eq!(frames.len(), (5.0 / step) as usize + 1);
    assert_eq!((frames[0].clip, frames[0].at, frames[0].start), (1, 0.0, 120.0));
    assert_eq!(frames.last().unwrap().at, 5.0);

    // One frame near a moment: the nearest one, which is what an aimed zoom asked for.
    let frames = clips.clip_frames(2, Some(12.4));
    assert_eq!(frames.len(), 1);
    assert_eq!((frames[0].clip, frames[0].at), (2, 12.0));

    // A clip that is not in the batch has no frames at all.
    assert!(clips.clip_frames(9, None).is_empty());
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s16_both_passes_share_one_rate_rule() {
    // One clamp for rates in both passes: a cut and its clips cannot disagree about what a rate means.
    assert_eq!(cutpass::apply_rate(MAX_RATE * 2.0, 60.0), MAX_RATE);
    // A rate at or below 1 lengthens a stretch, so the shortest-clip floor cannot bite it.
    assert_eq!(cutpass::apply_rate(0.9, 2.0), 0.9);

    // Over the ceiling on a stretch too short for even that: the clip floor wins, since it is the render's
    // own limit — and both passes reach it through the same function.
    let seconds = clampable();
    let applied = cutpass::apply_rate(MAX_RATE, seconds);
    assert!(applied < MAX_RATE && seconds / applied <= MIN_CLIP_SECONDS + 1e-9);

    let mut clips = Clips::new(&[(1, 0.0, seconds)]);
    let reply = ok(&clips.set_clip_speed(1, MAX_RATE));
    assert_eq!(num(&reply, "applied"), applied);

    let mut plan = a_plan();
    ok(&plan.add_segment(0.0, seconds, "short"));
    let reply = ok(&plan.set_speed(0.0, MAX_RATE));
    assert_eq!(num(&reply, "applied"), applied);
}
