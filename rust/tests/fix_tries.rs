//! P.machine.fixTries — how many asks one block of the transcript fixer gets.
//! `spec/10-parameters.md` row `P.machine.fixTries` (default 2, "attempts per block", prototype
//! `try < 2`), read by [`naivepost::fix_transcripts`] in F1.8.
//!
//! S1 the default and the bound as a predicate · S2 the loop spends its tries then lets the originals
//! stand · S3 a repair on the second ask saves the block · S4 only the first attempt is cached ·
//! S5 what the give-up line says · S6 where the id sits in the catalogue.

use std::cell::Cell;

use naivepost::params::{self, Family};
use naivepost::textfmt::Line;
use naivepost::fix_transcripts as fix;

/// One ASR row, ten seconds long, so a block of N reads as N distinct rows.
fn lines(count: usize) -> Vec<Line> {
    (0..count)
        .map(|index| Line {
            start: index as f64 * 10.0,
            end: index as f64 * 10.0 + 9.0,
            speaker: "SPEAKER_00".into(),
            text: format!("uh number {index} about the thing"),
        })
        .collect()
}

// --- S1: the default, and the bound read as a predicate ----------------------------------------

#[test]
fn p_machine_fixtries_s1_two_attempts_per_block() {
    // P.machine.fixTries = 2, the count the prototype wrote as `try < 2`.
    assert_eq!(fix::BLOCK_TRIES, 2);
    assert_eq!(fix::attempts(), 2);
    // The same bound as a question: after one ask another is owed, after two none is.
    assert!(fix::more_tries_left(0));
    assert!(fix::more_tries_left(1));
    assert!(!fix::more_tries_left(2));
    assert!(!fix::more_tries_left(3), "past the bound stays past it");
}

// --- S2: the live loop spends exactly its tries, then the originals stand ----------------------

#[test]
fn p_machine_fixtries_s2_an_unfixable_block_is_asked_twice_and_no_more() {
    let asked = Cell::new(0);
    let outcome = fix::fix_block(&lines(2), |_| {
        asked.set(asked.get() + 1);
        // Refused every time: a row that is not in this block, which the tool answers by name.
        Ok(vec![fix::Reply::Fix {
            n: 99,
            text: "not a row of this block".into(),
        }])
    })
    .unwrap();

    // Exactly P.machine.fixTries asks — no third try chasing an answer that never validates.
    assert_eq!(asked.get(), fix::attempts());
    assert!(!outcome.valid, "out of tries");
    assert_eq!(outcome.fixed, 0, "nothing was fixed");
    assert_eq!(outcome.lines, lines(2), "the ASR's own lines stand untouched");
}

// --- S3: the retry is what saves the block ----------------------------------------------------

#[test]
fn p_machine_fixtries_s3_the_second_try_can_repair_what_the_first_lost() {
    let seen = std::cell::RefCell::new(Vec::<String>::new());
    let outcome = fix::fix_block(&lines(2), |ask| {
        seen.borrow_mut().push(ask.to_string());
        if seen.borrow().len() == 1 {
            // First answer refused: one row right, one row out of the block.
            return Ok(vec![
                fix::Reply::Fix {
                    n: 1,
                    text: "cleaned first line".into(),
                },
                fix::Reply::Fix {
                    n: 9,
                    text: "out of range".into(),
                },
            ]);
        }
        // Second answer: both rows addressed and the reply closed with finish.
        Ok(vec![
            fix::Reply::Fix {
                n: 1,
                text: "cleaned first line".into(),
            },
            fix::Reply::Fix {
                n: 2,
                text: "cleaned second line".into(),
            },
            fix::Reply::Done,
        ])
    })
    .unwrap();

    let asks = seen.borrow();
    assert_eq!(asks.len(), 2, "saved on the retry, within the bound");
    assert!(outcome.valid);
    assert_eq!(outcome.fixed, 2);
    assert_eq!(outcome.lines[0].text, "cleaned first line");
    assert_eq!(outcome.lines[1].text, "cleaned second line");
    // The second ask is NOT a repeat of the first: it carries the refusal it has to correct, which
    // is why a second try earns anything at all (the prototype re-asked the identical question).
    assert!(!asks[0].contains("refused"));
    assert!(asks[1].contains("The previous answer was refused"), "{}", asks[1]);
    assert!(asks[1].contains("line 9 is not in this block"), "{}", asks[1]);
}

// --- S4: only the first attempt may be cached -------------------------------------------------

#[test]
fn p_machine_fixtries_s4_only_the_first_attempt_earns_a_cache_entry() {
    // §6's second irregularity: the fixer caches its first attempt and nothing after it. An answer
    // repaired after a refusal depends on the refusal it was shown, so replaying it from disk would
    // skip the validation that made it right — the retry is paid for again rather than trusted.
    // Both spellings are pinned against the same rule so the two cannot drift apart.
    assert!(fix::caches_attempt(1), "attempt 1 is the first, and it caches");
    assert!(!fix::caches_attempt(2), "a repaired answer is not cacheable");
    assert!(!fix::caches_attempt(3));
    assert_eq!(fix::caches_attempt(1), naivepost::llm_cache::fixer_caches(1));
    assert_eq!(fix::caches_attempt(2), naivepost::llm_cache::fixer_caches(2));
    // So with P.machine.fixTries = 2 at most one of a block's two asks can ever be free on a re-run.
    assert!(fix::attempts() >= 2);
    let cached: Vec<bool> = (1..=fix::attempts()).map(fix::caches_attempt).collect();
    assert_eq!(cached.iter().filter(|caches| **caches).count(), 1, "{cached:?}");
}

// --- S5: the give-up line names the block and the count ---------------------------------------

#[test]
fn p_machine_fixtries_s5_the_give_up_line_says_refused_not_crashed() {
    // A reader who sees only "failed" looks for a crash. There was none: the answer would not
    // validate twice, so this block keeps the ASR's own text and every other block still got fixed.
    let log = fix::give_up_log("lecture", 2, 5);
    assert!(log.contains("lecture"), "{log}");
    assert!(log.contains("block 3/5"), "one-based position in the source: {log}");
    assert!(log.contains("kept its ASR text"), "{log}");
    // The count comes from BLOCK_TRIES, so the line cannot go stale if the parameter moves.
    assert!(log.contains(&format!("refused {} times", fix::BLOCK_TRIES)), "{log}");
    assert!(log.starts_with("!!!"), "it is an error line: {log}");
}

// --- S6: where the id lives ------------------------------------------------------------------

#[test]
fn p_machine_fixtries_s6_family_is_machine_and_the_row_is_deliberately_unrowed() {
    // The family resolves from the prefix, so Settings' home answers correctly without a row.
    assert_eq!(params::family("P.machine.fixTries"), Family::Machine);
    // And there is no row: §04#4 does not name this id among the parameters it lists, and
    // rust/tests/prepare_parameters.rs pins that list by ORDER — inserting a row into
    // params::prepare() ahead of the ones it names would break that assertion. The value therefore
    // lives with the rule that reads it, in fix_transcripts::BLOCK_TRIES.
    assert!(
        params::find("P.machine.fixTries").is_none(),
        "unrowed by design; see BLOCK_TRIES' doc comment"
    );
    // Its sibling IS rowed, which makes the omission deliberate rather than an oversight.
    let blocks = params::find("P.machine.fixBlockLines");
    assert!(blocks.is_some(), "P.machine.fixBlockLines should have a row");
    assert_eq!(blocks.unwrap().from, "prepare::FIX_BLOCK_LINES");
}
