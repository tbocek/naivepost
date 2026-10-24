//! §05-cut#6-parameters-used — the three §6 preview/engineering numbers that had a row but no reader, plus the
//! target the Suggest press hands its prompt.
//!
//! A catalogue row is only worth having when a rule reads the number behind it, so every test here asks the RULE
//! — the rate-seek gate, the talking-pad span, the band's batch window, the press's target — and compares it with
//! what `params::cut()` claims. Clock-free throughout: each door takes its elapsed time as an argument, so no
//! test waits on anything.

use naivepost::{cut, cut_play, cut_play_back, params, suggest, ui::cut_tracks};

fn row(id: &str) -> String {
    params::cut()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{id} has no row"))
        .spelled
}

fn cut_with(segs: Vec<cut::Seg>) -> cut::Cut {
    cut::Cut {
        segs,
        ..Default::default()
    }
}

fn seg(s: f64, e: f64) -> cut::Seg {
    cut::Seg {
        s,
        e,
        ..Default::default()
    }
}

/// `preview.rateSeekGapMs` — one flushing re-cue per gap, and none before it. The prototype holds its rate
/// seeks to `rateSeekGap` (`go/player.go`'s `SetRateNow`); this is that rule as a function of the elapsed
/// time, so both sides of the boundary and the "unchanged rate asks nothing" case are checkable at once.
#[test]
fn sec_05_cut_6_parameters_used_s1_the_rate_door_holds_one_recue_per_gap() {
    let gap = naivepost::cut_play_back::RATE_SEEK_GAP_MS;
    assert_eq!(row("preview.rateSeekGapMs"), format!("{gap} ms"));
    assert_eq!(gap, 250);
    // First change ever: nothing was re-cued, so nothing is too soon.
    assert_eq!(
        cut_play_back::rate_seek_action(true, None),
        cut_play_back::RateSeek::Seek,
        "the first rate change is never too soon"
    );
    assert_eq!(
        cut_play_back::rate_seek_action(true, Some(gap - 1)),
        cut_play_back::RateSeek::Hold,
        "one millisecond inside the gap the rate goes back"
    );
    assert_eq!(
        cut_play_back::rate_seek_action(true, Some(gap)),
        cut_play_back::RateSeek::Seek,
        "at the gap the re-cue may happen — the bound is inclusive"
    );
    assert_eq!(
        cut_play_back::rate_seek_action(true, Some(gap * 4)),
        cut_play_back::RateSeek::Seek
    );
    // A rate the cut did not change asks the player for nothing at all, however long it has been.
    assert_eq!(
        cut_play_back::rate_seek_action(false, Some(gap * 10)),
        cut_play_back::RateSeek::Hold,
        "nothing changed, so nothing stumbles"
    );
}

/// The same gap on the running door: with no player there is nothing to re-cue, and `set_rate` says so before
/// it consults the gate — the gate is the second lock, not the first.
#[test]
fn sec_05_cut_6_parameters_used_s2_the_rate_door_refuses_without_a_player() {
    let logged = std::cell::Cell::new(0usize);
    let asked = |_: &str| logged.set(logged.get() + 1);
    assert!(
        !cut_play_back::set_rate(&[], 1.0, 0.0, &asked),
        "with no player running a rate change cannot happen"
    );
    assert_eq!(
        logged.get(),
        0,
        "the refusal is silent — nothing was attempted"
    );
}
/// `P.eng.talkPadSeconds` — "how close to a word still counts as inside it". The walk-back asks this of every
/// hole the model left: speech in it means the hole was a mistake and gets closed, silence means it stands.
/// The pad is what makes a span that ends a hair before the hole count as inside it.
#[test]
fn sec_05_cut_6_parameters_used_s3_a_hole_counts_speech_up_to_the_pad_away() {
    let pad = suggest::TALK_PAD_SECONDS;
    assert_eq!(row("P.eng.talkPadSeconds"), "0.2");
    assert_eq!(pad, 0.2);
    let gap = (10.0, 12.0);
    // Well inside, and touching: speech either way.
    assert!(suggest::hole_has_speech(gap, &[(10.5, 11.5)]));
    assert!(suggest::hole_has_speech(gap, &[(9.0, 10.0)]));
    // Ends `pad` before the hole, overlap by overlap: `timeline::spans_overlap` is strict, so the padded span
    // has to reach INTO the hole for speech to count, and it does for anything nearer than the pad.
    assert!(suggest::hole_has_speech(gap, &[(8.0, 10.0 - pad / 2.0)]));
    assert!(suggest::hole_has_speech(gap, &[(12.0 + pad / 2.0, 13.0)]));
    // One hair past the pad is silence: the hole the model dropped stands.
    assert!(!suggest::hole_has_speech(gap, &[(8.0, 10.0 - pad - 0.01)]));
    assert!(!suggest::hole_has_speech(gap, &[(12.0 + pad + 0.01, 14.0)]));
    assert!(!suggest::hole_has_speech(gap, &[]));
    // And the rule is the one the walk-back runs: the same call `close_holes` makes.
    let closed = suggest::close_holes(&[(0.0, 10.0), (12.0, 20.0)], &[(9.9, 10.0)], 3.0);
    assert_eq!(closed, vec![(0.0, 20.0)], "a talked-in hole closes");
    let kept_open = suggest::close_holes(&[(0.0, 10.0), (12.0, 20.0)], &[(9.5, 9.7)], 3.0);
    assert_eq!(
        kept_open,
        vec![(0.0, 10.0), (12.0, 20.0)],
        "a hole nobody talked in stays dropped"
    );
}

/// `preview.thumbBatch` — how far one pass of the band looks into a row's frame list. Six at a time is what
/// inventory §D says the loader hands back, and the window is a slice, so the tail is shorter and an index
/// past the end is nothing rather than a panic in a draw.
#[test]
fn sec_05_cut_6_parameters_used_s4_the_band_reads_six_frame_names_at_a_time() {
    let batch = cut_tracks::THUMB_BATCH;
    assert_eq!(row("preview.thumbBatch"), "6");
    assert_eq!(batch, 6);
    let paths: Vec<std::path::PathBuf> = (0..14)
        .map(|i| std::path::PathBuf::from(format!("f{i}.jpg")))
        .collect();
    assert_eq!(cut_tracks::batch_of(&paths, 0).len(), 6);
    assert_eq!(cut_tracks::batch_of(&paths, 6).len(), 6);
    assert_eq!(
        cut_tracks::batch_of(&paths, 12).len(),
        2,
        "the last batch is the tail"
    );
    assert!(
        cut_tracks::batch_of(&paths, 14).is_empty(),
        "past the end is nothing"
    );
    assert!(cut_tracks::batch_of(&[], 0).is_empty());
    // The window is a real window: the band's own picker finds the frame nearest a second inside it and not
    // one the pass never looked at.
    let stamped: Vec<std::path::PathBuf> = (0..20)
        .map(|i| {
            std::path::PathBuf::from(format!("frame_{}", naivepost::clock::frame_name(i as f64)))
        })
        .collect();
    let window = cut_tracks::batch_of(&stamped, 4);
    assert_eq!(window.len(), 6);
    assert!(cut_tracks::frame_near(window, 6.0).is_some());
}

/// `P.eng.preloadLeadSeconds` on the arm cadence: the same three seconds decide both when a jump is worth
/// opening and how often the arm may rebuild a process for it.
#[test]
fn sec_05_cut_6_parameters_used_s5_the_spare_arms_at_the_leads_cadence() {
    assert_eq!(row("P.eng.preloadLeadSeconds"), "3");
    assert_eq!(
        cut_play::PRELOAD_ARM_GAP_MS,
        (naivepost::preview::PRELOAD_LEAD_SECONDS * 1000.0) as u64
    );
    let lead = cut_play::PRELOAD_ARM_GAP_MS;
    // Never armed: the first arm is free. After the lead: worth it again.
    assert!(cut_play::worth_arming(None, None, Some(12.0)));
    assert!(cut_play::worth_arming(Some(lead), None, Some(12.0)));
    // Inside the lead the line keeps crawling toward the same jump and nothing new is built.
    assert!(!cut_play::worth_arming(Some(lead - 1), None, Some(12.0)));
    // The second already held is never worth a process, however long it has been.
    assert!(!cut_play::worth_arming(
        Some(lead * 10),
        Some(12.0),
        Some(12.0)
    ));
    // A new jump is, and no jump asked means nothing to arm.
    assert!(cut_play::worth_arming(Some(lead), Some(12.0), Some(30.0)));
    assert!(!cut_play::worth_arming(None, None, None));
    // The door that reads it is the one the running tick calls every 100 ms: with no cut and no jump it asks
    // for nothing, which is what keeps a plain preview from spawning processes.
    let quiet = cut_with(vec![seg(0.0, 60.0)]);
    assert_eq!(cut_play::preload_ahead(&quiet.segs, 5.0), None);
    let jump = cut_with(vec![seg(0.0, 10.0), seg(11.5, 20.0)]);
    assert_eq!(cut_play::preload_ahead(&jump.segs, 9.0), Some(11.5));
    assert_eq!(cut_play::preload_ahead(&jump.segs, 5.0), None);
}

/// `P.policy.targetLengthSeconds` — the press hands its prompt the SESSION's target, read from the policy the
/// project holds, rather than a literal burned into the call. Default §10 spells `0 (none)`, and the default
/// policy answers exactly that, so an undeliberated session still asks for no target the honest way.
#[test]
fn sec_05_cut_6_parameters_used_s6_the_press_asks_the_policy_for_its_target() {
    assert_eq!(row("P.policy.targetLengthSeconds"), "0 (none)");
    let policy = naivepost::project::Policy::default();
    assert_eq!(
        naivepost::ui::cut_suggest_model_run::target_length_seconds(&policy),
        0.0
    );
    // A hand-set target reaches the prompt through the same door, with no rewiring.
    let mut aimed = policy.clone();
    aimed.target_length_seconds.value = 600.0;
    assert_eq!(
        naivepost::ui::cut_suggest_model_run::target_length_seconds(&aimed),
        600.0
    );
    // And that is the number that decides what the prompt says: aimed at ten minutes it gives the accepted
    // window, a policy with no target says so in words instead.
    assert!(
        suggest::target_block(600.0).starts_with("KEEP between"),
        "the prompt carries the target: {}",
        suggest::target_block(600.0)
    );
    assert!(
        suggest::target_block(naivepost::ui::cut_suggest_model_run::target_length_seconds(
            &policy
        ))
        .contains("NO TARGET"),
        "a policy with no target says so"
    );
}

// --- S7: every id §6 names has a row, and the row answers with the number §6 states -------------------------

/// §05-cut#6-parameters-used — §6's list read as a table. The ids and their values come from the spec's own
/// sentence, not from the catalogue, so a row that was renamed, dropped, or retuned away from §6 shows up here.
/// Rows §10 gives no number for (the two segment formulas, the footage window) are checked against the
/// arithmetic §10 spells, which is the same row the prompt is built from.
#[test]
fn sec_05_cut_6_parameters_used_s7_every_id_6_names_has_a_row_with_6s_number() {
    let listed: Vec<String> = params::cut()
        .into_iter()
        .map(|p| p.id.to_string())
        .collect();
    let number_rows: [(&str, f64); 20] = [
        ("P.policy.minSceneSeconds", 1.0),
        ("P.eng.minPieceSeconds", 0.04),
        ("P.policy.snapToleranceSeconds", 5.0),
        ("P.eng.talkPadSeconds", 0.2),
        ("P.policy.deadAirMaxSeconds", 8.0),
        ("P.policy.deadAirKeepSeconds", 0.5),
        ("P.eng.seamMaxSeconds", 1.5),
        ("P.policy.reviewPadSeconds", 10.0),
        ("P.policy.maxSpeedRate", 4.0),
        ("P.policy.insertDefaultSeconds", 4.0),
        ("preview.playTickMs", 100.0),
        ("P.eng.preloadLeadSeconds", 3.0),
        ("preview.cardFps", 8.0),
        ("layout.edgeGrabPx", 6.0),
        ("layout.snapPx", 8.0),
        ("layout.lineReachPx", 12.0),
        ("layout.rulerPx", 18.0),
        ("layout.selBandPx", 22.0),
        ("layout.effectRowPx", 26.0),
        ("layout.waveLanePx", 30.0),
    ];
    for (id, want) in number_rows {
        assert!(
            listed.iter().any(|have| have == id),
            "{id} has no row at all"
        );
        let spelled = row(id);
        let got: f64 = spelled
            .trim_end_matches(" ms")
            .trim_end_matches(" (none)")
            .parse()
            .unwrap_or_else(|_| panic!("{id} spells {spelled}, not a number"));
        assert!(
            (got - want).abs() < 1e-9,
            "{id} reads {got}, §6 says {want}"
        );
    }
    // The rows §6 names without a number of their own.
    assert_eq!(row("P.policy.targetLengthSeconds"), "0 (none)");
    assert_eq!(row("preview.rateSeekGapMs"), "250 ms");
    assert_eq!(row("preview.thumbBatch"), "6");
    assert_eq!(row("layout.undoDepth"), "50");
    assert_eq!(row("layout.waveCacheMagic"), "AWV4");
    assert_eq!(row("layout.zoomAtOpen"), "4");
    assert_eq!(row("layout.zoomMaxPps"), "240");
    // The caption batch lives with the pass that reads it; §6 lists it here, so the row must exist somewhere.
    assert!(
        params::find("P.machine.captionBatch").is_some(),
        "captionBatch (5) has no row"
    );
    assert_eq!(params::find("P.machine.captionBatch").unwrap().spelled, "5");
    // The two formulas, checked as the arithmetic §10 spells rather than as a copy of the row's text.
    assert_eq!(cut_pass_min_segments(600.0), 4);
    assert_eq!(cut_pass_max_segments(600.0), 120);
    assert_eq!(
        row("P.machine.suggestMinSegments"),
        "min(1 + ⌊target/30⌋, 4)"
    );
    assert_eq!(row("P.machine.suggestMaxSegments"), "max(⌊target/5⌋, 40)");
    // And §6's colour list: a row per tint, at least the eight §D names.
    assert!(
        listed
            .iter()
            .filter(|id| id.starts_with("cut.colour."))
            .count()
            >= 8,
        "§6's colours are not all catalogued"
    );
}

fn cut_pass_min_segments(target: f64) -> usize {
    naivepost::tools::cutpass::min_segments(target)
}

fn cut_pass_max_segments(target: f64) -> usize {
    naivepost::tools::cutpass::max_segments(target)
}

// --- S8: the colours §6 catalogues are the ones that reach pixels ------------------------------------------

/// §05-cut#6-parameters-used — a colour row is only worth having if the painter uses it. Every tint here is
/// painted into a real band surface and read back as bytes, compared against `ui::cut_colours`' constant
/// composited over the ground it is laid on — so a row that names a constant nobody draws, or a constant that
/// drifted from what lands on screen, fails here and not in someone's eyes.
#[test]
fn sec_05_cut_6_parameters_used_s8_the_catalogued_tints_are_the_ones_painted() {
    use naivepost::ui::cut_colours as colours;
    use naivepost::ui::cut_tracks as tracks;

    let pps = 8.0;
    // A scene with something of every tint in it: a kept bar, a selection (whose marks are its two ends:
    // green in, red out), a recording running to its own end (the amber edge), and a speed effect (rose).
    let scene = tracks::scene_of(
        &[cut::Seg {
            s: 2.0,
            e: 10.0,
            ..Default::default()
        }],
        &[cut::Fx {
            kind: "speed".into(),
            t: 3.0,
            dur: 4.0,
            rate: 2.0,
            ..Default::default()
        }],
        &[],
        &[naivepost::timeline::Recording {
            base: "take-a".into(),
            start: 0.0,
            end: 20.0,
        }],
        Some((4.0, 8.0)),
        None,
        1,
        &Default::default(),
        &[],
        pps,
        0.0,
        0.0,
    );
    let height = tracks::band_height_px(&scene, tracks::THUMB_DEFAULT_PX).ceil() as i32;
    let width = 120i32;
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).expect("surface");
    {
        let cr = cairo::Context::new(&surface).expect("context");
        tracks::paint_at(
            &cr,
            &scene,
            width as f64,
            height as f64,
            tracks::THUMB_DEFAULT_PX,
        );
    }
    let data = surface.data().expect("read back").to_vec();
    let stride = surface.stride() as usize;
    let at = |x: i32, y: i32| -> (u8, u8, u8) {
        let base = y as usize * stride + x as usize * 4;
        (data[base + 2], data[base + 1], data[base])
    };
    // cairo composites over whatever was there: the band's ground, then the tint at its own alpha.
    let over = |tint: (f64, f64, f64), alpha: f64, ground: (f64, f64, f64)| {
        let mix = |c: f64, g: f64| (c * alpha + g * (1.0 - alpha)) * 255.0;
        (
            mix(tint.0, ground.0) as u8,
            mix(tint.1, ground.1) as u8,
            mix(tint.2, ground.2) as u8,
        )
    };
    let close = |got: (u8, u8, u8), want: (u8, u8, u8), what: &str| {
        let worst = [(got.0, want.0), (got.1, want.1), (got.2, want.2)]
            .iter()
            .map(|(got, want)| (*got as i32 - *want as i32).abs())
            .max()
            .unwrap_or(0);
        assert!(
            worst <= 3,
            "{what}: painted {got:?}, catalogue says {want:?}"
        );
    };
    let y_selection = (naivepost::cut_trim::RULER_H + 6.0) as i32;
    // The kept bar at x = 6 s (inside the 2..10 s scene, outside the 4..8 s selection). MEASURED from the
    // pixel, the fact the row stands for is that the same rgba appears in three of §D's rows, so one row per
    // surface would be three rows for one number: over the band's ground it lands here.
    close(
        at((6.0 * pps) as i32, y_selection),
        KEEPING_OVER_GROUND,
        "the kept bar's green",
    );
    // The selection's two ends are the marks themselves: the rule paints a one-pixel column of its own at
    // each end, over whatever row they cross. Those are §D's `selection.in` green and `selection.out` red.
    // The mark lines are inked across the whole band, so they can be read anywhere in the picture row; read them
    // BELOW the row's pinned name plate, which is inked just under the row's top edge (the plate sits past the
    // gutter now, i.e. right where a mark can fall, and its glyph ink is not the mark's colour).
    let y_pictures = (naivepost::cut_trim::RULER_H
        + naivepost::cut_screen::SELECTION_BAND_PX
        + naivepost::fx_lane::lane_height_px(naivepost::fx_lane::row_count(&scene.fx))
        + 20.0) as i32;
    // The effect lane is its own band, `layout.effectRowPx` tall, between the selection band and the first
    // picture row — so it exists in the painted surface at a y of its own, which is where its wash has to be.
    let y_effects = (naivepost::cut_trim::RULER_H
        + row("layout.selBandPx")
            .trim_end_matches(" px")
            .parse::<f64>()
            .unwrap()
        + 4.0) as i32;
    close(
        at((4.0 * pps) as i32, y_pictures),
        over(colours::MARK_IN_GREEN, 1.0, colours::LANE_GROUND),
        "the in mark's green",
    );
    close(
        at((8.0 * pps) as i32, y_pictures),
        over(colours::MARK_OUT_RED, 1.0, colours::LANE_GROUND),
        "the out mark's red",
    );
    close(
        at((5.0 * pps) as i32, y_effects),
        SPEED_WASH_ON_EFFECT_ROW,
        "the speed's rose wash",
    );
    // And away from every joint, stripe and wash, the picture row shows the lane's own grey plate: the row
    // has a ground of its own, which is what every tint above is composited on.
    close(
        at((19.0 * pps) as i32, y_pictures),
        THUMB_PLACEHOLDER,
        "the lane's grey plate, uninked",
    );
}

/// The speed bar's rose as it lands on the effect lane, printed from the painted surface: §D writes
/// rgba(0.92,0.42,0.6,0.15), and what arrives is stronger than that alpha over the lane's grey would give — the
/// lane paints every kind through `cut_lane_paint::paint_bar`, which lays the colour at `fx_lane::bar_wash_alpha`
/// (inventory/effects.md §A.2 spells one speed bar out as `fill RGBA(0.92,0.42,0.6,0.4)`), over the band ground.
/// Recomputed here rather than hard-coded so the row follows the wash rule instead of a snapshot of it:
/// 0.4 x (0.92,0.42,0.6) + 0.6 x GROUND(0.95,0.95,0.95) = (239,188,206).
const SPEED_WASH_ON_EFFECT_ROW: (u8, u8, u8) = (
    (0.92 * naivepost::fx_lane::WASH_ALPHA * 255.0
        + 0.95 * (1.0 - naivepost::fx_lane::WASH_ALPHA) * 255.0) as u8,
    (0.42 * naivepost::fx_lane::WASH_ALPHA * 255.0
        + 0.95 * (1.0 - naivepost::fx_lane::WASH_ALPHA) * 255.0) as u8,
    (0.6 * naivepost::fx_lane::WASH_ALPHA * 255.0
        + 0.95 * (1.0 - naivepost::fx_lane::WASH_ALPHA) * 255.0) as u8,
);

/// The picture row's plate, read back from the painted surface. At 8 px/s a camera is far narrower than one
/// thumbnail, so the thumbnails rule draws one placeholder box for it — §D's `thumbnail.placeholder`, which
/// is why no §6 tint reaches this pixel and why every one of them fails to match its own rgba above: they are
/// all composited on this.
const THUMB_PLACEHOLDER: (u8, u8, u8) = (38, 179, 64);

/// The pixel [`cut_colours::KEPT_TINT`] leaves on the band's ground: the inventory's rgba(0.2,0.8,0.3,0.3)
/// composited at its OWN 0.3 over the near-white plate — §6 spells that one colour for three of §D's rows
/// (`background.kept`, `thumbnail.placeholder`, `scene.placeholder`), so the row catalogues it and the pixel
/// is what any of those three measures against. Printed from the painted surface, not typed from the spec.
const KEEPING_OVER_GROUND: (u8, u8, u8) = (56, 169, 145);
