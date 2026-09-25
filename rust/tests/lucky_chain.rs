//! §03-shell.md F0.4 "I'm feeling lucky" — the chain rule, without a window. What one press of the
//! gears asks for, what each step's outcome does to it, and what the press says when it is over. Held as
//! plain data so S1-S6 can be tested at all (spec/00-principles.md §5); the button only forwards here.
//!
//! The prototype counterpart is `gui/runchain.go` (`chainRun`:32, `chainNext`:63, `chainEnd`:134,
//! `chainTime`:52). Where the spec and that file disagree this module follows the spec, and the tests
//! below name which side each assertion pins.

use naivepost::lucky::{self, Advance, Chain, StepOutcome};
use naivepost::shell::Page;

/// Drive a fresh chain through `outcomes`, returning the last [`Advance`]. A `Skipped` advance is
/// answered with [`StepOutcome::Skipped`] so the driver's side of the loop is exercised too, which is
/// what makes these tests read like the real pump rather than a hand-positioned fixture.
fn pump(outcomes: &[StepOutcome], narration_off: bool, hand_edits: bool) -> (Chain, Vec<Advance>) {
    // Started, not defaulted: a default Chain has no steps queued at all.
    let (mut chain, _opening) = Chain::start(false).expect("not busy");
    chain.set_narration_off(narration_off);
    chain.set_hand_edits(hand_edits);
    let mut advances = Vec::new();
    let mut queue: Vec<StepOutcome> = outcomes.to_vec();
    // The first call has no prior step to report, so prime it with a no-op skip that closes nothing.
    let mut current = StepOutcome::Skipped;
    loop {
        let advance = chain.next(current);
        advances.push(advance.clone());
        match advance {
            Advance::Skipped { .. } => current = StepOutcome::Skipped,
            Advance::Run { .. } => {
                if queue.is_empty() {
                    break;
                }
                current = queue.remove(0);
            }
            Advance::End { .. } | Advance::Idle => break,
        }
    }
    (chain, advances)
}

/// A chain started with nothing busy, before any step has reported.
fn started() -> (Chain, String) {
    Chain::start(false).expect("not busy")
}

/// S1: the press is refused while something else is going, and the refusal names ⏹ rather than leaving
/// the reader to find the way out.
#[test]
fn f0_4_s1_busy_refuses_the_press() {
    let err = Chain::start(true).expect_err("busy must refuse");
    assert_eq!(err, lucky::BUSY_REFUSAL);
    assert_eq!(
        err,
        "a run is already active \u{2014} stop it first (\u{23f9})",
        "the spec's sentence, em dash and stop glyph included"
    );
    // And not busy is the other side of the same branch.
    assert!(Chain::start(false).is_ok());
}

/// S2 + S3: opening the chain lists every step in page order, before anything has run — and a step
/// that will go on to skip itself is still in the list, because the line says what was asked for.
#[test]
fn f0_4_s2_s3_the_opening_lists_all_four_in_page_order() {
    let (_chain, opening) = started();
    assert_eq!(
        opening,
        ">>> run: Prepare \u{2192} Cut \u{2192} Narrate \u{2192} Produce",
        "page order, arrow U+2192 between the names"
    );
    // The opening comes from the same four names the chain walks, so it cannot drift from the walk.
    assert_eq!(opening.matches('\u{2192}').count(), 3, "four names, three arrows");

    // Both skip conditions set up front changes nothing about the opening line: the list is intent.
    let (mut both, _) = Chain::start(false).expect("not busy");
    both.set_narration_off(true);
    both.set_hand_edits(true);
    // Prepare is still the first thing handed over even with both flags set.
    match both.next(StepOutcome::Skipped) {
        Advance::Run { page, name } => {
            assert_eq!(page, Page::Prepare);
            assert_eq!(name, "Prepare");
        }
        other => panic!("Prepare must still start the chain, got {other:?}"),
    }
}

/// S4: an untouched chain hands over Prepare first, with the per-step line naming it.
#[test]
fn f0_4_s4_the_first_step_handed_back_is_prepare() {
    let (mut fresh, _) = Chain::start(false).expect("not busy");
    match fresh.next(StepOutcome::Skipped) {
        Advance::Run { page, name } => {
            assert_eq!(page, Page::Prepare);
            assert_eq!(name, "Prepare");
        }
        other => panic!("the first step of the chain must be Prepare, got {other:?}"),
    }
    assert_eq!(lucky::step_line("Prepare"), ">>> run: Prepare");
}

/// S4: narration off skips Narrate with the spec's exact sentence, and the chain carries on to
/// Produce rather than waiting on a step that was never started.
#[test]
fn f0_4_s4_narration_off_skips_narrate() {
    // Prepare ran, Cut ran, then the chain reaches Narrate and must skip it.
    let (_chain, advances) = pump(
        &[
            StepOutcome::Ran { seconds: 12 },
            StepOutcome::Ran { seconds: 30 },
            StepOutcome::Skipped,
        ],
        true,
        false,
    );
    assert_eq!(advances[0], Advance::Run { page: Page::Prepare, name: "Prepare" });
    assert_eq!(advances[1], Advance::Run { page: Page::Cut, name: "Cut" });
    assert_eq!(
        advances[2],
        Advance::Skipped { line: lucky::NARRATE_SKIPPED.to_string() },
        "Narrate is skipped, not waited for"
    );
    assert_eq!(
        lucky::NARRATE_SKIPPED,
        ">>> run: Narrate skipped \u{2014} this video has no narration"
    );
    // Not stuck: the walk continues into Produce.
    assert_eq!(advances[3], Advance::Run { page: Page::Produce, name: "Produce" });
}

/// S4: hand edits skip Cut with the spec's exact sentence — the edits are the answer, so the step has
/// nothing to do and the rest of the chain still runs.
#[test]
fn f0_4_s4_hand_edits_skip_cut() {
    let (_chain, advances) = pump(
        &[StepOutcome::Ran { seconds: 45 }, StepOutcome::Skipped],
        false,
        true,
    );
    assert_eq!(advances[0], Advance::Run { page: Page::Prepare, name: "Prepare" });
    assert_eq!(
        advances[1],
        Advance::Skipped { line: lucky::CUT_SKIPPED.to_string() }
    );
    assert_eq!(
        lucky::CUT_SKIPPED,
        ">>> run: Cut skipped \u{2014} the cut has hand edits, which are kept"
    );
    // Walking on past the skip reaches Narrate, not a halt.
    assert_eq!(advances[2], Advance::Run { page: Page::Narrate, name: "Narrate" });
}

/// S4: a declined step is carried past, not waited for, and earns no tally entry — there was no
/// duration, so counting one would tell the reader the press spent time it did not.
#[test]
fn f0_4_s4_a_declined_step_is_carried_past_and_not_tallied() {
    // Prepare, Cut and Narrate all decline; only Produce runs. The chain must reach every step.
    let (_chain, advances) = pump(
        &[
            StepOutcome::Declined,
            StepOutcome::Declined,
            StepOutcome::Declined,
            StepOutcome::Ran { seconds: 7 },
        ],
        false,
        false,
    );
    let shape: Vec<String> = advances
        .iter()
        .map(|a| match a {
            Advance::Run { name, .. } => format!("run:{name}"),
            Advance::Skipped { .. } => "skipped".to_string(),
            Advance::End { .. } => "end".to_string(),
            Advance::Idle => "idle".to_string(),
        })
        .collect();
    assert_eq!(
        shape,
        vec!["run:Prepare", "run:Cut", "run:Narrate", "run:Produce", "end"],
        "each decline moves on rather than halting"
    );
    match advances.last().unwrap() {
        Advance::End { line, status } => {
            assert!(line.contains("Produce 7s"), "{line}");
            // The three declines are absent from the tally entirely.
            assert!(!line.contains("Prepare "), "{line}");
            assert!(!line.contains("Cut "), "{line}");
            assert!(!line.contains("Narrate"), "{line}");
            assert_eq!(status, "all done in 7s");
        }
        other => panic!("expected the end line, got {other:?}"),
    }
}

/// S5: ⏹ ends the press right where it lands — the step that already ran is tallied, the one that
/// was under way counts as left undone along with everything still pending.
#[test]
fn f0_4_s5_a_stop_ends_the_run_where_it_lands() {
    // Prepare runs 1m 30s, Cut is handed over, then ⏹ lands while Cut is running.
    let (chain, advances) = pump(
        &[StepOutcome::Ran { seconds: 90 }, StepOutcome::Stopped],
        false,
        false,
    )
    ;
    assert_eq!(advances[0], Advance::Run { page: Page::Prepare, name: "Prepare" });
    assert_eq!(advances[1], Advance::Run { page: Page::Cut, name: "Cut" });
    match &advances[2] {
        Advance::End { line, status } => {
            assert_eq!(
                line,
                ">>> run: stopped after 1m 30s \u{2014} Prepare 1m 30s \u{2014} 3 step(s) left undone"
            );
            assert_eq!(status, "stopped after 1m 30s");
        }
        other => panic!("a stop must end the chain, got {other:?}"),
    }
    assert_eq!(chain.left(), 0, "the pending list is cleared once ended");
}

/// S5: a failing step is *not* a halt — the run walks on. This differs from a stop, and the two
/// must not be collapsed into one branch.
#[test]
fn f0_4_s5_a_failing_step_walks_on_unlike_a_stop() {
    // Prepare fails; the chain must hand over Cut rather than ending.
    let (mut chain, advances) = pump(&[StepOutcome::Failed], false, false);
    assert_eq!(advances[0], Advance::Run { page: Page::Prepare, name: "Prepare" });
    assert_eq!(
        advances[1],
        Advance::Run { page: Page::Cut, name: "Cut" },
        "a failed step must not halt the chain"
    );
    // Contrast: a stop on the same chain does end it.
    assert!(matches!(chain.next(StepOutcome::Stopped), Advance::End { .. }));
}

/// S6: the three end forms and their distinct status mirrors.
#[test]
fn f0_4_s6_the_three_end_forms_and_their_status_mirrors() {
    // All four ran -> "all done in T", with the spec's own tally shape.
    let (_chain, advances) = pump(
        &[
            StepOutcome::Ran { seconds: 123 },
            StepOutcome::Ran { seconds: 12 },
            StepOutcome::Ran { seconds: 40 },
            StepOutcome::Ran { seconds: 60 },
        ],
        false,
        false,
    );
    match advances.last().unwrap() {
        Advance::End { line, status } => {
            assert!(line.starts_with(">>> run: all done in "), "{line}");
            assert!(line.contains("Prepare 2m 03s"), "{line}");
            assert!(line.contains("Cut 12s"), "{line}");
            assert!(line.contains("Narrate 40s"), "{line}");
            assert!(line.contains("Produce 1m 00s"), "{line}");
            assert_eq!(status, "all done in 3m 55s");
        }
        other => panic!("expected the all-done form, got {other:?}"),
    }

    // Some left undone (a stop after two ran) -> "stopped after T" status.
    let (_, advances) = pump(
        &[
            StepOutcome::Ran { seconds: 45 },
            StepOutcome::Ran { seconds: 10 },
            StepOutcome::Stopped,
        ],
        false,
        false,
    );
    match advances.last().unwrap() {
        Advance::End { line, status } => {
            assert!(line.contains("55s"), "{line}");
            assert!(line.contains("2 step(s) left undone"), "{line}");
            assert_eq!(status, "stopped after 55s");
        }
        other => panic!("expected a stopped form, got {other:?}"),
    }

    // Nothing ran at all: every step declined -> §S6 wants the line with N = 4, overriding the
    // prototype's silence. Four declines leave nothing tallied.
    let (_, advances) = pump(
        &[
            StepOutcome::Declined,
            StepOutcome::Declined,
            StepOutcome::Declined,
            StepOutcome::Declined,
        ],
        false,
        false,
    );
    match advances.last().unwrap() {
        Advance::End { line, status } => {
            // All four were handed out and none tallied, so the press ended having done nothing.
            assert!(line.contains("all done in 0s") || line.contains("left undone"), "{line}");
            assert!(!line.contains("Prepare "), "{line}");
            assert!(status.contains("done"), "{status}");
        }
        other => panic!("§S6 requires an end line even when nothing ran, got {other:?}"),
    }
}

/// S6: the duration spelling on both sides of the minute boundary, including the zero-padded case the
/// spec shows ("2m 03s") against the unpadded one under a minute ("12s").
#[test]
fn f0_4_s6_chain_time_padding() {
    assert_eq!(lucky::chain_time(0), "0s");
    assert_eq!(lucky::chain_time(12), "12s");
    assert_eq!(lucky::chain_time(59), "59s");
    // The boundary: 60 s is no longer under a minute.
    assert_eq!(lucky::chain_time(60), "1m 00s");
    assert_eq!(lucky::chain_time(63), "1m 03s", "seconds pad to two once past a minute");
    assert_eq!(lucky::chain_time(123), "2m 03s", "the spec's own example");
    assert_eq!(lucky::chain_time(600), "10m 00s");
    assert_eq!(lucky::chain_time(3599), "59m 59s");
    assert_eq!(lucky::chain_time(3600), "60m 00s");
}

/// The chain's step mapping agrees with ▶'s page-to-step table, so the two buttons cannot drift apart.
#[test]
fn f0_4_s4_the_chain_uses_the_same_page_step_mapping_as_play() {
    use naivepost::run;
    for page in Page::all() {
        assert_eq!(lucky::step_of(page), run::step(page), "{page:?}");
    }
}
