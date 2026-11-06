//! F1.11 — two words that touch in the aligner's times: the cut goes where they part in the sound, not on the aligner's
//! 80 ms grid. 09-account 69.34: the cut after "Ethereum," sat 40 ms into the dropped "wie" and was heard; "Ethe-rum"
//! fell to a 60 ms dip at 69.27-69.33 and "wie" rose at 69.335. Envelopes are written by hand at 200 Hz.

use naivepost::edges::{place_edges, Edges, PART_REACH};
use naivepost::textfmt::Retake;

use crate::common;
use common::word;

const HZ: f64 = 200.0;
const ROOM: u8 = 3;

fn edges(spans: &[(f64, f64, u8)]) -> Edges {
    Edges::new(common::wave(4.0, spans, HZ, ROOM), 0.0)
}

/// "Ethe-rum" to 2.27, the dip, "wie" from 2.335: the 09-account shape, the aligner's boundary at 2.34.
fn ethereum_wie() -> Edges {
    edges(&[(1.0, 2.27, 30), (2.335, 2.50, 25), (2.50, 3.0, 28)])
}

#[test]
fn f1_11_a_boundary_on_the_next_words_onset_moves_into_the_dip() {
    let at = ethereum_wie().parting(2.34).expect("the boundary sits on sound, the dip is near");
    assert!((2.27..2.335).contains(&at), "inside the dip, before the dropped word rises: {at:.4}");
    assert!((2.34 - at) <= PART_REACH, "never further than one aligner step");
    assert!((PART_REACH - 0.08).abs() < 1e-9, "P.eng.partReachSeconds is one 80 ms step");
}

#[test]
fn f1_11_continuous_speech_with_no_dip_keeps_the_boundary() {
    let continuous = edges(&[(1.0, 3.0, 30)]);
    assert_eq!(continuous.parting(2.0), None, "no quieter moment within reach: the aligner's time stands");
}

#[test]
fn f1_11_a_boundary_already_in_quiet_stays() {
    let quiet = edges(&[(1.0, 2.0, 30), (2.2, 3.0, 30)]);
    assert_eq!(quiet.parting(2.1), None, "it is already where the words part");
}

#[test]
fn f1_11_place_edges_moves_a_touching_edge_to_where_the_words_part() {
    let env = ethereum_wie();
    let words = vec![word("ethereum", 1.0, 2.34), word("wie", 2.34, 2.50), word("zum", 2.50, 3.0)];
    let mark = Retake { s: 2.34, e: 3.0, again: 0.0, to: 3.0, text: String::new(), whole: String::new() };
    let (placed, notes) = place_edges(vec![mark], &words, |_| Some(&env), &[]);
    assert!((2.27..2.335).contains(&placed[0].s), "the cut ends in the dip: {:.4}", placed[0].s);
    assert!(notes.iter().any(|n| n.contains("where the two words part")), "{notes:?}");
    // Without an envelope the boundary stays on the word times, as before.
    let (bare, _) = place_edges(
        vec![Retake { s: 2.34, e: 3.0, again: 0.0, to: 3.0, text: String::new(), whole: String::new() }],
        &words,
        |_| None,
        &[],
    );
    assert_eq!(bare[0].s, 2.34);
}

#[test]
fn f1_11_the_resume_edge_of_touching_words_moves_too() {
    // The dropped "wie" ends where the kept "zum" starts at 2.34; "zum" rises at 2.335 after a dip.
    let env = edges(&[(1.0, 2.27, 25), (2.335, 3.0, 30)]);
    let words = vec![word("ethereum", 0.5, 1.0), word("wie", 1.0, 2.34), word("zum", 2.34, 3.0)];
    let mark = Retake { s: 1.0, e: 2.34, again: 0.0, to: 2.34, text: String::new(), whole: String::new() };
    let (placed, _) = place_edges(vec![mark], &words, |_| Some(&env), &[]);
    assert!((2.27..2.335).contains(&placed[0].to), "the cut resumes in the dip: {:.4}", placed[0].to);
}

/// One 5 ms peak per entry from `start` on, the room (3) elsewhere.
fn measured(start: f64, peaks: &[u8]) -> Edges {
    let mut wave = common::wave(4.0, &[], HZ, ROOM);
    let first = (start * HZ).round() as usize;
    wave.chans[0][first..first + peaks.len()].copy_from_slice(peaks);
    Edges::new(wave, 0.0)
}

#[test]
fn f1_11_a_soft_onset_is_left_out_of_the_kept_clip() {
    // 09-account on the recording's own clock, 69.240-69.400 s laid at 2.240: "-rum" falls, a valley, the soft "w"
    // of the dropped "wie" from 69.3475, the aligner's boundary at 69.364.
    let env = measured(2.240, &[18, 12, 10, 16, 16, 8, 10, 9, 7, 9, 6, 5, 8, 5, 7, 4, 5, 3, 4, 7, 4, 6, 6, 9, 7, 15, 22, 26, 18, 25, 23, 23]);
    let at = env.parting(2.364).expect("the boundary sits on the start of the w");
    assert!((at - 2.340).abs() < 1e-9, "the cut leaves the w out: {at:.4}");
}

#[test]
fn f1_11_a_short_word_said_straight_into_the_next_keeps_its_end() {
    // A pause, then the kept "Weg" for 60 ms straight into the dropped "und": no valley at the boundary, and the
    // pause before "Weg" is no place to cut — it would take the word.
    let env = edges(&[(1.0, 2.0, 25), (2.1, 2.16, 25), (2.16, 2.4, 25)]);
    assert_eq!(env.parting(2.16), None, "two words in one breath: the boundary stands");
}
