// §10-parameters · P.eng.effectMinSelectionSeconds — the 0.2 s of timeline that has to be under the band before
// ⏩ Speed treats it as a chosen stretch rather than a click that slipped (prototype `fxMinSel`, gui/cut_fx.go).
//
// The rule itself is not new: `cut_speed::MIN_MARKED_SECONDS` and its reading in `cut_speed::press` were written
// for F3.3 S1, and 🔊 Volume borrows the same floor. What this file pins is that the §10 row points at that rule
// rather than at a copy of the number, that the boundary behaves as "not below the floor" instead of "strictly
// above" (0.2 is not representable in binary, so a band dragged to exactly the floor can come out a ulp under),
// and that this bound stays distinct from the four other floors that also sit under an effect's length.

use naivepost::fx_lane;
use naivepost::{cut_clamp, cut_select, cut_speed, fx_volume, params};

/// Every id this value could be filed under, searched across the three lists that carry `P.` rows. Asserting
/// exactly one match is what keeps a row from being catalogued twice, once per page. (Copied from
/// `tests/effect_parameters.rs`, where the same helper plays the same role.)
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

/// A number §10 writes with a decimal point compared as a number: `params::num` trims the trailing zero, so
/// comparing strings would pin a spelling neither side chose.
fn number_anywhere(id: &str) -> f64 {
    anywhere(id)
        .spelled
        .parse()
        .unwrap_or_else(|_| panic!("{id} spells a number"))
}

/// A dragged second compared as a second, not as a bit pattern: 0.2 is not representable in binary, so a band's
/// width lands a ulp either side of it. What matters is whether the press counted the band or threw it away as a
/// slipped click, which 1e-9 s cannot tell apart from a real difference.
fn assert_close(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: got {got}, want {want} (±1e-9)"
    );
}

/// S1: `P.eng.effectMinSelectionSeconds` = 0.2, held by `cut_speed::MIN_MARKED_SECONDS`, catalogued once under
/// the family §10 files it in.
#[test]
fn p_eng_effectminselectionseconds_s1_the_selection_floor_is_0_2_and_catalogued_once() {
    // P.eng.effectMinSelectionSeconds — "timeline needed under the band before ⏩ Speed treats it as a chosen
    // stretch".
    assert_eq!(cut_speed::MIN_MARKED_SECONDS, 0.2);
    assert_eq!(number_anywhere("P.eng.effectMinSelectionSeconds"), 0.2);

    // The row names the constant its rule reads, not a copy of the number.
    assert_eq!(
        anywhere("P.eng.effectMinSelectionSeconds").from,
        "cut_speed::MIN_MARKED_SECONDS"
    );

    // §10 §5.1 files it among the engineering constants, so the prefix must answer to Eng.
    assert_eq!(
        params::family("P.eng.effectMinSelectionSeconds"),
        params::Family::Eng
    );

    // And it lives in the effects list specifically — the marked-band rule is §06's, not the Cut page's.
    assert!(
        params::effects()
            .iter()
            .any(|row| row.id == "P.eng.effectMinSelectionSeconds"),
        "the selection floor belongs to §06-effects#6's list"
    );
}

/// S2: a band over the floor IS the chosen stretch, and it wins over the red line — ⏩ Speed slows the marked
/// seconds rather than dropping a stop at the line when the hand has marked something.
#[test]
fn p_eng_effectminselectionseconds_s2_a_band_over_the_floor_is_the_chosen_stretch() {
    // P.eng.effectMinSelectionSeconds: 15 s marked, a line also present — the band answers.
    assert_eq!(
        cut_speed::press(Some((30.0, 45.0)), Some(99.0)),
        cut_speed::Pressed::Stretched { t: 30.0, dur: 15.0 }
    );

    // A right-to-left drag is the same stretch: the ends are ordered before they are measured, so the effect
    // starts at 30 and runs 15 s either way the band was drawn.
    assert_eq!(
        cut_speed::press(Some((45.0, 30.0)), Some(99.0)),
        cut_speed::Pressed::Stretched { t: 30.0, dur: 15.0 }
    );
}

/// S3: under the floor there is no band at all. The press falls through to the line, and with no line it refuses —
/// so nothing is placed over a tenth of a second nobody meant.
#[test]
fn p_eng_effectminselectionseconds_s3_under_the_floor_is_a_slipped_click() {
    // P.eng.effectMinSelectionSeconds: 0.1 s marked is half the floor, so it is not a stretch even though a
    // stretch was on screen.
    assert_ne!(
        cut_speed::press(Some((30.0, 30.1)), Some(99.0)),
        cut_speed::Pressed::Stretched { t: 30.0, dur: 0.1 },
        "a tenth of a second is not a chosen stretch"
    );
    assert_eq!(
        cut_speed::press(Some((30.0, 30.1)), Some(99.0)),
        cut_speed::Pressed::StopAtLine
    );

    // No line either → the refusal, and nothing is written for it.
    assert_eq!(
        cut_speed::press(Some((30.0, 30.1)), None),
        cut_speed::Pressed::Refused
    );
    assert_eq!(cut_speed::initial(cut_speed::Pressed::Refused), None);
}

/// S4: the boundary is "not below the floor", not "strictly above it". A band exactly as wide as the floor
/// counts, because `to - from` landing a ulp under 0.2 must not throw away a stretch the hand really dragged;
/// 0.199 s is clearly under and does not count.
#[test]
fn p_eng_effectminselectionseconds_s4_the_boundary_is_not_below_rather_than_strictly_above() {
    // P.eng.effectMinSelectionSeconds: exactly the floor's width.
    let exact = cut_speed::press(Some((30.0, 30.0 + cut_speed::MIN_MARKED_SECONDS)), None);
    match exact {
        cut_speed::Pressed::Stretched { t, dur } => {
            assert_eq!(t, 30.0);
            assert_close("a band exactly at the floor", dur, cut_speed::MIN_MARKED_SECONDS);
        }
        other => panic!("a band at the floor should be a chosen stretch, got {other:?}"),
    }

    // One ulp under the floor still counts — that is what the tolerance in `press` is for.
    let just_under = 30.0 + cut_speed::MIN_MARKED_SECONDS.next_down();
    assert!(
        matches!(
            cut_speed::press(Some((30.0, just_under)), None),
            cut_speed::Pressed::Stretched { .. }
        ),
        "a ulp under the floor is the same drag, not a click"
    );

    // Clearly under: 0.199 s is a thousandth short of the floor, far beyond any float slack, so the press falls
    // through — to the line here, and to the refusal without one.
    assert_eq!(
        cut_speed::press(Some((30.0, 30.199)), Some(99.0)),
        cut_speed::Pressed::StopAtLine
    );
    assert_eq!(
        cut_speed::press(Some((30.0, 30.199)), None),
        cut_speed::Pressed::Refused
    );
}

/// S5: two ids, one bound — and this bound is not any of the four others that sit under an effect's length.
#[test]
fn p_eng_effectminselectionseconds_s5_one_bound_two_rows_and_four_other_floors() {
    // §10's `P.` row and §06 §6's own bare row read ONE constant, so the two spellings cannot drift apart.
    assert_eq!(
        number_anywhere("P.eng.effectMinSelectionSeconds"),
        number_anywhere("effects.markedBandMinSeconds")
    );
    assert_eq!(
        anywhere("P.eng.effectMinSelectionSeconds").from,
        anywhere("effects.markedBandMinSeconds").from,
        "both rows must point at the same constant, not two copies of 0.2"
    );

    // effects.volumeMinSeconds (fx_volume::MIN_SECONDS, 0.1): the volume FORM's typed Length floor — what Apply
    // refuses a shorter typed number over. Not the same question as whether the hand dragged.
    assert_eq!(fx_volume::MIN_SECONDS, 0.1);
    assert_ne!(
        fx_volume::MIN_SECONDS, cut_speed::MIN_MARKED_SECONDS,
        "the form's typed floor is not the marked-band floor"
    );

    // P.eng.minPieceSeconds (cut_select::MIN_SECONDS, 0.04): is a remainder of footage worth keeping at all.
    assert_eq!(cut_select::MIN_SECONDS, 0.04);
    assert_ne!(
        cut_select::MIN_SECONDS, cut_speed::MIN_MARKED_SECONDS,
        "worth-keeping is not meant-to-drag"
    );

    // P.eng.effectMinSeconds (fx_lane::MIN_BAND_SECONDS, 0.1): how short a band may be DRAGGED down to. Same
    // digits as the volume form's floor, different rule again, and neither is this one.
    assert_eq!(fx_lane::MIN_BAND_SECONDS, 0.1);
    assert_ne!(
        fx_lane::MIN_BAND_SECONDS, cut_speed::MIN_MARKED_SECONDS,
        "the drag floor is not the selection floor"
    );

    // P.eng.effectMinSurvivingSeconds (cut_clamp::MIN_SURVIVING_SECONDS, 1.0): what a clamped band must have
    // left or it is dropped outright.
    assert_eq!(cut_clamp::MIN_SURVIVING_SECONDS, 1.0);
    assert_ne!(
        cut_clamp::MIN_SURVIVING_SECONDS, cut_speed::MIN_MARKED_SECONDS,
        "the drop threshold is not the selection floor"
    );

    // And volume really does borrow THIS floor rather than its own typed one: a 0.1 s band is long enough for the
    // volume form's Length (fx_volume::MIN_SECONDS), yet too short to be a marked stretch, so the press falls
    // through to the line. The two floors are different numbers precisely so that neither can stand in for the
    // other — which is what makes the borrowing observable here.
    assert!(
        fx_volume::MIN_SECONDS < cut_speed::MIN_MARKED_SECONDS,
        "the form's floor must stay under the selection floor or this test proves nothing"
    );
    assert_ne!(
        fx_volume::press(Some((30.0, 30.1)), Some(99.0)),
        fx_volume::Pressed::Stretched { t: 30.0, dur: 0.1 },
        "volume borrows cut_speed::MIN_MARKED_SECONDS, not its own 0.1, to decide what a marked stretch is"
    );
    assert_eq!(
        fx_volume::press(Some((30.0, 30.1)), Some(99.0)),
        fx_volume::Pressed::LoudAtLine
    );
    // Over the floor, volume takes the band's seconds exactly as speed does.
    assert_eq!(
        fx_volume::press(Some((30.0, 34.0)), None),
        fx_volume::Pressed::Stretched { t: 30.0, dur: 4.0 }
    );
}
