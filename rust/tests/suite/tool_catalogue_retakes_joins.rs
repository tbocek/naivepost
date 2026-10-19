//! §02-services#3-tool-catalogue-rewrite-directive-b — 3.4 Retake marking and 3.5 Text edit.
//!
//! Both sections are about a model naming something it cannot measure: which lines were abandoned,
//! how many words a seam loses. The tools' side of the bargain is to say back what that becomes in
//! seconds and in words, and to refuse with a sentence rather than to guess. Every boundary below is
//! asserted as a number.

use naivepost::tools::retakes::{self, Line, Marks, Trim};
use naivepost::tools::textedit::{self, Join, Side};

/// Ten lines of two seconds each with a one-second gap after every third — 20 s spoken out of a
/// 30-second session, so the ceiling is easy to read off.
fn session() -> Marks {
    let lines = (1..=10u32)
        .map(|n| {
            let start = (n as f64 - 1.0) * 3.0;
            Line { n, start, end: start + 2.0 }
        })
        .collect();
    Marks::new(lines, 20.0)
}

/// `mark_abandoned` answers with the stretch that will actually be removed and the running share.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s4_a_mark_reports_what_it_removes() {
    let mut marks = session();
    let body = marks.mark_abandoned(1, 3, Some(6), Trim { lead: 0.5, tail: 1.0 });
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("an ok is JSON");

    // Lines 1-3 run 0..8 s; trimming half a second off the front and the repeated tail off the back
    // leaves 0.5..7 — which is what the model needs to hear, because those are not its numbers.
    assert_eq!(parsed["removed"][0], serde_json::json!(0.5));
    assert_eq!(parsed["removed"][1], serde_json::json!(7.0));
    assert_eq!(parsed["seconds"], serde_json::json!(6.5));
    assert_eq!(parsed["trimmed"], serde_json::json!(true));
    assert_eq!(parsed["merged"], serde_json::json!(false));
    assert_eq!(parsed["again"], serde_json::json!(6));

    // The running share of the session's speech: 6.5 s of 20 s spoken.
    assert_eq!(parsed["speech_marked"], serde_json::json!(0.325));
    // P.machine.retakeCeil
    assert_eq!(parsed["ceiling"], serde_json::json!(0.4));

    // Lines run every 3 s with 2 s of speech, so line 8 starts at 21 and line 9 ends at 26. With no
    // trim the removal is exactly the lines named and `trimmed` says so.
    let body = marks.mark_abandoned(8, 9, Some(10), Trim::none());
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["removed"][0], serde_json::json!(21.0));
    assert_eq!(parsed["removed"][1], serde_json::json!(26.0));
    assert_eq!(parsed["trimmed"], serde_json::json!(false));

    // `again` 0 -- never picked up -- is always legal: no replacement to be near, so neither the
    // reach rule nor the inside rule has anything to check.
    let body = marks.mark_abandoned(10, 10, None, Trim::none());
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(parsed.get("error").is_none(), "{body}");
    assert_eq!(parsed["again"], serde_json::Value::Null);

    // A mark whose trim leaves nothing is a refusal, not an empty ok.
    let body = marks.mark_abandoned(4, 4, None, Trim { lead: 5.0, tail: 5.0 });
    assert!(body.contains("breath"), "{body}");
}

/// The four refusals §3.4 names, each in its own sentence.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s4_every_refusal_names_its_reason() {
    let mut marks = session();
    let reason = |body: String| -> String {
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // 1. The replacement is inside the stretch being abandoned. §3.4 gives this wording outright, so
    // only a-b and c change.
    let body = reason(marks.mark_abandoned(2, 6, Some(4), Trim::none()));
    assert_eq!(
        body,
        "lines 2-6 say they are said again at line 4, which is inside them -- not a mark"
    );

    // 2. Out of range: the session runs 1..=10 and says so.
    let body = reason(marks.mark_abandoned(9, 12, None, Trim::none()));
    assert!(body.contains("out of range"), "{body}");
    assert!(body.contains("1..=10"), "{body}");
    // A reversed range is out of range for the same reason.
    let body = reason(marks.mark_abandoned(6, 2, None, Trim::none()));
    assert!(body.contains("out of range"), "{body}");

    // 3. The replacement is farther than the reach. P.machine.retakeReachSeconds
    assert_eq!(retakes::RETAKE_REACH_SECONDS, 180.0);
    // A session whose second line starts exactly one reach after line 1 ends...
    let mut at_limit = Marks::new(
        vec![
            Line { n: 1, start: 0.0, end: 2.0 },
            Line { n: 2, start: 182.0, end: 184.0 },
            Line { n: 3, start: 400.0, end: 402.0 },
        ],
        6.0,
    );
    // ...is allowed: the gap is exactly 180 s, and a limit that refused itself would be one short.
    let body = at_limit.mark_abandoned(1, 1, Some(2), Trim::none());
    assert!(!body.contains("error"), "exactly the reach is the limit: {body}");
    // ...and one tenth further is not the same attempt.
    let mut past = Marks::new(
        vec![
            Line { n: 1, start: 0.0, end: 2.0 },
            Line { n: 2, start: 182.1, end: 184.0 },
            Line { n: 3, start: 400.0, end: 402.0 },
        ],
        6.0,
    );
    let body = reason(past.mark_abandoned(1, 1, Some(2), Trim::none()));
    assert!(body.contains("not the same attempt"), "{body}");
    assert!(body.contains("180"), "{body}");

    // A replacement line that does not exist is refused with the `again` 0 hint.
    let body = reason(marks.mark_abandoned(1, 2, Some(99), Trim::none()));
    assert!(body.contains("send 0"), "{body}");

    // 4. Shorter than a breath. P.machine.retakeMinSeconds
    assert_eq!(retakes::RETAKE_MIN_SECONDS, 0.3);
    let mut tight = Marks::new(
        vec![
            Line { n: 1, start: 0.0, end: 0.3 },
            Line { n: 2, start: 5.0, end: 6.0 },
        ],
        10.0,
    );
    // Exactly 0.3 s is allowed; the rule is "shorter than", not "at most".
    let body = tight.mark_abandoned(1, 1, None, Trim::none());
    assert!(!body.contains("error"), "exactly the minimum is a mark: {body}");
    let mut short = Marks::new(
        vec![
            Line { n: 1, start: 0.0, end: 0.29 },
            Line { n: 2, start: 5.0, end: 6.0 },
        ],
        10.0,
    );
    let body = reason(short.mark_abandoned(1, 1, None, Trim::none()));
    assert!(body.contains("breath"), "{body}");
    assert!(body.contains("0.29"), "{body}");

    // Nothing was recorded by any of the refusals: a refusal costs no share.
    let total = marks.finish();
    assert_eq!(total.share, 0.0);
    assert_eq!(total.marks, 0);
}

/// Overlapping marks merge, and `unmark` takes seconds back.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s4_marks_merge_and_can_be_taken_back() {
    let mut marks = session();
    marks.mark_abandoned(1, 3, None, Trim::none());
    // Touching the first mark's end: one stretch, not two, and the ok says so.
    let body = marks.mark_abandoned(3, 5, None, Trim::none());
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["merged"], serde_json::json!(true));
    assert_eq!(marks.marks().len(), 1);
    // Lines 1-3 run 0..8 s and lines 3-5 run 6..14 s, so the union is one stretch of 0..14.
    assert_eq!(marks.marks()[0].removed, (0.0, 14.0));

    // Over the ceiling: 14 s of 20 s spoken. Nothing has been thrown away — the prototype discarded
    // all three runs here, which is why a model that had marked carefully lost everything.
    let total = marks.finish();
    assert_eq!(total.share, 0.7);
    assert!(total.over);

    // Taking some back lowers the share and leaves the rest standing. Lines 4-5 run 9..14 s, so five
    // seconds come out of the merged mark and 0..9 remain.
    let body = marks.unmark(4, 5);
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["seconds_taken_back"], serde_json::json!(5.0));
    assert!((marks.finish().share - 0.45).abs() < 1e-9, "{:?}", marks.finish());
    assert!(marks.finish().over, "still over the ceiling after one take-back");
    // What is left of the mark is what was not asked back.
    assert_eq!(marks.marks()[0].removed, (0.0, 9.0));

    // One more take-back comes under it — which is the whole point of telling the model here rather
    // than discarding its work at the end.
    marks.unmark(1, 2);
    assert!(!marks.finish().over, "{:?}", marks.finish());

    // Unmarking outside the range is refused like a mark would be.
    let body = marks.unmark(11, 12);
    assert!(body.contains("out of range"), "{body}");
}

/// `get_lines` returns the pause before every line — including the short ones the brief omits.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s4_the_short_pauses_are_included() {
    // P.machine.retakePauseSeconds
    assert_eq!(retakes::RETAKE_PAUSE_SECONDS, 1.5);

    // Gaps of 0.9, 1.4 and 2.0 s: the first two are under the threshold, and they are exactly where a
    // speaker stops and starts again.
    let marks = Marks::new(
        vec![
            Line { n: 1, start: 0.0, end: 2.0 },
            Line { n: 2, start: 2.9, end: 4.0 },
            Line { n: 3, start: 5.4, end: 6.0 },
            Line { n: 4, start: 8.0, end: 9.0 },
        ],
        9.0,
    );
    let with_pause = marks.lines_with_pause(1, 4);
    let pauses: Vec<f64> = with_pause.iter().map(|(_, pause)| *pause).collect();
    assert!((pauses[1] - 0.9).abs() < 1e-9, "{pauses:?}");
    assert!((pauses[2] - 1.4).abs() < 1e-9, "{pauses:?}");
    assert!((pauses[3] - 2.0).abs() < 1e-9, "{pauses:?}");
    // Both sub-threshold gaps are present rather than filtered out.
    let under = pauses.iter().filter(|p| **p < retakes::RETAKE_PAUSE_SECONDS).count();
    assert_eq!(under, 3, "the first line has no pause before it: {pauses:?}");

    // A range asks for its own lines only, and the pause of the first is measured against the line
    // before it in the session rather than reported as zero.
    let tail = marks.lines_with_pause(3, 4);
    assert_eq!(tail.len(), 2);
    assert!((tail[0].1 - 1.4).abs() < 1e-9);

    // The same rule as a function, for a caller holding only the lines.
    assert!((retakes::pause_before(marks.lines(), 2) - 0.9).abs() < 1e-9);
    assert_eq!(retakes::pause_before(marks.lines(), 99), 0.0);
}

/// `finish` answers with the total marked against the ceiling, over or not.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s4_finish_reads_the_share_against_the_ceil(
) {
    // P.machine.retakeCeil
    assert_eq!(retakes::RETAKE_CEIL, 0.4);

    let mut marks = session();
    let total = marks.finish();
    assert_eq!((total.share, total.ceiling, total.over), (0.0, 0.4, false));
    assert_eq!(total.marks, 0);

    // 5 s of 20 s spoken: under the ceiling.
    marks.mark_abandoned(1, 2, None, Trim::none());
    let total = marks.finish();
    assert!((total.share - 0.25).abs() < 1e-9, "{total:?}");
    assert!(!total.over);

    // Past it: the model is told while there is still something to take back. Lines 4-8 run 9..23 s,
    // which takes the marked total over half of what was spoken.
    marks.mark_abandoned(4, 8, None, Trim::none());
    let total = marks.finish();
    assert!((total.share - 0.95).abs() < 1e-9, "{total:?}");
    assert!(total.share > retakes::RETAKE_CEIL, "{total:?}");
    assert!(total.over);
    assert_eq!(total.marks, 2);

    // A session with no spoken seconds cannot be over anything: a share of nothing is zero rather
    // than a division that reports infinity.
    let empty = Marks::new(vec![Line { n: 1, start: 0.0, end: 5.0 }], 0.0);
    assert_eq!(empty.finish().share, 0.0);
}

/// `drop_words` names the exact words that go and the sentence left reading across the join.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s5_a_drop_names_the_words_it_takes() {
    let mut join = Join::new(
        &["so", "that", "is", "the", "first"],
        &["take", "and", "then", "the", "next"],
    );

    // Two words off the front of the second take: they are named, in order, and what remains reads.
    let body = join.drop_words(Side::After, 2);
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("an ok is JSON");
    assert_eq!(parsed["side"], serde_json::json!("after"));
    assert_eq!(parsed["count"], serde_json::json!(2));
    assert_eq!(parsed["words"][0], serde_json::json!("take"));
    assert_eq!(parsed["words"][1], serde_json::json!("and"));
    assert_eq!(
        parsed["across_the_join"].as_str().unwrap(),
        "so that is the first then the next"
    );

    // A second drop on the same side adds to the first rather than replacing it.
    let body = join.drop_words(Side::After, 1);
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["count"], serde_json::json!(3));
    assert_eq!(join.words_taken(Side::After).len(), 3);
    assert_eq!(join.sentence(), "so that is the first the next");

    // Words off the end of the first take come off its tail -- nearest the join is what touches it.
    let mut other = Join::new(&["one", "two", "three"], &["four", "five"]);
    other.drop_words(Side::Before, 2);
    assert_eq!(other.words_taken(Side::Before), vec!["two", "three"]);
    assert_eq!(other.sentence(), "one four five");
}

/// The four errors a `drop_words` can meet.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s5_a_drop_is_refused_before_it_costs() {
    let mut join = Join::new(&["a", "b", "c", "d"], &["e", "f", "g", "h"]);
    let reason = |body: String| -> String {
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // (a) A stretch of no words is not at the join; keep_join is the answer that means that.
    let body = reason(join.drop_words(Side::Before, 0));
    assert!(body.contains("keep_join"), "{body}");
    assert_eq!(join.dropped_count(Side::Before), 0);

    // (b) Past what was shown. The side has four words, so five were never on the table.
    let body = reason(join.drop_words(Side::After, 5));
    assert!(body.contains("only 4 were shown"), "{body}");
    assert!(body.contains("after"), "{body}");

    // (d) More than the ceiling of the words shown. Eight words are shown, so 60% is 4.8: five of one
    // side is over it and four is not. P.machine.seamCeil
    assert_eq!(textedit::SEAM_CEIL, 0.6);
    let body = join.drop_words(Side::After, 4);
    assert!(!body.contains("error"), "four of eight is under the ceiling: {body}");
    // And the fifth word on top of those four is where it bites -- counted across calls, not per call.
    let body = reason(join.drop_words(Side::After, 1));
    assert!(body.contains("60%"), "{body}");
    assert_eq!(join.dropped_count(Side::After), 4, "the refusal took nothing more");

    // (c) More than the word maximum, counted across calls rather than per call.
    // P.machine.seamMaxWords
    assert_eq!(textedit::SEAM_MAX_WORDS, 40);
    let words: Vec<&str> = (0..120).map(|n| if n % 2 == 0 { "x" } else { "y" }).collect();
    let before = &words[..60];
    let after = &words[60..];
    let mut long = Join::new(before, after);
    // Thirty is under the maximum on its own...
    assert!(!long.drop_words(Side::Before, 30).contains("error"));
    // ...and thirty more would be sixty, which no single join may remove.
    let body = reason(long.drop_words(Side::Before, 30));
    assert!(body.contains("at most 40"), "{body}");
    assert_eq!(long.dropped_count(Side::Before), 30, "the refusal took nothing more");

    // The shown set is capped at the reach, so the ceiling counts against what was given.
    // P.machine.seamReachWords
    assert_eq!(textedit::SEAM_REACH_WORDS, 140);
    assert_eq!(long.shown_count(Side::Before), 140.min(60));
}

/// `keep_join` is a whole answer that removes nothing; `get_words` reads past the shown cap.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s5_keeping_is_an_answer_and_words_are_cheap(
) {
    let mut join = Join::new(&["one", "two"], &["three", "four"]);
    join.drop_words(Side::After, 1);
    assert_eq!(join.dropped_count(Side::After), 1);

    // The whole answer: nothing removed, and the drops made before it are off the table.
    let body = join.keep_join();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["kept"], serde_json::json!(true));
    assert_eq!(parsed["removed"], serde_json::json!(0));
    assert!(join.kept());
    assert_eq!(join.dropped_count(Side::After), 0);
    assert_eq!(join.sentence(), "one two three four");

    // get_words is not capped by the reach: asking for more than was shown is how a model checks a
    // seam that looked longer than the extract.
    let words: Vec<&str> = (0..300).map(|n| if n < 10 { "a" } else { "b" }).collect();
    let join = Join::new(&words, &words);
    assert_eq!(join.shown_count(Side::Before), textedit::SEAM_REACH_WORDS); // P.machine.seamReachWords
    let more = join.get_words(Side::Before, 200);
    assert_eq!(more.len(), 200, "past the shown cap");
    // Past what exists it returns the whole side rather than padding.
    assert_eq!(join.get_words(Side::After, 10_000).len(), 300);

    // The words nearest the join are the ones a drop would take, on both sides.
    let small = Join::new(&["one", "two", "three"], &["four", "five"]);
    assert_eq!(small.get_words(Side::Before, 2), vec!["two", "three"]);
    assert_eq!(small.get_words(Side::After, 2), vec!["four", "five"]);
}

/// The app still re-derives the deleted stretch to check it is one stretch at the join.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s5_two_stretches_are_not_one_join() {
    // One run, on one side: that is a stretch at the join.
    assert!(textedit::one_stretch_at_join(&[3], &[]));
    assert!(textedit::one_stretch_at_join(&[], &[7]));

    // Nothing at all is not a stretch -- it is keep_join, which is a different answer.
    assert!(!textedit::one_stretch_at_join(&[], &[]));
    // A zero-length run names words that are not there: a gap, not a stretch.
    assert!(!textedit::one_stretch_at_join(&[0], &[]));

    // Two runs on one side are two stretches with live words between them.
    assert!(!textedit::one_stretch_at_join(&[2, 3], &[]));
    // And one run on each side is a join in the middle of what would be removed.
    assert!(!textedit::one_stretch_at_join(&[2], &[2]));
    assert!(!textedit::one_stretch_at_join(&[1, 1, 1], &[]));
}
