//! F2.3 — Review every cut (▶✂✂).
//!
//! Which joins there are to hear, how much of the cut each one is heard with, where a review pressed at
//! the red line starts and what it says while it runs. The rules live in [`naivepost::cut_review`], so
//! none of this needs a player or a window (spec/00-principles.md §5).

use naivepost::cut::{Cut, Seg};
use naivepost::cut_review::{self as cut_review, Start, Window};
use naivepost::preview::Player;

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// Three clips with a gap between each: the joins are at 60 and 160, their windows [50,110] and [150,210].
fn three_clips() -> Cut {
    Cut { segs: vec![seg(0.0, 60.0), seg(100.0, 160.0), seg(200.0, 260.0)], ..Default::default() }
}

#[test]
fn f2_3_s1_a_cut_with_one_clip_has_nothing_to_review() {
    // S1: greyed with fewer than two clips.
    assert_eq!(
        cut_review::refused(&Cut::default()),
        Some("nothing to review \u{2014} a cut needs two clips to have a join between them")
    );
    let one = Cut { segs: vec![seg(0.0, 60.0)], ..Default::default() };
    assert_eq!(
        cut_review::refused(&one),
        Some("nothing to review \u{2014} a cut needs two clips to have a join between them")
    );

    let two = Cut { segs: vec![seg(0.0, 60.0), seg(100.0, 160.0)], ..Default::default() };
    assert_eq!(cut_review::refused(&two), None);

    // One join per pair of clips, and none for one clip or none at all.
    assert_eq!(cut_review::joins(&Cut::default()), 0);
    assert_eq!(cut_review::joins(&one), 0);
    assert_eq!(cut_review::joins(&two), 1);
}

#[test]
fn f2_3_s2_a_join_is_heard_ten_seconds_either_side_clamped_to_the_clips() {
    // P.policy.reviewPadSeconds = 10 s either side of the join, clamped to the clips.
    assert_eq!(cut_review::REVIEW_PAD_SECONDS, 10.0);

    let two = Cut { segs: vec![seg(0.0, 60.0), seg(100.0, 200.0)], ..Default::default() };
    assert_eq!(cut_review::window(&two.segs, 0), Window { from: 50.0, to: 110.0 });

    // A later clip shorter than the pad: heard to its own end and no further.
    let short_after = Cut { segs: vec![seg(0.0, 60.0), seg(100.0, 104.0)], ..Default::default() };
    assert_eq!(cut_review::window(&short_after.segs, 0).to, 104.0);

    // An earlier clip shorter than the pad: its start is where the lead starts, because what came before
    // it was removed rather than shortened.
    let short_before = Cut { segs: vec![seg(55.0, 60.0), seg(100.0, 200.0)], ..Default::default() };
    assert_eq!(cut_review::window(&short_before.segs, 0).from, 55.0);

    let three = three_clips();
    assert_eq!(cut_review::joins(&three), 2);
    assert_eq!(cut_review::window(&three.segs, 0), Window { from: 50.0, to: 110.0 });
    assert_eq!(cut_review::window(&three.segs, 1), Window { from: 150.0, to: 210.0 });
}

#[test]
fn f2_3_s3_the_review_starts_where_the_red_line_stands() {
    // S2: in a window → play on; between windows → the next join's run-up; past the last → the first.
    let cut = three_clips();
    assert_eq!(cut_review::start_from_line(&cut, 70.0), (0, Start::PlayOn));
    assert_eq!(cut_review::start_from_line(&cut, 130.0), (1, Start::SeekTo(150.0)));
    assert_eq!(cut_review::start_from_line(&cut, 30.0), (0, Start::SeekTo(50.0)));
    assert_eq!(cut_review::start_from_line(&cut, 300.0), (0, Start::SeekTo(50.0)), "past the last join: start over");

    // Exactly on a window's edges, half-open like the rest of the timeline: its own end is the next
    // join's business, its own start is already being heard.
    assert_eq!(cut_review::start_from_line(&cut, 50.0), (0, Start::PlayOn));
    assert_eq!(cut_review::start_from_line(&cut, 110.0), (1, Start::SeekTo(150.0)));
}

#[test]
fn f2_3_s4_the_status_names_the_join_out_of_the_cut() {
    // S4: "reviewing cut N of M — 10 s before and after the join at m:ss".
    let cut = three_clips();
    assert_eq!(
        cut_review::reviewing_status(&cut, 0),
        "reviewing cut 1 of 2 \u{2014} 10 s before and after the join at 01:00"
    );
    assert_eq!(
        cut_review::reviewing_status(&cut, 1),
        "reviewing cut 2 of 2 \u{2014} 10 s before and after the join at 02:40"
    );
}

#[test]
fn f2_3_s5_the_review_waits_seeks_and_finishes() {
    // S3, S4 and S5 in one tick function: wait, seek on, finish, or notice a hand on the line.
    let cut = three_clips();

    // Heard inside the window: nothing to do, and the review is now armed for this join.
    let mut review = cut_review::Review { join: 0, armed: true };
    assert_eq!(cut_review::tick(&cut, &mut review, 70.0), cut_review::Move::Stay);

    // Past it, with the next run-up ahead: go there, and the new window has not been seen yet.
    let mut review = cut_review::Review { join: 0, armed: true };
    assert_eq!(cut_review::tick(&cut, &mut review, 120.0), cut_review::Move::Seek);
    assert_eq!(review.join, 1);
    assert!(!review.armed, "join 1's window has not been heard in");

    // The same place with a seek still in the air: wait, do not declare the line moved. This is join 1's
    // own window — the review has just sought to its run-up at 150 and the player is answering the old
    // position for a tick or two — so an unarmed line inside it stays put rather than being read as a hand.
    let mut waiting = cut_review::Review { join: 1, armed: false };
    assert_eq!(cut_review::tick(&cut, &mut waiting, 155.0), cut_review::Move::Stay);
    assert_eq!(waiting.join, 1);

    // Unarmed and short of the window too: still waiting, because the seek has not landed yet.
    let mut waiting = cut_review::Review { join: 1, armed: false };
    assert_eq!(cut_review::tick(&cut, &mut waiting, 120.0), cut_review::Move::Stay);

    // A line found before a window it had already been inside is a hand's doing (S5).
    let mut armed = cut_review::Review { join: 0, armed: true };
    assert_eq!(cut_review::tick(&cut, &mut armed, 30.0), cut_review::Move::Lost);
    assert_eq!(
        cut_review::LINE_MOVED,
        "the line was moved \u{2014} the cut review is over; ▶✂✂ starts it again"
    );

    // The last join heard out: done (S4).
    let mut last = cut_review::Review { join: 1, armed: true };
    assert_eq!(cut_review::tick(&cut, &mut last, 250.0), cut_review::Move::Done);
    assert_eq!(cut_review::reviewed_all(&cut), "reviewed all 2 cuts");

    // Starting from the line arms a play-on and leaves a seek unarmed (S2's two answers).
    assert_eq!(cut_review::start(&cut, 70.0), cut_review::Review { join: 0, armed: true });
    assert_eq!(cut_review::start(&cut, 130.0), cut_review::Review { join: 1, armed: false });
}

#[test]
fn f2_3_s6_a_join_behind_the_line_is_played_into_not_rewound() {
    // S3: "or play on into it if already behind the line" — a stretch of short clips is heard once, in
    // order, and not rewound for every seam inside it.
    let cut = three_clips();

    // The line at 160 is inside join 1's window [150,210]: play on, no seek.
    assert_eq!(cut_review::start_from_line(&cut, 160.0), (1, Start::PlayOn));
    let mut review = cut_review::start(&cut, 160.0);
    assert_eq!(cut_review::tick(&cut, &mut review, 205.0), cut_review::Move::Stay);

    // Clips close together: [0,30],[40,70],[80,120] — windows [20,50] and [60,90]. A line at 65 is
    // already inside the second one, so the review hears it rather than rewinding to its run-up.
    let tight = Cut {
        segs: vec![seg(0.0, 30.0), seg(40.0, 70.0), seg(80.0, 120.0)],
        ..Default::default()
    };
    assert_eq!(cut_review::start_from_line(&tight, 65.0), (1, Start::PlayOn));
    let mut review = cut_review::Review { join: 0, armed: true };
    // Past join 0's window and inside join 1's: stay where the line is, having advanced the join.
    assert_eq!(cut_review::tick(&tight, &mut review, 65.0), cut_review::Move::Stay);
    assert_eq!(review.join, 1);
}

#[test]
fn f2_3_s7_pressing_the_review_button_starts_refuses_and_ends() {
    // S5's first half, then S1 and S2.
    let two = Cut { segs: vec![seg(0.0, 60.0), seg(100.0, 160.0)], ..Default::default() };

    let mut player = Player { playhead: Some(30.0), ..Default::default() };
    let started = cut_review::pressed(&mut player, &two);
    match &started {
        cut_review::Pressed::Started { join, seek_to, status } => {
            assert_eq!(*join, 0);
            assert_eq!(*seek_to, Some(50.0), "the line was short of the run-up");
            assert_eq!(*status, cut_review::reviewing_status(&two, 0));
        }
        other => panic!("a two-clip cut starts a review, not {other:?}"),
    }
    assert!(player.reviewing && player.cut_only);
    assert!(player.transport.playing && player.transport.started);
    assert_eq!(player.playhead, Some(50.0));

    // One clip: refused, and nothing about the preview changed.
    let one = Cut { segs: vec![seg(0.0, 60.0)], ..Default::default() };
    let mut idle = Player { playhead: Some(10.0), ..Default::default() };
    assert_eq!(
        cut_review::pressed(&mut idle, &one),
        cut_review::Pressed::Refused(
            "nothing to review \u{2014} a cut needs two clips to have a join between them"
        )
    );
    assert!(!idle.reviewing && !idle.cut_only && !idle.transport.playing);

    // Pressed while it is the one running: pause, and the review is over — resuming from here is ▶✂'s.
    let mut running = Player {
        reviewing: true,
        cut_only: true,
        transport: naivepost::run::Transport { playing: true, started: true },
        playhead: Some(70.0),
    };
    assert_eq!(
        cut_review::pressed(&mut running, &two),
        cut_review::Pressed::PausedAndEnded { status: "pause the cut review" }
    );
    assert!(!running.reviewing);
    assert!(!running.transport.playing);
    assert!(running.cut_only, "the preview stays the cut's; ▶✂ picks it up from here");
}

#[test]
fn f2_3_s8_the_other_two_buttons_switch_over_without_stopping() {
    // The three-button rule: each wears ⏸ only while its own thing runs; pressing another switches the
    // preview without stopping; exactly one is lit.
    let mut player = Player {
        reviewing: true,
        cut_only: true,
        transport: naivepost::run::Transport { playing: true, started: true },
        playhead: Some(70.0),
    };

    // ▶✂ during a review: the review ends, the cut plays on as the plain cut preview.
    assert!(cut_review::switch_over(&mut player));
    assert!(!player.reviewing);
    assert!(player.transport.playing, "switching over does not stop anything");
    assert!(player.cut_only, "and it stays on the cut");

    // Nothing was being reviewed the second time.
    assert!(!cut_review::switch_over(&mut player));

    // ▶ during a review switches to the recording and carries on playing (F2.1's S1), which is that
    // function's own doing rather than this one's.
    let mut player = Player {
        reviewing: true,
        cut_only: true,
        transport: naivepost::run::Transport { playing: true, started: true },
        playhead: Some(70.0),
    };
    assert_eq!(
        naivepost::preview::press_recording(&mut player, &[(0.0, 260.0)]),
        naivepost::preview::Press::SwitchedToRecording { kept_playing: true }
    );
    assert!(!player.reviewing && !player.cut_only && player.transport.playing);
}
