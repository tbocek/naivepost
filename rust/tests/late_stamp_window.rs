// §10-parameters · P.eng.lateStampSeconds — how late a stamp may be behind its sound (0.6 s;
// prototype `lateStamp`, gui/retake_edge.go:38). The rule already existed as `edges::LATE_STAMP`:
// `Edges::start_at` turns it into a bucket count (`(LATE_STAMP * hz) as i64`) and uses it ONLY when
// the stamp lands on quiet, to walk back over that quiet to the sound run before it. Quiet longer
// than the window is not a late stamp's tail — moving forward to the next sound would clip the very
// word the stamp names — so the stamp stands. `end_before` never consults it, and with aligned word
// times `place_edges` fences the edge instead of asking the envelope at all (that is what
// "envelope-only" means).
//
// What this file pins: the value and its single catalogue row, a resume that opens inside the window,
// a wider run whose SOUND-walk goes further back than the window itself (the window bounds the quiet
// crossing only — the reach bounds the walk), the window stopping a stamp that the reach alone would
// have allowed through, and that the window is asked only when the stamp lands on quiet.
//
// Fixtures are lifted from `tests/edge_reach_limit.rs` — same hand-written 200 Hz envelope, same
// helpers — so nothing here invents a second way to build an edge. Expectations are the exact bucket
// boundaries `start_at` answers on (`off + j/hz - EDGE_PAD`), not round literals: at 200 Hz the
// window is `(0.6 * 200) as i64 = 120` buckets and the loops allow `i - j <= window`.

use naivepost::edges::{self as edge, AlignedWord, Edges};
use naivepost::params;
use naivepost::textfmt::Retake;
use naivepost::wave::Wave;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

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
/// shape-parity with `tests/edge_reach_limit.rs` rather than used here.
#[allow(dead_code)]
fn word(text: &str, s: f64, e: f64) -> AlignedWord {
    AlignedWord { word: text.into(), s, e }
}

fn mark(s: f64, e: f64, again: f64, to: f64) -> Retake {
    Retake { s, e, again, to, text: String::new(), whole: String::new() }
}

/// No envelope anywhere. Kept for shape-parity with the sibling fixture files; every case here
/// supplies one, because the late-stamp window only exists in the envelope-only placement.
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

/// S1: `P.eng.lateStampSeconds` = 0.6, held by `edges::LATE_STAMP`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_latestampseconds_s1_the_window_is_0_6_and_catalogued_once() {
    // P.eng.lateStampSeconds — "envelope-only edge placement: how late a stamp may be".
    assert_eq!(edge::LATE_STAMP, 0.6);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.lateStampSeconds")
        .expect("the late-stamp window must be catalogued for Prepare, whose edge placement reads it");
    assert_eq!(row.from, "edges::LATE_STAMP", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "0.6");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.6);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.lateStampSeconds"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.lateStampSeconds").count(),
        1,
        "P.eng.lateStampSeconds catalogued more than once"
    );
}

/// S2: a stamp just after a sound run is that run's late word. The quiet between them is well inside
/// the window, so the walk crosses it, finds the run, and resumes at its onset less the pad.
#[test]
fn p_eng_latestampseconds_s2_a_stamp_inside_the_window_resumes_at_the_sound_that_runs_late() {
    // P.eng.lateStampSeconds: 0.1 s of quiet < 0.6 s, so the stamp still belongs to the run behind it.
    let e = edges(8.0, &[(4.6, 4.9, WORD)]);
    assert_eq!(e.start_at(5.0), 4.545, "at the run's onset, less the post-envelope pad");
    assert!(e.start_at(5.0) < 5.0, "inside the window the envelope answers, so the resume moves earlier");
    assert!(e.start_at(5.0) < 4.6, "and it opens onto the sound, not merely off the stamp");
}

/// S3: the window bounds the QUIET crossing, not the walk over the sound. A run that ends 0.5 s before
/// the stamp is inside the window, and the walk then travels back over the whole run to its onset —
/// further back than the window itself measures.
#[test]
fn p_eng_latestampseconds_s3_a_wider_short_run_still_lands_inside_the_window() {
    // P.eng.lateStampSeconds: quiet 0.5 s ≤ the 120-bucket window, so the crossing is allowed.
    let e = edges(8.0, &[(4.3, 4.5, WORD)]);
    assert_eq!(e.start_at(5.0), 4.25, "the resume lands on the run's onset, less the pad");
    // The walk back is 140 buckets — more than the 120-bucket window — yet legal, because how far the
    // SOUND may be followed is the reach's business (P.eng.edgeReachSeconds), not the window's.
    assert_eq!((5.0 - 4.25) * HZ, 150.0, "total travel exceeds the window");
    assert!((5.0 - 4.25) > edge::LATE_STAMP, "the answer lies further back than the window itself");
    assert!((5.0 - 4.25) < edge::EDGE_REACH, "but stays inside the reach");
}

/// S4: quiet longer than the window leaves the stamp alone. Chasing the next sound would clip the word
/// the stamp names, so the walk gives up — and it gives up on the WINDOW, not on the reach.
#[test]
fn p_eng_latestampseconds_s4_quiet_longer_than_the_window_leaves_the_stamp_alone() {
    // P.eng.lateStampSeconds: 0.7 s of quiet > 0.6 s, so the stamp stands.
    let e = edges(8.0, &[(3.7, 4.3, WORD)]);
    assert_eq!(e.start_at(5.0), 5.0, "too late to be that run's word: the stamp holds");

    // The tighter contrast that proves which bound stopped it: a 0.65 s gap costs 130 buckets against a
    // 120-bucket window, while that envelope's onset (4.3) is only 140 buckets back and so sits well
    // inside the 160-bucket reach (P.eng.edgeReachSeconds). Reach alone would have moved this edge;
    // the late window is the binding constraint.
    let tight = edges(8.0, &[(4.3, 4.35, WORD)]);
    assert_eq!(tight.start_at(5.0), 5.0, "out of the late window, though inside the reach");
    assert!(edge::LATE_STAMP < edge::EDGE_REACH, "0.6 < 0.8: the window is the tighter of the two");

    // Through the flow too: no word times, so the envelope alone is asked. Out of the window it hands the
    // stamp back unchanged, but `place_edges` has already opened the gap to that second, so the resume
    // still moves from 4.0 up to the stamp — the note says "where the retake starts to sound", which is
    // what the envelope was asked even when its answer was a refusal to move.
    let (marks, notes) = edge::place_edges(vec![mark(1.0, 4.6, 5.0, 4.0)], &[], |_| Some(&e), &[]);
    assert_eq!(marks[0].to, 5.0, "out of the window the envelope's answer is the stamp itself");
    assert!(marks[0].to <= 5.0 && marks[0].to >= 4.6, "the resume stays inside the gap up to the stamp");
    assert_eq!(notes.len(), 1, "the opened gap is reported once");
}

/// S5: the window is asked only when the stamp lands on quiet, and only on the resume side.
#[test]
fn p_eng_latestampseconds_s5_the_window_is_asked_only_when_the_stamp_lands_on_quiet() {
    // A stamp ON sound skips the quiet-crossing entirely: no window consulted, straight to the onset.
    let on = edges(8.0, &[(4.5, 5.5, WORD)]);
    assert_eq!(on.start_at(5.0), 4.45, "onset less the pad, with no late-stamp walk in between");

    // And the cut-END side never asks the window at all: a stamp inside a sound run has no quiet to step
    // back into, so `end_before` returns the stamp unchanged whatever LATE_STAMP says.
    assert_eq!(on.end_before(5.0), 5.0, "end_before walks by its own reach/quiet rule, not by lateness");

    // Each neighbour is a separate constant answering a separate question.
    // P.eng.edgePadSeconds (0.05): what is left of a sound once the envelope has placed it.
    assert_eq!(edge::EDGE_PAD, 0.05);
    // P.eng.wordPadSeconds (0.08): the room a cut leaves a word when there is no envelope to ask.
    assert_eq!(edge::WORD_PAD, 0.08);
    // P.eng.edgeReachSeconds (0.8): how far an edge may move off its stamp at all.
    assert_eq!(edge::EDGE_REACH, 0.8);
    // TROUGH_REACH (0.4): how far into a fenced gap the quietest moment is looked for — a search bound
    // inside a fence the words set, so not this item.
    assert_eq!(edge::TROUGH_REACH, 0.4);

    assert_ne!(edge::LATE_STAMP, edge::EDGE_PAD, "how late a stamp may be is not what to leave behind");
    assert_ne!(edge::LATE_STAMP, edge::WORD_PAD, "a stamp's lateness bound is not a word's room");
    assert_ne!(edge::LATE_STAMP, edge::EDGE_REACH, "the quiet a stamp may trail is not the edge's reach");
    assert_ne!(edge::LATE_STAMP, edge::TROUGH_REACH, "the stamp window is not the in-fence trough reach");

    // And this row points at the window, not at any neighbour's constant.
    assert_eq!(anywhere("P.eng.lateStampSeconds").from, "edges::LATE_STAMP");
}
