// §06-effects#2-the-lane-and-the-preview — the two drawn surfaces as values: the lane's bars and the paused/playing frame.
//
// `cut_six_lane_preview.rs` pins each rule of §2 against the single function that answers it. This file pins the
// composition the view calls instead: one value for the lane, one for the paused frame, one list for the playing
// frame, and the two per-bar questions (debt tail, plate). Same numbers, reached the way the Cut page reaches them.

use naivepost::cut::{EffectKind, Fx};
use naivepost::fx_lane as lane;
use naivepost::fx_record as rec;

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

fn fx(kind: EffectKind, t: f64, dur: f64) -> Fx {
    Fx { kind: name(kind).into(), t, dur, ..Default::default() }
}

const KINDS: [EffectKind; 6] = [
    EffectKind::Zoom,
    EffectKind::Speed,
    EffectKind::Text,
    EffectKind::Svg,
    EffectKind::Volume,
    EffectKind::Label,
];

/// §06-effects#2-the-lane-and-the-preview — the lane's bars carry their row, their colour and whether they
/// claim the row for their seconds.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s10_the_lane_layout_pairs_row_colour_and_the_floor() {
    let overlap = vec![fx(EffectKind::Zoom, 0.0, 5.0), fx(EffectKind::Text, 2.0, 5.0)];
    let bars = lane::lane_layout(&overlap);
    assert_eq!(bars.len(), 2);
    assert_eq!(
        bars.iter().map(|b| b.row).collect::<Vec<_>>(),
        vec![0, 1],
        "two over the top of each other take two rows"
    );
    assert_eq!(bars[0].kind, EffectKind::Zoom);
    assert_eq!(bars[0].colour, lane::bar_colour(EffectKind::Zoom, false));
    assert_eq!(bars[1].kind, EffectKind::Text);
    assert_eq!(bars[1].colour, lane::bar_colour(EffectKind::Text, false));
    assert!(bars[0].reserves_a_row && bars[1].reserves_a_row);

    // effects.packMinSeconds 0.4 -- under the floor a bar is still drawn and reserves nothing.
    let tiny = vec![fx(EffectKind::Label, 0.0, 0.39)];
    let bars = lane::lane_layout(&tiny);
    assert_eq!(bars.len(), 1, "the short bar is still drawn");
    assert!(!bars[0].reserves_a_row, "and it claims no row");
    // Exactly the floor is not under it.
    let at_floor = vec![fx(EffectKind::Label, 0.0, lane::PACK_MIN_SECONDS)];
    assert!(lane::lane_layout(&at_floor)[0].reserves_a_row);

    // An empty lane has no bars but is still one row deep (§2: "lane one row deep even when empty").
    assert!(lane::lane_layout(&[]).is_empty());
    assert_eq!(lane::row_count(&[]), 1);
    assert_eq!(lane::lane_height_px(lane::row_count(&[])), 26.0);
}

/// §06-effects#2-the-lane-and-the-preview — the staying zoom is the orange bar, and only a zoom can be staying.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s11_a_staying_zoom_is_the_orange_bar_and_only_a_zoom_can_be() {
    let staying_zoom = Fx { stay: true, ..fx(EffectKind::Zoom, 4.0, 3.0) };
    let bars = lane::lane_layout(&[staying_zoom]);
    assert!(bars[0].staying, "// P.policy... a zoom's own `stay` flag reads through");
    assert_eq!(bars[0].colour, (0.95, 0.62, 0.15), "the staying zoom's own colour");

    // A caption with `stay` set anyway is NOT staying: §06#1's table gives no other kind a `stay` field.
    let text_with_stay = Fx { stay: true, ..fx(EffectKind::Text, 4.0, 3.0) };
    let bars = lane::lane_layout(&[text_with_stay]);
    assert!(!bars[0].staying, "a text has no stay row, so it cannot be the staying form");
    assert_eq!(bars[0].colour, lane::bar_colour(EffectKind::Text, false));
    assert!(!rec::uses(EffectKind::Text, rec::Field::Stay));

    // Two zooms in one layout, one staying: the two bars differ in colour, which is the whole point of the
    // second colour existing.
    let pair = vec![fx(EffectKind::Zoom, 0.0, 3.0), Fx { stay: true, ..fx(EffectKind::Zoom, 10.0, 3.0) }];
    let bars = lane::lane_layout(&pair);
    assert!(!bars[0].staying && bars[1].staying);
    assert_ne!(bars[0].colour, bars[1].colour);
    assert_eq!(bars[0].colour, (0.25, 0.72, 0.82));

    // And no kind other than Zoom ever changes colour on `stay`.
    for kind in KINDS {
        let with_stay = Fx { stay: true, ..fx(kind, 1.0, 2.0) };
        let bar = &lane::lane_layout(&[with_stay])[0];
        assert_eq!(bar.staying, matches!(kind, EffectKind::Zoom), "{kind:?}");
    }
}

/// §06-effects#2-the-lane-and-the-preview — the paused frame dims outside the camera rect and lists every
/// visible overlay at its real fade alpha.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s12_paused_scene_dims_and_lists_visible_overlays_at_real_alpha() {
    let caption = Fx { trans: 2.0, tout: 1.0, ..fx(EffectKind::Text, 10.0, 10.0) };
    let scene = lane::paused_scene(&[caption.clone()], 11.0, None, false);
    assert_eq!(scene.dim_alpha, 0.45, "outside the camera rect dimmed black 0.45");
    assert_eq!(scene.rect_stroke_px, 1.5);
    assert_eq!(scene.overlays.len(), 1);
    assert_eq!(scene.overlays[0].0, 0);
    assert_eq!(scene.overlays[0].1, EffectKind::Text);
    // Half way through a 2 s fade in: 0.5, straight from `visibility_at` -- the same function the view uses.
    assert!((scene.overlays[0].2 - lane::visibility_at(&caption, 11.0)).abs() < 1e-12);
    assert!((scene.overlays[0].2 - 0.5).abs() < 1e-9);

    // Invisible is still listed: the view hides by opacity rather than by removing a widget mid-refresh.
    let late = lane::paused_scene(&[caption.clone()], 25.0, None, false);
    assert_eq!(late.overlays.len(), 1, "an invisible overlay stays in the list");
    assert_eq!(late.overlays[0].2, 0.0);

    // A zoom and a volume are not overlays at all: the camera rect IS their drawing, and a volume has no visual.
    let mixed = vec![
        fx(EffectKind::Zoom, 0.0, 5.0),
        fx(EffectKind::Volume, 0.0, 5.0),
        fx(EffectKind::Text, 0.0, 5.0),
    ];
    let scene = lane::paused_scene(&mixed, 2.0, None, false);
    assert_eq!(
        scene.overlays.iter().map(|(i, _, _)| *i).collect::<Vec<_>>(),
        vec![2],
        "only the caption is an overlay"
    );
    for kind in [EffectKind::Zoom, EffectKind::Speed, EffectKind::Volume] {
        assert!(!lane::drawn_paused(kind), "{kind:?} is not an overlay");
    }
    assert!(lane::drawn_paused(EffectKind::Label), "a label is drawn paused");
}

/// §06-effects#2-the-lane-and-the-preview — the held overlay is full with the dashed violet outline; a box
/// under the hand is dashed.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s13_the_held_overlay_is_full_with_the_dashed_violet_outline() {
    let captions = vec![
        fx(EffectKind::Text, 0.0, 5.0),
        Fx { trans: 2.0, ..fx(EffectKind::Text, 10.0, 5.0) },
    ];
    let held = lane::paused_scene(&captions, 11.0, Some(1), false);
    assert!(held.held_full, "the one in hand is drawn full");
    assert!(held.held_outline_dashed, "and the dashed outline is its signal");
    assert_eq!(
        held.overlays.iter().filter(|(i, _, _)| *i == 1).count(),
        0,
        "the held overlay is not also drawn at its fade value"
    );
    assert_eq!(held.overlays.iter().map(|(i, _, _)| *i).collect::<Vec<_>>(), vec![0]);

    let nobody = lane::paused_scene(&captions, 11.0, None, false);
    assert!(!nobody.held_full);
    assert!(!nobody.held_outline_dashed);
    assert_eq!(nobody.overlays.len(), 2, "both captions draw normally");

    // A box being drawn is dashed, and nothing else about the frame changes.
    let drawing = lane::paused_scene(&captions, 11.0, None, true);
    assert!(drawing.box_dashed);
    assert_eq!(drawing.dim_alpha, nobody.dim_alpha);
    assert_eq!(drawing.overlays, nobody.overlays);
    assert!(!nobody.box_dashed);

    // The outline really is a violet, and the module owns the number.
    let violet = lane::HELD_OUTLINE;
    assert!(violet[0] > violet[1] && violet[2] > violet[1], "{violet:?}");
}

/// §06-effects#2-the-lane-and-the-preview — the playing frame is titles only.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s14_playing_scene_is_titles_only() {
    let effects = vec![
        fx(EffectKind::Zoom, 0.0, 5.0),
        fx(EffectKind::Text, 0.0, 5.0),
        fx(EffectKind::Svg, 1.0, 5.0),
        fx(EffectKind::Volume, 0.0, 5.0),
        fx(EffectKind::Label, 0.0, 5.0),
    ];
    let titles = lane::playing_scene(&effects, 2.0);
    assert_eq!(
        titles.iter().map(|(i, _, _)| *i).collect::<Vec<_>>(),
        vec![1, 2],
        "text and svg only -- a label is a marker for the brief and a volume has no visual"
    );
    assert_eq!(titles[0].1, EffectKind::Text);
    assert_eq!(titles[1].1, EffectKind::Svg);
    // Alphas come from the fades, the same reading as paused.
    assert!(
        (titles[0].2 - lane::visibility_at(&effects[1], 2.0)).abs() < 1e-12,
        "alpha straight from visibility_at"
    );

    // The camera layer is not here because while playing there is only the mask over hidden footage.
    assert!(lane::mask_only_while_playing(EffectKind::Zoom));
    for kind in KINDS {
        assert_eq!(
            lane::mask_only_while_playing(kind),
            matches!(kind, EffectKind::Zoom),
            "{kind:?}"
        );
    }
    assert!(lane::camera_clock_is_smoothed());
    assert!(lane::playing_scene(&[], 1.0).is_empty(), "no effects, no titles");
}

/// §06-effects#2-the-lane-and-the-preview — the debt tail and the plate, asked one question per bar.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s15_the_debt_tail_and_the_plate_ask_one_question_per_bar() {
    let scene_snd = Fx { snd: "scene".into(), dur: 5.0, ..fx(EffectKind::Speed, 0.0, 5.0) };
    let own_snd = Fx { snd: "own".into(), dur: 5.0, ..fx(EffectKind::Speed, 0.0, 5.0) };
    let effects = vec![scene_snd, own_snd];

    // 5 s of effect over 3 s on screen = 2 s of debt.
    assert!(lane::debt_tail_for(&effects, 0, 3.0, true), "scene's end runs on, so the tail shows");
    assert!(!lane::debt_tail_for(&effects, 1, 3.0, true), "// \"1x to the effect's end\" closes its gap on the last frame");
    assert!(!lane::debt_tail_for(&effects, 0, 3.0, false), "a tail past the cut points at nothing");
    // effects.debtTailMinSeconds 0.05 -- under the floor is a rounding difference.
    assert!(!lane::debt_tail_for(&effects, 0, 4.96, true));
    // An index off the end asks about nothing.
    assert!(!lane::debt_tail_for(&effects, 9, 3.0, true));
    assert_eq!(rec::snd_of(&effects[0]), rec::Snd::Scene);
    assert_eq!(rec::snd_of(&effects[1]), rec::Snd::Own);

    // The plate waits for room: effects.plateMinPx 60, read as "wider than".
    assert_eq!(lane::plate_for(2.0, 60.0), None);
    assert_eq!(lane::plate_for(2.0, 60.1), Some("sound 2.0 s behind".to_string()));
    assert_eq!(lane::plate_for(-1.5, 200.0), Some("sound 1.5 s ahead".to_string()));
    assert_eq!(lane::plate_for(0.0, 200.0), None, "a plate saying nothing is no plate");
    assert_eq!(lane::plate_for(2.0, 10.0), None, "no room, no plate");
}
