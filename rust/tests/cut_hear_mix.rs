// F2.5 (Hush and mix — what the preview hears) — spec/05-cut.md F2.5.
// One test per branch of the item's diagram: which scene decides, what the footage's own sound does,
// where each lane sits on the clock and when it is started, how rate and gain follow the effects under
// the line, and the one preview volume every preview shares. Every rule is a plain function in
// naivepost::cut_hear, so nothing here needs a pipeline or a display.

use naivepost::cut::{Fx, Lane, Seg};
use naivepost::cut_hear as hear;

/// The screenshot's own lane: `2026-09-16 17-26-20.wav`, placed at 1:14–1:59 on the session clock — 45 s
/// of file starting at its first second.
fn recorder() -> Lane {
    Lane {
        name: "2026-09-16 17-26-20".into(),
        src: "project:audio/2026-09-16 17-26-20.wav".into(),
        at: 74.0,
        off: 0.0,
        dur: 45.0,
    }
}

/// A footage scene: no `ins`, so it is the picture's own stretch of the session.
fn scene(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// F2.5 (Hush and mix) S1: the scene under the line is what decides — a second the cut removed, or one
/// another scene's end excludes, answers for nothing.
#[test]
fn f2_5_s1_the_scene_under_the_line_is_what_decides() {
    let segs = [scene(0.0, 30.0), scene(40.0, 70.0)];
    assert_eq!(hear::scene_at(&segs, 12.0), Some(0), "the first scene covers its own seconds");
    assert_eq!(hear::scene_at(&segs, 55.0), Some(1), "and the second its own");
    assert_eq!(hear::scene_at(&segs, 34.0), None, "a removed second decides nothing");
    assert_eq!(hear::scene_at(&segs, 30.0), None, "a scene's end is exclusive");

    let kept = Seg { quiet: vec!["2026-09-16 17-26-20".into()], ..scene(40.0, 70.0) };
    assert_eq!(hear::hush(&kept), ["2026-09-16 17-26-20".to_string()].as_slice(), "the scene's own quiet list");

    // An insert brings its own sound or replaces the lane it was laid in: a quiet list left on one would
    // silence a lane by a scene that cannot silence anything.
    let card = Seg { ins: "sting.mp4".into(), quiet: vec!["2026-09-16 17-26-20".into()], ..scene(70.0, 75.0) };
    assert!(card.is_overwrite_insert(), "an overwriting insert is an insert");
    assert!(hear::hush(&card).is_empty(), "and hears every lane by default, whatever the list says");

    let spliced = Seg { s: 75.0, e: 75.0, ins: "sting.mp4".into(), dur: 4.0, quiet: vec!["2026-09-16 17-26-20".into()], ..Default::default() };
    assert!(spliced.is_insert(), "a spliced card is the other kind");
    assert!(hear::hush(&spliced).is_empty(), "and answers the same way");
}

/// F2.5 (Hush and mix) S2: the footage's own sound is muted by property when the scene silences it — the
/// flag answers, nothing else moves.
#[test]
fn f2_5_s2_the_footages_own_sound_is_muted_by_property() {
    assert!(hear::footage_sound_heard(&scene(0.0, 30.0)), "heard unless the scene says otherwise");
    let silent = Seg { mute: true, ..scene(0.0, 30.0) };
    assert!(!hear::footage_sound_heard(&silent), "its own tick mutes it");

    // The same flag on an insert is its "play it SILENT" tick (F2.7): the question is identical.
    let card = Seg { ins: "sting.mp4".into(), ..Default::default() };
    assert!(hear::footage_sound_heard(&card), "a sting's sound plays unless it was muted");
    assert!(!hear::footage_sound_heard(&Seg { ins: "sting.mp4".into(), mute: true, ..Default::default() }), "and not when it was");
}

/// F2.5 (Hush and mix) S3: a lane is placed by the time stamp in its name and heard by default — its own
/// second of the same instant, on the same clock as the footage.
#[test]
fn f2_5_s3_a_lane_is_placed_by_its_stamp_and_heard_by_default() {
    let lane = recorder();
    assert_eq!(hear::lane_seconds(&lane, 100.0), 26.0, "the screenshot's lane is at 26 s of its file at 1:40");

    assert!(hear::lane_covers(&lane, hear::lane_seconds(&lane, 74.0), 74.0), "its first second counts");
    assert!(hear::lane_covers(&lane, 44.9, 118.9), "and its last fraction");
    assert!(!hear::lane_covers(&lane, -0.1, 73.9), "before the stamp there is nothing to play");
    assert!(!hear::lane_covers(&lane, 45.0, 119.0), "and after its end either");

    let out = hear::lane_start(&lane, &scene(74.0, 90.0), 100.0);
    assert!(matches!(out, hear::LaneStart::Seek { from: 26.0, .. }), "every lane is heard by default: {out:?}");
}

/// F2.5 (Hush and mix) S3: a lane the scene under the line silences is never started, and the press says
/// which lane in which scene it just silenced.
#[test]
fn f2_5_s3_a_lane_the_scene_silences_is_never_started() {
    let lane = recorder();
    let quiet = Seg { quiet: vec!["2026-09-16 17-26-20".into()], ..scene(40.0, 70.0) };
    // Asked at a second the lane does cover, so it is the silencing alone that refuses the start.
    let out = hear::lane_start(&lane, &quiet, 100.0);
    assert_eq!(out, hear::LaneStart::Silenced, "a silenced lane holds no stream at all");

    assert_eq!(
        hear::hush_status("2026-09-16 17-26-20", false, 40.0),
        "2026-09-16 17-26-20 is silent in the scene at 0:40",
        "spec/05-cut.md F2.5's own status line"
    );
    assert_eq!(
        hear::hush_status("2026-09-16 17-26-20", true, 40.0),
        "2026-09-16 17-26-20 is heard in the scene at 0:40",
        "and the other way round; the minutes are not zero-padded"
    );
}

/// F2.5 (Hush and mix) S3: a lane whose file has nothing at that second is not started either — a seek
/// past its end would be a start from the file's first second, which is audible.
#[test]
fn f2_5_s3_a_lane_with_nothing_at_that_second_is_not_started_either() {
    let window = Lane { name: "clip-window".into(), src: "project:audio/window.wav".into(), at: 0.0, off: 10.0, dur: 10.0 };
    let out = hear::lane_start(&window, &scene(90.0, 120.0), 100.0);
    assert_eq!(out, hear::LaneStart::Nothing, "the file is silent at the line's second");

    // A separate recording writes no length: it is its whole file, from `off` on.
    let whole = Lane { name: "rec".into(), src: "project:audio/rec.wav".into(), at: 0.0, off: 5.0, dur: 0.0 };
    assert!(hear::lane_covers(&whole, hear::lane_seconds(&whole, 6.0), 6.0), "the second after its first");
    assert!(hear::lane_covers(&whole, 3600.0, 3605.0), "an hour in it is still its own file");
    assert!(!hear::lane_covers(&whole, 4.0, -1.0), "and before its first second there is nothing");
}

/// F2.5 (Hush and mix) S4: a lane started under a scene boundary is seeked with a stop at that boundary,
/// written in the lane's own clock so it falls silent there without waiting for a tick to notice.
#[test]
fn f2_5_s4_a_lane_started_under_a_scene_boundary_stops_at_it() {
    let lane = recorder();
    let held = scene(90.0, 110.0);
    assert_eq!(hear::stop_at_boundary(&held, &lane, 100.0), 36.0, "the scene's end is the file's second 36");

    let out = hear::lane_start(&lane, &held, 100.0);
    assert_eq!(out, hear::LaneStart::Seek { from: 26.0, stop: 36.0 }, "seeked here, stopped at the boundary");

    // The stop is the LANE's second, not the session's: sliding the file by 2.5 s moves the stop with it.
    let slid = Lane { off: 2.5, ..recorder() };
    assert_eq!(hear::stop_at_boundary(&held, &slid, 100.0), 38.5, "2.5 s of file skipped before the stamp");

    // A scene running past the lane's own end has no boundary inside it: 0 is "no stop".
    let past = scene(90.0, 200.0);
    assert_eq!(hear::stop_at_boundary(&past, &lane, 100.0), 0.0, "nothing of the lane falls silent there");
}

/// F2.5 (Hush and mix) S5: with no volume effect over the line the sound is left alone — and only a volume
/// effect touches it.
#[test]
fn f2_5_s5_no_volume_effect_leaves_the_sound_alone() {
    assert_eq!(hear::gain_under(&[], 5.0), 1.0, "no effect over it, no change to it");

    let zoom = Fx { kind: "zoom".into(), t: 0.0, dur: 10.0, gain: 0.2, ..Default::default() };
    assert_eq!(hear::gain_under(&[zoom], 5.0), 1.0, "a zoom says nothing about loudness");

    let quiet = Fx { kind: "volume".into(), t: 10.0, dur: 10.0, gain: 0.5, ..Default::default() };
    assert_eq!(hear::gain_under(&[quiet.clone()], 15.0), 0.5, "half as loud across its seconds");
    assert_eq!(hear::gain_under(&[quiet.clone()], 9.9), 1.0, "and untouched before them");
    assert_eq!(hear::gain_under(&[quiet], 20.0), 1.0, "or after them: its end is exclusive");
}

/// F2.5 (Hush and mix) S5: two volume effects over the same seconds multiply — two gains are two things
/// done to the same sound — and each one's fades ramp it in and out.
#[test]
fn f2_5_s5_two_volume_effects_multiply_and_fade() {
    let half = Fx { kind: "volume".into(), t: 0.0, dur: 10.0, gain: 0.5, ..Default::default() };
    let double = Fx { kind: "volume".into(), t: 0.0, dur: 10.0, gain: 2.0, ..Default::default() };
    assert_eq!(hear::gain_under(&[half, double], 5.0), 1.0, "×0.5 and ×2 over one another is the recorded level");

    let fade = Fx { kind: "volume".into(), t: 0.0, dur: 10.0, gain: 0.5, trans: 2.0, ..Default::default() };
    assert_eq!(hear::gain_under(&[fade.clone()], 0.5), 0.875, "a quarter in to the ramp: ×0.5 is ×0.875 there");
    assert_eq!(hear::gain_under(&[fade.clone()], 2.0), 0.5, "the ramp is over, the gain holds");
    assert_eq!(hear::gain_under(&[fade], 10.5), 1.0, "and it is gone past the effect");

    // P.eng.maxGain (spec/10-parameters.md: "volume effect ceiling (playbin's own)") caps what the
    // property will take, however loudly an effect asks.
    assert_eq!(hear::MAX_GAIN, 10.0);
    let loud = Fx { kind: "volume".into(), t: 0.0, dur: 10.0, gain: 100.0, ..Default::default() };
    assert_eq!(hear::gain_under(&[loud], 5.0), hear::MAX_GAIN, "held to what the volume property accepts");

    let row = naivepost::params::find("P.eng.maxGain").expect("the ceiling is catalogued");
    assert_eq!(row.spelled, "10", "§10 spells it as a whole number");
    assert_eq!(row.from, "cut_hear::MAX_GAIN", "and this module's rule owns it");
}

/// F2.5 (Hush and mix) S5: the rate follows the speed effect under the line — flat across its seconds, 1
/// with none over it, and a stop is not a rate.
#[test]
fn f2_5_s5_the_rate_follows_the_speed_effect_under_the_line() {
    assert_eq!(hear::rate_under(&[], 5.0), 1.0, "no speed effect, the session's own clock");

    let fast = Fx { kind: "speed".into(), t: 10.0, dur: 10.0, rate: 2.0, ..Default::default() };
    assert_eq!(hear::rate_under(&[fast.clone()], 15.0), 2.0, "a slowed or sped stretch is slow or fast to watch");
    assert_eq!(hear::rate_under(&[fast.clone()], 9.0), 1.0, "outside its seconds nothing changes");

    // A stop is ×0 in the render's arithmetic and an overlay in the preview's: the still covers the
    // stretch, so the footage under it runs on at full speed rather than not at all.
    let stop = Fx { kind: "stop".into(), t: 10.0, dur: 5.0, ..Default::default() };
    assert_eq!(hear::rate_under(&[stop], 12.0), 1.0, "a freeze has no observable rate");

    // Two speeds over one second: the preview takes the first it is given. The render averages them —
    // that arithmetic belongs to F5's planning round, and the preview is only asked to be playable.
    let slow = Fx { kind: "speed".into(), t: 10.0, dur: 10.0, rate: 0.5, ..Default::default() };
    assert_eq!(hear::rate_under(&[fast, slow], 15.0), 2.0, "the first covering speed answers");
}

/// F2.5 (Hush and mix) S6: one preview volume, shared by every preview — a number the sliders mirror, that
/// changes nothing rendered.
#[test]
fn f2_5_s6_one_preview_volume_shared_by_every_preview() {
    let mut volume = hear::PreviewVolume::new();
    assert_eq!(volume.value(), 1.0, "full travel until somebody says otherwise");

    volume.set_percent(40.0);
    assert_eq!(volume.value(), 0.4, "the slider is 0..100, the property 0..1");
    assert_eq!(volume.percent(), 40.0, "and every other slider mirrors this number back");

    volume.set_percent(-5.0);
    assert_eq!(volume.value(), 0.0, "below nought is silence, not a negative gain");
    volume.set_percent(137.0);
    assert_eq!(volume.value(), 1.0, "past full travel is full travel");

    assert_eq!(
        hear::VOLUME_TIP,
        "preview volume — the players only; nothing that is rendered, and the same setting wherever it is shown",
        "spec/inventory/cut.md's wording: the players only, never the render"
    );
}

/// F2.5 (Hush and mix) S6: what a pipeline is asked for is the slider times the cut's own say over the
/// seconds under the line, capped at what the property will take.
#[test]
fn f2_5_s6_the_slider_and_the_cut_multiplied_and_capped() {
    // P.eng.maxGain caps the mix as it caps an effect: a boosted stretch played at full travel lands
    // inside what the volume property accepts rather than past it.
    assert_eq!(hear::mix_gain(0.5, 0.5), 0.25, "half the room under half the cut");
    assert_eq!(hear::mix_gain(1.0, 100.0), hear::MAX_GAIN, "the ceiling, however loud the ask");
    assert_eq!(hear::mix_gain(-1.0, 2.0), 0.0, "a slider below nought stays silence, not a phase flip");
}
