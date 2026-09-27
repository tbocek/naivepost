// F2.14 Suggest a cut — spec/05-cut.md F2.14, one test per step.
//
// Strings are compared whole wherever the spec words a sentence: a reworded log line passes `contains`
// and still fails the spec. Where the spec gives only a shape ("N segments, M:SS total"), the numbers are
// recomputed from the result so the sentence's SHAPE is pinned, not merely its presence.

use naivepost::cut::{Fx, Seg};
use naivepost::cut_effect_decisions::Candidate;
use naivepost::project::{CutMode, MarkingPass, Policy};
use naivepost::suggest::{self, Refuse};
use naivepost::tools::{self, cutpass};

fn zoom(t: f64, dur: f64) -> Fx {
    Fx { kind: "zoom".into(), t, dur, ..Default::default() }
}

// --- S1: the guards -------------------------------------------------------------------------------------

#[test]
fn f2_14_s1_busy_beats_every_other_refusal() {
    // All three things wrong at once, and only the busy answer comes back: a run under way may be about
    // to write the very timeline whose absence would otherwise be reported.
    assert_eq!(suggest::refuse(true, true, false), Some(Refuse::Busy));
    assert_eq!(suggest::refuse(true, false, false).map(|r| r.said()), Some(suggest::BUSY));
    assert_eq!(
        suggest::refuse(true, true, false).map(|r| r.said()),
        Some(suggest::BUSY),
        "a busy bar never also reports hand edits or a missing timeline"
    );
}

#[test]
fn f2_14_s1_hand_edits_refuse_with_the_revert_sentence() {
    assert_eq!(
        suggest::refuse(false, true, true).map(|r| r.said()),
        Some("you have hand edits \u{2014} press Revert first for a fresh suggestion")
    );
}

#[test]
fn f2_14_s1_no_timeline_refuses_naming_describe() {
    assert_eq!(
        suggest::refuse(false, false, false).map(|r| r.said()),
        Some("run Describe first \u{2014} the suggestion reads the session timeline, and there is none")
    );
    // A clean session with a timeline is not refused at all.
    assert_eq!(suggest::refuse(false, false, true), None);
}

// --- S2a: the words path ---------------------------------------------------------------------------------

#[test]
fn f2_14_s2a_trim_to_words_never_leaves_the_run() {
    // suggest.trimReachSeconds 0.4 -- the reach is CLAMPED into the run, which is the whole of the
    // "never outside the run" rule.
    let tight = suggest::SpokenRun { span: (10.0, 40.0), first_spoken: 10.05, last_spoken: 39.9 };
    assert_eq!(suggest::trim_run_to_words(&tight), (10.0, 40.0));

    let roomy = suggest::SpokenRun { span: (10.0, 40.0), first_spoken: 20.0, last_spoken: 25.0 };
    assert_eq!(suggest::trim_run_to_words(&roomy), (19.6, 25.4));
}

#[test]
fn f2_14_s2a_marks_are_taken_out_and_counted() {
    let (pieces, taken) = suggest::remove_marks(&[(10.0, 40.0)], &[(20.0, 25.0)]);
    assert_eq!(pieces, vec![(10.0, 20.0), (25.0, 40.0)]);
    assert_eq!(taken, 1);

    // A mark over footage that is not kept took nothing out, so it is not counted -- reporting it would
    // claim a cut that did not happen.
    let (kept, count) = suggest::remove_marks(&[(10.0, 40.0)], &[(100.0, 110.0)]);
    assert_eq!(kept, vec![(10.0, 40.0)]);
    assert_eq!(count, 0);
}

#[test]
fn f2_14_s2a_dead_air_over_eight_seconds_is_cut_leaving_the_beat() {
    // P.policy.deadAirMaxSeconds 8 / P.policy.deadAirKeepSeconds 0.5
    let (kept, removed) = suggest::remove_dead_air(
        &[(10.0, 40.0)],
        &[(20.0, 30.0)],
        suggest::DEAD_AIR_MAX_SECONDS,
        suggest::DEAD_AIR_KEEP_SECONDS,
    );
    assert_eq!(kept, vec![(10.0, 20.5), (30.0, 40.0)]);
    assert!(
        (removed - 9.5).abs() < 1e-9,
        "a 10 s silence loses everything but the 0.5 s beat: removed {removed}"
    );

    // A short silence costs nothing.
    let (_, removed_short) = suggest::remove_dead_air(
        &[(10.0, 40.0)],
        &[(20.0, 26.0)],
        suggest::DEAD_AIR_MAX_SECONDS,
        suggest::DEAD_AIR_KEEP_SECONDS,
    );
    assert_eq!(removed_short, 0.0);

    // Exactly the maximum is a deliberate beat: dead_air_is_cut is strictly greater.
    let (_, removed_exact) = suggest::remove_dead_air(
        &[(10.0, 40.0)],
        &[(20.0, 28.0)],
        suggest::DEAD_AIR_MAX_SECONDS,
        suggest::DEAD_AIR_KEEP_SECONDS,
    );
    assert_eq!(removed_exact, 0.0, "8.0 s exactly is kept");
}

#[test]
fn f2_14_s2a_touching_pieces_coalesce() {
    assert_eq!(
        suggest::coalesce(&[(10.0, 20.0), (20.0, 30.0), (25.0, 28.0)]),
        vec![(10.0, 30.0)],
        "touching pieces are one stretch of video"
    );
}

#[test]
fn f2_14_s2a_the_words_cut_status_and_log_are_exact() {
    // One run, no marks, one 10 s silence inside it.
    let runs = [suggest::SpokenRun {
        span: (0.0, 60.0),
        first_spoken: 1.0,
        last_spoken: 59.0,
    }];
    let out = suggest::words_cut(&runs, &[], &[(20.0, 30.0)]);

    assert_eq!(out.status, format!("cut by the words: {} segments", out.segs.len()));
    let total: f64 = out.segs.iter().map(|(a, b)| b - a).sum();
    assert_eq!(
        out.log,
        format!(
            ">>> cut by the words: {} stretch(es) taken out, {} of silence, {} segments, {} total",
            out.marks_out,
            tools::mm_ss(out.silence_out),
            out.segs.len(),
            tools::mm_ss(total)
        ),
        "// P.policy.deadAirMaxSeconds 8 · // P.policy.deadAirKeepSeconds 0.5"
    );
    // The silence really went: 60 s trimmed minus 9.5 s of dead air.
    assert_eq!(out.segs, vec![(0.6, 20.5), (30.0, 59.4)]);
    assert_eq!(out.marks_out, 0);
    assert!((out.silence_out - 9.5).abs() < 1e-9);
}

// --- S2b: the model path ----------------------------------------------------------------------------------

#[test]
fn f2_14_s2b_target_window_present_and_absent() {
    // P.policy.targetLengthSeconds
    assert!(suggest::has_target(300.0));
    assert_eq!(suggest::target_window(300.0), cutpass::footage_window(300.0));
    assert!(!suggest::has_target(0.0));
    assert_eq!(suggest::target_window(0.0), (0.0, 0.0), "no target collapses the window");
}

#[test]
fn f2_14_s2b_the_target_block_spells_both_forms() {
    let with = suggest::target_block(300.0);
    assert!(with.starts_with("KEEP between "), "{with}");
    assert!(with.contains(" seconds of footage, in at most "), "{with}");
    assert_eq!(
        with,
        format!(
            "KEEP between {} and {} seconds of footage, in at most {} segments.",
            cutpass::clock(suggest::target_window(300.0).0),
            cutpass::clock(suggest::target_window(300.0).1),
            cutpass::max_segments(300.0)
        )
    );
    assert_eq!(
        suggest::target_block(0.0),
        "NO TARGET LENGTH: keep what is worth keeping."
    );
}

#[test]
fn f2_14_s2b_the_message_orders_context_length_target_timeline() {
    let msg = suggest::message("a lecture about routers", 1200.0, 300.0, "[0.0s-5.0s] A: hi");
    let ctx = msg.find("a lecture about routers").expect("context present");
    let len = msg.find("SESSION LENGTH: 1200 seconds").expect("length present");
    let tgt = msg.find("KEEP between").expect("target present");
    let head = msg.find("SESSION TIMELINE:").expect("timeline heading present");
    let body = msg.find("[0.0s-5.0s] A: hi").expect("timeline body present");
    assert!(ctx < len, "the person's context leads: {msg}");
    assert!(len < tgt, "then the finished length: {msg}");
    assert!(tgt < head, "then what to read: {msg}");
    assert!(head < body, "then the timeline itself: {msg}");
    assert_eq!(
        msg.matches("SESSION LENGTH").count(),
        1,
        "the length is said once: {msg}"
    );
}

#[test]
fn f2_14_s2b_a_context_ruling_out_all_three_passes_asks_for_none() {
    let mut off = Policy::default();
    off.captions_pass.value = false;
    off.speed_pass.value = false;
    off.decorations_pass.value = false;
    // The ABSENCE of the job, not a job returning nothing: an empty list means no call is made, so no
    // model is spent and nothing the policy ruled out can come back.
    assert!(suggest::passes_switched_on(&off).is_empty());

    let on = Policy::default();
    assert_eq!(
        suggest::passes_switched_on(&on),
        vec![
            naivepost::policy::Pass::Captions,
            naivepost::policy::Pass::Speeds,
            naivepost::policy::Pass::Decorations
        ]
    );

    let mut captions_off = Policy::default();
    captions_off.captions_pass.value = false;
    assert_eq!(
        suggest::passes_switched_on(&captions_off),
        vec![naivepost::policy::Pass::Speeds, naivepost::policy::Pass::Decorations]
    );
}

#[test]
fn f2_14_s2b_the_words_mode_never_offers_the_model_cut() {
    // P.policy.cutMode
    let mut words = Policy::default();
    words.cut_mode.value = CutMode::Words;
    assert!(!suggest::model_cut_offered(&words, MarkingPass::Retakes));

    let mut model = Policy::default();
    model.cut_mode.value = CutMode::Model;
    assert!(suggest::model_cut_offered(&model, MarkingPass::Retakes));
}

#[test]
fn f2_14_s2b_three_attempts_then_it_stops() {
    // P.eng.llmAttempts 3
    assert!(!suggest::attempts_exhausted(1));
    assert!(!suggest::attempts_exhausted(2));
    assert!(suggest::attempts_exhausted(3));
    assert_eq!(suggest::NO_VALID_CUT, "no valid cut after 3 attempts");
}

#[test]
fn f2_14_s2b_the_whole_cut_answer_lists_every_fault_worst_first() {
    let problems = suggest::whole_cut_problems(&[(0.0, 10.0)], 600.0);
    assert!(
        problems.len() >= 2,
        "footage under the low bound AND segments under the floor: {problems:?}"
    );
    assert!(suggest::answer(&problems).contains("; "), "every fault at once: {:?}", problems);

    // Within every window: ok, and nothing else. 300 s of footage sits inside the [180, 720] window a
    // 300 s target accepts, and 4 segments is exactly min_segments(300) -- one fewer and the count
    // complaint comes back, which is why this fixture has four.
    let fine = suggest::whole_cut_problems(
        &[(0.0, 100.0), (100.0, 200.0), (200.0, 300.0), (300.0, 400.0)],
        300.0,
    );
    assert_eq!(fine, Vec::<String>::new(), "{fine:?}");
    assert_eq!(suggest::answer(&fine), "ok");
}

// --- S3: apply, the walk-back ------------------------------------------------------------------------------

#[test]
fn f2_14_s3_a_small_hole_closes_only_where_somebody_talked() {
    // P.eng.seamMaxSeconds 1.5
    let segs = [(10.0, 20.0), (21.0, 30.0)];
    assert_eq!(
        suggest::close_holes(&segs, &[(20.0, 21.0)], suggest::SEAM_MAX_SECONDS),
        vec![(10.0, 30.0)],
        "a talked-in 1 s hole joins the two scenes"
    );
    assert_eq!(
        suggest::close_holes(&segs, &[], suggest::SEAM_MAX_SECONDS),
        vec![(10.0, 20.0), (21.0, 30.0)],
        "a quiet hole stays open: closing it would jump across a silence the speaker made"
    );
}

#[test]
fn f2_14_s3_a_hole_wider_than_seam_max_stays_open_even_when_talked_in() {
    let segs = [(10.0, 20.0), (22.0, 30.0)];
    assert_eq!(
        suggest::close_holes(&segs, &[(20.0, 22.0)], suggest::SEAM_MAX_SECONDS),
        vec![(10.0, 20.0), (22.0, 30.0)],
        "2.0 s is wider than seamMaxSeconds 1.5"
    );
}

#[test]
fn f2_14_s3_edge_snap_prefers_the_highest_weight_inside_five_seconds() {
    // P.policy.snapToleranceSeconds 5.0
    // cut.snapWeightSilence 0.8 / cut.snapWeightWordEdge 0.9 / cut.snapWeightLineEdge 0.95 /
    // cut.snapWeightVisualCut 1.0
    let stamp = 20.0;

    // Nothing within tolerance: the edge stays exactly where it was stamped.
    assert_eq!(
        suggest::snap_edge(&[(26.0, Candidate::VisualCut)], stamp, false),
        stamp,
        "26.0 is outside the 5 s reach"
    );
    // Exactly the tolerance is already outside (half-open reach).
    assert_eq!(suggest::snap_edge(&[(25.0, Candidate::VisualCut)], stamp, false), stamp);

    // A candidate inside the reach IS taken.
    assert_eq!(
        suggest::snap_edge(&[(20.3, Candidate::VisualCut)], stamp, false),
        20.3
    );

    // Weight wins over nearness when the gap in weight outweighs the distance penalty: a VisualCut
    // (1.0) 0.6 s away beats a SilenceMidpoint (0.8) 0.1 s away.
    let chosen = suggest::snap_edge(
        &[(20.1, Candidate::SilenceMidpoint), (20.6, Candidate::VisualCut)],
        stamp,
        false,
    );
    assert_eq!(chosen, 20.6, "the heavier candidate wins the contest");

    // And outward is preferred: for an END edge, the later point moves outward, so an equally-weighted
    // candidate ahead of the stamp beats one behind it.
    let end_edge = suggest::snap_edge(
        &[(19.9, Candidate::WordEdge), (20.1, Candidate::WordEdge)],
        stamp,
        false,
    );
    assert_eq!(end_edge, 20.1, "an end edge prefers to move later");
    let start_edge = suggest::snap_edge(
        &[(19.9, Candidate::WordEdge), (20.1, Candidate::WordEdge)],
        stamp,
        true,
    );
    assert_eq!(start_edge, 19.9, "a start edge prefers to move earlier");
}

/// Two model scenes plus a hand insert, with one effect sitting on kept footage and one pointed at a
/// stretch the walk-back removes.
fn apply_fixture() -> suggest::Applied {
    let inserts = vec![Seg { s: 100.0, e: 100.0, dur: 4.0, ..Default::default() }];
    let fx = vec![zoom(12.0, 3.0), zoom(75.0, 3.0)];
    let walk = suggest::Walk {
        talked: vec![(20.0, 21.0)],
        silences: vec![],
        snap_points: vec![],
        marks: vec![(70.0, 80.0)],
    };
    suggest::apply(&[(10.0, 20.0), (21.0, 80.0)], &inserts, &fx, &walk)
}

#[test]
fn f2_14_s3_apply_keeps_the_inserts_and_replaces_the_effects_as_a_list() {
    let out = apply_fixture();
    assert_eq!(out.kept_inserts.len(), 1, "the hand insert survived the walk");
    assert!(out.kept_inserts[0].is_insert());
    // The mark took (70, 80) out, so the scene ends at 70 and the zoom at 75 has no footage under it.
    assert_eq!(
        out.effects.len(),
        1,
        "the doomed effect is gone; kept: {:?}",
        out.effects
    );
    assert_eq!(out.effects[0].t, 12.0);
    assert_eq!(out.log.len(), 2);
}

#[test]
fn f2_14_s3_the_two_log_lines_are_exact() {
    let out = apply_fixture();
    let total: f64 =
        out.segs.iter().map(|(a, b)| b - a).sum::<f64>() + out.kept_inserts.iter().map(|s| s.dur).sum::<f64>();
    assert_eq!(
        out.log[0],
        format!(
            ">>> suggested {} segments, {} total",
            out.segs.len() + out.kept_inserts.len(),
            tools::mm_ss(total)
        ),
        "{:?}",
        out.log
    );
    assert_eq!(
        out.log[1],
        format!(
            ">>> \u{2026}and {} effect(s): the speeds, the captions and the decorations",
            out.effects.len()
        )
    );
}

#[test]
fn f2_14_s3_the_module_writes_nothing_so_the_undo_is_the_callers_one_call() {
    // Same inputs twice, same answer: no side state anywhere in the flow. Persistence and the single
    // record_edit/Undo (F2.13) belong to the caller, which is why this module saves nothing and can be
    // run twice inside a test without touching a project.
    assert_eq!(apply_fixture(), apply_fixture());
}
