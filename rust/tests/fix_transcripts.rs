//! F1.8 Fix the transcripts (spec/04-prepare.md) — blocks of P.machine.fixBlockLines = 25, asked
//! through the §3 fixer's tools (fix_line, flag_line, get_lines, finish), with the marking pass
//! P.policy.markingPass = retakes.

use std::cell::Cell;

use naivepost::fix_transcripts as fix;
use naivepost::layout::Tree;
use naivepost::prepare::FIX_BLOCK_LINES;
use naivepost::project::MarkingPass;
use naivepost::textfmt::{self, FrameEvent, Line, SessionLine};
use naivepost::tools::fix::FIX_CONTEXT_SECONDS;

const BASE: &str = "lecture";
const OTHER: &str = "crowd";

fn tree(tag: &str) -> Tree {
    // A project is a folder ending in .naivepost (spec/01-project-and-files.md §2), so that is what
    // `Tree::new` insists on.
    let dir = std::env::temp_dir().join(format!(
        "naivepost-fix-{tag}-{}-{}.naivepost",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Tree::new(&dir).unwrap()
}

fn line(start: f64, end: f64, speaker: &str, text: &str) -> Line {
    Line { start, end, speaker: speaker.into(), text: text.into() }
}

/// `count` rows of ten seconds each.
fn lines(count: usize) -> Vec<Line> {
    (0..count)
        .map(|index| {
            line(
                index as f64 * 10.0,
                index as f64 * 10.0 + 9.0,
                "SPEAKER_00",
                &format!("uh number {index} about the thing"),
            )
        })
        .collect()
}

fn source(base: &str, file_name: &str, start: f64, duration: f64, video: bool) -> fix::Source {
    fix::Source {
        base: base.into(),
        file_name: file_name.into(),
        start,
        duration,
        is_video: video,
        is_commentary: !video,
    }
}

fn session(start: f64, end: f64, source: &str, who: &str, text: &str) -> SessionLine {
    SessionLine { start, end, source: source.into(), who: who.into(), text: text.into() }
}

// --- S1: every source on the session clock, offsets.tsv written first -----------------------------

#[test]
fn f1_8_s1_the_earliest_stamped_name_is_second_nought() {
    // P.machine.fixBlockLines has nothing to do with placement, but the clock does: an unstamped name
    // sits at the session's start rather than at its mtime.
    let starts = fix::placement(&[
        "2026-09-18_14-00-10_crowd.wav",
        "2026-09-18_14-00-00_lecture.mkv",
        "phone.wav",
    ]);
    assert_eq!(starts[1], 0.0, "the earliest stamp is second nought");
    assert_eq!(starts[0], 10.0);
    assert_eq!(starts[2], 0.0, "unstamped sits at the session's start");

    let rows = fix::offsets_rows(&[
        source(BASE, "lecture.mkv", starts[1], 60.0, true),
        source(OTHER, "crowd.wav", starts[0], 60.0, false),
    ]);
    assert_eq!(rows, vec![(BASE.into(), OTHER.into(), -10.0)]);
    assert_eq!(fix::offsets_log(BASE, OTHER, -10.0), ">>> offset: lecture starts -10 s into crowd");

    // An all-audio session has nothing to pair.
    assert!(fix::offsets_rows(&[source(OTHER, "crowd.wav", 0.0, 60.0, false)]).is_empty());
}

#[test]
fn f1_8_s1_offsets_tsv_is_written_before_any_block_goes_out() {
    let tree = tree("offsets");
    let placed = [
        source(BASE, "lecture.mkv", 0.0, 60.0, true),
        source(OTHER, "crowd.wav", 10.0, 60.0, false),
    ];
    assert_eq!(fix::write_offsets(&tree, &placed).unwrap(), 1);
    assert_eq!(
        std::fs::read_to_string(tree.offsets_tsv()).unwrap(),
        "lecture\tcrowd\t-10.00\n"
    );

    // Nothing to pair: the file is still written, empty, so a reader never mistakes absence for zero.
    let audio_only = [source(OTHER, "crowd.wav", 0.0, 60.0, false)];
    assert_eq!(fix::write_offsets(&tree, &audio_only).unwrap(), 0);
    assert_eq!(std::fs::read_to_string(tree.offsets_tsv()).unwrap(), "");
}

#[test]
fn f1_8_s1_no_transcript_is_nothing_heard_not_a_crash() {
    let tree = tree("absent");
    // A source with no transcript is a recording with no speech in it, not a failure: the step asks
    // nothing of it and moves on.
    assert_eq!(fix::load_transcript(&tree, BASE).unwrap(), Vec::<Line>::new());
    assert!(fix::load_events(&tree, BASE).is_empty());

    textfmt::write_lines(&lines(2), &tree.transcript_tsv(BASE)).unwrap();
    assert_eq!(fix::load_transcript(&tree, BASE).unwrap(), lines(2));

    textfmt::write_events(
        &[FrameEvent { start: 1.0, end: 2.0, text: "a slide changes".into() }],
        &tree.events_tsv(BASE),
    )
    .unwrap();
    assert_eq!(fix::load_events(&tree, BASE).len(), 1);
}

// --- S2: blocks of P.machine.fixBlockLines with five seconds of context --------------------------

#[test]
fn f1_8_s2_blocks_of_p_machine_fixblocklines_twentyfive() {
    assert_eq!(FIX_BLOCK_LINES, 25);
    assert_eq!(fix::blocks(&lines(60)).len(), 3);
    // The short tail is still a block: padding it would ask for fixes to lines that do not exist.
    assert_eq!(fix::blocks(&lines(51))[2].len(), 1);
    assert!(fix::blocks(&[]).is_empty());
}

#[test]
fn f1_8_s2_the_context_reaches_five_seconds_past_the_block() {
    assert_eq!(FIX_CONTEXT_SECONDS, 5.0);
    assert_eq!(fix::context_window(20.0, 30.0), (15.0, 35.0));

    // Clipped at the session's own edges: nothing outside the timeline can be grounding.
    let blocks = fix::blocks(&lines(60));
    let asks = fix::asks_for(&blocks, 0.0, 600.0);
    assert_eq!(asks.len(), blocks.len());
    assert_eq!(asks[0].0, 1, "1-based: these are the blocks the log counts");
    assert_eq!(asks[0].1, 0.0, "the first window opens at nought, not at -5");
    assert_eq!(asks.last().unwrap().2, 600.0, "the last closes at the session's end");
    // A middle block reaches five seconds past its own rows in both directions: rows 25..=49 run
    // 250.00-259.00 for the first and 490.00-499.00 for the last.
    assert_eq!(asks[1], (2, 245.0, 504.0));
}

#[test]
fn f1_8_s2_the_grounding_is_every_other_source_and_this_one_s_events() {
    let placed = [
        source(BASE, "lecture.mkv", 0.0, 60.0, true),
        source(OTHER, "crowd.wav", 0.0, 60.0, false),
    ];
    let rows = vec![
        (BASE.to_string(), line(11.0, 13.0, "SPEAKER_00", "uh own   words")),
        (OTHER.to_string(), line(12.0, 14.0, "SPEAKER_01", "the crowd agrees")),
        // Touching the window's end is outside it.
        (OTHER.to_string(), line(20.0, 24.0, "SPEAKER_01", "later still")),
    ];
    let events = vec![
        (BASE.to_string(), FrameEvent { start: 12.5, end: 13.5, text: "a slide changes".into() }),
        (OTHER.to_string(), FrameEvent { start: 40.0, end: 41.0, text: "far away".into() }),
    ];

    let context = fix::context_text(10.0, 20.0, BASE, &placed, &rows, &events);
    assert_eq!(
        context,
        "SPEAKER_01 (crowd): the crowd agrees\nEVENT (lecture): a slide changes\n"
    );

    // A solo recording has nothing beside it, and that is normal rather than a problem.
    assert!(fix::context_text(10.0, 20.0, BASE, &placed[..1], &rows[..1], &[]).is_empty());
}

// --- S3: the fixer's tools, two asks per block, a refusal that never ends the flow ----------------

#[test]
fn f1_8_s3_the_ask_numbers_its_rows_and_carries_the_grounding() {
    let ask = fix::ask_text("EVENT (lecture): a slide changes\n", &lines(2));
    assert!(ask.contains("Transcript lines to clean (2 lines, return exactly 2):"));
    assert!(ask.contains("EVENT (lecture): a slide changes"), "the grounding is shown with the block");
    // `n` starts at 1 because that is what the model sends back.
    assert!(ask.contains("\n1\t0.00\t9.00\tSPEAKER_00\tuh number 0 about the thing\n"));
    assert!(ask.contains("\n2\t10.00\t19.00\tSPEAKER_00\tuh number 1 about the thing\n"));

    assert_eq!(fix::checkpoint(1, 3, BASE), ">>> fixing lecture: block 1/3");
    assert_eq!(fix::FIX_STEP, "fix");
}

#[test]
fn f1_8_s3_the_fixer_answers_through_fix_line_and_times_survive() {
    let outcome = fix::fix_block(&lines(3), |message| {
        // The reply is read off the message it was asked with, so the numbers mean the same thing.
        let mut reply = Vec::new();
        for row in message.lines().filter(|row| row.starts_with(char::is_numeric)) {
            let fields: Vec<&str> = row.split('\t').collect();
            assert_eq!(fields.len(), 5, "n, start, end, speaker, text");
            reply.push(fix::Reply::Fix {
                n: fields[0].parse().unwrap(),
                text: fields[4].replace("uh ", "").replace("thing", "widget"),
            });
        }
        Ok(reply)
    })
    .unwrap();

    assert!(outcome.valid);
    assert_eq!(outcome.fixed, 3);
    assert_eq!(outcome.lines[0].text, "number 0 about the widget");
    // A respell may not shift a time or change a speaker — §3 enforces it by having nowhere to put one.
    assert_eq!(outcome.lines[0].start, 0.0);
    assert_eq!(outcome.lines[2].end, 29.0);
    assert_eq!(outcome.lines[2].speaker, "SPEAKER_00");
}

#[test]
fn f1_8_s3_flag_line_and_get_lines_are_answers_not_refusals() {
    let outcome = fix::fix_block(&lines(2), |_| {
        Ok(vec![
            fix::Reply::Fix { n: 1, text: "number 0 about the widget".into() },
            fix::Reply::Flag { n: 2, why: fix::GARBAGE.into() },
            fix::Reply::Done,
        ])
    })
    .unwrap();

    assert!(outcome.valid, "only finish ends the flow, and it refuses nothing");
    assert_eq!(outcome.fixed, 1);
    assert_eq!(outcome.flags, vec![(2, fix::GARBAGE.to_string())]);
    // A flagged row keeps the ASR's text: a flag is a note for a person, not an edit.
    assert_eq!(outcome.lines[1], lines(2)[1]);
}

#[test]
fn f1_8_s3_two_asks_per_block_then_the_originals_stand() {
    assert_eq!(fix::BLOCK_TRIES, 2);

    let asked = Cell::new(0);
    let outcome = fix::fix_block(&lines(2), |_| {
        asked.set(asked.get() + 1);
        Ok(vec![fix::Reply::Fix { n: 99, text: "not a row of this block".into() }])
    })
    .unwrap();

    assert_eq!(asked.get(), fix::BLOCK_TRIES);
    assert!(!outcome.valid);
    assert_eq!(outcome.fixed, 0);
    assert_eq!(outcome.lines, lines(2), "the originals stand");
}

#[test]
fn f1_8_s3_the_second_ask_is_told_what_was_refused() {
    let seen = std::cell::RefCell::new(Vec::<String>::new());
    fix::fix_block(&lines(1), |message| {
        seen.borrow_mut().push(message.to_string());
        Ok(vec![fix::Reply::Fix { n: 9, text: "out of range".into() }])
    })
    .unwrap();

    let asks = seen.borrow();
    assert_eq!(asks.len(), fix::BLOCK_TRIES as usize);
    assert!(!asks[0].contains("refused"));
    assert!(asks[1].contains("The previous answer was refused"), "re-asking unchanged earns the same answer");
    assert!(asks[1].contains("line 9 is not in this block"), "the refusal names the line");
}

#[test]
fn f1_8_s3_apply_reply_goes_through_the_tool_surface() {
    let mut block = naivepost::tools::fix::Block::new(
        lines(2)
            .iter()
            .enumerate()
            .map(|(index, row)| naivepost::tools::fix::Line {
                n: index as u32 + 1,
                start: row.start,
                end: row.end,
                speaker: row.speaker.clone(),
                text: row.text.clone(),
            })
            .collect(),
    );

    let answers = fix::apply_reply(
        &mut block,
        &[
            fix::Reply::Fix { n: 1, text: "clean".into() },
            fix::Reply::Fix { n: 9, text: "nope".into() },
            fix::Reply::Flag { n: 2, why: "wrong speaker".into() },
            fix::Reply::Done,
        ],
    );
    assert_eq!(answers.len(), 4);
    assert!(answers[0].contains("\"text\""), "a fix answers with what it did");
    assert!(!answers[0].contains("error"));
    assert!(answers[1].contains("is not in this block"), "a refusal is an answer the model can use");
    assert!(!answers[2].contains("error"), "a flag is a note, not a failure");
    assert!(answers[3].contains("finished"));
    assert_eq!(block.flags(), [(2, "wrong speaker".to_string())]);
}

#[test]
fn f1_8_s3_a_block_is_cached_under_its_own_rows() {
    let tree = tree("cache");
    let rows = lines(2);
    // The key is the block's own text: one changed row asks a different question.
    let key = fix::block_key(&rows);
    assert_eq!(key.len(), 16, "a short stable hash names one request's reply");
    let mut changed = rows.clone();
    changed[1].text = "something else entirely".into();
    assert_ne!(key, fix::block_key(&changed));

    assert!(fix::cached_block(&tree, &rows).is_none(), "nothing asked yet, nothing to replay");

    let mut fixed = rows.clone();
    fixed[0].text = "number 0 about the widget".into();
    fix::store_block(&tree, &rows, &fixed).unwrap();

    // Only the row whose text differs is a `fix_line` call to replay.
    assert_eq!(
        fix::cached_block(&tree, &rows).unwrap(),
        vec![fix::Reply::Fix { n: 1, text: "number 0 about the widget".into() }]
    );
    // A changed transcript does not read the stale answer back.
    assert!(fix::cached_block(&tree, &changed).is_none());
}

// --- S4: transcript.fixed.tsv (+ subtitles.srt) or commentary.fixed.tsv ---------------------------

#[test]
fn f1_8_s4_a_video_writes_its_fixed_text_and_subtitles() {
    let tree = tree("video");
    let rows = lines(3);
    let written = fix::write_fixed(&tree, BASE, true, false, &rows).unwrap();

    assert_eq!(written, vec![tree.transcript_fixed_tsv(BASE), tree.subtitles_srt(BASE)]);
    assert_eq!(textfmt::read_lines(&tree.transcript_fixed_tsv(BASE)).unwrap(), rows);

    let subtitles = std::fs::read_to_string(tree.subtitles_srt(BASE)).unwrap();
    assert_eq!(subtitles.matches("-->").count(), 3, "the same text re-timed, one cue each");
    assert!(subtitles.contains("00:00:00,000 --> 00:00:09,000"));
    assert!(subtitles.contains("uh number 2 about the thing"));
}

#[test]
fn f1_8_s4_a_recorder_writes_commentary_and_nothing_to_caption() {
    let tree = tree("recorder");
    let rows = lines(2);
    let written = fix::write_fixed(&tree, OTHER, false, true, &rows).unwrap();

    assert_eq!(written, vec![tree.commentary_fixed_tsv(OTHER)]);
    assert!(!tree.subtitles_srt(OTHER).exists(), "there is nothing to caption over an audio file");
    assert_eq!(textfmt::read_lines(&tree.commentary_fixed_tsv(OTHER)).unwrap(), rows);
}

// --- S5: session.tsv, sorted by start, events labelled EVENT --------------------------------------

#[test]
fn f1_8_s5_the_merged_timeline_is_ordered_and_labels_events() {
    let rows = fix::merge_session(
        &[
            (BASE.to_string(), vec![line(20.0, 24.0, "SPEAKER_00", "third"), line(0.0, 4.0, "SPEAKER_00", "first")]),
            (OTHER.to_string(), vec![line(10.0, 14.0, "SPEAKER_01", "second")]),
        ],
        &[(BASE.to_string(), vec![FrameEvent { start: 10.0, end: 12.0, text: "a slide changes".into() }])],
    );

    // Sorted by start; a row and an event in the same second keep the order they were handed over in.
    assert_eq!(
        rows.iter().map(|row| (row.start, row.text.as_str())).collect::<Vec<_>>(),
        vec![(0.0, "first"), (10.0, "second"), (10.0, "a slide changes"), (20.0, "third")]
    );
    assert_eq!(rows[2].who, "EVENT");

    let tree = tree("session");
    textfmt::write_session(&rows, &tree.session_tsv()).unwrap();
    assert_eq!(textfmt::read_session(&tree.session_tsv()).unwrap(), rows);
    assert_eq!(fix::timeline_log(4, 2), ">>> session timeline: 4 rows across 2 source(s)");
}

// --- S6: the marking pass P.policy.markingPass names ---------------------------------------------

#[test]
fn f1_8_s6_p_policy_markingpass_none_asks_for_nothing() {
    let marks = fix::marks_for(MarkingPass::None, || panic!("asked when the policy says none")).unwrap();
    assert!(marks.is_empty());

    let mark = fix::Mark { start: 0.0, end: 10.0, again: Some(20.0) };
    let got = fix::marks_for(MarkingPass::Retakes, || Ok(vec![mark.clone()])).unwrap();
    assert_eq!(got, vec![mark]);

    // F1.9 owns retakes and F1.10 owns joins; this step only calls for whichever the policy names.
    let asked = Cell::new(0);
    fix::marks_for(MarkingPass::Joins, || {
        asked.set(asked.get() + 1);
        Ok(Vec::new())
    })
    .unwrap();
    assert_eq!(asked.get(), 1);
}

#[test]
fn f1_8_s6_a_marking_pass_that_cannot_run_is_named_by_pass() {
    assert_eq!(
        fix::marking_log("the marker is off", MarkingPass::Retakes),
        "!!! retakes: the marker is off -- the timeline stands unmarked"
    );
    assert_eq!(
        fix::marking_log("the marker is off", MarkingPass::Joins),
        "!!! joins: the marker is off -- the timeline stands unmarked"
    );
}

// --- S7: session.txt exactly as the cut model sees it --------------------------------------------

#[test]
fn f1_8_s7_a_row_is_labelled_by_voice() {
    assert_eq!(fix::label(BASE, "EVENT", ""), "EVENT");
    assert_eq!(fix::label(OTHER, "SPEAKER_03", OTHER), "NARRATOR");
    assert_eq!(fix::label(OTHER, "", OTHER), "NARRATOR", "the microphone is a role, not a diariser's guess");
    assert_eq!(fix::label(BASE, "   ", ""), "SPEAKER");
    assert_eq!(fix::label(BASE, "SPEAKER_02", ""), "SPEAKER_02");
    assert_eq!(fix::stamp(65.0), "01:05");
    assert_eq!(fix::stamp_span(0.0, 4.0), "00:00-00:04");
}

#[test]
fn f1_8_s7_a_marked_stretch_folds_to_one_line() {
    let rows = vec![
        session(0.0, 2.0, BASE, "SPEAKER_00", "first attempt"),
        session(4.0, 6.0, BASE, "SPEAKER_00", "still the attempt"),
        session(20.0, 22.0, BASE, "SPEAKER_00", "the take that stayed"),
    ];
    let mark = fix::Mark { start: 0.0, end: 10.0, again: Some(20.0) };

    // Two rows inside the mark, one line out — and it says what the cut does with them.
    assert_eq!(
        fix::fold_marks(&rows, &[mark]),
        "00:00 (abandoned attempt to 00:10, said again at 00:20 -- already removed, read straight past it)\n\
         [20s-22s | 00:20] SPEAKER_00: the take that stayed\n"
    );

    // A mark with no second take says nothing about one; a row starting on the mark's end is outside it.
    let bare = &fix::Mark { start: 0.0, end: 10.0, again: None };
    assert!(fix::fold_marks(&rows, &[bare.clone()]).starts_with("00:00 (abandoned attempt to 00:10 -- already removed"));
    let outside = vec![session(10.0, 12.0, BASE, "SPEAKER_00", "after")];
    assert!(fix::fold_marks(&outside, &[bare.clone()]).contains("SPEAKER_00: after"));

    // The narrator is named in the text the user opens as well as the text the cut reads.
    let narrated = vec![session(0.0, 2.0, OTHER, "SPEAKER_09", "the narration")];
    assert_eq!(fix::fold_marks(&narrated, &[]), "[0s-2s | 00:00] SPEAKER_09: the narration\n");
    assert_eq!(
        fix::labelled(&narrated, OTHER, &[]),
        "[0s-2s | 00:00] NARRATOR: the narration\n"
    );
}

#[test]
fn f1_8_s7_session_txt_is_written_as_the_cut_reads_it() {
    let tree = tree("txt");
    let rows = vec![session(0.0, 2.0, BASE, "SPEAKER_00", "one thing")];
    fix::write_session_text(&tree, &fix::session_text(&rows, "", &[])).unwrap();
    assert_eq!(
        std::fs::read_to_string(tree.session_txt()).unwrap(),
        "[0s-2s | 00:00] SPEAKER_00: one thing\n"
    );
}

// --- S1-S7 together -------------------------------------------------------------------------------

fn run_with<F, M>(tree: &Tree, sources: &[fix::Source], pass: MarkingPass, ask: F, mark: M) -> fix::Outcome
where
    F: FnMut(&str) -> Result<Vec<fix::Reply>, String>,
    M: FnMut() -> Result<Vec<fix::Mark>, String>,
{
    fix::run(tree, sources, "", pass, ask, mark).unwrap()
}

fn no_marks() -> Result<Vec<fix::Mark>, String> {
    Ok(Vec::new())
}

/// A fixer that respells every row it is shown, reading the numbers off its own message.
fn respeller() -> impl FnMut(&str) -> Result<Vec<fix::Reply>, String> {
    |message: &str| {
        let mut reply = Vec::new();
        for row in message.lines().filter(|row| row.starts_with(char::is_numeric)) {
            let fields: Vec<&str> = row.split('\t').collect();
            reply.push(fix::Reply::Fix {
                n: fields[0].parse().unwrap(),
                text: fields[4].replace("uh ", "").replace("thing", "widget"),
            });
        }
        Ok(reply)
    }
}

#[test]
fn f1_8_s7_a_run_places_fixes_merges_and_writes_every_file() {
    let tree = tree("run");
    textfmt::write_lines(&lines(3), &tree.transcript_tsv(BASE)).unwrap();
    let placed = [source(BASE, "lecture.mkv", 0.0, 60.0, true)];

    let outcome = run_with(&tree, &placed, MarkingPass::None, respeller(), no_marks);

    assert_eq!(outcome.blocks, 1);
    assert_eq!(outcome.fixed_lines, 3);
    assert_eq!(outcome.rows, 3);
    assert_eq!(outcome.sources, 1);
    assert_eq!(outcome.steps, vec![fix::checkpoint(1, 1, BASE)]);
    // S1's file is written before any block goes out, so the seconds a context names are real ones.
    assert!(tree.offsets_tsv().exists());
    assert_eq!(textfmt::read_lines(&tree.transcript_fixed_tsv(BASE)).unwrap()[0].text, "number 0 about the widget");

    // S5's log line ends the list, and S7's file is what the cut reads.
    assert_eq!(outcome.logs.last().unwrap(), &fix::timeline_log(outcome.rows, outcome.sources));
    let rows = textfmt::read_session(&tree.session_tsv()).unwrap();
    assert_eq!(std::fs::read_to_string(tree.session_txt()).unwrap(), fix::session_text(&rows, "", &[]));
    assert!(!outcome.marked);

    // Every file this step wrote is named.
    for path in &outcome.files {
        assert!(path.exists(), "{}", path.display());
    }
}

#[test]
fn f1_8_s7_two_sources_count_once_each_and_the_narrator_is_named() {
    let tree = tree("pair");
    textfmt::write_lines(&lines(2), &tree.transcript_tsv(BASE)).unwrap();
    textfmt::write_lines(&[line(0.0, 4.0, "SPEAKER_09", "uh the narrator said it")], &tree.transcript_tsv(OTHER)).unwrap();
    let placed = [
        source(BASE, "lecture.mkv", 0.0, 60.0, true),
        source(OTHER, "crowd.wav", 0.0, 60.0, false),
    ];

    let outcome = fix::run(&tree, &placed, OTHER, MarkingPass::None, respeller(), no_marks).unwrap();
    assert_eq!(outcome.sources, 2);
    // Each source's rows are asked about separately, so each gets its own checkpoints.
    assert_eq!(outcome.blocks, 2);

    let text = std::fs::read_to_string(tree.session_txt()).unwrap();
    assert!(text.contains("NARRATOR: the narrator said it"), "S7 names the microphone by role");
    assert!(text.contains("SPEAKER_00: number 0 about the widget"));
}

#[test]
fn f1_8_s7_a_refused_block_keeps_its_asr_text_and_the_run_goes_on() {
    let refused = tree("refused");
    textfmt::write_lines(&lines(2), &refused.transcript_tsv(BASE)).unwrap();
    let placed = [source(BASE, "lecture.mkv", 0.0, 60.0, true)];

    let outcome = run_with(
        &refused,
        &placed,
        MarkingPass::None,
        |_| Ok(vec![fix::Reply::Fix { n: 99, text: "out of range".into() }]),
        no_marks,
    );

    assert_eq!(outcome.fixed_lines, 0);
    let kept = outcome.logs.iter().find(|line| line.contains("kept its ASR text")).unwrap();
    assert!(kept.contains("refused"), "{kept}");
    // The originals stand on disk rather than being dropped.
    assert_eq!(
        textfmt::read_lines(&refused.transcript_fixed_tsv(BASE)).unwrap(),
        lines(2)
    );

    // A flag reaches a person through the log.
    let flagged = tree("flagged");
    textfmt::write_lines(&lines(2), &flagged.transcript_tsv(BASE)).unwrap();
    let outcome = run_with(
        &flagged,
        &placed,
        MarkingPass::None,
        |_| Ok(vec![fix::Reply::Flag { n: 2, why: "wrong speaker".into() }]),
        no_marks,
    );
    assert!(outcome.logs.contains(&">>> lecture line 2: wrong speaker".to_string()));
}

#[test]
fn f1_8_s7_a_marking_failure_leaves_the_timeline_unmarked_not_lost() {
    let tree = tree("markfail");
    textfmt::write_lines(&lines(2), &tree.transcript_tsv(BASE)).unwrap();
    let placed = [source(BASE, "lecture.mkv", 0.0, 60.0, true)];

    let outcome = run_with(
        &tree,
        &placed,
        MarkingPass::Retakes,
        respeller(),
        || Err("the marker is off".to_string()),
    );

    assert!(outcome.logs.contains(&fix::timeline_log(outcome.rows, outcome.sources)));
    assert!(outcome.logs.contains(&fix::marking_log("the marker is off", MarkingPass::Retakes)));
    assert!(!outcome.marked);
    // The fixes it just paid for are still there, unmarked.
    assert_eq!(textfmt::read_lines(&tree.transcript_fixed_tsv(BASE)).unwrap()[0].text, "number 0 about the widget");
    assert!(std::fs::read_to_string(tree.session_txt()).unwrap().contains("number 0 about the widget"));

    // And a pass that works marks: the fold reaches session.txt.
    let outcome = run_with(
        &tree,
        &placed,
        MarkingPass::Retakes,
        respeller(),
        || Ok(vec![fix::Mark { start: 0.0, end: 10.0, again: Some(20.0) }]),
    );
    assert!(outcome.marked);
    assert_eq!(outcome.marks, 1);
    assert!(std::fs::read_to_string(tree.session_txt())
        .unwrap()
        .contains("already removed, read straight past it"));
}

#[test]
fn f1_8_s7_policy_none_never_calls_the_marking_pass() {
    let tree = tree("nonemark");
    textfmt::write_lines(&lines(1), &tree.transcript_tsv(BASE)).unwrap();
    let placed = [source(BASE, "lecture.mkv", 0.0, 60.0, true)];

    let asked = Cell::new(0);
    let mut mark = || {
        asked.set(asked.get() + 1);
        Ok(Vec::new())
    };
    let outcome = fix::run(&tree, &placed, "", MarkingPass::None, respeller(), &mut mark).unwrap();
    assert_eq!(asked.get(), 0, "P.policy.markingPass = none is a policy, not an omission");
    assert!(!outcome.marked);
}
