//! F3.6 Volume by hand — `spec/06-effects.md` F3.6, steps S1–S5, with §A.6 of `spec/inventory/effects.md`.
//!
//! A volume is the one effect with no geometry, so every assertion here is a string or a number: where its seconds
//! come from (S1), what the first record holds (S2), the form's five fields and the percent that is also a gain (S3),
//! the 0.1 s floor and playbin's ceiling (S4), and the fact that it is heard rather than seen — including while the
//! preview is paused (S5). Strings are compared whole, since a reworded refusal is a different refusal; floats go
//! through a tolerance because `0.2` and `0.1` are not representable in binary, and every expected number below is
//! derived from the rule it tests rather than read off a run. No widget and no display: `rust/src/ui/window.rs`
//! renders only Prepare, so there is no 🔊 Volume button to press yet.

use naivepost::cut::{EffectKind, Fx};
use naivepost::cut_hear;
use naivepost::cut_speed;
use naivepost::fx_lane;
use naivepost::fx_record::{self, Field};
use naivepost::fx_volume::{self, Pressed};
use naivepost::fx_zoom;
use naivepost::tools;

/// Floats from a rule, never `assert_eq!`: this is the tolerance the F3.4 round needed and did not have.
const EPS: f64 = 1e-9;

fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

/// A volume over its own seconds. `gain` is what the file stores — a linear multiplier, not the form's percent.
fn vol(gain: f64, t: f64, dur: f64, fade: f64) -> Fx {
    Fx { kind: "volume".into(), t, dur, gain, trans: fade, tout: fade, ..Default::default() }
}

/// The form as Apply would receive it: 200 % over four seconds with quarter-second ramps, i.e. what a press leaves.
fn form(percent: f64, dur: f64, trans: f64, tout: f64) -> fx_volume::Form {
    fx_volume::Form {
        t: 12.0,
        percent,
        dur,
        trans,
        tout,
        curve: fx_zoom::CURVE_CHOICES[0].to_string(),
    }
}

// --- S1: a band, a line, or neither -------------------------------------------------------------------------------

/// S1 (`selection or line..+2 s → defaults gain 200 %, ramps 0.25 s · neither → "click a track or mark a stretch
/// first — volume needs seconds to work on"`): the marked band wins, then the red line, then the refusal. The floor is
/// [`cut_speed::MIN_MARKED_SECONDS`]'s, borrowed because it belongs to every effect's press; under it a band is a click
/// that slipped and the line answers instead of refusing.
#[test]
fn f3_6_s1_a_band_or_a_line_and_neither_refuses() {
    let band = fx_volume::press(Some((30.0, 47.5)), None);
    assert_eq!(band, Pressed::Stretched { t: 30.0, dur: 17.5 });

    // A right-to-left drag is the same stretch, so the record never has to carry a negative length.
    assert_eq!(fx_volume::press(Some((47.5, 30.0)), None), band);

    // The floor itself counts as a band. 0.2 is not representable in binary, so the width is built from the bound
    // rather than written as `30.2 - 30.0`, which would be a hair under it and get thrown away.
    let (from, to) = (30.0, 30.0 + cut_speed::MIN_MARKED_SECONDS);
    assert_eq!(fx_volume::press(Some((from, to)), None), Pressed::Stretched { t: from, dur: to - from });

    // Under the floor it is not a band: with a line the line answers …
    assert_eq!(fx_volume::press(Some((30.0, 30.1)), Some(12.0)), Pressed::LoudAtLine);
    // … and without one there is nothing to be loud over.
    assert_eq!(fx_volume::press(Some((30.0, 30.1)), None), Pressed::Refused);

    // Only a line: two seconds start where the red line is.
    assert_eq!(fx_volume::press(None, Some(12.0)), Pressed::LoudAtLine);
    assert_eq!(fx_volume::press(None, Some(0.0)), Pressed::LoudAtLine, "the first second is a line like any other");

    // Neither — the refusal whole, and this button's own sentence. §F3.1 lists one per button, so volume's must not
    // be speed's with the noun swapped at print time; asserted apart or they would quietly become one string.
    assert_eq!(fx_volume::press(None, None), Pressed::Refused);
    assert_eq!(
        fx_volume::NO_SECONDS,
        "click a track or mark a stretch first \u{2014} volume needs seconds to work on"
    );
    assert_ne!(fx_volume::NO_SECONDS, cut_speed::NO_SECONDS, "a volume asks in a volume's words");
}

// --- S2: the first record ------------------------------------------------------------------------------------------

/// S1/S2 (`defaults gain 200 %, ramps 0.25 s`): what lands before the form is touched — twice as loud over two
/// seconds with quarter-second ramps, and nothing that would put a shape on the picture.
#[test]
fn f3_6_s2_the_first_record_is_twice_as_loud_with_quarter_second_ramps() {
    // P.policy.effectDefaultSeconds ("stop/speed/volume/label 2") is where LINE_SECONDS comes from; §10 has no row of
    // its own for the default gain, so that one is `effects.defaultGain` (see fx_volume::DEFAULT_GAIN).
    assert_close("a line gives two seconds", fx_volume::LINE_SECONDS, 2.0);
    assert_close("P.policy.effectDefaultFades: volume 0.25", fx_volume::FADE_SECONDS, 0.25);
    // §A.6 "defaults gain 2" — twice as loud, the ask a too-quiet lav mic actually is.
    assert_close("effects.defaultGain", fx_volume::DEFAULT_GAIN, 2.0);

    let band = fx_volume::initial(Pressed::Stretched { t: 30.0, dur: 17.5 }).expect("a band places something");
    assert_eq!(band.kind, "volume");
    assert_close("the band's own start", band.t, 30.0);
    assert_close("the band's own length", band.dur, 17.5);
    assert_close("twice as loud", band.gain, 2.0);
    assert_close("a ramp in", band.trans, 0.25);
    assert_close("a ramp out", band.tout, 0.25);

    // The line branch: same record, starting at the second someone named. `initial` cannot know the playhead, so it
    // leaves 0.0 for the thin page to replace; `initial_at_line` is the same rule with the line supplied.
    let at = fx_volume::initial_at_line(12.0);
    assert_eq!(at.kind, "volume");
    assert_close("at the line", at.t, 12.0);
    assert_close("P.policy.effectDefaultSeconds", at.dur, 2.0);
    assert_close("twice as loud", at.gain, 2.0);
    assert_close("ramps", at.trans, 0.25);
    assert_close("both ways", at.tout, 0.25);

    // No box and no lane: §A.6's "No box, no drag", and an unnamed lane is what makes it ride the whole bed.
    assert_eq!((at.cx, at.cy, at.hf, at.wf), (None, None, None, None));
    assert_eq!(at.lane, "");
    assert!(fx_record::rides_whole_bed(&at));
    // No curve chosen, so no `ease` key is written (§06#1: old files stay byte-identical).
    assert_eq!(at.ease, fx_record::LINEAR_EASE);

    // The refusal places nothing.
    assert_eq!(fx_volume::initial(Pressed::Refused), None);
}

// --- S3: the form ---------------------------------------------------------------------------------------------------

/// S3 (`Form "Volume a – b" · Volume % (0..1000) · Length (s) · Fade in ("0 is the hard step, \u{2026} audible as a
/// click") · Fade out · Curve`): the title's two seconds, the five fields in §A.6's order, and the help that says why
/// the top of the field is 1000 and why nought is allowed.
#[test]
fn f3_6_s3_the_form() {
    // Asserted through tools::mm_ss rather than spelled: mm_ss pads, so the title reads `Volume 00:12 – 00:16`.
    assert_eq!(
        fx_volume::form_title(12.0, 4.0),
        format!("Volume {} \u{2013} {}", tools::mm_ss(12.0), tools::mm_ss(16.0))
    );
    assert_eq!(fx_volume::form_title(12.0, 4.0), "Volume 00:12 \u{2013} 00:16");

    // The order is the sentence §A.6 reads, and the percent is first because it is the only answer about the sound.
    assert_eq!(
        fx_volume::FORM_FIELDS,
        ["Volume %", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"]
    );

    // The 1000 in the tooltip IS playbin's ceiling: one sentence, not a number someone must keep agreeing with
    // `P.eng.maxGain`. Read through max_percent() because a const cannot call fx_record::gain_ceiling().
    assert_close("Volume % tops out at 1000", fx_volume::max_percent(), 1000.0);
    assert_close("and that is the ceiling times 100", fx_volume::max_percent(), fx_record::gain_ceiling() * 100.0);
    assert_eq!(
        fx_volume::VOLUME_HELP,
        "up to 1000 for a passage recorded too quietly to hear \u{2014} though a passage lifted that far brings its \
         hiss up with it"
    );
    assert!(fx_volume::VOLUME_HELP.contains("1000"), "the ceiling is stated where the field is filled in");

    // §A.6's fuller wording: 100 untouched, 50 half, 0 silent, and the same hiss clause at the end.
    assert_eq!(
        fx_volume::VOLUME_HELP_FULL,
        "how loud these seconds are played, against how they were recorded. 100 is untouched, 50 half as loud, 0 \
         silent, and up to 1000 for a passage recorded too quietly to hear \u{2014} though a passage lifted that far \
         brings its hiss up with it"
    );
    assert!(fx_volume::VOLUME_HELP_FULL.ends_with(fx_volume::VOLUME_HELP), "one tooltip, the longer one");

    // Fade in: nought is explained rather than forbidden, because the ramp exists precisely because a step is a click.
    assert_eq!(
        fx_volume::FADE_IN_HELP,
        "0 is the hard step, which on a big change is audible as a click"
    );
    assert_eq!(
        fx_volume::FADE_OUT_HELP,
        "how long it takes to come back to the recorded loudness at the end, on the same terms"
    );

        // The curve list is §A.1's, owned by zoom: volume borrows it rather than keeping a copy that could drift — the
    // borrowing happens at [`fx_zoom::curve_stored`], which is what `apply` calls, and is pinned in f3_6_s4 below.
    assert_eq!(fx_zoom::CURVE_CHOICES, ["Linear"]);
}

/// S3 (`Volume %`): the field's percent and the file's linear gain are one number read two ways, so the round trip is
/// exact at every value a person can type from the tooltip — and out of range it clamps instead of refusing.
#[test]
fn f3_6_s3b_the_percent_and_the_gain_are_one_number() {
    // (percent, gain): 100 is untouched and 1000 is playbin's ceiling, so both ends of the field are meanings, not
    // merely numbers. 0.5 and 5.0 are exact in binary; 2.0/10.0 are whole.
    let table = [(0.0, 0.0), (50.0, 0.5), (100.0, 1.0), (200.0, 2.0), (1000.0, 10.0)];
    for (percent, gain) in table {
        assert_close(&format!("{percent} % as a gain"), fx_volume::gain_of_percent(percent), gain);
        assert_close(&format!("{gain} as a percent"), fx_volume::percent_of(gain), percent);
        // Round trip both directions, so neither reading is the inverse of a different rule.
        assert_close(
            &format!("{percent} % typed, stored and read back"),
            fx_volume::percent_of(fx_volume::gain_of_percent(percent)),
            percent,
        );
        assert_close(
            &format!("{gain} stored, shown and retyped"),
            fx_volume::gain_of_percent(fx_volume::percent_of(gain)),
            gain,
        );
    }

    // Outside the field: 1500 % means "as loud as this goes", not an error to explain. Clamped at both ends, and the
    // ceiling is the same one cut_hear works in.
    assert_close("nought is silence", fx_volume::gain_of_percent(-50.0), 0.0);
    assert_close("the top of the field", fx_volume::gain_of_percent(1500.0), fx_record::gain_ceiling());
    assert_close("a stored gain above the ceiling still reads 1000", fx_volume::percent_of(100.0), 1000.0);
    // The percent is what the field and the lane print, so it stays a whole number without decimals.
    let loud = vol(2.0, 12.0, 4.0, 0.25);
    assert_eq!(fx_volume::percent_label(&loud), "200%");
}

// --- S4: the floor, the ceiling and what the form is allowed to write ----------------------------------------------

/// S3/S4 (`Length \u{2265} 0.1`, `Volume % 0..1000`, fades shared by clampFades, curve stored as zoom stores it):
/// the record the form writes back — and nothing else on it.
#[test]
fn f3_6_s4_the_floor_and_the_ceiling() {
    // effects.volumeMinSeconds: the floor is taken, one tenth under it is refused naming both numbers. 0.1 is not
    // exactly representable either, so the accepted case is built from the constant rather than written as a literal.
    assert_close("effects.volumeMinSeconds", fx_volume::MIN_SECONDS, 0.1);
    assert!(fx_volume::apply(&form(200.0, fx_volume::MIN_SECONDS, 0.25, 0.25)).is_ok());
    assert_eq!(
        fx_volume::apply(&form(200.0, 0.09, 0.25, 0.25)),
        Err("a volume change of 0.09 s is shorter than the shortest one this form takes (0.1) s".to_string())
    );

    // The percent arrives as loudness and leaves as a factor; typed over the top it clamps at the ceiling instead of
    // writing a gain cut_hear would have to clamp again.
    assert_close("500 % is five times", fx_volume::apply(&form(500.0, 4.0, 0.25, 0.25)).unwrap().gain, 5.0);
    assert_close(
        "1500 % is as loud as this goes",
        fx_volume::apply(&form(1500.0, 4.0, 0.25, 0.25)).unwrap().gain,
        cut_hear::MAX_GAIN,
    );

    // Two quarter-second ramps over half a second do not fit, so §A.2's sharing decides: half the length each, i.e.
    // (0.25, 0.25) of 0.5 — the fourth form reading that one function.
    let shared = fx_volume::apply(&form(200.0, 0.5, 1.0, 1.0)).unwrap();
    assert_eq!((shared.trans, shared.tout), (0.25, 0.25));
    assert_eq!(
        (shared.trans, shared.tout),
        cut_speed::clamp_fades(1.0, 1.0, 0.5),
        "one clamp for every form"
    );

    // The curve is §A.1's storage: Linear writes no `ease` key, a name this build does not know is kept verbatim.
    assert_eq!(fx_volume::apply(&form(200.0, 4.0, 0.25, 0.25)).unwrap().ease, fx_record::LINEAR_EASE);
    assert_eq!(fx_volume::apply(&form(200.0, 4.0, 0.25, 0.25)).unwrap().ease, "");
    let glide = fx_volume::Form { curve: "Glide".into(), ..form(200.0, 4.0, 0.25, 0.25) };
    assert_eq!(fx_volume::apply(&glide).unwrap().ease, "Glide");

    // A volume writes nothing else on the record: no words (§1's table marks `text` "–" for it), no file, no camera
    // row, and no lane so that it rides the whole bed.
    let fx = fx_volume::apply(&form(200.0, 4.0, 0.25, 0.25)).unwrap();
    assert_eq!((fx.text.as_str(), fx.src.as_str(), fx.lane.as_str()), ("", "", ""));
    assert_eq!((fx.cx, fx.cy, fx.hf, fx.wf), (None, None, None, None));
    assert_eq!(fx.snd, "");
    assert_eq!(fx.cam, 0);
    assert_eq!((fx.t, fx.dur), (12.0, 4.0), "the form's own seconds pass through");
}

// --- S5: heard, not seen ----------------------------------------------------------------------------------------------

/// S5 (`applied in the preview even while paused`) + §F3.6's `overlapping gains multiply`: the loudness of a second is
/// a function of the second. Two volumes are two things done to the same sound, so they multiply — where two rates
/// average ([`cut_hear::gain_under`]'s doc says exactly that), and where one gain reaches playbin's ceiling a further
/// one has nowhere left to go.
#[test]
fn f3_6_s5_overlapping_gains_multiply() {
    let loud = vol(2.0, 12.0, 4.0, 0.25);

    // Mid-band the envelope is 1, so the gain is what it says: trans = tout = 0.25 around t = 14 well inside 4 s.
    assert_close("one volume doubles it", cut_hear::gain_under(&[loud.clone()], 14.0), 2.0);
    // Two of them are two things done to the same sound: 2 times 2, not the average of 2 and 2.
    assert_close("two overlapping gains multiply", cut_hear::gain_under(&[loud.clone(), loud.clone()], 14.0), 4.0);
    // Nought is silence and stays silence however it is reached; fx_record's own reading agrees.
    let hush = vol(0.0, 12.0, 4.0, 0.25);
    assert_close("a volume at nought is silence", cut_hear::gain_under(&[hush.clone()], 14.0), 0.0);
    assert!(fx_record::gain_is_silence(&hush));
    assert!(!fx_record::gain_is_silence(&loud));
    // A volume at 100 % changes nothing, which is the same number as no volume at all.
    assert_close("100 % is untouched", cut_hear::gain_under(&[vol(1.0, 12.0, 4.0, 0.25)], 14.0), 1.0);
    // The ceiling caps the product, so a fourth one after it buys nothing — that is why the field tops out at 1000 %.
    assert_close("playbin's ceiling", cut_hear::gain_under(&[vol(100.0, 12.0, 4.0, 0.25)], 14.0), 10.0);
    assert!(
        cut_hear::gain_under(&[vol(100.0, 12.0, 4.0, 0.25), vol(4.0, 12.0, 4.0, 0.25)], 14.0) <= cut_hear::MAX_GAIN
    );

    // Mid-ramp: with a one-second ramp at half a second in the envelope is 0.5, so gain 2 reads halfway from 1 —
    // 1 + (2 - 1) × 0.5 = 1.5. The fades are why a volume is never a step unless asked to be.
    let ramped = vol(2.0, 12.0, 4.0, 1.0);
    assert_close("halfway up the ramp", cut_hear::gain_under(&[ramped.clone()], 12.5), 1.5);

    // Paused or playing: §A.6's "Preview gain applies even paused". Same function, so the same number at a second
    // where the envelope is not even 1 — which is the point, since the fades can only be judged without a moving frame.
    assert_close("heard paused mid-band", fx_volume::heard_while_paused(&[loud.clone()], 14.0), 2.0);
    assert_eq!(fx_volume::heard_while_paused(&[loud.clone()], 14.0), cut_hear::gain_under(&[loud.clone()], 14.0));
    assert_close("heard paused mid-ramp", fx_volume::heard_while_paused(&[ramped.clone()], 12.5), 1.5);
    assert_eq!(
        fx_volume::heard_while_paused(&[ramped.clone()], 12.5),
        cut_hear::gain_under(&[ramped], 12.5)
    );

    // And nothing to see: §A.6's "No box, no drag" is a statement about the picture, so while paused there is by
    // definition no change on screen — §06#2's rule that only kinds with a visual differ there.
    assert!(fx_volume::gain_has_no_visual());
    assert!(!fx_lane::drawn_paused(EffectKind::Volume));
}

/// S5 (`status "\u{2026} \u2014 the picture is untouched"`) + §A.6's `label the percentage`: what the page says
/// afterwards, in the unit the form asked in.
#[test]
fn f3_6_s5b_the_status_says_the_picture_is_untouched() {
    let loud = vol(2.0, 12.0, 4.0, 0.25);

    // The three verbs are §A.6's own readings of the field: nought is silence, under 100 % quieter, from 100 % up
    // louder. A volume at exactly 100 % does nothing and is still meant as a raising of the voice.
    assert_eq!(fx_volume::verb(2.0), "louder");
    assert_eq!(fx_volume::verb(1.0), "louder");
    assert_eq!(fx_volume::verb(0.5), "quieter");
    assert_eq!(fx_volume::verb(0.0), "silent");

    // Percent, not a factor: "200 % is a loudness and 2 is arithmetic homework".
    assert_eq!(fx_volume::percent_label(&loud), "200%");
    assert_eq!(fx_volume::percent_label(&vol(0.5, 12.0, 4.0, 0.25)), "50%");
    assert_eq!(fx_volume::percent_label(&vol(0.0, 12.0, 4.0, 0.25)), "0%");

    let label = fx_volume::label(&loud);
    assert_eq!(label, "00:12 louder at 200% for 4.0s");
    for part in ["louder", "200%", "4.0s"] {
        assert!(label.contains(part), "{label} is missing {part}");
    }

    // The status whole, including §F3.6's em dash — and no `↶ Undo takes it back` tail, which belongs to speed's §A.3
    // wording rather than to this flow's quote.
    assert_eq!(fx_volume::placed_status(&loud), format!("{label} \u{2014} the picture is untouched"));
    assert_eq!(
        fx_volume::placed_status(&loud),
        "00:12 louder at 200% for 4.0s \u{2014} the picture is untouched"
    );
    assert!(!fx_volume::placed_status(&loud).contains("Undo"), "that suffix is §A.3's, not §F3.6's");

    // A silenced passage and a quiet one read as what they are; the lane and the status line are the same string, so
    // one cannot promise a loudness the other denies.
    assert!(fx_volume::placed_status(&vol(0.0, 12.0, 4.0, 0.25)).contains("silent at 0%"));
    assert!(fx_volume::placed_status(&vol(0.5, 12.0, 4.0, 0.25)).contains("quieter at 50%"));
}

// --- §A.6: no box, no drag, and the whole bed ------------------------------------------------------------------------

/// §A.6 (`No box, no drag` / `lane (new): "" = the whole bed`) + §1's table marking `cx cy hf wf` "–" for volume:
/// which parts of a record a volume owns, that it rides everything unless named for one lane, and the colour its bar
/// gets on the effects lane.
#[test]
fn f3_6_s6_no_box_no_drag_and_the_whole_bed() {
    // The table read at volume's row: loudness and length yes, a box no — that IS "no box, no drag" in code, since
    // there is no field for a drag to write.
    assert!(fx_record::uses(EffectKind::Volume, Field::Gain));
    assert!(fx_record::uses(EffectKind::Volume, Field::Dur));
    assert!(!fx_record::uses(EffectKind::Volume, Field::Cx));
    assert!(!fx_record::uses(EffectKind::Volume, Field::Cy));
    assert!(!fx_record::uses(EffectKind::Volume, Field::Hf));
    assert!(!fx_record::uses(EffectKind::Volume, Field::Wf));

    // An unnamed lane is the whole bed; naming one is how a single track gets raised instead of everything.
    let bed = vol(2.0, 12.0, 4.0, 0.25);
    assert_eq!(bed.lane, "");
    assert!(fx_record::rides_whole_bed(&bed));
    let one_lane = Fx { kind: "volume".into(), lane: "mic1".into(), ..Default::default() };
    assert!(!fx_record::rides_whole_bed(&one_lane));

    // Silence is a stored nought, and the field's nought stores exactly that.
    assert!(fx_record::gain_is_silence(&vol(0.0, 12.0, 4.0, 0.25)));
    assert_close("the field's nought", fx_volume::gain_of_percent(0.0), 0.0);

    // One ceiling for the form, the preview and the render: P.eng.maxGain, named once in cut_hear.
    assert_close("P.eng.maxGain", fx_record::gain_ceiling(), 10.0);
    assert_eq!(fx_record::gain_ceiling(), cut_hear::MAX_GAIN);

    // The lane's colour: amber, i.e. audio's own family — the same pair of floats §D gives it, compared as numbers so
    // a re-tuned colour fails here rather than in someone's screenshot.
    let (r, g, b) = fx_lane::bar_colour(EffectKind::Volume, false);
    assert_close("amber red", r, 0.95);
    assert_close("amber green", g, 0.85);
    assert_close("amber blue", b, 0.2);
}
