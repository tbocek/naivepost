// §10-parameters · P.eng.edgeTailDB — the word-fenced edge placement's tail threshold (12 dB;
// prototype `edgeTailDB`, gui/retake_edge.go). The rule already existed as `edges::EDGE_TAIL_DB`,
// read only by the private `Edges::tail_level`: take the peak over the word's own buckets, drop it by
// twelve decibels, and never below the room. A breath sits twenty dB or more under the word it
// follows; a trailing consonant a few dB under. Twelve is the line that keeps the `s` and leaves the
// breath out.
//
// What this file pins: the value, its byte equivalent on the cache's linear scale, and its single
// catalogue row; a tail inside the threshold being followed to the cap; a deeper tail being left at
// the aligner's end; the threshold being relative to the word's own peak rather than an absolute
// level; and that this bound belongs to the fenced side — the envelope-only methods judge against the
// room floor instead and never see it.
//
// Fixtures are lifted from `tests/envelope_floor_window.rs` — same hand-written 200 Hz envelope,
// ROOM = 8, WORD = 200. At this rate `EDGE_TAIL_MAX` is 50 buckets and `TROUGH_REACH` 80, and the
// ±4 s window around these stamps is mostly ROOM, so the room floor they are clamped against is 24.

use naivepost::edges::{self as edge, AlignedWord, Edges};
use naivepost::params;
use naivepost::textfmt::Retake;
use naivepost::wave::Wave;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

const HZ: f64 = 200.0;
const ROOM: u8 = 8;
/// A talk level, high above any plausible room floor.
const WORD: u8 = 200;

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

fn word(text: &str, s: f64, e: f64) -> AlignedWord {
    AlignedWord { word: text.into(), s, e }
}

#[allow(dead_code)]
fn mark(s: f64, e: f64, again: f64, to: f64) -> Retake {
    Retake { s, e, again, to, text: String::new(), whole: String::new() }
}

/// No envelope anywhere. Kept for shape-parity with the sibling fixture files; every case here
/// supplies one, since a tail threshold needs a sound to measure against.
#[allow(dead_code)]
fn none(_: f64) -> Option<&'static Edges> {
    None
}

/// A row found anywhere in the catalogue, asserted NON-EMPTY: some ids are deliberately catalogued twice
/// in this tree, so a strict helper would trip on sibling ids named below. This round's own single-row
/// check is done explicitly in s1.
#[allow(dead_code)]
fn anywhere(id: &str) -> params::Param {
    let found = all_rows().into_iter().filter(|row| row.id == id).collect::<Vec<_>>();
    assert!(!found.is_empty(), "{id} catalogued nowhere");
    found.into_iter().next().unwrap()
}

/// S1: `P.eng.edgeTailDB` = 12, held by `edges::EDGE_TAIL_DB`, catalogued once under the family §10
/// files it in.
#[test]
fn p_eng_edgetaildb_s1_the_threshold_is_12_db_and_catalogued_once() {
    // P.eng.edgeTailDB — "word-fenced edge placement: tail threshold".
    assert_eq!(edge::EDGE_TAIL_DB, 12.0);

    // The cache holds LINEAR peaks (a byte is peak × 255 of full scale) while this threshold is written
    // as a −70…0 dBFS meter, so twelve decibels arrives as forty-four bytes. The mismatch is recorded
    // in §04 as known-and-kept: this placement scored better against three hand-cut projects than every
    // alternative tried, including a real dB envelope.
    assert_eq!((edge::EDGE_TAIL_DB * 255.0 / 70.0).round(), 44.0);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.edgeTailDB")
        .expect("the tail threshold must be catalogued for Prepare, whose fenced placement reads it");
    assert_eq!(row.from, "edges::EDGE_TAIL_DB", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "12");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 12.0);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.edgeTailDB"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.edgeTailDB").count(),
        1,
        "P.eng.edgeTailDB catalogued more than once"
    );
}

/// S2: a tail within the threshold is the word, and the cut follows it until the follow-cap stops it.
#[test]
fn p_eng_edgetaildb_s2_a_tail_within_the_threshold_is_followed() {
    // P.eng.edgeTailDB: the word peaks at 200, so the threshold is 200 − 44 = 156. The plateau at 170
    // is thirty bytes (~8 dB) under — inside twelve — so it still counts as the word's own sound.
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 2.4, 170)]);
    let w = word("stays", 1.0, 1.5);
    // Followed to where EDGE_TAIL_MAX stops it: 1.5 + 0.25 s = 1.75, bucket 350.
    assert_eq!(e.end_after(&w, 4.0), 1.75, "the tail is walked to the follow-cap, not dropped at the stamp");
    assert!(e.end_after(&w, 4.0) > w.e, "inside the threshold the cut goes past the aligner's end");
}

/// S3: a tail deeper than the threshold is not the word, and nothing drags the cut into it.
#[test]
fn p_eng_edgetaildb_s3_a_tail_deeper_than_the_threshold_is_left_behind() {
    // P.eng.edgeTailDB: the same plateau at 100 is ~27.5 dB under the 200 peak — well past twelve —
    // so the walk stops dead at the aligner's end.
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 2.4, 100)]);
    let w = word("stays", 1.0, 1.5);
    assert_eq!(e.end_after(&w, 4.0), 1.5, "the cut lands on the aligner's end: the plateau is not the word");
    // The trough walk over the next 0.4 s finds nothing quieter than the plateau either, so no trough
    // pulls the edge into the breath: a tail deeper than twelve decibels is left out of the cut.
    assert!(e.end_after(&w, 4.0) <= w.e, "never dragged past the word by what follows it");

    // The contrast without literals: the identical envelope with the plateau back inside the threshold
    // is followed to 1.75. Twelve decibels is the whole difference between these two cuts.
    let kept = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 2.4, 170)]);
    assert_ne!(kept.end_after(&w, 4.0), e.end_after(&w, 4.0), "1.75 vs 1.5, the twelve-decibel line");
}

/// S4: the threshold is the word's OWN peak minus twelve decibels, not an absolute level — the same
/// byte value is the word under one peak and not under another.
#[test]
fn p_eng_edgetaildb_s4_the_threshold_is_the_words_own_peak_minus_twelve_decibels_not_an_absolute_level() {
    // A quieter word, 140, puts the threshold at 140 − 44 = 96.
    // Exactly AT the threshold still counts: `sound` compares >=.
    let at = edges(8.0, &[(1.0, 1.5, 140), (1.5, 2.0, 96)]);
    assert_eq!(at.end_after(&word("stays", 1.0, 1.5), 4.0), 2.0, "exactly at the threshold it still counts");

    // One byte UNDER and the tail is gone — the cut lands on the aligner's end.
    let under = edges(8.0, &[(1.0, 1.5, 140), (1.5, 2.0, 95)]);
    assert_eq!(under.end_after(&word("stays", 1.0, 1.5), 4.0), 1.5, "one byte under and the tail is gone");

    // So 96 is the word here and not under a 200 peak (where the threshold is 156): the bound travels
    // with the word, which is why it cannot be written as an absolute level.
    let loud = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 2.0, 96)]);
    assert_eq!(loud.end_after(&word("stays", 1.0, 1.5), 4.0), 1.5, "96 is not the word under a 200 peak");
}

/// S5: the tail threshold belongs to the fenced side. The envelope-only methods ask the room instead,
/// and each neighbouring constant answers a different question.
#[test]
fn p_eng_edgetaildb_s5_the_tail_threshold_belongs_to_the_fenced_side_and_is_not_its_neighbours() {
    // The s3 envelope: the plateau at 100 is NOT the word to `end_after`...
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 2.4, 100)]);
    let w = word("stays", 1.0, 1.5);
    assert_eq!(e.end_after(&w, 4.0), 1.5);
    // ...but plain SOUND to `end_before`, which judges against the room floor (24), never against
    // peak-minus-twelve. That is the difference edgeTailDB marks: the fenced side asks "is this still
    // the word", the envelope-only side asks "is this louder than the room".
    // (2.4 + 0.05 lands on 2.4499999999999997 in binary floating point, so the bound is compared with
    // a tolerance rather than as a literal.)
    assert!((e.end_before(2.5) - 2.45).abs() < 1e-9, "to the envelope-only rule the plateau is sound: the pad lands after it");
    assert_ne!(e.end_after(&w, 4.0), e.end_before(2.5), "two rules, two thresholds, two answers");

    // Each neighbour is a separate constant answering a separate question.
    // P.eng.edgePadSeconds (0.05): what is left of a sound once the envelope has placed it.
    assert_eq!(edge::EDGE_PAD, 0.05);
    // P.eng.wordPadSeconds (0.08): the room a cut leaves a word when there is no envelope to ask.
    assert_eq!(edge::WORD_PAD, 0.08);
    // P.eng.edgeReachSeconds (0.8): how far an edge may move off its stamp at all.
    assert_eq!(edge::EDGE_REACH, 0.8);
    // P.eng.lateStampSeconds (0.6): how late a stamp may trail its sound.
    assert_eq!(edge::LATE_STAMP, 0.6);
    // TROUGH_REACH (0.4): how far into a gap the quietest moment is looked for.
    assert_eq!(edge::TROUGH_REACH, 0.4);
    // EDGE_TAIL_MAX (0.25): how long the tail is followed at most — the time bound paired with this
    // level bound, and a different quantity from it.
    assert_eq!(edge::EDGE_TAIL_MAX, 0.25);
    // P.eng.envelopeWindowSeconds (4.0): the stretch the room itself is measured over.
    assert_eq!(edge::ENVELOPE_WINDOW, 4.0);

    // And this is the only one of them measured in decibels; every other bound on the list is seconds.
    assert_ne!(edge::EDGE_TAIL_DB, edge::EDGE_PAD, "a level is not a length");
    assert_ne!(edge::EDGE_TAIL_DB, edge::WORD_PAD, "a level is not a room");
    assert_ne!(edge::EDGE_TAIL_DB, edge::EDGE_REACH, "a level is not a reach");
    assert_ne!(edge::EDGE_TAIL_DB, edge::LATE_STAMP, "a level is not a lateness");
    assert_ne!(edge::EDGE_TAIL_DB, edge::TROUGH_REACH, "a level is not a search distance");
    assert_ne!(edge::EDGE_TAIL_DB, edge::EDGE_TAIL_MAX, "how deep a tail may be is not how long it is followed");
    assert_ne!(edge::EDGE_TAIL_DB, edge::ENVELOPE_WINDOW, "a level is not a window");

    // And this row points at the threshold, not at any neighbour's constant.
    assert_eq!(anywhere("P.eng.edgeTailDB").from, "edges::EDGE_TAIL_DB");
}
