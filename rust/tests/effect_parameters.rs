// §06-effects#6-parameters-used — spec/06-effects.md §6, checked against naivepost::params::effects() and the
// rows §05's list already carries.
//
// §6 is a list of ids, not of numbers, so what these tests pin is that the two cannot drift: every id §6 names
// either has a row built from the constant its rule reads, or is absent because no rule in this tree reads it —
// and the second case is asserted too, so a number arriving with nothing behind it is caught here rather than
// shipping. An id §05's §6 names as well keeps its single row there: one row, one home.

use naivepost::cut_hear;
use naivepost::cut_speed;
use naivepost::cut_speed_pass;
use naivepost::params;
use naivepost::tools::clips;
use naivepost::tools::cutpass;
use naivepost::{cut_cards, cut_clamp, cut_effects_pass};
use naivepost::{fx_band, fx_svg, fx_text, fx_volume, fx_zoom};

/// §06 §6's rows, in §6's order.
fn effect_rows() -> Vec<params::Param> {
    params::effects()
}

/// The list as ids only.
fn effect_ids() -> Vec<String> {
    effect_rows().into_iter().map(|row| row.id.to_string()).collect()
}

/// Every id this flow could be catalogued under — §06's own list, the §05 one it shares bounds with, and Prepare's,
/// which is where §10's `P.` ids are filed (`P.eng.maxGain`, `P.machine.captionBatch`). Asserting exactly one match
/// across all three is what keeps a row from being filed twice, once per page.
fn anywhere(id: &str) -> params::Param {
    let mut found = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .filter(|row| row.id == id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "{id} catalogued {} times", found.len());
    found.pop().unwrap()
}

/// One row of §06's list, by its id.
fn row(id: &str) -> params::Param {
    effect_rows()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{id} is in §06 §6's list and so must be catalogued"))
}

/// A number §6 writes with a decimal point (`0.05`, `4.0`) compared as a number: `params::num` trims the trailing
/// zero, so comparing strings would pin a spelling neither side chose.
fn number(id: &str) -> f64 {
    row(id).spelled.parse().unwrap_or_else(|_| panic!("{id} spells a number"))
}

/// The same for a row found through the chain.
fn number_anywhere(id: &str) -> f64 {
    anywhere(id)
        .spelled
        .parse()
        .unwrap_or_else(|_| panic!("{id} spells a number"))
}

// --- S1: what a speed or volume effect may be asked to do ----------------------------------------------------

/// §06-effects#6-parameters-used — `minRate, maxRate, minClipSeconds, rampStepSeconds`: the four clamps §6 opens
/// with, each holding its §10 value and naming the constant that holds it.
#[test]
fn sec_06_effects_6_parameters_used_s1_the_speed_and_volume_clamps_are_catalogued_with_their_values() {
    assert_eq!(number("P.eng.minRate"), 0.05);
    assert_eq!(number("P.eng.maxRate"), 100.0);
    assert_eq!(number("P.eng.minClipSeconds"), 0.5);
    assert_eq!(number("P.eng.rampStepSeconds"), 0.6);

    // A row that names no constant is a row nobody can follow back to its rule.
    assert_eq!(row("P.eng.minRate").from, "cut_speed::MIN_RATE");
    assert_eq!(row("P.eng.maxRate").from, "cut_speed::MAX_RATE");
    assert_eq!(row("P.eng.minClipSeconds").from, "tools::cutpass::MIN_CLIP_SECONDS");
    assert_eq!(row("P.eng.rampStepSeconds").from, "cut_speed::RAMP_STEP_SECONDS");

    // And the spelled number is the one the rule reads, not a copy that can drift: clamp_speed's floor and ceiling
    // come from cut_speed, the render's shortest clip from cutpass.
    assert_eq!(number("P.eng.minRate"), cut_speed::MIN_RATE);
    assert_eq!(number("P.eng.maxRate"), cut_speed::MAX_RATE);
    assert_eq!(number("P.eng.minClipSeconds"), cutpass::MIN_CLIP_SECONDS);
    assert_eq!(number("P.eng.rampStepSeconds"), cut_speed::RAMP_STEP_SECONDS);
}

// --- S2: the bounds §05's §6 names too ------------------------------------------------------------------------

/// §06-effects#6-parameters-used — `maxGain (10), speedGapSeconds, captionBatch, captionMinSeconds,
/// effectMinSurvivingSeconds`: named by both flows, catalogued once. Each is found through either list and
/// asserted absent from §06's own, which is what "one row, one home" means in practice.
#[test]
fn sec_06_effects_6_parameters_used_s2_the_bounds_shared_with_the_cut_page_are_catalogued_once() {
    assert_eq!(number_anywhere("P.eng.maxGain"), 10.0);
    assert_eq!(number_anywhere("P.policy.speedGapSeconds"), cut_speed_pass::GAP_SECONDS);
    assert_eq!(number_anywhere("P.machine.captionBatch"), 5.0);
    assert_eq!(number_anywhere("P.policy.captionMinSeconds"), clips::CAPTION_MIN_SECONDS);
    assert_eq!(number_anywhere("P.eng.effectMinSurvivingSeconds"), cut_clamp::MIN_SURVIVING_SECONDS);

    assert_eq!(anywhere("P.eng.maxGain").from, "cut_hear::MAX_GAIN");
    assert_eq!(cut_hear::MAX_GAIN, 10.0, "playbin's own ceiling is what §6 writes as maxGain (10)");

    let listed = effect_ids();
    for shared in [
        "P.eng.maxGain",
        "P.policy.speedGapSeconds",
        "P.machine.captionBatch",
        "P.policy.captionMinSeconds",
        "P.eng.effectMinSurvivingSeconds",
    ] {
        assert!(
            !listed.iter().any(|id| id == shared),
            "{shared} is catalogued for the Cut page already; a second row would be two homes for one bound"
        );
    }
}

// --- S3: the defaults one §10 row shares out ------------------------------------------------------------------

/// §06-effects#6-parameters-used — `default lengths (zoom/text/svg 3 s; label/speed/volume 2 s), default fades
/// (text/svg 0.3; volume 0.25; stop 0.5; zoom 1), default gain 2, default rate 0.5, suggested zoom hf 0.6`: the
/// cells §10 gives to a family of kinds, spelled as §10 spells them with every number behind them named.
#[test]
fn sec_06_effects_6_parameters_used_s3_the_family_defaults_are_spelled_as_section_10_spells_them() {
    let lengths = row("P.policy.effectDefaultSeconds");
    assert_eq!(lengths.spelled, "zoom/text/svg 3; stop/speed/volume/label 2");
    assert_eq!(fx_zoom::DEFAULT_SECONDS, 3.0);
    assert_eq!(fx_volume::LINE_SECONDS, 2.0);

    let fades = row("P.policy.effectDefaultFades");
    assert_eq!(fades.spelled, "zoom 1; text/svg 0.3; volume 0.25; stop 0.5");
    assert_eq!(fx_zoom::GLIDE_SECONDS, 1.0);
    assert_eq!(fx_text::FADE_SECONDS, 0.3);
    assert_eq!(fx_volume::FADE_SECONDS, 0.25);
    assert_eq!(cut_speed::STOP_FADE_SECONDS, 0.5);

    assert_eq!(number_anywhere("effects.defaultGain"), fx_volume::DEFAULT_GAIN);
    assert_eq!(number_anywhere("effects.defaultRate"), cut_speed::DEFAULT_RATE);

    // One constant answering two ids: §10's row and §F3.11's own name for the same height fraction. Both rows read
    // it, so the two spellings cannot disagree about what a proposed zoom's height is.
    let suggested = row("P.policy.suggestedZoomHeight");
    let proposed = anywhere("effects.proposedZoomHeight");
    assert_eq!(suggested.spelled, "0.6");
    assert_eq!(proposed.spelled, suggested.spelled);
    assert!(suggested.from.ends_with("cut_effects_pass::ZOOM_HEIGHT"), "{}", suggested.from);
    assert!(proposed.from.ends_with("cut_effects_pass::ZOOM_HEIGHT"), "{}", proposed.from);
    assert_eq!(cut_effects_pass::ZOOM_HEIGHT, 0.6);
}

// --- S4: the floors a form holds to ---------------------------------------------------------------------------

/// §06-effects#6-parameters-used — `forms' typed floors (zoom 0.4, text/svg 0.3, speed 0.5 for a stop else
/// clampSpeed, volume 0.1, label 0.4), the 0.2 s floor under which a band is not a marked stretch`: each floor the
/// form refuses a shorter number over, and the one §6 lists on its own.
#[test]
fn sec_06_effects_6_parameters_used_s4_the_forms_typed_floors_are_the_numbers_apply_holds_to() {
    assert_eq!(number("effects.zoomFloorSeconds"), fx_zoom::MIN_SECONDS);
    assert_eq!(number("effects.textMinSeconds"), fx_text::MIN_SECONDS);
    assert_eq!(number_anywhere("effects.volumeMinSeconds"), fx_volume::MIN_SECONDS);
    assert_eq!(number("effects.markedBandMinSeconds"), cut_speed::MIN_MARKED_SECONDS);

    // The stop's 0.5 gets no row of its own: §10 says outright that minClipSeconds is "also the speed clamp floor",
    // and cut_speed::apply holds a stop's Length to exactly that constant, so a second id would be two names for one
    // bound — and two knobs where the spec has one.
    assert_eq!(number("P.eng.minClipSeconds"), 0.5);
    let listed = effect_ids();
    assert!(
        !listed.iter().any(|id| id.contains("stopMin")),
        "a stop's floor is P.eng.minClipSeconds and must not be catalogued twice"
    );
}

// --- S5: what §6 names that is not here, and why --------------------------------------------------------------

/// §06-effects#6-parameters-used — the drawing numbers §6's Engineering sentence lists ("lane height 26, grip/kill
/// widths, snap 8/10 px, … svg preview 512 px, card constants, bake fps") against the two items it names that have
/// no row: the label form's floor and the decorations density. A number nothing reads has no module to live in, so
/// asserting their absence is what stops one arriving as a decoration.
#[test]
fn sec_06_effects_6_parameters_used_s5_the_drawing_numbers_are_here_and_the_unowned_items_are_not() {
    assert_eq!(number_anywhere("layout.effectRowPx"), 26.0);
    // §6's "snap 8/10 px" is a pair on two surfaces: the lane's 8 px pulls a band onto a word edge, the preview's
    // 10 px drags a text box across a picture. Both rows exist and they are not the same rule.
    assert_eq!(number_anywhere("layout.snapPx"), 8.0);
    assert_eq!(number("effects.snapPx"), fx_text::SNAP_PX);
    assert_eq!(number_anywhere("effects.previewRasterPx"), fx_svg::PREVIEW_RASTER_PX);

    // §06-effects#5's card numbers, spelled as the spec writes them.
    assert_eq!(number_anywhere("card.stillnessSeconds"), cut_cards::STILLNESS_SECONDS);
    assert_eq!(number_anywhere("card.bakeFps"), 25.0);
    assert_eq!(anywhere("card.canvas").spelled, format!("{}×{}", cut_cards::CANVAS.0, cut_cards::CANVAS.1));

    let listed: Vec<String> = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .map(|param| param.id.to_string())
        .collect();

    // F3.7's label form floor (0.4) DOES have a constant now -- `fx_label::MIN_SECONDS` -- and stays out of §6's
    // catalogue because §10 gives it no `P.` row on purpose (see the note above `params::effects`): nothing tunes
    // it, the form only refuses under it.
    for absent in ["labelMin", "labelFloor"] {
        assert!(
            !listed.iter().any(|id| id.contains(absent)),
            "{absent} would catalogue a floor §10 deliberately leaves uncatalogued"
        );
    }
    // The decorations density is §F3.11's prompt wording ("few and deliberate: three or four across five minutes"),
    // not a value this app multiplies anything by.
    assert!(
        !listed.iter().any(|id| id.contains("density")),
        "the density lives in prompts/effects.md as wording, not as a number"
    );
    // §I's killMin 32, fxMinBand 30 and fxGrab 9: no constant here holds them. Matched case-sensitively on whole
    // names — `layout.edgeGrabPx` already contains "grab", and the 10 px hit radius is §10's list's business.
    // F3.8's kill width WAS in this list as a number nothing read -- §A.9 states it, no module used it. That is
    // no longer true: `fx_band::KILL_MIN_PX` is the rule that decides whether a bar carries a ✕, so the row is
    // catalogued and asserted against the constant instead of asserted absent.
    assert_eq!(number_anywhere("effects.killMinPx"), fx_band::KILL_MIN_PX);
    for absent in ["bandMin", "grabReach"] {
        assert!(
            !listed.iter().any(|id| id.contains(absent)),
            "{absent} has no constant reading it and so must not be catalogued"
        );
    }
}

// --- S6: the list's own shape ----------------------------------------------------------------------------------

/// §06-effects#6-parameters-used — no id twice, every row naming where its value lives, each prefix answering to
/// the family §10 files it under, and §6's own order kept.
#[test]
fn sec_06_effects_6_parameters_used_s6_every_row_names_where_its_value_lives() {
    let listed = effect_ids();
    for id in &listed {
        assert_eq!(listed.iter().filter(|other| *other == id).count(), 1, "{id} catalogued twice");
    }
    for param in effect_rows() {
        assert!(!param.from.is_empty(), "{} names no constant", param.id);
        assert!(param.from.contains("::"), "{}: {} is not module::CONST", param.id, param.from);
        assert!(!param.spelled.is_empty(), "{} spells nothing", param.id);
    }

    // The families §10 files the ids under — including that this flow's unrowed numbers answer to no family.
    assert_eq!(params::family("P.eng.minRate"), params::Family::Eng);
    assert_eq!(params::family("P.policy.effectDefaultSeconds"), params::Family::Policy);
    assert_eq!(params::family("P.machine.captionBatch"), params::Family::Machine);
    assert_eq!(params::family("effects.zoomFloorSeconds"), params::Family::Other);
    assert_eq!(params::family("layout.gripPx"), params::Family::Other);

    // §6's own order, as far as it goes: the clamps first, then the defaults and the suggested height, then the
    // floors, then what the lane and the picture are drawn with.
    let at = |id: &str| listed.iter().position(|row| row == id).unwrap();
    let order = [
        "P.eng.minRate",
        "P.eng.maxRate",
        "P.eng.minClipSeconds",
        "P.eng.rampStepSeconds",
        "P.policy.effectDefaultSeconds",
        "P.policy.effectDefaultFades",
        "P.policy.suggestedZoomHeight",
        "effects.zoomFloorSeconds",
        "effects.textMinSeconds",
        "effects.markedBandMinSeconds",
        "layout.gripPx",
        "effects.snapPx",
        "effects.textAdvance",
        "effects.textMaxLines",
        "effects.edgeRadius",
    ];
    for pair in order.windows(2) {
        assert!(at(pair[0]) < at(pair[1]), "{} should come before {}", pair[0], pair[1]);
    }
}
