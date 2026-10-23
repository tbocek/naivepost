//! §06-effects#8 bullet 4 on the ENCODE path: the planned 1× read head is a
//! decision the render makes, not a filter it always adds.
//!
//! Three things had to become real: `produce_read_head::opens` answering the drift
//! and tape question together, `produce_render::plan_clips` storing the run on the
//! clip only when it opens, and `produce_fx_chain` spending half of the lane's
//! 0.15 s dip on each side of the join rather than the whole dip twice.

use naivepost::cut::{EffectKind, Fx, Seg};
use naivepost::effect_details;
use naivepost::fx_lane;
use naivepost::produce_details as details;
use naivepost::produce_read_head as head;
use naivepost::produce_render as render;

/// A 2× speed effect over a stretch, spelled the way the lane stores one.
fn sped(rate: f64, snd: &str, t: f64, dur: f64) -> Fx {
    Fx {
        kind: "speed".into(),
        t,
        dur,
        rate,
        snd: snd.into(),
        ..Default::default()
    }
}

/// §8 (`opens only where debt ≥ 0.05 s`): the threshold is `machine.driftSeconds`
/// and it is INCLUSIVE — 0.049 s of debt is inaudible, 0.05 s is worth a dip.
#[test]
fn sec_06_effects_8_details_s4_read_head_opens_only_on_drift() {
    assert_eq!(details::DRIFT_SECONDS, 0.05, "machine.driftSeconds");
    // 30.0 s of footage, 29.95 s on screen: 0.05 s of debt, the floor itself.
    assert!(head::opens(30.0, 29.95, true, true), "0.05 s of debt opens");
    assert!(
        head::opens(30.0, 30.05, true, true),
        "and the same debt the other way"
    );
    // One thousandth short of the floor: nothing to hear, so nothing to dip.
    assert!(!head::opens(30.0, 29.951, true, true), "0.049 s does not");
    assert!(!head::opens(30.0, 30.0, true, true), "no debt, no head");
    // The lane's own floor is the same number: preview and render agree.
    assert_eq!(fx_lane::DEBT_TAIL_MIN_SECONDS, details::DRIFT_SECONDS);
    assert!(
        (fx_lane::debt(30.0, 29.95) - 0.05).abs() < 1e-9,
        "the debt arithmetic is the lane's, not a copy"
    );
}

/// §8 (`a card, held frame or clip on no recording closes it`): the arithmetic
/// can say anything it likes about a clip that films nothing.
#[test]
fn sec_06_effects_8_details_s4b_a_card_held_frame_or_missing_recording_closes_it() {
    // Footage on tape: the only case that opens.
    assert!(head::opens(30.0, 29.9, true, true));
    // An insert (a card, a copy) films nothing to read back.
    assert!(!head::opens(30.0, 29.9, false, true), "a card closes it");
    // Ground with no recording under it has no sound to read.
    assert!(!head::opens(30.0, 29.9, true, false), "no tape closes it");
    assert!(
        !head::opens(30.0, 29.9, false, false),
        "both at once still closes"
    );
    // The two halves answer as the spec spells them, unchanged.
    assert!(details::run_closes(false, true));
    assert!(details::run_closes(true, false));
    assert!(!details::run_closes(true, true));
    // And the source/tape questions read the segment, not a guess: an `ins` is an insert.
    let footage = Seg {
        s: 0.0,
        e: 30.0,
        ..Default::default()
    };
    let card = Seg {
        s: 0.0,
        e: 30.0,
        ins: "cards/title.svg".into(),
        ..Default::default()
    };
    assert!(head::source_is_footage(&footage));
    assert!(!head::source_is_footage(&card), "a card is not footage");
    // Tape means the segment overlaps a filmed run; a gap does not.
    let runs = [(0.0, 20.0)];
    assert!(head::on_recording(&footage, &runs));
    assert!(
        !head::on_recording(
            &Seg {
                s: 25.0,
                e: 30.0,
                ..Default::default()
            },
            &runs
        ),
        "a gap"
    );
    assert!(!head::on_recording(&footage, &[]), "no tape named at all");
}

/// §8 (`the dip half on each side of the join`): two `afade`s, half the lane's dip
/// each, at the run's own ends — and NEITHER when the planner said the head is shut.
#[test]
fn sec_06_effects_8_details_s4c_the_chain_gets_half_a_dip_on_each_side() {
    let half = fx_lane::SOUND_DIP_SECONDS / 2.0;
    assert_eq!(effect_details::dip_each_side(), half);
    let mut clip = render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 5.0,
        rate: 2.0,
        read_head: Some((2.0, 8.0)),
        on_recording: true,
        ..Default::default()
    };
    let own = vec![sped(2.0, "own", 0.0, 10.0)];
    let built = naivepost::produce_fx_chain::for_clip(
        &clip,
        &own,
        (1920, 1080),
        std::path::Path::new("/tmp"),
    );
    let audio = built.audio.join(" , ");
    assert!(
        audio.contains(&format!("afade=t=in:st=2.000:d={half}")),
        "the incoming half of the join: {audio}"
    );
    assert!(
        audio.contains(&format!("afade=t=out:st=8.000:d={half}")),
        "the outgoing half: {audio}"
    );
    assert_eq!(
        built
            .audio
            .iter()
            .filter(|f| f.starts_with("afade="))
            .count(),
        2,
        "exactly two fades, one per side: {:?}",
        built.audio
    );
    assert!(
        !built
            .audio
            .iter()
            .any(|f| f.contains(&format!("d={}", fx_lane::SOUND_DIP_SECONDS))),
        "the whole dip is never spent on one side: {:?}",
        built.audio
    );
    // A shut head: no fades at all, and the sound simply follows the picture.
    clip.read_head = None;
    let shut = naivepost::produce_fx_chain::for_clip(
        &clip,
        &own,
        (1920, 1080),
        std::path::Path::new("/tmp"),
    );
    assert!(
        !shut.audio.iter().any(|f| f.contains("afade")),
        "a closed head dips nothing: {:?}",
        shut.audio
    );
    // `scene` reads the head the same way; a pitched or muted stretch never does.
    for snd in ["scene", "own"] {
        clip.read_head = Some((1.0, 4.0));
        let built = naivepost::produce_fx_chain::for_clip(
            &clip,
            &[sped(2.0, snd, 0.0, 10.0)],
            (1920, 1080),
            std::path::Path::new("/tmp"),
        );
        assert_eq!(
            built
                .audio
                .iter()
                .filter(|f| f.starts_with("afade="))
                .count(),
            2,
            "`{snd}` reads the head"
        );
    }
    for snd in ["pitch", "mute", ""] {
        let built = naivepost::produce_fx_chain::for_clip(
            &clip,
            &[sped(2.0, snd, 0.0, 10.0)],
            (1920, 1080),
            std::path::Path::new("/tmp"),
        );
        assert!(
            !built.audio.iter().any(|f| f.starts_with("afade=")),
            "`{snd}` has no run to read: {:?}",
            built.audio
        );
    }
}

/// The plan stores the run only when `opens` says so: a 2× stretch of footage on
/// tape gets one, the same stretch as a card does not, and a clip nobody re-timed
/// never does.
#[test]
fn sec_06_effects_8_details_s4d_plan_clips_stores_the_run() {
    let no_fx: &dyn Fn(&Seg) -> Vec<Fx> = &|_| Vec::new();
    let mut sped_seg = Seg {
        s: 0.0,
        e: 20.0,
        rate: 2.0,
        ..Default::default()
    };
    let fx = vec![sped(2.0, "own", 0.0, 20.0)];
    let fx_for: &dyn Fn(&Seg) -> Vec<Fx> = &|_| fx.clone();
    let (clips, logs) = render::plan_clips(
        &[sped_seg.clone()],
        fx_for,
        "16:9",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        render::unknown_footage,
    );
    assert!(logs.is_empty(), "{logs:?}");
    assert_eq!(
        clips[0].read_head,
        Some((0.0, 20.0)),
        "20 s of footage, 10 s on screen: 10 s of debt, so the run opens"
    );
    assert_eq!(clips[0].rate, 2.0);

    // The same seconds as a card: no tape to read back, so no run stored.
    sped_seg.ins = "cards/title.svg".into();
    let (cards, _) = render::plan_clips(
        &[sped_seg.clone()],
        fx_for,
        "16:9",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        render::unknown_footage,
    );
    assert_eq!(cards[0].read_head, None, "a card is not footage");
    assert_eq!(cards[0].source, "cards/title.svg");

    // Footage with no recording named for the session: same answer, other reason.
    sped_seg.ins = String::new();
    let (gap, _) = render::plan_clips(
        &[sped_seg],
        fx_for,
        "16:9",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        false,
        render::unknown_footage,
    );
    assert_eq!(gap[0].read_head, None, "no tape closes the head");

    // A clip nobody re-timed has no own-clock sound, whatever the tape says.
    let plain = Seg {
        s: 0.0,
        e: 20.0,
        ..Default::default()
    };
    let (still, _) = render::plan_clips(
        &[plain],
        no_fx,
        "16:9",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        render::unknown_footage,
    );
    assert_eq!(still[0].read_head, None, "no speed effect, no run");

    // A 1.000× stretch shows what it films: no debt, so the head stays shut.
    let even = Seg {
        s: 0.0,
        e: 20.0,
        rate: 1.0,
        ..Default::default()
    };
    let (flat, _) = render::plan_clips(
        &[even],
        fx_for,
        "16:9",
        |_| true,
        &[],
        &[],
        (1920, 1080),
        true,
        render::unknown_footage,
    );
    assert_eq!(flat[0].read_head, None, "no drift means no dip");
    assert!(
        clips[0]
            .fx
            .iter()
            .any(|e| e.effect_kind() == Some(EffectKind::Speed)),
        "the sped stretch still carries its speed effect"
    );
}
