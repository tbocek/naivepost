// §10-parameters · P.eng.envelopeWindowSeconds — the stretch of a recording searched for its room
// (4 s each side; prototype 400 buckets at the 100 Hz envelope, gui/retake_edge.go:163). The rule
// already existed as the literal `4.0` inside `Edges::floor`: take the window, sort it, read the 20th
// percentile as the room, raise it to `max(3×, +8)`. That floor is the threshold every envelope-only
// placement asks against (`end_before`, `start_at`), so this window underlies all of them — and it is
// the only one of the edge bounds that decides what "silent in THIS room" means.
//
// What this file pins: the value and its single catalogue row, a room inside the window setting the
// floor, the same room outside the window being invisible to it (which is what makes the width matter
// rather than just existing), the window reaching forward as well as back, and the window clamping at
// the ends of the file instead of failing.
//
// Fixtures are lifted from `tests/late_stamp_window.rs` — same hand-written 200 Hz envelope, same
// helpers. Note the level: these fixtures use WORD = 200 (a talk level), because the floor is read off
// the loud part of the window as well as the quiet part. At 200 Hz the ±4 s window is 800 buckets each
// side; for the stamp at t = 7.0 (bucket 1400) that is buckets 600..2199, 1600 wide, so the 20th
// percentile is index 320 of the sorted window.

use naivepost::edges::{self as edge, AlignedWord, Edges};
use naivepost::params;
use naivepost::textfmt::Retake;
use naivepost::wave::Wave;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows, word, anywhere};

const HZ: f64 = 200.0;
const ROOM: u8 = 8;
/// A talk level, high above any plausible room floor, so a window full of it reads as "no room here".
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

/// One aligned word. This round asks the envelope alone (no word times), so the helper is kept for
/// shape-parity with the sibling fixture files rather than used here.
#[allow(dead_code)]
#[allow(dead_code)]
fn mark(s: f64, e: f64, again: f64, to: f64) -> Retake {
    Retake { s, e, again, to, text: String::new(), whole: String::new() }
}

/// No envelope anywhere. Kept for shape-parity; every case here supplies one, since the room only
/// exists in an envelope.
#[allow(dead_code)]
fn none(_: f64) -> Option<&'static Edges> {
    None
}

/// A row found anywhere in the catalogue, asserted NON-EMPTY: some ids are deliberately catalogued twice
/// in this tree, so a strict helper would trip on sibling ids named below. This round's own single-row
/// check is done explicitly in s1.
#[allow(dead_code)]
/// S1: `P.eng.envelopeWindowSeconds` = 4, held by `edges::ENVELOPE_WINDOW`, catalogued once under the
/// family §10 files it in.
#[test]
fn p_eng_envelopewindowseconds_s1_the_window_is_4_seconds_and_catalogued_once() {
    // P.eng.envelopeWindowSeconds — "envelope searched each side of a stamp (±400 buckets)".
    assert_eq!(edge::ENVELOPE_WINDOW, 4.0);
    // This tree caches the envelope at 200 Hz, so the same ±4 s is 800 buckets where the prototype's
    // 400 was counted at 100 Hz. The SECONDS are the constant; the bucket count follows the cache rate.
    assert_eq!((edge::ENVELOPE_WINDOW * HZ) as usize, 800);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.envelopeWindowSeconds")
        .expect("the room window must be catalogued for Prepare, whose floor reads it");
    assert_eq!(row.from, "edges::ENVELOPE_WINDOW", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "4");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 4.0);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.envelopeWindowSeconds"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.envelopeWindowSeconds").count(),
        1,
        "P.eng.envelopeWindowSeconds catalogued more than once"
    );
}

/// S2: a room inside the window sets the floor. The room band sits 0–2 s from the stamp, deep inside
/// ±4 s, so the 20th percentile lands on it.
#[test]
fn p_eng_envelopewindowseconds_s2_room_inside_the_window_sets_the_floor() {
    // P.eng.envelopeWindowSeconds: ROOM occupies [5.0, 7.0) — 400 buckets inside the window's 1600.
    let e = edges(14.0, &[(0.0, 5.0, WORD), (7.0, 14.0, WORD)]);
    assert_eq!(e.floor(7.0), 24, "room 8 raised to max(3*8, 8+8) = 24");
    assert!(e.floor(7.0) > ROOM, "the floor sits above raw room so a breath still counts as sound");
    assert!(e.floor(7.0) < WORD, "and far below the talk level, so speech is not mistaken for room");
}

/// S3: the same room pushed outside the window is not found at all, and the floor goes to the ceiling.
/// This is what makes the width the number that matters: the window does not merely prefer nearby room,
/// it cannot see beyond itself.
#[test]
fn p_eng_envelopewindowseconds_s3_room_beyond_the_window_is_not_found() {
    // P.eng.envelopeWindowSeconds: ROOM now [1.0, 3.0) ends at bucket 599; the window starts at 600.
    let out = edges(14.0, &[(0.0, 1.0, WORD), (3.0, 14.0, WORD)]);
    assert_eq!(out.floor(7.0), u8::MAX, "window all talk: max(3*200, 200+8) clamps to 255");

    // The contrast without literals: the identical room band, moved from inside the window to outside it,
    // changes the floor from 24 to the ceiling. Same recording, same stamp, different window reach.
    let inside = edges(14.0, &[(0.0, 5.0, WORD), (7.0, 14.0, WORD)]);
    assert_ne!(inside.floor(7.0), out.floor(7.0), "inside the window it is found, outside it is not");
    assert_eq!(inside.floor(7.0), 24);
}

/// S4: the window is centred on the second being placed, so it reaches 4 s forward as well as back.
/// Room ahead of the stamp counts just as room behind it does; past either edge it does not exist.
#[test]
fn p_eng_envelopewindowseconds_s4_the_window_reaches_the_far_side_too() {
    // P.eng.envelopeWindowSeconds: ROOM immediately after the stamp ([7.0, 9.0)) is inside the window.
    let near = edges(14.0, &[(0.0, 7.0, WORD), (9.0, 14.0, WORD)]);
    assert_eq!(near.floor(7.0), 24, "room ahead of the stamp is inside the forward half of the window");

    // Room starting at 11.0 is 4 s past the stamp — bucket 2200, one past the window's last bucket 2199.
    let far = edges(14.0, &[(0.0, 11.0, WORD)]);
    assert_eq!(far.floor(7.0), u8::MAX, "beyond the forward edge the room is not there to find");
    assert_ne!(near.floor(7.0), far.floor(7.0), "the forward reach is bounded like the backward one");
}

/// S5: the window clamps at the ends of the file rather than failing, and it is not any of the three
/// bounds that sit next to it on a cut edge.
#[test]
fn p_eng_envelopewindowseconds_s5_the_window_clamps_at_the_file_and_is_not_its_neighbours() {
    // P.eng.envelopeWindowSeconds: a 2 s all-room file gives a 400-bucket window, clamped from ±800 by
    // the file's own length — and still finds its room. The window is a bound, not a requirement.
    assert_eq!(edges(2.0, &[]).floor(1.0), 24);

    // A second outside the file has no envelope to measure: nothing can be sound.
    assert_eq!(edges(2.0, &[]).floor(-1.0), u8::MAX);

    // Each neighbour answers a different question.
    // P.eng.edgeReachSeconds (0.8): how far an edge may move off its stamp at all.
    assert_eq!(edge::EDGE_REACH, 0.8);
    // P.eng.lateStampSeconds (0.6): how late a stamp may trail its sound.
    assert_eq!(edge::LATE_STAMP, 0.6);
    // TROUGH_REACH (0.4): how far into a fenced gap the quietest moment is looked for.
    assert_eq!(edge::TROUGH_REACH, 0.4);

    assert_ne!(edge::ENVELOPE_WINDOW, edge::EDGE_REACH, "where the room is measured is not how far an edge moves");
    assert_ne!(edge::ENVELOPE_WINDOW, edge::LATE_STAMP, "the room window is not the stamp-lateness bound");
    assert_ne!(edge::ENVELOPE_WINDOW, edge::TROUGH_REACH, "the room window is not the in-fence trough reach");
    assert!(
        edge::ENVELOPE_WINDOW > edge::EDGE_REACH * 4.0,
        "the room window is five times the walk's reach: room needs a stretch, an edge does not"
    );

    // And this row points at the window, not at any neighbour's constant.
    assert_eq!(anywhere("P.eng.envelopeWindowSeconds").from, "edges::ENVELOPE_WINDOW");
}
