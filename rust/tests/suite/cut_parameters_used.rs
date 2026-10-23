// §05-cut#6-parameters-used — spec/05-cut.md §6, checked against naivepost::params::cut().
//
// §6 is a list of ids, not of numbers, so what these tests pin is that the two cannot drift: every id §6 names
// either has a row built from the constant its rule reads, or is absent because no rule in this tree reads it
// yet — and the second case is asserted too, so a number appearing with nothing behind it is caught here rather
// than shipping.

use naivepost::cut;
use naivepost::params;
use naivepost::tools::cutpass;
use naivepost::ui::cut_colours;
use naivepost::wave;

/// §6's rows, in §6's order.
fn rows() -> Vec<params::Param> {
    params::cut()
}

/// The list as ids only.
fn ids() -> Vec<String> {
    rows().into_iter().map(|row| row.id.to_string()).collect()
}

/// One row by its §6 id.
fn row(id: &str) -> params::Param {
    rows()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{id} is in §6's list and so must be catalogued"))
}

/// A number §6 writes with a decimal point (`1.0`, `5.0`) compared as a number: `params::num` trims the trailing
/// zero, so comparing strings would pin a spelling neither side chose.
fn number(id: &str) -> f64 {
    row(id)
        .spelled
        .parse()
        .unwrap_or_else(|_| panic!("{id} spells a number"))
}

// --- S1: what bounds an edit -----------------------------------------------------------------------------

/// §05-cut#6-parameters-used — `minSceneSeconds (1.0), minPieceSeconds (0.04), snapToleranceSeconds (5.0) …
/// reviewPadSeconds (10)`: the four bounds a hand meets on the band, each holding §6's value and naming the
/// constant that holds it.
#[test]
fn sec_05_cut_6_parameters_used_s1_what_bounds_an_edit_is_catalogued_with_its_value() {
    assert_eq!(number("P.policy.minSceneSeconds"), 1.0);
    assert_eq!(number("P.eng.minPieceSeconds"), 0.04);
    assert_eq!(number("P.policy.snapToleranceSeconds"), 5.0);
    assert_eq!(number("P.policy.reviewPadSeconds"), 10.0);

    // A row that names no constant is a row nobody can follow back to its rule.
    assert_eq!(
        row("P.policy.minSceneSeconds").from,
        "cut_select::MIN_SCENE_SECONDS"
    );
    assert_eq!(row("P.eng.minPieceSeconds").from, "cut_select::MIN_SECONDS");
    assert_eq!(
        row("P.policy.snapToleranceSeconds").from,
        "tools::cutpass::SNAP_TOLERANCE_SECONDS"
    );
    assert_eq!(
        row("P.policy.reviewPadSeconds").from,
        "cut_review::REVIEW_PAD_SECONDS"
    );

    // §6's `targetLengthSeconds (0 = none)` in §10's own spelling — the default is not a very short video.
    assert_eq!(row("P.policy.targetLengthSeconds").spelled, "0 (none)");
}

// --- S2: what the target arithmetic computes on -----------------------------------------------------------

/// §05-cut#6-parameters-used — `suggestMinSegments/maxSegments formulas, footageWindow factors`: three rows with
/// no single number to spell, so they carry §10's formula text and name the function that computes it. The
/// values computed are checked against those functions, which is what keeps a formula row honest: a row quoting
/// `⌊target/5⌋` beside a function doing `/3` would read fine and mean something else.
#[test]
fn sec_05_cut_6_parameters_used_s2_the_target_arithmetic_rows_spell_the_formulas() {
    assert_eq!(
        row("P.machine.suggestMinSegments").spelled,
        "min(1 + \u{230a}target/30\u{230b}, 4)",
        "§10's own text"
    );
    assert_eq!(
        row("P.machine.suggestMaxSegments").spelled,
        "max(\u{230a}target/5\u{230b}, 40)"
    );
    assert_eq!(
        row("P.machine.footageWindow").spelled,
        "target \u{00d7} [0.6, 1.2] (\u{2264} shortTargetSeconds) else [0.6, 1.5]; ceiling \u{00d7} maxSpeedRate"
    );
    assert_eq!(
        row("P.machine.suggestMinSegments").from,
        "tools::cutpass::min_segments"
    );
    assert_eq!(
        row("P.machine.suggestMaxSegments").from,
        "tools::cutpass::max_segments"
    );
    assert_eq!(
        row("P.machine.footageWindow").from,
        "tools::cutpass::footage_window"
    );

    // The formulas as the functions work them: a 30 s target is a format, a 120 s one is a wish.
    assert_eq!(cutpass::min_segments(30.0), 2);
    assert_eq!(cutpass::max_segments(30.0), 40);
    assert_eq!(cutpass::footage_window(30.0), (18.0, 144.0));
    assert_eq!(
        cutpass::footage_window(120.0),
        (72.0, 720.0),
        "the ceiling is × maxSpeedRate"
    );

    // The factors and the ceiling the formulas are built from.
    assert_eq!(number("P.policy.shortTargetSeconds"), 60.0);
    // P.policy.maxSpeedRate = 4: footage over the ceiling could not be squeezed into the target at any rate.
    assert_eq!(cutpass::MAX_RATE, 4.0);
    assert_eq!(number("P.policy.maxSpeedRate"), cutpass::MAX_RATE);

    // P.policy.targetLengthSeconds = 0 (none): no target collapses the window rather than aiming at nothing.
    assert_eq!(cutpass::footage_window(0.0), (0.0, 0.0));

    // The dead-air pair and the seam bound, read by suggest's S2a/S3 walk-back from src/suggest.rs.
    assert_eq!(number("P.policy.deadAirMaxSeconds"), 8.0);
    assert_eq!(number("P.policy.deadAirKeepSeconds"), 0.5);
    assert_eq!(number("P.eng.seamMaxSeconds"), 1.5);

    // P.policy.insertDefaultSeconds = 4: a card with no length of its own.
    assert_eq!(naivepost::cut_insert::DEFAULT_SECONDS, 4.0);
    assert_eq!(
        number("P.policy.insertDefaultSeconds"),
        naivepost::cut_insert::DEFAULT_SECONDS
    );
}

// --- S3: what the preview does ----------------------------------------------------------------------------

/// §05-cut#6-parameters-used — `P.preview: playTick 100 ms … card fps 8`. §6 heads these `P.preview` and §10 has
/// no such family (they are `spec/inventory/cut.md` §D's numbers), so the rows take a bare prefix exactly as
/// `machine.jpegQuality` does rather than inventing ids §10 does not carry.
#[test]
fn sec_05_cut_6_parameters_used_s3_the_preview_numbers_are_catalogued_without_inventing_a_family() {
    assert_eq!(row("preview.playTickMs").spelled, "100 ms");
    assert_eq!(row("preview.playTickMs").from, "preview::TICK_MS");
    assert_eq!(number("preview.cardFps"), 8.0);
    assert_eq!(row("preview.cardFps").from, "cut_insert::PREVIEW_FPS");

    let invented: Vec<String> = ids()
        .into_iter()
        .filter(|id| id.starts_with("P.preview."))
        .collect();
    assert!(
        invented.is_empty(),
        "§10 has no P.preview family: {invented:?}"
    );
}

// --- S4: the engineering group ------------------------------------------------------------------------------

/// §05-cut#6-parameters-used — `Engineering: pixel reaches (6, 8, 10, 12 px), band heights, zoom limits, undo
/// depth, waveform cache format`: what the page draws with, catalogued from the same constants its drawing reads.
#[test]
fn sec_05_cut_6_parameters_used_s4_the_engineering_group_is_the_constants_the_page_draws_with() {
    // Three of §6's four pixel reaches: grab a border, snap to an edge, reach for the red line. The fourth (§I's
    // 10 px hit radius) is named in s5 as absent — no rule in this tree owns it yet.
    assert_eq!(number("layout.edgeGrabPx"), 6.0);
    assert_eq!(number("layout.snapPx"), 8.0);
    assert_eq!(number("layout.lineReachPx"), 12.0);

    // Band heights, top to bottom of a row, plus the gap between rows.
    let bands: Vec<(String, f64)> = [
        "layout.rulerPx",
        "layout.selBandPx",
        "layout.effectRowPx",
        "layout.waveLanePx",
    ]
    .iter()
    .map(|id| (id.to_string(), number(id)))
    .collect();
    assert_eq!(
        bands,
        vec![
            ("layout.rulerPx".to_string(), 18.0),
            ("layout.selBandPx".to_string(), 22.0),
            ("layout.effectRowPx".to_string(), 26.0),
            ("layout.waveLanePx".to_string(), 30.0),
        ]
    );
    assert_eq!(number("layout.laneGapPx"), 3.0);

    // Zoom limits: where the page opens, how deep it goes, what one step of the wheel is.
    assert_eq!(number("layout.zoomAtOpen"), 4.0);
    assert_eq!(number("layout.zoomMaxPps"), 240.0);
    assert_eq!(number("layout.zoomStep"), 1.25);

    // Undo depth, and the waveform cache's format — its magic, because a file not starting with these four bytes
    // is decoded again rather than trusted, which is why it was renamed AWV2 → AWV3 → AWV4.
    assert_eq!(row("layout.undoDepth").spelled, "50");
    assert_eq!(row("layout.undoDepth").spelled, cut::UNDO_DEPTH.to_string());
    assert_eq!(row("layout.waveCacheMagic").spelled, "AWV4");
    assert_eq!(
        row("layout.waveCacheMagic").spelled,
        std::str::from_utf8(wave::MAGIC).unwrap()
    );

    // §6's last engineering item is its colour list, and each row answers with the tuple the painter uses — so a
    // row that drifted from `cut_colours`, or from what `spec/inventory/cut.md` §D names, shows up here as a
    // number rather than as a wrong pixel. The kept tint is the one the inventory spells in full (RGBA(0.2,0.8,
    // 0.3,0.3)), which pins both halves of it: the ink and the alpha it is laid down at.
    assert_eq!(row("cut.colour.keptTint").spelled, "0.2 0.8 0.3");
    assert_eq!(cut_colours::KEPT_TINT_ALPHA, 0.3);
    for (id, rgb) in [
        ("cut.colour.keptTint", cut_colours::KEPT_TINT),
        ("cut.colour.selectionBlue", cut_colours::SELECTION_BLUE),
        ("cut.colour.insertViolet", cut_colours::INSERT_VIOLET),
        ("cut.colour.markInGreen", cut_colours::MARK_IN_GREEN),
        ("cut.colour.markOutRed", cut_colours::MARK_OUT_RED),
        ("cut.colour.edgeAmber", cut_colours::EDGE_AMBER),
        ("cut.colour.scrim", cut_colours::SCRIM),
        ("cut.colour.gutterGround", cut_colours::GUTTER_GROUND),
        ("cut.colour.gutterEdge", cut_colours::GUTTER_EDGE),
        ("cut.colour.speedTint", cut_colours::SPEED_TINT),
        ("cut.colour.playhead", cut_colours::PLAYHEAD_RGB),
    ] {
        assert_eq!(
            row(id).spelled,
            cut_colours::spell(rgb),
            "{id} is not the tint being painted"
        );
    }
}

// --- S5: what is deferred ------------------------------------------------------------------------------------

/// §05-cut#6-parameters-used — what is still deferred, and the three that are NOT any more. A number nothing
/// reads has no module to live in, which is the rule this catalogue exists to keep; each of the three below
/// gained a reader this round, so each now has a row built from the constant its rule reads — asserting that
/// (with the value §6 states) is what keeps a row from being a decoration:
/// `P.eng.talkPadSeconds` (0.2) by [`naivepost::suggest::hole_has_speech`], which decides whether a hole the
/// model dropped had speech in it; `preview.rateSeekGapMs` (250 ms) by [`naivepost::cut_play_back`]'s rate
/// door, which refuses a second re-cue inside the gap; and `preview.thumbBatch` (6) by the band's own frame
/// window. (`deadAirMaxSeconds`, `deadAirKeepSeconds` and `seamMaxSeconds` left the deferred list when F2.14
/// wrote the walk-back that reads them; §10's colour list left it when `ui::cut_colours` gave every tint a
/// constant the painter reads, so they are catalogued under `cut.colour.*`.)
#[test]
fn sec_05_cut_6_parameters_used_s5_what_is_deferred_is_absent_and_said_so() {
    let listed = ids();
    assert!(
        listed.iter().any(|id| id.contains("talkPad")),
        "talkPad lost its rule again"
    );
    assert!(
        listed.iter().any(|id| id.contains("rateSeekGap")),
        "rateSeekGap lost its rule again"
    );
    assert!(
        listed.iter().any(|id| id.contains("thumbBatch")),
        "thumbBatch lost its rule again"
    );
    assert_eq!(number("P.eng.talkPadSeconds"), 0.2);
    assert_eq!(row("preview.rateSeekGapMs").spelled, "250 ms");
    assert_eq!(number("preview.thumbBatch"), 6.0);

    // §6's `10 px` hit reach: §I gives it as a radius on two widgets, no constant owns it.
    assert!(
        !listed.iter().any(|id| id.contains("hit")),
        "no rule owns the 10 px hit radius"
    );
    // Its colours DO have rows now, because `ui::cut_colours` holds each one and the painters read it: §10 lists
    // them among the timeline's engineering values, so a tuning pass can move a tint from one place. Every colour
    // row must name the constant it came from, not merely repeat a tuple nobody consults.
    let colours: Vec<String> = listed
        .iter()
        .filter(|id| id.starts_with("cut.colour."))
        .cloned()
        .collect();
    assert!(
        colours.len() >= 8,
        "§6's colour list needs a row per tint the page paints, found {}",
        colours.len()
    );
    for param in rows() {
        if !param.id.starts_with("cut.colour.") {
            continue;
        }
        assert!(
            param.from.starts_with("ui::cut_colours::"),
            "{} does not name the colour constant the painter reads",
            param.id
        );
    }
}

// --- S6: housekeeping ------------------------------------------------------------------------------------------

/// §05-cut#6-parameters-used — the list's own shape: no id twice (one row, one home), every row naming where its
/// value lives, and each prefix answering to the family §10 files it under.
#[test]
fn sec_05_cut_6_parameters_used_s6_every_row_names_where_its_value_lives() {
    let listed = ids();
    for id in &listed {
        assert_eq!(
            listed.iter().filter(|other| *other == id).count(),
            1,
            "{id} catalogued twice"
        );
    }
    for param in rows() {
        assert!(!param.from.is_empty(), "{} names no constant", param.id);
        assert!(
            param.from.contains("::"),
            "{}: {} is not module::CONST",
            param.id,
            param.from
        );
        assert!(!param.spelled.is_empty(), "{} spells nothing", param.id);
    }

    // Families by prefix — including that §6's preview and layout rows claim no `P.` prefix they have not earned.
    assert_eq!(
        params::family("P.policy.minSceneSeconds"),
        params::Family::Policy
    );
    assert_eq!(
        params::family("P.machine.footageWindow"),
        params::Family::Machine
    );
    assert_eq!(params::family("P.eng.minPieceSeconds"), params::Family::Eng);
    assert_eq!(params::family("preview.playTickMs"), params::Family::Other);
    assert_eq!(params::family("layout.undoDepth"), params::Family::Other);

    // §6's own order is kept: the edit bounds come first, the cache format ends the engineering group, and only
    // §6's colours follow it (they are the last thing §6's sentence lists).
    assert_eq!(listed.first().unwrap(), "P.policy.minSceneSeconds");
    assert_eq!(
        listed.last().unwrap(),
        "cut.colour.playhead",
        "the colour rows go last, after the engineering group"
    );
    let at = |id: &str| listed.iter().position(|row| row == id).unwrap();
    // §6's groups in order: the bounds it starts with, then the target arithmetic, then preview, then drawing.
    assert!(at("P.policy.reviewPadSeconds") < at("P.machine.footageWindow"));
    assert!(at("preview.playTickMs") < at("layout.edgeGrabPx"));
    assert!(at("layout.edgeGrabPx") < at("layout.waveCacheMagic"));
    assert!(at("layout.waveCacheMagic") < at("cut.colour.keptTint"));
}
