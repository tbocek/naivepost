//! P.machine.seamRetries — the refused join asked again, and what each ask costs.
//! `spec/10-parameters.md` row `P.machine.seamRetries` (default 1), read by the text-edit pass of
//! Prepare ([`naivepost::seam_retries`], [`naivepost::tools::textedit`]).
//!
//! S1 the default and how many asks it makes · S2 a slot per ask so the cache cannot re-serve the
//! refusal · S3 the two log tails · S4 the counter stops at the bound · S5 a refused join keeps the
//! stumble · S6 where the id sits in the catalogue.

use naivepost::params::{self, Family};
use naivepost::seam_retries::{self as sr, SEAM_RETRIES};
use naivepost::tools::textedit::{Join, Side};
use serde_json::json;

// --- S1: the default, and the number of asks it buys -------------------------------------------

#[test]
fn p_machine_seamretries_s1_default_one_makes_two_asks() {
    // P.machine.seamRetries = 1: one retry after the first answer is refused.
    assert_eq!(SEAM_RETRIES, 1);
    // The prototype's loop was `for try := 0; try <= seamRetries`, so the join is asked twice:
    // the first answer plus the one retry this parameter pays for.
    assert_eq!(sr::attempts(SEAM_RETRIES), 2);
    // With no retries allowed there is only ever the one ask.
    assert_eq!(sr::attempts(0), 1);
}

// --- S2: every ask has its own cache slot ------------------------------------------------------

#[test]
fn p_machine_seamretries_s2_each_ask_keys_elsewhere() {
    let base = "JOIN 41 of 57.\n\nBEFORE: ... \n\nAFTER: ...";
    let first = sr::ask_slot(base, 0);
    let second = sr::ask_slot(base, 1);
    // The first ask is the request unchanged; the retry carries `try 2`.
    assert_eq!(first, base);
    assert!(second.contains("try 2"), "{second}");
    assert_ne!(first, second);
    // And they key to different files: llm_cache keys on the exact request, so a retry that shared
    // the first ask's slot would be answered out of cache by the reply just refused.
    let k1 = params_key(&first);
    let k2 = params_key(&second);
    assert!(k1.is_some() && k2.is_some());
    assert_ne!(k1, k2, "a refused answer must not be re-served to the retry");
    // A third ask differs from both, so N retries need N distinct slots.
    assert_ne!(k2, params_key(&sr::ask_slot(base, 2)));
}

/// The cache key of one ask, through the same function the pass stores under.
fn params_key(slot: &str) -> Option<String> {
    naivepost::llm_cache::key(&[json!("textedit"), json!(slot)])
}

// --- S3: what the log says after a refusal -----------------------------------------------------

#[test]
fn p_machine_seamretries_s3_log_tail_says_whether_it_will_ask_again() {
    // While a retry remains the line ends "asking once more" ...
    assert_eq!(sr::log_tail(0, SEAM_RETRIES), "asking once more");
    // ... and on the last ask it ends "nothing removed there", which is also what happened.
    assert_eq!(sr::log_tail(1, SEAM_RETRIES), "nothing removed there");
    // No retries allowed: the single ask is already the last one, so it never says "once more".
    assert_eq!(sr::log_tail(0, 0), "nothing removed there");
    // Same wording at any bound: the tail tracks what is left, not the number itself.
    assert_eq!(sr::log_tail(2, 3), "asking once more");
    assert_eq!(sr::log_tail(3, 3), "nothing removed there");
}

// --- S4: the counter spends its asks and then stops --------------------------------------------

#[test]
fn p_machine_seamretries_s4_the_join_cannot_be_asked_past_the_bound() {
    let mut asking = sr::Asking::start(SEAM_RETRIES);
    assert_eq!(asking.attempt, 0);
    assert!(!asking.spent(), "the first ask has not been made yet");

    // Refused the first time: one retry owed, said in the log tail.
    assert_eq!(asking.refused(), Some("asking once more"));
    assert_eq!(asking.attempt, 1);
    assert!(!more_coming(&asking));

    // Refused again: this was the last allowed ask, so the tail switches to what it means.
    assert_eq!(asking.refused(), Some("nothing removed there"));
    assert_eq!(asking.attempt, 2);
    assert!(asking.spent());

    // A third refusal grants nothing: two asks were all this join could get.
    assert_eq!(asking.refused(), None);
    assert_eq!(asking.attempt, 2, "no ask granted past attempts(1)");
    assert!(!sr::more_coming(asking.attempt, asking.retries));

    // Each ask of the sequence used its own slot, including the retry.
    let mut fresh = sr::Asking::start(SEAM_RETRIES);
    let a = fresh.slot("base");
    fresh.refused();
    let b = fresh.slot("base");
    assert_eq!(a, "base");
    assert_eq!(b, "base|try 2");
}

/// Whether this join still owes an ask. Named here so the test reads as the rule, not the arithmetic.
fn more_coming(asking: &sr::Asking) -> bool {
    asking.attempt < asking.retries
}

// --- S5: a refusal keeps the join ------------------------------------------------------------

#[test]
fn p_machine_seamretries_s5_a_refusal_removes_nothing() {
    // The safe way to fail: a join nobody could answer for stays. Both sides were said, and keeping a
    // stumble is a smaller fault than cutting something said once (spec/04-prepare.md: "A refusal
    // still keeps the stumble").
    let words_before = ["und", "das", "ist"];
    let words_after = ["der", "aktuellste", "Release"];

    // What the tool answers when the model gives up on the join: kept, zero removed.
    let mut kept = Join::new(&words_before, &words_after);
    let answer = kept.keep_join();
    assert!(answer.contains("\"kept\":true") || answer.contains("kept"), "{answer}");
    assert!(answer.contains("removed"), "{answer}");
    assert!(kept.kept());
    assert_eq!(kept.dropped_count(Side::Before), 0);
    assert_eq!(kept.dropped_count(Side::After), 0);

    // The other shape of refusal — a count of zero, which is not a stretch at the join — also takes
    // nothing away, and says why instead of silently agreeing.
    let mut refused = Join::new(&words_before, &words_after);
    let err = refused.drop_words(Side::After, 0);
    assert!(err.contains("keep_join"), "{err}");
    assert_eq!(refused.dropped_count(Side::After), 0);
    assert!(!refused.kept(), "a refusal is not the whole answer the pass wanted");
    // So after the last refusal of S4 the join is exactly as it arrived: nothing dropped either side.
    assert_eq!(refused.words_taken(Side::After).len(), 0);
}

// --- S6: where the id lives ------------------------------------------------------------------

#[test]
fn p_machine_seamretries_s6_family_is_machine_and_the_row_is_deliberately_unrowed() {
    // §10's family comes from the prefix, so the Settings dialog's home answers correctly even
    // though this id carries no row of its own.
    assert_eq!(params::family("P.machine.seamRetries"), Family::Machine);
    // And there is no row: §04#4 does not name this id among the eighteen it lists, and
    // rust/tests/prepare_parameters.rs pins that list by ORDER — inserting a row into
    // params::prepare() ahead of the ones it names would break that assertion. The value therefore
    // lives with the rule that uses it, in seam_retries::SEAM_RETRIES.
    assert!(
        params::find("P.machine.seamRetries").is_none(),
        "unrowed by design; see seam_retries' module comment"
    );
    // Its siblings ARE rowed, which is what makes the omission deliberate rather than an oversight.
    for sibling in [
        "P.machine.seamReachWords",
        "P.machine.seamMaxWords",
        "P.machine.seamCeil",
        "P.machine.seamSnapWords",
        "P.machine.seamNoiseWords",
    ] {
        assert!(params::find(sibling).is_some(), "{sibling} should have a row");
    }
}
