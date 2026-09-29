//! F4.7's rules, tested as plain logic: no window, no widget, no dial. Each test names the step whose
//! sentence or number it pins, so a reworded refusal fails here rather than in a screenshot.

use naivepost::cut::Seg;
use naivepost::narrate_edit::{self as edit, MoveResult};
use naivepost::narrate_screen;
use naivepost::narration::{Entry, Narration, Silent};

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

fn line(s: f64, e: f64, at: f64, text: &str, emotion: &str, pos: &str) -> Entry {
    Entry { s, e, at, text: text.into(), emotion: emotion.into(), pos: pos.into(), roll: 0 }
}

/// S1 (＋ beside the slider): the clip's ONLY entry, empty in every field, is the deliberate-silence
/// marker, and it becomes the new line instead of a second row being laid beside it.
#[test]
fn f4_7_s1_silent_marker_becomes_the_line_instead_of_a_second_row() {
    let marker = vec![line(20.0, 30.0, 0.0, "", "", "")];
    assert_eq!(edit::silent_marker_on(20.0, &marker), Some(0), "the lone empty entry IS the marker");

    // A written sibling means the clip speaks: nothing here is a marker to promote.
    let with_words = vec![line(20.0, 30.0, 0.0, "", "", ""), line(20.0, 30.0, 4.0, "spoken", "calm", "")];
    assert_eq!(edit::silent_marker_on(20.0, &with_words), None, "two entries on the clip: no marker");

    // A written entry alone is a line, not a silence.
    let spoken = vec![line(20.0, 30.0, 0.0, "only words", "calm", "")];
    assert_eq!(edit::silent_marker_on(20.0, &spoken), None, "a written entry is not a marker");

    // A tag or a delivery on an otherwise-empty entry makes it a caption/delivery, not a silence.
    let tagged = vec![line(20.0, 30.0, 0.0, "", "", "top")];
    assert_eq!(edit::silent_marker_on(20.0, &tagged), None, "a placement is not a silent marker");
}

/// S1: the three ways ＋ refuses, each with its own sentence, checked in the flowchart's order.
#[test]
fn f4_7_s1_add_at_playhead_three_refusals_each_with_its_own_words() {
    let segs = [seg(0.0, 10.0), seg(10.0, 20.0)];

    // Between clips: nothing to narrate.
    let gap = narrate_screen::add_at_playhead(25.0, &segs, &[]);
    assert_eq!(
        gap.err().as_deref(),
        Some("the playhead is between clips \u{2014} the cut has nothing to narrate here"),
        "a playhead off every clip says so"
    );

    // Within P.machine's near band of an existing line: jump to it instead of adding.
    let near = vec![line(0.0, 10.0, 2.0, "already here", "calm", "")];
    let too_close = narrate_screen::add_at_playhead(2.5, &segs, &near).expect_err("2.5 is within 1 s of 2.0");
    assert!(
        too_close.starts_with("a line already starts here \u{2014} edit it, or move the playhead (it is at "),
        "the near-line refusal names where that line is: {too_close}"
    );

    // Inside a speaking line: the estimate (chars / P.eng.speechCharsPerSecond 15 + the 0.2 s tail) is
    // what bounds the line until its take exists — this one starts at 2.0 and runs to 4.8, so pressing
    // at 4.5 lands under a voice that is still talking.
    let speaking = vec![line(0.0, 10.0, 2.0, "this line runs for several seconds and a few more", "calm", "")];
    let busy = narrate_screen::add_at_playhead(4.5, &segs[..1], &speaking).expect_err("still speaking at 4.5");
    assert!(
        busy.starts_with("a line is speaking here until "),
        "the speaking refusal opens with its own sentence: {busy}"
    );
    assert!(busy.ends_with(" \u{2014} add after it"), "and closes with what to do: {busy}");
}

/// S2 (＋ on a row): half a second after a row's audio, a flat 1.2 s when there was none.
#[test]
fn f4_7_s2_add_below_waits_0_5_after_words_and_1_2_after_a_wordless_row() {
    let worded = line(0.0, 20.0, 8.0, "words", "calm", "");
    let wordless = line(0.0, 20.0, 3.0, "", "", "");
    assert_eq!(edit::below_gap(&worded), 0.5, "// below.wordedSeconds");
    assert_eq!(edit::below_gap(&wordless), 1.2, "// below.wordlessSeconds");

    assert_eq!(edit::add_below(8.0, 20.0, 0.5), Ok(8.5), "audio end + the gap");
    assert_eq!(
        edit::add_below(3.0, 20.0, edit::below_gap(&wordless)),
        Ok(4.2),
        "a wordless row gets its flat 1.2 s"
    );
    assert_eq!(
        edit::add_below(8.6, 10.0, 0.5).err().as_deref(),
        Some("no room after this line \u{2014} the clip ends first"),
        "9.1 lands inside the clip's last second, so it is refused"
    );
}

/// S3 (time field): into another clip — the target's bounds are adopted and both seconds are said.
#[test]
fn f4_7_s3_move_into_another_clip_adopts_its_bounds_said_twice() {
    let entry = line(0.0, 10.0, 2.0, "a", "calm", "");
    let target = seg(10.0, 20.0);
    assert_eq!(edit::move_line(&entry, &target, 12.0), MoveResult::Moved { at: 2.0 });
    assert_eq!(
        edit::moved_said(&target, 2.0),
        "moved this line to the clip at 00:10 \u{2014} it now starts at 00:12.0",
        "the clip's start and the line's new start, both printed"
    );
}

/// S3: a second in the gap never moves the line, and the refusal recites all three numbers.
#[test]
fn f4_7_s3_a_gap_is_refused_and_written_back_to_where_it_was() {
    let entry = line(0.0, 10.0, 2.0, "a", "calm", "");
    let target = seg(10.0, 20.0);
    let said = match edit::move_line(&entry, &target, 25.0) {
        MoveResult::Refused { said } => said,
        MoveResult::Moved { at } => panic!("25.0 is outside the target clip, got Moved({at})"),
    };
    assert_eq!(said, narrate_screen::time_refused(25.0, 0.0, 10.0, 2.0));
    assert_eq!(
        said,
        "00:25 is outside the cut \u{2014} this line stays in its clip (00:00\u{2013}00:10), at 00:02.0",
        "typed second, the clip's bounds, and where it stayed"
    );
}

/// S3: never in the clip's last second. The clamp keeps the line where it was rather than sliding it
/// to the boundary, so a request inside the final second leaves the offset untouched.
#[test]
fn f4_7_s3_a_move_never_lands_in_the_clips_last_second() {
    let entry = line(10.0, 20.0, 2.0, "a", "calm", "");
    let target = seg(10.0, 20.0);
    let at = match edit::move_line(&entry, &target, 19.5) {
        MoveResult::Moved { at } => at,
        MoveResult::Refused { said } => panic!("19.5 is inside the clip, got Refused({said})"),
    };
    assert!(at < 20.0 - 1.0, "{at} sits in the clip's last second");
    assert_eq!(at, 2.0, "the previous offset survives rather than moving to the boundary");
}

/// S4 (🗑): only the clip's LAST line leaves a deliberate silence behind.
#[test]
fn f4_7_s4_only_a_clips_last_line_marks_it_deliberately_silent() {
    let a = line(0.0, 10.0, 1.0, "first", "calm", "");
    let b = line(10.0, 20.0, 1.0, "second", "calm", "");
    let entries = [a.clone(), b.clone()];
    assert!(edit::delete_leaves_clip_silent(&entries, &b), "b was alone on its clip");

    let a2 = line(0.0, 10.0, 4.0, "also first", "calm", "");
    let crowded = [a.clone(), a2.clone(), b.clone()];
    assert!(!edit::delete_leaves_clip_silent(&crowded, &a), "a sibling still speaks on that clip");
    assert!(edit::delete_leaves_clip_silent(&crowded, &b), "b is still alone on its own clip");
}

/// S4: the marker `narration.json`'s `silent` list takes, and the two answers that keep a delete
/// after a delete from writing the same clip twice.
#[test]
fn f4_7_s4_emptied_clip_gives_the_silent_marker_bounds() {
    let dir = std::env::temp_dir().join(format!("np-f47-rules-{}.naivepost", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp project folder");
    let tree = naivepost::layout::Tree::new(&dir).expect("a folder ending in .naivepost is a project");

    narration_default(&tree);
    let emptied = edit::emptied_clip(&tree, 40.0, 65.0).expect("nothing on that clip");
    assert!(!emptied.already_marked, "an untouched record marks nothing");
    assert_eq!(emptied.marker(), Silent { s: 40.0, e: 65.0 }, "the marker is the clip's bounds");

    let marked = Narration {
        entries: vec![],
        silent: vec![Silent { s: 40.0, e: 65.0 }],
    };
    naivepost::narration::save(&marked, &tree).expect("save the marked record");
    let again = edit::emptied_clip(&tree, 40.0, 65.0).expect("still emptied");
    assert!(again.already_marked, "a second delete knows it is already remembered");

    let still_spoken = Narration {
        entries: vec![line(40.0, 65.0, 1.0, "still here", "calm", "")],
        silent: vec![],
    };
    naivepost::narration::save(&still_spoken, &tree).expect("save the live record");
    assert_eq!(edit::emptied_clip(&tree, 40.0, 65.0), None, "a clip with a line is not emptied");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The file's own writer, called through a helper so the test reads as three states rather than setup.
fn narration_default(tree: &naivepost::layout::Tree) {
    naivepost::narration::save(&Narration::default(), tree).expect("save the empty record");
}

/// S5 (↻): one more roll, and nothing else.
#[test]
fn f4_7_s5_reroll_increments_the_roll() {
    assert_eq!(edit::reroll(0), 1);
    assert_eq!(edit::reroll(2), 3);
}

/// S1 (＋ beside the slider, node J): "within 1 s of a line \u2192 jump to it". The second jumped to is the
/// line's own start, and the window is `narrate.addNearSeconds` wide on either side of it.
#[test]
fn f4_7_s1_the_playhead_jumps_to_the_line_that_already_starts_there() {
    let entries = vec![line(0.0, 10.0, 3.0, "spoken", "calm", "")];
    // 3.6 is within the second: the answer is the LINE's start, not where the person pressed.
    assert_eq!(
        narrate_screen::near_line_start(3.6, &entries),
        Some(3.0),
        "a press near a line answers that line's start, which is what the page moves the playhead to"
    );
    // Exactly at it counts too — that is the case the sentence names.
    assert_eq!(narrate_screen::near_line_start(3.0, &entries), Some(3.0));
    // Past the window there is nothing to jump to, and ＋ goes on to add or refuse normally.
    assert_eq!(
        narrate_screen::near_line_start(3.0 + narrate_screen::ADD_NEAR_SECONDS, &entries),
        None,
        "the window is exclusive at its edge: one whole second away is free"
    );
    assert_eq!(narrate_screen::near_line_start(1.5, &entries), None, "before the window, free as well");
    assert_eq!(narrate_screen::near_line_start(3.0, &[]), None, "no lines, nothing to jump to");
}

/// S6 (text box): `[tag] words` — a placement clears the emotion, an emptied box keeps it, and `@N`
/// carries the second the line should move to.
#[test]
fn f4_7_s6_a_placement_tag_clears_the_emotion() {
    let caption = edit::parsed_text("[top] hello", "angry");
    assert_eq!((caption.pos.as_str(), caption.emotion.as_str(), caption.words.as_str()), ("top", "", "hello"),
        "a placement is a caption: read, never spoken, so no delivery survives it");

    let delivered = edit::parsed_text("[calm] hi", "angry");
    assert_eq!((delivered.pos.as_str(), delivered.emotion.as_str()), ("", "calm"), "a known tag is the delivery");

    let emptied = edit::parsed_text("   ", "angry");
    assert_eq!(emptied.emotion, "angry", "emptying the box keeps the old delivery");
    assert!(emptied.kept_old_emotion, "and says so, so the caller need not re-read the box");
    assert_eq!(emptied.words, "");

    let moved = edit::parsed_text("[top @12] hi", "");
    assert_eq!(moved.moves_to, Some(12.0), "@N is the second to move to");
    assert_eq!(moved.pos, "top", "the placement rides along with it");
    assert_eq!(moved.words, "hi");
}

/// S3/S4: the field's face reads back. `m:ss.s`, or a bare number of seconds; junk is no time at all.
#[test]
fn f4_7_s4_parse_clock_reads_the_field_face_back() {
    assert_eq!(edit::parse_clock("1:30.5"), Some(90.5));
    assert_eq!(edit::parse_clock("45"), Some(45.0));
    assert_eq!(edit::parse_clock(""), None, "an empty box is not a second");
    assert_eq!(edit::parse_clock("1:75"), None, "75 is not a minute's remainder");
    assert_eq!(edit::parse_clock("00:02.0"), Some(2.0), "the face the page prints reads straight back");
}
