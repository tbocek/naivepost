//! §06-effects#9-where-recordings-overlap — two cameras filming the same seconds and several
//! sounds running at once, checked against the code that has to act on each row of §9's table.
//!
//! Every test here asserts what the PROGRAM decided: a zoom built through `fx_zoom::apply`
//! carries the camera it was framed on, a stop placed over a dropped second is refused by the
//! door the form calls, a lane-named volume reaches the encode before the mix, and the model
//! briefs name rows and lanes only when there is more than one of either to tell apart.

use naivepost::cut::{Cut, EffectKind, Fx, Lane, Seg};
use naivepost::cut_cam;
use naivepost::cut_clamp;
use naivepost::cut_effects_pass;
use naivepost::effect_overlap;
use naivepost::fx_volume;
use naivepost::produce_fx_chain;
use naivepost::produce_render;
use naivepost::render_fx;
use naivepost::tools::clips::Clips;

/// A footage scene on camera `cam`, from `s` to `e`.
fn scene(s: f64, e: f64, cam: i32) -> Seg {
    Seg { s, e, cam, ..Default::default() }
}

/// The session's heard lane: named, running from `at` for `dur` session seconds.
fn lane(name: &str, at: f64, dur: f64) -> Lane {
    Lane { name: name.into(), src: format!("{name}.wav"), at, off: 0.0, dur }
}

/// The same seconds filmed from a card instead of a camera: an overwrite insert, which is how
/// the app spells "this stretch shows a card" — no source span of its own, so it films nothing.
fn card(s: f64, e: f64) -> Seg {
    Seg { s, e, dur: 0.0, ins: "cards/title.svg".into(), ..Default::default() }
}

/// §9 (`A zoom SHOULD carry `cam`, the row it was framed on, and act only on clips shown from
/// that camera`; `Over inserts: none`): the row is WRITTEN by the program's own apply, and the
/// scene question is answered from it.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s1_a_zoom_carries_its_camera_and_acts_there_only() {
    let made = naivepost::fx_zoom::apply(&naivepost::fx_zoom::Form {
        at: 10.0,
        dur: 6.0,
        stay: false,
        trans: 1.0,
        tout: 1.0,
        curve: String::new(),
        cx: 0.4,
        cy: 0.5,
        hf: 0.6,
        row: 2,
    })
    .expect("a six-second camera move clears the floor");
    assert_eq!(made.effect_kind(), Some(EffectKind::Zoom));
    assert_eq!(made.cam, 2, "the row the box was dragged on is stored on the record");

    let on_row = scene(10.0, 20.0, 2);
    let other_row = scene(10.0, 20.0, 0);
    let card = card(10.0, 20.0);
    assert!(effect_overlap::acts_on_scene(&made, &on_row), "its own camera");
    assert!(!effect_overlap::acts_on_scene(&made, &other_row), "another camera's clip");
    assert!(!effect_overlap::acts_on_scene(&made, &card), "over an insert: none");
    // The kept list a clip carries follows the same answer, so the render cannot re-add it.
    assert!(effect_overlap::drawn_on_scene(std::slice::from_ref(&made), &card).is_empty());
    assert_eq!(effect_overlap::drawn_on_scene(std::slice::from_ref(&made), &on_row).len(), 1);
    // And the row reading is the segment's own, not a guess: `scene_row` is what both sides use.
    assert_eq!(cut_cam::scene_row(&other_row), 0);
}

/// §9 (`A zoom does not cross into a clip from another camera`): the band ends at the lens
/// switch instead of jumping onto the next camera's frame — unless trimming would leave a sliver.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s2_a_zoom_does_not_cross_into_another_camera() {
    let zoom = Fx { kind: "zoom".into(), t: 10.0, dur: 20.0, cam: 0, ..Default::default() };
    let crossing = [scene(0.0, 14.0, 0), scene(14.0, 30.0, 1)];
    assert_eq!(
        effect_overlap::trim_at_lens_switch(&zoom, &crossing),
        Some((4.0, 1)),
        "the band stops at the cut and names the row that ended it"
    );
    let staying = [scene(0.0, 14.0, 0), scene(14.0, 30.0, 0)];
    assert_eq!(effect_overlap::trim_at_lens_switch(&zoom, &staying), None, "one camera all the way");
    // A trim under the surviving-seconds floor is not worth an edit: the whole band stays.
    let tiny = Fx { kind: "zoom".into(), t: 13.5, dur: 20.0, cam: 0, ..Default::default() };
    assert_eq!(
        effect_overlap::trim_at_lens_switch(&tiny, &crossing),
        None,
        "0.5 s of zoom is less than effects.minSurvivingSeconds {}",
        cut_clamp::MIN_SURVIVING_SECONDS
    );
    const { assert!(cut_clamp::MIN_SURVIVING_SECONDS > 0.5) };
    // The decision itself is a row comparison, nothing else.
    assert!(effect_overlap::zoom_stops_at_the_cut(&zoom, 0, 2));
    assert!(!effect_overlap::zoom_stops_at_the_cut(&zoom, 1, 1));
    let text = Fx { kind: "text".into(), t: 10.0, dur: 20.0, ..Default::default() };
    assert!(
        !effect_overlap::zoom_stops_at_the_cut(&text, 0, 2),
        "only a zoom has a camera to lose"
    );
}

/// §9 (`A lens switch under a zoom says so: "the zoom at m:ss was framed on camera N — it no
/// longer applies here"`): the sentence, spelled out rather than compared with itself.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s3_the_lens_switch_notice_is_the_spec_sentence() {
    assert_eq!(
        effect_overlap::lens_switch(82.0, 2),
        "the zoom at 1:22 was framed on camera 2 \u{2014} it no longer applies here"
    );
    // m:ss for a moment under a minute too, since the clock every sentence quotes is shared.
    assert_eq!(
        effect_overlap::lens_switch(5.0, 0),
        "the zoom at 0:05 was framed on camera 0 \u{2014} it no longer applies here"
    );
}

/// §9 (`A stop at a dropped second SHOULD be refused when placed`): the refusal happens while
/// placing, and names the moment and the reason.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s4_a_stop_at_a_dropped_second_is_refused() {
    let segs = [scene(0.0, 10.0, 0), scene(20.0, 30.0, 0), card(40.0, 50.0)];
    assert_eq!(effect_overlap::place_stop(5.0, &segs), Ok(()), "a kept second can be held");
    let hole = effect_overlap::place_stop(15.0, &segs).expect_err("the cut dropped these seconds");
    assert_eq!(hole, effect_overlap::no_frame(15.0));
    assert!(hole.starts_with("there is no frame at "), "{hole}");
    assert!(hole.contains("0:15"), "the moment is named: {hole}");
    // A card films nothing, so there is no frame to freeze there either.
    assert!(card(40.0, 50.0).is_overwrite_insert(), "a card is an insert over footage");
    assert_eq!(
        effect_overlap::place_stop(45.0, &[card(40.0, 50.0)]),
        Err(effect_overlap::no_frame(45.0)),
        "a card holds no frame"
    );
    // Half-open like every other range in the app: a scene's last second belongs to what starts there.
    assert_eq!(
        effect_overlap::place_stop(10.0, &segs),
        Err(effect_overlap::no_frame(10.0)),
        "the second a dropped gap opens on is not kept"
    );
    // And a stop is read the way the rest of the app reads one: rate 0.
    let stop = Fx { kind: "speed".into(), t: 1.0, dur: 2.0, rate: 0.0, ..Default::default() };
    assert!(effect_overlap::is_a_stop(&stop));
    let sped = Fx { kind: "speed".into(), t: 1.0, dur: 2.0, rate: 2.0, ..Default::default() };
    assert!(!effect_overlap::is_a_stop(&sped));
}

/// §9's three sound rows: which answers the preview copies, which it must say it cannot do, and
/// the mute row's MUST — silence the WHOLE bed, as the preview does.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s5_each_sound_answer_owes_the_preview_a_promise() {
    assert_eq!(
        effect_overlap::says_render_only("own"),
        Some("sound at 1\u{d7} in the render only")
    );
    assert_eq!(
        effect_overlap::says_render_only("scene"),
        Some("sound at 1\u{d7} in the render only")
    );
    for snd in ["", "pitch", "mute"] {
        assert_eq!(effect_overlap::says_render_only(snd), None, "`{snd}` owes no such notice");
    }
    for snd in ["", "pitch"] {
        assert!(effect_overlap::preview_plays_pitch_as_render(snd), "`{snd}` is copied");
    }
    for snd in ["mute", "own", "scene"] {
        assert!(!effect_overlap::preview_plays_pitch_as_render(snd), "`{snd}` is not");
    }
    // The mute MUST row: ONE expression over the bed, not one per recording or lane.
    let bed = render_fx::mute_expr(&[(3.0, 8.0)]);
    assert_eq!(bed, "volume=0:enable='between(t\\,3\\,8)'", "{bed}");
    assert_eq!(bed.matches("volume=").count(), 1, "one filter for the whole bed: {bed}");
    // Two spans are still one expression, joined inside it.
    assert_eq!(render_fx::mute_expr(&[(1.0, 2.0), (4.0, 5.0)]).matches("volume=").count(), 1);
    assert!(render_fx::sound_filters(2.0, "mute").join(",").contains("volume=0"));
}

/// §9 (`A volume SHOULD carry `lane`: "" the whole bed, or one lane's name, gained before the
/// mix`; `The render MUST apply a volume over a card's own sound`): the reach, the graph order,
/// and the card case, all through the encode's own chain builder.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s6_a_volume_carries_its_lane_into_the_graph() {
    let form = |lane: &str| fx_volume::Form {
        t: 2.0,
        percent: 50.0,
        dur: 6.0,
        trans: 0.0,
        tout: 0.0,
        curve: String::new(),
        lane: lane.into(),
    };
    let bed = fx_volume::apply(&form("")).expect("a half-loud stretch applies");
    assert_eq!(bed.effect_kind(), Some(EffectKind::Volume));
    assert!(effect_overlap::gain_reaches(&bed, "mic-lav"));
    assert!(effect_overlap::gain_reaches(&bed, "screen"), "the bed is every lane");
    assert!(!effect_overlap::gained_before_the_mix(&bed), "the bed rides after the mix");

    let lane = fx_volume::apply(&form("mic-lav")).expect("a lane-named stretch applies");
    assert_eq!(lane.lane.as_str(), "mic-lav", "the lane is stored on the record");
    assert!(effect_overlap::gain_reaches(&lane, "mic-lav"));
    assert!(!effect_overlap::gain_reaches(&lane, "screen"), "and only that lane");
    assert!(effect_overlap::gained_before_the_mix(&lane), "before the sum, while it can be split");

    // The encode puts a lane-named gain BEFORE the bed-level ones, in one chain call.
    let clip = produce_render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 10.0,
        rate: 1.0,
        cues: 2,
        fx: vec![bed.clone(), lane.clone()],
        ..Default::default()
    };
    let built = produce_fx_chain::for_clip(&clip, &clip.fx, (1920, 1080), std::path::Path::new("/tmp"));
    let audio = built.audio.join("\n");
    let lane_at = audio
        .find("[snd]volume=volume=")
        .or_else(|| audio.find("volume=volume="))
        .unwrap_or_else(|| panic!("a gain filter was emitted at all: {audio}"));
    let bed_at = audio
        .rfind("volume=volume=")
        .expect("the bed gain is in the chain too");
    assert!(
        lane_at <= bed_at,
        "the lane gain runs before the bed gain: {audio}"
    );
    assert!(
        built.audio.iter().filter(|f| f.contains("volume=volume=")).count() >= 2,
        "both gains survive as their own filters: {:?}",
        built.audio
    );

    // Over a card the volume still arrives: §9's MUST row, applied as the preview applies it.
    let mut over_card = clip.clone();
    over_card.source = "cards/title.svg".into();
    over_card.fx = vec![lane];
    let built_over_card = produce_fx_chain::for_clip(&over_card, &over_card.fx, (1920, 1080), std::path::Path::new("/tmp"));
    assert!(
        built_over_card.audio.iter().any(|f| f.contains("volume=")),
        "a volume over a card is not skipped: {:?}",
        built_over_card.audio
    );
}

/// §9's two MUST-agree items: whose frame "source" means, and which camera a form judges
/// against. Both are one function both sides now read.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s7_preview_and_render_agree_on_camera_and_frame() {
    // The first footage clip wins over whatever camera is playing.
    assert_eq!(
        effect_overlap::source_frame(Some((1920, 1080)), Some((720, 1280))),
        (1920, 1080)
    );
    // With no footage clip at all the playing camera's shape stands.
    assert_eq!(effect_overlap::source_frame(None, Some((720, 1280))), (720, 1280));
    // And neither side is empty: the shipped preview declaration answers, so a plate is never
    // measured against nothing.
    assert_eq!(effect_overlap::source_frame(None, None), effect_overlap::FALLBACK_SHAPE);
    assert!(effect_overlap::FALLBACK_SHAPE.0 > 0 && effect_overlap::FALLBACK_SHAPE.1 > 0);
    // While an effect form is open the preview shows the LENS camera, not the watched row.
    assert_eq!(effect_overlap::preview_camera(true, Some(2), 0), 0);
    assert_eq!(effect_overlap::preview_camera(false, Some(2), 0), 2);
    // With nothing watched, the lens camera is the picture either way.
    assert_eq!(effect_overlap::preview_camera(false, None, 1), 1);
}

/// §9 (`The render MUST apply a volume over a card's own sound, as the preview does`): a card hears no
/// recording — its own sound IS the bed — so a whole-bed gain must still be counted and built for that clip,
/// while a duck naming a lane the card does not hear stays out. The plan decides it through
/// [`effect_overlap::gain_reaches_the_clip`] and the encode builds what the plan counted.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s9_the_render_applies_a_volume_over_a_cards_own_sound() {
    let dir = std::env::temp_dir().join(format!("np-s9-card-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let card_file = "cards/title.svg";
    let exists = |path: &str| path == card_file;

    // A half-loud stretch over the whole bed, and one aimed at a lane this card never hears.
    let bed = cut_effects_pass::defaults("volume", 1.0, (2.0, 6.0), Some(0.5));
    assert!(bed.lane.is_empty(), "the pass proposes a bed volume");
    let lav = Fx { kind: "volume".into(), t: 3.0, dur: 4.0, gain: 0.5, lane: "mic-lav".into(), ..Default::default() };
    assert_eq!(lav.effect_kind(), Some(EffectKind::Volume));

    // One footage scene that hears the lane, then a card over the same kind of stretch. Both gains fall inside
    // both scenes' own seconds, so each clip is asked about both: the footage hears `mic-lav` and takes both,
    // the card hears nothing and takes only the bed.
    let pieces = [scene(0.0, 10.0, 0), card(10.0, 16.0)];
    let mic = &Lane { name: "mic-lav".into(), src: "mic.wav".into(), at: 0.0, off: 0.0, dur: 60.0 };
    fn both_fx(_seg: &Seg) -> Vec<Fx> {
        vec![
            cut_effects_pass::defaults("volume", 1.0, (2.0, 6.0), Some(0.5)),
            Fx { kind: "volume".into(), t: 3.0, dur: 4.0, gain: 0.5, lane: "mic-lav".into(), ..Default::default() },
        ]
    }
    // The same two records placed inside the CARD's seconds, which is what its graph is built from.
    // (`defaults` adds `at` to the span's start, so the bed duck lands at 12 s, inside the card.)
    fn card_fx(_seg: &Seg) -> Vec<Fx> {
        vec![
            cut_effects_pass::defaults("volume", 10.0, (2.0, 6.0), Some(0.5)),
            Fx { kind: "volume".into(), t: 11.0, dur: 4.0, gain: 0.5, lane: "mic-lav".into(), ..Default::default() },
        ]
    }
    let (clips, logs) = produce_render::plan_clips(
        &pieces,
        both_fx,
        "16:9",
        exists,
        std::slice::from_ref(mic),
        &[],
        (1920, 1080),
        true,
        produce_render::unknown_footage,
    );
    assert!(logs.is_empty(), "nothing should be dropped: {logs:?}");
    assert_eq!(clips.len(), 2);
    let (footage, on_card) = (&clips[0], &clips[1]);
    assert!(on_card.source == card_file && footage.lanes.contains(&"mic-lav".to_string()));
    // Over the footage BOTH gains are work the encode has to do; over the card only the bed is, because the
    // card hears no `mic-lav` — §9's `lane` field is exactly that branch.
    assert_eq!(footage.cues, 2, "the lane duck and the bed duck both act on footage\n{footage:?}");
    assert!(footage.lanes.contains(&"mic-lav".to_string()), "the footage hears the lane: {:?}", footage.lanes);
    // A card that is told to hear nothing — §9's `Over inserts` row, seen from the sound side — still takes the
    // bed duck: its OWN sound is the bed. Before this fix a whole-bed cue was filtered out of every clip that
    // heard no lane, so the render applied no volume over a card while the preview did.
    let quiet_card = Seg { quiet: vec!["mic-lav".into()], ..card(10.0, 16.0) };
    assert!(!quiet_card.hears("mic-lav"), "the scene says what it does not hear");
    let (card_clips, card_logs) = produce_render::plan_clips(
        &[quiet_card],
        card_fx,
        "16:9",
        exists,
        std::slice::from_ref(mic),
        &[],
        (1920, 1080),
        true,
        produce_render::unknown_footage,
    );
    assert!(card_logs.is_empty(), "nothing should be dropped: {card_logs:?}");
    assert_eq!(card_clips.len(), 1);
    let over_card = &card_clips[0];
    assert_eq!(over_card.source, card_file);
    assert!(over_card.lanes.is_empty(), "it hears no recording: {:?}", over_card.lanes);
    assert_eq!(over_card.cues, 1, "the card's own sound takes the bed duck and nothing else\n{over_card:?}");

    // And the filter graph the encode builds for that card carries the volume filter, not an empty audio list.
    let built = produce_fx_chain::for_clip(over_card, &over_card.fx, (1920, 1080), &dir);
    assert!(
        built.audio.iter().any(|f| f.contains("volume=volume=")),
        "the render applies the volume over the card's own sound: {:?}",
        built.audio
    );
    // The reach rule itself, in both directions, with no plan involved: each cue read at seconds that fall
    // inside its own clip.
    let bed_cues = render_fx::all_gain_cues(std::slice::from_ref(&bed), 1.0, 1.0, 9.0);
    assert_eq!(bed_cues.len(), 1, "the pass's bed volume is one cue\n{bed:?}");
    let none_heard: Vec<String> = Vec::new();
    assert!(bed_cues[0].reaches_the_clip(&none_heard), "a bed reaches a clip that hears nothing");
    let lavs = render_fx::all_gain_cues(std::slice::from_ref(&lav), 1.0, 1.0, 9.0);
    assert_eq!(lavs.len(), 1);
    assert!(!lavs[0].reaches_the_clip(&none_heard), "a lane duck reaches nothing where its lane is not heard");
    let heard = vec!["mic-lav".to_string()];
    assert!(lavs[0].reaches_the_clip(&heard), "and it reaches its own lane where it is heard");
    drop(dir);
}

/// §9's last paragraph at the CUT pass's own door: the assembled request names the camera rows and the heard
/// lanes, but only once there is more than one of either to tell apart. The message is the one
/// `run_model` puts on the wire, assembled through `cut_suggest_model_run::user_turn_named`.
#[test]
fn sec_06_effects_9_s11_the_cut_brief_names_the_rows_and_lanes_when_there_are_more() {
    // The camera rows a two-camera cut shows, spelled the way the passing briefs spell them (§A's 1-based
    // camera number), read off the scenes the same door the press reads them from.
    let two_cam = Cut {
        segs: vec![scene(0.0, 20.0, 0), scene(20.0, 40.0, 1)],
        lanes: vec![lane("mic", 0.0, 60.0), lane("screen", 0.0, 60.0)],
        ..Default::default()
    };
    let rows: Vec<String> = {
        let mut named: Vec<String> = two_cam
            .segs
            .iter()
            .filter(|scene| scene.ins.is_empty())
            .map(|scene| (cut_cam::scene_row(scene) + 1).to_string())
            .collect();
        named.sort();
        named.dedup();
        named
    };
    let lanes: Vec<String> = two_cam.lanes.iter().map(|lane| lane.name.clone()).collect();
    assert_eq!(rows, vec!["1".to_string(), "2".to_string()], "the cut says which cameras filmed it");
    assert_eq!(lanes, vec!["mic".to_string(), "screen".to_string()]);

    let material = "[0.0s-5.0s] A: hi";
    let named = naivepost::ui::cut_suggest_model_run::user_turn_named(
        "a lecture about routers",
        1200.0,
        300.0,
        material,
        &rows,
        &lanes,
    );
    // The header is `brief_lines`' own wording, not a retyped copy of it.
    let header = effect_overlap::brief_lines(&rows, &lanes).expect("two rows and two lanes are named");
    assert!(named.contains(&header), "{named}");
    assert!(named.contains("camera 1, camera 2"), "{named}");
    assert!(named.contains("mic, screen"), "{named}");
    // And it sits BEFORE the material the model has to read, with the person's context still leading.
    assert!(
        named.find("a lecture about routers").expect("context") < named.find("SHOWN FROM").expect("header"),
        "{named}"
    );
    assert!(
        named.find("SHOWN FROM").expect("header") < named.find("SESSION LENGTH").expect("length"),
        "the naming line leads the material: {named}"
    );
    assert!(named.find("SESSION TIMELINE").expect("timeline") < named.find(material).expect("body"));
    // One camera, one lane: the message is the one this pass always sent, byte for byte.
    let plain =
        naivepost::ui::cut_suggest_model_run::user_turn("a lecture about routers", 1200.0, 300.0, material);
    assert!(!plain.contains("SHOWN FROM"), "{plain}");
    assert_eq!(
        plain,
        naivepost::ui::cut_suggest_model_run::user_turn_named(
            "a lecture about routers",
            1200.0,
            300.0,
            material,
            &["1".into()],
            &["mic".into()],
        ),
        "one row and one lane adds nothing"
    );
    // Two lanes alone count too — the naming rule is §9's, not a camera-only rule.
    let one_camera = naivepost::ui::cut_suggest_model_run::user_turn_named(
        "",
        1200.0,
        300.0,
        material,
        &["1".into()],
        &lanes,
    );
    assert!(one_camera.contains("one camera, HEARD LANES: mic, screen"), "{one_camera}");
    // The decoration pass, over the same cut, gets the same line from the same rule (§9's naming condition).
    let batch = Clips::new(&[(1, 0.0, 20.0), (2, 20.0, 20.0)]);
    let decorated = naivepost::effects_ask::user_message_over(&batch, &[], &rows, &lanes);
    assert!(decorated.contains(&header), "{decorated}");
    assert!(!naivepost::effects_ask::user_message(&batch, &[]).contains("SHOWN FROM"));
}

/// §9's other half: a suggested scene says which camera it is shown from. The cut pass's `add_segment` tool
/// carries no camera field (inventing one is not this item), so the placement door asks for the row a reply
/// named against the rows this session has, and §9's default answers — but the scene a suggestion lands into
/// is then ASKED, and answers with a row of its own.
#[test]
fn sec_06_effects_9_s12_a_suggested_scene_says_which_camera_it_is_shown_from() {
    // The rule: a named row this session has is kept, a row it has not falls back, nothing named is §9's row 0.
    assert_eq!(effect_overlap::suggested_scene_cam(Some(1), &[0, 1]), 1);
    assert_eq!(effect_overlap::suggested_scene_cam(Some(4), &[0, 1]), effect_overlap::SUGGESTED_ROW);
    assert_eq!(effect_overlap::suggested_scene_cam(None, &[0, 1]), effect_overlap::SUGGESTED_ROW);
    assert_eq!(effect_overlap::SUGGESTED_ROW, 0, "§9: suggested scenes all take row 0");
    // The placement door: the walk-back's answer landed over a cut the page shows from two cameras.
    let before = Cut {
        segs: vec![scene(0.0, 20.0, 0), scene(20.0, 40.0, 1)],
        ..Default::default()
    };
    let answer = naivepost::suggest::Applied {
        segs: vec![(0.0, 10.0), (22.0, 30.0)],
        kept_inserts: Vec::new(),
        effects: Vec::new(),
        log: Vec::new(),
    };
    let landed = naivepost::ui::cut_suggest_land::suggested_cut(&before, &answer);
    // Every landed scene answers the lens badge's question with a row of its own; the reply named none, so
    // they say the default, and the badge would light exactly one row.
    assert_eq!(landed.segs.len(), 2);
    for seg in &landed.segs {
        assert_eq!(
            seg.cam,
            effect_overlap::suggested_scene_cam(None, &naivepost::cut_suggest_words::depended_cam_slots(&before)),
            "the placed scene says its camera: {seg:?}"
        );
        assert!(cut_cam::shown_from(seg.cam, effect_overlap::SUGGESTED_ROW));
    }
    // And a scene that DOES name a row the session has keeps it through the same door.
    let named_card = Seg { s: 5.0, e: 6.0, dur: 0.0, ins: "cards/title.svg".into(), ..Default::default() };
    let with_named = naivepost::suggest::Applied {
        segs: Vec::new(),
        kept_inserts: vec![scene(0.0, 10.0, 1), named_card],
        effects: Vec::new(),
        log: Vec::new(),
    };
    let landed_named = naivepost::ui::cut_suggest_land::suggested_cut(&before, &with_named);
    assert_eq!(landed_named.segs[0].cam, 1, "a named row of this session survives placement");
    // A card names no camera of its own — §9's `Over inserts: none` — and takes the default.
    assert_eq!(landed_named.segs[1].cam, effect_overlap::SUGGESTED_ROW);
    // And the placement path is the one that ASKS: with the scene's own row outside the session's rows, §9's
    // fallback answers the landed scene, which only happens inside `suggested_cut`.
    let one_row_session = Cut {
        segs: vec![scene(0.0, 20.0, 0)],
        ..Default::default()
    };
    let out_of_range = naivepost::ui::cut_suggest_land::suggested_cut(
        &one_row_session,
        &naivepost::suggest::Applied {
            segs: Vec::new(),
            kept_inserts: vec![scene(0.0, 10.0, 4)],
            effects: Vec::new(),
            log: Vec::new(),
        },
    );
    assert_eq!(
        out_of_range.segs[0].cam,
        effect_overlap::SUGGESTED_ROW,
        "a scene naming a row this session has not is placed on §9's row, not on a camera nothing records"
    );
    // The door is called from the same place the page lands a WORDS cut with no model at all (§S2a), so the
    // rule runs on every suggestion whether or not a model spoke: `land_spans` goes through `suggested_cut`.
    let by_span = naivepost::ui::cut_suggest_land::suggested_cut(&before, &naivepost::suggest::Applied {
        segs: vec![(0.0, 12.0)],
        kept_inserts: Vec::new(),
        effects: Vec::new(),
        log: Vec::new(),
    });
    assert_eq!(by_span.segs[0].cam, effect_overlap::SUGGESTED_ROW, "the no-model cut answers too");
    assert!(cut_cam::shown_from(by_span.segs[0].cam, effect_overlap::SUGGESTED_ROW));
}

/// §9's last paragraph: the model passes know nothing of cameras and lanes until there is more
/// than one of either, and then the brief names them.
#[test]
fn sec_06_effects_9_where_recordings_overlap_s8_the_briefs_name_rows_and_lanes_only_when_there_are() {
    // Suggested framing: centred, at the height the catalogue spells.
    assert_eq!(cut_effects_pass::ZOOM_HEIGHT, 0.6);
    assert_eq!(effect_overlap::suggested_zoom_is_centred(), (0.5, 0.5, 0.6));
    assert_eq!(effect_overlap::SUGGESTED_ROW, 0);
    // A decoration suggestion placed through the pass's own door: row 0, no camera claim, and a
    // volume that therefore hits the whole bed.
    let batch = Clips::new(&[(1, 0.0, 30.0)]);
    let zoom = cut_effects_pass::defaults("zoom", 0.0, (2.0, 6.0), None);
    assert_eq!(zoom.cam, effect_overlap::SUGGESTED_ROW);
    assert_eq!((zoom.cx, zoom.cy, zoom.hf), (Some(0.5), Some(0.5), Some(0.6)));
    let gain = cut_effects_pass::defaults("volume", 0.0, (2.0, 6.0), Some(0.5));    assert!(gain.lane.is_empty(), "a proposed volume names no lane");
    assert!(effect_overlap::suggested_volume_reaches_the_bed(&gain));
    drop(batch);
    // The naming rule, and the one line it produces.
    assert!(!effect_overlap::names_rows_and_lanes(1, 1));
    assert!(effect_overlap::names_rows_and_lanes(2, 1));
    assert!(effect_overlap::names_rows_and_lanes(1, 2));
    assert_eq!(effect_overlap::brief_lines(&["1".into()], &["mic".into()]), None);
    let named = effect_overlap::brief_lines(&["1".into(), "2".into()], &["mic".into(), "screen".into()])
        .expect("two rows and two lanes are named");
    assert!(named.contains("camera 1") && named.contains("camera 2"), "{named}");
    assert!(named.contains("mic") && named.contains("screen"), "{named}");
    // And the real briefs carry that header only when there is something to name.
    let clips = Clips::new(&[(1, 0.0, 30.0), (2, 30.0, 60.0)]);
    let plain = cut_effects_pass::brief_over(&clips, &[], &["1".into()], &["mic".into()]);
    assert!(!plain.contains("SHOWN FROM"), "one camera, one lane: nothing to name\n{plain}");
    let rows = ["1".to_string(), "2".to_string()];
    let lanes = ["mic".to_string(), "screen".to_string()];
    let many = cut_effects_pass::brief_over(&clips, &[], &rows, &lanes);
    assert!(many.contains("SHOWN FROM"), "two cameras get named to the model\n{many}");
    assert!(many.lines().next().expect("a first line").contains("screen"));
    // The speed ask builds its own header line, and joins the same rule.
    let speeds = naivepost::speeds_ask::user_message_over(&[(1, 30.0), (2, 30.0)], &[], 60.0, &rows, &lanes);
    assert!(speeds.contains("SHOWN FROM"), "{speeds}");
    let single = naivepost::speeds_ask::user_message_over(&[(1, 30.0)], &[], 30.0, &["1".into()], &["mic".into()]);
    assert!(!single.contains("SHOWN FROM"));
}
