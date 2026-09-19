//! §02-services#3-tool-catalogue-rewrite-directive-b — 3.2 Describe and 3.3 Fix.
//!
//! Two jobs whose tools exist to stop the prototype's habit of throwing away good work over one bad
//! row: describe dropped an event line without a word when its offset drifted, and fix discarded a
//! whole block of 25 lines when one came back with a changed time. Both are asserted here as
//! behaviour, not as prose about behaviour.

use naivepost::tools::describe::{self, Batch, MissingReason, ParseOutcome, Spoken};
use naivepost::tools::fix::{self, Block, Line};

/// Four frames at one second apart, starting at 12 s into the session: the batch shape every test
/// below shares.
fn batch() -> Batch {
    Batch::new(12.0, 1.0, 4)
}

/// `record_event` names the frame it landed on and that frame's session second, replaces a second
/// recording of the same frame, and refuses a frame outside the batch.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_record_event_names_the_frame() {
    let mut b = batch();

    // 1-based as stamped: frame 3 of this batch is 14 s into the session, and that is what comes
    // back rather than the index the model sent.
    let body = b.record_event(3, "the slide changes", false);
    assert!(body.contains("\"frame\":3"), "{body}");
    assert!(body.contains("14"), "{body}");
    assert!(body.contains("the slide changes"), "{body}");
    assert!(body.contains("\"replaced\":false"), "{body}");

    // A frame recorded twice replaces, and says so: a plain ok would leave the model believing its
    // first words were still there.
    let again = b.record_event(3, "the speaker stands up", false);
    assert!(again.contains("\"replaced\":true"), "{again}");
    assert_eq!(b.events().len(), 1);
    assert_eq!(b.events()[0].text, "the speaker stands up");

    // Both ends of the range are refused, not clamped: a model that says frame 9 of 4 has lost the
    // batch, and filing its words on the last frame invents an event at a second nobody described.
    for off in [0, 5, 9] {
        let body = b.record_event(off, "words", false);
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("a refusal is JSON");
        let reason = parsed["error"].as_str().unwrap();
        assert!(reason.contains(&off.to_string()), "{reason}");
        assert!(reason.contains("1..=4"), "{reason}");
    }
    assert_eq!(b.events().len(), 1, "a refused frame records nothing");

    // `calm` travels with the event and is reported back.
    let body = b.record_event(2, "nothing changes", true);
    assert!(body.contains("\"calm\":true"), "{body}");

    // Insertion order is what `events()` keeps; the log wants them by frame.
    let mut b = batch();
    b.record_event(4, "late", false);
    b.record_event(1, "early", false);
    let frames: Vec<usize> = b.events().iter().map(|e| e.frame).collect();
    assert_eq!(frames, vec![4, 1]);
    let sorted: Vec<usize> = b.events_sorted().iter().map(|e| e.frame).collect();
    assert_eq!(sorted, vec![1, 4]);
}

/// `set_state` sets the running state; empty text clears it rather than leaving the old one.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_an_empty_state_clears_it() {
    let mut b = batch();
    let body = b.set_state("a lecture hall, one speaker at a lectern");
    assert!(body.contains("lecture hall"), "{body}");
    assert_eq!(b.state(), Some("a lecture hall, one speaker at a lectern"));

    // The clear is the point: a state that outlives what it describes gets read forward through
    // every later chunk and becomes the description of a scene nobody watched.
    let body = b.set_state("");
    assert!(body.contains("\"cleared\":true"), "{body}");
    assert_eq!(b.state(), None);

    // Whitespace and newlines are the same answer as nothing at all.
    b.set_state("still here");
    b.set_state("   \n  ");
    assert_eq!(b.state(), None);

    // A state written over two lines is one state.
    b.set_state("lap 3 of 5,\nthe red car last");
    assert_eq!(b.state().unwrap(), "lap 3 of 5, the red car last");
}

/// `speech_around` returns every line spoken in the stretch, from every recording — not the brief's
/// two-per-side extract.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_speech_around_takes_every_source() {
    let b = batch();
    // Two recordings, and one of them has six lines in range: a cap would silently drop four.
    let lines = vec![
        Spoken { source: "cam".into(), start: 8.0, end: 10.0, text: "before the window".into() },
        Spoken { source: "cam".into(), start: 13.0, end: 14.0, text: "inside".into() },
        Spoken { source: "mic".into(), start: 15.0, end: 16.0, text: "other recording".into() },
        Spoken { source: "mic".into(), start: 12.5, end: 13.5, text: "straddling the edge".into() },
        Spoken { source: "mic".into(), start: 40.0, end: 41.0, text: "far away".into() },
        Spoken { source: "cam".into(), start: 20.0, end: 21.0, text: "after".into() },
    ];

    let found = b.speech_around(12.0, 16.0, &lines);
    let texts: Vec<&str> = found.iter().map(|line| line.text.as_str()).collect();
    // Overlap, not containment: a sentence that starts before the window and runs into it is still
    // spoken there.
    assert!(texts.contains(&"straddling the edge"), "{texts:?}");
    // Every source, however many lines one of them has.
    assert_eq!(found.iter().filter(|l| l.source == "mic").count(), 2, "{texts:?}");
    assert_eq!(found.iter().filter(|l| l.source == "cam").count(), 1, "{texts:?}");
    // And nothing outside the stretch.
    assert!(!texts.contains(&"before the window"), "{texts:?}");
    assert!(!texts.contains(&"far away"), "{texts:?}");
    assert!(!texts.contains(&"after"), "{texts:?}");

    // Reversed bounds ask the same question.
    assert_eq!(b.speech_around(16.0, 12.0, &lines).len(), found.len());
}

/// `finish` asks for the frames still without an event, and for the batch's first frame when it was
/// left "same" with no history to be the same as.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_finish_asks_for_the_gaps() {
    // Frames 1..=4, only 2 answered.
    let mut b = batch();
    b.record_event(2, "a hand reaches off frame", false);
    let missing = b.finish();
    let frames: Vec<usize> = missing.iter().map(|m| m.frame).collect();
    assert_eq!(frames, vec![1, 3, 4]);
    // The opening second is asked for as itself, not as an ordinary gap: with nothing before it,
    // "same" would be a word pointing nowhere, which is why the prototype force-wrote a line there.
    assert_eq!(missing[0].reason, MissingReason::FirstFrameWithNoHistory);
    assert_eq!(missing[1].reason, MissingReason::NoEvent);
    assert_eq!(missing[0].at, 12.0, "the missing frame is named by its session second");

    // A first frame answered with "same" and no history is the same problem: the model is asked.
    let mut b = batch();
    for frame in 2..=4 {
        b.record_event(frame, "moving on", false);
    }
    b.record_event(1, "Same.", false);
    let missing = b.finish();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].frame, 1);
    assert_eq!(missing[0].reason, MissingReason::FirstFrameWithNoHistory);

    // With history behind it, "same" is a real answer and nothing more is asked.
    let mut b = batch().with_history();
    for frame in 1..=4 {
        let text = if frame == 1 { "same view" } else { "moving on" };
        b.record_event(frame, text, false);
    }
    assert!(b.finish().is_empty(), "{:?}", b.finish());

    // Every frame answered with something said: nothing missing.
    let mut b = batch();
    for frame in 1..=4 {
        b.record_event(frame, "something happens", false);
    }
    assert!(b.finish().is_empty());

    // The line the prototype used to write in its place, kept so a caller can still choose it.
    assert_eq!(Batch::SAME_WITHOUT_HISTORY, "Calm; same view.");
}

/// The prose-reply cleanups: snapped within half an interval, dropped beyond it, and the three other
/// ways a model's answer comes back wrong.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_a_drifted_offset_is_snapped_or_gone() {
    // Frames at 0, 1, 2 s with a one-second interval, so half an interval is 0.5 s.
    let frames = [0.0, 1.0, 2.0];

    // Exactly half snaps: at that distance the nearest frame is still unique.
    let out = describe::parse_event_line("EVENT [+0.5s]: a hand moves", 1.0, &frames);
    match out {
        ParseOutcome::Snapped { frame, text, .. } => {
            assert_eq!(frame, 1, "the nearest of the two it sits between");
            assert_eq!(text, "a hand moves");
        }
        other => panic!("half an interval snaps, got {other:?}"),
    }

    // Past half an interval is dropped without a word. Note that an offset of 0.51 s is only 0.49 s
    // from the *next* frame, so it snaps there — "nearest within half an interval" is measured to the
    // nearest one, not from the frame the model was looking at. Past the batch's last frame there is
    // no next one to be near, and 2.6 s on this grid belongs to no frame at all.
    let out = describe::parse_event_line("EVENT [+2.6s]: a hand moves", 1.0, &frames);
    assert_eq!(
        out,
        ParseOutcome::Dropped(describe::DropReason::TooFarFromEveryFrame)
    );

    // A wider interval lets a bigger drift snap: the bound is half of THIS batch's interval. 2 s off
    // is nothing on a five-second grid, where half an interval is 2.5 s — and with frames at 0, 1 and
    // 2 s the nearest one to 2 s is the last frame, which is also the batch's own end.
    let out = describe::parse_event_line("EVENT [+2.0s]: late", 5.0, &frames);
    assert!(matches!(out, ParseOutcome::Snapped { frame: 3, .. }), "{out:?}");

    // The nearest frame and its drift, exposed because a caller may want to say how far the model's
    // number was off rather than silently moving it.
    assert_eq!(
        describe::place_offset(0.5, 1.0, &frames),
        describe::Placement::Frame { frame: 1, at: 0.0, drift: 0.5 }
    );
    // 0.51 s is nearer the *next* frame than the one it drifted from, and that is the snap.
    assert_eq!(
        describe::place_offset(0.51, 1.0, &frames),
        describe::Placement::Frame { frame: 2, at: 1.0, drift: 0.49 }
    );
    // Nothing on the grid is within half an interval past the last frame.
    assert_eq!(describe::place_offset(2.6, 1.0, &frames), describe::Placement::TooFar(2.6));
    // An empty batch has no frame to land on, however small the offset.
    assert_eq!(describe::place_offset(0.0, 1.0, &[]), describe::Placement::TooFar(0.0));

    // An unparseable offset is dropped silently — and a missing bracket is the same case, because
    // there is no moment to attach the words to either.
    assert_eq!(
        describe::parse_event_line("EVENT [soon]: a hand moves", 1.0, &frames),
        ParseOutcome::Dropped(describe::DropReason::UnparseableOffset)
    );
    assert_eq!(
        describe::parse_event_line("EVENT a hand moves", 1.0, &frames),
        ParseOutcome::Dropped(describe::DropReason::UnparseableOffset)
    );

    // A negative offset is a number like any other.
    let out = describe::parse_event_line("EVENT [-1s]: before", 1.0, &frames);
    assert_eq!(
        out,
        ParseOutcome::Dropped(describe::DropReason::TooFarFromEveryFrame)
    );
}

#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s2_an_unlabelled_reply_is_the_description()
{
    // Six chunks in one recording wrote the description straight out and labelled the STATE correctly
    // underneath. The whole reply was then filed as the event: a sixty-word line about nothing, with
    // the state repeated inside it, in the brief the cut reads.
    let out = describe::parse_event_line(
        "a lecture hall, one speaker at a lectern STATE: still the lecture",
        1.0,
        &[0.0, 1.0],
    );
    match out {
        ParseOutcome::UnlabeledDescription(text) => {
            assert_eq!(text, "still the lecture", "the labelled half is the state");
        }
        other => panic!("an unlabelled reply is filed as the description, got {other:?}"),
    }

    // With no STATE line either, the words are the description whole.
    let out = describe::parse_event_line("a lecture hall, one speaker", 1.0, &[0.0]);
    assert_eq!(
        out,
        ParseOutcome::UnlabeledDescription("a lecture hall, one speaker".into())
    );

    // A multi-line answer is one answer: two lines in events.tsv would read as two events.
    let out = describe::parse_event_line(
        "EVENT [+1s]: the slide changes\nand the speaker turns to it",
        1.0,
        &[0.0, 1.0, 2.0],
    );
    match out {
        ParseOutcome::Flattened { frame, text, .. } => {
            assert_eq!(frame, 2);
            assert_eq!(text, "the slide changes and the speaker turns to it");
        }
        other => panic!("a multi-line answer is flattened, got {other:?}"),
    }

    // And a plain one-line event is none of those.
    let out = describe::parse_event_line("EVENT [+1s]: a hand moves", 1.0, &[0.0, 1.0]);
    assert!(matches!(out, ParseOutcome::Snapped { .. }), "{out:?}");

    // "same" is read the way the prototype read it: quotes and a full stop are decoration.
    for text in ["same", "Same.", "\"same\"", "same view", "Calm; same"] {
        assert!(describe::is_same(text), "{text}");
    }
    assert!(!describe::is_same("same as before, but louder"));
    assert!(!describe::is_same("a hand moves"));
}

/// `fix_line` refuses a row it cannot place or cannot store; everything else in the block stands.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s3_one_bad_row_is_one_refusal() {
    let mut block = Block::new(vec![
        Line { n: 1, start: 0.0, end: 2.0, speaker: "1".into(), text: "asr one".into() },
        Line { n: 2, start: 2.0, end: 4.0, speaker: "2".into(), text: "asr two".into() },
        Line { n: 3, start: 4.0, end: 6.0, speaker: "1".into(), text: "asr three".into() },
    ]);

    let body = block.fix_line(2, "respelled two");
    assert!(body.contains("\"line\":2"), "{body}");
    // A line no fix_line named keeps its ASR text — the whole point of the tool's shape.
    let applied = block.apply();
    assert_eq!(applied[0].asr_text(), "asr one");
    assert_eq!(applied[1].text, "respelled two");
    assert_eq!(applied[2].text, "asr three");

    // A number outside the block: refused, naming the range it should have picked from. The other
    // two lines keep their fixes rather than being thrown away with it — this is where the prototype
    // discarded all 25 rows of a block over one bad line.
    let body = block.fix_line(9, "somewhere else");
    let reason = serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(reason.contains("9"), "{reason}");
    assert!(reason.contains("1..=3"), "{reason}");
    assert_eq!(block.fixes().len(), 1);

    // A tab is the field separator of transcript.txt: a respelling carrying one would arrive on disk
    // as two rows. Said, not silently dropped.
    let body = block.fix_line(2, "two\tfields");
    let reason = serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(reason.contains("tab"), "{reason}");
    assert!(reason.contains("line 2"), "{reason}");
    assert_eq!(block.fixes()[0].1, "respelled two", "the refusal changed nothing");

    // Times and speakers are not arguments: `fix_line` takes a number and words, so there is no way
    // to move a line in time or reassign it. Asserted by the signature above; asserted again here by
    // what survives a fix.
    block.fix_line(1, "respelled one");
    let applied = block.apply();
    assert_eq!((applied[0].start, applied[0].end), (0.0, 2.0));
    assert_eq!(applied[0].speaker, "1");

    // A second fix of the same line replaces it rather than adding a row.
    block.fix_line(1, "respelled again");
    assert_eq!(block.fixes().iter().filter(|(n, _)| *n == 1).count(), 1);
    assert_eq!(block.apply()[0].text, "respelled again");

    // An empty block says so instead of refusing every number with a range it cannot print.
    let mut none = Block::new(Vec::new());
    let body = none.fix_line(1, "anything");
    assert!(body.contains("no lines"), "{body}");
}

/// `flag_line` logs what the model saw and must not change — the channel §3.3 says is missing.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s3_a_flag_leaves_the_line_standing() {
    let mut block = Block::new(vec![Line {
        n: 1,
        start: 0.0,
        end: 2.0,
        speaker: "1".into(),
        text: "asr one".into(),
    }]);

    let body = block.flag_line(1, "this is the second speaker, not the first");
    assert!(body.contains("\"flagged\":true"), "{body}");
    // The line stands, unchanged: a flag is a note, not an edit.
    assert_eq!(block.apply()[0].speaker, "1");
    assert_eq!(block.apply()[0].text, "asr one");
    assert_eq!(block.flags().len(), 1);
    assert_eq!(block.flags()[0].1, "this is the second speaker, not the first");

    // Notes are kept in order and can pile up on one line — a mis-timed row and a wrong speaker
    // together are two things a person needs to hear about.
    block.flag_line(1, "and the start is half a second late");
    assert_eq!(block.flags().len(), 2);

    // A flag on a line that is not in the block is refused like any other number.
    let body = block.flag_line(7, "not here");
    assert!(body.contains("error"), "{body}");
    assert_eq!(block.flags().len(), 2);
}

/// `get_lines` reads around the block from every source, beyond the brief's ±5 s window.
#[test]
fn sec_02_services_3_tool_catalogue_rewrite_directive_b_s3_the_context_window_grounds_across_sources(
) {
    // P.machine.fixContextSeconds
    assert_eq!(fix::FIX_CONTEXT_SECONDS, 5.0);

    let other_source = vec![
        // Exactly at the edge of the ±5 s window from `from` = 20: overlap is what counts, so a line
        // ending AT 15.0 says nothing inside it and falls away.
        Line { n: 1, start: 14.0, end: 15.0, speaker: "2".into(), text: "ends at the edge".into() },
        // Straddles the window's opening: started long before, spoken into it. Containment would
        // drop this, and it is the grounding a respelling most needs.
        Line { n: 2, start: 10.0, end: 16.0, speaker: "2".into(), text: "straddles in".into() },
        Line { n: 3, start: 24.0, end: 25.0, speaker: "1".into(), text: "inside".into() },
        // Ends just past the closing edge at 25 s, so it is still spoken inside the window.
        Line { n: 4, start: 24.5, end: 25.5, speaker: "2".into(), text: "straddles out".into() },
        Line { n: 5, start: 30.0, end: 31.0, speaker: "2".into(), text: "past the window".into() },
    ];

    let block = Block::default();
    let found = block.context_lines(20.0, 24.0, &other_source);
    let texts: Vec<&str> = found.iter().map(|line| line.text.as_str()).collect();
    assert!(texts.contains(&"straddles in"), "{texts:?}");
    assert!(texts.contains(&"inside"), "{texts:?}");
    assert!(texts.contains(&"straddles out"), "{texts:?}");
    assert!(!texts.contains(&"past the window"), "{texts:?}");
    assert!(!texts.contains(&"ends at the edge"), "{texts:?}");

    // Both sources are read: nothing here takes one line per recording.
    let mut block = Block::new(vec![Line {
        n: 1,
        start: 20.0,
        end: 24.0,
        speaker: "1".into(),
        text: "this block".into(),
    }]);
    block.fix_line(1, "respelled here");
    let all = block.context_lines(20.0, 24.0, other_source.iter().chain(block.lines().iter()));
    assert!(all.iter().any(|line| line.text == "this block"), "own lines are read too");
    assert!(all.len() >= 3);

    // Reversed bounds ask the same question.
    assert_eq!(
        block.context_lines(24.0, 20.0, &other_source).len(),
        block.context_lines(20.0, 24.0, &other_source).len()
    );
}
