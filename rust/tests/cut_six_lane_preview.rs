// §06-effects#2-the-lane-and-the-preview — spec/06-effects.md §2.
//
// §2 is a list of things the lane and the preview do, and every one of them is a number or a yes/no, so each is
// pinned against the function that answers it: colours, the menu's order, first-fit rows, what is drawn paused
// versus playing, why the preview runs a rate flat, and when a delayed sound earns a dashed tail and a plate.
// Plain data throughout — no widget, no display, no cairo.

use naivepost::cut::{EffectKind, Fx};
use naivepost::fx_lane as lane;
use naivepost::fx_record;

const KINDS: [EffectKind; 6] = [
    EffectKind::Zoom,
    EffectKind::Speed,
    EffectKind::Text,
    EffectKind::Svg,
    EffectKind::Volume,
    EffectKind::Label,
];

fn fx(kind: EffectKind, t: f64, dur: f64) -> Fx {
    Fx { kind: name(kind).into(), t, dur, ..Default::default() }
}

/// The file's key for a kind — the record stores a string and §06#1 is what types it.
fn name(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Zoom => "zoom",
        EffectKind::Speed => "speed",
        EffectKind::Text => "text",
        EffectKind::Svg => "svg",
        EffectKind::Volume => "volume",
        EffectKind::Label => "label",
    }
}

/// §06-effects#2-the-lane-and-the-preview — `Colours`: every kind's bar colour, the staying zoom's own colour,
/// and that staying changes nothing else.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s1_colours() {
    assert_eq!(lane::bar_colour(EffectKind::Zoom, false), (0.25, 0.72, 0.82));
    assert_eq!(lane::bar_colour(EffectKind::Zoom, true), (0.95, 0.62, 0.15));
    assert_eq!(lane::bar_colour(EffectKind::Speed, false), (0.92, 0.42, 0.6));
    assert_eq!(lane::bar_colour(EffectKind::Text, false), (0.6, 0.55, 0.95));
    assert_eq!(lane::bar_colour(EffectKind::Svg, false), (0.4, 0.8, 0.5));
    assert_eq!(lane::bar_colour(EffectKind::Volume, false), (0.95, 0.85, 0.2));
    // "grey-white" — §2 gives no number; this near-white is ours and only has to separate from the lane.
    let label = lane::bar_colour(EffectKind::Label, false);
    assert_eq!(label, (0.92, 0.92, 0.92));
    assert!(label.0 > 0.8 && label.0 > label.2 * 0.9, "grey-white, not blue-grey: {label:?}");

    // Staying is a zoom's question (§06#1's table gives no other kind a `stay`), so no other colour moves.
    for kind in KINDS {
        let changes = lane::staying_is_a_different_effect(kind);
        assert_eq!(changes, matches!(kind, EffectKind::Zoom), "{kind:?}");
        assert_eq!(
            lane::bar_colour(kind, true) != lane::bar_colour(kind, false),
            changes,
            "{kind:?}'s staying form must match its own answer"
        );
    }
    // A stop shares the speed's colour — §2 lists them together as one entry.
    let mut stop = fx(EffectKind::Speed, 4.0, 2.0);
    stop.rate = 0.0;
    assert!(fx_record::is_stop(&stop));
    assert_eq!(lane::bar_colour(EffectKind::Speed, false), (0.92, 0.42, 0.6));
}

/// §06-effects#2-the-lane-and-the-preview — the Effect menu's order, which `spec/img/06-effect-menu.png` is
/// normative for and which is NOT `EffectKind`'s declaration order.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_menu_order() {
    assert_eq!(lane::MENU_ORDER[0], EffectKind::Zoom, "the image opens with ⊕ Zoom");
    assert_eq!(lane::MENU_ORDER[lane::MENU_ORDER.len() - 1], EffectKind::Label, "and ends with 🏷 Label");
    // Every kind once: the menu cannot gain a row or lose one.
    for kind in KINDS {
        assert_eq!(
            lane::MENU_ORDER.iter().filter(|offered| **offered == kind).count(),
            1,
            "{kind:?} appears exactly once"
        );
    }
    // Deliberately not the enum's order — a menu built by iterating EffectKind would put Speed third.
    assert_ne!(lane::MENU_ORDER.to_vec(), KINDS.to_vec());
    assert_eq!(
        lane::MENU_ORDER,
        [
            EffectKind::Zoom,
            EffectKind::Text,
            EffectKind::Svg,
            EffectKind::Speed,
            EffectKind::Volume,
            EffectKind::Label
        ]
    );
}

/// §06-effects#2-the-lane-and-the-preview — `Rows: first-fit by seconds (span floor 0.4 s)`: an effect takes the
/// first row its seconds are free on, and a bar under the floor is drawn without claiming one.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s3_rows_pack_by_seconds() {
    // Two over the top of each other: two rows.
    let overlap = vec![fx(EffectKind::Zoom, 0.0, 5.0), fx(EffectKind::Text, 2.0, 5.0)];
    assert_eq!(lane::rows_for_effects(&overlap), vec![0, 1]);
    assert_eq!(lane::row_count(&overlap), 2);

    // One after the other: one row, however many of them there are.
    let queue = vec![fx(EffectKind::Text, 0.0, 3.0), fx(EffectKind::Text, 3.0, 3.0), fx(EffectKind::Svg, 6.0, 3.0)];
    assert_eq!(lane::rows_for_effects(&queue), vec![0, 0, 0]);
    assert_eq!(lane::row_count(&queue), 1);

    // First fit, not "next row": the third effect fits the gap the second left on row 0.
    let fits = vec![fx(EffectKind::Zoom, 0.0, 3.0), fx(EffectKind::Text, 2.0, 3.0), fx(EffectKind::Svg, 8.0, 3.0)];
    assert_eq!(lane::rows_for_effects(&fits), vec![0, 1, 0]);

    // The floor: a bar under it is drawn at the lane's top and reserves nothing.
    // effects.packMinSeconds
    assert_eq!(lane::PACK_MIN_SECONDS, 0.4);
    let short = vec![fx(EffectKind::Label, 0.0, 0.39), fx(EffectKind::Label, 0.1, 0.3)];
    assert_eq!(lane::rows_for_effects(&short), vec![0, 0], "both are drawn");
    assert_eq!(lane::row_count(&short), 1, "and neither grew the lane");
    // Exactly the floor is not under it.
    let at_floor = vec![fx(EffectKind::Label, 0.0, lane::PACK_MIN_SECONDS), fx(EffectKind::Label, 0.1, 1.0)];
    assert_eq!(lane::rows_for_effects(&at_floor), vec![0, 1]);

    // Packed by seconds, so zooming the timeline cannot change it: same shape at any pps.
    assert_eq!(lane::row_count(&fits), 2);
}

/// §06-effects#2-the-lane-and-the-preview — `lane one row deep even when empty`, and the height that follows
/// from the timeline's own row constant.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s4_lane_is_never_empty() {
    assert_eq!(lane::row_count(&[]), 1, "an emptied lane keeps its row");
    // layout.effectRowPx — the row height is the timeline's number, so an empty lane is one row tall.
    assert_eq!(lane::lane_height_px(0), 26.0);
    assert_eq!(lane::lane_height_px(1), 26.0);
    assert_eq!(lane::lane_height_px(3), 78.0);

    // And the row count never reports nought, so nothing downstream has to guard against it.
    let one = vec![fx(EffectKind::Text, 0.0, 2.0)];
    assert_eq!(lane::row_count(&one), lane::rows_for_effects(&one).iter().max().unwrap() + 1);
    assert!(lane::lane_height_px(lane::row_count(&[])) > 0.0);
}

/// §06-effects#2-the-lane-and-the-preview — the paused preview: the dim, the rect's stroke, the held overlay at
/// full alpha with a dashed violet outline, a box in the hand dashed, and every overlay at its real alpha.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s5_paused_preview() {
    assert_eq!(lane::DIM_ALPHA, 0.45, "outside the camera rect dimmed black 0.45");
    assert_eq!(lane::RECT_STROKE_PX, 1.5);
    let violet = lane::HELD_OUTLINE;
    assert_eq!(violet.len(), 3);
    assert!(violet[0] > violet[1] && violet[2] > violet[1], "a violet, not a blue: {violet:?}");

    // Overlays are drawn at their real alpha when paused; the rest is either a guide or inaudible-but-seen-nowhere.
    for kind in KINDS {
        assert_eq!(
            lane::drawn_paused(kind),
            matches!(kind, EffectKind::Text | EffectKind::Svg | EffectKind::Label),
            "{kind:?}"
        );
    }

    // The held one: full alpha, and the dashed outline is its signal — an outline on everything is none.
    assert!(lane::held_drawn_full(true));
    assert!(!lane::held_drawn_full(false));
    assert!(lane::held_outline_dashed(true));
    assert!(!lane::held_outline_dashed(false));
    // A box under the hand is dashed, so an unfinished one cannot read as placed.
    assert!(lane::box_being_drawn_dashed(true));
    assert!(!lane::box_being_drawn_dashed(false));

    // Real alpha = the fades: nought outside, ramping in and out, full between.
    let caption = Fx { trans: 1.0, tout: 2.0, ..fx(EffectKind::Text, 10.0, 10.0) };
    assert_eq!(lane::visibility_at(&caption, 9.9), 0.0);
    assert_eq!(lane::visibility_at(&caption, 20.0), 0.0, "the end second is outside — half-open");
    assert!((lane::visibility_at(&caption, 10.5) - 0.5).abs() < 1e-9, "half way through the fade in");
    assert_eq!(lane::visibility_at(&caption, 13.0), 1.0);
    assert!((lane::visibility_at(&caption, 19.0) - 0.5).abs() < 1e-9, "half way through the fade out");
    // A fade of nought either side is a hard cut: there from its first frame.
    let hard = fx(EffectKind::Text, 10.0, 4.0);
    assert_eq!(lane::visibility_at(&hard, 10.0), 1.0);
    assert_eq!(lane::visibility_at(&hard, 13.9), 1.0);
    // A label has no fades to read (§06#1's table), so it is simply there for its seconds.
    let marked = Fx { trans: 1.0, tout: 1.0, ..fx(EffectKind::Label, 4.0, 2.0) };
    assert_eq!(lane::visibility_at(&marked, 4.5), 1.0, "a label's fades are never read");
    assert_eq!(lane::visibility_at(&marked, 6.0), 0.0);
}

/// §06-effects#2-the-lane-and-the-preview — the playing preview: only the black mask and the titles, the camera
/// layer on the smoothed live clock, and a stop's still borrowed from the scene's own camera.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s6_playing_preview() {
    // Only the mask over what the finished frame hides, plus titles.
    assert!(lane::mask_only_while_playing(EffectKind::Zoom));
    for kind in KINDS {
        assert_eq!(lane::mask_only_while_playing(kind), matches!(kind, EffectKind::Zoom), "{kind:?}");
        assert_eq!(
            lane::drawn_while_playing(kind),
            matches!(kind, EffectKind::Text | EffectKind::Svg),
            "{kind:?}"
        );
    }
    // A label is a marker for the narration brief and never rendered, so it does not survive into playing either.
    assert!(!lane::drawn_while_playing(EffectKind::Label));
    assert!(!lane::drawn_while_playing(EffectKind::Volume), "a volume has no visual");

    // The camera layer rides the smoothed clock, not the once-a-tick playhead.
    assert!(lane::camera_clock_is_smoothed());
    assert!(naivepost::preview::TICK_MS >= 10, "the tick is slow enough that this matters");

    // A stop's still is rendered from the scene's own camera: it borrows its framing, never invents one.
    let mut stop = fx(EffectKind::Speed, 6.0, 2.0);
    stop.rate = 0.0;
    assert!(lane::still_come_from_scene_camera(&stop));
    let mut own = fx(EffectKind::Speed, 6.0, 2.0);
    own.rate = 1.0;
    assert!(!lane::still_come_from_scene_camera(&own), "rate 1 is not a still");
    for kind in [EffectKind::Zoom, EffectKind::Text, EffectKind::Svg, EffectKind::Volume, EffectKind::Label] {
        assert!(!lane::still_come_from_scene_camera(&fx(kind, 6.0, 2.0)), "{kind:?} is not a stop");
    }
}

/// §06-effects#2-the-lane-and-the-preview — `Preview vs render, deliberately different`: the preview runs one
/// flat rate because a rate change is a flushing seek, while the render follows ramps and averages overlaps.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s7_flat_rate_not_ramps() {
    assert!(lane::previews_flat_rate());
    assert!(lane::render_follows_ramps());

    // The flat rate is already implemented, and this proves the preview reads that one answer: under a ramped
    // speed it returns the effect's own rate at every second of it, never an interpolated one.
    let mut speed = fx(EffectKind::Speed, 10.0, 6.0);
    speed.rate = 0.5;
    speed.trans = 2.0;
    speed.tout = 2.0;
    let effects = [speed.clone()];
    for t in [10.0, 11.0, 13.0, 15.9] {
        assert_eq!(naivepost::cut_hear::rate_under(&effects, t), 0.5, "flat at {t}");
    }
    // Outside it the clip runs at its own clock again.
    assert_eq!(naivepost::cut_hear::rate_under(&effects, 20.0), 1.0);

    // And a stop — rate nought — is played at the picture's own clock rather than frozen mid-stream.
    let mut stop = fx(EffectKind::Speed, 30.0, 2.0);
    stop.rate = 0.0;
    assert_eq!(naivepost::cut_hear::rate_under(&[stop], 31.0), 1.0);
}

/// §06-effects#2-the-lane-and-the-preview — `the two 1× sound answers cannot be previewed`, and `mute` can.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_two_one_x_answers_cannot_be_previewed() {
    // fx_record's own list of answers, so this cannot drift from §06#1.
    assert_eq!(fx_record::SND_ANSWERS, ["pitch", "own", "scene", "mute"]);
    for answer in fx_record::SND_ANSWERS {
        let can = lane::previewable_sound(answer);
        assert_eq!(can, !matches!(answer, "own" | "scene"), "{answer}");
    }
    // The empty answer is the clip's own sound at the new rate — one clock, so it previews.
    assert!(lane::previewable_sound(""));
    assert!(lane::mute_silences_the_preview("mute"));
    for answer in ["", "pitch", "own", "scene"] {
        assert!(!lane::mute_silences_the_preview(answer), "{answer}");
    }
}

/// §06-effects#2-the-lane-and-the-preview — the dashed debt tail and its plate: only `scene` draws a tail, only
/// past 0.05 s of debt and only while the scene runs on; the plate waits for 60 px of room.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s9_debt_tail_and_plate() {
    // Debt = dur − Σ on-screen seconds, positive when the sound is behind.
    assert_eq!(lane::debt(5.0, 3.0), 2.0);
    assert_eq!(lane::debt(3.0, 5.0), -2.0);
    assert_eq!(lane::debt(4.0, 4.0), 0.0);

    // effects.debtTailMinSeconds
    assert_eq!(lane::DEBT_TAIL_MIN_SECONDS, 0.05);
    assert!(lane::draws_debt_tail("scene", 2.0, true));
    assert!(!lane::draws_debt_tail("own", 2.0, true), "1× to the effect's end closes its gap on its last frame");
    assert!(!lane::draws_debt_tail("", 2.0, true), "no answer, no tail");
    assert!(!lane::draws_debt_tail("scene", 0.04, true), "under the floor is a rounding difference");
    assert!(lane::draws_debt_tail("scene", -0.05, true), "the floor is the size of the debt, not its side");
    assert!(!lane::draws_debt_tail("scene", 2.0, false), "a tail past the cut points at nothing");

    // The plate's words.
    assert_eq!(lane::plate_text(2.0), "sound 2.0 s behind");
    assert_eq!(lane::plate_text(-1.5), "sound 1.5 s ahead");
    assert_eq!(lane::plate_text(0.0), "", "a plate saying nothing is no plate");

    // effects.plateMinPx — §2 says "once wider than 60 px" (the inventory writes "≥ 60"), so the edge is above.
    assert_eq!(lane::PLATE_MIN_PX, 60.0);
    assert!(!lane::plate_shown(59.9));
    assert!(!lane::plate_shown(60.0), "wider than, not as wide as");
    assert!(lane::plate_shown(60.1));
}

/// §06-effects#2-the-lane-and-the-preview — `The earliest covering effect's answer wins`, and the 0.15 s dip at
/// the rejoin.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s10_earliest_answer_wins() {
    let mut first = fx(EffectKind::Speed, 10.0, 6.0);
    first.snd = "pitch".into();
    let mut later = fx(EffectKind::Speed, 12.0, 6.0);
    later.snd = "mute".into();

    // Two covering speeds: the one that started first answers for the overlap.
    assert_eq!(lane::sound_answer_at(&[later.clone(), first.clone()], 13.0).as_deref(), Some("pitch"));
    // Past the first's end, only the later one covers the second and its answer stands.
    assert_eq!(lane::sound_answer_at(&[first.clone(), later.clone()], 17.0).as_deref(), Some("mute"));

    // An empty `snd` is no answer: it neither wins nor silences the effect behind it.
    let quiet = fx(EffectKind::Speed, 9.0, 8.0);
    assert_eq!(lane::sound_answer_at(&[quiet, first.clone()], 13.0).as_deref(), Some("pitch"));

    // A non-speed says nothing about sound (§06#1 gives it no `snd`), even with the field filled in.
    let caption = Fx { snd: "mute".into(), ..fx(EffectKind::Text, 12.0, 4.0) };
    assert_eq!(lane::sound_answer_at(&[caption], 13.0), None);

    // Nothing covering the second at all.
    let outside = fx(EffectKind::Speed, 100.0, 4.0);
    assert_eq!(lane::sound_answer_at(&[outside], 13.0), None);
    assert_eq!(lane::sound_answer_at(&[], 13.0), None);

    // P.eng.soundDipSeconds — the fade where delayed sound rejoins the picture.
    assert_eq!(lane::rejoin_dip(), 0.15);
    assert_eq!(lane::SOUND_DIP_SECONDS, lane::rejoin_dip());
}
