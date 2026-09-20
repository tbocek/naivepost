//! §05-cut#1-screen — the Cut page's view rules.
//!
//! Which button is greyed, what it says when hovered, where one wheel motion goes and how far the
//! two ladders may run: all of it is answered in [`naivepost::cut_screen`], so all of it is checked
//! here without a window. The widgets themselves are the page's business; they hold no rule
//! (spec/00-principles.md §5).

use naivepost::cut::{Cut, Seg};
use naivepost::cut_screen::{self as cut_screen, Preview, Wheel, WheelOver};

/// A kept stretch of footage — what §A counts as a clip.
fn footage(start: f64, end: f64) -> Seg {
    Seg { s: start, e: end, ..Default::default() }
}

#[test]
fn sec_05_cut_1_screen_s1_the_toolbar_groups_are_six_and_in_spec_order() {
    // §1: "Toolbar groups (left to right): transport …; preview volume; verbs …; effects dropdown
    // …; history …; zoom". The window builds one box per entry, so this list is that order.
    assert_eq!(cut_screen::TOOLBAR_GROUPS, ["transport", "volume", "verbs", "effects", "history", "zoom"]);
}

#[test]
fn sec_05_cut_1_screen_s2_the_two_cut_buttons_are_greyed_until_there_is_a_cut() {
    // ▶✂ is "sensitive when segs > 0"; ▶✂✂ "sensitive with ≥2 clips"; ▶ is "(never greyed)".
    let empty = Cut::default();
    assert!(!cut_screen::can_play_cut(&empty));
    assert!(!cut_screen::can_review(&empty));

    let one = Cut { segs: vec![footage(0.0, 10.0)], ..Default::default() };
    assert!(cut_screen::can_play_cut(&one), "one scene is still a cut to play");
    assert!(!cut_screen::can_review(&one), "one scene has no join to review");

    let two = Cut { segs: vec![footage(0.0, 10.0), footage(20.0, 30.0)], ..Default::default() };
    assert!(cut_screen::can_play_cut(&two));
    assert!(cut_screen::can_review(&two));

    // A card is not a clip: reviewing it would play one still and call the tour done.
    let with_card = Cut {
        segs: vec![footage(0.0, 10.0), Seg { ins: "card.svg".into(), dur: 3.0, ..Default::default() }],
        ..Default::default()
    };
    assert!(cut_screen::can_play_cut(&with_card));
    assert!(!cut_screen::can_review(&with_card), "an insert is not a second clip");

    assert!(cut_screen::play_recording_sensitive());
}

#[test]
fn sec_05_cut_1_screen_s3_the_lit_state_follows_what_the_preview_is_doing() {
    // ▶✂ is "lit while cutOnly and no review"; ▶✂✂ is lit while the review runs.
    let cut_running = Preview { cut_only: true, reviewing: false };
    assert!(cut_screen::play_cut_lit(&cut_running));
    assert!(!cut_screen::review_lit(&cut_running));

    let reviewing = Preview { cut_only: false, reviewing: true };
    assert!(cut_screen::review_lit(&reviewing));
    assert!(!cut_screen::play_cut_lit(&reviewing), "nothing is running but the review");

    // The review skips as ▶✂ does, so it holds the light while it runs.
    let both = Preview { cut_only: true, reviewing: true };
    assert!(cut_screen::review_lit(&both));
    assert!(!cut_screen::play_cut_lit(&both), "two lit play buttons read as two previews");

    assert!(!cut_screen::play_cut_lit(&Preview::default()));
    assert!(!cut_screen::review_lit(&Preview::default()));
}

#[test]
fn sec_05_cut_1_screen_s4_the_wheel_steps_frames_over_the_transport_and_the_picture() {
    // "Wheel over the transport bar **and over the preview picture** steps frames (Shift = 5)".
    assert_eq!(cut_screen::frame_step(false), 1);
    assert_eq!(cut_screen::frame_step(true), 5);

    for over in [WheelOver::Transport, WheelOver::Preview] {
        assert_eq!(cut_screen::wheel(over, false, 1.0, 0.0, 1000.0), Wheel::StepFrames { frames: 1 });
        assert_eq!(cut_screen::wheel(over, false, -1.0, 0.0, 1000.0), Wheel::StepFrames { frames: -1 });
        assert_eq!(cut_screen::wheel(over, true, 1.0, 0.0, 1000.0), Wheel::StepFrames { frames: 5 });
        assert_eq!(cut_screen::wheel(over, true, -2.0, 0.0, 1000.0), Wheel::StepFrames { frames: -5 });
    }

    // Nothing moved is nothing stepped — a wheel that reports an event it did not make.
    assert_eq!(cut_screen::wheel(WheelOver::Transport, false, 0.0, 0.0, 1000.0), Wheel::StepFrames { frames: 0 });
}

#[test]
fn sec_05_cut_1_screen_s5_the_wheel_zooms_around_the_cursor_and_pans_an_eighth() {
    // "over the tracks it zooms around the cursor; Shift+wheel or a trackpad sideways swipe pans an
    // eighth of the view".
    let view = 1000.0;
    assert_eq!(
        cut_screen::wheel(WheelOver::Tracks, false, 1.0, 0.0, view),
        Wheel::Zoom { factor: 1.0 / cut_screen::ZOOM_STEP, at_x: 0.0 }
    );
    assert_eq!(
        cut_screen::wheel(WheelOver::Tracks, false, -1.0, 0.0, view),
        Wheel::Zoom { factor: cut_screen::ZOOM_STEP, at_x: 0.0 }
    );

    let expected = cut_screen::pan_px(view);
    assert_eq!(expected, 125.0, "an eighth of the view");
    for swipe in [
        cut_screen::wheel(WheelOver::Tracks, true, 1.0, 0.0, view),
        cut_screen::wheel(WheelOver::Tracks, false, 0.0, 3.0, view),
    ] {
        match swipe {
            Wheel::Pan { px } => assert_eq!(px.abs(), expected),
            other => panic!("a sideways gesture pans, not {other:?}"),
        }
    }

    // Zooming at the cursor keeps the second under it: what you were looking at stays there.
    let (pps, origin) = cut_screen::zoom_around(4.0, cut_screen::ZOOM_STEP, 400.0, 100.0, 1.0);
    let before = (400.0 - 100.0) / 4.0;
    let after = (400.0 - origin) / pps;
    assert!((before - after).abs() < 1e-9, "{before} moved to {after}");
    assert!(pps > 4.0, "zooming in is more px per second");

    // The two ladders stop where §A says: the floor is what fits, the ceiling is 240 px/s.
    let floor = cut_screen::fit_pps(600.0, 1200.0);
    let mut pps = cut_screen::ZOOM_AT_OPEN;
    for _ in 0..40 {
        pps = cut_screen::zoom_down(pps, floor);
    }
    assert_eq!(pps, floor, "zooming out stops where the whole session is on screen");
    let mut pps = cut_screen::ZOOM_AT_OPEN;
    for _ in 0..80 {
        pps = cut_screen::zoom_up(pps, floor);
    }
    assert_eq!(pps, cut_screen::ZOOM_MAX, "240 px a second is the deepest zoom");

    // An empty session still has a timeline: the floor is a number that draws.
    assert_eq!(cut_screen::fit_pps(0.0, 1200.0), cut_screen::ZOOM_AT_OPEN);
}
