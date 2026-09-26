//! F2.2 — ▶✂ Play the cut: the logic, one test per branch of spec/05-cut.md §F2.2's flowchart.
//!
//! Pure state and rules from `naivepost::cut_play`; no display, no GTK. The wire through the real
//! button is proven separately in tests/cut_play_the_cut_widgets.rs.

use naivepost::cut::{Cut, Seg};
use naivepost::preview::Player;
use naivepost::{cut_play, shell};

/// Two kept clips with a removed stretch between them: 0-10 kept, 10-20 dropped, 20-30 kept.
fn two_clips_with_a_hole() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 20.0, e: 30.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

fn player(cut_only: bool, reviewing: bool, playing: bool, playhead: Option<f64>) -> Player {
    Player {
        transport: naivepost::run::Transport { playing, started: playing },
        cut_only,
        reviewing,
        playhead,
        volume: None,
        footage_muted: false,
    }
}

// --- S1: greyed with no clips -------------------------------------------------------------------

#[test]
fn f2_2_s1_no_clips_refuses_and_greys_the_button() {
    // P.eng.preloadLeadSeconds is exercised in f2_2_s5_preloads_the_next_jump_three_seconds_ahead.
    let empty = Cut::default();
    assert!(cut_play::empty(&empty), "a cut with no segs has nothing for \u{25b6}\u{2702} to play");
    assert!(
        !naivepost::cut_screen::can_play_cut(&empty),
        "the greyed rule and the refusal ask one function the same question"
    );

    let mut p = player(false, false, false, Some(0.0));
    match cut_play::pressed(&mut p, &empty) {
        cut_play::Pressed::Refused(reason) => {
            assert_eq!(reason, cut_play::NOTHING_TO_PLAY, "S2's own words for an empty cut");
        }
        other => panic!("an empty cut must refuse, got {other:?}"),
    }
    assert!(!p.cut_only, "a refused press does not switch the preview behind the person's back");
    assert_eq!(p.playhead, Some(0.0), "nor does it move the line");
}

#[test]
fn f2_2_s1_other_ways_in_name_the_recording_instead() {
    // Space / picture click / run bar cannot switch the preview, so their refusal points at ▶.
    assert_eq!(
        cut_play::EMPTY_CUT_REFUSAL,
        "the cut is empty \u{2014} add a clip to play it, or press \u{25b6} to play the recording instead"
    );
    assert_ne!(
        cut_play::EMPTY_CUT_REFUSAL, cut_play::NOTHING_TO_PLAY,
        "the two refusals are different sentences for different paths, not one reused"
    );
}

// --- S2: the preview is the recording → switch to the cut --------------------------------------

#[test]
fn f2_2_s2_switch_from_the_recording_snaps_the_line_onto_kept_material() {
    let cut = two_clips_with_a_hole();
    // The line sits in the removed stretch (15 s): switching must land it on kept material.
    let mut p = player(false, false, true, Some(15.0));
    match cut_play::pressed(&mut p, &cut) {
        cut_play::Pressed::SwitchedToCut { kept_playing, status } => {
            assert!(kept_playing, "it was playing, so it carries on playing");
            assert_eq!(status, cut_play::SWITCH_STATUS, "and says the clock just changed meaning");
        }
        other => panic!("expected a switch, got {other:?}"),
    }
    assert!(p.cut_only && !p.reviewing, "the preview is now the plain cut");
    assert!(p.transport.playing, "switching never pauses");
    assert_eq!(p.playhead, Some(20.0), "15 s is inside a dropped stretch -> the next clip's start");
}

#[test]
fn f2_2_s2_switch_while_paused_stays_paused() {
    let cut = two_clips_with_a_hole();
    let mut p = player(false, false, false, Some(5.0));
    match cut_play::pressed(&mut p, &cut) {
        cut_play::Pressed::SwitchedToCut { kept_playing, .. } => {
            assert!(!kept_playing, "nothing was playing, so nothing starts")
        }
        other => panic!("expected a switch, got {other:?}"),
    }
    assert_eq!(p.playhead, Some(5.0), "5 s is already kept material, so the line stays put");
    assert!(!p.transport.playing);
}

#[test]
fn f2_2_s2_snap_past_the_last_clip_lands_on_the_last_clip_start() {
    let cut = two_clips_with_a_hole();
    // Past everything: parking the line at 45 s would show nothing, so it goes back to real material.
    assert_eq!(cut_play::snap_to_kept(&cut.segs, 45.0), 20.0);
    assert_eq!(cut_play::snap_to_kept(&cut.segs, 12.0), 20.0, "inside the hole -> forward");
    assert_eq!(cut_play::snap_to_kept(&cut.segs, 25.0), 25.0, "inside a clip -> unchanged");
    assert_eq!(cut_play::snap_to_kept(&[], 3.0), 0.0, "no material at all -> second zero");
}

// --- S3: review running → end it, carry on as the plain cut ----------------------------------

#[test]
fn f2_2_s3_ends_a_running_review_and_carries_on_as_the_plain_cut() {
    let cut = two_clips_with_a_hole();
    let mut p = player(true, true, true, Some(22.0));
    match cut_play::pressed(&mut p, &cut) {
        cut_play::Pressed::ReviewEnded { status } => {
            assert_eq!(status, cut_play::REVIEW_ENDED_STATUS);
        }
        other => panic!("a running review must end, got {other:?}"),
    }
    assert!(!p.reviewing, "S3: the review is ended");
    assert!(p.cut_only, "and what remains is the cut preview");
    assert!(p.transport.playing, "\"carry on\" means no pause");
    assert_eq!(p.playhead, Some(22.0), "the line was on kept material, so it is untouched");
}

#[test]
fn f2_2_s3_ending_a_review_also_snaps_a_line_left_in_a_hole() {
    let cut = two_clips_with_a_hole();
    let mut p = player(true, true, true, Some(17.0));
    let _ = cut_play::pressed(&mut p, &cut);
    assert_eq!(p.playhead, Some(20.0), "the plain cut cannot sit on a removed stretch either");
}

// --- S4: else toggle --------------------------------------------------------------------------

#[test]
fn f2_2_s4_toggles_play_and_pause_when_already_the_cut() {
    let cut = two_clips_with_a_hole();

    let mut playing = player(true, false, true, Some(22.0));
    match cut_play::pressed(&mut playing, &cut) {
        cut_play::Pressed::Toggled(cut_play::Toggle::Paused) => {}
        other => panic!("playing -> pause expected, got {other:?}"),
    }
    assert!(!playing.transport.playing);
    assert!(playing.cut_only, "pausing keeps the preview the cut");

    let mut paused = player(true, false, false, Some(22.0));
    match cut_play::pressed(&mut paused, &cut) {
        cut_play::Pressed::Toggled(cut_play::Toggle::Playing { from }) => {
            assert_eq!(from, 22.0, "resumes from the red line, not from the top");
        }
        other => panic!("paused -> play expected, got {other:?}"),
    }
    assert!(paused.transport.playing && paused.transport.started);
}

#[test]
fn f2_2_s4_a_page_that_never_had_a_line_starts_at_the_first_kept_second() {
    let cut = two_clips_with_a_hole();
    let mut p = player(true, false, false, None);
    match cut_play::pressed(&mut p, &cut) {
        cut_play::Pressed::Toggled(cut_play::Toggle::Playing { from }) => assert_eq!(from, 0.0),
        other => panic!("expected a start from the first clip, got {other:?}"),
    }
}

// --- S5: skipping, jumping, ending -------------------------------------------------------------

#[test]
fn f2_2_s5_kept_seconds_play_and_a_removed_stretch_becomes_a_jump() {
    let cut = two_clips_with_a_hole();
    assert_eq!(cut_play::advance(&cut.segs, 4.0, 1.0), cut_play::At::Kept { t: 5.0 });
    // Crossing 10 s leaves the kept material: the line jumps rather than walking through the gap.
    assert_eq!(cut_play::advance(&cut.segs, 9.5, 1.0), cut_play::At::JumpTo { t: 20.0 });
    assert_eq!(cut_play::advance(&cut.segs, 25.0, 0.1), cut_play::At::Kept { t: 25.1 });
}

#[test]
fn f2_2_s5_past_the_last_clip_the_line_ends_so_the_caller_pauses() {
    let cut = two_clips_with_a_hole();
    assert_eq!(cut_play::advance(&cut.segs, 29.5, 1.0), cut_play::At::Ended);
    assert_eq!(cut_play::advance(&cut.segs, 40.0, 1.0), cut_play::At::Ended);
}

#[test]
fn f2_2_s5_back_to_back_clips_are_one_run_not_a_fake_jump() {
    let cut = Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 10.0, e: 20.0, ..Default::default() },
        ],
        ..Default::default()
    };
    assert_eq!(cut_play::kept_runs(&cut.segs), vec![(0.0, 20.0)], "touching clips merge");
    assert_eq!(cut_play::advance(&cut.segs, 9.0, 2.0), cut_play::At::Kept { t: 11.0 });
}

// --- S5: P.eng.preloadLeadSeconds --------------------------------------------------------------

#[test]
fn f2_2_s5_preloads_the_next_jump_three_seconds_ahead() {
    // P.eng.preloadLeadSeconds = 3: the next clip is opened this far before it is needed, so the join
    // is a swap rather than a reload.
    assert_eq!(naivepost::preview::PRELOAD_LEAD_SECONDS, 3.0);
    let cut = two_clips_with_a_hole();

    // Exactly at the lead: inclusive, because opening one clip early costs an idle pipeline while
    // opening it late stalls every join visibly.
    assert_eq!(cut_play::preload_ahead(&cut.segs, 17.0), Some(20.0));
    // Too far off: nothing opens, the spare stays free for the stretch we are actually in.
    assert_eq!(cut_play::preload_ahead(&cut.segs, 10.0), None);
    // Inside the second clip: no further jump exists, so nothing to arm.
    assert_eq!(cut_play::preload_ahead(&cut.segs, 25.0), None);

    // One lead, three callers: the same helper answers the review's run-up and the walk-on at a
    // recording's end, so retuning it moves all three together.
    let runs = vec![(0.0, 17.0), (20.0, 40.0)];
    assert_eq!(
        naivepost::preview::preload_target(&runs, 18.0),
        cut_play::preload_ahead(&cut.segs, 18.0).or(Some(20.0)),
        "both sides of the shared spare pipeline agree on the lead"
    );
    assert_eq!(naivepost::preview::walk_on(&runs, 18.0), Some(20.0));
}

// --- S5: the clock reads the cut's own time ---------------------------------------------------

#[test]
fn f2_2_s5_the_clock_reads_the_cuts_own_time_not_the_sessions() {
    let cut = two_clips_with_a_hole();
    assert_eq!(cut_play::cut_time(&cut.segs, 0.0), Some(0.0));
    assert_eq!(cut_play::cut_time(&cut.segs, 5.0), Some(5.0), "first clip: session == cut");
    // Second clip starts at session 20 but at cut 10 -- the dropped 10 s is simply not counted.
    assert_eq!(cut_play::cut_time(&cut.segs, 20.0), Some(10.0));
    assert_eq!(cut_play::cut_time(&cut.segs, 27.5), Some(17.5));
    assert_eq!(cut_play::cut_time(&cut.segs, 15.0), None, "in a removed stretch there is no cut time");
    assert_eq!(cut_play::cut_time(&cut.segs, 30.0), None, "past the video");

    // Never past the finished video's length: both count only segs with e > s.
    let last = cut_play::cut_time(&cut.segs, 29.9).expect("inside the last clip");
    assert!(last < shell::cut_seconds(&cut), "the clock stays under the video's own length");
    assert_eq!(shell::cut_seconds(&cut), 20.0);
    // mm_ss zero-pads the minutes, so 10 s of cut reads "00:10.0" -- never "1:00".
    assert_eq!(naivepost::preview::clock(cut_play::cut_time(&cut.segs, 20.0)), "00:10.0");
    assert!(cut_play::cut_clock_note().contains("cut's own time"), "the tooltip explains the number");
}

// --- S5: flat rate, stills, volume --------------------------------------------------------------

#[test]
fn f2_2_s5_speed_plays_flat_stops_show_their_still_volume_applies() {
    let plain = Seg { s: 0.0, e: 10.0, ..Default::default() };
    assert_eq!(cut_play::played_rate(&plain), cut_play::FLAT_RATE, "no rate written -> flat");

    let sped = Seg { s: 0.0, e: 10.0, rate: 2.0, ..Default::default() };
    assert_eq!(cut_play::played_rate(&sped), 2.0, "its own rate, held steady -- no staircase here");

    // A stop is an effect over the timeline, not a property of the segment under it.
    let stopped = Cut {
        segs: vec![plain.clone()],
        fx: vec![naivepost::cut::Fx {
            kind: "speed".into(),
            t: 4.0,
            dur: naivepost::cut_speed::STOP_SECONDS,
            rate: 0.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(cut_play::shows_a_still(&stopped, 4.5), "inside the stop's stretch -> a still");
    assert!(!cut_play::shows_a_still(&stopped, 3.9), "before it, the live picture");
    assert!(
        !cut_play::shows_a_still(&Cut { segs: vec![plain.clone()], ..Default::default() }, 5.0),
        "a clip with no speed effect never shows a still"
    );

    assert!(cut_play::volume_applies(&sped) && cut_play::volume_applies(&plain), "volume effects apply");
}

// --- S6: dropped stretches dimmed ------------------------------------------------------------

#[test]
fn f2_2_s6_dropped_stretches_come_back_dimmed_and_kept_material_never_does() {
    let cut = two_clips_with_a_hole();
    let filmed = vec![(0.0, 30.0)];
    let dimmed = cut_play::dimmed_spans(&filmed, &cut);
    assert_eq!(dimmed, vec![(10.0, 20.0)], "the removed middle stretch is the dimmed one");

    for &(start, end) in &dimmed {
        assert!(
            !cut.segs.iter().any(|seg| cut_play::cut_time(&[seg.clone()], start).is_some()),
            "kept material must never appear in the dimmed list, got {start}..{end}"
        );
    }

    let full = Cut {
        segs: vec![Seg { s: 0.0, e: 30.0, ..Default::default() }],
        ..Default::default()
    };
    assert!(cut_play::dimmed_spans(&filmed, &full).is_empty(), "nothing removed -> nothing dimmed");
}
