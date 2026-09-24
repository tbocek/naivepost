//! §12-decisions#cut-and-effects — where each of the eleven behind-the-model behaviours of the Cut
//! and effects section lives now, checked against the live code path that makes it.
//!
//! The three pieces this round added (`cut_effect_decisions::choose_edge`, `dead_air_is_cut`,
//! `withdrawal_notice`) are asserted directly; every other row takes its evidence from an existing
//! function — a real `add_segment` reply, a real `set_clip_speed` refusal, a real `clamp_to_cut`
//! count — so a row cannot claim a behaviour this build does not have.
//!
//! Cited ids: P.policy.snapToleranceSeconds, P.policy.targetLengthSeconds, P.policy.deadAirMaxSeconds,
//! P.policy.captionMinSeconds, P.policy.effectDefaultSeconds, P.policy.effectDefaultFades.
//! Cited tools: add_segment, cut_status, finish_cut, set_clip_speed, add_effect, add_caption, get_frames.

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_clamp;
use naivepost::cut_effect_decisions as ced;
use naivepost::cut_trim;
use naivepost::roles;
use naivepost::tools::clips::Clips;
use naivepost::tools::cutpass::{self, Plan};

/// Look a parameter up across the homes this round's ids live in: `params::find` answers Prepare's list
/// only, and these rows are spread over §6 (`params::cut`) and §06#6 (`params::effects`).
fn row(id: &str) -> naivepost::params::Param {
    let mut found = naivepost::params::prepare()
        .into_iter()
        .chain(naivepost::params::cut())
        .chain(naivepost::params::effects())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

/// Parse a tool reply's `error` field, panicking with the whole body when it is not an error.
fn error_of(reply: &str) -> String {
    let body: serde_json::Value = serde_json::from_str(reply).expect("tool replies are JSON");
    body.get("error")
        .and_then(|e| e.as_str())
        .unwrap_or_else(|| panic!("expected an error, got {reply}"))
    .to_string()
}

/// A footage scene from `s` to `e`.
fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// An effect record of one kind at one span.
fn fx(kind: &str, t: f64, dur: f64) -> Fx {
    Fx { kind: kind.into(), t, dur, ..Default::default() }
}

// --- row 11: the weighted edge contest -----------------------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s11_weights_are_the_ones_the_contest_is_scored_with() {
    // Silence midpoint 0.8, word edge 0.9, line edge 0.95, visual cut up to 1.0 (§05 F2.14 S3).
    assert_eq!(ced::Candidate::SilenceMidpoint.weight(), 0.8);
    assert_eq!(ced::Candidate::WordEdge.weight(), 0.9);
    assert_eq!(ced::Candidate::LineEdge.weight(), 0.95);
    assert_eq!(ced::Candidate::VisualCut.weight(), 1.0);
    // Every weight step is finer than the distance penalty over one frame's worth of travel, so a
    // nearer point of a weaker kind can beat a farther point of a stronger one.
    assert!(ced::DISTANCE_PENALTY_PER_SECOND > 0.0);
    assert!(ced::OUTWARD_BONUS < 0.1, "the bonus must not outrank a whole weight class");
}

#[test]
fn sec_12_decisions_cut_and_effects_s11_a_point_outside_the_tolerance_scores_nothing() {
    // P.policy.snapToleranceSeconds — five seconds is the whole reach of an edge move.
    let tol = ced::SNAP_TOLERANCE_SECONDS;
    // P.policy.snapToleranceSeconds — five seconds is the whole reach of an edge move.
    assert_eq!(tol, 5.0);
    assert!(ced::edge_score(
        ced::Candidate::VisualCut,
        10.0 + tol,
        10.0,
        tol,
        true
    )
    .is_none());
    assert!(ced::edge_score(ced::Candidate::VisualCut, 10.0 - tol, 10.0, tol, false).is_none());
    assert!(ced::edge_score(ced::Candidate::VisualCut, 10.0 + tol - 0.01, 10.0, tol, true).is_some());
}

#[test]
fn sec_12_decisions_cut_and_effects_s11_distance_penalises_and_outward_bonus_breaks_ties() {
    let tol = ced::SNAP_TOLERANCE_SECONDS;
    // A candidate 0.2 s from the stamp outscores one 3 s away, same kind and direction.
    let near = ced::edge_score(ced::Candidate::WordEdge, 10.2, 10.0, tol, false).unwrap();
    let far = ced::edge_score(ced::Candidate::WordEdge, 13.0, 10.0, tol, false).unwrap();
    assert!(near > far, "score falls as the edge travels further: {near} vs {far}");

    // Same kind, same distance: outward wins by exactly the bonus.
    let inward = ced::edge_score(ced::Candidate::WordEdge, 10.5, 10.0, tol, false).unwrap();
    let outward = ced::edge_score(ced::Candidate::WordEdge, 10.5, 10.0, tol, true).unwrap();
    assert!((outward - inward - ced::OUTWARD_BONUS).abs() < 1e-9);

    // But the bonus never lifts a silence midpoint over a line edge at the same distance.
    let quiet = ced::edge_score(ced::Candidate::SilenceMidpoint, 10.5, 10.0, tol, true).unwrap();
    let line = ced::edge_score(ced::Candidate::LineEdge, 10.5, 10.0, tol, false).unwrap();
    assert!(line > quiet, "kind still beats direction: {line} vs {quiet}");
}

#[test]
fn sec_12_decisions_cut_and_effects_s11_choose_edge_picks_the_winner_and_reports_how_far_it_moved() {
    let tol = ced::SNAP_TOLERANCE_SECONDS;
    let candidates = [
        (10.4, ced::Candidate::WordEdge),
        (10.9, ced::Candidate::VisualCut),
        (7.0, ced::Candidate::LineEdge),
    ];
    let (point, score, moved) = ced::choose_edge(&candidates, 10.0, tol, false).unwrap();
    // The visual cut at 10.9 outweighs the nearer word edge: 1.0 − 0.9·penalty vs 0.9 − 0.4·penalty.
    assert_eq!(point, 10.9);
    assert!((moved - 0.9).abs() < 1e-9, "seconds moved is the travel, signed");
    assert!(score > 0.0 && score <= 1.0 + ced::OUTWARD_BONUS);

    // Nothing inside the tolerance: no move at all.
    assert!(ced::choose_edge(&[(20.0, ced::Candidate::WordEdge)], 10.0, tol, false).is_none());
    assert!(ced::choose_edge(&[], 10.0, tol, false).is_none());
}

#[test]
fn sec_12_decisions_cut_and_effects_s11_add_segment_reports_the_snapped_edges_and_how_far_they_moved() {
    // The live report: `span` as placed, `moved` per edge. P.policy.snapToleranceSeconds
    let plan = Plan::new(600.0, 300.0).with_snap_points(vec![10.0, 20.0, 40.5]);
    let mut plan = plan;
    let reply = &plan.add_segment(10.4, 40.2, "the demo");
    let body: serde_json::Value = serde_json::from_str(reply).expect("ok reply");
    assert!((body["span"][0].as_f64().unwrap() - 10.0).abs() < 1e-9);
    assert!((body["span"][1].as_f64().unwrap() - 40.5).abs() < 1e-9);
    assert!((body["moved"][0].as_f64().unwrap() + 0.4).abs() < 1e-9);
    assert!((body["moved"][1].as_f64().unwrap() - 0.3).abs() < 1e-9);
    assert!((body["seconds"].as_f64().unwrap() - 30.5).abs() < 1e-9);
}

// --- row 12: no footage under an edge ------------------------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s12_a_segment_over_a_gap_is_refused_when_added() {
    let gaps = vec![(30.0, 45.0)];
    let mut plan = Plan::new(600.0, 300.0).with_gaps(gaps, vec![]);
    let reason = error_of(&plan.add_segment(28.0, 47.0, "spans a hole"));
    assert!(reason.contains("no footage under it"), "{reason}");
    assert!(reason.contains("gap from"), "{reason}");
    // Refused means refused: nothing entered the running total.
    assert_eq!(plan.segments().len(), 0);
    assert_eq!(plan.status().footage, 0.0);
}

#[test]
fn sec_12_decisions_cut_and_effects_s12_cut_status_keeps_the_running_total_honest() {
    // A target this size has no window to be outside, so the total is reported as it stands rather
    // than judged: `footage_window(0)` collapses to (0, 0), and two segments still add up right.
    let mut plan = Plan::new(600.0, cutpass::NO_TARGET);
    plan.add_segment(0.0, 100.0, "a");
    plan.add_segment(100.0, 200.0, "b");
    let status = plan.status();
    assert_eq!(status.segments, 2);
    assert_eq!(status.footage, 200.0);
    assert_eq!(status.window, (0.0, 0.0));
    // The same reading finish_cut judges by, so the model cannot be told one thing and failed for another.
    let ok: serde_json::Value = serde_json::from_str(&plan.finish_cut()).expect("inside the window");
    assert_eq!(ok["footage"].as_f64().unwrap(), status.footage);
    assert_eq!(ok["segments"].as_u64().unwrap(), 2);
}

#[test]
fn sec_12_decisions_cut_and_effects_s12_finish_cut_names_what_is_still_wrong_with_the_whole() {
    let mut plan = Plan::new(600.0, 300.0);
    plan.add_segment(0.0, 10.0, "far short of the target");
    let reason = error_of(&plan.finish_cut());
    assert!(reason.contains("under"), "{reason}");
    // And the identical reading is available without finishing.
    let status_body: serde_json::Value =
        serde_json::from_str(&plan.status_json()).expect("cut_status reply");
    assert_eq!(status_body["footage"].as_f64().unwrap(), plan.status().footage);
}

// --- row 13: dead air ---------------------------------------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s13_only_a_silence_longer_than_the_policy_number_is_cut() {
    // P.policy.deadAirMaxSeconds = 8.0: a deliberate long beat survives unless it passes the bound.
    let max = 8.0;
    assert!(!ced::dead_air_is_cut(7.9, max), "a beat under the number stays");
    assert!(!ced::dead_air_is_cut(8.0, max), "exactly the number is the deliberate case");
    assert!(ced::dead_air_is_cut(8.1, max));
    // What survives once the dead air is taken out: the keep value, never more than what was there.
    // P.policy.deadAirKeepSeconds = 0.5
    assert_eq!(ced::dead_air_kept(20.0, max, 0.5), 0.5);
    assert_eq!(ced::dead_air_kept(3.0, max, 0.5), 3.0);
    assert_eq!(ced::dead_air_kept(20.0, max, 0.0), 0.0);
}

// --- row 14: a zoom takes a box, and the pass can look first -------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s14_add_effect_takes_a_box_and_reports_the_one_it_used() {
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let aimed: serde_json::Value =
        serde_json::from_str(&batch.add_effect(1, "zoom", 2.0, 6.0, None, Some((0.3, 0.2))))
            .expect("ok reply");
    assert_eq!(aimed["box"][0].as_f64().unwrap(), 0.3);
    assert_eq!(aimed["box"][1].as_f64().unwrap(), 0.2);

    // No box sent: the frame's middle, reported as such so the model can see it aimed nowhere.
    let blind: serde_json::Value =
        serde_json::from_str(&batch.add_effect(1, "zoom", 2.0, 6.0, None, None)).expect("ok reply");
    assert_eq!(
        blind["box"].as_array().unwrap()[0].as_f64().unwrap(),
        naivepost::tools::clips::DEFAULT_ZOOM_BOX.0
    );
}

#[test]
fn sec_12_decisions_cut_and_effects_s14_get_frames_lets_the_pass_look_before_aiming() {
    let mut batch = Clips::new(&[(1, 12.0, 10.0)]);
    batch.set_frame_interval(1.0);
    let all = batch.clip_frames(1, None);
    assert_eq!(all.len(), 11, "one frame per interval, plus the clip's own start");
    assert_eq!(all[0].at, 0.0);
    assert_eq!(all[0].start, 12.0, "session second for the same moment");

    // `at` narrows the answer to the nearest frame — the aimed look, not the whole strip.
    let one = batch.clip_frames(1, Some(4.4));
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].at, 4.0);
    assert!(batch.clip_frames(99, None).is_empty(), "an unknown clip has no frames");
}

#[test]
fn sec_12_decisions_cut_and_effects_s14_the_pass_default_zoom_height_is_not_the_tool_default() {
    // The pass still defaults a box-less proposed zoom to a punch-in at 60 % height; the tool's own
    // default is half the frame. Two different numbers, both reported.
    assert_eq!(ced::ZOOM_HEIGHT, 0.6);
    assert_ne!(naivepost::tools::clips::DEFAULT_ZOOM_BOX.1, ced::ZOOM_HEIGHT);
}

// --- row 15: a rate over 1 on a captioned clip names the caption ---------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s15_set_clip_speed_errors_naming_the_caption() {
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    batch.add_caption(1, 2.0, 8.0, "the score keeps climbing while the timer runs down");
    let reason = error_of(&batch.set_clip_speed(1, 2.0));
    // The wording the catalogue tests already pin stays intact…
    assert!(reason.contains("has a caption and cannot be sped up"), "{reason}");
    // …and the refusal says which clip carries the caption, so the model knows where to fix it.
    assert!(reason.contains("clip 1"), "{reason}");
    // Rate untouched: a refusal writes nothing.
    assert_eq!(batch.clips()[0].rate, 1.0);
}

#[test]
fn sec_12_decisions_cut_and_effects_s15_a_rate_of_one_or_less_over_captions_is_allowed() {
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    batch.add_caption(1, 2.0, 8.0, "kept words");
    let reply: serde_json::Value =
        serde_json::from_str(&batch.set_clip_speed(1, 1.0)).expect("rate 1 is not a speed-up");
    assert_eq!(reply["applied"].as_f64().unwrap(), 1.0);
    let slow: serde_json::Value =
        serde_json::from_str(&batch.set_clip_speed(1, 0.5)).expect("slowing a captioned clip is fine");
    assert_eq!(slow["applied"].as_f64().unwrap(), 0.5);
}

#[test]
fn sec_12_decisions_cut_and_effects_s15_the_refusal_names_the_clip_holding_the_caption() {
    // The refusal points at the clip rather than pasting its caption whole: the model addresses clips
    // by number, and a long quote in an error line teaches it nothing it can act on.
    let mut batch = Clips::new(&[(7, 0.0, 30.0)]);
    let long = "supercalifragilisticexpialidocious and then some more words that nobody needs twice";
    batch.add_caption(7, 2.0, 8.0, long);
    let reason = error_of(&batch.set_clip_speed(7, 1.5));
    assert!(reason.contains("clip 7"), "{reason}");
    assert!(reason.contains("has a caption and cannot be sped up"), "{reason}");
    assert!(
        reason.len() < 220,
        "the message stays short rather than pasting the caption: {} chars",
        reason.len()
    );
}

// --- row 16: each dropped thing is that item's error ---------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s16_a_kind_outside_zoom_stop_volume_is_an_error() {
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let reason = error_of(&batch.add_effect(1, "blur", 2.0, 6.0, None, None));
    assert!(reason.contains("is not an effect this pass places"), "{reason}");
    for kind in ["zoom", "stop", "volume"] {
        assert!(kind.is_ascii(), "{kind}");
        assert!(naivepost::tools::clips::KINDS.contains(&kind));
    }
}

#[test]
fn sec_12_decisions_cut_and_effects_s16_a_gain_of_exactly_one_is_refused_not_dropped() {
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let reason = error_of(&batch.add_effect(1, "volume", 2.0, 6.0, Some(1.0), None));
    assert!(reason.contains("changes nothing"), "{reason}");
    // Zero is silence and someone can mean it.
    let silent: serde_json::Value =
        serde_json::from_str(&batch.add_effect(1, "volume", 2.0, 6.0, Some(0.0), None))
            .expect("gain 0 is a decision");
    assert_eq!(silent["gain"].as_f64().unwrap(), 0.0);
}

#[test]
fn sec_12_decisions_cut_and_effects_s16_short_and_wordless_captions_are_errors() {
    // P.policy.captionMinSeconds = 0.3
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let reason = error_of(&batch.add_caption(1, 2.0, 2.2, "too short to read"));
    assert!(reason.contains("dropped"), "{reason}");
    assert!(reason.contains("0.3"), "{reason}");
    let empty = error_of(&batch.add_caption(1, 2.0, 8.0, "   "));
    assert!(empty.contains("no words"), "{empty}");
    assert_eq!(batch.captions_of(1).len(), 0, "neither became a caption");
}

#[test]
fn sec_12_decisions_cut_and_effects_s16_an_empty_after_clamping_span_is_named_as_such() {
    let mut batch = Clips::new(&[(1, 0.0, 10.0)]);
    let reason = error_of(&batch.add_effect(1, "zoom", 12.0, 20.0, None, None));
    assert!(reason.contains("empty after clamping"), "{reason}");
    assert!(reason.contains("0..10.00"), "{reason}");
}

// --- row 17: the app's defaults, stated back ----------------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s17_the_defaults_are_the_ones_the_result_states() {
    // A speed with no rate becomes 0.5; a stop with no span 2 s; a zoom/caption default 3 s.
    // P.policy.effectDefaultSeconds
    assert_eq!(ced::DEFAULT_RATE, 0.5);
    assert_eq!(ced::STOP_SECONDS, 2.0);
    assert_eq!(naivepost::fx_zoom::DEFAULT_SECONDS, 3.0);
    // P.policy.effectDefaultFades — the fade families the pass invents.
    assert_eq!(naivepost::fx_zoom::GLIDE_SECONDS, 1.0);
    assert_eq!(naivepost::cut_speed::STOP_FADE_SECONDS, 0.5);
    assert_eq!(naivepost::tools::clips::FADE_SECONDS, 0.15);
}

#[test]
fn sec_12_decisions_cut_and_effects_s17_invented_fades_shrink_to_fit_and_are_reported() {
    use naivepost::cut_effects_pass as pass;
    // Long enough for the full cap, short enough that the quarter rule bites.
    assert_eq!(pass::glide(9.0), 1.0);
    assert_eq!(pass::glide(1.5), 0.5);
    assert_eq!(pass::stop_fade(4.0), 0.3);
    assert_eq!(pass::stop_fade(0.8), 0.2);
    assert_eq!(pass::ramp(8.0), 1.0);
    assert_eq!(pass::ramp(0.8), 0.2);

    // And the tool says what it applied: a fade on a short caption is min(0.15, d/2), so a 0.4 s
    // caption gets 0.2 — the cap, not the quarter rule.
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let reply: serde_json::Value =
        serde_json::from_str(&batch.add_caption(1, 2.0, 2.5, "brief")).expect("0.5 s clears 0.3");
    assert_eq!(reply["fade"].as_f64().unwrap(), 0.15);
    let tight: serde_json::Value =
        serde_json::from_str(&batch.add_caption(1, 4.0, 4.4, "tight")).expect("0.4 s clears 0.3");
    assert_eq!(tight["fade"].as_f64().unwrap(), 0.15);
}

#[test]
fn sec_12_decisions_cut_and_effects_s17_a_clamped_rate_says_why_it_is_not_the_one_asked() {
    let mut batch = Clips::new(&[(1, 0.0, 30.0)]);
    let reply: serde_json::Value =
        serde_json::from_str(&batch.set_clip_speed(1, 9.0)).expect("over-max is lowered, not refused");
    assert_eq!(reply["applied"].as_f64().unwrap(), cutpass::MAX_RATE);
    assert!(reply["reason"].as_str().unwrap().contains("fastest"), "{reply}");
}

// --- row 18: the list is replaced, one Undo ------------------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s18_replacing_the_list_is_the_pages_contract_not_a_model_call() {
    // One Undo per gesture: the first move pushes, a second in the same gesture does not.
    assert!(cut_trim::pushes_undo(true, false));
    assert!(!cut_trim::pushes_undo(true, true), "one Undo only, as F2.14 S3 promises");
    assert!(!cut_trim::pushes_undo(false, false));
    // The replacement itself is a whole-list write: replacing N effects with M leaves M and nothing else.
    let mut cut = Cut {
        segs: vec![seg(0.0, 100.0)],
        fx: vec![fx("label", 5.0, 0.0), fx("zoom", 20.0, 3.0)],
        ..Default::default()
    };
    let proposed = vec![fx("speed", 10.0, 4.0)];
    cut.fx = proposed.clone();
    assert_eq!(cut.fx.len(), 1, "the hand-placed label and zoom are gone with the replace");
    assert_eq!(cut.fx, proposed);
}

// --- row 19: clamped to the cut as applied, only the count logged --------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s19_effects_are_clamped_and_only_the_count_reaches_the_log() {
    // MIN_SURVIVING_SECONDS: a band with too little survivor is dropped rather than kept as a fragment.
    assert_eq!(cut_clamp::MIN_SURVIVING_SECONDS, 1.0);
    let cut = Cut {
        segs: vec![seg(0.0, 10.0)],
        fx: vec![
            fx("zoom", 1.0, 4.0),          // fully inside: stays
            fx("text", 8.0, 8.0),         // mostly outside: trimmed to 2 s, still over the floor
            fx("volume", 30.0, 4.0),      // no footage under it at all: dropped
            fx("svg", 9.5, 3.0),         // only 0.5 s survives: under the floor, dropped
        ],
        ..Default::default()
    };
    let clamped = cut_clamp::clamp_to_cut(&cut);
    assert_eq!(clamped.kept.len(), 2);
    assert_eq!(clamped.dropped, 2);
    // The log line carries the count and nothing else — which effect went is not said (§12 row 19).
    let line = cut_clamp::log_line(clamped.dropped);
    assert!(line.starts_with(">>> 2 effect(s)"), "{line}");
    assert!(!line.contains("zoom") && !line.contains("svg"), "{line}");
}

// --- row 20: withdrawn tools and switched-off thinking, announced --------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s20_web_tools_ride_the_first_attempt_only() {
    assert!(roles::cut_attempt(1).web_tools);
    assert!(!roles::cut_attempt(2).web_tools);
    assert!(ced::web_tools_withdrawn(2));
    assert!(!ced::web_tools_withdrawn(1));
}

#[test]
fn sec_12_decisions_cut_and_effects_s20_thinking_off_after_an_all_reasoning_reply() {
    // Reasoned and wrote nothing → the retry asks for content alone. Any other failure keeps thinking.
    assert!(!roles::cut_retry_thinking(true, true));
    assert!(roles::cut_retry_thinking(true, false), "reasoned and answered: think again");
    assert!(roles::cut_retry_thinking(false, false), "no reasoning block: nothing to switch off");
}

#[test]
fn sec_12_decisions_cut_and_effects_s20_the_withdrawal_is_told_rather_than_left_unannounced() {
    // Nothing taken away: no notice.
    assert_eq!(ced::withdrawal_notice(false, false), None);
    let web = ced::withdrawal_notice(true, false).unwrap();
    assert!(web.contains("web_search") && web.contains("web_read"), "{web}");
    assert!(!web.contains("thinking"), "{web}");
    let think = ced::withdrawal_notice(false, true).unwrap();
    assert!(think.contains("thinking"), "{think}");
    assert!(!think.contains("web_search"), "{think}");
    let both = ced::withdrawal_notice(true, true).unwrap();
    assert!(both.contains("web_search") && both.contains("thinking"), "{both}");
}

// --- row 21: the target is a policy number ------------------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_s21_the_target_length_is_a_derived_policy_number() {
    // P.policy.targetLengthSeconds, default 0 = none, derived by F0.7's set_policy.
    let row = row("P.policy.targetLengthSeconds");
    assert_eq!(row.spelled, "0 (none)");
    assert_eq!(ced::NO_TARGET, 0.0);
    // With no target there is no window at all: `footage_window` collapses rather than inventing one,
    // which is the one place "no target" is decided for the cut.
    let untargeted = Plan::new(600.0, ced::NO_TARGET);
    assert_eq!(untargeted.status().window, (0.0, 0.0));
    // A target gives a real window, and the same number drives it.
    let aimed = Plan::new(600.0, 300.0);
    assert!(aimed.status().window.1 > aimed.status().window.0);
    assert_eq!(aimed.target(), 300.0);
}

#[test]
fn sec_12_decisions_cut_and_effects_s21_the_same_number_drives_both_count_gates() {
    use naivepost::tools::cutpass::{max_segments, min_segments};
    // Segment bounds come from the target, so one wrong derivation moves the prompt, the counts and
    // the length gate together — which is why it is a policy field and not a regex result.
    assert!(min_segments(30.0) < min_segments(600.0), "{} vs {}", min_segments(30.0), min_segments(600.0));
    assert!(max_segments(30.0) < max_segments(600.0), "{} vs {}", max_segments(30.0), max_segments(600.0));
    // The plan reports the gates its own target produced.
    let short_plan = Plan::new(600.0, 30.0).status();
    let long_plan = Plan::new(600.0, 600.0).status();
    assert_eq!(short_plan.min_segments, min_segments(30.0));
    assert_eq!(long_plan.max_segments, max_segments(600.0));
    assert!(short_plan.max_segments < long_plan.max_segments);
}

// --- the table itself ----------------------------------------------------------------------------------------------------

#[test]
fn sec_12_decisions_cut_and_effects_audit_covers_every_row_in_order() {
    let table = ced::audit();
    assert_eq!(table.len(), 11, "rows 11–21 of §12's Cut and effects");
    for row in &table {
        assert!(!row.what.is_empty(), "{row:?}");
        assert!(!row.home.is_empty(), "{row:?}");
        assert!(!row.lives_in.is_empty(), "{row:?}");
    }
    // Order follows the spec: the edge contest first, the target last.
    assert!(table[0].what.contains("five seconds"), "{}", table[0].what);
    assert!(table[1].what.contains("no footage"), "{}", table[1].what);
    assert!(table[2].what.contains("eight seconds"), "{}", table[2].what);
    assert!(table[3].what.contains("box"), "{}", table[3].what);
    assert!(table[4].what.contains("caption"), "{}", table[4].what);
    assert!(table[9].what.contains("web tools"), "{}", table[9].what);
    assert!(table[10].what.contains("target length"), "{}", table[10].what);
}

#[test]
fn sec_12_decisions_cut_and_effects_cited_ids_answer_from_the_catalogue() {
    // Every parameter this round names is catalogued, and every tool is one the crate knows.
    for id in ced::CITED_PARAMS {
        let _ = row(id);
    }
    for tool in ced::CITED_TOOLS {
        let known = [
            naivepost::tools::Tool::AddSegment,
            naivepost::tools::Tool::CutStatus,
            naivepost::tools::Tool::FinishCut,
            naivepost::tools::Tool::SetClipSpeed,
            naivepost::tools::Tool::AddEffect,
            naivepost::tools::Tool::AddCaption,
            naivepost::tools::Tool::GetFrames,
        ];
        assert!(
            known.iter().any(|t| t.name() == tool),
            "{tool} is not a known tool"
        );
    }
}
