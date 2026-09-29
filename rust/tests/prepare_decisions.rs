//! §12-decisions#prepare — where each of Prepare's ten behind-the-model behaviours lives now.
//!
//! Every assertion here takes its evidence from the live code path that makes the decision, not from
//! the audit prose: a real `mark_abandoned` reply, a real `place_edges` call, a real `drop_words`
//! answer. The two pieces this round added (`prepare_decisions::suffix_match` and
//! `prepare_decisions::classify`) are checked directly; the other eight rows point at existing
//! machinery, which is asserted to still do what its row claims.
//!
//! Cited ids and tools: `tool:mark_abandoned`, `tool:drop_words`, `tool:fix_line`, `tool:flag_line`,
//! `tool:record_event`, `tool:speech_around`; `P.machine.retakeCeil`, `P.machine.seamSnapWords`,
//! `P.machine.seamNoiseWords`; rule 2.2 (models propose, the machine places); `09 F6.1` (the tool
//! protocol).

use naivepost::edges::{self as edge, AlignedWord};
use naivepost::prepare_decisions as pd;
use naivepost::textfmt::Retake;
use naivepost::tool_loop;
use naivepost::tools::describe::{self, Batch, MissingReason, Placement, Spoken};
use naivepost::tools::fix::{self, Block, Line as FixLine};
use naivepost::tools::retakes::{Line, Marks, Trim};
use naivepost::tools::textedit::{self, Join, Side};
use naivepost::exchanges::ToolCall;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{word};

const ITEM: &str = "§12-decisions#prepare";

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).expect("a tool always answers JSON")
}

fn error(reply: &str) -> String {
    json(reply)["error"]
        .as_str()
        .expect("a refusal is an error, never a silent ok")
        .to_string()
}

/// No envelope anywhere on the clock (same helper shape as tests/edge_placement.rs).
fn no_envelope(_: f64) -> Option<&'static edge::Edges> {
    None
}

#[test]
fn sec_12_decisions_prepare_s1_the_mark_is_trimmed_to_a_fuzzy_matched_word_suffix() {
    assert_eq!(ITEM, "§12-decisions#prepare");
    // tool:mark_abandoned -- the model marks whole lines; the app cuts inside a line at its own
    // boundary, so what comes back is the stretch that will really go rather than the range sent.
    let tail = ["so", "the", "tower", "fires", "again"];

    // Four of five turning up again clears the 70% bar; the anchor is the earliest hit, not the best.
    let later = ["and", "then", "so", "the", "tower", "fires", "again"];
    let found = pd::suffix_match(&tail, &later).expect("a repeat starting two words in is found");
    assert_eq!(found.starts_at, 2, "earliest anchor");
    assert!(found.share >= pd::TAIL_MATCH_MIN, "{}", found.share);

    // Stepping over three words is allowed...
    let skip3 = ["so", "x", "y", "z", "the", "tower", "fires", "again"];
    let m = pd::suffix_match(&tail, &skip3).expect("three skipped words still match");
    assert_eq!(m.skipped, 3, "{m:?}");
    // ...four is not.
    let skip4 = ["so", "x", "y", "z", "w", "the", "tower", "fires", "again"];
    assert!(pd::suffix_match(&tail, &skip4).is_none(), "TAIL_SKIP_MAX = 3");

    // One imperfectly-heard word is forgiven; two are not.
    let edit1 = ["so", "the", "towr", "fires", "again"];
    let m = pd::suffix_match(&tail, &edit1).expect("one edit allowed");
    assert_eq!(m.edits, 1, "{m:?}");
    let edit2 = ["so", "the", "towrr", "fyres", "again"];
    assert!(pd::suffix_match(&tail, &edit2).is_none(), "TAIL_EDIT_MAX = 1");

    // Two of five is 40%, under the bar.
    assert!(pd::suffix_match(&["a", "b", "c", "d", "e"], &["a", "b", "zz", "yy", "xx"]).is_none());
    // Nothing to match.
    assert!(pd::suffix_match(&[], &["x"]).is_none());
    assert!(pd::suffix_match(&["a"], &[]).is_none());

    // Wired through the tool: marking lines whose words differ from the take still answers with
    // `removed` seconds -- the trimmed stretch -- not the line bounds that were sent.
    let mut marks = Marks::new(
        vec![
            Line { n: 1, start: 0.0, end: 4.0 },
            Line { n: 2, start: 4.0, end: 8.0 },
            Line { n: 3, start: 8.0, end: 12.0 },
        ],
        12.0,
    );
    let replied = json(&marks.mark_abandoned(1, 2, None, Trim::none()));
    assert!(replied["error"].is_null(), "{replied}");
    let removed = [
        replied["removed"][0].as_f64().unwrap(),
        replied["removed"][1].as_f64().unwrap(),
    ];
    // A trim of 1.5 s off the front moves the removal inside the marked lines.
    let trimmed = json(&marks.mark_abandoned(3, 3, None, Trim { lead: 1.5, tail: 0.0 }));
    assert!(trimmed["trimmed"].as_bool().unwrap(), "{trimmed}");
    let second = [
        trimmed["removed"][0].as_f64().unwrap(),
        trimmed["removed"][1].as_f64().unwrap(),
    ];
    assert!((second[0] - 9.5).abs() < 1e-9, "cut inside the line: {second:?}");
    assert!(removed[0] < second[0], "the reported stretch is the trimmed one");
}

#[test]
fn sec_12_decisions_prepare_s2_both_edges_move_onto_the_envelope_and_are_seen_in_the_result() {
    // Rule 2.2: models propose, the machine places. The placement itself happens after `finish`, but
    // the placed seconds come back in the same result tuple, so the model is not kept in the dark.
    let input = Retake {
        s: 3.0,
        e: 4.0,
        again: 0.0,
        to: 4.0,
        text: String::new(),
        whole: String::new(),
    };
    let words = vec![word("stays", 1.0, 1.5), word("next", 6.0, 6.5)];
    let (placed, notes) = edge::place_edges(vec![input.clone()], &words, no_envelope, &[]);

    assert_eq!(placed.len(), 1);
    // The mark came back moved: placement is not a copy of what was handed over.
    assert_ne!(placed[0].s, input.s, "the left edge was re-placed: {:?}", placed[0]);
    // Notes are transcript lines for a human, one per note.
    for note in &notes {
        assert!(note.starts_with(">>> "), "a walk-back note: {note}");
        assert!(!note.contains('\n'), "one sentence: {note}");
    }
    // And the audit says so: after finish, with the placed seconds in the same result.
    let row = &pd::audit()[1];
    assert!(row.home.contains("after finish"), "{}", row.home);
    assert!(row.home.contains("rule 2.2"), "{}", row.home);
    assert_eq!(row.lives_in, "edges::place_edges");
    // Both edges are in what came back -- the pair (s, to) is the placed cut.
    assert!(placed[0].to >= placed[0].s);
}

#[test]
fn sec_12_decisions_prepare_s3_each_case_comes_back_with_one_sentence_of_reason() {
    assert_eq!(pd::classify(true, false), pd::MarkCase::WholeTake);
    let whole = pd::MarkCase::WholeTake.reason();
    assert!(whole.contains("again") && whole.contains('0'), "{whole}");

    assert_eq!(pd::classify(false, false), pd::MarkCase::RephraseFragment);
    let fragment = pd::MarkCase::RephraseFragment.reason();
    assert!(fragment.contains("fragment"), "{fragment}");

    // A second ASR pass overrides either reading of the first.
    assert_eq!(pd::classify(true, true), pd::MarkCase::Resurrected);
    assert_eq!(pd::classify(false, true), pd::MarkCase::Resurrected);
    let lifted = pd::MarkCase::Resurrected.reason();
    assert!(lifted.contains("second") && lifted.contains("lifted"), "{lifted}");

    // One sentence each: non-empty, single-line, all three different.
    let reasons = [whole, fragment, lifted];
    for reason in &reasons {
        assert!(!reason.trim().is_empty(), "an empty reason is no reason");
        assert!(!reason.contains('\n'), "one sentence: {reason}");
    }
    assert_ne!(whole, fragment);
    assert_ne!(fragment, lifted);
    assert_ne!(whole, lifted);

    // The audit row names the classifier as the owner of this home.
    let row = &pd::audit()[2];
    assert!(row.home.contains("result"), "{}", row.home);
    assert!(row.lives_in.contains("classify"), "{}", row.lives_in);
}

#[test]
fn sec_12_decisions_prepare_s4_over_the_ceiling_finish_tells_the_total_so_marks_come_back() {
    // P.machine.retakeCeil -- the share of spoken seconds that may be called abandoned.
    let mut marks = Marks::new(
        (1..=10)
            .map(|i| Line {
                n: i,
                start: f64::from(i - 1) * 2.0,
                end: f64::from(i) * 2.0,
            })
            .collect(),
        20.0,
    );
    // Six of twenty seconds marked is 0.3 -- under the ceiling.
    marks.mark_abandoned(1, 3, None, Trim::none());
    let under = marks.finish();
    assert!(!under.over, "{}", under.share);
    assert!((under.share - 0.3).abs() < 1e-9, "{}", under.share);

    // Push past it and finish reports the running total rather than silently discarding. Lines 4-6
    // overlap line 3's mark at a boundary, so they merge into one stretch -- which is itself reported.
    let merged = json(&marks.mark_abandoned(4, 6, None, Trim::none()));
    assert!(merged["merged"].as_bool().unwrap(), "touching marks merge: {merged}");
    let over = marks.finish();
    assert!(over.over, "0.6 of speech marked must exceed {}", over.ceiling);
    assert!((over.ceiling - 0.4).abs() < 1e-9, "// P.machine.retakeCeil");
    assert!(over.share > over.ceiling, "{}", over.share);
    assert_eq!(over.marks, 1, "the two marks merged into one stretch");

    // A second, separate stretch keeps the count honest about how many there are.
    marks.mark_abandoned(9, 10, None, Trim::none());
    assert_eq!(marks.finish().marks, 2);

    // Taking marks back is the way out -- unmark clears the condition without losing the session.
    // The whole merged stretch goes, leaving only the first mark standing.
    let taken = json(&marks.unmark(4, 10));
    assert!(taken["seconds_taken_back"].as_f64().unwrap() > 0.0, "{taken}");
    let back = marks.finish();
    assert!(!back.over, "the model took them back: {}", back.share);
    assert!((back.share - 0.3).abs() < 1e-9, "{}", back.share);
    // Clearing the rest leaves nothing marked at all.
    marks.unmark(1, 10);
    assert_eq!(marks.finish().marks, 0, "nothing left marked");
    assert!((marks.finish().share).abs() < 1e-9);

    // A run set aside logs both halves: what happened, and what is going on instead.
    let lines = retakes_log();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains("run 1") && lines[0].contains("set aside"), "{lines:?}");
    assert!(lines[1].contains("going on with 2"), "{lines:?}");
}

/// The set-aside log line pair, through the real helper.
fn retakes_log() -> Vec<String> {
    naivepost::tools::retakes::run_set_aside_log(0, "nothing matched", 2)
}

#[test]
fn sec_12_decisions_prepare_s5_drop_words_takes_counts_and_answers_with_the_words() {
    // tool:drop_words -- the count is an argument; the answer is the words that will go. The model is
    // never asked to restate the joined text, which is what lost a join over one respelled word before.
    let mut join = Join::new(
        &["so", "we", "can", "start", "the", "run"],
        &["and", "then", "go", "from", "here"],
    );
    let replied = json(&join.drop_words(Side::Before, 2));
    assert!(replied["error"].is_null(), "{replied}");
    assert_eq!(replied["side"], "before");
    assert_eq!(replied["count"], 2);
    let going: Vec<&str> = replied["words"]
        .as_array()
        .expect("a list of words")
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert_eq!(
        going,
        vec!["the", "run"],
        "the last two of the before side, in order"
    );
    // It answers with the words themselves, so nothing can be dropped unseen.
    assert!(replied["across_the_join"].is_string());
    // Nothing asks for a restatement of the join.
    assert!(json(&join.keep_join())["joined"].is_null(), "no `joined` field anywhere");

    // Zero named words is refused rather than read as an empty stretch at the seam.
    let mut fresh = Join::new(&["a", "b", "c"], &["d", "e"]);
    assert!(fresh.drop_words(Side::After, 0).contains("keep_join"));

    // P.machine.seamSnapWords -- how close to the join a named stretch still counts as being at it.
    assert_eq!(textedit::SEAM_SNAP_WORDS, 3);
    assert!(
        textedit::seam_snap(Side::Before, 3).is_ok(),
        "exactly the snap distance is still the join"
    );
    let too_far = textedit::seam_snap(Side::Before, 4).unwrap_err();
    assert!(too_far.contains("4 words short"), "{too_far}");
    // The two directions are told apart because they are fixed in opposite directions.
    let past = textedit::seam_snap(Side::After, 9).unwrap_err();
    assert!(past.contains("past the join"), "{past}");
    // P.machine.seamNoiseWords -- a stretch this small elsewhere reads as a respelling, not a cut.
    assert_eq!(textedit::SEAM_NOISE_WORDS, 2);
}

#[test]
fn sec_12_decisions_prepare_s6_dedupejoins_is_named_in_the_log_not_asked_of_the_model() {
    // S6: deleting a word the model deliberately kept stays after `finish` -- it is a rule about the
    // cut, not about the sentence -- and the log has to say it happened.
    let note = textedit::dedupe_note(12.0, "said twice");
    assert!(note.contains("12"), "{note}");
    assert!(note.contains("said again straight after the cut"), "{note}");
    assert!(note.contains("the earlier one goes"), "{note}");
    // It is a log line, not a tool reply: no JSON object, nothing the model answers to.
    assert!(!note.contains('{') && !note.contains('['), "not a tool answer: {note}");

    let row = &pd::audit()[5];
    assert!(row.what.contains("dedupeJoins"), "{}", row.what);
    assert!(row.home.contains("after finish"), "{}", row.home);
    assert!(row.home.contains("cut"), "a rule about the cut, not the sentence: {}", row.home);
    assert!(row.lives_in.contains("dedupe_note"), "{}", row.lives_in);
}

#[test]
fn sec_12_decisions_prepare_s7_one_bad_row_is_one_refusal_and_a_wrong_speaker_can_be_reported() {
    // tool:fix_line / tool:flag_line. The prototype threw away a block of 25 lines when one row's
    // time, speaker or tab did not match, and re-asked twice without saying why. Here a bad row is a
    // single refusal, and a speaker the model must not touch can still be flagged.
    let mut block = Block::new(vec![
        FixLine { n: 1, start: 0.0, end: 2.0, speaker: "SPEAKER_01".into(), text: "first line".into() },
        FixLine { n: 2, start: 2.0, end: 4.0, speaker: "SPEAKER_02".into(), text: "second line".into() },
    ]);

    // A tab would split the row on the way to disk, so it is refused by name.
    let tabbed = error(&block.fix_line(1, "two\tfields"));
    assert!(tabbed.contains("tab"), "{tabbed}");
    // A line outside this block is refused; the rest of the block survives.
    let outside = error(&block.fix_line(9, "not here"));
    assert!(outside.contains("line 9"), "{outside}");
    // A good fix lands.
    let ok = json(&block.fix_line(2, "second line, respelled"));
    assert!(ok["error"].is_null(), "{ok}");
    assert_eq!(ok["text"], "second line, respelled");

    // The flag reports without rewriting: the line stands exactly as it was.
    let flagged = json(&block.flag_line(1, "this speaker is wrong"));
    assert_eq!(flagged["flagged"], serde_json::json!(true));
    assert_eq!(flagged["why"], "this speaker is wrong");
    let applied = block.apply();
    assert_eq!(applied[0].text, "first line", "a flag changes nothing on the row");
    assert_eq!(applied[0].speaker, "SPEAKER_01", "nor the speaker");
    assert_eq!(applied[1].text, "second line, respelled", "but the fix did land");
    assert_eq!(block.flags().len(), 1);
    assert_eq!(block.fixes().len(), 1);
    // An out-of-block flag is refused too -- there is nowhere to attach the note.
    assert!(block.flag_line(9, "nowhere to put this").contains("\"error\""));

    let row = &pd::audit()[6];
    assert!(row.home.contains("per line"), "{}", row.home);
    assert!(row.home.contains("flag_line"), "{}", row.home);
}

#[test]
fn sec_12_decisions_prepare_s8_record_event_answers_with_the_frame_it_landed_on() {
    // tool:record_event. Drift beyond half an interval is dropped, an unparseable offset is dropped,
    // and instead of force-writing the batch's first frame the finish names it.
    let frames = [10.0, 11.0, 12.0];
    let interval = 1.0;

    // Within half an interval: that frame is what the stamp meant.
    assert_eq!(
        describe::place_offset(0.3, interval, &frames),
        Placement::Frame { frame: 1, at: 10.0, drift: 0.3 }
    );
    // Further than half an interval from every frame: gone, not rounded. Offsets count from the
    // batch's own first frame, so on a 1 s grid [0, 2] all land somewhere -- past the last frame's
    // half-interval there is no nearest frame and the line is dropped.
    assert_eq!(describe::place_offset(2.6, interval, &frames), Placement::TooFar(2.6));
    // Exactly half an interval still snaps -- the nearest frame is unique there.
    assert_eq!(
        describe::place_offset(0.5, interval, &frames),
        Placement::Frame { frame: 1, at: 10.0, drift: 0.5 }
    );

    // An unparseable offset is dropped with that reason named. (An *unlabelled* reply is a different
    // case -- filed as the description, `UnlabeledDescription` -- so the line carries the EVENT
    // label and only the offset fails to read.)
    let dropped = describe::parse_event_line("EVENT: +xx the tower", interval, &frames);
    assert!(format!("{dropped:?}").contains("UnparseableOffset"), "{dropped:?}");

    // record_event answers with the frame and its session second.
    let mut batch = Batch::new(10.0, interval, 3);
    let replied = json(&batch.record_event(2, "the tower fires", false));
    assert!(replied["error"].is_null(), "{replied}");
    assert_eq!(replied["frame"], 2);
    assert_eq!(replied["at"], 11.0);
    // A frame outside the batch is refused with the range named.
    assert!(error(&batch.record_event(9, "not shown", false)).contains("1..=3"));

    // Finish names what is still empty, and the opening frame with nothing behind it is called out by
    // the reason rather than quietly written as "Calm; same view.".
    batch.record_event(3, "smoke clears", true);
    let missing = batch.finish();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].frame, 1);
    assert_eq!(missing[0].at, 10.0);
    assert_eq!(missing[0].reason, MissingReason::FirstFrameWithNoHistory);
    // The string the prototype force-wrote is now only a name in the report.
    assert_eq!(Batch::SAME_WITHOUT_HISTORY, "Calm; same view.");
    // With history behind it the same gap is an ordinary missing event.
    let mut continued = Batch::new(20.0, interval, 2).with_history();
    continued.record_event(2, "still going", false);
    assert_eq!(continued.finish()[0].reason, MissingReason::NoEvent);

    let row = &pd::audit()[7];
    assert!(row.home.contains("record_event"), "{}", row.home);
    assert!(row.home.contains("finish"), "{}", row.home);
}

#[test]
fn sec_12_decisions_prepare_s9_speech_around_returns_every_overlap_with_no_cap() {
    // tool:speech_around. The brief used to take two lines per side per source, which made "nobody
    // spoke" and "your window was too small" the same answer. Overlap-not-containment, capped by
    // nothing, keeps them distinguishable.
    let lines = vec![
        Spoken { source: "mic-a".into(), start: 9.0, end: 11.0, text: "inside".into() },
        Spoken { source: "mic-b".into(), start: 10.5, end: 12.0, text: "crossing the edge".into() },
        Spoken { source: "mic-b".into(), start: 11.5, end: 13.0, text: "also inside".into() },
        Spoken { source: "mic-c".into(), start: 9.5, end: 10.2, text: "a third source".into() },
        Spoken { source: "mic-c".into(), start: 8.5, end: 9.4, text: "running into the window".into() },
    ];
    let batch = Batch::new(0.0, 1.0, 4);
    let around = batch.speech_around(9.0, 12.0, &lines);
    // All five overlap the window -- none of them is dropped for being the fourth or fifth line, or
    // the third source.
    assert_eq!(around.len(), 5, "{:?}", around.iter().map(|l| l.text.as_str()).collect::<Vec<_>>());
    // A line merely crossing the boundary is included: overlap, not containment.
    assert!(around.iter().any(|l| l.text == "crossing the edge"));
    // Reversed bounds still ask the same question.
    assert_eq!(batch.speech_around(12.0, 9.0, &lines).len(), 5);
    // A window with nothing in it returns nothing -- which now means nobody spoke, not that the
    // extract was full.
    assert!(batch.speech_around(100.0, 101.0, &lines).is_empty());
    // No cap constant exists to apply.
    let row = &pd::audit()[8];
    assert!(row.home.contains("capped by nothing"), "{}", row.home);
    assert!(row.lives_in.contains("speech_around"), "{}", row.lives_in);
}

#[test]
fn sec_12_decisions_prepare_s10_the_tool_loop_is_the_correction_not_a_quiet_replay() {
    // 09 F6.1. A tool's refusal travels back to the model as text it can act on; what changes between
    // rounds is the `tools` field, never a note telling the model it is being re-asked. Only a
    // transport retry repeats silently.
    let refused = ToolCall {
        name: "get_lines".into(),
        args: "{}".into(),
        result: tools_error("lines 9-9 are out of range"),
    };
    let back = tool_loop::tool_result(&refused);
    // Verbatim: the model gets the refusal itself, so it may pick another tool or another argument.
    assert_eq!(back, refused.result);
    assert!(back.contains("out of range"), "{back}");
    // An empty tool answer is still an answer the model sees, phrased as a refusal.
    let empty = ToolCall { name: "x".into(), args: "{}".into(), result: String::new() };
    assert!(tool_loop::tool_result(&empty).contains("\"error\""));

    // The fallback: a server that refuses the tools field gets asked again without it, once.
    assert!(tool_loop::retry_without_tools("HTTP 400: unknown field tools", 0, false));
    assert!(
        !tool_loop::retry_without_tools("HTTP 400: unknown field tools", 0, true),
        "a refusal already honoured is not asked twice"
    );
    // And the log of that says what changed -- the tools field, not a word to the model.
    let logged = tool_loop::refused_log("retake", "HTTP 400");
    assert!(logged.contains("asked again without them"), "{logged}");
    assert!(!logged.contains("re-asked"), "the model is never told: {logged}");

    let row = &pd::audit()[9];
    assert!(row.home.contains("tool loop is the correction"), "{}", row.home);
    assert!(row.home.contains("F6.1"), "{}", row.home);
    assert!(row.home.contains("transport retry is silent"), "{}", row.home);
}

/// `tools::error`'s shape, built without importing the module just for one string.
fn tools_error(reason: &str) -> String {
    format!("{{\"error\":{}}}", quote(reason))
}

/// JSON-quote a string the way `serde_json` would for a plain ASCII reason.
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[test]
fn sec_12_decisions_prepare_s11_the_audit_table_covers_all_ten_rows_in_order() {
    let table = pd::audit();
    assert_eq!(table.len(), 10);
    for row in &table {
        assert!(!row.what.is_empty(), "{row:?}");
        assert!(!row.home.is_empty(), "{row:?}");
        assert!(!row.lives_in.is_empty(), "{row:?}");
    }

    // The two after-finish homes: the envelope placement (rule 2.2) and dedupeJoins.
    assert!(table[1].home.contains("after finish"), "{}", table[1].home);
    assert!(table[5].home.contains("after finish"), "{}", table[5].home);

    // The result-and-argument homes name where the model is told.
    for index in [0usize, 2usize, 4usize, 6usize, 7usize] {
        let home = table[index].home;
        assert!(
            home.contains("result")
                || home.contains("answers")
                || home.contains("fix_line")
                || home.contains("drop_words"),
            "row {index} should name a told home: {home}"
        );
    }

    // Each row points at code that exists in this build.
    assert!(table[0].lives_in.contains("suffix_match"));
    assert!(table[3].lives_in.contains("Marks::finish"));
    assert!(table[3].lives_in.contains("P.machine.retakeCeil"));
    assert!(table[4].lives_in.contains("drop_words"));
    assert!(table[4].lives_in.contains("P.machine.seamSnapWords"));
    assert!(table[4].lives_in.contains("P.machine.seamNoiseWords"));
    assert!(table[6].lives_in.contains("fix_line"));
    assert!(table[6].lives_in.contains("flag_line"));
    assert!(table[9].lives_in.contains("tool_loop"));
}
