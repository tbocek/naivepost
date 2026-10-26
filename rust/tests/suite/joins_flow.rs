//! F1.10 Repair the joins — the flow rules, with no window and no model server.
//!
//! Every fixture here hands the answers in as text, which is what makes the checks possible: the
//! requests themselves need a live textedit server, and nothing in this file asks for one. The
//! worked examples are the spec's own (ETH lecture, 57 joins), so an assertion that fails against
//! them is a rule misread rather than a fixture mistake.
//!
//! Step tests: `f1_10_s1_*` (the words a join pass may use) through `f1_10_s6_*` (both files
//! written together). The run reaches the pass as the fix stage's merge row (`f1_8_fix_session`), never at the ▶ press
//! (`f0_2_s4_the_press_asks_no_model`).

use naivepost::edges::AlignedWord;
use naivepost::hand_edit;
use naivepost::joins;
use naivepost::layout::Tree;
use naivepost::textfmt::{self, Retake};
use naivepost::tools::textedit::{Join, Side};
use naivepost::word_list::{self, Word};

/// One word of a synthetic session list, one second long so word `i` spans `(i, i+1)`.
fn w(source: &str, written: &str, start: f64, end: f64) -> Word {
    Word {
        source: source.to_string(),
        match_word: word_list::bare(written),
        written: written.to_string(),
        start,
        end,
        stray: false,
    }
}

/// A run of words from one recording, each one second, starting at `start`.
fn run(source: &str, words: &[&str], start: f64) -> Vec<Word> {
    words
        .iter()
        .enumerate()
        .map(|(n, word)| w(source, word, start + n as f64, start + n as f64 + 1.0))
        .collect()
}

/// Two recordings meeting at a seam: `take-a` then `take-b`, four words each.
fn two_takes() -> Vec<Word> {
    let mut words = run("take-a", &["one", "two", "three", "four"], 0.0);
    words.extend(run("take-b", &["five", "six", "seven", "eight"], 4.0));
    words
}

fn strings(words: &[String]) -> Vec<&str> {
    words.iter().map(String::as_str).collect()
}

// ---- S1: the words a join pass may use ----------------------------------

#[test]
fn f1_10_s1_three_joinable_words_is_too_few_and_four_is_not() {
    let three = run("take-a", &["a", "b", "c"], 0.0);
    assert!(joins::too_few(&three), "three words cannot hold a join");
    let four = run("take-a", &["a", "b", "c", "d"], 0.0);
    assert!(
        !joins::too_few(&four),
        "four is the bound itself, and the bound is joinable"
    );
    // The sentence names the count it fell short by, so the log is readable without knowing MIN_WORDS.
    assert_eq!(
        joins::too_few_log(3),
        ">>> text edit: nothing to repair -- 3 joinable words, fewer than 4"
    );
}

#[test]
fn f1_10_s1_the_narrator_mic_is_left_out_of_the_join_window() {
    // A voice-over mic is heard by the app and not played by the video, so a seam into it is not a
    // seam in the finished film at all.
    let mut words = run("take-a", &["one", "two"], 0.0);
    words.extend(run("voice", &["nnn", "ooo"], 2.0));
    words.extend(run("take-b", &["three", "four"], 4.0));
    // `dropped` is indexed by the caller's OWN list: absent means "this pass has no opinion", so a
    // short slice drops nothing rather than dropping everything.
    let dropped = vec![];
    let joinable = joins::joinable_words(&words, Some("voice"), &dropped);
    let sources: Vec<&str> = joinable.iter().map(|word| word.source.as_str()).collect();
    assert_eq!(
        sources,
        vec!["take-a", "take-a", "take-b", "take-b"],
        "the narrator's words never reach the join model"
    );
    // With no narrator named they are all there, which is the same call on a session with no mic.
    let all = joins::joinable_words(&words, None, &dropped);
    assert_eq!(all.len(), 6, "nothing excluded when no narrator is named");
}

#[test]
fn f1_10_s1_words_an_earlier_join_took_are_left_out_so_it_cannot_be_blamed_twice() {
    // The 2026-09-18 fix. Shown a repeat that a previous join already removed, the one-stretch check
    // counts THAT stretch against this join and refuses an answer that was right.
    let words = two_takes();
    let _none_gone = vec![false; words.len()];
    assert_eq!(
        joins::seams(&words),
        vec![4],
        "one seam, at take-b's first word"
    );

    // Take the whole of take-a out (an earlier join's doing) and the seam moves to where the film now
    // actually changes.
    let gone = vec![true, true, true, true, false, false, false, false];
    let joinable = joins::joinable_words(&words, None, &gone);
    assert_eq!(joinable.len(), 4, "only take-b survives the earlier join");
    assert_eq!(
        joins::seams(&joinable),
        Vec::<usize>::new(),
        "with the earlier half already gone there is no seam left to ask about"
    );
    // And a drop in the MIDDLE leaves a seam, just a different one: the check is about location, not
    // about how many words went.
    let middle = vec![false, true, false, false, false, false, false, false];
    let joinable = joins::joinable_words(&words, None, &middle);
    assert_eq!(joins::seams(&joinable), vec![3]);
}

// ---- S2: the seams ------------------------------------------------------

#[test]
fn f1_10_s2_a_seam_is_where_one_recording_ends_and_another_begins() {
    assert_eq!(joins::seams(&two_takes()), vec![4]);
    let three = {
        let mut words = run("a", &["x", "y"], 0.0);
        words.extend(run("b", &["p", "q"], 2.0));
        words.extend(run("c", &["m", "n"], 4.0));
        words
    };
    assert_eq!(
        joins::seams(&three),
        vec![2, 4],
        "each change of source is one seam"
    );
    let one = run("only", &["a", "b", "c", "d"], 0.0);
    assert_eq!(
        joins::seams(&one),
        Vec::<usize>::new(),
        "one recording has no seam"
    );
    assert_eq!(
        joins::no_seam_log(),
        ">>> text edit: one recording, no seam to repair -- every word stands"
    );
}

// ---- S3: what one join is asked -----------------------------------------

#[test]
fn f1_10_s3_the_request_is_the_specs_message_for_one_join() {
    let before: Vec<String> = ["die", "parallele", "Ausführung", "ermöglichen"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let after: Vec<String> = ["was", "die", "Gas-Limits", "nachoben"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let wide = "die parallele Ausführung ermöglichen [CUT] was die Gas-Limits nachoben";
    let ask = joins::request("", 23, 57, wide, &before, &after, None);
    assert!(
        ask.contains("JOIN 23 of 57."),
        "the join is numbered: {ask}"
    );
    assert!(
        ask.contains("WIDE VIEW (this join is at [CUT]") && ask.contains(wide),
        "the wide view rides along, marked: {ask}"
    );
    assert!(
        ask.contains("CLOSE TO THE CUT:\ndie parallele Ausführung ermöglichen [CUT] was die Gas-Limits nachoben"),
        "the close-up shows the words either side of [CUT]: {ask}"
    );
    assert!(
        ask.contains("BEFORE (the end of the take that was interrupted):\n...die parallele Ausführung ermöglichen"),
        "BEFORE is labelled as the spec labels it: {ask}"
    );
    assert!(
        ask.contains("AFTER (the whole take that follows, up to where the next one starts):\nwas die Gas-Limits nachoben"),
        "AFTER is labelled as the spec labels it: {ask}"
    );
    assert!(
        ask.contains(r#"Answer {"drop_before":"...","drop_after":"..."} and nothing else"#),
        "the shape of the answer is stated: {ask}"
    );
    assert!(
        !ask.contains("previous answer"),
        "a first ask carries no refusal: {ask}"
    );
    // An empty User Context adds NO header over nothing.
    assert!(
        !ask.contains("USER CONTEXT"),
        "no context means no context block: {ask}"
    );
}

/// The close-up is CLOSE_WORDS either side and BEFORE prints its last BEFORE_WORDS: the wide view carries the
/// rest, and a long window is not printed whole three times over.
#[test]
fn f1_10_s3_the_close_up_and_before_are_cut_to_their_bounds() {
    let before: Vec<String> = (0..40).map(|n| format!("b{n}")).collect();
    let after: Vec<String> = (0..30).map(|n| format!("a{n}")).collect();
    let ask = joins::request("", 1, 1, "wide", &before, &after, None);
    let close = ask
        .split("CLOSE TO THE CUT:\n")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .expect("a close-up");
    assert_eq!(
        close,
        format!(
            "{} [CUT] {}",
            before[40 - joins::CLOSE_WORDS..].join(" "),
            after[..joins::CLOSE_WORDS].join(" ")
        )
    );
    let shown_before = ask
        .split("BEFORE (the end of the take that was interrupted):\n...")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .expect("BEFORE");
    assert_eq!(shown_before, before[40 - joins::BEFORE_WORDS..].join(" "));
}

/// A retry is told why its predecessor was refused, so it does not answer the same way again.
#[test]
fn f1_10_s3_a_retry_is_told_why_the_last_answer_was_refused() {
    let before = split("alpha beta");
    let after = split("gamma delta");
    let ask = joins::request(
        "",
        3,
        9,
        "alpha beta [CUT] gamma delta",
        &before,
        &after,
        Some("\"beta gamma\" is not how BEFORE ends -- quote its last words exactly as shown"),
    );
    assert!(
        ask.ends_with("Your previous answer at this join was refused: \"beta gamma\" is not how BEFORE ends -- quote its last words exactly as shown. Answer again."),
        "the refusal closes the ask: {ask}"
    );
}

#[test]
fn f1_10_s3_a_written_context_outranks_the_job_rules_in_the_same_message() {
    let before = vec!["alpha".to_string()];
    let after = vec!["beta".to_string()];
    let ask = joins::request(
        "Cut nothing before the demo.",
        1,
        1,
        "alpha [CUT] beta",
        &before,
        &after,
        None,
    );
    assert!(
        ask.contains("USER CONTEXT"),
        "the header rides with a real context: {ask}"
    );
    assert!(
        ask.contains("Cut nothing before the demo."),
        "the context is passed verbatim: {ask}"
    );
    assert!(
        ask.contains("JOIN 1 of 1."),
        "and the join still gets its own number: {ask}"
    );
}

/// Three takes: a, a short abandoned b, then c that says it again.
fn three_takes() -> Vec<Word> {
    let mut words = run("take-a", &["one", "two", "three", "four"], 0.0);
    words.extend(run("take-b", &["and", "five", "six"], 4.0));
    words.extend(run("take-c", &["and", "five", "six", "seven"], 7.0));
    words
}

/// AFTER is the take that follows and stops where the next one starts: past it lies the next join's stumble,
/// which the model took for this one's and cut into the good take (07-news, 2026-10-26).
#[test]
fn f1_10_s3_after_stops_where_the_next_take_starts() {
    let words = three_takes();
    let seams = joins::seams(&words);
    assert_eq!(seams, vec![4, 7], "two seams: a|b and b|c");
    let kept = vec![true; words.len()];
    let after = naivepost::join_ask::window_after(&words, &kept, seams[0], seams[1]);
    assert_eq!(
        strings(&after),
        ["and", "five", "six"],
        "take-b alone, not take-c"
    );
    let last = naivepost::join_ask::window_after(&words, &kept, seams[1], words.len());
    assert_eq!(
        strings(&last),
        ["and", "five", "six", "seven"],
        "the last join's AFTER runs to the end of the list"
    );
}

/// The wide view puts [CUT] at this join and marks every other join, so the model does not take another
/// join's stumble for its own.
#[test]
fn f1_10_s3_the_wide_view_marks_this_join_and_the_others() {
    let words = three_takes();
    let seams = joins::seams(&words);
    let kept = vec![true; words.len()];
    assert_eq!(
        joins::wide_view(&words, &kept, seams[0], &seams, 60),
        "one two three four [CUT] and five six [another take starts] and five six seven"
    );
    assert_eq!(
        joins::wide_view(&words, &kept, seams[1], &seams, 60),
        "one two three four [another take starts] and five six [CUT] and five six seven"
    );
    // Its reach counts standing words either side, and a word an earlier join took is not shown: with "six"
    // gone, the two standing words before the second join are take-b's first two, whose start is a join.
    let mut gone = kept.clone();
    gone[6] = false;
    assert_eq!(
        joins::wide_view(&words, &gone, seams[1], &seams, 2),
        "[another take starts] and five [CUT] and five"
    );
}

#[test]
fn f1_10_s3_thinking_is_on_for_this_job() {
    // P.machine.textEditThinking: measured 15/29 joins right without thinking, 20/29 with.
    assert!(joins::thinking_on(), "the join pass thinks");
}

#[test]
fn f1_10_s3_the_bounds_are_the_parameter_rows_named_in_the_spec() {
    // P.machine.seamReachWords 140, P.machine.seamSnapWords 3, P.machine.seamMaxWords 40,
    // P.machine.seamCeil 0.6, P.machine.seamRetries 1, P.eng.joinReachWords 3.
    assert_eq!(
        joins::bounds(),
        (140, 3, 40, 0.6, 1, 3),
        "every number the join rules count with"
    );
    assert_eq!(joins::noise_words(), 2, "P.machine.seamNoiseWords");
}

// ---- S3: reading the quoted answer ----------------------------------------

/// The ETH join-23 shape: BEFORE ends mid-sentence, AFTER restarts it.
fn eth_join23() -> (Vec<String>, Vec<String>) {
    let before = split(
        "und Block-Level level Access Lists die parallele Ausführung ermöglichen und den Weg \
         zu einer Gas-Limit was die Gas-Limits nach oben setzen sollte",
    );
    let after = split("Und die Native Account Abstraction für das übernächste version also");
    (before, after)
}

fn split(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// Built so the enum's field names stay visible in the assertion without repeating the type path.
fn applied(before: usize, after: usize) -> joins::Outcome {
    joins::Outcome::Applied { before, after }
}

#[test]
fn f1_10_s3_a_quote_of_befores_last_words_is_applied() {
    let (before, after) = eth_join23();
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"was die Gas-Limits nach oben setzen sollte","drop_after":""}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    assert_eq!(
        outcome,
        applied(7, 0),
        "the quoted tail of BEFORE is what goes: {outcome:?}"
    );
    assert_eq!(outcome.removed(), 7, "the seven-word false start goes");
    assert!(outcome.usable(), "a used answer may be cached");
}

/// The quote is compared bare: the case and the punctuation are the transcript's, not the word's, so a comma
/// the model did not copy, or a capital it added, is no obstacle.
#[test]
fn f1_10_s3_a_quote_matches_whatever_its_case_and_punctuation() {
    let before = split("Finma justified the exception with Bitcoin's long-term");
    let after = split("With Bitcoin's long trading history, you could buy");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"WITH bitcoin's long-term.","drop_after":""}"#;
    assert_eq!(
        joins::answer_join(&mut join, reply, &before, &after),
        applied(3, 0)
    );
    // And on the AFTER side: its first words, punctuation dropped.
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"","drop_after":"with bitcoins long trading history"}"#;
    assert_eq!(
        joins::answer_join(&mut join, reply, &before, &after),
        applied(0, 5)
    );
}

/// Both sides at once is one stretch through the seam, and allowed.
#[test]
fn f1_10_s3_a_quote_from_each_side_is_one_stretch_through_the_join() {
    let before = split("alpha beta gamma delta epsilon zeta");
    let after = split("eta theta iota kappa lambda");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"zeta","drop_after":"eta"}"#;
    assert_eq!(
        joins::answer_join(&mut join, reply, &before, &after),
        applied(1, 1)
    );
}

#[test]
fn f1_10_s3_removing_nothing_is_keep_join_not_a_refusal() {
    let before = split("alpha beta gamma");
    let after = split("delta epsilon");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"","drop_after":""}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    assert_eq!(
        outcome,
        joins::Outcome::Kept,
        "nothing left out is the ordinary answer"
    );
    assert_eq!(outcome.removed(), 0, "a kept join removes nothing");
    assert!(outcome.usable(), "keep_join is a usable answer and caches");
}

/// A quote that is not how BEFORE ends (words out of its middle) is refused with the sentence the retry is shown.
#[test]
fn f1_10_s3_a_quote_that_is_not_how_before_ends_is_refused() {
    let before = split("bei Fusaka Das ist der aktuellste Release und wurde im Dezember aktiviert");
    let after = split("und wurde im Dezember aktiviert und führte dazu dass die Kosten");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"der aktuellste Release","drop_after":""}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    assert_eq!(
        outcome,
        joins::Outcome::Refused(
            "\"der aktuellste Release\" is not how BEFORE ends -- quote its last words exactly as shown"
                .to_string()
        )
    );
    assert!(!outcome.usable(), "a refusal is never cached");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"","drop_after":"im Dezember"}"#;
    assert_eq!(
        joins::answer_join(&mut join, reply, &before, &after),
        joins::Outcome::Refused(
            "\"im Dezember\" is not how AFTER starts -- quote its first words exactly as shown"
                .to_string()
        )
    );
}

#[test]
fn f1_10_s3_more_than_seamMaxWords_is_refused_with_the_tools_own_sentence() {
    // P.machine.seamMaxWords = 40.
    let big_before: Vec<String> = (0..60).map(|n| format!("w{n}")).collect();
    let after = split("tail word");
    let mut join = Join::new(&strings(&big_before), &strings(&after));
    // Quote all but the first two words of BEFORE: 58 go, well past the bound.
    let reply = format!(
        r#"{{"drop_before":"{}","drop_after":""}}"#,
        big_before[2..].join(" ")
    );
    let outcome = joins::answer_join(&mut join, &reply, &big_before, &after);
    match &outcome {
        joins::Outcome::Refused(reason) => assert!(
            reason.contains("at most") && reason.contains('4'),
            "the tool's ceiling sentence reaches the caller: {reason}"
        ),
        other => panic!("58 words must not pass a 40-word bound: {other:?}"),
    }
}

#[test]
fn f1_10_s3_more_than_seamCeil_of_the_shown_words_is_refused() {
    // P.machine.seamCeil = 0.6 of what was SHOWN. Eight shown, five wanted gone is 62 %.
    let before = split("aa bb cc dd ee ff");
    let after = split("gg hh");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"drop_before":"bb cc dd ee ff","drop_after":""}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    match &outcome {
        joins::Outcome::Refused(reason) => assert!(
            reason.contains('%') || reason.contains("two takes"),
            "the ceiling refusal explains itself: {reason}"
        ),
        other => panic!("five of eight shown must be refused: {other:?}"),
    }
}

#[test]
fn f1_10_s3_a_reply_that_is_not_the_answer_shape_is_refused() {
    let before = split("alpha beta");
    let after = split("gamma delta");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let not_json = joins::answer_join(&mut join, "I think the join reads fine", &before, &after);
    assert!(
        matches!(&not_json, joins::Outcome::Refused(r) if r.contains("drop_before")),
        "no JSON means no quote: {not_json:?}"
    );
    let mut join = Join::new(&strings(&before), &strings(&after));
    let empty_object = joins::answer_join(&mut join, "{}", &before, &after);
    assert!(
        matches!(&empty_object, joins::Outcome::Refused(r) if r.contains("drop_before")),
        "an object without the fields is refused the same way: {empty_object:?}"
    );
    // A fence around the object is tolerated.
    let mut join = Join::new(&strings(&before), &strings(&after));
    let fenced = joins::answer_join(
        &mut join,
        "```json\n{\"drop_before\":\"beta\",\"drop_after\":\"\"}\n```",
        &before,
        &after,
    );
    assert_eq!(fenced, applied(1, 0), "a fenced answer is still read");
    assert!(!not_json.usable() && !empty_object.usable());
}

#[test]
fn f1_10_s3_the_refusal_line_names_the_join_the_reason_and_the_tail() {
    // P.machine.seamRetries governs the tail: "asking once more" while one is owed, "nothing
    // removed there" on the last ask.
    assert_eq!(
        joins::refusal_line(22, "two stretches left out, not one at the join", "nothing removed there"),
        "!!! text edit: join 22: two stretches left out, not one at the join -- nothing removed there"
    );
    assert_eq!(
        joins::refusal_line(7, "no words in the answer", "asking once more"),
        "!!! text edit: join 7: no words in the answer -- asking once more"
    );
}

#[test]
fn f1_10_s3_a_retry_gets_its_own_cache_slot() {
    // Without a distinct slot per attempt the retry is served straight back the refusal it was meant
    // to escape -- which is why P.machine.seamRetries needed a round of its own.
    let base = joins::ask_slot("join-23-request", 0);
    let retry = joins::ask_slot("join-23-request", 1);
    assert_ne!(base, retry, "the retry must key elsewhere");
    assert_eq!(
        base, "join-23-request",
        "the first ask is the request unchanged"
    );
}

// ---- S4: the doubled saying straight across a cut -----------------------

#[test]
fn f1_10_s4_a_saying_repeated_across_the_cut_loses_its_earlier_copy() {
    // P.eng.joinReachWords = 3 either side. The function's contract is a DROPPED run with kept
    // words on both sides: it takes up to three KEPT words nearest the cut on each side and drops
    // the earlier copy of whatever the far side begins with. So the fixture repeats "up fast" over
    // the cut, with one dropped word between the two sayings.
    let words = split("the result goes up fast again up fast now");
    let times: Vec<(f64, f64)> = (0..words.len())
        .map(|i| (i as f64, i as f64 + 1.0))
        .collect();
    let mut kept = vec![true; words.len()];
    // Drop the "again" (index 5) that sits between the two sayings of "up fast".
    kept[5] = false;
    let notes = joins::dedupe_across(&words, &times, &mut kept);
    assert_eq!(notes.len(), 1, "one join fixed: {notes:?}");
    assert!(
        notes[0].contains("said again straight after the cut -- the earlier one goes"),
        "the note is the spec's: {}",
        notes[0]
    );
    // The earlier "up fast" (indices 3, 4) went with it; the later one (7, 8) stays.
    assert!(!kept[3] && !kept[4], "the earlier saying went: {kept:?}");
    assert!(kept[7] && kept[8], "the later saying stays: {kept:?}");
    assert!(
        kept[0] && kept[1] && kept[2],
        "words outside the reach are untouched: {kept:?}"
    );
}

// ---- S5: marks, edges, whole takes -------------------------------------

#[test]
fn f1_10_s5_dropped_runs_become_marks_whose_edges_come_from_the_sound() {
    let times: Vec<(f64, f64)> = (0..8).map(|i| (i as f64, i as f64 + 1.0)).collect();
    let mut kept = vec![true; 8];
    kept[2] = false;
    kept[3] = false;
    // No aligned words and no envelope: the marks sit on the word times, which is what a run with no
    // audio to ask has. F1.11 owns any move; this only calls it. With words but no envelope the next
    // case shows the word pad alone fences the edge, and with both the envelope chooses inside it.
    let (marks, notes) = joins::marks_for(&times, &kept, &[], |_| None, &[]);
    assert_eq!(marks.len(), 1, "one dropped run is one mark");
    assert_eq!(marks[0].s, 2.0, "the run's own start");
    assert_eq!(marks[0].e, 4.0, "the run's own end");
    assert!(
        notes.is_empty(),
        "nothing moved with no sound to ask: {notes:?}"
    );
}

#[test]
fn f1_10_s5_a_dropped_run_reaches_the_sound_for_its_edges() {
    // The same dropped run, now with the session's aligned words in hand: word 2 (index 1) ends at
    // 2.0 s and the mark's raw edge sits at exactly there, so F1.11 fences it and says so. The point
    // this pins is that the JOIN pass hands those words over at all — before, it passed `&[]` and the
    // marks were never fenced, so §F1.11 never ran for a join.
    // Word 2 (index 1) ENDS at 2.0 s and the mark's raw edge sits at exactly there: the fence has no
    // width, so `place_edges` skips the note and the edge lands on the aligner's time. The join model
    // is answered in words, not stamps, so a dropped run begins at its own first word and this is the
    // normal shape (retakes, whose stamps come off the sound, are the case that moves — and when the
    // stamp does run late, the pad moves it and the note says so; second half below).
    let times: Vec<(f64, f64)> = (0..8).map(|i| (i as f64, i as f64 + 1.0)).collect();
    let words: Vec<AlignedWord> = times
        .iter()
        .enumerate()
        .map(|(i, (s, e))| AlignedWord {
            word: format!("w{i}"),
            s: *s,
            e: *e,
        })
        .collect();
    let mut kept = vec![true; 8];
    kept[2] = false;
    kept[3] = false;
    let (marks, notes) = joins::marks_for(&times, &kept, &words, |_| None, &times);
    assert_eq!(marks.len(), 1);
    assert!(
        marks[0].s >= 2.0 - 1e-9 && marks[0].s <= 2.0 + 1e-9,
        "the edge lands on the last word that survives, not past it: {}",
        marks[0].s
    );
    assert!(
        notes.is_empty(),
        "a fence with no width has nothing to report: {notes:?}"
    );
    // And a stamp that runs late does move: the aligner says the dropped word began at 2.0 s but the
    // word before it ends at 1.9 s, so the cut ends at the pad and the note names the seconds.
    let mut late: Vec<AlignedWord> = words.clone();
    late[1].e = 1.9;
    let (moved, moved_notes) = joins::marks_for(&times, &kept, &late, |_| None, &times);
    assert!(
        moved[0].s > 1.9 && moved[0].s <= 2.0,
        "the word pad lands after the last word that stays: {}",
        moved[0].s
    );
    assert!(
        moved_notes.iter().any(|n| n.contains("earlier")),
        "the move is spoken: {moved_notes:?}"
    );
}

#[test]
fn f1_11_s1_a_session_with_a_measured_envelope_moves_the_join_mark() {
    // The other half of the case above: the recordings ARE on disk and their lane has a cache, so
    // `session_lanes` hands back a lane, `edges::finder` answers an envelope, and F1.11's sound rules run
    // for a JOIN mark -- which is what `join_ask::run_seams` now does on every press.
    let tree = temp_tree("lanes-hit");
    let words: Vec<Word> = two_takes()
        .into_iter()
        .map(|mut word| {
            // The spelling a session file carries, because `session_lanes` stats exactly this string
            // (`stat_recording` resolves `project:` inside the project folder).
            word.source = format!("project:sources/{}.mkv", word.source);
            word
        })
        .collect();
    // The cache key is the base name without its extension (`lane_key`), which is how every per-recording
    // folder is keyed -- NOT the stored path, which still carries `project:sources/`.
    const LANE_A_SUFFIX: &str = "take-a";
    // Two real files: `session_lanes` stats each one, so they must exist to be lanes at all.
    let mut stamps: Vec<(String, i64, i64)> = Vec::new();
    for base in ["take-a.mkv", "take-b.mkv"] {
        // `stat_recording` resolves a `project:`-prefixed name inside the project folder, which is how a
        // session file spells its recordings -- so that is the spelling the words carry below.
        let path = tree.dir().join(format!("sources/{base}"));
        std::fs::create_dir_all(path.parent().expect("a sources folder")).expect("the folder");
        std::fs::write(&path, b"recording-bytes").expect("the recording");
        let meta = std::fs::metadata(&path).expect("stamped");
        let mtime = meta
            .modified()
            .expect("a time")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs() as i64;
        stamps.push((base.to_string(), meta.len() as i64, mtime));
    }
    // take-a's envelope: room 8 everywhere, its speech from 0.5-1.9 s at 120 bytes, and a tail at
    // 1.90-1.96 of 90 bytes -- still the word (the tail threshold is peak less 44 bytes), and quiet after.
    // The dropped run starts at 2.0 s but the aligner says the word before it ended at 1.9 s (a late
    // stamp), so the fence runs 1.9-2.0 and the sound chooses inside it: the cut ends where take-a's own
    // sound stops, not on the aligner's second.
    let hz = 200.0;
    let mut peaks = vec![8u8; (4.0 * hz) as usize];
    for slot in &mut peaks[(0.5 * hz) as usize..(1.9 * hz) as usize] {
        *slot = 120;
    }
    for slot in &mut peaks[(1.9 * hz) as usize..(1.96 * hz) as usize] {
        *slot = 90;
    }
    // The cache key is what `lane_key` produced from the stored path: read back off the lane rather than
    // guessed at here, so the test cannot drift from the program's own key.
    let held_lanes = joins::session_lanes(&tree, &words);
    let lane_a = held_lanes
        .iter()
        .find(|lane| lane.key.ends_with(LANE_A_SUFFIX))
        .expect("take-a is a lane")
        .key
        .clone();
    let (_, size_a, mtime_a) = &stamps[0];
    naivepost::wave::write(
        &tree,
        &lane_a,
        &naivepost::wave::Wave {
            hz,
            chans: vec![peaks],
        },
        *size_a,
        *mtime_a,
    )
    .expect("the cache is written");

    let held = naivepost::edges::load(&tree, &held_lanes);
    assert_eq!(
        held_lanes.len(),
        2,
        "both recordings are on disk, so both are lanes (a missing stat silently drops a lane)"
    );
    assert!(
        !held.is_empty(),
        "and at least one of them has a cache to read: {held_lanes:?}"
    );

    let times: Vec<(f64, f64)> = words.iter().map(|word| (word.start, word.end)).collect();
    let aligned: Vec<AlignedWord> = words
        .iter()
        .map(|word| AlignedWord {
            word: word.match_word.clone(),
            s: word.start,
            e: word.end,
        })
        .collect();
    let mut kept = vec![true; words.len()];
    kept[2] = false; // "three", 2.0-3.0: the join takes it off the end of take-a
    let mut late = aligned.clone();
    late[1].e = 1.9; // the aligner heard "two" ending before its nominal second

    // The exact expression `join_ask::run_seams` uses.
    let (marks, notes) = joins::marks_for(
        &times,
        &kept,
        &late,
        naivepost::edges::finder(&held),
        &times,
    );
    assert_eq!(marks.len(), 1);
    assert!(
        marks[0].s > 1.9 && marks[0].s < 2.0,
        "the edge came off the word time onto the envelope: {}",
        marks[0].s
    );
    assert_eq!(
        marks[0].s, 1.96,
        "followed to where take-a's own sound drops"
    );
    assert!(
        notes.iter().any(|n| n.contains("earlier")),
        "the move is spoken with its seconds: {notes:?}"
    );

    // And without the cache the same mark sits on the pad instead -- the branch the no-lane case above
    // pins, shown here over identical words so the difference is the envelope alone.
    let (bare, bare_notes) = joins::marks_for(&times, &kept, &late, |_| None, &times);
    assert!(
        !bare_notes.is_empty(),
        "the word-fence note still goes out: {bare_notes:?}"
    );
    assert_ne!(
        bare[0].s, marks[0].s,
        "with vs without an envelope are different answers"
    );
}

#[test]
fn f1_10_s5_a_lane_with_no_wave_cache_answers_no_envelope() {
    // `session_lanes` keys a lane by the recording's base name and stamps it with the real file. A word
    // list naming recordings that are not on disk therefore yields NO lane, and `edges::finder` over an
    // empty set answers `None` everywhere — which is how "no audio extracted yet" reaches F1.11: the
    // marks stand on their word times rather than on a guessed envelope.
    let tree = temp_tree("lanes");
    let words = two_takes();
    assert!(
        joins::session_lanes(&tree, &words).is_empty(),
        "a recording that cannot be stat-ed has no envelope to offer"
    );
    let held = naivepost::edges::load(&tree, &joins::session_lanes(&tree, &words));
    assert!(held.is_empty());
    assert!(
        naivepost::edges::finder(&held)(1.0).is_none(),
        "so every second answers None"
    );
}

#[test]
fn f1_10_s5_a_whole_take_goes_is_flagged_not_refused() {
    let words = two_takes();
    let all_gone = vec![true; words.len()];
    let flagged = joins::whole_takes(&words, &all_gone);
    assert_eq!(flagged.len(), 2, "both recordings went whole");
    assert_eq!(flagged[0].0, "take-a");
    assert_eq!(flagged[0].1, 0.0);
    assert_eq!(flagged[0].2, 4.0);
    assert_eq!(
        joins::whole_take_log("take-a", 0.0, 4.0),
        ">>> text edit: all of take-a goes (0:00-0:04) -- marked yellow on Cut, worth a look"
    );
    // One word kept and the recording is NOT flagged: a join that took almost everything is not a
    // join that took all of it, and the yellow wash would lie.
    let mostly = vec![true, true, true, false, true, true, true, true];
    assert!(
        joins::whole_takes(&words, &mostly)
            .iter()
            .all(|(base, _, _)| base != "take-a"),
        "take-a kept a word and is not flagged"
    );
    // Nothing dropped flags nothing.
    let none = vec![false; words.len()];
    assert!(joins::whole_takes(&words, &none).is_empty());
}

// ---- S6: the two files, written together -------------------------------

fn temp_tree(tag: &str) -> Tree {
    // A project IS a folder ending in `.naivepost` (spec/00-principles), so the temp root carries
    // the suffix rather than being a bare directory `Tree::new` would refuse.
    let root = std::env::temp_dir().join(format!(
        "naivepost-f110-{tag}-{}.naivepost",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    Tree::new(&root).expect("a .naivepost folder is a project")
}

#[test]
fn f1_10_s6_final_txt_marks_only_where_the_recording_changes() {
    let tree = temp_tree("markers");
    let words = split("one two three four five six seven eight");
    let sources: Vec<String> = vec![
        "take-a", "take-a", "take-a", "take-a", "take-b", "take-b", "take-b", "take-b",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Drop the last two of take-a (the false start) and the first of take-b: all of it AT the join.
    let mut kept = vec![true; 8];
    kept[2] = false;
    kept[3] = false;
    kept[4] = false;
    let marks: Vec<Retake> = hand_edit::marks_from(
        &(0..8)
            .map(|i| (i as f64, i as f64 + 1.0))
            .collect::<Vec<_>>(),
        &kept,
    );
    joins::write_pair(&tree, &sources, &words, &kept, &[], &marks).expect("both files written");
    let text = std::fs::read_to_string(tree.final_txt()).expect("final.txt exists");
    // The count is what went BETWEEN the survivors on either side of the seam: "three four" before it
    // and nothing after ("five" sits past the seam, so it is not between them).
    assert_eq!(
        text, "one two |cut 2| six seven eight",
        "one marker, at the change of recording, counting the words gone there"
    );
    // Between survivors of the SAME recording there is no marker: nothing was spliced there.
    assert!(
        !text.contains("six |cut"),
        "no marker inside a recording: {text}"
    );
    // The words read back are the survivors, markers gone.
    let (read_words, read_joins) = textfmt::read_final(&text);
    assert_eq!(
        strings(&read_words),
        vec!["one", "two", "six", "seven", "eight"],
        "round-trips through the reader"
    );
    assert_eq!(read_joins, vec![2], "the count survives the round trip");
    // Both files landed in the same step.
    assert!(
        tree.retakes_tsv().exists(),
        "retakes.tsv written alongside final.txt"
    );
}

#[test]
fn f1_10_s6_a_whole_take_transition_writes_the_longer_marker() {
    let tree = temp_tree("whole");
    let words = split("keep one gone-a gone-b tail word");
    let sources: Vec<String> = vec!["a", "a", "b", "b", "c", "c"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Recording "b" loses both its words: the join into it took the whole take.
    let kept = vec![true, true, false, false, true, true];
    joins::write_pair(&tree, &sources, &words, &kept, &["b".to_string()], &[]).expect("written");
    let text = std::fs::read_to_string(tree.final_txt()).expect("final.txt");
    // `write_final` glues its marker with no space of its own, so the shape is exactly as written.
    assert!(
        text.contains("|cut|whole take|"),
        "the whole-take spelling is there: {text}"
    );
    let (read_words, _) = textfmt::read_final(&text);
    assert_eq!(
        strings(&read_words),
        vec!["keep", "one", "tail", "word"],
        "the reader takes no words out of the marker"
    );
}

#[test]
fn f1_10_s6_no_survivors_still_writes_both_files() {
    // A pass that cut every word it had still owes both files: the marks say where the words went.
    let tree = temp_tree("empty");
    let words = vec!["gone".to_string()];
    let mark = naivepost::textfmt::Retake {
        s: 0.0,
        e: 1.0,
        again: 1.0,
        to: 1.0,
        text: "gone".into(),
        whole: String::new(),
    };
    joins::write_pair(
        &tree,
        &["a".to_string()],
        &words,
        &[false],
        &[],
        std::slice::from_ref(&mark),
    )
    .expect("written");
    assert!(tree.final_txt().exists());
    assert!(tree.retakes_tsv().exists());
    assert_eq!(std::fs::read_to_string(tree.final_txt()).unwrap(), "");
    // Nothing heard at all is no answer to give: neither file, and none left over from before (§3).
    joins::write_pair(&tree, &[], &[], &[], &[], &[]).expect("nothing to write");
    assert!(!tree.final_txt().exists() && !tree.retakes_tsv().exists());
}

#[test]
fn f1_10_s6_the_completion_line_counts_words_and_stretches() {
    assert_eq!(
        joins::written_log(16, 40, 1),
        ">>> text edit: 16 of 40 words removed in 1 stretch(es)"
    );
    assert_eq!(
        joins::written_log(0, 40, 0),
        ">>> text edit: 0 of 40 words removed in 0 stretch(es)"
    );
}

// ---- the gate ----------------------------------------------------------

#[test]
fn f1_10_the_pass_is_gated_on_the_policy_naming_joins() {
    use naivepost::project::MarkingPass;
    let tree = temp_tree("gate");
    // Not the joins pass: contributes nothing at all, so one press never speaks for F1.9 too.
    let project = naivepost::project::Project::default();
    assert!(
        joins::press_joins(&tree, &project, MarkingPass::Retakes, &mut |_, _| {}).is_empty(),
        "under the retakes pass the joins flow says nothing"
    );
    assert!(
        joins::press_joins(&tree, &project, MarkingPass::None, &mut |_, _| {}).is_empty(),
        "under no pass it says nothing either"
    );
    // The joins pass on a session with no words at all: S1 answers, and no 0-byte file is left behind (§3).
    let lines = joins::press_joins(&tree, &project, MarkingPass::Joins, &mut |_, _| {});
    assert_eq!(
        lines,
        vec![">>> text edit: nothing to repair -- 0 joinable words, fewer than 4"],
        "S1 settles with no server"
    );
    assert!(!tree.final_txt().exists() && !tree.retakes_tsv().exists());
}

// ---- S3 over the wire: the tool loop this job is offered (F1.10 §3.2) ------
//
// The rules above hand answers in as text, which is why the ask's SHAPE was never checked here. These
// cases share the live scenarios' harness (`tests/suite/join_live_harness.rs`) so what is asserted is the
// bytes that reached a socket. That harness moves the process-wide env and `settings::save` replaces
// the whole file, so it is its OWN binaries' property — the same reason those scenarios are one file
// each. What is checked here is everything that does not need the env: the offer a job gets, and what
// a tool reply comes to.

/// §3.5: the text edit offers NO tools and thinks. Offered drop_words/keep_join beside a prompt that asked for
/// text, the 27B answered nothing usable at 125 of 127 asks on 07-news; the quoted answer needs no tool.
#[test]
fn f1_10_s3_the_ask_offers_no_tools_and_thinks() {
    assert!(
        naivepost::tools::offered(naivepost::roles::Job::Joins).is_empty(),
        "no tools for the text edit"
    );
    let body =
        naivepost::join_ask::body_for_test(&["one two three".to_string(), "four five".to_string()]);
    let value: serde_json::Value = serde_json::from_str(&body).expect("the ask body is JSON");
    assert!(
        value.get("tools").is_none(),
        "no tools field went out: {body}"
    );
    assert_eq!(
        value.get("enable_thinking").and_then(|v| v.as_bool()),
        Some(true),
        "§F1.10 says thinking ON per seam"
    );
}

/// A quote longer than what was shown cannot be how BEFORE ends, and is refused rather than read as "all of it".
#[test]
fn f1_10_s3_a_quote_past_what_was_shown_is_refused() {
    const TAKE_A: [&str; 5] = ["the", "network", "is", "live", "today"];
    const TAKE_B: [&str; 5] = ["the", "network", "goes", "live", "now"];
    let before: Vec<String> = TAKE_A.iter().map(|w| w.to_string()).collect();
    let after: Vec<String> = TAKE_B.iter().map(|w| w.to_string()).collect();
    let mut join = Join::new(TAKE_A.as_slice(), TAKE_B.as_slice());
    let reply = r#"{"drop_before":"and so the network is live today","drop_after":""}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    assert!(
        matches!(&outcome, joins::Outcome::Refused(why) if why.contains("is not how BEFORE ends")),
        "{outcome:?}"
    );
    assert_eq!(
        join.dropped_count(Side::Before),
        0,
        "a refused quote takes nothing"
    );
}

/// The seam's own arithmetic belongs to `Join`, not to a copy of its rules: what one word off the
/// BEFORE side is worth is the tool's answer, quoted unchanged (empty means "legal", any text is the
/// reason the rule gave).
#[test]
fn f1_10_s3_a_seam_the_rules_answer_themselves() {
    // "the" repeated straight across the cut: dropping one word from BEFORE is a legal repair, and the
    // tool says so in its own answer rather than through a copy of its rule written here.
    let mut join = Join::new(
        &["the", "network", "is", "live", "today"],
        &["the", "network", "goes", "live", "now"],
    );
    let repeat = join.drop_words(Side::Before, 1);
    assert!(
        repeat.contains("across_the_join"),
        "a one-word drop at this seam is legal in the tool's own answer: {repeat}"
    );
    // A join with nothing at all to drop is the one the rules settle themselves: `keep_join` answers it
    // without a model, and the answer says so.
    let mut join = Join::new(&["a", "b"], &["c", "d"]);
    assert!(
        join.keep_join().contains("kept"),
        "keep_join is a whole answer"
    );
    assert!(join.kept());
}
