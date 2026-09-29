// §10-parameters · P.eng.edgeReachSeconds — the search reach of the envelope-only edge placement
// (0.8 s; prototype `edgeReach` / `retake_edge`, gui/retake_edge.go). The rule already existed as
// `edges::EDGE_REACH`: `Edges::end_before` and `Edges::start_at` each turn it into a bucket count
// (`(EDGE_REACH * hz) as i64`) and hand the stamp back unchanged when their walk runs past it. It is
// "envelope-only" in the strict sense: with aligned word times the words fence the edge (`end_after` /
// `start_before`) and this bound is never consulted, so every assertion here goes through the two
// methods that ask the mono envelope alone.
//
// What this file pins: the value and its single catalogue row, that a gap inside the reach moves the cut
// onto the sound, that the same distance beyond the reach leaves the stamp where it was on BOTH sides,
// and that the reach is neither the late-stamp window nor either pad next door.
//
// Fixtures are lifted from `tests/word_pad_room.rs` — same hand-written 200 Hz envelope, same `word` /
// `mark` / `none` helpers — so nothing here invents a second way to build an edge. Expectations are
// built from the constants (`2.0 + edge::EDGE_PAD`) rather than bare literals, so a re-tuned neighbour
// moves the expectation with it instead of failing for the wrong reason.

use naivepost::edges::{self as edge, AlignedWord, Edges};
use naivepost::params;
use naivepost::textfmt::Retake;
use naivepost::wave::Wave;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows, word, anywhere};

const HZ: f64 = 200.0;
const ROOM: u8 = 8;
const WORD: u8 = 120;

/// An envelope of `total` seconds, silent at [`ROOM`] except in these spans.
fn wave(total: f64, spans: &[(f64, f64, u8)]) -> Wave {
    let mut peaks = vec![ROOM; (total * HZ) as usize];
    for (start, end, level) in spans {
        for bucket in (start * HZ) as usize..(end * HZ) as usize {
            peaks[bucket] = *level;
        }
    }
    Wave { hz: HZ, chans: vec![peaks] }
}

/// One recording filling the session clock from second nought.
fn edges(total: f64, spans: &[(f64, f64, u8)]) -> Edges {
    Edges::new(wave(total, spans), 0.0)
}

/// One aligned word. This round asks the envelope alone (no word times), so the helper is kept for
/// shape-parity with `tests/word_pad_room.rs` rather than used here.
#[allow(dead_code)]
fn mark(s: f64, e: f64, again: f64, to: f64) -> Retake {
    Retake { s, e, again, to, text: String::new(), whole: String::new() }
}

/// No envelope anywhere. This round's rule is the envelope-only walk, so every case supplies one; kept for
/// shape-parity with `tests/word_pad_room.rs`, where the pad DOES have a no-envelope branch.
#[allow(dead_code)]
fn none(_: f64) -> Option<&'static Edges> {
    None
}

/// A row found anywhere in the catalogue, asserted NON-EMPTY: some ids are deliberately catalogued twice
/// in this tree, so a strict helper would trip on sibling ids named below. This round's own single-row
/// check is done explicitly in s1.
#[allow(dead_code)]
/// S1: `P.eng.edgeReachSeconds` = 0.8, held by `edges::EDGE_REACH`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_edgereachseconds_s1_the_reach_is_0_8_and_catalogued_once() {
    // P.eng.edgeReachSeconds — "envelope-only edge placement: search reach".
    assert_eq!(edge::EDGE_REACH, 0.8);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.edgeReachSeconds")
        .expect("the reach must be catalogued for Prepare, whose edge placement reads it");
    assert_eq!(row.from, "edges::EDGE_REACH", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "0.8");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.8);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.edgeReachSeconds"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.edgeReachSeconds").count(),
        1,
        "P.eng.edgeReachSeconds catalogued more than once"
    );
}

/// S2: inside the reach the envelope places the cut. Half a second of quiet behind the stamp is well
/// within 0.8 s, so the walk steps out of the quiet, back over the sound run that stays, and lands just
/// past where that run ended — plus the post-envelope pad.
#[test]
fn p_eng_edgereachseconds_s2_a_quiet_gap_inside_the_reach_moves_the_cut_to_where_the_sound_stopped() {
    // P.eng.edgeReachSeconds: 0.5 s of quiet < 0.8 s, so the cut moves off its stamp.
    let e = edges(8.0, &[(1.0, 2.0, WORD)]);
    assert_eq!(e.end_before(2.5), 2.0 + edge::EDGE_PAD, "where the sound before it stopped, plus the pad");
    assert!(
        e.end_before(2.5) < 2.5,
        "inside the reach the envelope answers, so the cut ends earlier than the stamp"
    );
    // The moved distance is what the note reports: 0.5 - 0.05 of a second of quiet dropped.
    assert!((2.5 - e.end_before(2.5) - (0.5 - edge::EDGE_PAD)).abs() < 1e-9);
}

/// S3: the same shape too far back is not the same sound. A whole second of quiet exceeds the reach, so
/// the walk gives up and the stamp stands — the reach is a hard bound, not a soft preference.
#[test]
fn p_eng_edgereachseconds_s3_a_sound_further_off_than_the_reach_leaves_the_stamp_alone() {
    // P.eng.edgeReachSeconds: 1.0 s of quiet > 0.8 s, so nothing moves.
    let e = edges(8.0, &[(1.0, 2.0, WORD)]);
    assert_eq!(e.end_before(3.0), 3.0, "out of reach: the cut stays where the stamp says");

    // The boundary is the reach itself: the nearer gap moved, this one did not.
    assert_ne!(e.end_before(2.5), 2.5, "0.5 s away IS within the reach");
    assert_eq!(e.end_before(3.0), 3.0, "1.0 s away is not");

    // Through the flow too: no word times, so the envelope alone is asked, and being out of reach means
    // the mark keeps its own second and no note is written.
    let (marks, notes) = edge::place_edges(vec![mark(3.0, 4.0, 0.0, 4.0)], &[], |_| Some(&e), &[]);
    assert_eq!(marks[0].s, 3.0, "out of reach, the stamp stands through place_edges as well");
    assert!(notes.is_empty(), "a stamp that did not move writes no note");
}

/// S4: the resume side walks back over the retake's sound under the same reach. Inside it the cut starts at
/// the onset; further back than the reach it refuses to travel, because arriving there would clip two
/// seconds of sound the retake actually speaks.
#[test]
fn p_eng_edgereachseconds_s4_the_resume_side_walks_back_over_the_sound_only_inside_the_reach() {
    // P.eng.edgeReachSeconds: the onset is 0.4 s before the stamp, inside the reach, so the cut resumes
    // just before the sound begins.
    // The answer is a bucket boundary: `off + j/hz - EDGE_PAD`, so it lands on 4.545 (bucket 910) and
    // not on the round 4.55 — the envelope quantises the placement to its own 5 ms grid.
    let e = edges(8.0, &[(4.6, 4.9, WORD)]);
    assert!((e.start_at(5.0) - (4.6 - edge::EDGE_PAD)).abs() < 1.0 / HZ + 1e-9,
        "at the retake's onset, less the pad, within one bucket of quantisation");
    assert_eq!(e.start_at(5.0), 4.545, "the exact bucket the walk stops on");
    assert!(e.start_at(5.0) < 5.0, "inside the reach the envelope pulls the resume earlier");

    // The same call with the onset 2 s back: the walk hits the EDGE_REACH limit and hands the stamp back
    // rather than swallowing two seconds of speech.
    let long = edges(8.0, &[(3.0, 5.0, WORD)]);
    assert_eq!(long.start_at(5.0), 5.0, "past the reach the resume stays where the stamp says");
}

/// S5: the reach is its own bound, distinct from the three numbers that sit next to it on a cut edge.
#[test]
fn p_eng_edgereachseconds_s5_the_reach_is_not_the_late_stamp_window_nor_the_pad() {
    // A stamp with 0.7 s of quiet before it: inside EDGE_REACH, but outside LATE_STAMP (0.6), so the
    // quiet is too long to be a late stamp's tail and the reach does NOT rescue it — moving forward to
    // the next sound would clip the very word the stamp names.
    let e = edges(8.0, &[(3.7, 4.3, WORD)]);
    assert_eq!(e.start_at(5.0), 5.0, "the late-stamp window decides here, not the reach");
    assert!(edge::LATE_STAMP < edge::EDGE_REACH, "0.6 < 0.8: the window is the tighter of the two");

    // Each neighbour is a separate constant with a separate question.
    // P.eng.edgePadSeconds (0.05): what is left of a sound once the envelope has placed it.
    assert_eq!(edge::EDGE_PAD, 0.05);
    // P.eng.wordPadSeconds (0.08): the room a cut leaves a word when there is no envelope to ask.
    assert_eq!(edge::WORD_PAD, 0.08);
    // TROUGH_REACH (0.4): how far into a fenced gap the quietest moment is looked for — a search bound
    // too, but inside a fence the words set, so not this item.
    assert_eq!(edge::TROUGH_REACH, 0.4);

    assert_ne!(edge::EDGE_REACH, edge::EDGE_PAD, "how far to look is not what to leave behind");
    assert_ne!(edge::EDGE_REACH, edge::WORD_PAD, "the envelope's reach is not the word's room");
    assert_ne!(edge::EDGE_REACH, edge::LATE_STAMP, "the walk's bound is not the stamp-lateness bound");
    assert_ne!(edge::EDGE_REACH, edge::TROUGH_REACH, "the stamp-side reach is not the in-fence trough reach");

    // And this row points at the reach, not at any neighbour's constant.
    assert_eq!(anywhere("P.eng.edgeReachSeconds").from, "edges::EDGE_REACH");
    assert_eq!(anywhere("P.eng.wordPadSeconds").from, "edges::WORD_PAD");
}
