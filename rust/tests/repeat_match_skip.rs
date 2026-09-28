// §10-parameters · P.eng.repeatSkip — how many words of the later take may be stepped over while the fuzzy repeat
// matcher looks for the next tail word (3; prototype `repeatSkip`, gui/retake.go). The rule already existed as
// `prepare_decisions::TAIL_SKIP_MAX`, read by `run_from`'s inner loop and reached through `suffix_match`, which
// is what trims a retake mark inside a line. What this file pins is that the §10 row points at that constant, and
// that the bound really is a step-over limit rather than a share requirement.
//
// Fixtures follow `tests/prepare_decisions.rs`: plain string slices for the marked tail and the later take, with
// filler standing for words the two takes did not say alike.

use naivepost::params;
use naivepost::prepare_decisions as pd;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

/// A row found anywhere in the catalogue. Asserted NON-EMPTY rather than exactly-one: some ids are deliberately
/// catalogued twice in this tree (`P.eng.minPieceSeconds` has a Prepare row and a Cut-page row), so a strict
/// helper would trip on sibling ids named below. The single-row check for THIS round's own id is done explicitly
/// in s1 against `params::prepare()`.
#[allow(dead_code)]
fn anywhere(id: &str) -> params::Param {
    let found = all_rows().into_iter().filter(|row| row.id == id).collect::<Vec<_>>();
    assert!(!found.is_empty(), "{id} catalogued nowhere");
    found.into_iter().next().unwrap()
}

/// S1: `P.eng.repeatSkip` = 3, held by `prepare_decisions::TAIL_SKIP_MAX`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_repeatskip_s1_the_skip_is_3_and_catalogued_once() {
    // P.eng.repeatSkip — "fuzzy repeat match: words skipped".
    assert_eq!(pd::TAIL_SKIP_MAX, 3);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.repeatSkip")
        .expect("the skip bound must be catalogued for Prepare, whose retake pass reads it");
    assert_eq!(row.from, "prepare_decisions::TAIL_SKIP_MAX", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "3");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 3.0);

    // §10 §5.1 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.repeatSkip"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.repeatSkip").count(),
        1,
        "P.eng.repeatSkip catalogued more than once"
    );
}

/// S2: three words of step-over are allowed. This is what the 3 buys — a retake that resumes three words late is
/// still recognised as saying the same thing, because a hand-marked tail rarely lines up word for word with what the
/// second take actually said.
#[test]
fn p_eng_repeatskip_s2_three_words_of_step_over_are_allowed() {
    // P.eng.repeatSkip: `one two three` are stepped over without consuming a tail word.
    let tail = ["alpha", "bravo", "charlie", "delta"];
    let later = ["alpha", "one", "two", "three", "bravo", "charlie", "delta"];

    let found = pd::suffix_match(&tail, &later).expect("three filler words must not lose the repeat");
    assert_eq!(found.starts_at, 0, "the anchor is where the tail's first word was found");
    assert_eq!(found.matched, 4, "every tail word turned up");
    assert_eq!(found.skipped, pd::TAIL_SKIP_MAX, "exactly the allowed three were stepped over");
    assert_eq!(found.edits, 0, "nothing here needed forgiving");
    assert!(found.share >= pd::TAIL_MATCH_MIN, "{}", found.share);
}

/// S3: a fourth step-over is refused. The bound is inclusive at three and closed beyond it, which is the whole
/// point of counting: enough slack for a word or two heard differently, not enough to walk the rest of the take
/// looking for any common word.
#[test]
fn p_eng_repeatskip_s3_a_fourth_step_over_is_refused() {
    // P.eng.repeatSkip: `four` sits at step 4 past the previous match, one past the bound.
    let tail = ["alpha", "bravo", "charlie", "delta"];
    let later = ["alpha", "one", "two", "three", "four", "bravo", "charlie", "delta"];
    assert!(
        pd::suffix_match(&tail, &later).is_none(),
        "step 4 is past TAIL_SKIP_MAX, so the run ends before bravo"
    );

    // The boundary asserted per step rather than only across a long tail: a two-word tail whose SECOND word sits
    // exactly `TAIL_SKIP_MAX` steps past the first is matched, and one further word of filler puts it out of
    // reach. (The tail itself must be short — with a longer tail the same later list would match the FILLER words
    // as tail words, which is a different fixture's business.)
    let tail2 = ["alpha", "bravo"];
    let exact = ["alpha", "p", "q", "r", "bravo"];
    let m = pd::suffix_match(&tail2, &exact)
        .expect("three steps reaches the second tail word");
    assert_eq!(m.starts_at, 0);
    assert_eq!(m.matched, 2, "{m:?}");
    assert_eq!(m.skipped, pd::TAIL_SKIP_MAX, "{m:?}");

    let one_more = ["alpha", "p", "q", "r", "s", "bravo"];
    assert!(
        pd::suffix_match(&tail2, &one_more).is_none(),
        "with the second word four steps away the run cannot reach it"
    );
}

/// S4: the refusal comes from the SKIP bound, not from the share bar. Both fixtures below would clear
/// `TAIL_MATCH_MIN` if the extra word could be stepped over — one of them can, the other can't, and only the
/// step count differs.
#[test]
fn p_eng_repeatskip_s4_the_bound_that_refuses_is_the_skip_not_the_share() {
    // P.eng.repeatSkip: within the bound, all four tail words arrive and three fillers are stepped over.
    let within = ["alpha", "one", "two", "three", "bravo", "charlie", "delta"];
    let tail = ["alpha", "bravo", "charlie", "delta"];
    let ok = pd::suffix_match(&tail, &within).expect("three filler words are allowed");
    assert_eq!(ok.matched, 4);
    assert_eq!(ok.skipped, 3);

    // Beyond it: `XK9` pushes every remaining tail word to step 4 and further, so no anchor in the later take
    // ever accounts for enough of the tail to clear TAIL_MATCH_MIN. The share never got a chance — the run dies
    // at the first unreachable word, which is why this is P.eng.repeatSkip refusing and not the 70 % bar.
    let beyond = ["alpha", "one", "two", "three", "XK9", "bravo", "charlie", "delta"];
    assert!(pd::suffix_match(&tail, &beyond).is_none());
    assert!(
        pd::TAIL_MATCH_MIN < 1.0,
        "a full-tail match would hide which bound refused; the share bar stays below 1"
    );

    // The guard clause: an empty tail or an empty later take finds nothing, without walking anything.
    assert!(pd::suffix_match(&[], &within).is_none(), "an empty tail repeats nothing");
    assert!(pd::suffix_match(&tail, &[]).is_none(), "no later take holds no repeat");
}
