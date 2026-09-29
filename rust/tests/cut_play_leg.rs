//! F2.2 — the ▶✂ leg: the tick loop's parts, the spare pipeline's arm, the frame answer and the
//! dimmed paint. Logic only; the click and the key go through the real widgets in
//! tests/cut_play_the_cut_widgets.rs.

use naivepost::cut::{Cut, Seg};
use naivepost::{cut_play, cut_play_leg, preview};

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

#[test]
fn f2_2_s5_the_tick_skips_a_removed_stretch_and_returns_the_cuts_clock() {
    let cut = two_clips_with_a_hole();
    // A tick that stays inside a clip: head and clock move together (first clip, so they are equal).
    let inside = cut_play_leg::tick(&cut, 4.0, 100);
    assert_eq!(inside.head, 4.1, "one tick of 100 ms inside the first clip");
    assert!(!inside.ended, "still material ahead");

    // Crossing into the removed stretch jumps to the next clip's first playable second...
    let jumped = cut_play_leg::tick(&cut, 9.5, 1000);
    assert_eq!(jumped.head, 20.0, "the line jumps over the gap onto kept material");
    assert!(!jumped.ended, "there is a clip past the gap, so playback goes on");
    // ...and the clock reads the CUT's own time: 10 s of finished video, not session second 20.
    assert_eq!(
        jumped.clock,
        cut_play::cut_time(&cut.segs, 20.0).unwrap(),
        "the clock comes from the cut's own arithmetic"
    );
    assert_eq!(jumped.clock, 10.0, "two 10 s clips: the second one starts at cut time 10");
    assert_ne!(jumped.clock, 20.0, "it is NOT the session second -- the dropped 10 s is not counted");
}

#[test]
fn f2_2_s5_the_tick_ends_past_the_last_clip_so_the_page_pauses() {
    let cut = two_clips_with_a_hole();
    let over = cut_play_leg::tick(&cut, 29.5, 1000);
    assert!(over.ended, "nothing kept ahead of the last clip: S5 says pause");
    assert_eq!(over.head, 30.0, "the line parks on the last kept end, not wherever it overshot");
    // The clock is asked at the session second the tick came FROM (the head that was still on material),
    // because the overshoot itself sits past the video: 19.5 s of the finished film, one tenth short.
    assert_eq!(over.clock, 19.5, "the clock shows how far into the finished video the run got");

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
    assert_eq!(cut_play_leg::arm_spare(&cut, 17.0), Some(20.0), "armed at exactly the lead");
    assert_eq!(cut_play_leg::spare_armed(), Some(20.0), "the slot holds what was just armed");

    // Too far off: nothing opens, so the spare stays free for the stretch actually being played.
    assert_eq!(cut_play_leg::arm_spare(&cut, 10.0), None, "10 s out is outside the lead");
    assert_eq!(cut_play_leg::spare_armed(), None, "a non-arm clears the slot rather than leaving a stale one");

    // Past the last jump there is nothing to arm either.
    assert_eq!(cut_play_leg::arm_spare(&cut, 25.0), None);
    cut_play_leg::clear_spare();
    assert_eq!(cut_play_leg::spare_armed(), None, "clearing releases the pipeline");
}

#[test]
fn f2_2_s5_the_frame_answers_still_flat_rate_and_volume() {
    let plain = Seg { s: 0.0, e: 10.0, ..Default::default() };
    let flat = cut_play_leg::frame(&Cut::default(), &plain, 5.0);
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
    let in_stop = cut_play_leg::frame(&stopped, &plain, 4.5);
    assert!(in_stop.still, "inside the stop's stretch: the still shows");
    assert!(in_stop.volume, "and volume still applies over a still");
    let before_stop = cut_play_leg::frame(&stopped, &plain, 3.9);
    assert!(!before_stop.still, "before the stop: the live picture");

    let sped = Seg { s: 0.0, e: 10.0, rate: 2.0, ..Default::default() };
    let fast = cut_play_leg::frame(&Cut::default(), &sped, 5.0);
    assert_eq!(fast.rate, 2.0, "its own rate, held steady -- the flat rate, no staircase");
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
        segs: vec![Seg { s: 0.0, e: 10.0, ..Default::default() }],
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
        segs: vec![Seg { s: 5.0, e: 5.0, ..Default::default() }],
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

#[test]
fn f2_2_s6_the_leg_returns_only_removed_stretches_to_dim() {
    let cut = two_clips_with_a_hole();
    let filmed = vec![(0.0, 30.0)];
    assert_eq!(
        cut_play_leg::dimmed(&cut, &filmed),
        vec![(10.0, 20.0)],
        "the removed middle stretch is the only thing to dim"
    );

    let full = Cut {
        segs: vec![Seg { s: 0.0, e: 30.0, ..Default::default() }],
        ..Default::default()
    };
    assert!(cut_play_leg::dimmed(&full, &filmed).is_empty(), "nothing removed, nothing dimmed");

    // And the paint really puts pixels down: inside the span differs from outside it.
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 120, 20)
        .expect("an image surface to paint on");
    {
        let cr = cairo::Context::new(&surface).expect("a cairo context");
        // Ground the whole surface white first, so "dimmed" means darker-than-white rather than
        // merely non-zero alpha over an uninitialised buffer.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().expect("paint the ground");
        // pps 6.0 puts the 10..20 s span at x 60..120; the outside sample sits at x 0..60.
        cut_play_leg::paint_dimmed(&cr, &[(10.0, 20.0)], 0.0, 20.0, 6.0);
    }
    let data = surface
        .data()
        .expect("read the painted pixels back")
        .to_vec();
    let stride = surface.stride() as usize;
    let pixel_at = |x: usize| -> (u8, u8, u8) {
        let base = 10 * stride + x * 4;
        // ARgb32 on little-endian is B,G,R,A.
        (data[base + 2], data[base + 1], data[base])
    };
    let outside = pixel_at(30);
    let inside = pixel_at(90);
    assert_eq!(outside, (255, 255, 255), "outside the span stays the white ground");
    assert!(
        inside.0 < 255 && inside.1 < 255 && inside.2 < 255,
        "inside the span is dimmed toward black, got {inside:?}"
    );
    let expected = (cut_play_leg::DIM_GREY * 255.0) as u8;
    assert!(
        inside.0.abs_diff(expected) <= 2
            && inside.1.abs_diff(expected) <= 2
            && inside.2.abs_diff(expected) <= 2,
        "the fill is the solid dim grey: expected ~{expected}, got {inside:?}"
    );
}

/// S6, the visibility claim in numbers rather than by eye: a removed stretch must read darker than
/// BOTH things it sits beside on the strip — the empty ground (0.95) and the kept footage fill
/// (`rgba(0.2, 0.8, 0.3, 0.3)` over that ground). A gap only lighter than one of them is invisible
/// against the other, which is exactly what the 0.18-alpha wash did.
#[test]
fn f2_2_s6_the_dim_reads_darker_than_ground_and_kept() {
    // Same strip recipe `paint_track_strip` uses, painted in the same order, on one surface.
    const GROUND: f64 = 0.95;
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, 300, 20).expect("an image surface");
    {
        let cr = cairo::Context::new(&surface).expect("a cairo context");
        cr.set_source_rgb(GROUND, GROUND, GROUND);
        cr.paint().expect("ground");
        // Kept footage first, then the dim over its own spans: the real strip paints the dim BEFORE
        // the boxes, so sample each where only it was laid down.
        cr.set_source_rgba(0.2, 0.8, 0.3, 0.3);
        cr.rectangle(0.0, 0.0, 100.0, 20.0);
        cr.fill().expect("kept green");
        // pps 2.0 puts the 25..50 s span at x 50..100, inside the kept block painted just above:
        // this is the strip's real situation, a removed stretch sitting in filmed material.
        cut_play_leg::paint_dimmed(&cr, &[(25.0, 50.0)], 0.0, 20.0, 2.0);
        // The plain ground region is x 200..300 (nothing painted over it).
    }
    let data = surface.data().expect("read the pixels back").to_vec();
    let stride = surface.stride() as usize;
    let pixel_at = |x: usize| -> (u8, u8, u8) {
        let base = 10 * stride + x * 4;
        // ARgb32 little-endian is B,G,R,A.
        (data[base + 2], data[base + 1], data[base])
    };
    let lum = |p: (u8, u8, u8)| -> f64 {
        (0.2126 * p.0 as f64 + 0.7152 * p.1 as f64 + 0.0722 * p.2 as f64) / 255.0
    };
    let ground = lum(pixel_at(250));
    let kept = lum(pixel_at(20));
    let dim = lum(pixel_at(75));
    assert!(
        ground > 0.9,
        "the ground reads as the strip's light grey, got {ground:.3}"
    );
    assert!(
        dim < ground - 0.15,
        "the removed stretch reads clearly darker than the empty ground: dim {dim:.3} vs ground {ground:.3}"
    );
    assert!(
        dim < kept - 0.15,
        "and clearly darker than the kept footage fill: dim {dim:.3} vs kept {kept:.3}"
    );
}
