// §06-effects#8-details-confirmed-against-the-code-verification-pass — spec/06-effects.md §8's four bullets, one
// test per part of them in the order §8 writes them.
//
// §8 is a verification pass: it lists the small behaviours that were checked against the prototype and are
// therefore not to be "improved" away. What these tests pin is that each is still true, and stated once — most of
// them already live in the module whose rule they bound, and this file reaches them through that module rather than
// restating a copy. Everything here is plain data: no widget, no display, no rasteriser run.

use naivepost::cut::{self, Fx};
use naivepost::cut_cards::{self as cards, BADGE, TIER};
use naivepost::cut_speed::{self as speed, STOP_SECONDS};
use naivepost::cut_suggest::{self as suggest};
use naivepost::effect_details;
use naivepost::fx_lane::MENU_ORDER;
use naivepost::fx_svg::{self, Slot};
use naivepost::tools;

/// §8 bullet 1, drawing half: the two refusals are worded as the spec writes them, the preview complains once and
/// never again, and none of it stops the effect.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s1_the_drawing_is_said_once_and_the_effect_stays(
) {
    // One file, asked about twice before anything came back: the second ask is answered "already on its way".
    let mut rasters = fx_svg::Rasters::default();
    assert_eq!(rasters.request("/x/assets/map.svg"), Slot::NeedsRaster);
    assert_eq!(
        rasters.request("/x/assets/map.svg"),
        Slot::Busy,
        "the same file is asked about once"
    );

    // It failed. The line is printed the first time and never again — a drawing that cannot be read is asked about
    // on every frame the preview draws, and a log full of one line hides everything else.
    let first = rasters.failure_line("/x/assets/map.svg", "ffmpeg drew nothing");
    assert_eq!(
        first.as_deref(),
        Some(">>> the drawing map.svg cannot be shown: ffmpeg drew nothing")
    );
    assert_eq!(
        rasters.failure_line("/x/assets/map.svg", "ffmpeg drew nothing"),
        None
    );
    assert_eq!(
        rasters.request("/x/assets/map.svg"),
        Slot::Failed,
        "and it is not asked for again"
    );

    // §8 quotes the line with §D's `>>> ` prefix, which the raster table itself spells.
    let said = rasters.failure_line("/y/assets/sting.svg", "no such file");
    assert_eq!(
        said.as_deref(),
        Some(">>> the drawing sting.svg cannot be shown: no such file")
    );

    // Neither stops the effect: `apply` asks nothing of the rasters, so a file the preview cannot show is placed
    // exactly as one it can — the render runs ffmpeg and often manages what this one-shot decode could not.
    let form = fx_svg::Form {
        t: 12.0,
        dur: 3.0,
        trans: 0.3,
        tout: 0.3,
        curve: "Linear".into(),
        file: "/x/assets/map.svg".into(),
        on: fx_svg::MIDDLE,
    };
    assert!(
        fx_svg::apply(&form).is_ok(),
        "the drawing stands though its preview copy failed"
    );
    assert!(!effect_details::preview_failure_stops_the_effect());
}

/// §8 bullet 1, still half: the stop frame's own line, spelled on the same clock as every other sentence about the
/// effect — and again, said rather than undone.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s2_the_stop_frame_is_said_and_the_effect_stays(
) {
    let line = effect_details::still_failure(125.0, "no frame at that second");
    assert!(line.starts_with(">>> the stop frame at "), "{line}");
    assert!(line.contains(" cannot be shown in the preview: "), "{line}");
    assert!(line.ends_with("no frame at that second"), "{line}");

    // The m:ss comes from the tree's one clock, so a minute and five seconds is written like every other mention.
    assert_eq!(tools::mm_ss(65.0), "01:05");
    assert_eq!(
        line,
        format!(
            ">>> the stop frame at {} cannot be shown in the preview: no frame at that second",
            tools::mm_ss(125.0)
        )
    );

    // And the stop itself is still placed: a rate-0 form of the default length applies whatever the preview made of
    // its frame. P.policy.effectDefaultSeconds / effectDefaultFades, read at a stop's values.
    let form = speed::Form {
        t: 40.0,
        dur: STOP_SECONDS,
        rate: 0.0,
        trans: speed::STOP_FADE_SECONDS,
        tout: speed::STOP_FADE_SECONDS,
        curve: "Linear".into(),
        snd: "",
    };
    let fx = speed::apply(&form);
    assert_eq!(
        fx.rate, 0.0,
        "a stop whose frame cannot be shown is still a stop: {fx:?}"
    );
    assert_eq!(fx.dur, STOP_SECONDS);
    assert!(!effect_details::preview_failure_stops_the_effect());
}

/// §8 bullet 2 (`A cut-model rate for a whole segment is a speed effect spanning it, not counted against any
/// decorations ceiling`).
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s3_a_segment_rate_is_a_speed_effect_outside_the_ceiling(
) {
    assert!(suggest::is_speed_effect(cut::EffectKind::Speed));
    for kind in MENU_ORDER {
        if kind != cut::EffectKind::Speed {
            assert!(
                !suggest::is_speed_effect(kind),
                "{kind:?} is not the kind a rate means"
            );
            assert!(
                !suggest::is_speed_effect(kind),
                "{kind:?} counts against the ceiling; only a rate does not"
            );
        }
    }
    assert!(suggest::is_speed_effect(cut::EffectKind::Speed));

    // The reading that makes it an effect: one speed spanning the segment answers that segment's seconds, which is
    // what a rate written on the segment would have had to answer.
    let mid = 5.0;
    let spanning = Fx {
        kind: "speed".into(),
        t: 0.0,
        dur: 10.0,
        rate: 2.0,
        ..Default::default()
    };
    assert_eq!(speed::rate_at(&[spanning], mid), speed::applied_rate(2.0));
    // A stop inside it holds the frame and costs its seconds at 1× — §05#8's same reading of rate 0.
    assert_eq!(speed::applied_rate(0.0), 1.0);
}

/// §8 bullet 3, first half: a stamped file is drawn again by its card, an unstamped one only gets its holes
/// filled, and a file that can take no parameters says so when asked for them.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s4_a_card_is_stamped_redrawn_or_filled_in(
) {
    let badge = cards::badge_svg("S", "top");
    let board = cards::tier_svg(&[], &[]);
    // A shipped card records the arguments it was drawn with, so a path can be merged OVER them (§8) rather than
    // replacing them: `produce_cards::changed_note` reads this string to decide whether the path changed anything.
    assert_eq!(
        cards::stamped(&badge),
        Some((BADGE, "letter=S&caption=top")),
        "{BADGE} ships with its own letter and caption recorded"
    );
    assert_eq!(cards::stamped(&board), Some((TIER, "")));

    // The two attributes §8 names are the ones the generators write on the root.
    assert_eq!(cards::STAMP, "data-naivepost");
    assert_eq!(cards::STAMP_ARGS, "data-naivepost-args");
    for doc in [&badge, &board] {
        let root = &doc[..doc.find('>').unwrap()];
        assert!(
            root.contains(cards::STAMP) && root.contains(cards::STAMP_ARGS),
            "{root}"
        );
    }

    // Unstamped: not a card of ours, whatever it says about itself elsewhere in the document.
    let plain = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
    assert_eq!(cards::stamped(plain), None);

    let args = vec![("title".to_string(), "Best maps".to_string())];
    // Stamped → redrawn by the card; the `except` half names what the path changed.
    let note = cards::arrived_at(&badge, &args);
    assert!(note.starts_with("drawn by the built-in "), "{note}");
    assert!(note.contains(BADGE), "{note}");
    assert_eq!(
        cards::drawn_by(BADGE, None),
        "drawn by the built-in badge card"
    );
    assert_eq!(
        cards::drawn_by(BADGE, Some("title was set to Best maps")),
        "drawn by the built-in badge card, except that title was set to Best maps"
    );

    // Unstamped with a hole → filled in, and nothing else.
    let holed = "<svg xmlns=\"http://www.w3.org/2000/svg\"><text>{{title}}</text></svg>";
    assert_eq!(cards::arrived_at(holed, &args), cards::FILLED_IN);
    assert_eq!(cards::FILLED_IN, "filled in");

    // Unstamped with nowhere to put them → said, verbatim from §8 including the doubled braces: a path asking for a
    // title the file has no room for is otherwise a change nobody sees.
    assert_eq!(cards::arrived_at(plain, &args), cards::IGNORED);
    assert_eq!(
        cards::IGNORED,
        "has no {{placeholders}} and was not drawn by a card, so its parameters were ignored"
    );
    // Nothing asked for, nothing to complain about.
    assert_eq!(cards::arrived_at(plain, &[]), cards::FILLED_IN);
}
