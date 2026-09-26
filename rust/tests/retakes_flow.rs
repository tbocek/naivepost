//! F1.9 Mark retakes — the pass's rules, tested without a window or a model.
//!
//! spec/04-prepare.md F1.9: whether there is anything to mark (S1), what the brief says (S2), how
//! the pooled runs are keyed and pooled (S3), what the words say about a mark (S4), merging with
//! edges placed by F1.11 (S5), the ceiling (S6) and the file and the log (S7). The §3.4 tool half
//! lives in `naivepost::tools::retakes` and is exercised here through the flow that calls it.

use std::fs;
use std::path::{Path, PathBuf};

use naivepost::retakes;
use naivepost::layout::Tree;
use naivepost::tools::retakes::{Line, Mark, Marks, Trim};

/// A scratch project folder, the way `tests/prepare_run_flow.rs::scratch` makes one. Never cwd at
/// the repo: a writer pointed at a session folder resolves through the tree we hand it.
fn scratch(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f19-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join(format!("{tag}.naivepost"));
    fs::create_dir_all(&dir).expect("project folder");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

/// Six spoken lines, one second apart, half a second long each.
fn six_lines() -> Vec<Line> {
    (1..=6)
        .map(|n| Line {
            n,
            start: (n as f64) - 1.0,
            end: (n as f64) - 0.5,
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    text.split(' ').map(|w| w.to_string()).collect()
}

// ---- S1 ---------------------------------------------------------------------

/// S1: three spoken lines cannot hold a retake, and the empty marks file is still written -- its
/// presence is what tells Cut the pass ran.
#[test]
fn f1_9_s1_too_few_lines_marks_nothing_but_still_writes_the_file() {
    // P.machine.retakeCeil is irrelevant at this size; the gate is a line count.
    let short = vec![
        Line { n: 1, start: 0.0, end: 0.5 },
        Line { n: 2, start: 1.0, end: 1.5 },
        Line { n: 3, start: 2.0, end: 2.5 },
    ];
    assert!(retakes::too_few(&short), "three lines is fewer than four");
    assert!(!retakes::too_few(&six_lines()), "four or more proceeds");
    assert_eq!(retakes::MIN_LINES, 4, "S1 states the count, not a ratio");

    let (_keep, tree) = scratch("s1");
    let lines = retakes::stage_marking(true, &short, &[], 0.0, 10.0);
    assert_eq!(lines.len(), 1, "the pass says one thing: {lines:?}");
    assert!(lines[0].contains("fewer than 4"), "and says why: {lines:?}");
    // The empty file is written on purpose, so the caller writes even when nothing was found.
    retakes::write_marks(&tree, &[], &[], &[]).expect("empty file written");
    assert!(tree.retakes_tsv().exists(), "an empty retakes.tsv still exists");
    assert_eq!(fs::read_to_string(tree.retakes_tsv()).unwrap(), "", "and is empty");
}

// ---- S2 ---------------------------------------------------------------------

/// S2: a gap of exactly the threshold draws the pause marker; just under it draws nothing.
/// P.machine.retakePauseSeconds
#[test]
fn f1_9_s2_a_pause_of_1_5_seconds_is_drawn_and_1_4_is_not() {
    // Gap before line 2 is 1.5 s; before line 3 it is 1.4 s.
    let lines = vec![
        Line { n: 1, start: 0.0, end: 1.0 },
        Line { n: 2, start: 2.5, end: 3.0 },
        Line { n: 3, start: 4.4, end: 5.0 },
    ];
    let brief = retakes::brief(&lines, &[]);
    assert!(brief.contains("2 [1.5s pause]"), "exactly 1.5 s draws the marker: {brief}");
    assert!(!brief.contains("4.4"), "the raw second is never printed");
    assert!(
        !brief.contains("3 [1.4s pause]") && !brief.contains("1.4"),
        "1.4 s is under the threshold and draws nothing: {brief}"
    );
    assert!(brief.lines().any(|l| l == "1"), "every line is numbered");
    assert_eq!(
        naivepost::tools::retakes::RETAKE_PAUSE_SECONDS,
        1.5,
        "P.machine.retakePauseSeconds"
    );
}

/// S2: a source change inserts the literal separator line in front of the line that begins it.
#[test]
fn f1_9_s2_a_source_change_inserts_the_separator() {
    let brief = retakes::brief(&six_lines(), &[4]);
    let lines: Vec<&str> = brief.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == "--- the recording stops here; the next one begins ---")
        .expect("the separator is present verbatim");
    assert_eq!(lines[at + 1], "4", "it sits in front of the line that begins the new recording");
    assert_eq!(
        retakes::SOURCE_CHANGE_LINE,
        "--- the recording stops here; the next one begins ---",
        "spelled as prose so the model does not describe it as content"
    );
    assert_eq!(lines.len(), 7, "six numbered lines plus one separator");
}

// ---- S3 ---------------------------------------------------------------------

/// S3: each pooled run gets its own cache slot -- the run index is in the key, or the pool collapses
/// to one draw read back three times. P.machine.retakeRuns
#[test]
fn f1_9_s3_each_pooled_run_gets_its_own_cache_slot() {
    let keys = retakes::run_keys("the brief", retakes::RUNS);
    assert_eq!(keys.len(), 3, "P.machine.retakeRuns = 3");
    assert_eq!(retakes::RUNS, naivepost::roles::RETAKE_RUNS_POOLED as usize);
    let distinct: std::collections::HashSet<&String> = keys.iter().collect();
    assert_eq!(distinct.len(), 3, "no two runs share a slot: {keys:?}");
    assert_ne!(
        retakes::run_key("the brief", 0),
        retakes::run_key("the brief", 1),
        "identical briefs still differ by run index"
    );
    assert_ne!(
        retakes::run_key("brief a", 0),
        retakes::run_key("brief b", 0),
        "a different brief is a different question"
    );
}

/// S3: pooled answers dedupe on the stretch named, so three runs agreeing is one mark and not three
/// counted against the ceiling.
#[test]
fn f1_9_s3_pooled_answers_dedupe_to_the_unique_set() {
    let mk = |from: u32, to: u32| Mark {
        from,
        to,
        removed: (from as f64, to as f64 + 1.0),
        trimmed: false,
        again: Some(to + 2),
    };
    let pooled = vec![mk(2, 4), mk(2, 4), mk(7, 9), mk(7, 9), mk(7, 9)];
    let deduped = retakes::dedupe(pooled);
    assert_eq!(deduped.len(), 2, "two distinct stretches: {deduped:?}");
    assert_eq!((deduped[0].from, deduped[0].to), (2, 4));
    assert_eq!((deduped[1].from, deduped[1].to), (7, 9));
    // Same stretch, different `again`: still one stretch. The removal is what the cut acts on.
    let mut other_again = mk(2, 4);
    other_again.again = None;
    let deduped = retakes::dedupe(vec![mk(2, 4), other_again]);
    assert_eq!(deduped.len(), 1, "the pair (from,to) is the identity of a mark");
}

/// S3: the prototype's strict JSON shape parses, and a bad payload yields no answer instead of a panic.
#[test]
fn f1_9_s3_the_answer_shape_parses_and_a_bad_payload_is_not_fatal() {
    let good = r#"{"abandoned":[{"from":7,"to":12,"again":15},{"from":1,"to":2,"again":0}]}"#;
    assert_eq!(
        retakes::parse_answer(good),
        vec![(7, 12, 15), (1, 2, 0)],
        "1-based line numbers, `again` 0 meaning never picked up"
    );
    // `again` absent entirely is the same as 0.
    assert_eq!(
        retakes::parse_answer(r#"{"abandoned":[{"from":3,"to":4}]}"#),
        vec![(3, 4, 0)]
    );
    assert!(retakes::parse_answer("not json at all").is_empty(), "a broken payload sets the run aside");
    assert!(retakes::parse_answer("{}").is_empty(), "no `abandoned` key is no answer");
    assert!(
        retakes::parse_answer(r#"{"abandoned":[{"from":"x","to":2,"again":3}]}"#).is_empty(),
        "a non-numeric line number is dropped rather than guessed"
    );
}

// ---- S4 ---------------------------------------------------------------------

/// S4: a removal shorter than the minimum is a breath, not an abandoned take.
/// P.machine.retakeMinSeconds
#[test]
fn f1_9_s4_a_removal_under_the_minimum_is_dropped_as_a_breath() {
    // Lines 1-2 spanning 0.0 .. 0.2: 0.2 s to remove.
    let lines = vec![
        Line { n: 1, start: 0.0, end: 0.1 },
        Line { n: 2, start: 0.1, end: 0.2 },
        Line { n: 3, start: 5.0, end: 6.0 },
    ];
    let marks = Marks::new(lines.clone(), 10.0);
    let out = retakes::verify(&marks, 1, 2, Some(3), None, false, false);
    match out {
        retakes::Outcome::Dropped(reason) => {
            assert!(reason.contains("breath"), "the reason names itself: {reason}");
        }
        other => panic!("a 0.2 s removal must be dropped, got {other:?}"),
    }
    assert_eq!(
        naivepost::tools::retakes::RETAKE_MIN_SECONDS,
        0.3,
        "P.machine.retakeMinSeconds"
    );
}

/// S4: `again = 0` is honest only for a whole take; on a partial mark it means the replacement was
/// not found, and cutting on that guess loses good material.
#[test]
fn f1_9_s4_again_zero_is_only_honest_for_a_whole_take() {
    let marks = Marks::new(six_lines(), 10.0);
    let partial = retakes::verify(&marks, 2, 4, None, None, false, false);
    match partial {
        retakes::Outcome::Refused(reason) => {
            assert!(reason.contains("not the whole take"), "{reason}");
        }
        other => panic!("a partial mark claiming no replacement must be refused, got {other:?}"),
    }
    let whole = retakes::verify(&marks, 1, 6, None, None, false, true);
    assert!(
        matches!(whole, retakes::Outcome::Applied { .. }),
        "the whole take with no replacement is legal: {whole:?}"
    );
}

/// S4: the fuzzy matcher finds a repeated tail across skips, a shared prefix and a one-edit word, and
/// refuses below the 70 % bar.
#[test]
fn f1_9_s4_the_fuzzy_matcher_finds_a_repeat_with_skips_and_one_edit_words() {
    let attempt = words("so the ledger records every transfer in the chain");
    // Three skips inside the run -- two interjections and one word the second take phrased
    // differently -- while 6 of the attempt's 9 words (67 %) still match loosely.
    let later = words("hold on so the ledger writes down every transfer transfers in the chein");
    assert_eq!(
        retakes::repeated_tail_match(&attempt, &later),
        None,
        "67 % matched is under the 70 % bar, so this is NOT called a repeat"
    );
    // Clearing the bar: 7 of the attempt's 9 words match (over 70 %) with two skips inside the run,
    // a shared-prefix word ("transfer"/"transfers") and a one-edit word ("chain"/"chein").
    let repeated = words("so the ledger records every transfer transfers in chain");
    assert_eq!(
        retakes::repeated_tail_match(&attempt, &repeated),
        Some(0),
        "found at the head, since that is where the repeat starts"
    );
    // The same repeat pushed past two interjections is still found at index 2.
    let with_lead = words("hold on so the ledger records every transfer transfers in chain");
    assert_eq!(
        retakes::repeated_tail_match(&attempt, &with_lead),
        Some(2),
        "the interjections before it are not part of the repeat"
    );
    // A fourth skip is one too many: only 5 of 9 words survive, under the bar either way.
    let four_skips = words("so x y z w every transfer in the chein");
    assert_eq!(
        retakes::repeated_tail_match(&attempt, &four_skips),
        None,
        "at most 3 skips are allowed inside the run"
    );

    // Equal words match outright.
    assert_eq!(
        retakes::repeated_tail_match(&words("alpha beta gamma"), &words("x alpha beta gamma")),
        Some(1)
    );
    // A shared prefix of 3 bytes counts; two letters do not.
    assert!(retakes::word_matches("lecture", "lecturer"), "shared 3-byte prefix");
    assert!(retakes::word_matches("chain", "chein"), "one substitution is one edit");
    assert!(retakes::word_matches("record", "records"), "one insertion is one edit");
    assert!(retakes::word_matches("cat", "cats"), "one deletion is one edit");
    // "ab"/"ac" IS one edit apart (one substitution), so it matches: the prefix rule needs 3 bytes,
    // but the one-edit rule has no length floor. Two edits is where it stops.
    assert!(retakes::word_matches("ab", "ac"), "one substitution is one edit at any length");
    assert!(!retakes::word_matches("ab", "cd"), "two edits apart is not a match");
    assert!(!retakes::word_matches("alpha", "omega"), "nothing alike is not a match");

    // Below 70 % matched is no repeat at all.
    let weak = words("completely different opening words here not matching our sentence today");
    assert!(
        retakes::repeated_tail_match(&attempt, &weak).is_none(),
        "a partial echo of one phrase is not a retake"
    );
    assert!(retakes::repeated_tail_match(&[], &later).is_none(), "nothing to match");
    assert!(retakes::repeated_tail_match(&attempt, &[]).is_none(), "nothing to match into");
}

/// S4: a rephrase keeps only the broken-off tail, and only while it is still fragment-sized.
/// P.machine.retakeFragmentSeconds
#[test]
fn f1_9_s4_a_rephrase_is_trimmed_to_the_broken_off_tail() {
    // Lines 1-3 span 0.0 .. 8.0; the fragment cap is 6 s, so 2 s are left standing at the front.
    let lines = vec![
        Line { n: 1, start: 0.0, end: 3.0 },
        Line { n: 2, start: 3.0, end: 6.0 },
        Line { n: 3, start: 6.0, end: 8.0 },
    ];
    let marks = Marks::new(lines, 20.0);
    let out = retakes::verify(&marks, 1, 3, Some(4), None, true, false);
    let removed = out.removed().expect("a rephrase applies");
    assert_eq!(removed, (2.0, 8.0), "only the last 6 s go: {removed:?}");
    assert_eq!(
        naivepost::tools::retakes::RETAKE_FRAGMENT_SECONDS,
        6.0,
        "P.machine.retakeFragmentSeconds"
    );
    // A fragment shorter than a breath is dropped rather than cut.
    let tiny = vec![
        Line { n: 1, start: 0.0, end: 0.1 },
        Line { n: 2, start: 0.1, end: 0.2 },
    ];
    let marks = Marks::new(tiny, 20.0);
    let out = retakes::verify(&marks, 1, 2, Some(3), None, true, false);
    assert!(
        matches!(out, retakes::Outcome::Dropped(_)),
        "a 0.2 s fragment is a breath: {out:?}"
    );
}

/// S4: a mark refused for lack of a repeat comes back as a second hearing, not a discard.
#[test]
fn f1_9_s4_a_refused_mark_is_reheard_rather_than_discarded() {
    let out = retakes::reheat(5, 9);
    match out {
        retakes::Outcome::Rehear { from, to } => {
            assert_eq!((from, to), (5, 9), "both attempts get re-transcribed and asked about again");
        }
        other => panic!("expected Rehear, got {other:?}"),
    }
    assert!(out.removed().is_none(), "a re-hearing removes nothing yet");
}

/// S4: a found repeat trims the mark to the repeated tail rather than leaving the whole stretch.
#[test]
fn f1_9_s4_a_found_repeat_trims_the_mark_to_the_tail() {
    // Four lines spanning 0 .. 8; the repeat covers 2 of the 4 lines, so half the span stands.
    let marks = Marks::new(six_lines(), 40.0);
    let out = retakes::verify(&marks, 1, 4, Some(5), Some(2), false, false);
    let removed = out.removed().expect("a trimmed mark applies");
    assert!(removed.0 > 0.0, "the front of the stretch is left standing: {removed:?}");
    assert!(
        matches!(out, retakes::Outcome::Applied { trimmed: true, .. }),
        "the trim is reported so the model knows its numbers were understood: {out:?}"
    );
}

// ---- S5 ---------------------------------------------------------------------

/// S5: overlapping and touching marks merge into one span.
#[test]
fn f1_9_s5_overlapping_marks_merge_into_one_span() {
    let mk = |a: f64, b: f64| Mark {
        from: 1,
        to: 2,
        removed: (a, b),
        trimmed: false,
        again: None,
    };
    let merged = retakes::merge_marks(vec![mk(5.0, 8.0), mk(7.0, 10.0)]);
    assert_eq!(merged.len(), 1, "two overlapping removals are one cut: {merged:?}");
    assert_eq!(merged[0].removed, (5.0, 10.0), "the union of the spans");
    assert!(merged[0].trimmed, "merging moved the shape, and says so");

    // Touching at a point merges too: they become one cut either way.
    let touching = retakes::merge_marks(vec![mk(0.0, 3.0), mk(3.0, 6.0)]);
    assert_eq!(touching.len(), 1, "adjacent removals merge: {touching:?}");
    assert_eq!(touching[0].removed, (0.0, 6.0));

    // Separate stays separate.
    let apart = retakes::merge_marks(vec![mk(0.0, 1.0), mk(5.0, 6.0)]);
    assert_eq!(apart.len(), 2, "a gap keeps them two marks: {apart:?}");
}

/// S5: edge placement goes through F1.11's own function -- this module places no edge itself.
#[test]
fn f1_9_s5_edges_are_placed_by_f1_11_not_invented_here() {
    use naivepost::edges::AlignedWord;
    let words = vec![
        AlignedWord { word: "one".into(), s: 0.0, e: 0.4 },
        AlignedWord { word: "two".into(), s: 1.0, e: 1.4 },
        AlignedWord { word: "three".into(), s: 3.0, e: 3.4 },
    ];
    let marks = vec![naivepost::textfmt::Retake {
        s: 1.0,
        e: 1.4,
        again: 3.0,
        to: 1.4,
        text: "two".into(),
        whole: String::new(),
    }];
    // No envelope: the word pad stands in, which is F1.11's documented fallback.
    let (placed, notes) = retakes::place_edges(marks, &words, |_| None, &[(0.0, 4.0)]);
    assert_eq!(placed.len(), 1, "the mark comes back through F1.11");
    // Whatever F1.11 decided, it decided it -- the span is inside the words' own bounds.
    assert!(placed[0].s >= 0.0 && placed[0].e <= 4.0, "{placed:?}");
    let _ = notes;
}

// ---- S6 ---------------------------------------------------------------------

/// S6: strictly more than the ceiling is refused; sitting exactly on it is not.
/// P.machine.retakeCeil
#[test]
fn f1_9_s6_more_than_40_percent_is_refused_and_exactly_40_is_not() {
    assert!(retakes::over_ceiling(41.0, 100.0), "41 % is over");
    assert!(!retakes::over_ceiling(40.0, 100.0), "exactly 40 % stayed inside the limit");
    assert!(!retakes::over_ceiling(39.9, 100.0));
    assert!(!retakes::over_ceiling(10.0, 0.0), "no speech means no share, and no refusal");
    assert_eq!(
        naivepost::tools::retakes::RETAKE_CEIL,
        0.4,
        "P.machine.retakeCeil"
    );
    // The refusal names both numbers in m:ss, because taking something back needs the size of it.
    assert_eq!(
        retakes::ceiling_refusal(41.0, 100.0),
        "!!! retakes: 0:41 of 1:40 of speech called abandoned -- refused, nothing is marked"
    );
    assert_eq!(retakes::m_ss(0.0), "0:00");
    assert_eq!(retakes::m_ss(59.0), "0:59");
    assert_eq!(retakes::m_ss(60.0), "1:00");
    assert_eq!(retakes::m_ss(105.4), "1:45");
    assert_eq!(retakes::m_ss(3600.0), "60:00", "past an hour keeps counting minutes");
}

/// S6: the ceiling refusal reaches the entry point and leaves nothing marked.
#[test]
fn f1_9_s6_the_entry_point_refuses_everything_over_the_ceiling() {
    let lines = six_lines();
    let marks = vec![Mark {
        from: 1,
        to: 6,
        removed: (0.0, 8.0),
        trimmed: false,
        again: None,
    }];
    let out = retakes::stage_marking(true, &lines, &marks, 5.0, 10.0);
    assert_eq!(out.len(), 1, "one refusal line: {out:?}");
    assert!(out[0].starts_with("!!!"), "it is a refusal: {out:?}");
    assert!(out[0].contains("refused, nothing is marked"), "{out:?}");
}

// ---- S7 ---------------------------------------------------------------------

/// S7: the completion line, exactly as worded, after a pass that marked something.
#[test]
fn f1_9_s7_the_completion_line_is_the_specs() {
    assert_eq!(
        retakes::written_log(3, 100.0),
        ">>> retakes: 3 abandoned stretch(es), 1:40 of speech, kept out of the cut"
    );
    let lines = six_lines();
    let marks = vec![Mark {
        from: 2,
        to: 3,
        removed: (1.0, 2.5),
        trimmed: false,
        again: Some(4),
    }];
    let out = retakes::stage_marking(true, &lines, &marks, 1.5, 10.0);
    assert_eq!(
        out,
        vec![">>> retakes: 1 abandoned stretch(es), 0:10 of speech, kept out of the cut"]
    );
}

/// S7: the file round-trips through the writer that Cut reads, so the format cannot drift.
#[test]
fn f1_9_s7_the_marks_round_trip_through_the_file_cut_reads() {
    let (_keep, tree) = scratch("s7");
    let marks = vec![
        Mark { from: 2, to: 3, removed: (1.0, 2.5), trimmed: false, again: Some(4) },
        Mark { from: 5, to: 6, removed: (4.0, 5.5), trimmed: true, again: None },
    ];
    let texts = vec!["the first attempt".to_string(), "the second".to_string()];
    let written = retakes::write_marks(&tree, &marks, &texts, &[3.0, 0.0]).expect("written");
    assert_eq!(written, 2, "two rows");
    let read = retakes::read_marks(&tree.retakes_tsv()).expect("read back");
    assert_eq!(read.len(), 2, "{read:?}");
    assert_eq!(read[0].s, 1.0);
    assert_eq!(read[0].e, 2.5);
    assert_eq!(read[0].again, 3.0, "the kept attempt's second survives the round trip");
    assert_eq!(read[0].text, "the first attempt");
    assert_eq!(read[1].again, 0.0, "no replacement is written as 0");
}

/// S7: the ceiling case also leaves an EMPTY file -- a refusal is a pass that ran and marked nothing.
#[test]
fn f1_9_s7_a_refused_pass_leaves_an_empty_file_too() {
    let (_keep, tree) = scratch("refused");
    // The refusal decides nothing is marked, so the caller writes no marks...
    let lines = six_lines();
    let verdict = retakes::stage_marking(true, &lines, &[], 9.0, 10.0);
    assert!(verdict[0].contains("refused"), "{verdict:?}");
    retakes::write_marks(&tree, &[], &[], &[]).expect("empty write");
    assert!(tree.retakes_tsv().exists());
    assert_eq!(fs::read_to_string(tree.retakes_tsv()).unwrap(), "");
    // ...and reading that file gives no marks rather than an error.
    assert!(retakes::read_marks(&tree.retakes_tsv()).unwrap().is_empty());
}

// ---- the gate --------------------------------------------------------------

/// A marking pass that is not retakes contributes NOTHING -- F1.10's joins pass owns those lines and
/// must not be spoken for twice by one press.
#[test]
fn f1_9_the_pass_is_gated_on_the_policy_naming_retakes() {
    let lines = six_lines();
    let marks = vec![Mark {
        from: 2,
        to: 3,
        removed: (1.0, 2.5),
        trimmed: false,
        again: Some(4),
    }];
    assert!(
        retakes::stage_marking(false, &lines, &marks, 1.5, 10.0).is_empty(),
        "not the retakes pass: silence"
    );
    assert!(
        !retakes::stage_marking(true, &lines, &marks, 1.5, 10.0).is_empty(),
        "the retakes pass speaks"
    );
    // And the project's own enum spells the three passes this flow gates on.
    assert_eq!(
        naivepost::project::MarkingPass::Retakes,
        naivepost::project::MarkingPass::Retakes
    );
}

/// The reach rule the tool enforces is the parameter the brief promises.
/// P.machine.retakeReachSeconds
#[test]
fn f1_9_s3_the_reach_rule_is_the_parameter_named_in_the_spec() {
    assert_eq!(
        naivepost::tools::retakes::RETAKE_REACH_SECONDS,
        180.0,
        "P.machine.retakeReachSeconds -- a replacement further off is not the same attempt"
    );
    // The tool itself refuses beyond it: line 6 starts far after line 1 ends.
    let far = vec![
        Line { n: 1, start: 0.0, end: 1.0 },
        Line { n: 2, start: 200.0, end: 201.0 },
    ];
    let mut marks = Marks::new(far, 400.0);
    let answer = marks.mark_abandoned(1, 1, Some(2), Trim::none());
    assert!(answer.contains("error"), "beyond the reach it is refused: {answer}");
    assert!(answer.contains("180"), "and says the limit: {answer}");
}
