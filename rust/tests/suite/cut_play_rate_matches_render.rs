//! ▶✂ plays a ⏩ Speed at the rate the render gives those seconds (F2.2 S5 "speed effects at their flat
//! rate", §06 §2). The page's cut is never split by speed, so its segs carry no rate; the leg used to read
//! only `Seg::rate` and played every speed at 1× while Produce rendered it sped.

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::{cut_play, cut_play_leg, cut_speed_split};

fn sped_cut() -> Cut {
    Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        }],
        fx: vec![Fx {
            kind: "speed".into(),
            t: 2.0,
            dur: 4.0,
            rate: 2.0,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// The rate the render's planner stamps on the piece holding `t` (`produce_exec::plan` reads the same list).
fn render_rate_at(cut: &Cut, t: f64) -> f64 {
    cut_speed_split::sped_cut(cut)
        .iter()
        .find(|seg| seg.s <= t && t < seg.e)
        .map(|seg| if seg.rate > 0.0 { seg.rate } else { 1.0 })
        .expect("the render plans a piece over every kept second")
}

#[test]
fn f2_2_s5_the_cut_preview_plays_a_speed_at_the_rate_the_render_gives_it() {
    let cut = sped_cut();
    assert!(
        cut.segs.iter().all(|seg| seg.rate == 0.0),
        "the page's cut carries no stamped rate"
    );
    for t in [1.0, 3.0, 5.9, 7.0] {
        assert_eq!(
            cut_play_leg::route(&cut, t, 1.0).rate,
            render_rate_at(&cut, t),
            "at {t} s the preview and the render must play the same rate"
        );
    }
    assert_eq!(cut_play_leg::route(&cut, 3.0, 1.0).rate, 2.0);
    // A removed stretch has no clip, so nothing is re-cued there.
    assert_eq!(
        cut_play_leg::route(&cut, 12.0, 1.0).rate,
        cut_play::FLAT_RATE
    );
}
