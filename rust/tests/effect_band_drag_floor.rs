// §10-parameters · P.eng.effectMinSeconds — the shortest an effect band may be dragged down to (0.1), and the
// drag rule that holds it: `fx_lane::MIN_BAND_SECONDS` read by `fx_lane::drag_end`, catalogued by one row in
// `params::effects()`.
//
// The value alone is nothing without the arithmetic behind it, so these tests pin three things at once: that the
// constant holds §10's number, that a row points back at it (so the catalogue cannot drift from the rule), and
// that dragging either end of a band stops AT the floor rather than through it — no inverted band, no start
// before second 0. The last group keeps this bound apart from the three other rules that also bound a length from
// below, two of which share its number or its name by coincidence.

use naivepost::fx_lane;
use naivepost::{cut_clamp, cut_select, fx_volume, params};

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

/// A dragged second compared as a second, not as a bit pattern: the drag is clamped against sums like
/// `t + dur`, whose float result sits a ulp either side of the exact value. What matters is that the band lands
/// ON the floor and not under it, which 1e-9 s (a nanosecond, far below any frame or any pixel's worth of
/// time at the deepest zoom) cannot tell apart from a real difference.
fn assert_close(what: &str, got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-9,
        "{what}: got {got}, want {want} (±1e-9)"
    );
}

/// S1: `P.eng.effectMinSeconds` = 0.1, held by `fx_lane::MIN_BAND_SECONDS`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_effectminseconds_s1_the_floor_is_0_1_and_catalogued_once() {
    // P.eng.effectMinSeconds — "shortest an effect band may be dragged down to".
    assert_eq!(fx_lane::MIN_BAND_SECONDS, 0.1);
    assert_eq!(number_anywhere("P.eng.effectMinSeconds"), 0.1);

    // The row names the constant its rule reads, not a copy of the number.
    assert_eq!(anywhere("P.eng.effectMinSeconds").from, "fx_lane::MIN_BAND_SECONDS");

    // §10 §5.1 files it among the engineering constants, so the prefix must answer to Eng.
    assert_eq!(params::family("P.eng.effectMinSeconds"), params::Family::Eng);

    // And it lives in the effects list specifically — the lane is §06's, not the Cut page's.
    let in_effects = params::effects()
        .iter()
        .any(|row| row.id == "P.eng.effectMinSeconds");
    assert!(in_effects, "the drag floor belongs to §06-effects#6's list");
}

/// S2: dragging the TRAILING end below the floor stops at it. The start holds, the stop cannot come nearer than
/// 0.1 s, and the band never crosses over into a negative length.
#[test]
fn p_eng_effectminseconds_s2_dragging_the_trailing_end_below_the_floor_stops_at_it() {
    // P.eng.effectMinSeconds: a band at (10.0, 4.0) whose trailing end is dragged almost home.
    let (t, dur) = fx_lane::drag_end(10.0, 4.0, true, 10.02);
    assert_eq!(t, 10.0, "the stationary end does not move");
    assert_close("trailing drag to 10.02", dur, fx_lane::MIN_BAND_SECONDS);

    // Dragged all the way onto its own start: still the floor, still the same direction. An end dragged past its
    // own still stops at the shortest band there is instead of crossing over and inverting the selection.
    let (t_zero, dur_zero) = fx_lane::drag_end(10.0, 4.0, true, 10.0);
    assert_eq!(t_zero, 10.0);
    assert_close("trailing drag onto its own start", dur_zero, fx_lane::MIN_BAND_SECONDS);
    assert!(dur_zero > 0.0, "a dragged band never has zero or negative length");
    assert!(t_zero + dur_zero > t_zero, "and never runs backwards");
}

/// S3: dragging the LEADING end stops at the floor measured from the stationary trailing end, and never leaves
/// second 0 (§06#7 `effects cannot leave the timeline`).
#[test]
fn p_eng_effectminseconds_s3_dragging_the_leading_end_stops_at_the_floor_and_at_zero() {
    // P.eng.effectMinSeconds: (2.0, 3.0) → the stop is 5.0, so the start can reach 4.9 and no further.
    let (t, dur) = fx_lane::drag_end(2.0, 3.0, false, 4.99);
    assert_close("leading drag to 4.99", t, 5.0 - fx_lane::MIN_BAND_SECONDS);
    assert_close("leading drag's length", dur, fx_lane::MIN_BAND_SECONDS);
    assert_close("the trailing end stayed put", t + dur, 5.0);

    // A band starting near the front of the session, dragged leading. `to` is an ABSOLUTE second, so dragging
    // it to 0.5 moves the start there and shortens the band (stop stays at 1.02): the floor never bites, but
    // the length follows the hand. What the rule guarantees here is only that the answer is not negative.
    let (t_front, dur_front) = fx_lane::drag_end(0.02, 1.0, false, 0.5);
    assert_eq!(t_front, 0.5, "the start the hand asked for");
    assert_close("stop unchanged, so the band shortened", t_front + dur_front, 1.02);

    // The same band dragged off the front of the session lands on 0 rather than on a negative second.
    let (t_off, dur_off) = fx_lane::drag_end(0.02, 1.0, false, -3.0);
    assert_eq!(t_off, 0.0, "never before second 0");
    assert_close("length after dragging off the front", t_off + dur_off, 1.02);
    assert!(t_off >= 0.0 && dur_off > 0.0);
}

/// S4: the floor is a FLOOR, not a size. A drag that stays above 0.1 s passes straight through, both ends.
#[test]
fn p_eng_effectminseconds_s4_it_is_a_floor_not_a_fixed_size() {
    // P.eng.effectMinSeconds must not squash a normal drag down to 0.1 s.
    assert_eq!(fx_lane::drag_end(5.0, 3.0, true, 8.5), (5.0, 3.5));
    assert_eq!(fx_lane::drag_end(5.0, 3.0, false, 4.25), (4.25, 3.75));

    // Exactly ON the floor is allowed too: the rule clamps below it, not at it.
    let (t_at, dur_at) = fx_lane::drag_end(5.0, 3.0, true, 5.1);
    assert_eq!(t_at, 5.0);
    assert_close("trailing drag exactly on the floor", dur_at, fx_lane::MIN_BAND_SECONDS);
}

/// S5: four different rules bound a length from below and only one of them is this one. Two of the neighbours
/// share its number or its prototype name by coincidence, so the distinction is asserted rather than assumed.
#[test]
fn p_eng_effectminseconds_s5_the_floor_is_not_the_other_three_seconds_rules() {
    // P.eng.effectMinSurvivingSeconds (cut_clamp::MIN_SURVIVING_SECONDS, 1.0): what a CLAMPED band must have
    // left or it is dropped outright. Nothing to do with how short a hand may drag a band.
    assert_eq!(cut_clamp::MIN_SURVIVING_SECONDS, 1.0);
    assert_ne!(
        fx_lane::MIN_BAND_SECONDS, cut_clamp::MIN_SURVIVING_SECONDS,
        "the drag floor and the drop threshold are different rules"
    );

    // effects.volumeMinSeconds (fx_volume::MIN_SECONDS): the VOLUME FORM's typed floor. It happens to be 0.1
    // too, which is why the equality here is against the surviving-seconds bound and not a bare number: same
    // digits, different rule, and one must not be quietly substituted for the other.
    assert_eq!(fx_volume::MIN_SECONDS, fx_lane::MIN_BAND_SECONDS, "both are 0.1 by coincidence");
    assert_ne!(
        fx_volume::MIN_SECONDS, cut_clamp::MIN_SURVIVING_SECONDS,
        "the volume form's floor is not the drop threshold"
    );
    // They stay separate rows nonetheless: the form's floor is a typed-input bound, this is a drag bound.
    assert_eq!(number_anywhere("effects.volumeMinSeconds"), fx_volume::MIN_SECONDS);
    assert_ne!(
        anywhere("effects.volumeMinSeconds").from,
        anywhere("P.eng.effectMinSeconds").from,
        "two rules, two homes — neither borrows the other's constant"
    );

    // P.eng.minPieceSeconds (cut_select::MIN_SECONDS, 0.04): the SELECTION band's floor, about a frame. The
    // effects lane's floor is longer because a band has to stay grabbable, not merely visible.
    assert_eq!(cut_select::MIN_SECONDS, 0.04);
    assert_ne!(
        fx_lane::MIN_BAND_SECONDS, cut_select::MIN_SECONDS,
        "the selection band's floor is not the effects lane's"
    );
}
