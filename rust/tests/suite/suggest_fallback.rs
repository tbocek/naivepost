//! P.eng.suggestFallbackSegments — the denominator for progress when a reply has no readable end.
//! `spec/10-parameters.md` row `P.eng.suggestFallbackSegments` (value 20, "denominator for a reply
//! whose segments have no readable end (progress only)", prototype `cut.go suggestMaxSegs`), read by
//! [`naivepost::cut_suggest`] while the suggest call streams.
//!
//! S1 the value and family · S2 the denominator divides · S3 the floor keeps it off zero · S4 clamped
//! at one · S5 position wins over the count · S6 deliberately unrowed.

use naivepost::cut_suggest::{self as suggest};
use naivepost::params::{self, Family};

/// The fraction N moments out of the fallback denominator reads as.
fn expected(moments: usize) -> f64 {
    moments as f64 / suggest::SUGGEST_FALLBACK_SEGMENTS as f64
}

// --- S1: the value, and whose table it is -----------------------------------------------------

#[test]
fn p_eng_suggestfallbacksegments_s1_twenty_and_an_engineering_value() {
    // P.eng.suggestFallbackSegments = 20.
    assert_eq!(suggest::SUGGEST_FALLBACK_SEGMENTS, 20);
    assert_eq!(
        params::family("P.eng.suggestFallbackSegments"),
        Family::Eng,
        "an engineering constant, not a per-project policy"
    );
}

// --- S2: the constant is what divides ---------------------------------------------------------

#[test]
fn p_eng_suggestfallbacksegments_s2_the_denominator_is_what_divides() {
    // Computed from the constant rather than written as literals, so moving 20 moves the expectation.
    assert_eq!(suggest::fallback_fraction(10), expected(10));
    assert_eq!(suggest::fallback_fraction(10), 0.5);
    assert_eq!(suggest::fallback_fraction(suggest::SUGGEST_FALLBACK_SEGMENTS), 1.0);
    assert_eq!(suggest::fallback_fraction(5), expected(5));
}

// --- S3: the floor keeps the bar off zero ------------------------------------------------------

#[test]
fn p_eng_suggestfallbacksegments_s3_never_reads_zero() {
    // suggest.progressFloorSeconds — the same floor the placed progress uses.
    assert_eq!(suggest::fallback_fraction(0), suggest::PROGRESS_FLOOR_SECONDS);
    assert!(suggest::fallback_fraction(0) > 0.0, "a bar at 0 looks like no run at all");
    // A count that would divide below the floor is lifted to it, not left under it.
    let raw = expected(0);
    assert!(raw < suggest::PROGRESS_FLOOR_SECONDS);
    assert!(suggest::fallback_fraction(1) >= suggest::PROGRESS_FLOOR_SECONDS);
}

// --- S4: clamped at one ------------------------------------------------------------------------

#[test]
fn p_eng_suggestfallbacksegments_s4_a_full_bar_not_five() {
    // Past the denominator the bar is full; a longer reply never reports 5 or 500%.
    assert_eq!(suggest::fallback_fraction(suggest::SUGGEST_FALLBACK_SEGMENTS), 1.0);
    assert_eq!(suggest::fallback_fraction(100), 1.0);
    assert_eq!(suggest::fallback_fraction(usize::MAX / 2), 1.0, "no overflow past the ceiling either");
    // And every value in between stays inside the band.
    for moments in 0..=40usize {
        let fraction = suggest::fallback_fraction(moments);
        assert!(
            (suggest::PROGRESS_FLOOR_SECONDS..=1.0).contains(&fraction),
            "{moments} moments gave {fraction}"
        );
    }
}

// --- S5: a real position beats any count -------------------------------------------------------

#[test]
fn p_eng_suggestfallbacksegments_s5_position_wins_over_the_count() {
    // Somewhere reached in a session of known length: the position says it, whatever the count was.
    assert_eq!(suggest::progress_fraction(1, 60.0, 300.0), 60.0 / 300.0);
    assert_eq!(suggest::progress_fraction(99, 150.0, 300.0), 0.5);
    // No span at all, or nothing reached yet: fall back to counting.
    assert_eq!(
        suggest::progress_fraction(10, 0.0, 0.0),
        suggest::fallback_fraction(10)
    );
    assert_eq!(
        suggest::progress_fraction(5, 0.0, 300.0),
        suggest::fallback_fraction(5),
        "a span with nowhere reached is still no position"
    );
    assert_eq!(
        suggest::progress_fraction(3, 120.0, 0.0),
        suggest::fallback_fraction(3),
        "a zero-length session gives no fraction of anything"
    );
    // Nothing closed and nowhere reached: the floor, not zero.
    assert_eq!(
        suggest::progress_fraction(0, 0.0, 0.0),
        suggest::PROGRESS_FLOOR_SECONDS
    );
}

// --- S6: deliberately unrowed ------------------------------------------------------------------

#[test]
fn p_eng_suggestfallbacksegments_s6_no_row_lives_with_the_rule_instead() {
    const ID: &str = "P.eng.suggestFallbackSegments";
    // find answers Prepare's rows; scan all five sections so the claim holds either way.
    assert!(params::find(ID).is_none());
    for section in [
        params::prepare(),
        params::cut(),
        params::effects(),
        params::narrate(),
        params::produce(),
    ] {
        assert_eq!(
            section.iter().filter(|row| row.id == ID).count(),
            0,
            "{ID} would have a row; §05#6 names no fallback denominator and \
             cut_parameters_used.rs pins that list"
        );
    }
    // The value lives with the rule that reads it, in cut_suggest — the same choice made for
    // P.machine.seamRetries and P.policy.narratorSlots.
    assert_eq!(suggest::SUGGEST_FALLBACK_SEGMENTS, 20);
    // A sibling engineering constant IS rowed in cut(), so this omission reads as deliberate.
    assert!(params::cut()
        .iter()
        .any(|row| row.id == "P.eng.preloadLeadSeconds"));
}
