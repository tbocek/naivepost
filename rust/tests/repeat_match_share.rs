// §10-parameters · P.eng.repeatShare — the share of a marked tail that has to turn up again for the fuzzy repeat
// match to count (0.7; prototype 0.7). The rule already existed as `prepare_decisions::TAIL_MATCH_MIN`, applied in
// `suffix_match` as `matched >= ceil(share × tail_len)`. What this file pins is that the §10 row points at that
// constant, and that the rounding inside it is understood: the stated 0.7 is a floor on the share, while the ceil
// decides how many WORDS that means for a given tail length.
//
// Only `suffix_match` and `SuffixMatch` are public (`run_from` is private), so every case goes through the one
// entry point and reads what it reports back.

use naivepost::params;
use naivepost::prepare_decisions as pd;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

/// A row found anywhere in the catalogue. Asserted NON-EMPTY rather than exactly-one: some ids are deliberately
/// catalogued twice in this tree (`P.eng.minPieceSeconds` has a Prepare row and a Cut-page row), so a strict
/// helper would trip on sibling ids named below. This round's own single-row check is done explicitly in s1.
#[allow(dead_code)]
fn anywhere(id: &str) -> params::Param {
    let found = all_rows().into_iter().filter(|row| row.id == id).collect::<Vec<_>>();
    assert!(!found.is_empty(), "{id} catalogued nowhere");
    found.into_iter().next().unwrap()
}

/// S1: `P.eng.repeatShare` = 0.7, held by `prepare_decisions::TAIL_MATCH_MIN`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_repeatshare_s1_the_share_is_0_7_and_catalogued_once() {
    // P.eng.repeatShare — "fuzzy repeat match: share that must match".
    assert_eq!(pd::TAIL_MATCH_MIN, 0.7);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.repeatShare")
        .expect("the share bound must be catalogued for Prepare, whose retake pass reads it");
    assert_eq!(row.from, "prepare_decisions::TAIL_MATCH_MIN", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "0.7");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.7);

    // §10 §5.1 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.repeatShare"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.repeatShare").count(),
        1,
        "P.eng.repeatShare catalogued more than once"
    );
}

/// S2: the share met exactly. Ten tail words with seven of them present in the later take sit right on the bar,
/// and the bar is inclusive: `ceil(0.7 × 10) == 7`, so seven matched words clear it.
#[test]
fn p_eng_repeatshare_s2_seven_of_ten_meets_the_bar_exactly() {
    // P.eng.repeatShare: the bar is 7 words here, and exactly 7 arrive.
    let tail = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliett",
    ];
    let later = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf"];

    let found = pd::suffix_match(&tail, &later).expect("seven of ten reaches the bar, not falls short");
    assert_eq!(found.matched, 7, "the walk stops when the later take runs out");
    assert_eq!(found.share, 0.7, "{found:?}");
    assert!(found.share >= pd::TAIL_MATCH_MIN, "{}", found.share);
    // 0.7 is not representable in binary, but 0.7 × 10 rounds to exactly 7.0 in f64 and `ceil` of that is 7 —
    // so the acceptance here is the rule being inclusive at the bar, not a float coincidence papering over a
    // shortfall. Six of ten would refuse: ceil(0.7 × 10) is 7 either way.
    let six = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];
    assert!(pd::suffix_match(&tail, &six).is_none(), "six of ten is under the bar");
}

/// S3: over and under the bar on the SAME tail length, so nothing but the count of returned words differs.
/// A five-word tail needs `ceil(0.7 × 5) = 4`.
#[test]
fn p_eng_repeatshare_s3_four_of_five_passes_and_three_does_not() {
    // P.eng.repeatShare: 4/5 = 0.8 clears 0.7; 3/5 = 0.6 does not.
    let tail = ["alpha", "bravo", "charlie", "delta", "echo"];

    let four = pd::suffix_match(&tail, &["alpha", "bravo", "charlie", "delta"])
        .expect("four of five is §12's clearing case");
    assert_eq!(four.matched, 4);
    assert_eq!(four.share, 0.8);
    assert!(four.share >= pd::TAIL_MATCH_MIN, "{}", four.share);
    assert_eq!(four.starts_at, 0);
    assert_eq!(four.edits, 0);

    let three = pd::suffix_match(&tail, &["alpha", "bravo", "charlie"]);
    assert!(three.is_none(), "three of five is under the bar, whatever the tail's other words");

    // The two fixtures differ ONLY in how many tail words turned up — same tail, same leading words, one fewer
    // present — which is what makes the bar the thing that decided rather than any difference in shape.
    assert_eq!(tail.len(), 5);
    assert_eq!((4 as f64 / 5.0) >= pd::TAIL_MATCH_MIN, true);
    assert_eq!((3 as f64 / 5.0) >= pd::TAIL_MATCH_MIN, false);
}

/// S4: the ceil raises the bar above the stated share for a short tail — and this bound is not the two
/// neighbouring bounds that also gate the same matcher.
#[test]
fn p_eng_repeatshare_s4_short_tails_are_stricter_and_the_neighbours_stay_distinct() {
    // P.eng.repeatShare: for a three-word tail the bar is ceil(0.7 × 3) = 3, i.e. ALL of it. Two of three
    // (0.667) fails — but the reason is the ROUNDING, not the 0.7 value: even a plain `>=` on the raw share
    // would refuse 0.667 against 0.7. What the ceil adds is that no partial word ever counts, so a short tail
    // effectively needs a full repeat.
    let tail3 = ["alpha", "bravo", "charlie"];
    assert!(pd::suffix_match(&tail3, &["alpha", "bravo"]).is_none(), "two of three cannot reach 3");
    assert!(
        pd::suffix_match(&tail3, &tail3).is_some(),
        "found whole, a three-word tail clears its own bar"
    );

    // The neighbouring gates are different rules with different homes, not restatements of the share:
    // TAIL_SKIP_MAX (P.eng.repeatSkip) is how far ahead in the later take a word may be SOUGHT;
    // TAIL_EDIT_MAX (§12's "one edit allowed") is how many imperfect matches a run may carry, and has no §10 row.
    assert_eq!(pd::TAIL_SKIP_MAX, 3);
    assert_eq!(pd::TAIL_EDIT_MAX, 1);
    assert_ne!(pd::TAIL_MATCH_MIN, 3.0, "the share is not the skip count");
    assert_ne!(pd::TAIL_MATCH_MIN, 1.0, "the share is not the edit allowance");

    // And each keeps its own row pointing at its own constant.
    let share_row = anywhere("P.eng.repeatShare");
    let skip_row = anywhere("P.eng.repeatSkip");
    assert_ne!(
        share_row.from, skip_row.from,
        "two bounds, two homes — neither borrows the other's constant"
    );
    assert_eq!(share_row.from, "prepare_decisions::TAIL_MATCH_MIN");
    assert_eq!(skip_row.from, "prepare_decisions::TAIL_SKIP_MAX");

    // Guard clause: nothing to match against finds nothing, without walking anything.
    let within = ["alpha", "bravo", "charlie", "delta"];
    assert!(pd::suffix_match(&[], &within).is_none(), "an empty tail repeats nothing");
    assert!(pd::suffix_match(&tail3, &[]).is_none(), "an empty later take holds no repeat");
}
