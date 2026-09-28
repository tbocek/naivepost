// §10-parameters · P.eng.edgeTailMaxSeconds — the longest a word's tail is followed (0.25 s;
// prototype `edgeTailMax`, gui/retake_edge.go:51). The rule already existed as `edges::EDGE_TAIL_MAX`:
// in `end_after` the walk over the word's own sound stops at `i1 + 50 buckets`, and in `start_before`
// the walk back stops at `i0 - 50`. It pairs with `P.eng.edgeTailDB` — twelve decibels decides WHICH
// sound is still the word, this decides how long to keep chasing it — and it belongs to the fenced side
// only: the envelope-only methods bound their walk by `EDGE_REACH` and never consult it.
//
// What this file pins: the value and its single catalogue row; the cap cutting a follow that would
// otherwise run on for a second and a half; the cap being a ceiling rather than a target, so a short
// tail is answered short; the same cap holding the resume side; and the envelope-only side travelling
// further than the cap because it asks a different question.
//
// Fixtures are lifted from `tests/edge_tail_threshold.rs` — same hand-written 200 Hz envelope,
// ROOM = 8, WORD = 200. At this rate the cap is 50 buckets and `TROUGH_REACH` is 80, and every
// fixture here has a mostly-ROOM ±4 s window, so the room floor is 24 and `tail_level` is peak − 44.

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
/// supplies one, since a tail needs sound to chase.
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

/// S1: `P.eng.edgeTailMaxSeconds` = 0.25, held by `edges::EDGE_TAIL_MAX`, catalogued once under the
/// family §10 files it in.
#[test]
fn p_eng_edgetailmaxseconds_s1_the_cap_is_0_25_and_catalogued_once() {
    // P.eng.edgeTailMaxSeconds — "word-fenced edge placement: longest tail".
    assert_eq!(edge::EDGE_TAIL_MAX, 0.25);
    // The SECONDS are the constant; the bucket count follows whatever rate this tree caches at, here
    // 200 Hz, so the follow window is fifty buckets.
    assert_eq!((edge::EDGE_TAIL_MAX * HZ) as i64, 50);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.edgeTailMaxSeconds")
        .expect("the tail cap must be catalogued for Prepare, whose fenced placement reads it");
    assert_eq!(row.from, "edges::EDGE_TAIL_MAX", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "0.25");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.25);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.edgeTailMaxSeconds"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.edgeTailMaxSeconds").count(),
        1,
        "P.eng.edgeTailMaxSeconds catalogued more than once"
    );
}

/// S2: the cap stops the follow while the sound is still going. Without it the cut would run a second
/// and a half past the aligner's end, into what the fence says is not the word.
#[test]
fn p_eng_edgetailmaxseconds_s2_the_cap_stops_the_follow_when_the_sound_kept_going() {
    // P.eng.edgeTailMaxSeconds: the word peaks at 200 so the tail threshold is 156, and the 170
    // plateau clears it for a whole 1.5 s — far longer than the cap allows.
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 3.0, 170)]);
    let w = word("stays", 1.0, 1.5);
    // Sound runs on to bucket 600; the walk is bounded at 300 + 50 = 350, and the trough scan from
    // there finds nothing quieter than 170, so the answer is 350/200.
    assert_eq!(e.end_after(&w, 4.0), 1.75, "the follow stops at the cap, not where the sound stops");
    assert_eq!(e.end_after(&w, 4.0), w.e + edge::EDGE_TAIL_MAX, "exactly the cap past the aligner's end");
    assert!(e.end_after(&w, 4.0) < 3.0, "the cap cuts the tail off long before the sound does");
}

/// S3: the cap is a ceiling, not a target. A tail that ends early is answered early — the cap never
/// extends a cut toward itself.
#[test]
fn p_eng_edgetailmaxseconds_s3_the_cap_is_a_ceiling_not_a_target() {
    // P.eng.edgeTailMaxSeconds: the plateau lasts only 0.1 s, so the walk stops at bucket 320 well
    // inside the fifty-bucket window, and the trough after it finds nothing quieter.
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 1.6, 170)]);
    let w = word("stays", 1.0, 1.5);
    assert_eq!(e.end_after(&w, 4.0), 1.6, "a short tail lands where it stops, well short of the cap");
    assert!(e.end_after(&w, 4.0) < w.e + edge::EDGE_TAIL_MAX, "nowhere near the ceiling");

    // Against the long tail: 1.6 vs 1.75. The cap only ever truncates a follow, it never lengthens one.
    let long = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 3.0, 170)]);
    assert!(e.end_after(&w, 4.0) < long.end_after(&w, 4.0), "1.6 < 1.75: the cap truncates, never extends");
}

/// S4: the same cap bounds the resume side, walking back over the retake's leading sound.
#[test]
fn p_eng_edgetailmaxseconds_s4_the_same_cap_bounds_the_resume_side() {
    // P.eng.edgeTailMaxSeconds: the plateau at 170 sits above the 156 threshold for a full second
    // ahead of the word, but `tail_start` = 400 − 50 = 350 holds the walk there.
    let e = edges(8.0, &[(1.0, 2.0, 170), (2.0, 2.5, WORD)]);
    let w = word("resumes", 2.0, 2.5);
    // The trough scan back over 270..350 finds nothing quieter than the plateau, so 350/200 stands.
    assert_eq!(e.start_before(&w, 1.0), 1.75, "the walk back stops at the cap: 2.0 - 0.25");
    assert_eq!(e.start_before(&w, 1.0), w.s - edge::EDGE_TAIL_MAX, "exactly the cap before the word starts");
    assert!(e.start_before(&w, 1.0) > 1.0, "and nowhere near the fence the words set");
}

/// S5: the cap is fenced-side only, and it is not any of the bounds that sit next to it.
#[test]
fn p_eng_edgetailmaxseconds_s5_the_cap_is_fenced_only_and_is_not_its_neighbours() {
    // The s2 envelope again: the fenced side is capped at 1.75...
    let e = edges(8.0, &[(1.0, 1.5, WORD), (1.5, 3.0, 170)]);
    let w = word("stays", 1.0, 1.5);
    assert_eq!(e.end_after(&w, 4.0), 1.75);
    // ...while `end_before`, asked about the same envelope by the envelope-only rule, steps back to the
    // end of the sound run and adds the pad: from bucket 700 over the room to 599, then 600/200 + 0.05.
    // That travel (0.45 s) is LONGER than the tail cap, which the fenced side could never manage — the
    // fenced side asks "how long is this word's tail" (bounded here), the envelope-only side asks
    // "where did the sound before me end" (bounded by EDGE_REACH instead).
    assert!((e.end_before(3.5) - 3.05).abs() < 1e-9, "end_before ignores the tail cap: the pad lands after the run");
    assert!(3.5 - e.end_before(3.5) > edge::EDGE_TAIL_MAX, "it travels further than the tail cap allows");

    // Each neighbour answers a different question.
    // P.eng.edgeTailDB (12 dB): the paired LEVEL bound — which sound is still the word. This one is the
    // LENGTH bound in seconds; the two together stop a cut from eating a breath.
    assert_eq!(edge::EDGE_TAIL_DB, 12.0);
    // TROUGH_REACH (0.4): how far the quietest moment is searched AFTER the tail stops — deliberately
    // longer than the tail itself, so the cut can settle into the gap behind it.
    assert_eq!(edge::TROUGH_REACH, 0.4);
    // P.eng.edgePadSeconds (0.05): what is left of a sound once the envelope has placed it.
    assert_eq!(edge::EDGE_PAD, 0.05);
    // P.eng.wordPadSeconds (0.08): the room a cut leaves a word when there is no envelope to ask.
    assert_eq!(edge::WORD_PAD, 0.08);
    // P.eng.edgeReachSeconds (0.8): how far an edge may move off its stamp at all.
    assert_eq!(edge::EDGE_REACH, 0.8);
    // P.eng.lateStampSeconds (0.6): how late a stamp may trail its sound.
    assert_eq!(edge::LATE_STAMP, 0.6);
    // P.eng.envelopeWindowSeconds (4.0): the stretch the room itself is measured over.
    assert_eq!(edge::ENVELOPE_WINDOW, 4.0);

    assert_ne!(edge::EDGE_TAIL_MAX, edge::EDGE_TAIL_DB, "how long to chase the tail is not how deep it may be");
    assert_ne!(edge::EDGE_TAIL_MAX, edge::TROUGH_REACH, "the follow window is not the search window after it");
    assert_ne!(edge::EDGE_TAIL_MAX, edge::EDGE_PAD, "a follow bound is not a leftover pad");
    assert_ne!(edge::EDGE_TAIL_MAX, edge::WORD_PAD, "a follow bound is not a word's room");
    assert_ne!(edge::EDGE_TAIL_MAX, edge::EDGE_REACH, "the tail cap is not the edge's reach");
    assert_ne!(edge::EDGE_TAIL_MAX, edge::LATE_STAMP, "the tail cap is not the stamp-lateness bound");
    assert_ne!(edge::EDGE_TAIL_MAX, edge::ENVELOPE_WINDOW, "the tail cap is not the room window");
    assert!(edge::EDGE_TAIL_MAX < edge::TROUGH_REACH, "the tail is followed for less time than the trough is searched after it");

    // And this row points at the cap, not at any neighbour's constant.
    assert_eq!(anywhere("P.eng.edgeTailMaxSeconds").from, "edges::EDGE_TAIL_MAX");
}
