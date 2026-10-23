//! §06-effects#6-parameters-used — every number §6 lists is READ where it takes effect.
//!
//! A catalogue row proves only that a number was written down twice. What this file checks is the
//! other half: that the rule which owns the value is the one the program runs through, so retuning
//! `layout.effectRowPx` moves the lane the lane-drawer draws, `P.eng.minRate`/`maxRate`/
//! `rampStepSeconds` decide how many constant-rate stretches ffmpeg is asked to encode, `P.eng.maxGain`
//! bounds the gain that reaches the `volume=` filter, and `P.machine.maxProposedEffects` is compared
//! against a real reply rather than quoted beside an empty one.

use naivepost::{
    cut::Fx, cut_effects_pass as effects_pass, cut_hear, cut_speed, fx_lane, fx_record, params,
    render_fx,
};

use crate::common;
use common::anywhere;

fn speed(t: f64, dur: f64, rate: f64, ramp: f64) -> Fx {
    Fx {
        kind: "speed".into(),
        t,
        dur,
        rate,
        trans: ramp,
        tout: ramp,
        ..Default::default()
    }
}

fn volume(t: f64, dur: f64, gain: f64) -> Fx {
    Fx {
        kind: "volume".into(),
        t,
        dur,
        gain,
        ..Default::default()
    }
}

/// §6's "lane height 26": the catalogue names the owner the lane is drawn with, not a second copy of
/// the number in the timeline module.
#[test]
fn sec_06_effects_6_parameters_used_s1_lane_height_has_one_owner() {
    let row = anywhere("layout.effectRowPx");
    assert_eq!(
        row.from, "fx_lane::lane_height_px",
        "the row must name the function the lane sizes itself with, not a constant beside it"
    );
    // And the number §6 spells is what that owner answers for one row.
    assert_eq!(fx_lane::lane_height_px(1), 26.0);
    assert_eq!(
        fx_lane::lane_height_px(3),
        78.0,
        "rows multiply the one number"
    );
    // An empty lane still costs a row (§2), so the floor is one row, never nought.
    assert_eq!(fx_lane::lane_height_px(0), 26.0);
}

/// §6's minRate / maxRate / rampStepSeconds: the render's speed spans come from the staircase those
/// three own, so a longer or steeper ramp means more constant-rate stretches on the encode chain.
#[test]
fn sec_06_effects_6_parameters_used_s2_the_render_reads_the_staircase() {
    // The picture list the render builds for a sped clip is the staircase, not one flat span.
    let seg = naivepost::cut::Seg {
        s: 0.0,
        e: 20.0,
        ..Default::default()
    };
    let long_ramp = vec![speed(0.0, 12.0, 8.0, 6.0)];
    let short_ramp = vec![speed(0.0, 12.0, 8.0, 0.5)];
    let long = render_fx::split_pictures(&long_ramp, &seg);
    let short = render_fx::split_pictures(&short_ramp, &seg);
    assert!(
        long.iter().filter(|p| p.rate > 1.0).count() > 2,
        "a 6 s ramp up to ×8 is encoded as several stairs, got {long:?}"
    );
    assert!(
        long.len() > short.len(),
        "a longer, steeper ramp cannot give fewer stairs: {} vs {}",
        long.len(),
        short.len()
    );
    // Every stair sits inside the clip and carries a rate the clamps allow.
    for picture in &long {
        assert!(
            picture.from >= seg.s && picture.to <= seg.e,
            "{picture:?} outside the clip"
        );
        assert!(
            picture.rate == 0.0
                || (cut_speed::MIN_RATE..=cut_speed::MAX_RATE).contains(&picture.rate),
            "stair rate {} is outside P.eng.minRate..=maxRate",
            picture.rate
        );
    }
    // A rate beyond the bound is pulled back before the stairs are counted, on this path too.
    let wild = render_fx::split_pictures(&[speed(0.0, 10.0, 500.0, 4.0)], &seg);
    assert!(
        wild.iter().all(|p| p.rate <= cut_speed::MAX_RATE),
        "an absurd ask must not escape the clamp: {wild:?}"
    );
    // A stop keeps its meaning: the encode never gives a freeze a rate — the footage runs on at 1×
    // and a still rides over it — so no stair may come out of a `rate 0` ask. `steps_of` answers ×0
    // (that is what makes it a stop to every reader), and this is the rule that refuses to spend it.
    let freezing = [speed(0.0, 3.0, 0.0, 1.0)];
    assert!(
        render_fx::footage_stays_at_one(&freezing),
        "a stop leaves the footage at 1x"
    );
    assert!(
        cut_speed::render_stairs(&freezing, &seg)
            .iter()
            .all(|span| span.rate == 0.0),
        "the staircase itself must not turn a freeze into slow motion"
    );
    // The price of a stair is rampStepSeconds' rule, spelled by the same constant.
    assert_eq!(cut_speed::RAMP_STEP_SECONDS, 0.6);
}

/// §6's maxGain (10): the ceiling that reaches the ffmpeg `volume=` expression is the one the record
/// rule names, so the loudest ask cannot exceed what the render will honour.
#[test]
fn sec_06_effects_6_parameters_used_s3_max_gain_bounds_the_filter() {
    assert_eq!(fx_record::gain_ceiling(), 10.0);
    assert_eq!(fx_record::gain_ceiling(), cut_hear::MAX_GAIN);
    let cues = render_fx::gain_cues(&[volume(0.0, 4.0, 99.0)], 0.0, 1.0, 4.0);
    assert_eq!(cues.len(), 1, "one cue per volume effect");
    assert_eq!(
        cues[0].gain,
        fx_record::gain_ceiling(),
        "a gain over the ceiling arrives at the filter AT the ceiling"
    );
    // The filter's own multiplier is the ceiling minus unity: `volume=1+9.0000*…` reaches ×10 at the
    // top of the band, and a gain above the ceiling cannot make it say ten.
    let expr = render_fx::gain_expr(&cues[0]);
    assert!(
        expr.contains(&format!("{:.4}", fx_record::gain_ceiling() - 1.0)),
        "the volume expression must carry the ceiling as its multiplier, got {expr}"
    );
    assert!(
        !expr.contains(&format!("{:.4}", 99.0 - 1.0)),
        "an ask over the ceiling must not reach the filter: {expr}"
    );
    // Silence stays silence: 0 is 0 and not the floor of a clamp.
    let quiet = render_fx::gain_cues(&[volume(0.0, 4.0, 0.0)], 0.0, 1.0, 4.0);
    assert_eq!(quiet[0].gain, 0.0);
}

/// §6's decorations density ceiling: `P.machine.maxProposedEffects` is compared against a real reply.
/// At the cap the answer stands; one past it is refused and said, exactly as a failed ask is.
#[test]
fn sec_06_effects_6_parameters_used_s4_the_cap_is_compared_against_a_reply() {
    let cap = effects_pass::MAX_PROPOSED_EFFECTS;
    let at_cap: Vec<Fx> = (0..cap)
        .map(|i| Fx {
            kind: "zoom".into(),
            t: i as f64,
            dur: 2.0,
            ..Default::default()
        })
        .collect();
    assert_eq!(effects_pass::proposed_effect_count(&at_cap), cap);
    assert!(effects_pass::effect_cap_allows(
        effects_pass::proposed_effect_count(&at_cap)
    ));
    let over = {
        let mut more = at_cap.clone();
        more.push(Fx {
            kind: "zoom".into(),
            t: cap as f64,
            dur: 2.0,
            ..Default::default()
        });
        more
    };
    assert!(!effects_pass::effect_cap_allows(
        effects_pass::proposed_effect_count(&over)
    ));
    // The door the pass walks: a reply over the cap comes back refused, with the reason in hand.
    let verdict = effects_pass::effect_cap_allows_reply(&over);
    assert!(
        !verdict.allowed,
        "a reply past the cap must be refused, not placed"
    );
    assert!(
        verdict.reason.contains("maxProposedEffects") || verdict.reason.contains("cap"),
        "the refusal says which bound it hit: {:?}",
        verdict.reason
    );
    let ok = effects_pass::effect_cap_allows_reply(&at_cap);
    assert!(ok.allowed, "exactly the cap is allowed: {ok:?}");
}

/// §6's remaining numbers each sit on the row of the module that READS them — a retune there changes
/// the behaviour, which is what makes the row worth having.
#[test]
fn sec_06_effects_6_parameters_used_s5_each_row_names_the_module_that_reads_it() {
    let reads: &[(&str, &str)] = &[
        ("P.eng.minClipSeconds", "tools::cutpass::MIN_CLIP_SECONDS"),
        ("P.policy.speedGapSeconds", "cut_speed_pass::GAP_SECONDS"),
        ("P.machine.captionBatch", "cut_captions::BATCH"),
        (
            "P.policy.captionMinSeconds",
            "tools::clips::CAPTION_MIN_SECONDS",
        ),
        (
            "P.eng.effectMinSurvivingSeconds",
            "cut_clamp::MIN_SURVIVING_SECONDS",
        ),
        ("P.eng.minRate", "cut_speed::MIN_RATE"),
        ("P.eng.maxRate", "cut_speed::MAX_RATE"),
        ("P.eng.rampStepSeconds", "cut_speed::RAMP_STEP_SECONDS"),
        ("P.eng.maxGain", "fx_record::gain_ceiling"),
        ("layout.effectRowPx", "fx_lane::lane_height_px"),
        ("layout.snapPx", "cut_select::SNAP_PX"),
        ("effects.snapPx", "fx_text::SNAP_PX"),
        ("effects.previewRasterPx", "fx_svg::PREVIEW_RASTER_PX"),
        ("card.bakeFps", "cut_cards::BAKE_FPS"),
        (
            "effects.markedBandMinSeconds",
            "cut_speed::MIN_MARKED_SECONDS",
        ),
    ];
    for (id, from) in reads {
        let row = anywhere(id);
        assert_eq!(
            row.from, *from,
            "{id} must be catalogued from the rule that reads it"
        );
    }
    // The values §6 spells outright, read off their owners.
    assert_eq!(anywhere("card.bakeFps").spelled, "25");
    assert_eq!(anywhere("effects.previewRasterPx").spelled, "512");
    assert_eq!(anywhere("P.eng.maxGain").spelled, "10");
    assert_eq!(anywhere("layout.effectRowPx").spelled, "26");
    assert_eq!(anywhere("P.machine.captionBatch").spelled, "5");
    // And the defaults/floors §6 lists live in the modules whose forms apply them.
    assert_eq!(naivepost::fx_zoom::MIN_SECONDS, 0.4);
    assert_eq!(naivepost::fx_text::MIN_SECONDS, 0.3);
    assert_eq!(naivepost::fx_label::MIN_SECONDS, 0.4);
    assert_eq!(naivepost::fx_volume::MIN_SECONDS, 0.1);
    assert_eq!(cut_speed::MIN_MARKED_SECONDS, 0.2);
    assert_eq!(naivepost::fx_svg::PREVIEW_RASTER_PX, 512.0);
    assert_eq!(params::effects().len().cmp(&0), std::cmp::Ordering::Greater);
}

/// §6's minRate / maxRate / rampStepSeconds / minClipSeconds on the path a RENDER walks: the clip list ffmpeg is
/// given comes from `cut_speed_split::sped_segs`, which `produce_exec::plan` calls before it plans a single clip,
/// so these four numbers decide how many clips there are and what each one runs at.
#[test]
fn sec_06_effects_6_parameters_used_s6_the_speed_bounds_reach_the_clip_list_the_encoder_gets() {
    use naivepost::cut_speed_split as split;
    let seg = naivepost::cut::Seg {
        s: 0.0,
        e: 40.0,
        ..Default::default()
    };

    // (a) A rate past P.eng.maxRate reaches the clip list INSIDE the bound: `steps_of` clamps before any stair is
    // counted, so no piece can carry the absurd ask onward to `setpts`.
    let wild = split::sped_segs(std::slice::from_ref(&seg), &[speed(0.0, 20.0, 500.0, 0.0)]);
    assert!(!wild.is_empty(), "a sped scene still yields clips");
    for piece in &wild {
        if piece.rate > 0.0 {
            assert!(
                (cut_speed::MIN_RATE..=cut_speed::MAX_RATE).contains(&piece.rate),
                "piece rate {} escaped P.eng.minRate..=maxRate",
                piece.rate
            );
        }
    }
    // The clamp really did move this ask, rather than the number being harmless anyway.
    assert!(
        wild.iter()
            .any(|p| p.rate > 0.0 && p.rate <= cut_speed::MAX_RATE),
        "the sped pieces carry a clamped rate: {:?}",
        wild.iter().map(|p| p.rate).collect::<Vec<_>>()
    );

    // (b) P.eng.rampStepSeconds decides how many constant-rate stretches a ramp becomes: every stair must hold on
    // screen for that long, so a long ramp cuts the scene into more clips than the same rate with no ramps.
    let ramped = split::sped_segs(std::slice::from_ref(&seg), &[speed(0.0, 30.0, 8.0, 9.0)]);
    let flat = split::sped_segs(std::slice::from_ref(&seg), &[speed(0.0, 30.0, 8.0, 0.0)]);
    assert!(
        ramped.len() > flat.len(),
        "a ramped speed cuts more clips than a flat one: {} vs {}",
        ramped.len(),
        flat.len()
    );
    // Each stair is at least RAMP_STEP_SECONDS on screen — that is the rule the count obeys.
    for piece in &ramped {
        let seconds =
            (piece.e - piece.s) / cut_speed::applied_rate(piece.rate).max(cut_speed::MIN_RATE);
        assert!(
            seconds + 1e-9 >= cut_speed::RAMP_STEP_SECONDS || piece.rate == 0.0,
            "a stair shorter than P.eng.rampStepSeconds reached the clip list: {piece:?}"
        );
    }

    // (c) P.eng.minClipSeconds owns the drop of a sliver: a span thinner than the floor where it meets a scene
    // border is not encoded, and the number that said so is the render's own shortest-clip constant.
    assert_eq!(naivepost::tools::cutpass::MIN_CLIP_SECONDS, 0.5);
    let sliver = split::sped_segs(
        std::slice::from_ref(&naivepost::cut::Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        }),
        &[speed(9.8, 0.1, 4.0, 0.0)],
    );
    for piece in &sliver {
        let on_screen =
            (piece.e - piece.s) / cut_speed::applied_rate(piece.rate).max(cut_speed::MIN_RATE);
        assert!(
            on_screen + 1e-9 >= naivepost::tools::cutpass::MIN_CLIP_SECONDS,
            "a piece under P.eng.minClipSeconds survived the split: {piece:?} ({on_screen} s)"
        );
    }
    // And the same door leaves an un-sped cut alone, piece for piece: the splits are the speed effects' doing.
    let untouched = split::sped_segs(std::slice::from_ref(&seg), &[]);
    assert_eq!(untouched.len(), 1, "no speed effect, no split");
    assert_eq!(untouched[0].rate, 0.0, "unstamped means plain footage");
}

/// §6's maxGain (10) on the encode path: `produce_fx_chain::for_clip` is what builds one clip's filter text, and
/// its audio chain is spliced straight into the command (`produce_render.rs` extends the chain with `built.audio`).
/// So the ceiling has to be inside THAT text, not only inside the preview's mix.
#[test]
fn sec_06_effects_6_parameters_used_s7_max_gain_reaches_the_encode_filter() {
    let scratch = std::env::temp_dir().join(format!("np-06e6-gain-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let fx = vec![volume(0.0, 4.0, 99.0)];
    let clip = naivepost::produce_render::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 4.0,
        rate: 1.0,
        frame: (1920, 1080),
        src: (1920, 1080),
        fx: fx.clone(),
        scratch: scratch.clone(),
        ..Default::default()
    };
    let built = naivepost::produce_fx_chain::for_clip(&clip, &fx, clip.src, &scratch);
    let audio = built.audio.join(",");
    assert!(
        audio.contains("volume="),
        "the clip's audio chain carries the volume filter: {audio}"
    );
    assert!(
        audio.contains(&format!("{:.4}", fx_record::gain_ceiling() - 1.0)),
        "the multiplier ffmpeg is asked for is the ceiling minus unity: {audio}"
    );
    assert!(
        !audio.contains("98.0000"),
        "an ask over the ceiling must not reach the filter: {audio}"
    );
    // Silence stays silence through the same door: 0 is 0, not the floor of a clamp.
    let quiet_fx = vec![volume(0.0, 4.0, 0.0)];
    let quiet_clip = naivepost::produce_render::Clip {
        fx: quiet_fx.clone(),
        ..clip.clone()
    };
    let quiet = naivepost::produce_fx_chain::for_clip(&quiet_clip, &quiet_fx, clip.src, &scratch);
    let cues = render_fx::all_gain_cues(&quiet_fx, 0.0, 1.0, 4.0);
    assert_eq!(cues.len(), 1, "a hush is still a cue");
    assert_eq!(
        cues[0].gain, 0.0,
        "P.eng.maxGain clamps the top, never the bottom"
    );
    assert!(
        !quiet.audio.is_empty(),
        "the hush reaches the audio chain too"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}
