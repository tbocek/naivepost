//! F1.10 Repair the joins — the flow rules, with no window and no model server.
//!
//! Every fixture here hands the answers in as text, which is what makes the checks possible: the
//! requests themselves need a live textedit server, and nothing in this file asks for one. The
//! worked examples are the spec's own (ETH lecture, 57 joins), so an assertion that fails against
//! them is a rule misread rather than a fixture mistake.
//!
//! Step tests: `f1_10_s1_*` (the words a join pass may use) through `f1_10_s6_*` (both files
//! written together). The wire — a real ▶ click reaching the pass — is `tests/suite/joins_widgets.rs`.

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
    let ask = joins::request("", 23, 57, &before, &after);
    assert!(
        ask.contains("JOIN 23 of 57."),
        "the join is numbered: {ask}"
    );
    assert!(
        ask.contains("BEFORE (the end of the take that was interrupted):"),
        "BEFORE is labelled as the spec labels it: {ask}"
    );
    assert!(
        ask.contains("AFTER (the beginning of the take that follows):"),
        "AFTER is labelled as the spec labels it: {ask}"
    );
    assert!(
        ask.contains(r#"Answer {"joined":"..."} and nothing else"#),
        "the shape of the answer is stated: {ask}"
    );
    assert!(
        ask.contains("these 4 words and then these 4"),
        "counts given: {ask}"
    );
    // An empty User Context adds NO header over nothing.
    assert!(
        !ask.contains("USER CONTEXT"),
        "no context means no context block: {ask}"
    );
}

#[test]
fn f1_10_s3_a_written_context_outranks_the_job_rules_in_the_same_message() {
    let before = vec!["alpha".to_string()];
    let after = vec!["beta".to_string()];
    let ask = joins::request("Cut nothing before the demo.", 1, 1, &before, &after);
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

// ---- S3: matching an answer back ----------------------------------------

/// The ETH join-23 shape: BEFORE ends mid-sentence, AFTER restarts it, and the answer runs the two
/// together by leaving the tail of BEFORE out.
fn eth_join23() -> (Vec<String>, Vec<String>) {
    // BEFORE's shown tail is the abandoned attempt; AFTER starts with what was said again. The answer
    // keeps BEFORE up to "ermöglichen" and all of AFTER, so the removal is the tail of BEFORE plus
    // nothing from AFTER -- one stretch at the join.
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

#[test]
fn f1_10_s3_an_answer_that_runs_the_two_parts_on_is_applied() {
    let (before, after) = eth_join23();
    let mut join = Join::new(&strings(&before), &strings(&after));
    // The reply keeps BEFORE up to "ermöglichen" and all of AFTER: what went is the abandoned tail.
    // The spec's own reply for join 23: BEFORE run on to where it repeats, then AFTER whole.
    // Kept: BEFORE through "Gas-Limit" (the whole take, nothing dropped from its middle), then all of
    // AFTER. The removal is the attempt that was said twice -- BEFORE's tail after "ermöglichen" --
    // which sits right at the seam, so it is one stretch and not two.
    let reply = r#"{"joined":"und Block-Level level Access Lists die parallele Ausführung ermöglichen und den Weg zu einer Gas-Limit Und die Native Account Abstraction für das übernächste version also"}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    assert_eq!(
        outcome,
        Outcome_applied(7, 0),
        "the removal is charged to BEFORE's tail, where the stumble is: {outcome:?}"
    );
    assert_eq!(outcome.removed(), 7, "the seven-word false start goes");
    assert!(outcome.usable(), "a used answer may be cached");
}

/// Built so the enum's field names stay visible in the assertion without repeating the type path.
fn Outcome_applied(before: usize, after: usize) -> joins::Outcome {
    joins::Outcome::Applied { before, after }
}

#[test]
fn f1_10_s3_repeating_everything_shown_is_keep_join_not_a_refusal() {
    let before = split("alpha beta gamma");
    let after = split("delta epsilon");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let reply = r#"{"joined":"alpha beta gamma delta epsilon"}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    assert_eq!(
        outcome,
        joins::Outcome::Kept,
        "nothing left out is the ordinary answer"
    );
    assert_eq!(outcome.removed(), 0, "a kept join removes nothing");
    assert!(outcome.usable(), "keep_join is a usable answer and caches");
}

#[test]
fn f1_10_s3_two_stretches_left_out_is_refused() {
    // The join-22 / join-52 case: the answer drops something near the join AND something further off,
    // which is two cuts and not one splice.
    let before = split("bei Fusaka Das ist der aktuellste Release und wurde im Dezember aktiviert");
    let after = split("und wurde im Dezember aktiviert und führte dazu dass die Kosten");
    let mut join = Join::new(&strings(&before), &strings(&after));
    // Keeps a piece of BEFORE's head, skips a middle stretch, then rejoins: two separate removals.
    let reply =
        r#"{"joined":"bei Fusaka Das ist der aktuellste Release und führte dazu dass die Kosten"}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    match &outcome {
        joins::Outcome::Refused(reason) => assert!(
            reason.contains("stretches") || reason.contains("short of the join"),
            "the refusal says why: {reason}"
        ),
        other => panic!("two removals must not be applied: {other:?}"),
    }
    assert!(!outcome.usable(), "a refusal is never cached");
}

#[test]
fn f1_10_s3_more_than_seamMaxWords_is_refused_with_the_tools_own_sentence() {
    // P.machine.seamMaxWords = 40.
    let big_before: Vec<String> = (0..60).map(|n| format!("w{n}")).collect();
    let after = split("tail word");
    let mut join = Join::new(&strings(&big_before), &strings(&after));
    // Keep only the last two words of BEFORE: 58 dropped, well past the bound.
    let mut kept_words = vec![big_before[58].clone(), big_before[59].clone()];
    kept_words.extend(after.iter().cloned());
    let reply = format!(r#"{{"joined":"{}"}}"#, kept_words.join(" "));
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
    // P.machine.seamCeil = 0.6 of what was SHOWN. Six shown, five wanted gone is 83 %.
    let before = split("aa bb cc dd ee ff");
    let after = split("gg hh");
    let mut join = Join::new(&strings(&before), &strings(&after));
    // Keep one word of BEFORE plus all of AFTER: five of eight shown go.
    let reply = r#"{"joined":"ff gg hh"}"#;
    let outcome = joins::answer_join(&mut join, &reply, &before, &after);
    match &outcome {
        joins::Outcome::Refused(reason) => assert!(
            reason.contains('%') || reason.contains("two takes"),
            "the ceiling refusal explains itself: {reason}"
        ),
        other => panic!("five of eight shown must be refused: {other:?}"),
    }
}

#[test]
fn f1_10_s3_a_stretch_past_the_snap_bound_is_refused_by_name() {
    // P.machine.seamSnapWords = 3: within three words of the seam is still the seam; past it the
    // stretch belongs to somebody else's sentence.
    let before = split("one two three four five six seven eight nine ten");
    let after = split("eleven twelve");
    let _join = Join::new(&strings(&before), &strings(&after));
    // Keep everything but the LAST two of BEFORE: that touches the seam and snaps fine.
    let ok_reply = r#"{"joined":"one two three four five six seven eight nine ten eleven twelve"}"#;
    let untouched = joins::answer_join(
        &mut Join::new(&strings(&before), &strings(&after)),
        ok_reply,
        &before,
        &after,
    );
    assert_eq!(
        untouched,
        joins::Outcome::Kept,
        "repeating all is keep_join"
    );

    // The refusing shape: drop "seven eight nine ten"? No -- to STOP SHORT of the seam the kept head
    // must resume before the end, so drop words from the middle and keep the last ones: leaving out
    // "four five six" while keeping "seven eight nine ten" puts the gap four words off the seam, past
    // the three-word snap, and must be refused.
    let far_reply = r#"{"joined":"one two three seven eight nine ten eleven twelve"}"#;
    let mut join = Join::new(&strings(&before), &strings(&after));
    let outcome = joins::answer_join(&mut join, far_reply, &before, &after);
    match &outcome {
        joins::Outcome::Refused(reason) => assert!(
            !reason.is_empty(),
            "a stretch that misses the seam is refused with a reason"
        ),
        other => panic!("a removal stopping short of the seam must not apply: {other:?}"),
    }
}

#[test]
fn f1_10_s3_a_reply_that_is_not_the_answer_shape_is_refused() {
    let before = split("alpha beta");
    let after = split("gamma delta");
    let mut join = Join::new(&strings(&before), &strings(&after));
    let not_json = joins::answer_join(&mut join, "I think the join reads fine", &before, &after);
    assert!(
        matches!(&not_json, joins::Outcome::Refused(r) if r.contains("joined")),
        "no JSON means no joined field: {not_json:?}"
    );
    let mut join = Join::new(&strings(&before), &strings(&after));
    let empty_object = joins::answer_join(&mut join, "{}", &before, &after);
    assert!(
        matches!(&empty_object, joins::Outcome::Refused(r) if r.contains("joined")),
        "an object without the field is refused the same way: {empty_object:?}"
    );
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
    // A pass that found nothing still owes both files: their presence is what tells Cut the question
    // was asked, which is the same reason F1.9 writes an empty retakes.tsv.
    let tree = temp_tree("empty");
    joins::write_pair(&tree, &[], &[], &[], &[], &[]).expect("written");
    assert!(tree.final_txt().exists());
    assert!(tree.retakes_tsv().exists());
    assert_eq!(std::fs::read_to_string(tree.final_txt()).unwrap(), "");
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
        joins::press_joins(&tree, &project, MarkingPass::Retakes).is_empty(),
        "under the retakes pass the joins flow says nothing"
    );
    assert!(
        joins::press_joins(&tree, &project, MarkingPass::None).is_empty(),
        "under no pass it says nothing either"
    );
    // The joins pass on a session with no words at all: S1 answers and both files are still written.
    let lines = joins::press_joins(&tree, &project, MarkingPass::Joins);
    assert_eq!(
        lines,
        vec![">>> text edit: nothing to repair -- 0 joinable words, fewer than 4"],
        "S1 settles with no server"
    );
    assert!(tree.final_txt().exists() && tree.retakes_tsv().exists());
}

// ---- S3 over the wire: the tool loop this job is offered (F1.10 §3.2) ------
//
// The rules above hand answers in as text, which is why the ask's SHAPE was never checked here. These
// cases share the live scenarios' harness (`tests/suite/join_live_harness.rs`) so what is asserted is the
// bytes that reached a socket. That harness moves the process-wide env and `settings::save` replaces
// the whole file, so it is its OWN binaries' property — the same reason those scenarios are one file
// each. What is checked here is everything that does not need the env: the offer a job gets, and what
// a tool reply comes to.

/// The offer this job must send: its four tools by name. A job that is offered nothing can only answer
/// in prose, which is the gap these cases exist to close (§3.2).
#[test]
fn f1_10_s3_the_ask_offers_the_textedit_tools() {
    let offered = naivepost::tools::offered(naivepost::roles::Job::Joins);
    let names: Vec<&str> = offered.iter().map(|tool| tool.name()).collect();
    for tool in ["drop_words", "keep_join", "get_words", "finish"] {
        assert!(
            names.contains(&tool),
            "{tool} is offered to this job: {names:?}"
        );
    }
    // And that offer reaches the wire: `join_ask` builds its body from THIS list, through the same
    // `llm_request::body` every chat ask rides, so a body whose `tools` array names them is the proof
    // the field is sent rather than dropped. Checked on the body the builder produces.
    let body =
        naivepost::join_ask::body_for_test(&["one two three".to_string(), "four five".to_string()]);
    let value: serde_json::Value = serde_json::from_str(&body).expect("the ask body is JSON");
    let tools = value
        .get("tools")
        .and_then(|v| v.as_array())
        .expect("a tools array went out");
    let sent: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.pointer("/function/name").and_then(|n| n.as_str()))
        .collect();
    for tool in ["drop_words", "keep_join", "get_words", "finish"] {
        assert!(sent.contains(&tool), "{tool} rode out in the ask: {sent:?}");
    }
    assert_eq!(
        value.get("enable_thinking").and_then(|v| v.as_bool()),
        Some(true),
        "offering tools did not turn thinking off (§F1.10 says thinking ON per seam)"
    );
}

/// The anti-duplication pin: a `drop_words(side,count)` call and the `{"joined":"…"}` text that means
/// the same thing must come to the same `Outcome`. If they ever disagree, one of the two paths has
/// grown its own copy of a rule `Join` already owns.
#[test]
fn f1_10_s3_a_drop_words_answer_applies_the_same_counts() {
    // The two takes the live scenarios use, spelled out here so this file keeps needing no env: five
    // words each, take-a ending "the network is live today" and take-b saying it again.
    const TAKE_A: [&str; 5] = ["the", "network", "is", "live", "today"];
    const TAKE_B: [&str; 5] = ["the", "network", "goes", "live", "now"];
    let before: Vec<String> = TAKE_A.iter().map(|w| w.to_string()).collect();
    let after: Vec<String> = TAKE_B.iter().map(|w| w.to_string()).collect();
    // The tool way: 3 words off BEFORE, which is take-a's tail "is live today". `call_for_test` is the
    // same door the live loop runs every call through, so this is the real dispatch, not a copy.
    let mut join = Join::new(TAKE_A.as_slice(), TAKE_B.as_slice());
    let mut refusals: Vec<String> = Vec::new();
    for name in ["drop_words", "finish"] {
        let args = if name == "drop_words" {
            serde_json::json!({ "side": "before", "count": 3 })
        } else {
            serde_json::json!({})
        };
        naivepost::join_ask::call_for_test(
            &naivepost::exchanges::ToolCall {
                name: name.to_string(),
                args: args.to_string(),
                result: String::new(),
            },
            &mut join,
            &mut refusals,
        );
    }
    let via_tool = naivepost::join_ask::outcome_for_test(&join);
    assert_eq!(
        via_tool,
        joins::Outcome::Applied {
            before: 3,
            after: 0
        },
        "the tool's own counts: {refusals:?}"
    );
    assert!(
        refusals.is_empty(),
        "three off a five-word side is legal: {refusals:?}"
    );
    // The prose way, over the same words: the answer that keeps take-a's first two words and all of
    // take-b, which is the same repair said in the prototype's shape.
    let kept = format!("{} {}", TAKE_A[..2].join(" "), TAKE_B.join(" "));
    let mut prose = Join::new(TAKE_A.as_slice(), TAKE_B.as_slice());
    let via_prose = joins::answer_join(
        &mut prose,
        &format!("{{\"joined\":\"{kept}\"}}"),
        &before,
        &after,
    );
    assert_eq!(
        via_prose, via_tool,
        "one rule, two shapes: the tool answer and the prose answer must agree"
    );
}

/// A count the TOOL refuses (past what was shown) comes back as its own `{"error": …}` sentence and
/// the join is refused with that same reason — not with a second sentence invented here, and not with
/// the seam quietly repaired.
#[test]
fn f1_10_s3_a_tool_refusal_reaches_the_model_and_the_join_is_still_decided_by_the_rules() {
    const TAKE_A: [&str; 5] = ["the", "network", "is", "live", "today"];
    const TAKE_B: [&str; 5] = ["the", "network", "goes", "live", "now"];
    // Five words shown on the BEFORE side; asking for nine is the tool's "only 5 were shown".
    let mut join = Join::new(TAKE_A.as_slice(), TAKE_B.as_slice());
    let mut refusals: Vec<String> = Vec::new();
    naivepost::join_ask::call_for_test(
        &naivepost::exchanges::ToolCall {
            name: "drop_words".to_string(),
            args: serde_json::json!({ "side": "before", "count": 9 }).to_string(),
            result: String::new(),
        },
        &mut join,
        &mut refusals,
    );
    assert_eq!(
        join.dropped_count(Side::Before),
        0,
        "a refused drop takes nothing"
    );
    assert_eq!(
        refusals.len(),
        1,
        "the refusal is kept for the model AND for the log"
    );
    let reason = refusals[0].clone();
    assert!(
        reason.contains("\"error\""),
        "it is the tool's own error shape: {reason}"
    );
    assert!(
        reason.contains("only 5 were shown"),
        "the tool's sentence, unchanged: {reason}"
    );
    // And the JOIN's answer when that refusal is all the model ever offered: the tool's own reason,
    // said as a refusal of the answer — "a refusal still keeps the stumble", so nothing goes and the
    // reason on the log is the rule's sentence, not one invented here.
    let outcome = naivepost::join_ask::tool_refusal_outcome(&refusals);
    assert!(
        matches!(&outcome, joins::Outcome::Refused(why) if why.contains("only 5 were shown")),
        "the refusal reaches the join unchanged: {outcome:?}"
    );
    assert!(!outcome.usable(), "a refused answer is never cached");
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
