// §10-parameters · P.eng.troughReachSeconds — how far past the word's tail the quietest place to cut
// is searched (0.4 s; prototype `troughReach`, gui/retake_edge.go). The rule already existed as
// `edges::TROUGH_REACH`: after the tail walk stops, `end_after` scans `end..=end + 80 buckets` and
// keeps the strictly lowest bucket, and `start_before` mirrors that backwards. It picks up where
// `P.eng.edgeTailMaxSeconds` leaves off — the tail bound follows the word's own sound, this searches
// the gap behind it — and it belongs to the fenced side only: the envelope-only methods step back over
// the quiet onto the end of the sound run and never look for a trough.
//
// What this file pins: the value and its single catalogue row; a dip inside the reach pulling the cut
// off the tail's end; the same dip outside the reach being invisible (with the last bucket counted);
// ties falling toward the word because the scan compares strictly-less; the mirror on the resume side;
// and that the envelope-only side has no trough search at all.
//
// Fixtures are lifted from `tests/edge_tail_max.rs` — same hand-written 200 Hz envelope, ROOM = 8,
// WORD = 200. Base shape throughout: word [1.0,1.5) at 200 with a 0.1 s plateau of 170 at [1.5,1.6),
// so the tail threshold is 156, the tail walk stops at bucket 320, and the trough scan covers
// buckets 320..=400 (end + 80).

use naivepost::edges::{self as edge, AlignedWord, Edges};
use naivepost::params;
use naivepost::textfmt::Retake;
use naivepost::wave::Wave;

const HZ: f64 = 200.0;
const ROOM: u8 = 8;
/// A talk level, high above any plausible room floor.
const WORD: u8 = 200;
/// A dip below even the room: nothing quieter can be found in these envelopes.
const DIP: u8 = 4;

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
/// supplies one, since a trough needs a gap to search.
#[allow(dead_code)]
fn none(_: f64) -> Option<&'static Edges> {
    None
}

/// Every list that carries rows, chained across all five pages so uniqueness is checked against the whole
/// catalogue rather than one page (`tests/hands_off.rs`'s `row()` makes the same choice).
fn all_rows() -> Vec<params::Param> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .collect()
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

/// S1: `P.eng.troughReachSeconds` = 0.4, held by `edges::TROUGH_REACH`, catalogued once under the
/// family §10 files it in.
#[test]
fn p_eng_troughreachseconds_s1_the_reach_is_0_4_and_catalogued_once() {
    // P.eng.troughReachSeconds — "word-fenced edge placement: trough search".
    assert_eq!(edge::TROUGH_REACH, 0.4);
    // The SECONDS are the constant; the bucket count follows whatever rate this tree caches at, here
    // 200 Hz, so the search window is eighty buckets.
    assert_eq!((edge::TROUGH_REACH * HZ) as i64, 80);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.troughReachSeconds")
        .expect("the trough reach must be catalogued for Prepare, whose fenced placement reads it");
    assert_eq!(row.from, "edges::TROUGH_REACH", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "0.4");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.4);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.troughReachSeconds"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.troughReachSeconds").count(),
        1,
        "P.eng.troughReachSeconds catalogued more than once"
    );
}

/// S2: a dip inside the reach moves the cut off the tail's end onto the quietest bucket.
#[test]
fn p_eng_troughreachseconds_s2_the_cut_settles_into_a_trough_inside_the_reach() {
    // P.eng.troughReachSeconds: the tail stops at bucket 320 and the scan runs to 400; the dip starts
    // at bucket 370, inside that window and below even the room's 8, so it wins.
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.6, 170), (1.85, 1.95, DIP)]);
    let w = word("stays", 1.0, 1.5);
    assert_eq!(e.end_after(&w, 4.0), 1.85, "the cut moves off the tail's end onto the quietest bucket in reach");
    assert!(e.end_after(&w, 4.0) > 1.6, "further from the word than the tail's end: the trough pulled it");
    assert!(
        e.end_after(&w, 4.0) <= 1.5 + edge::EDGE_TAIL_MAX + edge::TROUGH_REACH,
        "never past tail cap plus reach"
    );
}

/// S3: the same dip beyond the reach is not found at all — and the boundary itself is scanned.
#[test]
fn p_eng_troughreachseconds_s3_a_quieter_spot_beyond_the_reach_is_not_found() {
    let w = word("stays", 1.0, 1.5);
    // 2.1 s is bucket 420, past trough_end 400: invisible, so the cut stays on the tail's end.
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.6, 170), (2.1, 2.2, DIP)]);
    assert_eq!(e.end_after(&w, 4.0), 1.6, "the dip is out of reach: the cut stays on the tail's end");

    // 2.0 s is bucket 400, which IS trough_end — the range is inclusive, so this one is scanned.
    let at = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.6, 170), (2.0, 2.05, DIP)]);
    assert_eq!(at.end_after(&w, 4.0), 2.0, "bucket 400 IS trough_end and is scanned");

    // Ten tenths of a second decide whether the quietest place in the gap is reachable at all.
    assert_ne!(e.end_after(&w, 4.0), at.end_after(&w, 4.0), "0.1 s decides whether the trough is reachable");
}

/// S4: equal dips fall toward the word (the scan keeps the first lowest bucket), and the same reach
/// bounds the walk back on the resume side.
#[test]
fn p_eng_troughreachseconds_s4_the_earliest_quietest_bucket_wins_and_the_reach_mirrors_on_the_resume_side() {
    let w = word("stays", 1.0, 1.5);
    // Two identical dips inside the window: the comparison is strict `<`, so the later one never takes
    // over and the cut settles on the nearer, keeping the cut as short as the tie allows.
    let ties = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.6, 170), (1.7, 1.75, DIP), (1.9, 1.95, DIP)]);
    assert_eq!(ties.end_after(&w, 4.0), 1.7, "equal dips: the strict less-than keeps the first, toward the word");

    // Mirrored: `start_before` walks back from the word's own start bucket only while the previous
    // bucket is at or above the tail threshold, so with silence right before the word it does not move
    // and the search window is the full `[word start − 80 buckets, word start]`. For r = [2.2, 2.7)
    // that is buckets 360..=440 = [1.8, 2.2). The dip's first sub-DIP bucket in that window is 370,
    // so the resume lands on 1.85 — the earliest quietest bucket, again because the scan is `<`.
    let r = word("resumes", 2.2, 2.7);
    let back = edges(8.0, &[(1.75, 1.85, 170), (1.85, 1.9, DIP), (2.2, 2.7, WORD)]);
    assert_eq!(back.start_before(&r, 1.0), 1.85, "the dip inside the backward reach takes the resume");

    // The same dip moved to [1.5,1.55) is buckets 300..309 — outside the window's 360 floor, so it is
    // never seen. What the scan does find inside 360..=440 is the tail of the [1.75,1.85) plateau at
    // bucket 369 (the span ends one bucket before 1.8), still above the room but below its start
    // bucket: the resume lands on 1.85 either way. Reach decides WHICH quiet is searched, not whether
    // any cut moves — with no reachable quiet below the room, nothing budges.
    let far = edges(8.0, &[(1.5, 1.55, DIP), (1.75, 1.85, 170), (2.2, 2.7, WORD)]);
    assert_eq!(far.start_before(&r, 1.0), 1.85, "bucket 369 (plateau tail) is the only quiet found inside the reach");

    // Now remove every reachable quiet. The window is still 360..=440, but the scan runs from the floor
    // upward and stops at the FIRST bucket strictly below its start (bucket 440, room level 8) — which
    // is 360 itself, plain room with nothing above it. So the cut lands on 1.8, exactly one reach back:
    // an unreachable dip buys nothing, and the search's own first step is all that is left.
    let quiet = edges(8.0, &[(1.5, 1.55, DIP), (2.2, 2.7, WORD)]);
    assert_eq!(quiet.start_before(&r, 1.0), 1.8, "no reachable quiet: the cut sits at the reach's own floor");
    assert_ne!(back.start_before(&r, 1.0), quiet.start_before(&r, 1.0), "a reachable dip moves the cut; an unreachable one does not");}

/// S5: the envelope-only side has no trough search, and this reach is not any of its neighbours.
#[test]
fn p_eng_troughreachseconds_s5_there_is_no_trough_search_on_the_envelope_only_side_and_this_is_not_its_neighbours() {
    // The s2 envelope: the fenced side settles on the trough at 1.85...
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.6, 170), (1.85, 1.95, DIP)]);
    let w = word("stays", 1.0, 1.5);
    assert_eq!(e.end_after(&w, 4.0), 1.85);
    // ...while `end_before`, asked about the same envelope by the envelope-only rule, leaves this stamp
    // where it is: at 2.5 s there is no sound run in reach to step out of, so the answer stays 2.5 and
    // never settles on 1.85, the quietest bucket the fenced search picked. The fenced side chooses the
    // QUIETEST place inside a word's fence; the envelope-only side follows the end of a sound run.
    assert_ne!(e.end_before(2.5), e.end_after(&w, 4.0), "two rules, two thresholds, two answers");
    assert_ne!(e.end_before(2.5), 1.85, "no trough search: the stamp is not pulled onto the quiet bucket");

    // Each neighbour answers a different question.
    // P.eng.edgeTailMaxSeconds (0.25): the bound immediately before this one — tail, then gap.
    assert_eq!(edge::EDGE_TAIL_MAX, 0.25);
    // P.eng.edgeTailDB (12 dB): the level that ends the tail and hands over to this search.
    assert_eq!(edge::EDGE_TAIL_DB, 12.0);
    // P.eng.edgePadSeconds (0.05): what is left of a sound once the edge has been placed.
    assert_eq!(edge::EDGE_PAD, 0.05);
    // P.eng.wordPadSeconds (0.08): the room a cut leaves a word when there is no envelope to ask.
    assert_eq!(edge::WORD_PAD, 0.08);
    // P.eng.edgeReachSeconds (0.8): how far an edge may move off its stamp at all.
    assert_eq!(edge::EDGE_REACH, 0.8);
    // P.eng.lateStampSeconds (0.6): how late a stamp may trail its sound.
    assert_eq!(edge::LATE_STAMP, 0.6);
    // P.eng.envelopeWindowSeconds (4.0): the stretch the room itself is measured over.
    assert_eq!(edge::ENVELOPE_WINDOW, 4.0);

    assert_ne!(edge::TROUGH_REACH, edge::EDGE_TAIL_MAX, "the search behind the tail is not the follow of the tail");
    assert_ne!(edge::TROUGH_REACH, edge::EDGE_TAIL_DB, "a distance is not a level");
    assert_ne!(edge::TROUGH_REACH, edge::EDGE_PAD, "a search distance is not a leftover pad");
    assert_ne!(edge::TROUGH_REACH, edge::WORD_PAD, "a search distance is not a word's room");
    assert_ne!(edge::TROUGH_REACH, edge::EDGE_REACH, "the trough search is not the edge's total reach");
    assert_ne!(edge::TROUGH_REACH, edge::LATE_STAMP, "the trough search is not the stamp-lateness bound");
    assert_ne!(edge::TROUGH_REACH, edge::ENVELOPE_WINDOW, "the trough search is not the room window");

    // The three nested windows of one fenced cut, smallest to largest.
    assert!(
        edge::EDGE_TAIL_MAX < edge::TROUGH_REACH && edge::TROUGH_REACH < edge::EDGE_REACH,
        "tail cap < trough reach < edge reach: the three nested windows of one cut"
    );
    assert!(
        edge::EDGE_TAIL_MAX + edge::TROUGH_REACH < edge::EDGE_REACH,
        "a fenced cut may travel tail cap plus reach, which stays inside the envelope-only reach"
    );

    // And this row points at the trough reach, not at any neighbour's constant.
    assert_eq!(anywhere("P.eng.troughReachSeconds").from, "edges::TROUGH_REACH");
}
