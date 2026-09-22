//! F3.12 Clamp to the cut as applied — `spec/06-effects.md` F3.12, checked against [`naivepost::cut_clamp`].
//!
//! One pass over the effect list after the timeline has stopped moving. Every assertion is on a returned `Fx` or on the
//! count S5 logs: no widget and no display, because `rust/src/ui/window.rs` still renders only Prepare and there is no
//! button to press that would run this.

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_clamp as clamp;

/// A footage scene from `s` to `e`.
fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

/// A spliced insert of `dur` seconds — its own picture, and no footage for an effect to sit on.
fn card(dur: f64) -> Seg {
    Seg {
        s: 0.0,
        e: 0.0,
        ins: "project:assets/tier.svg".into(),
        dur,
        ..Default::default()
    }
}

fn fx(kind: &str, t: f64, dur: f64) -> Fx {
    Fx {
        kind: kind.into(),
        t,
        dur,
        ..Default::default()
    }
}

/// The same effect with both fades and, for a speed, a rate.
fn fx_faded(kind: &str, t: f64, dur: f64, trans: f64, tout: f64) -> Fx {
    Fx {
        trans,
        tout,
        ..fx(kind, t, dur)
    }
}

fn cut_of(segs: &[Seg], effects: &[Fx]) -> Cut {
    Cut {
        segs: segs.to_vec(),
        fx: effects.to_vec(),
        ..Default::default()
    }
}

/// The one effect the pass kept, panicking when it did not keep exactly one — every step below asks about a single
/// effect, and "it kept none" is a failure worth naming rather than an index panic.
fn one(kept: &[Fx]) -> &Fx {
    assert_eq!(kept.len(), 1, "expected exactly one effect to survive");
    &kept[0]
}

/// F3.12 S1 (`after snapping, dead air, marks, coalescing`) — the pass reads the scenes it is handed and nothing else,
/// so it can run at any point after the cut stopped moving. A band that fits inside a scene comes back unchanged, with
/// its fades where they were: an effect the cut has no complaint about must not be rewritten at all, or every save
/// after a suggestion would show as an edit the user never made.
#[test]
fn f3_12_s1_an_effect_inside_kept_footage_is_untouched() {
    assert_eq!(clamp::MIN_SURVIVING_SECONDS, 1.0); // P.eng.effectMinSurvivingSeconds

    let inside = fx_faded("zoom", 12.0, 4.0, 1.0, 0.5);
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 30.0)], &[inside.clone()]));
    assert_eq!(out.dropped, 0);
    assert_eq!(out.kept, vec![inside]);

    // The band ending exactly on a scene border is inside it: that second is where the next scene starts, and a zoom
    // reaching it is on the picture, not past it.
    let border = fx("text", 6.0, 4.0);
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0), seg(20.0, 30.0)], &[border]));
    assert_eq!(out.dropped, 0);
    assert_eq!(one(&out.kept).t, 6.0);
    assert_eq!(one(&out.kept).dur, 4.0);
}

/// F3.12 S1 (`a point effect needs a footage clip under it`) — with no width there is nothing to overlap, so being
/// inside a scene is the whole question. A stamp on a second the cut dropped has no picture to mark and disappears;
/// one landing exactly on a scene's first or last second counts as on it.
#[test]
fn f3_12_s2_a_point_effect_needs_footage_under_it() {
    // Inside footage: kept, and still a point — the pass gives a stamp no width it did not have.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[fx("label", 4.0, 0.0)]));
    assert_eq!(out.dropped, 0);
    assert_eq!(one(&out.kept).dur, 0.0);

    // In the hole between two scenes: nothing is playing at 15 s, so there is nothing to mark.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0), seg(20.0, 30.0)], &[fx("label", 15.0, 0.0)]));
    assert_eq!(out.dropped, 1);
    assert!(out.kept.is_empty());

    // Over an insert only: a card brings its own picture, and a moment marked on it is not a moment of the recording.
    let out = clamp::clamp_to_cut(&cut_of(&[card(5.0)], &[fx("label", 2.0, 0.0)]));
    assert_eq!(out.dropped, 1);

    // The two ends of a scene are inside it.
    let out = clamp::clamp_to_cut(&cut_of(
        &[seg(10.0, 20.0)],
        &[fx("label", 10.0, 0.0), fx("label", 20.0, 0.0)],
    ));
    assert_eq!(out.dropped, 0);
    assert_eq!(out.kept.len(), 2);
}

/// F3.12 S2 (`a zoom/text/svg/volume band is trimmed to the clip it overlaps most`) — the winner is the scene sharing
/// the most time with the band, not the first one it touches, and trimming moves `t` as well as shortening `dur`.
#[test]
fn f3_12_s3_the_band_follows_the_scene_it_overlaps_most() {
    // Two scenes; the band straddles both with more of it in the second.
    let segs = [seg(0.0, 10.0), seg(10.0, 30.0)];
    let out = clamp::clamp_to_cut(&cut_of(&segs, &[fx("zoom", 7.0, 8.0)]));
    assert_eq!(out.dropped, 0);
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (10.0, 5.0));

    // The same band the other way round belongs to the first scene.
    let out = clamp::clamp_to_cut(&cut_of(&segs, &[fx("zoom", 2.0, 8.0)]));
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (2.0, 8.0));

    // A band reaching past the last scene is pulled back to where the footage ends.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[fx("volume", 6.0, 8.0)]));
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (6.0, 4.0));

    // A band over a card with footage beside it: the card is skipped, so the overlap that decides is with the footage.
    let out = clamp::clamp_to_cut(&cut_of(
        &[card(20.0), seg(0.0, 10.0)],
        &[fx("text", 8.0, 6.0)],
    ));
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (8.0, 2.0));

    // No footage under it at all: gone rather than floated over a card.
    let out = clamp::clamp_to_cut(&cut_of(&[card(30.0)], &[fx("text", 2.0, 6.0)]));
    assert_eq!(out.dropped, 1);
}

/// F3.12 S3 (`dropped under P.eng.effectMinSurvivingSeconds`) — a *clamped* band shorter than a second is not the effect
/// that was meant, since most of the moment it decorated is no longer in the video. The floor belongs to the survivor:
/// an effect the cut has no reason to trim is never judged by it, or a hand-placed zoom would vanish on reload.
#[test]
fn f3_12_s4_too_little_of_it_survives_and_it_goes() {
    // Exactly the floor survives: `>=`, so a band reaching 1 s past the footage's end stays at that one second.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[fx("zoom", 9.0, 2.0)]));
    assert_eq!(out.dropped, 0);
    assert_eq!(one(&out.kept).dur, 1.0);

    // A band with nothing to trim has nothing to measure: the floor belongs to a CLAMPED band, so a 0.5 s zoom sitting
    // whole inside kept footage comes back exactly as placed. Dropping it here would delete a hand-placed effect the cut
    // has no complaint about.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(20.0, 30.0)], &[fx("zoom", 25.0, 0.5)]));
    assert_eq!(out.dropped, 0);
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (25.0, 0.5));

    // A hair under the floor: gone. This band reaches 1.5 s past the footage, so 0.5 s of it survives — most of the
    // moment it decorated is not in the video, and a quarter-second zoom on screen is worse than none.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[fx("zoom", 9.5, 2.0)]));
    assert_eq!(out.dropped, 1);
    assert!(out.kept.is_empty());

    // A band reaching in from before the footage is pulled up to where it starts: this one covers 19.5–23.5 and comes
    // back as 20.0–23.5 — moved, still long enough, and never padded out past what was proposed.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(20.0, 30.0)], &[fx("text", 19.5, 4.0)]));
    assert_eq!(out.dropped, 0);
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (20.0, 3.5));

    // …and one reaching past a scene's end is trimmed there too: `s` and `e` are both session seconds, so the cut does
    // say where this scene's picture stops. 8.5–10.5 comes back 1.5 s long, which is over the floor and stays.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[fx("text", 8.5, 2.0)]));
    assert_eq!(out.dropped, 0);
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (8.5, 1.5));

    // A band sharing no time with kept footage is gone rather than floated over the hole: an effect between two scenes
    // decorates no picture, because the seconds it marks are not in the video.
    let out = clamp::clamp_to_cut(&cut_of(
        &[seg(0.0, 10.0), seg(30.0, 40.0)],
        &[fx("svg", 22.0, 1.0)],
    ));
    assert_eq!(out.dropped, 1);
}

/// F3.12 S4 (`a speed is re-clamped unless a stop`) — a rate has to stay playable over the seconds the cut left, so the
/// clock gives way before the marked seconds do. A stop is not a rate: clamping it would hand the frozen frame one and
/// turn a still into slow motion because the cut moved under it.
#[test]
fn f3_12_s5_a_speed_is_re_clamped_and_a_stop_is_not() {
    // 40 s of footage left as 2 s of band: ×20 puts 25 s on screen, well over what is there, so the rate falls to
    // dur / P.eng.minClipSeconds = 2 / 0.5 = 4. The band keeps its length — the seconds are what was marked.
    let fast = Fx {
        rate: 20.0,
        ..fx("speed", 8.0, 40.0)
    };
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[fast]));
    assert_eq!(out.dropped, 0);
    assert_eq!(one(&out.kept).dur, 2.0);
    assert_eq!(one(&out.kept).rate, 4.0);

    // A rate inside the range and playable over what is left comes back as proposed.
    let ok = Fx {
        rate: 2.0,
        ..fx("speed", 1.0, 3.0)
    };
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[ok]));
    assert_eq!(one(&out.kept).rate, 2.0);
    assert_eq!(one(&out.kept).dur, 3.0);

    // A stop reaching past the footage keeps rate 0 and is only trimmed — no ramp appears on a still.
    let stop = Fx {
        rate: 0.0,
        ..fx("stop", 6.0, 8.0)
    };
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[stop]));
    assert_eq!(out.dropped, 0);
    assert_eq!(one(&out.kept).rate, 0.0);
    assert_eq!(one(&out.kept).dur, 4.0);

    // A stop is kept by the same rule even where a rate would have been clamped away: under a second of survivor is
    // still too little, and it is the floor that decides, not the rate.
    let stop = Fx {
        rate: 0.0,
        ..fx("stop", 9.5, 1.2)
    };
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[stop]));
    assert_eq!(out.dropped, 1);
}

/// F3.12 S4 (`fades shrink proportionally`) — each fade keeps the share of the band it had before the trim, and both
/// together still fit inside what is left. A caption that loses most of its length must not spend the seconds it has on
/// fading in.
#[test]
fn f3_12_s6_the_fades_shrink_with_the_band() {
    // 8 s band with a 2 s fade either side, trimmed to half: each fade becomes 1 s.
    let out = clamp::clamp_to_cut(&cut_of(
        &[seg(0.0, 10.0)],
        &[fx_faded("text", 6.0, 8.0, 2.0, 2.0)],
    ));
    assert_eq!((one(&out.kept).dur, one(&out.kept).trans, one(&out.kept).tout), (4.0, 1.0, 1.0));

    // Uneven fades keep their ratio: a third of the band left turns 3 s in and 1 s out into 1 s and 1/3 s.
    let out = clamp::clamp_to_cut(&cut_of(
        &[seg(0.0, 10.0)],
        &[fx_faded("svg", 6.0, 9.0, 3.0, 1.0)],
    ));
    assert_eq!(one(&out.kept).dur, 4.0);
    assert!((one(&out.kept).trans - 4.0 / 3.0).abs() < 1e-9);
    assert!((one(&out.kept).tout - 4.0 / 9.0).abs() < 1e-9);

    // Fades that no longer fit the trimmed band are shared out in proportion, the one fade rule of §06-effects §7:
    // 5 s of fade over a 2 s band becomes 1.6 and 0.4.
    let out = clamp::clamp_to_cut(&cut_of(
        &[seg(0.0, 10.0)],
        &[fx_faded("volume", 8.0, 4.0, 4.0, 1.0)],
    ));
    assert_eq!((one(&out.kept).dur, one(&out.kept).trans, one(&out.kept).tout), (2.0, 1.6, 0.4));

    // A band the pass does not trim keeps its fades whole — shrinking is about the piece the cut took away.
    let out = clamp::clamp_to_cut(&cut_of(
        &[seg(0.0, 30.0)],
        &[fx_faded("zoom", 5.0, 4.0, 1.0, 1.0)],
    ));
    assert_eq!((one(&out.kept).trans, one(&out.kept).tout), (1.0, 1.0));
}

/// F3.12 S5 (`">>> N effect(s) pointed at footage the final cut does not keep — dropped"`) — only the count reaches the
/// log, and only when something went: which effect was dropped is arithmetic anyone can redo from the cut, and a line
/// per effect on a caption run would bury the rest of it. The separator is an em dash (U+2014).
#[test]
fn f3_12_s7_only_the_count_of_drops_reaches_the_log() {
    let line = clamp::log_line(3);
    assert_eq!(
        line,
        ">>> 3 effect(s) pointed at footage the final cut does not keep \u{2014} dropped"
    );
    assert!(!line.contains("--"), "the log uses an em dash, not two hyphens");
    assert_eq!(clamp::log_line(1), ">>> 1 effect(s) pointed at footage the final cut does not keep \u{2014} dropped");

    // The count is of effects, not of scenes or of trims: three of five go here.
    let segs = [seg(0.0, 10.0), seg(20.0, 30.0)];
    let effects = [
        fx("zoom", 2.0, 4.0),   // inside the first scene: stays
        fx("text", 15.0, 3.0),  // in the hole: goes
        fx("volume", 8.0, 6.0), // trimmed to the first scene's last 2 s: stays
        fx("label", 15.0, 0.0), // a point in the hole: goes
        fx("svg", 29.5, 3.0),   // 0.5 s of survivor: goes
    ];
    let out = clamp::clamp_to_cut(&cut_of(&segs, &effects));
    assert_eq!(out.dropped, 3);
    assert_eq!(clamp::log_line(out.dropped), line);

    // Nothing dropped: no line is written at all, so a clean run stays quiet.
    let out = clamp::clamp_to_cut(&cut_of(&segs, &[fx("zoom", 2.0, 4.0)]));
    assert_eq!(out.dropped, 0);
}

/// F3.12 S2 and §7 (`effects cannot leave the timeline`) — running the pass twice over its own answer changes nothing,
/// because a trimmed band now sits inside the scene it was trimmed to. A re-run is what happens whenever the user
/// presses the suggest button again, so an idempotent pass must not shave the effect down each time.
#[test]
fn f3_12_s8_a_second_pass_over_the_answer_changes_nothing() {
    let segs = [seg(0.0, 10.0), seg(14.0, 30.0)];
    let effects = [
        fx_faded("zoom", 7.0, 9.0, 2.0, 1.0),
        Fx {
            rate: 8.0,
            ..fx("speed", 1.0, 30.0)
        },
        fx("label", 20.0, 0.0),
    ];
    let once = clamp::clamp_to_cut(&cut_of(&segs, &effects));
    assert_eq!(once.dropped, 0);

    let twice = clamp::clamp_to_cut(&cut_of(&segs, &once.kept));
    assert_eq!(twice.dropped, 0);
    assert_eq!(twice.kept, once.kept);
}

/// F3.12 (`a label is trimmed like the rest`, where the prototype drops it outright) — a label marks a moment for the
/// narration brief, and §F3.12 says so outright: "the moment may still be in the cut". So a label whose band hangs over
/// the end of the footage is pulled back to it rather than deleted, and only loses its mark when the marked seconds are
/// gone from the video altogether. A kind this build does not know how to trim is left where it is: an old file with a
//  future `kind` in it must not cost the user an effect.
#[test]
fn f3_12_s9_a_label_is_trimmed_and_an_unknown_kind_is_left_alone() {
    // Trimmed, not dropped: the label survives with its text and its trimmed band.
    let marked = Fx {
        text: "results".into(),
        ..fx("label", 8.0, 6.0)
    };
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 12.0)], &[marked]));
    assert_eq!(out.dropped, 0);
    assert_eq!((one(&out.kept).t, one(&out.kept).dur), (8.0, 4.0));
    assert_eq!(one(&out.kept).text, "results");

    // The same label entirely over an insert has no footage to mark and goes.
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0), card(6.0)], &[fx("label", 2.0, 4.0)]));
    assert_eq!(out.dropped, 0); // that band is inside the footage beside the card

    let out = clamp::clamp_to_cut(&cut_of(&[card(30.0)], &[fx("label", 2.0, 4.0)]));
    assert_eq!(out.dropped, 1);

    // An unknown kind: kept as proposed rather than dropped for having no trim rule.
    let odd = fx("marker", 8.0, 6.0);
    let out = clamp::clamp_to_cut(&cut_of(&[seg(0.0, 10.0)], &[odd.clone()]));
    assert_eq!(out.dropped, 0);
    assert_eq!(one(&out.kept), &odd);
}
