// §06-effects#8-details-confirmed-against-the-code-verification-pass — spec/06-effects.md §8's four bullets, one
// test per part of them in the order §8 writes them.
//
// §8 is a verification pass: it lists the small behaviours that were checked against the prototype and are
// therefore not to be "improved" away. What these tests pin is that each is still true, and stated once — most of
// them already live in the module whose rule they bound, and this file reaches them through that module rather than
// restating a copy. Everything here is plain data: no widget, no display, no rasteriser run.

use naivepost::cut::{self, Fx};
use naivepost::cut_cards::{self as cards, BADGE, TIER};
use naivepost::cut_insert::{self as ins, Kind};
use naivepost::cut_speed::{self as speed, STOP_SECONDS};
use naivepost::cut_suggest::{self as suggest};
use naivepost::effect_details;
use naivepost::fx_lane::{self, MENU_ORDER};
use naivepost::fx_svg::{self, Slot};
use naivepost::fx_text;
use naivepost::tools;

/// §8 bullet 1, drawing half: the two refusals are worded as the spec writes them, the preview complains once and
/// never again, and none of it stops the effect.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s1_the_drawing_is_said_once_and_the_effect_stays() {
    assert_eq!(
        fx_svg::NO_LINE,
        "click a track first \u{2014} the drawing needs a moment to appear at"
    );
    assert_eq!(fx_svg::NO_FILE, "choose a drawing and it goes on the picture");

    // One file, asked about twice before anything came back: the second ask is answered "already on its way".
    let mut rasters = fx_svg::Rasters::default();
    assert_eq!(rasters.request("/x/assets/map.svg"), Slot::NeedsRaster);
    assert_eq!(rasters.request("/x/assets/map.svg"), Slot::Busy, "the same file is asked about once");

    // It failed. The line is printed the first time and never again — a drawing that cannot be read is asked about
    // on every frame the preview draws, and a log full of one line hides everything else.
    let first = rasters.failure_line("/x/assets/map.svg", "ffmpeg drew nothing");
    assert_eq!(first.as_deref(), Some("the drawing map.svg cannot be shown: ffmpeg drew nothing"));
    assert_eq!(rasters.failure_line("/x/assets/map.svg", "ffmpeg drew nothing"), None);
    assert_eq!(rasters.request("/x/assets/map.svg"), Slot::Failed, "and it is not asked for again");

    // §8 quotes the line with §D's `>>> ` prefix; the body above is what a caller hands to `log`.
    let said = rasters.failure_line("/y/assets/sting.svg", "no such file");
    assert_eq!(
        effect_details::log(&said.expect("a first failure is said")),
        ">>> the drawing sting.svg cannot be shown: no such file"
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
    assert!(fx_svg::apply(&form).is_ok(), "the drawing stands though its preview copy failed");    assert!(!effect_details::preview_failure_stops_the_effect());
}

/// §8 bullet 1, still half: the stop frame's own line, spelled on the same clock as every other sentence about the
/// effect — and again, said rather than undone.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s2_the_stop_frame_is_said_and_the_effect_stays() {
    let line = effect_details::still_failure(125.0, "no frame at that second");
    assert!(line.starts_with(">>> the stop frame at "), "{line}");
    assert!(line.contains(" cannot be shown in the preview: "), "{line}");
    assert!(line.ends_with("no frame at that second"), "{line}");

    // The m:ss comes from the tree's one clock, so a minute and five seconds is written like every other mention.
    assert_eq!(tools::mm_ss(65.0), "01:05");
    assert_eq!(
        line,
        format!(">>> the stop frame at {} cannot be shown in the preview: no frame at that second", tools::mm_ss(125.0))
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
    assert_eq!(fx.rate, 0.0, "a stop whose frame cannot be shown is still a stop: {fx:?}");
    assert_eq!(fx.dur, STOP_SECONDS);
    assert!(!effect_details::preview_failure_stops_the_effect());
}

/// §8 bullet 2 (`A cut-model rate for a whole segment is a speed effect spanning it, not counted against any
/// decorations ceiling`).
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s3_a_segment_rate_is_a_speed_effect_outside_the_ceiling() {
    assert!(suggest::is_speed_effect(cut::EffectKind::Speed));
    for kind in MENU_ORDER {
        if kind != cut::EffectKind::Speed {
            assert!(!suggest::is_speed_effect(kind), "{kind:?} is not the kind a rate means");
            assert!(
                !suggest::exempt_from_decorations_cap(kind),
                "{kind:?} counts against the ceiling; only a rate does not"
            );
        }
    }
    assert!(suggest::exempt_from_decorations_cap(cut::EffectKind::Speed));

    // The reading that makes it an effect: one speed spanning the segment answers that segment's seconds, which is
    // what a rate written on the segment would have had to answer.
    let mid = 5.0;
    let spanning = Fx { kind: "speed".into(), t: 0.0, dur: 10.0, rate: 2.0, ..Default::default() };
    assert_eq!(speed::rate_at(&[spanning], mid), speed::applied_rate(2.0));
    // A stop inside it holds the frame and costs its seconds at 1× — §05#8's same reading of rate 0.
    assert_eq!(speed::applied_rate(0.0), 1.0);
}

/// §8 bullet 3, first half: a stamped file is drawn again by its card, an unstamped one only gets its holes
/// filled, and a file that can take no parameters says so when asked for them.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s4_a_card_is_stamped_redrawn_or_filled_in() {
    let badge = cards::badge_svg("S", "top");
    let board = cards::tier_svg(&[], &[]);
    assert_eq!(cards::stamped(&badge), Some((BADGE, "")), "{BADGE} ships with no arguments");
    assert_eq!(cards::stamped(&board), Some((TIER, "")));

    // The two attributes §8 names are the ones the generators write on the root.
    assert_eq!(cards::STAMP, "data-naivepost");
    assert_eq!(cards::STAMP_ARGS, "data-naivepost-args");
    for doc in [&badge, &board] {
        let root = &doc[..doc.find('>').unwrap()];
        assert!(root.contains(cards::STAMP) && root.contains(cards::STAMP_ARGS), "{root}");
    }

    // Unstamped: not a card of ours, whatever it says about itself elsewhere in the document.
    let plain = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>";
    assert_eq!(cards::stamped(plain), None);

    let args = vec![("title".to_string(), "Best maps".to_string())];
    // Stamped → redrawn by the card; the `except` half names what the path changed.
    let note = cards::arrived_at(&badge, &args);
    assert!(note.starts_with("drawn by the built-in "), "{note}");
    assert!(note.contains(BADGE), "{note}");
    assert_eq!(cards::drawn_by(BADGE, None), "drawn by the built-in badge card");
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

/// §8 bullet 3, the contract (`The CARDS.md contract is normative`): what a card file may and may not reach out
/// for, stated in the guide every project is handed.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s5_the_cards_md_contract_is_the_norm() {
    // Unchanged: the insert form still comes out of `<!-- Input: key[flags] | Label | hint -->`.
    assert!(ins::CARD_GUIDE.contains("<!-- Input: title | Title |"));
    assert!(ins::CARD_GUIDE.contains("[logo]"));

    // Self-contained: images travel as data URLs, and the guide says why — the render reads baked frames out of a
    // temporary folder, where a relative href resolves against nothing and an absolute one is refused.
    assert!(ins::CARD_GUIDE.contains("data:") && ins::CARD_GUIDE.contains("href"), "{}", "the guide names data: URLs");
    assert!(ins::CARD_GUIDE.contains("refused"));
    // Machine fonts, as a family list, and no webfont.
    assert!(ins::CARD_GUIDE.contains("@font-face"));
    assert!(ins::CARD_GUIDE.contains("DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif"));
    // The sizing estimate is the fitter's own number, so guide and code cannot drift apart.
    let em = format!("{}em", fx_text::CHAR_ADVANCE_EM);
    assert!(ins::CARD_GUIDE.contains(&em), "{} missing from the guide", em);

    // The check itself: only a `data:` URL (or no image at all) travels. An absolute path or a URL is reported —
    // the renderer has been told not to go and find it. A relative href is NOT this check's business: §8 splits the
    // two failures, "resolves no relative href, refuses an absolute one", and a relative one resolves against the
    // temporary folder rather than being refused outright.
    assert_eq!(cards::absolute_href("<svg><image href=\"data:image/png;base64,AAAA\"/></svg>"), None);
    assert_eq!(cards::absolute_href("<svg><image href=\"\"/></svg>"), None);
    assert_eq!(cards::absolute_href("<svg><image href=\"/abs/x.png\"/></svg>").as_deref(), Some("/abs/x.png"));
    assert_eq!(
        cards::absolute_href("<svg><image href=\"https://x/y.png\"/></svg>").as_deref(),
        Some("https://x/y.png")
    );
    // A relative href is reported too: §8 says the render folder "resolves no relative href", which is a failure to
    // draw rather than a refusal, but still a picture that will not arrive.
    assert_eq!(cards::absolute_href("<svg><image href=\"x.png\"/></svg>").as_deref(), Some("x.png"));

    // Fonts.
    assert!(!cards::uses_font_face("<svg><style>text{font-family:sans}</style></svg>"));
    assert!(cards::uses_font_face("<svg><style>@font-face{src:url(a.woff)}</style></svg>"));

    // And both shipped cards obey all three: the render's frame size, an opaque background, no font pulled in, no
    // image reaching outside the document.
    assert!(cards::is_frame_size(cards::CANVAS));
    assert!(!cards::BACKGROUND.is_empty(), "the canvas is painted, not transparent");
    for doc in [cards::badge_svg("A", "top").as_str(), cards::tier_svg(&[], &[]).as_str()] {
        assert!(!cards::uses_font_face(doc));
        assert_eq!(cards::absolute_href(doc), None);
    }
}

/// §8 bullets 3 and 4's arithmetic: the seeds in their stated order and written once, a card with no end taking the
/// default length, one static SVG per baked frame, a CSS animation with no `@keyframes` being a still, and the 1×
/// read head — which opens only where there is debt worth a line and something filmed to read back.
#[test]
fn sec_06_effects_8_details_confirmed_against_the_code_verification_pass_s6_seeds_bake_stillness_and_the_read_head() {
    // Seeds: badges first, then the board (whose logos are those badge files), then the guide — §8's order.
    let names: Vec<&str> = cards::seeds().iter().map(|(name, _)| *name).collect();
    assert_eq!(&names[..6], ["s.svg", "a.svg", "b.svg", "c.svg", "d.svg", "f.svg"]);
    assert_eq!(names[6], "tier.svg");
    assert_eq!(names[7], ins::CARD_GUIDE_FILE);

    // Written on first use, nothing the second time: a board someone restyled is theirs.
    let dir = std::env::temp_dir().join(format!("naivepost-cards-8-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let seeds = cards::seeds();
    assert!(!ins::seed_cards(&dir, &seeds).is_empty(), "the first open writes them");
    let restyled = "<svg>mine</svg>";
    std::fs::write(dir.join("tier.svg"), restyled).expect("a card is writable");
    assert!(ins::seed_cards(&dir, &seeds).is_empty(), "nothing already there is replaced");
    assert_eq!(std::fs::read_to_string(dir.join("tier.svg")).unwrap(), restyled);
    let _ = std::fs::remove_dir_all(&dir);

    // An indefinitely repeating document has no own length — it takes the default insert length (P.policy.
    // insertDefaultSeconds) and loops for its slot, so `card_length` says nothing rather than guessing a number.
    let anim = |begin: f64, dur: f64, repeat: f64| cards::Anim {
        attr: "opacity".into(),
        values: vec!["0".into(), "1".into()],
        key_times: vec![0.0, 1.0],
        begin,
        dur,
        repeat,
        freeze: true,
        additive: false,
    };
    assert_eq!(cards::card_length(&[anim(0.5, 1.0, f64::INFINITY)]), None);
    assert_eq!(cards::card_length(&[]), None, "nothing to measure");
    assert_eq!(cards::card_length(&[anim(0.5, 1.0, 3.0)]), Some(0.5 + 1.0 * 3.0));
    assert_eq!(ins::DEFAULT_SECONDS, 4.0);

    // The bake: one static SVG per frame of the card at the render's fps (`f%05d.svg`, named by its caller).
    assert_eq!(cards::bake_times(2.0).len(), (2.0 * cards::BAKE_FPS) as usize);
    assert!(cards::bake_times(0.0).is_empty());

    // A CSS animation with no `@keyframes` is drawn as a still, so it is a still for the preview too: one frame, not
    // eight a second. The contrast is a document that does carry its keyframes.
    assert!(matches!(ins::kind("card.svg"), Kind::Svg));
    let css_only = "<svg><style>#a{animation:m 2s}</style></svg>";
    let with_keyframes = "<svg><style>@keyframes m {from{opacity:0}}</style></svg>";
    assert_eq!(css_only.contains("@keyframes"), false);
    assert!(with_keyframes.contains("@keyframes"));
    assert_eq!(ins::preview_fps(Kind::Svg, false), None, "a still is one texture");
    assert_eq!(ins::preview_fps(Kind::Svg, true), Some(ins::PREVIEW_FPS));

    // The read head (effects.debtTailMinSeconds — §8's 0.05 has no `P.` row; P.eng.soundDipSeconds for the dip).
    let floor = fx_lane::DEBT_TAIL_MIN_SECONDS;
    assert!(effect_details::read_head_open("scene", floor, true, true, false));
    assert!(!effect_details::read_head_open("scene", floor - 1e-6, true, true, false), "a rounding difference");
    for snd in ["own", "pitch", "mute", ""] {
        assert!(!effect_details::read_head_open(snd, 5.0, true, true, false), "{snd:?} closes on the effect");
    }
    assert!(!effect_details::read_head_open("scene", 5.0, false, true, false), "the scene is over");
    assert!(!effect_details::read_head_open("scene", 5.0, true, false, false), "a clip on no recording");
    assert!(!effect_details::read_head_open("scene", 5.0, true, true, true), "a card or held frame");

    // Half the dip on each side of the join — and `rejoin_dip` stays the whole crossfade.
    assert_eq!(effect_details::dip_each_side(), fx_lane::SOUND_DIP_SECONDS / 2.0);
    assert_eq!(fx_lane::rejoin_dip(), 0.15);
}
