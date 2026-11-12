//! spec/05-cut.md F2.15: the camera follows the speaker.

use std::fs;

use crate::common;
use naivepost::cut::Seg;
use naivepost::cut_follow::{self, Switch};
use naivepost::layout::Tree;
use naivepost::textfmt::SessionLine;
use naivepost::timeline::Recording;

const HOST: i32 = 0;
const GUEST: i32 = 1;

fn rows(found: &[Switch]) -> Vec<i32> {
    found.iter().map(|switch| switch.row).collect()
}

/// S4: the picture changes a moment before the new speaker's first word.
#[test]
fn f2_15_s4_the_switch_leads_the_first_word() {
    let (found, interjections) =
        cut_follow::switches(&[(0.0, 10.0, HOST), (10.5, 30.0, GUEST), (30.4, 40.0, HOST)]);
    assert_eq!(rows(&found), [HOST, GUEST, HOST]);
    assert_eq!(interjections, 0);
    assert!(
        found[0].at.is_infinite(),
        "the picture starts on whoever speaks first"
    );
    assert!(
        (found[1].at - 10.2).abs() < 1e-9,
        "0.3 s before the first word: {}",
        found[1].at
    );
    assert!((found[2].at - 30.1).abs() < 1e-9);
}

/// S4: the lead never reaches back over the last word of the one before; while both talk, the picture
/// changes at the first word.
#[test]
fn f2_15_s4_the_lead_stops_at_the_last_word_and_an_overlap_switches_on_the_word() {
    let (found, _) = cut_follow::switches(&[(0.0, 10.0, HOST), (10.1, 30.0, GUEST)]);
    assert!(
        (found[1].at - 10.0).abs() < 1e-9,
        "not before the host has finished: {}",
        found[1].at
    );
    let (found, _) = cut_follow::switches(&[(0.0, 10.0, HOST), (9.0, 30.0, GUEST)]);
    assert!(
        (found[1].at - 9.0).abs() < 1e-9,
        "talked over: at the first word: {}",
        found[1].at
    );
}

/// S3: a short "mhm" inside the other's talk, or just before they go on, does not take the picture.
#[test]
fn f2_15_s3_an_interjection_does_not_take_the_floor() {
    let (found, interjections) = cut_follow::switches(&[
        (0.0, 20.0, GUEST),
        (8.0, 8.6, HOST),   // inside the guest's talk
        (20.3, 21.0, HOST), // "right" -- and the guest goes on
        (21.5, 40.0, GUEST),
    ]);
    assert_eq!(rows(&found), [GUEST], "the picture never leaves the guest");
    assert_eq!(interjections, 2);
    // The same half second followed by silence is a turn: nobody else went on.
    let (found, interjections) = cut_follow::switches(&[(0.0, 20.0, GUEST), (20.3, 21.0, HOST)]);
    assert_eq!(rows(&found), [GUEST, HOST]);
    assert_eq!(interjections, 0);
}

/// S3: one speaker's lines less than a second apart are one stretch of talk.
#[test]
fn f2_15_s3_close_lines_of_one_speaker_are_one_stretch() {
    let talk = cut_follow::stretches(&[(0.0, 4.0, HOST), (4.6, 9.0, HOST), (12.0, 15.0, HOST)]);
    assert_eq!(talk.len(), 2);
    assert!((talk[0].e - 9.0).abs() < 1e-9);
}

/// S4: a shot that would last under the shortest is not made; the picture stays.
#[test]
fn f2_15_s4_no_shot_under_the_shortest() {
    let (found, _) = cut_follow::switches(&[
        (0.0, 10.0, GUEST),
        (10.5, 12.3, HOST), // 1.8 s of talk: a turn, but its shot would last 2.0 s
        (12.5, 30.0, GUEST),
    ]);
    assert_eq!(
        rows(&found),
        [GUEST],
        "the short shot is not made: {found:?}"
    );
    let (found, _) = cut_follow::switches(&[
        (0.0, 10.0, GUEST),
        (10.5, 14.0, HOST), // long enough
        (14.4, 30.0, GUEST),
    ]);
    assert_eq!(rows(&found), [GUEST, HOST, GUEST]);
}

fn scene(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

/// S5: a scene is split at the switches inside it, each piece on its speaker's camera; nothing is added,
/// removed or moved.
#[test]
fn f2_15_s5_scenes_are_split_at_the_switches() {
    let (found, _) =
        cut_follow::switches(&[(0.0, 10.0, HOST), (10.5, 30.0, GUEST), (30.4, 40.0, HOST)]);
    let landed = cut_follow::apply(&[scene(0.0, 40.0)], &found, &|_, _, _| true);
    let shape: Vec<(f64, f64, i32)> = landed.iter().map(|seg| (seg.s, seg.e, seg.cam)).collect();
    assert_eq!(shape.len(), 3, "{shape:?}");
    assert_eq!(shape[0].2, HOST);
    assert_eq!(shape[1].2, GUEST);
    assert_eq!(shape[2].2, HOST);
    assert!((shape[0].1 - 10.2).abs() < 1e-9 && (shape[1].0 - 10.2).abs() < 1e-9);
    assert_eq!(
        (shape[0].0, shape[2].1),
        (0.0, 40.0),
        "the scene's own edges stand"
    );
    assert!(
        !landed[0].split && landed[1].split && landed[2].split,
        "a switch is a border nobody merges away"
    );
    let (changes, shown) = cut_follow::tally(&landed);
    assert_eq!(changes, 2);
    assert!(
        (shown[1].1 - 19.9).abs() < 1e-9,
        "the guest is shown for 19.9 s: {shown:?}"
    );
}

/// S5: a switch close to a join moves onto the join, so a cut in the talk and a change of camera are one cut.
#[test]
fn f2_15_s5_a_switch_near_a_join_moves_onto_it() {
    let (found, _) = cut_follow::switches(&[(0.0, 10.0, HOST), (10.5, 40.0, GUEST)]);
    // The cut dropped 11.0-11.5: the switch at 10.2 is 0.8 s before that join.
    let landed = cut_follow::apply(
        &[scene(0.0, 11.0), scene(11.5, 40.0)],
        &found,
        &|_, _, _| true,
    );
    let shape: Vec<(f64, f64, i32)> = landed.iter().map(|seg| (seg.s, seg.e, seg.cam)).collect();
    assert_eq!(
        shape,
        [(0.0, 11.0, HOST), (11.5, 40.0, GUEST)],
        "no sliver of a shot before the join"
    );
}

/// S5: a second a camera did not film is shown from the scene's own camera; an insert is left alone.
#[test]
fn f2_15_s5_unfilmed_seconds_and_inserts_are_left() {
    let (found, _) = cut_follow::switches(&[(0.0, 10.0, HOST), (10.5, 40.0, GUEST)]);
    let landed = cut_follow::apply(&[scene(0.0, 40.0)], &found, &|row, _, _| row == HOST);
    assert!(
        landed.iter().all(|seg| seg.cam == HOST),
        "the guest's camera filmed none of it"
    );
    let card = Seg {
        s: 5.0,
        e: 5.0,
        dur: 4.0,
        ins: "assets/a.svg".to_string(),
        ..Default::default()
    };
    let landed = cut_follow::apply(&[card.clone()], &found, &|_, _, _| true);
    assert_eq!(landed, [card]);
}

fn line(start: f64, end: f64, who: &str) -> SessionLine {
    SessionLine {
        start,
        end,
        source: "cam2".to_string(),
        who: who.to_string(),
        text: "talk".to_string(),
    }
}

/// S1–S6 through the door: who.json says whose camera is whose, the session's lines say who talks, and
/// the landed scenes come back split, with the log's two kinds of line.
#[test]
fn f2_15_follow_reads_who_json_and_the_session() {
    let dir = common::temp_dir_created("cut_follow_door").join("talk.naivepost");
    fs::create_dir_all(&dir).unwrap();
    let tree = Tree::new(&dir).expect("a project folder");
    fs::create_dir_all(tree.prepare_dir()).unwrap();
    naivepost::textfmt::write_session(
        &[
            line(0.0, 10.0, "SPEAKER_00"),
            line(10.5, 30.0, "SPEAKER_01"),
            line(12.0, 13.0, "EVENT"),
            line(30.4, 40.0, "SPEAKER_00"),
        ],
        &tree.session_tsv(),
    )
    .unwrap();
    let recordings = vec![
        Recording {
            base: "cam2".to_string(),
            start: 0.0,
            end: 60.0,
        },
        Recording {
            base: "cam1".to_string(),
            start: 0.0,
            end: 60.0,
        },
    ];
    let rows = [0usize, 1];

    let none = cut_follow::follow(&tree, &[scene(0.0, 40.0)], &recordings, &rows).unwrap_err();
    assert_eq!(none, "no prepare/who.json says who is on which camera");

    fs::write(
        naivepost::who::who_path(&tree),
        r#"{"people":[{"speaker":"SPEAKER_00","name":"Thomas","camera":"cam2","because":"asks"},
                      {"speaker":"SPEAKER_01","name":"Ada","camera":"cam1","because":"answers"}]}"#,
    )
    .unwrap();
    let (landed, logs) = cut_follow::follow(&tree, &[scene(0.0, 40.0)], &recordings, &rows)
        .expect("something to follow");
    let cams: Vec<i32> = landed.iter().map(|seg| seg.cam).collect();
    assert_eq!(cams, [0, 1, 0]);
    assert_eq!(logs[0], ">>> cameras: SPEAKER_00 is Thomas, shown on cam2");
    assert_eq!(
        logs.last().unwrap(),
        ">>> cameras: 2 switch(es) -- cam2 for 00:20, cam1 for 00:19; 0 interjection(s) left on the other's camera"
    );

    // Both speakers on one camera: nothing to choose between.
    fs::write(
        naivepost::who::who_path(&tree),
        r#"{"people":[{"speaker":"SPEAKER_00","camera":"cam2"},{"speaker":"SPEAKER_01","camera":""}]}"#,
    )
    .unwrap();
    assert!(
        cut_follow::follow(&tree, &[scene(0.0, 40.0)], &recordings, &rows)
            .unwrap_err()
            .starts_with("who.json puts fewer than two speakers on a camera")
    );
}
