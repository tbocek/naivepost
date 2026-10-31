//! What a paste, a fold and a lane's ✕ leave behind.

use naivepost::cut::{Cut, Lane, Seg};
use naivepost::cut_copy::{copy, lay_over, spliced, Take};
use naivepost::cut_fold::{dropped_gaps, remove_lane, sync_folds};
use naivepost::cut_select::{Scope, Selection, ANY_ROW};

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn hand(scope: Scope, start: f64, end: f64) -> naivepost::cut_copy::Hand {
    let band = Selection { scope, start, end };
    match copy(Some(&band)) {
        Take::Taken(hand) => hand,
        _ => panic!("the selection copies"),
    }
}

#[test]
fn footage_under_a_laid_sound_is_not_a_gap() {
    let mut over = seg(15.0, 20.0);
    over.ins = "project:sound.wav".to_string();
    let segs = [seg(10.0, 15.0), over, seg(20.0, 30.0)];
    assert_eq!(dropped_gaps(&[(10.0, 30.0)], &segs), vec![]);
}

#[test]
fn a_fold_keeps_only_what_is_still_dropped() {
    let mut cut = Cut {
        segs: vec![seg(0.0, 35.0), seg(40.0, 50.0)],
        folds: vec![[30.0, 40.0], [10.0, 20.0]],
        ..Default::default()
    };
    sync_folds(&mut cut, &[(0.0, 50.0)]);
    assert_eq!(
        cut.folds,
        vec![[35.0, 40.0]],
        "a trim and an undo gave seconds back"
    );
}

#[test]
fn a_laid_sound_leaves_the_picture_as_it_was() {
    let mut footage = seg(0.0, 20.0);
    footage.cam = 1;
    footage.quiet = vec!["mic".to_string()];
    let mut cut = Cut {
        segs: vec![footage],
        ..Default::default()
    };
    let held = hand(
        Scope::Sound {
            recording: "mic-lav".to_string(),
        },
        0.0,
        5.0,
    );
    assert_eq!(
        lay_over(&mut cut, "project:mic-lav.wav", &held, 5.0, 0.0),
        1
    );
    let piece = cut.segs.iter().find(|s| !s.ins.is_empty()).unwrap();
    assert_eq!(piece.cam, 1);
    assert_eq!(piece.quiet, vec!["mic".to_string()]);
}

#[test]
fn a_copy_from_the_ruler_lands_on_a_real_row() {
    let held = hand(Scope::Footage { row: ANY_ROW }, 10.0, 15.0);
    assert_eq!(spliced(&held, 3.0).cam, 0);
}

#[test]
fn a_lanes_cross_takes_only_what_is_its_own() {
    let lane = |name: &str| Lane {
        name: name.to_string(),
        src: "a.mp4".to_string(),
        ..Default::default()
    };
    let mut picture = seg(0.0, 2.0);
    picture.ins = "a.mp4".to_string();
    let mut cut = Cut {
        lanes: vec![lane("Copied"), lane("Copied-2")],
        segs: vec![picture],
        ..Default::default()
    };
    remove_lane(&mut cut, "Copied");
    assert_eq!(cut.segs.len(), 1, "the other lane still windows that file");
    remove_lane(&mut cut, "Copied-2");
    assert!(cut.segs.is_empty());
}
