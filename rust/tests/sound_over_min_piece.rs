// §10-parameters · P.eng.soundMinPieceSeconds — under 0.05 s a piece of laid-over sound is not worth a segment
// of its own (prototype `sndMinLn`, gui/cut.go). The rule lives in `cut_copy::MIN_SOUND_PIECE_SECONDS`, read by
// `cut_copy::sound_piece_overlap`, which both the piece builder (`lay_pieces`) and the list rebuild (`lay_over`)
// ask — so the two cannot disagree about whether a stretch earns a piece.
//
// The floor has two halves and only asserting one of them would leave half the rule untested: no piece may be laid
// over a blink, AND nothing may be split when no piece was laid. A split around a piece that never arrived is
// exactly the cost this constant exists to avoid, so s3 checks the footage comes back whole.

use naivepost::cut::{Cut, Seg};
use naivepost::cut_copy as cp;
use naivepost::cut_select::Scope;
use naivepost::{cut_select, fx_lane, fx_volume, params};

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

/// The recording a lane names — same fixture name `tests/cut_copy_paste_lane.rs` uses.
const REC: &str = "2026-09-16 17-26-20";
/// The sound file each laid piece names as its asset.
const FILE: &str = "assets/audio.wav";

fn anywhere(id: &str) -> params::Param {
    let mut found = all_rows().into_iter().filter(|row| row.id == id).collect::<Vec<_>>();
    // `P.eng.minPieceSeconds` is catalogued twice by design (Prepare's §4 and the Cut page's §6, both reading
    // `cut_select::MIN_SECONDS`), so this helper takes the first of them instead of insisting on one. The
    // single-row check for THIS round's own id is done in s1 against `params::cut()`.
    assert!(!found.is_empty(), "{id} catalogued nowhere");
    found.pop().unwrap()
}

/// A number §10 writes with a decimal point compared as a number: `params::num` trims the trailing zero, so
/// comparing strings would pin a spelling neither side chose.
fn number_anywhere(id: &str) -> f64 {
    anywhere(id)
        .spelled
        .parse()
        .unwrap_or_else(|_| panic!("{id} spells a number"))
}

/// A second compared as a second, not as a bit pattern: 0.05 is not representable in binary, so an overlap
/// clipped to the floor lands a ulp either side of it.
fn assert_close(what: &str, got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "{what}: got {got}, want {want} (±1e-9)");
}

/// A cut keeping the given spans of footage, as `tests/cut_copy_paste_lane.rs::kept` builds one.
fn kept(spans: &[(f64, f64)]) -> Cut {
    let mut cut = Cut::default();
    for (s, e) in spans {
        cut.segs.push(Seg { s: *s, e: *e, cam: 0, ..Default::default() });
    }
    cut
}

/// A hand holding `length` seconds of the recording's sound, starting at session second `from`.
fn sound_hand(from: f64, length: f64) -> cp::Hand {
    cp::Hand { from, length, scope: Scope::Sound { recording: REC.to_string() } }
}

/// The seconds of every piece of footage the cut keeps, sorted — how s3 checks the picture was left alone.
fn film(cut: &Cut) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> =
        cut.segs.iter().filter(|seg| seg.ins.is_empty()).map(|seg| (seg.s, seg.e)).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    spans
}

/// S1: `P.eng.soundMinPieceSeconds` = 0.05, held by `cut_copy::MIN_SOUND_PIECE_SECONDS`, catalogued once under
/// the family §10 files it in.
#[test]
fn p_eng_soundminpieceseconds_s1_the_floor_is_0_05_and_catalogued_once() {
    // P.eng.soundMinPieceSeconds — "under this a sound-over piece is not worth a segment of its own".
    assert_eq!(cp::MIN_SOUND_PIECE_SECONDS, 0.05);
    assert_eq!(number_anywhere("P.eng.soundMinPieceSeconds"), 0.05);

    // The row names the constant its rule reads, not a copy of the number.
    assert_eq!(anywhere("P.eng.soundMinPieceSeconds").from, "cut_copy::MIN_SOUND_PIECE_SECONDS");

    // §10 §5.1 files it among the engineering constants, so the prefix must answer to Eng.
    assert_eq!(params::family("P.eng.soundMinPieceSeconds"), params::Family::Eng);

    // And the row sits on the Cut page's list, where F2.9's paste rules live.
    assert!(
        params::cut().iter().any(|row| row.id == "P.eng.soundMinPieceSeconds"),
        "the sound-over floor belongs to §05-cut#6's list"
    );
}

/// S2: a stretch over the floor DOES get a piece, and the lay reports it. This is the case the floor leaves
/// untouched — proving the rule is a floor and not a refusal to lay anything at all.
#[test]
fn p_eng_soundminpieceseconds_s2_a_stretch_over_the_floor_gets_its_piece() {
    let cut = kept(&[(0.0, 10.0)]);
    let footage = &cut.segs[0];

    // P.eng.soundMinPieceSeconds: six seconds under the span is a hundred times the floor.
    assert_eq!(cp::sound_piece_overlap(footage, 2.0, 8.0), Some((2.0, 8.0)));

    let hand = sound_hand(2.0, 6.0);
    let pieces = cp::lay_pieces(&cut, FILE, &hand, 2.0, 1.0);
    assert_eq!(pieces.len(), 1, "one kept stretch, one piece");
    assert_eq!((pieces[0].s, pieces[0].e), (2.0, 8.0));
    assert_eq!(pieces[0].ins, FILE, "the piece names the sound file it plays");
    // `ss` walks with the session second (another item's rule, asserted here only to show the floor did not
    // disturb it): the file second handed in, plus nothing — at `at == 2.0` and a piece starting at 2.0 the
    // piece is at the very start of the span, so it resumes where the caller said the span begins.
    assert_close("file second the piece starts at", pieces[0].ss, 1.0);

    // And across a hole the walk really does carry: laying over 8–30 with footage kept at 0–10 and 25–30 puts
    // the second piece far into the file even though only five of its seconds played. `ss` walks with the
    // SESSION second, hole and all — six unplayed seconds still count as read past.
    let holed = kept(&[(0.0, 10.0), (25.0, 30.0)]);
    let walked = cp::lay_pieces(&holed, FILE, &sound_hand(8.0, 22.0), 8.0, 4.0);
    assert_eq!(walked.len(), 2);
    assert_close("first piece resumes at the span's file second", walked[0].ss, 4.0);
    assert_close("second piece resumes mid-file", walked[1].ss, 21.0);

    // The laid-over cut keeps the picture whole outside the pieces.
    let mut cut = kept(&[(0.0, 10.0)]);
    let hand = sound_hand(2.0, 6.0);
    assert_eq!(cp::lay_over(&mut cut, FILE, &hand, 2.0, 1.0), 1, "one piece laid");
    assert_eq!(film(&cut), vec![(0.0, 2.0), (8.0, 10.0)]);
}

/// S3: under the floor there is no piece AND no split. Both halves matter: a lay that skipped the piece but still
//  cut the clip would leave the footage in two pieces with nothing heard over the join — the exact cost the
/// constant exists to avoid.
#[test]
fn p_eng_soundminpieceseconds_s3_under_the_floor_lays_nothing_and_splits_nothing() {
    let mut cut = kept(&[(0.0, 10.0)]);
    let footage = &cut.segs[0];

    // P.eng.soundMinPieceSeconds: the span [9.98, 12.0) overlaps the kept 0–10 by 0.02 s, well under 0.05.
    assert_eq!(cp::sound_piece_overlap(footage, 9.98, 12.0), None, "0.02 s is a blink");
    let hand = sound_hand(9.98, 2.0);
    assert!(cp::lay_pieces(&cut, FILE, &hand, 9.98, 5.0).is_empty(), "no piece is built");

    // The lay reports nothing laid, and — the load-bearing part — the footage comes back WHOLE.
    assert_eq!(cp::lay_over(&mut cut, FILE, &hand, 9.98, 5.0), 0);
    assert_eq!(cut.segs.len(), 1, "not split into 0–9.98 and 9.98–10");
    assert_eq!(film(&cut), vec![(0.0, 10.0)], "the picture is exactly what it was");
    assert!(cut.segs.iter().all(|seg| seg.ins.is_empty()), "no stray sound segment was added");
}

/// S4: inserts are stepped over whatever their length, because a card covers no footage of its own — the
/// prototype's other half of the same condition (`if f.isInsert() || t1-t0 < sndMinLn`).
#[test]
fn p_eng_soundminpieceseconds_s4_an_insert_is_stepped_over_at_any_length() {
    // A card lying OVER a wide stretch: far more than the floor of overlap, and still no piece.
    let card = Seg { s: 4.0, e: 20.0, ins: "assets/card.png".to_string(), ..Default::default() };
    assert!(card.is_overwrite_insert(), "the fixture is an over-the-footage card");
    assert_eq!(
        cp::sound_piece_overlap(&card, 0.0, 30.0),
        None,
        "an insert covers no footage, so a sound cannot lie over it"
    );

    // A spliced insert (s == e, its length in `dur`) likewise: it takes no session seconds to cover.
    let spliced = Seg { s: 6.0, e: 6.0, ins: "copy:12.000".to_string(), dur: 8.0, ..Default::default() };
    assert!(spliced.is_insert(), "the fixture is a spliced copy");
    assert_eq!(cp::sound_piece_overlap(&spliced, 0.0, 30.0), None);

    // A cut holding both kinds of insert plus kept footage gets pieces only over the footage.
    let mut cut = kept(&[(0.0, 5.0), (25.0, 30.0)]);
    cut.segs.push(card.clone());
    cut.segs.push(spliced.clone());
    let hand = sound_hand(0.0, 30.0);
    let pieces = cp::lay_pieces(&cut, FILE, &hand, 0.0, 0.0);
    // Only the two filmed stretches carry a piece; the card and the spliced copy are stepped over at any
    // overlap size. The inserts are identified by what they name rather than by counting: each kept stretch is
    // asked directly, which is the question `lay_pieces` filters on.
    let asked: Vec<(f64, f64)> = cut
        .segs
        .iter()
        .filter(|seg| seg.ins.is_empty())
        .filter_map(|seg| cp::sound_piece_overlap(seg, 0.0, 30.0))
        .collect();
    assert_eq!(asked, vec![(0.0, 5.0), (25.0, 30.0)], "only the filmed stretches offer a piece");
    assert_eq!(pieces.len(), 2, "and that is what got laid");
    assert_eq!(cp::lay_over(&mut cut, FILE, &hand, 0.0, 0.0), 2);
    // The laid pieces stand over the two filmed stretches and nowhere else: the card's sixteen seconds and the
    // spliced copy's point got no piece even though the span covered them whole. (A laid piece is told from an
    // insert by its `lane` — a sound-over piece names the recording it replaces; an insert names its own asset
    // and no lane.)
    let laid_spans: Vec<(f64, f64)> = cut
        .segs
        .iter()
        .filter(|seg| !seg.lane.is_empty())
        .map(|seg| (seg.s, seg.e))
        .collect();
    assert_eq!(laid_spans, vec![(0.0, 5.0), (25.0, 30.0)], "pieces only where there was picture");
    // Both inserts are still in the cut, unchanged.
    assert_eq!(
        cut.segs.iter().filter(|seg| seg.lane.is_empty() && !seg.ins.is_empty()).count(),
        2,
        "the card and the spliced copy both survive as they were"
    );
}

/// S5: the boundary is "not below the floor", and this bound is not any of the three look-alike floors that also
/// sit under a length somewhere in the app.
#[test]
fn p_eng_soundminpieceseconds_s5_the_boundary_and_three_look_alike_floors() {
    // Exactly the floor is KEPT: an overlap clipped to 0.05 by float arithmetic must not be thrown away as a
    // blink, which is why the test is "not below" rather than "strictly above".
    let clip = kept(&[(0.0, 10.0)]);
    let footage = &clip.segs[0];
    assert_eq!(cp::sound_piece_overlap(footage, 0.0, 0.05), Some((0.0, 0.05)));
    // A ulp under the floor still counts, for the same reason.
    assert!(
        cp::sound_piece_overlap(footage, 0.0, 0.05f64.next_down()).is_some(),
        "a ulp under the floor is the same blink-free piece"
    );
    // Clearly under: a thousandth short of the floor, far beyond any float slack.
    assert_eq!(cp::sound_piece_overlap(footage, 0.0, 0.0499), None);

    // P.eng.minPieceSeconds (cut_select::MIN_SECONDS, 0.04): is a remainder left by a REMOVAL worth keeping.
    // One twentieth of a second under this floor, and a different question entirely. It is catalogued twice in
    // this tree — once for Prepare's §4 and once for the Cut page's §6, both reading `cut_select::MIN_SECONDS`
    // — so it is checked through `params::find` rather than the single-match helper above.
    assert_eq!(cut_select::MIN_SECONDS, 0.04);
    assert_ne!(
        cp::MIN_SOUND_PIECE_SECONDS, cut_select::MIN_SECONDS,
        "worth-keeping-after-a-removal is not worth-a-piece-of-laid-sound"
    );
    let piece_floor: f64 =
        params::find("P.eng.minPieceSeconds").expect("the removal floor is catalogued").spelled.parse().unwrap();
    assert_eq!(piece_floor, cut_select::MIN_SECONDS);

    // P.eng.effectMinSeconds (fx_lane::MIN_BAND_SECONDS, 0.1): how short an effect band may be DRAGGED.
    assert_eq!(fx_lane::MIN_BAND_SECONDS, 0.1);
    assert_ne!(
        cp::MIN_SOUND_PIECE_SECONDS, fx_lane::MIN_BAND_SECONDS,
        "the drag floor is not the sound-piece floor"
    );
    assert_eq!(number_anywhere("P.eng.effectMinSeconds"), fx_lane::MIN_BAND_SECONDS);

    // effects.volumeMinSeconds (fx_volume::MIN_SECONDS, 0.1): the volume FORM's typed Length floor.
    assert_eq!(fx_volume::MIN_SECONDS, 0.1);
    assert_ne!(
        cp::MIN_SOUND_PIECE_SECONDS, fx_volume::MIN_SECONDS,
        "a form's typed floor is not a laying floor"
    );
    assert_eq!(number_anywhere("effects.volumeMinSeconds"), fx_volume::MIN_SECONDS);

    // Four floors, four rows, none of them borrowing another's constant.
    let sources = [
        anywhere("P.eng.soundMinPieceSeconds").from,
        anywhere("P.eng.minPieceSeconds").from,
        anywhere("P.eng.effectMinSeconds").from,
        anywhere("effects.volumeMinSeconds").from,
    ];
    for (i, a) in sources.iter().enumerate() {
        for (j, b) in sources.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "two of these floors share a constant: {a} / {b}");
            }
        }
    }
}
