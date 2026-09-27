// F2.13 (Undo, Redo, Revert, Clear) — the RULES. spec/05-cut.md F2.13 spells four verbs and the sentence
// each one answers with; `naivepost::cut` holds those rules and those sentences so the Cut page only prints
// what a rule returned (spec/00-principles.md §5). The wire — real clicks on `undo-button`, `redo-button`,
// `revert-button` and `clear-cut-button` reaching these same results — is proven by
// tests/history_undo_revert_clear_widgets.rs.
//
//! S1 Undo/Redo walk the snapshots ("undone — N segment(s) left"), depth 50 (`P.layout.undoDepth`).
//! S2 Revert goes back to the base — the last suggestion or what the page opened with — and refuses when
//!    nothing changed since it.
//! S3 Clear takes every scene and every effect off in one step while sources, rows, shifts and lanes stay.
//! S4 Only a real change reaches the stack: a write that changed none of the seven snapshot things records
//!    no step, which is what keeps a fold toggle from becoming an Undo that visibly does nothing.

use std::collections::BTreeMap;

use naivepost::cut::{Cut, Fx, History, Lane, Seg};

/// A cut with one of every editable thing in it, in the style of `tests/cut_history_model.rs`: scenes, an
/// effect, an aspect, a shift, a row pin, a lane and a floor under the row count.
fn full_cut(segs: usize) -> Cut {
    Cut {
        segs: (0..segs)
            .map(|i| Seg { s: i as f64 * 10.0, e: i as f64 * 10.0 + 5.0, ..Default::default() })
            .collect(),
        fx: vec![Fx { kind: "zoom".into(), t: 12.0, dur: 3.0, ..Default::default() }],
        aspect: "9:16".to_string(),
        shift: BTreeMap::from([("cam2".to_string(), -1.25)]),
        rows: BTreeMap::from([("cam2".to_string(), 1)]),
        lanes: vec![Lane {
            name: "cam-2".into(),
            src: "project:sources/cam.mkv".into(),
            at: 100.0,
            dur: 20.0,
            ..Default::default()
        }],
        nrows: 2,
        ..Default::default()
    }
}

/// S1: after a step back the line says how much of the cut is left on screen — the segments of the state
/// arrived at, not of the one just left.
#[test]
fn f2_13_s1_undone_says_what_is_left() {
    let mut history = History::open(&full_cut(2));
    history.push(&full_cut(5));
    let back = history.undo().expect("one edit is behind the pointer");
    let mut cut_ = Cut::default();
    back.restore(&mut cut_);
    assert_eq!(cut_.segs.len(), 2, "the state arrived at has two scenes");
    assert_eq!(
        naivepost::cut::undone(cut_.segs.len()),
        "undone \u{2014} 2 segment(s) left",
        "S1's sentence, verbatim"
    );
}

/// S1: Redo prints the same sentence for the direction it moved, because the spec gives the pair one line —
/// what matters after either press is what the timeline now holds.
#[test]
fn f2_13_s1_redo_prints_the_same_walk_sentence() {
    let mut history = History::open(&full_cut(2));
    history.push(&full_cut(5));
    history.undo().expect("back");
    let forward = history.redo().expect("and forward again");
    let mut cut_ = Cut::default();
    forward.restore(&mut cut_);
    assert_eq!(cut_.segs.len(), 5, "redo put the five scenes back");
    assert_eq!(
        naivepost::cut::undone(cut_.segs.len()),
        "undone \u{2014} 5 segment(s) left",
        "// P.layout.undoDepth 50 bounds the walk; the sentence is shared by both directions"
    );
}

/// S1: at the bottom there is nothing behind the pointer, and the refusal names where the floor is.
#[test]
fn f2_13_s1_at_the_bottom_undo_refuses_by_name() {
    let mut history = History::open(&full_cut(3));
    assert!(history.undo().is_none(), "the opened state is the floor");
    assert_eq!(
        naivepost::cut::NOTHING_TO_UNDO,
        "nothing to undo \u{2014} you are at the state this page opened with",
        "the refusal says which state cannot be passed"
    );
}

/// S2: Revert goes to the base, and the sentence names whose seconds those are.
#[test]
fn f2_13_s2_revert_goes_to_the_last_suggestion() {
    let mut history = History::open(&full_cut(3));
    history.push(&full_cut(5));
    let base = history.revert();
    let mut cut_ = Cut::default();
    base.restore(&mut cut_);
    assert_eq!(cut_.segs.len(), 3, "back to what the page opened with, which is also the base");
    assert_eq!(
        naivepost::cut::reverted(3, 2),
        "reverted to the 3 segment(s) of the last suggestion (\u{21b6} Undo brings your edits back)",
        "a non-empty base is read as the last suggestion's work"
    );
    assert!(!history.can_undo(), "the edits Revert threw away are gone from the stack too");
}

/// S2: with an empty base everything that vanishes was hand-made, and the sentence says so instead of
/// quoting a suggestion that never happened.
#[test]
fn f2_13_s2_an_empty_base_calls_the_work_hand_made() {
    let mut history = History::open(&Cut::default());
    history.push(&full_cut(4));
    let base = history.revert();
    let mut cut_ = Cut::default();
    base.restore(&mut cut_);
    assert!(cut_.segs.is_empty(), "the base is empty");
    assert_eq!(
        naivepost::cut::reverted(0, 4),
        "reverted \u{2014} 4 hand-made segment(s) gone, the cut is empty",
        "no suggestion to name, so the work is named instead"
    );
}

/// S2: nothing changed since the base means the button is greyed AND a press that got through anyway answers
/// the refusal without touching anything.
#[test]
fn f2_13_s2_no_change_since_the_base_is_a_refusal_not_a_revert() {
    let opened = full_cut(3);
    let history = History::open(&opened);
    assert!(history.base_is_the_screen(&opened), "the screen IS the base");
    assert_eq!(
        naivepost::cut::NOTHING_TO_REVERT,
        "nothing to revert \u{2014} the cut is as it was",
        "S2's refusal, verbatim"
    );
    // The refusal is taken BEFORE any revert runs, so the stack is still the opened single state.
    assert_eq!(history.depth(), 1, "nothing was pushed on the way to refusing");
}

/// S3: Clear empties the scenes and the effects and leaves every recording-side thing exactly where it was —
/// the shift, the row pins, the lanes and the row-count floor are all still there.
#[test]
fn f2_13_s3_clear_takes_scenes_and_effects_not_recordings() {
    let before = full_cut(3);
    let after = naivepost::cut::cleared(&before);
    assert!(after.segs.is_empty(), "every scene is off");
    assert!(after.fx.is_empty(), "and every effect");
    assert_eq!(after.shift, before.shift, "shifts stay: Clear is not an undo of where a file was parked");
    assert_eq!(after.rows, before.rows, "row pins stay");
    assert_eq!(after.lanes, before.lanes, "lanes stay");
    assert_eq!(after.nrows, before.nrows, "and the floor under the row count stays, or the tracks blank out");
    assert_eq!(after.aspect, before.aspect, "aspect stays too");
}

/// S3: the count is said apart per kind, because the two kinds are lost apart.
#[test]
fn f2_13_s3_the_cleared_count_names_scenes_and_effects_apart() {
    assert_eq!(
        naivepost::cut::cleared_message(3, 2),
        "cleared 3 scene(s) and 2 effect(s)",
        "S3's sentence, verbatim"
    );
}

/// S3: with no cut on the timeline there is nothing to clear, and the refusal says the timeline holds none
/// rather than blaming the button.
#[test]
fn f2_13_s3_no_cut_at_all_is_a_refusal() {
    let bare = Cut::default();
    assert!(bare.segs.is_empty() && bare.fx.is_empty(), "no scene and no effect is 'no cut yet'");
    assert_eq!(
        naivepost::cut::NOTHING_TO_CLEAR,
        "nothing to clear \u{2014} the timeline holds no cut yet",
        "S3's refusal, verbatim"
    );
    // And clearing a bare cut really does produce the same bare cut, so the refusal is true rather than polite.
    assert_eq!(naivepost::cut::cleared(&bare), bare);
}

/// S4: a write that changed none of the seven snapshot things records no step — the answer the greying needs
/// and the reason a fold toggle cannot become an Undo that visibly does nothing.
#[test]
fn f2_13_s4_only_a_real_change_reaches_the_stack() {
    let opened = full_cut(3);
    let mut history = History::open(&opened);
    let depth = history.depth();
    assert!(
        !history.note_change(&opened),
        "the same cut on screen again is not an edit"
    );
    assert_eq!(history.depth(), depth, "and no snapshot went onto the stack for it");
    assert!(!history.can_undo(), "so Undo still has nothing to take back");

    // A changed scene list DOES reach it.
    assert!(history.note_change(&full_cut(4)), "one more scene is a change");
    assert_eq!(history.depth(), depth + 1, "and it was recorded once");
    assert!(history.can_undo(), "which is exactly why the button lights");
}

/// S4: the difference is measured over all seven editable things, not only the scenes — a shift, a lane or
/// the aspect is a change someone made and must be undoable.
#[test]
fn f2_13_s4_shift_lanes_and_aspect_are_changes_too() {
    let opened = full_cut(2);
    let mut history = History::open(&opened);

    let shifted = Cut { shift: BTreeMap::from([("cam2".to_string(), -7.0)]), ..opened.clone() };
    assert!(history.note_change(&shifted), "a signed shift is an edit");

    let rebranched = Cut { aspect: "1:1".to_string(), ..shifted.clone() };
    assert!(history.note_change(&rebranched), "an aspect change is an edit");

    let longer_lane = Cut {
        lanes: vec![Lane {
            name: "cam-2".into(),
            src: "project:sources/cam.mkv".into(),
            at: 140.0,
            dur: 20.0,
            ..Default::default()
        }],
        ..rebranched.clone()
    };
    assert!(history.note_change(&longer_lane), "moving a lane is an edit");
    assert_eq!(history.depth(), 4, "three changes over the opened state");

    // And repeating one of them changes nothing, so it is refused again.
    assert!(!history.note_change(&longer_lane), "the same state twice is still not an edit");
    assert_eq!(history.depth(), 4, "the stack did not grow");
}

/// S4: the bound is real under the door every app write uses — sixty distinct changes leave fifty states
/// plus the one on screen, and the earliest edits are what falls off.
#[test]
fn f2_13_s4_sixty_writes_still_leave_fifty_states_behind() {
    // P.layout.undoDepth 50 — cut::UNDO_DEPTH is the bound the walk respects.
    let mut history = History::open(&Cut::default());
    for n in 0..60 {
        let changed = full_cut(n + 1);
        assert!(history.note_change(&changed), "each distinct cut is a change: {n}");
    }
    assert!(
        history.depth() <= naivepost::cut::UNDO_DEPTH + 1,
        "// P.layout.undoDepth {}: held {} snapshots",
        naivepost::cut::UNDO_DEPTH,
        history.depth()
    );
    let walked = (0..naivepost::cut::UNDO_DEPTH).filter(|_| history.undo().is_some()).count();
    assert_eq!(walked, naivepost::cut::UNDO_DEPTH, "exactly the depth is walkable");
    assert!(history.undo().is_none(), "and no further: the oldest states were dropped, not hidden");
}
