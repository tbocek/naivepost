//! F3.3 Speed and stop by hand — `spec/06-effects.md` F3.3, checked against [`naivepost::cut_speed`].
//!
//! The flow is one decision (a band, a line, or neither), six form fields, two clamps and the arithmetic that turns
//! overlapping rates into flat stretches. Strings are compared whole — a reworded status line counts as a broken
//! one — and every number is compared against the constant §10 names. No widget and no display:
//! `rust/src/ui/window.rs` still renders only Prepare, so there is no ⏩ Speed button to press yet.

use naivepost::cut::Fx;
use naivepost::cut_hear;
use naivepost::cut_speed as speed;
use naivepost::fx_lane;
use naivepost::fx_record;
use naivepost::tools::cutpass;

/// A speed over its own seconds, no ramps.
fn rate_at(t: f64, dur: f64, rate: f64) -> Fx {
    Fx { kind: "speed".into(), t, dur, rate, ..Default::default() }
}

/// The form as it would be handed to Apply.
fn form(t: f64, dur: f64, rate: f64, trans: f64, tout: f64) -> speed::Form {
    speed::Form { t, dur, rate, trans, tout, curve: "Linear".into(), snd: "" }
}

/// F3.3 S1 — a marked stretch of at least 0.2 s gives its seconds and rate 0.5; only a line gives a stop; neither
/// refuses.
#[test]
fn f3_3_s1_a_band_a_line_or_neither() {
    assert_eq!(speed::MIN_MARKED_SECONDS, 0.2);

    let band = speed::press(Some((30.0, 42.0)), None);
    assert_eq!(band, speed::Pressed::Stretched { t: 30.0, dur: 12.0 });
    let effect = speed::initial(band).expect("a marked stretch places something");
    assert_eq!(effect.kind, "speed");
    assert_eq!((effect.t, effect.dur), (30.0, 12.0));
    assert_eq!(effect.rate, speed::DEFAULT_RATE);
    assert_eq!(speed::DEFAULT_RATE, 0.5);
    assert_eq!((effect.trans, effect.tout), (0.0, 0.0));
    // No curve was chosen, so no `ease` key is written (§06#1: "" = linear, old files byte-identical).
    assert_eq!(effect.ease, fx_record::LINEAR_EASE);

    // A right-to-left drag is the same stretch.
    assert_eq!(speed::press(Some((42.0, 30.0)), None), band);

    // A fifth of a second under the pointer is a click that slipped: not a band, so the line answers.
    assert_eq!(speed::press(Some((30.0, 30.19)), Some(50.0)), speed::Pressed::StopAtLine);
    // The floor is where a band starts counting. 0.2 is not representable in binary, so the width is built from the
    // bound itself — writing `30.2 - 30.0` here would ask for a hair under the floor and lose it to rounding.
    let (from, to) = (30.0, 30.0 + speed::MIN_MARKED_SECONDS);
    assert_eq!(speed::press(Some((from, to)), None), speed::Pressed::Stretched { t: from, dur: to - from });

    // A stop at the line: 2 s with 0.5 s fades.
    // P.policy.effectDefaultSeconds (stop/speed/volume/label 2) and P.policy.effectDefaultFades (stop 0.5).
    assert_eq!(speed::STOP_SECONDS, 2.0);
    assert_eq!(speed::STOP_FADE_SECONDS, 0.5);
    let stop = speed::initial(speed::Pressed::StopAtLine).expect("a line places a stop");
    assert_eq!((stop.rate, stop.dur), (0.0, 2.0));
    assert_eq!((stop.trans, stop.tout), (0.5, 0.5));
    assert!(fx_record::is_stop(&stop));

    // Neither: the refusal, whole — and nothing placed.
    assert_eq!(speed::press(None, None), speed::Pressed::Refused);
    assert_eq!(speed::press(Some((30.0, 30.1)), None), speed::Pressed::Refused);
    assert_eq!(
        speed::NO_SECONDS,
        "click a track or mark a stretch first \u{2014} speed needs seconds to work on"
    );
    assert_eq!(speed::initial(speed::Pressed::Refused), None);
}

/// F3.3 S2 — the form's title, its six fields in order, the rate list with Custom after it, and the five sound answers.
#[test]
fn f3_3_s2_the_form_asks_six_questions() {
    assert_eq!(speed::form_title(12.0, 5.0), "Speed 00:12 \u{2013} 00:17");

    assert_eq!(speed::RATES, [0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0, 8.0, 20.0, 100.0]);
    assert_eq!(speed::CUSTOM, "Custom\u{2026}");
    assert_eq!(speed::rate_label(0.0), "\u{00d7}0 \u{2014} stop");
    assert_eq!(speed::rate_label(1.0), "\u{00d7}1 \u{2014} as filmed");
    assert_eq!(speed::rate_label(0.25), "\u{00d7}0.25");
    assert_eq!(speed::rate_label(100.0), "\u{00d7}100", "no trailing .0 on a whole rate");

    // The listed rows by index, and everything else landing on Custom with its own number kept.
    assert_eq!(speed::rate_index(0.0), 0);
    assert_eq!(speed::rate_index(0.5), 2);
    assert_eq!(speed::rate_index(1.0), 4);
    assert_eq!(speed::rate_index(100.0), 10);
    for unlisted in [3.0, 0.6, -1.0] {
        assert_eq!(speed::rate_index(unlisted), speed::RATES.len(), "{unlisted} is Custom");
    }

    assert_eq!(
        speed::FORM_FIELDS,
        ["Speed \u{00d7}", "Sound", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"]
    );
    assert_eq!(
        speed::SOUND_CHOICES,
        [
            "With the picture",
            "With the picture, pitched",
            "1\u{00d7} to the effect's end",
            "1\u{00d7} to the scene's end",
            "Silent",
        ]
    );

    // The two lists are one list read twice: each row stores the key fx_record reads, in order.
    let stored = speed::SOUND_CHOICES.map(|row| speed::sound_stored(row));
    assert_eq!(stored, ["", "pitch", "own", "scene", "mute"]);
    assert_eq!(fx_record::snd_of(&Fx { snd: stored[0].into(), ..Default::default() }), fx_record::Snd::Default);
    assert_eq!(fx_record::snd_of(&Fx { snd: stored[1].into(), ..Default::default() }), fx_record::Snd::Pitch);
    assert_eq!(fx_record::snd_of(&Fx { snd: stored[2].into(), ..Default::default() }), fx_record::Snd::Own);
    assert_eq!(fx_record::snd_of(&Fx { snd: stored[3].into(), ..Default::default() }), fx_record::Snd::Scene);
    assert_eq!(fx_record::snd_of(&Fx { snd: stored[4].into(), ..Default::default() }), fx_record::Snd::Mute);

    // Fade in's help states the stair price, computed from the bound rather than repeated.
    let help = speed::fade_in_help();
    assert!(
        help.contains("A ramp needs about 0.6s of footage for every \u{00d7} of the rate \u{2014} 2.4s at \u{00d7}4"),
        "{help}"
    );
    assert!(help.ends_with("under that it is treated as 0."), "{help}");
}

/// F3.3 S2 — the cost note: the sound arithmetic done for whoever is choosing, and silence where there is nothing to say.
#[test]
fn f3_3_s2b_the_cost_note_is_the_arithmetic_done_for_them() {
    // A stop: four of the five answers are the same sound, because the footage under a held frame runs on at 1×.
    assert_eq!(
        speed::cost_note("", 0.0, 2.0),
        "A stop's footage runs on at 1\u{00d7} under the held frame, so every answer but Silent sounds the same here."
    );

    // With the picture (and pitched) the clock takes the sound too — no gap to state, at any rate.
    assert_eq!(speed::cost_note("", 4.0, 8.0), "");
    assert_eq!(speed::cost_note("pitch", 0.25, 8.0), "");
    assert_eq!(speed::cost_note("mute", 2.0, 8.0), "");

    // The two 1× answers put the sound on its own clock: behind when the picture runs fast \u2026
    let behind = speed::cost_note("own", 2.0, 6.0);
    assert_eq!(
        behind,
        "3 s on screen: the sound ends 3 s behind the picture, and going back in sync skips those seconds."
    );
    // \u2026 and the number it prints is the lane's own debt.
    assert_eq!(fx_lane::debt(6.0, 3.0), 3.0);

    // \u2026 and ahead when it runs slow.
    assert_eq!(
        speed::cost_note("scene", 0.5, 4.0),
        "8 s on screen: the sound runs 4 s ahead of the picture, and going back in sync plays those seconds again."
    );

    // A gap under a twentieth of a second is a rounding difference, not something to warn about.
    assert_eq!(speed::cost_note("own", 1.0 + 1e-6, 1.0), "");
}

/// F3.3 S3 — rate 0 is a stop and never clamped; otherwise the rate is held inside P.eng.minRate…maxRate and gives
/// way to P.eng.minClipSeconds.
#[test]
fn f3_3_s3_a_stop_is_not_a_rate() {
    assert_eq!(speed::MIN_RATE, 0.05); // P.eng.minRate
    assert_eq!(speed::MAX_RATE, 100.0); // P.eng.maxRate
    assert_eq!(cutpass::MIN_CLIP_SECONDS, 0.5); // P.eng.minClipSeconds

    // In bounds: untouched.
    assert_eq!(speed::clamp_speed(0.5, 8.0), (0.5, 8.0));

    // The floor.
    assert_eq!(speed::clamp_speed(0.01, 3.0), (speed::MIN_RATE, 3.0));

    // Over the ceiling: clamped to 100 and then the rate gives way — dur / minClipSeconds = 20 — because the seconds
    // are what the person marked and can see.
    assert_eq!(speed::clamp_speed(200.0, 10.0), (20.0, 10.0));

    // Exactly on the floor: 1 s of footage at ×2 is 0.5 s on screen, which renders.
    let (rate, dur) = speed::clamp_speed(4.0, 1.0);
    assert_eq!((rate, dur), (2.0, 1.0));
    assert!((dur / rate - cutpass::MIN_CLIP_SECONDS).abs() < 1e-12);

    // A band shorter than the floor grows to it and the rate comes back to ×1.
    assert_eq!(speed::clamp_speed(2.0, 0.2), (1.0, 0.5));

    // Apply: a stop keeps its 0 and only takes the length floor — clampSpeed would have handed it a rate.
    let stopped = speed::apply(&form(10.0, 0.1, 0.0, 0.5, 0.5));
    assert_eq!(stopped.rate, 0.0);
    assert_eq!(stopped.dur, 0.5);
    assert!(fx_record::is_stop(&stopped));

    // Apply: overrunning fades share the band in proportion, both still positive.
    let ramped = speed::apply(&form(0.0, 3.0, 2.0, 2.0, 2.0));
    assert!(ramped.trans > 0.0 && ramped.tout > 0.0, "{ramped:?}");
    assert!(ramped.trans + ramped.tout <= ramped.dur + 1e-9, "{ramped:?}");
    assert!((ramped.trans - ramped.tout).abs() < 1e-9, "shared equally because both were asked equally");
    // A negative fade is nought, not a backwards ramp.
    let hard = speed::apply(&form(0.0, 4.0, 2.0, -1.0, 0.5));
    assert_eq!((hard.trans, hard.tout), (0.0, 0.5));

    // The curve: "Linear" writes no key; a name this build does not know survives rather than being flattened.
    assert!(fx_record::keeps_the_old_bytes(&ramped));
    let mut unknown = form(0.0, 4.0, 2.0, 0.0, 0.0);
    unknown.curve = "EaseInOut".into();
    assert_eq!(speed::apply(&unknown).ease, "EaseInOut");
}

/// F3.3 S4 — the two status sentences, one per branch, each after the effect's own label.
#[test]
fn f3_3_s4_what_the_page_says() {
    let fast = speed::apply(&form(75.0, 6.0, 2.0, 0.0, 0.0));
    let stopped = speed::apply(&form(75.0, 2.0, 0.0, 0.5, 0.5));

    // The label names the moment through tools::mm_ss, so it reads like every other stamp on the page.
    assert_eq!(speed::label(&fast), "01:15 sped up \u{00d7}2 for 6.0s");
    assert_eq!(speed::label(&stopped), "stop at 01:15 for 2.0s");

    let status = speed::placed_status(&fast);
    assert!(status.starts_with("01:15 sped up \u{00d7}2 for 6.0s \u{2014} "), "{status}");
    assert!(
        status.ends_with(
            "the footage plays at that rate there and the cut gets longer or shorter to match; \u{21b6} Undo takes it back"
        ),
        "{status}"
    );

    let stopped_status = speed::placed_status(&stopped);
    assert!(stopped_status.starts_with("stop at 01:15 for 2.0s \u{2014} "), "{stopped_status}");
    assert!(
        stopped_status.ends_with(
            "the picture stands still there while the clock runs; \u{21b6} Undo takes it back"
        ),
        "{stopped_status}"
    );

    // One sentence each, and neither leaks into the other's branch.
    assert_ne!(status, stopped_status);
    assert!(!status.contains("stands still"));
    assert!(!stopped_status.contains("gets longer or shorter"));
}

/// §F3.3 (`overlapping rates average per span`): every boundary any effect draws cuts the clock, and between two of
/// them the covering rates agree on the arithmetic mean.
#[test]
fn f3_3_s5_overlapping_rates_average_per_span() {
    let fx = [rate_at(0.0, 10.0, 2.0), rate_at(5.0, 10.0, 4.0)];
    let spans = speed::rate_spans(&fx);
    assert_eq!(spans.len(), 3, "{spans:?}");
    assert_eq!((spans[0].from, spans[0].to, spans[0].rate), (0.0, 5.0, 2.0));
    assert_eq!((spans[1].from, spans[1].to, spans[1].rate), (5.0, 10.0, 3.0), "the mean of ×2 and ×4");
    assert_eq!((spans[2].from, spans[2].to, spans[2].rate), (10.0, 15.0, 4.0));
    for span in &spans {
        assert!((span.to - span.from) / speed::applied_rate(span.rate) >= cutpass::MIN_CLIP_SECONDS - 1e-9, "{span:?}");
    }

    // The render's clock follows the spans; seconds no effect covers are simply ×1.
    assert_eq!(speed::rate_at(&fx, 2.0), 2.0);
    assert_eq!(speed::rate_at(&fx, 7.5), 3.0);
    assert_eq!(speed::rate_at(&fx, 20.0), 1.0);

    // Two effects agreeing on a rate do not cut the video: one stretch comes back.
    let same = [rate_at(0.0, 10.0, 2.0), rate_at(10.0, 10.0, 2.0)];
    assert_eq!(speed::rate_spans(&same).len(), 1, "{:?}", speed::rate_spans(&same));

    // Nothing placed at all is no clock to speak of.
    assert!(speed::rate_spans(&[]).is_empty());
    assert_eq!(speed::rate_at(&[], 5.0), 1.0);
}

/// §F3.3 (`spans rendering under 0.5 s heal into the longer neighbour`): a sliver two effects slice out of each other
/// is not encoded, so its seconds go to the longer neighbour rather than becoming missing footage.
#[test]
fn f3_3_s5b_a_sliver_heals_into_the_longer_neighbour() {
    let fx = [rate_at(10.0, 5.0, 2.0), rate_at(10.1, 5.0, 4.0)];
    let spans = speed::rate_spans(&fx);
    assert!(!spans.is_empty(), "{spans:?}");
    for span in &spans {
        assert!(
            (span.to - span.from) / speed::applied_rate(span.rate) >= cutpass::MIN_CLIP_SECONDS - 1e-9,
            "{span:?} renders under the floor"
        );
    }
    // The covered interval survives whole: nothing was lost, only re-assigned.
    assert_eq!(spans.first().unwrap().from, 10.0, "{spans:?}");
    assert_eq!(spans.last().unwrap().to, 15.1, "{spans:?}");

    // The head [10, 10.1) at ×2 is the thinnest and its right neighbour (10.1–15 at ×3, then 4) is longer, so the
    // first returned stretch starts at the band's start and runs at the neighbour's rate.
    assert_eq!(spans[0].from, 10.0);
    assert!(spans[0].to > 10.1, "the sliver was taken by its longer neighbour: {:?}", spans[0]);

    // A short stop on its own cuts nothing — it is the only thing there and both neighbours are ×1 — so healing
    // leaves it alone rather than deleting someone's still.
    let stop = [Fx { kind: "speed".into(), t: 4.0, dur: 0.2, rate: 0.0, ..Default::default() }];
    let spans = speed::rate_spans(&stop);
    assert_eq!(spans.len(), 1, "a lone stop is not healed away: {spans:?}");
    assert_eq!((spans[0].from, spans[0].to, spans[0].rate), (4.0, 4.2, 0.0));
}

/// §F3.3 (`footage under a still runs at 1×`), and the deliberate difference between the render's clock and the
/// preview's flat one (§06#2).
#[test]
fn f3_3_s5c_footage_under_a_still_runs_at_1x() {
    let stop = [Fx { kind: "speed".into(), t: 10.0, dur: 2.0, rate: 0.0, ..Default::default() }];
    assert!(fx_record::is_stop(&stop[0]));
    // The mean keeps the 0 (that is what makes it a still); what plays underneath is ×1.
    assert_eq!(speed::rate_spans(&stop)[0].rate, 0.0);
    assert_eq!(speed::rate_at(&stop, 11.0), 1.0);
    // The preview's own reading agrees here — and only here does it agree by accident.
    assert_eq!(cut_hear::rate_under(&stop, 11.0), 1.0);

    // A ×2 crossing a stop comes out ×1 where they overlap: the mean of 2 and 0 is 1, so the footage runs and the
    // still steps aside for exactly those seconds.
    let crossed = [stop[0].clone(), rate_at(9.0, 4.0, 2.0)];
    assert_eq!(speed::rate_at(&crossed, 11.0), 1.0);

    // And the two readings stay apart on purpose: the preview runs flat at one covering effect's rate because a rate
    // change is a flushing seek, while the render follows the stairs.
    assert!(fx_lane::previews_flat_rate());
    assert!(fx_lane::render_follows_ramps());
    let ramped = [Fx { kind: "speed".into(), t: 0.0, dur: 20.0, rate: 8.0, trans: 8.0, tout: 0.0, ..Default::default() }];
    assert!(speed::steps_of(&ramped[0]).len() > 1, "the render has stairs to follow");
    assert_eq!(cut_hear::rate_under(&ramped, 2.0), 8.0, "the preview stays flat at the effect's own rate");
}

/// §F3.3 (`ramps are geometric staircases, each stair ≥ P.eng.rampStepSeconds (0.6) on screen, built whole or not at
/// all`): what a ramp costs, how many stairs it buys, and when it is dropped instead.
#[test]
fn f3_3_s5d_a_staircase_is_built_whole_or_not_at_all() {
    assert_eq!(speed::RAMP_STEP_SECONDS, 0.6); // P.eng.rampStepSeconds

    // One stair to ×4 costs 0.6·√4 = 1.2 s of ramp, so a 1 s fade is treated as no fade at all \u2026
    assert_eq!(speed::ramps(4.0, 20.0, 1.0, 0.0).0, 0.0);
    // \u2026 and 2.4 s is exactly enough to be one.
    assert_eq!(speed::ramps(4.0, 20.0, 2.4, 0.0).0, 2.4);

    // An eight-second ramp to ×8 affords two stairs — judged at the fastest stair's own middle, not at the top.
    assert_eq!(speed::stair_count(8.0, 1.0, 8.0), 2);

    // The stairs rise geometrically: a constant ratio between neighbours, each measured at its own middle, and every
    // one of them long enough to render.
    let steps = speed::stairs(8.0, 20.0, 8.0, 0.0);
    assert!(steps.len() >= 3, "{steps:?}");
    for pair in steps.windows(2) {
        let forward = pair[1].rate / pair[0].rate;
        assert!((forward - 1.0).abs() > 1e-9, "two stairs at the same rate are one stair: {steps:?}");
    }
    for span in &steps {
        let on_screen = (span.to - span.from) / speed::applied_rate(span.rate);
        assert!(on_screen >= speed::RAMP_STEP_SECONDS - 1e-9, "{span:?} is a stair the render would drop");
    }

    // Ramps asked to cover more than the effect holds share it in proportion \u2026
    let (inp, out) = speed::ramps(2.0, 3.0, 2.0, 2.0);
    assert!(inp > 0.0 && out > 0.0, "{inp}, {out}");
    assert!(inp + out <= 3.0 + 1e-9, "the band is the limit: {inp} + {out}");
    assert!((inp - out).abs() < 1e-9, "asked equally, shared equally");

    // \u2026 and a flat middle that would render under the floor disappears into them: 4 s at ×2 leaves 1 s of flat
    // middle, which is 0.5 s on screen — exactly the floor — so one tenth of a second off it makes the ramps fill.
    let (inp, out) = speed::ramps(2.0, 3.9, 1.5, 1.5);
    assert!((inp + out - 3.9).abs() < 1e-9, "the ramps fill the band: {inp} + {out}");

    // A staircase that cannot be built whole is not built at all: one flat stretch at the effect's own rate.
    let plain = speed::stairs(8.0, 0.6, 0.5, 0.5);
    assert_eq!(plain.len(), 1, "{plain:?}");
    assert_eq!((plain[0].from, plain[0].to, plain[0].rate), (0.0, 0.6, 8.0));

    // No ramps at all is one flat stretch too — the same answer by a different road.
    assert_eq!(speed::stairs(2.0, 5.0, 0.0, 0.0), vec![speed::Span { from: 0.0, to: 5.0, rate: 2.0 }]);
}
