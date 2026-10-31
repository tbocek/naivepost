//! What an edit may never do: bring back removed footage, lose a scene to a join the hand did not make, or walk
//! past the edits Revert promises to bring back.

use naivepost::cut::{Cut, History, Seg};
use naivepost::cut_play::cut_time;
use naivepost::cut_select::{Scope, Selection, ANY_ROW};
use naivepost::cut_trim::{merge_at_border, Border};
use naivepost::cut_verbs::{add, Outcome};

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn spans(segs: &[Seg]) -> Vec<(f64, f64)> {
    segs.iter().map(|seg| (seg.s, seg.e)).collect()
}

fn add_band(start: f64, end: f64, row: usize, segs: &[Seg]) -> Vec<Seg> {
    let band = Selection {
        start,
        end,
        scope: Scope::Footage { row },
    };
    match add(Some(&band), segs, &[]) {
        Outcome::Applied { segs, .. } => segs,
        _ => panic!("the band adds"),
    }
}

#[test]
fn add_leaves_scenes_clear_of_the_band_alone() {
    let out = add_band(10.0, 20.0, 0, &[seg(0.0, 5.0), seg(30.0, 40.0)]);
    assert_eq!(spans(&out), [(0.0, 5.0), (10.0, 20.0), (30.0, 40.0)]);
}

#[test]
fn add_still_splits_a_scene_it_lands_inside() {
    let out = add_band(10.0, 20.0, 0, &[seg(0.0, 30.0)]);
    assert_eq!(spans(&out), [(0.0, 10.0), (10.0, 20.0), (20.0, 30.0)]);
}

#[test]
fn add_on_an_empty_cut_makes_the_scene() {
    assert_eq!(spans(&add_band(10.0, 20.0, ANY_ROW, &[])), [(10.0, 20.0)]);
}

#[test]
fn a_join_crosses_only_the_border_the_hand_moved() {
    let mut split = vec![
        seg(0.0, 10.0),
        Seg {
            split: true,
            ..seg(10.0, 20.0)
        },
    ];
    assert_eq!(merge_at_border(&mut split, 0, Border::Start), None);
    assert_eq!(split.len(), 2, "the far border's Split stays");
    assert_eq!(merge_at_border(&mut split, 0, Border::End), Some(0));
    assert_eq!(spans(&split), [(0.0, 20.0)]);
}

#[test]
fn a_join_from_the_later_scene_names_the_survivor() {
    let mut segs = vec![seg(0.0, 10.0), seg(10.0, 20.0), seg(25.0, 30.0)];
    assert_eq!(merge_at_border(&mut segs, 1, Border::Start), Some(0));
    assert_eq!(spans(&segs), [(0.0, 20.0), (25.0, 30.0)]);
}

#[test]
fn undo_brings_back_what_revert_took() {
    let opened = Cut {
        segs: vec![seg(0.0, 10.0)],
        ..Default::default()
    };
    let edited = Cut {
        segs: vec![seg(0.0, 5.0)],
        ..Default::default()
    };
    let mut history = History::open(&opened);
    history.push(&edited);
    assert_eq!(history.revert().segs, opened.segs);
    assert_eq!(history.undo().expect("a step").segs, edited.segs);
}

#[test]
fn cut_time_reads_a_list_out_of_session_order() {
    let segs = [seg(10.0, 20.0), seg(0.0, 5.0)];
    assert_eq!(cut_time(&segs, 2.0), Some(12.0));
    assert_eq!(cut_time(&segs, 7.0), None);
}
