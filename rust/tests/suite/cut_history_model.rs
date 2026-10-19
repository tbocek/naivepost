//! §05-cut#2-model-summary-full-schema-in-01-project-and-filesmd-3 — undo snapshots, their depth, and the base.
//!
//! What a snapshot holds, what it must not hold, how long the stack gets and where Revert goes: §2's
//! one sentence about the history, held by [`naivepost::cut::History`] so that F2.13's buttons have a
//! rule to forward to rather than a rule of their own (spec/00-principles.md §5).

use std::collections::BTreeMap;

use naivepost::cut::{self as cut, Cut, Fx, History, Lane, Seg, Snapshot};

/// A cut with one of every editable thing in it, plus a fold: the view that must not travel with a
/// snapshot (§B: folds are "a view, not an edit").
fn full_cut(segs: usize) -> Cut {
    Cut {
        segs: (0..segs).map(|i| Seg { s: i as f64 * 10.0, e: i as f64 * 10.0 + 5.0, ..Default::default() }).collect(),
        fx: vec![Fx { kind: "zoom".into(), t: 12.0, dur: 3.0, ..Default::default() }],
        aspect: "9:16".to_string(),
        shift: BTreeMap::from([("cam2".to_string(), -1.25)]),
        rows: BTreeMap::from([("cam2".to_string(), 1)]),
        lanes: vec![Lane { name: "cam-2".into(), src: "project:sources/cam.mkv".into(), at: 100.0, dur: 20.0, ..Default::default() }],
        nrows: 2,
        folds: vec![[20.0, 30.0]],
        ..Default::default()
    }
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s6_a_snapshot_holds_the_seven_editable_things_and_no_view()
{
    // §2: "undo snapshots (segments, effects, aspect, shift, rows, lanes, nrows)".
    let source = full_cut(3);
    let snap = Snapshot::of(&source);

    let mut target = Cut::default();
    snap.restore(&mut target);

    assert_eq!(target.segs, source.segs, "segments");
    assert_eq!(target.fx, source.fx, "effects");
    assert_eq!(target.aspect, "9:16", "aspect");
    assert_eq!(target.shift, source.shift, "shift");
    assert_eq!(target.rows, source.rows, "rows");
    assert_eq!(target.lanes, source.lanes, "lanes");
    assert_eq!(target.nrows, 2, "nrows");

    // The eighth field is not part of a snapshot: undoing an Add must not unfold the page.
    assert!(target.folds.is_empty(), "the view came from the target, not the snapshot");
    assert_eq!(source.folds, vec![[20.0, 30.0]], "and the source kept its fold to draw from");

    // A cut that was already folded stays folded across a restore.
    let mut watching = Cut { folds: vec![[44.0, 50.0]], ..Default::default() };
    snap.restore(&mut watching);
    assert_eq!(watching.folds, vec![[44.0, 50.0]], "undo does not unfold what was being watched");

    // A snapshot round-trips through the clone: nothing in it aliases the cut it came from.
    let mut later = source.clone();
    later.segs.clear();
    assert_eq!(Snapshot::of(&source).segs.len(), 3, "the copy kept its own segments");
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s7_undo_walks_back_and_a_new_edit_ends_the_redo_branch()
{
    // §A: "snapshot {…}; pushUndo clears redo".
    let opened = Cut::default();
    let mut history = History::open(&opened);
    assert!(!history.can_undo(), "nothing has been done yet");
    assert!(!history.can_redo());

    let one = full_cut(1);
    let two = full_cut(2);
    let three = full_cut(3);
    history.push(&one);
    history.push(&two);
    history.push(&three);
    assert!(history.can_undo() && !history.can_redo());

    let back_one = history.undo().expect("three edits to go back through");
    assert_eq!(back_one, Snapshot::of(&two));
    let back_two = history.undo().expect("still one more");
    assert_eq!(back_two, Snapshot::of(&one), "the state before the second edit");
    assert_eq!(back_two.segs.len(), 1);

    // Redo walks forward again while the branch is intact.
    assert!(history.can_redo());
    assert_eq!(history.redo().expect("the edit just undone"), Snapshot::of(&two));

    // A new edit here kills that branch: redoing into a past the person left behind would re-apply
    // something they undid, and nothing would say so.
    let other = full_cut(9);
    history.push(&other);
    assert!(!history.can_redo(), "the redo branch is gone");
    // `push` records what was handed it and leaves it on screen, so the state before this edit — which
    // is what an undo now returns — is `two`: the edit that was on screen when it was pushed. What the
    // new edit did kill is the redo branch, asserted just above; had `three` survived there, a redo
    // would have re-applied it after an edit that already replaced it.
    assert_eq!(history.undo().expect("back to what was on screen"), Snapshot::of(&two));
    assert_eq!(history.undo().expect("and one further"), Snapshot::of(&one));

    // And at the bottom there is simply nothing to undo: the stack still holds [opened, one], so one
    // more walk back reaches what the page opened with and the next ask is refused.
    assert_eq!(history.undo().expect("back to what the page opened with"), Snapshot::of(&opened));
    assert!(history.undo().is_none());
    assert!(!history.can_undo());
    assert!(history.undo().is_none(), "and asking twice changes nothing");
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s8_fifty_snapshots_is_all_the_history_holds_and_revert_goes_to_the_last_suggestion()
{
    // §2: "depth 50; base = the last suggestion or what the page opened with".
    let mut history = History::open(&Cut::default());
    for n in 0..60 {
        history.push(&full_cut(n + 1));
    }
    assert!(history.depth() <= cut::UNDO_DEPTH + 1, "{} held", history.depth());

    // The oldest states are unreachable: exactly UNDO_DEPTH undos and there is nothing left to undo,
    // which it could only be true of if the stack had dropped the first ten.
    for _ in 0..cut::UNDO_DEPTH {
        assert!(history.undo().is_some(), "walk {} back", cut::UNDO_DEPTH);
    }
    assert!(!history.can_undo(), "the bound is real: 60 edits did not leave 60 states");

    // Revert goes to the last suggestion, not to what the page opened with.
    let mut history = History::open(&full_cut(1));
    history.push(&full_cut(2));
    let suggested = full_cut(7);
    history.suggested(&suggested);
    history.push(&full_cut(8));
    assert!(!history.base_is_the_screen(&full_cut(8)));

    let back = history.revert();
    assert_eq!(back, Snapshot::of(&suggested), "the model's answer is the base now");
    assert!(history.base_is_the_screen(&suggested));
    assert!(!history.can_undo(), "the hand edits Revert threw away are gone from the stack too");

    // With no suggestion yet, the base is what the page opened with (§2).
    let opened = full_cut(4);
    let mut history = History::open(&opened);
    assert!(history.base_is_the_screen(&opened), "nothing to revert yet");
    history.push(&full_cut(5));
    assert_eq!(history.revert(), Snapshot::of(&opened));

    // A suggestion keeps the undo stack: ↶ after a suggestion that lost a run is still wanted.
    let mut history = History::open(&full_cut(1));
    history.push(&full_cut(2));
    history.suggested(&full_cut(7));
    assert!(history.can_undo(), "the hand edit is still undoable");
    assert_eq!(history.undo().expect("the pre-suggestion edit"), Snapshot::of(&full_cut(1)));
}
