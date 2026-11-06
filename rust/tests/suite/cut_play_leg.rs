//! F2.2 — the ▶✂ leg: the tick loop's parts, the spare pipeline's arm, the frame answer and the
//! dimmed paint. Logic only; the click and the key go through the real widgets in
//! tests/suite/cut_play_the_cut_widgets.rs.

use naivepost::cut::{Cut, Seg};
use naivepost::{cut_play, cut_play_leg, preview};

/// Two kept clips with a removed stretch between them: 0-10 kept, 10-20 dropped, 20-30 kept.
fn two_clips_with_a_hole() -> Cut {
    Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 10.0,
                ..Default::default()
            },
            Seg {
                s: 20.0,
                e: 30.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn f2_2_s5_the_tick_skips_a_removed_stretch_and_returns_the_cuts_clock() {
    let cut = two_clips_with_a_hole();
    // A tick that stays inside a clip: head and clock move together (first clip, so they are equal).
    let inside = cut_play_leg::tick(&cut, 4.0, 100);
    assert_eq!(inside.head, 4.1, "one tick of 100 ms inside the first clip");
    assert!(!inside.ended, "still material ahead");

    // Crossing into the removed stretch jumps to the next clip's first playable second...
    // A seam inside the tick is not crossed early: the line holds where the player is and says when the clip
    // ends, so the loop crosses at that moment, where the render cuts (first rewrite: the tick jumped as soon as
    // the player was within one tick of the end, so up to 100 ms of every kept clip never played).
    let near = cut_play_leg::tick(&cut, 9.5, 1000);
    assert_eq!(near.head, 9.5, "the line stays on the player until the clip ends");
    assert_eq!(near.seam_in, Some(0.5), "the clip ends half a second on");
    assert!(!near.jumped);
    // ...at the end of the clip, crossing into the removed stretch jumps to the next clip's first playable second...
    let jumped = cut_play_leg::tick(&cut, 10.0, 0);
    assert_eq!(
        jumped.head, 20.0,
        "the line jumps over the gap onto kept material"
    );
    assert!(
        !jumped.ended,
        "there is a clip past the gap, so playback goes on"
    );
    // ...and the clock reads the CUT's own time: 10 s of finished video, not session second 20.
    assert_eq!(
        jumped.clock,
        cut_play::cut_time(&cut.segs, 20.0).unwrap(),
        "the clock comes from the cut's own arithmetic"
    );
    assert_eq!(
        jumped.clock, 10.0,
        "two 10 s clips: the second one starts at cut time 10"
    );
    assert_ne!(
        jumped.clock, 20.0,
        "it is NOT the session second -- the dropped 10 s is not counted"
    );
}

#[test]
fn f2_2_s5_the_tick_ends_past_the_last_clip_so_the_page_pauses() {
    let cut = two_clips_with_a_hole();
    let near = cut_play_leg::tick(&cut, 29.5, 1000);
    assert_eq!(near.seam_in, Some(0.5), "the last clip ends inside the tick: crossed when it does");
    assert!(!near.ended);
    let over = cut_play_leg::tick(&cut, 30.0, 0);
    assert!(
        over.ended,
        "nothing kept ahead of the last clip: S5 says pause"
    );
    assert_eq!(
        over.head, 30.0,
        "the line parks on the last kept end, not wherever it overshot"
    );
    // The run got to the end of the finished film: the clock reads all of it, 20 s.
    assert_eq!(
        over.clock, 20.0,
        "the clock shows how far into the finished video the run got"
    );

    // Well past the end still ends rather than wrapping or going blank.
    assert!(cut_play_leg::tick(&cut, 60.0, 100).ended);
}

#[test]
fn f2_2_s5_the_spare_pipeline_arms_three_seconds_before_the_jump() {
    // P.eng.preloadLeadSeconds = 3 (tolerance 0.01 s): the next clip opens this far early so the
    // join is a swap and not a reload.
    assert!(
        (preview::PRELOAD_LEAD_SECONDS - 3.0).abs() <= 0.01,
        "P.eng.preloadLeadSeconds is 3, got {}",
        preview::PRELOAD_LEAD_SECONDS
    );
    let cut = two_clips_with_a_hole();

    // Inside the lead window: the jump at 20 s gets armed while the line is at 17 s.
    assert_eq!(
        cut_play_leg::arm_spare(&cut, 17.0),
        Some(20.0),
        "armed at exactly the lead"
    );
    assert_eq!(
        cut_play_leg::peek_spare(),
        Some(20.0),
        "the slot holds what was just armed"
    );

    // In the removed stretch the jump is now: the next clip is armed at once.
    assert_eq!(cut_play_leg::arm_spare(&cut, 10.0), Some(20.0));
    // Too far off — five seconds before the run's end, where the jump is made: nothing opens, so the spare
    // stays free for the stretch actually being played.
    assert_eq!(
        cut_play_leg::arm_spare(&cut, 5.0),
        None,
        "5 s before the jump is outside the lead"
    );
    assert_eq!(
        cut_play_leg::peek_spare(),
        None,
        "a non-arm clears the slot rather than leaving a stale one"
    );

    // Past the last jump there is nothing to arm either.
    assert_eq!(cut_play_leg::arm_spare(&cut, 25.0), None);
    assert_eq!(
        cut_play_leg::peek_spare(),
        None,
        "clearing releases the pipeline"
    );
}

#[test]
fn f2_2_s5_the_frame_answers_still_flat_rate_and_volume() {
    let plain = Seg {
        s: 0.0,
        e: 10.0,
        ..Default::default()
    };
    let flat = cut_play_leg::frame(&Cut::default(), Some(&plain), 5.0);
    assert!(!flat.still, "no stop laid over the picture: live frames");
    assert_eq!(flat.rate, cut_play::FLAT_RATE, "no rate written plays flat");
    assert!(flat.volume, "volume effects apply");

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
    let in_stop = cut_play_leg::frame(&stopped, Some(&plain), 4.5);
    assert!(in_stop.still, "inside the stop's stretch: the still shows");
    assert!(in_stop.volume, "and volume still applies over a still");
    let before_stop = cut_play_leg::frame(&stopped, Some(&plain), 3.9);
    assert!(!before_stop.still, "before the stop: the live picture");

    let sped = Seg {
        s: 0.0,
        e: 10.0,
        rate: 2.0,
        ..Default::default()
    };
    let fast = cut_play_leg::frame(&Cut::default(), Some(&sped), 5.0);
    assert_eq!(
        fast.rate, 2.0,
        "its own rate, held steady -- the flat rate, no staircase"
    );
    assert!(fast.volume);
}

#[test]
fn f2_2_s2_space_a_picture_click_and_the_run_bar_refuse_an_empty_cut() {
    // Those ways in cannot switch the preview, so on an empty cut they get S2's sentence verbatim.
    assert_eq!(
        cut_play_leg::other_way_in(&Cut::default()),
        Some(cut_play::EMPTY_CUT_REFUSAL),
        "an empty cut refuses every way in that cannot switch"
    );
    assert_eq!(
        cut_play::EMPTY_CUT_REFUSAL,
        "the cut is empty \u{2014} add a clip to play it, or press \u{25b6} to play the recording instead",
        "spec/05-cut.md F2.2 S2's wording, unchanged"
    );

    let one_clip = Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        cut_play_leg::other_way_in(&one_clip),
        None,
        "with a clip there the way in may toggle whatever the preview already is"
    );

    // A card-only cut: `cut_screen::can_play_cut` asks only whether the cut has segs at all, so a cut
    // carrying an instant card is NOT refused here -- greying and refusing stay one question (§A), and
    // splitting them would let the button look live while the press said otherwise.
    let cards_only = Cut {
        segs: vec![Seg {
            s: 5.0,
            e: 5.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(
        cut_play_leg::other_way_in(&cards_only).is_none(),
        "one rule answers grey-and-refuse, so a cut with a card in it is not refused by this path"
    );
    assert!(
        naivepost::cut_screen::can_play_cut(&cards_only),
        "and the greyed rule agrees: a seg exists"
    );
}

/// S5: a seam is crossed at the clip's end, not a tick before it; back-to-back clips are one run and no seam.
#[test]
fn f2_2_s5_a_seam_is_crossed_where_the_render_cuts() {
    let cut = two_clips_with_a_hole();
    assert_eq!(cut_play_leg::seam_within(&cut, 9.93, 0.1).map(|l| (l * 1000.0).round()), Some(70.0));
    assert_eq!(cut_play_leg::seam_within(&cut, 9.8, 0.1), None, "200 ms on: the next tick sees it");
    assert_eq!(cut_play_leg::seam_within(&cut, 15.0, 0.1), None, "off kept material: no seam to time");
    let joined = Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, ..Default::default() },
            Seg { s: 10.0, e: 20.0, ..Default::default() },
        ],
        ..Default::default()
    };
    assert_eq!(cut_play_leg::seam_within(&joined, 9.95, 0.1), None, "clips that touch play straight through");
    let inside = cut_play_leg::tick(&joined, 9.95, 100);
    assert!(inside.head > 10.0 && !inside.jumped, "{inside:?}");
}

/// S5: the timed crossing is dropped when its moment comes and things moved on: a hand put the line elsewhere,
/// the player already jumped, or an edit took the seam away.
#[test]
fn f2_2_s5_a_timed_crossing_is_made_only_while_it_is_still_due() {
    let cut = two_clips_with_a_hole();
    assert!(cut_play_leg::seam_still_due(&cut, 10.0, Some(9.99)), "the player is at the seam");
    assert!(cut_play_leg::seam_still_due(&cut, 10.0, None), "a player that cannot say: trust the timer");
    assert!(!cut_play_leg::seam_still_due(&cut, 10.0, Some(4.0)), "a hand moved the line back");
    assert!(!cut_play_leg::seam_still_due(&cut, 10.0, Some(20.02)), "the player already jumped");
    let edited = Cut {
        segs: vec![Seg { s: 0.0, e: 30.0, ..Default::default() }],
        ..Default::default()
    };
    assert!(!cut_play_leg::seam_still_due(&edited, 10.0, Some(9.99)), "the hole was filled meanwhile");
}
