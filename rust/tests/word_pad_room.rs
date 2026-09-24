// §10-parameters · P.eng.wordPadSeconds — the room a cut leaves a word (0.08; prototype `wordPad`,
// gui/retake.go). The rule already existed as `edges::WORD_PAD`, applied by `Edges::end_after` and
// `Edges::start_before` when there is no envelope to ask, and by the retake-mark path in `place_edges`.
// What this file pins is that the §10 row points at that constant, that the pad opens on the CORRECT side of
// the word in each direction, and that it is a room rather than a minimum: a fence or a limit inside the pad
// wins over the pad every time.
//
// Fixtures are lifted from `tests/edge_placement.rs` — same hand-written 200 Hz envelope, same `word` /
// `mark` / `none` helpers — so nothing here invents a second way to build an edge.

use naivepost::cut_select;
use naivepost::edges::{self as edge, AlignedWord, Edges};
use naivepost::params;
use naivepost::textfmt::Retake;
use naivepost::wave::Wave;

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

fn word(text: &str, s: f64, e: f64) -> AlignedWord {
    AlignedWord { word: text.into(), s, e }
}

fn mark(s: f64, e: f64, again: f64, to: f64) -> Retake {
    Retake { s, e, again, to, text: String::new(), whole: String::new() }
}

/// No envelope anywhere, and nothing else spoken.
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

/// A row found anywhere in the catalogue. Asserted NON-EMPTY rather than exactly-one: some ids are
/// deliberately catalogued twice in this tree (`P.eng.minPieceSeconds` has a Prepare row and a Cut-page
/// row), so a strict helper would trip on sibling ids named below. This round's own single-row check is
/// done explicitly in s1.
#[allow(dead_code)]
fn anywhere(id: &str) -> params::Param {
    let found = all_rows().into_iter().filter(|row| row.id == id).collect::<Vec<_>>();
    assert!(!found.is_empty(), "{id} catalogued nowhere");
    found.into_iter().next().unwrap()
}

/// S1: `P.eng.wordPadSeconds` = 0.08, held by `edges::WORD_PAD`, catalogued once under the family §10
/// files it in.
#[test]
fn p_eng_wordpadseconds_s1_the_pad_is_0_08_and_catalogued_once() {
    // P.eng.wordPadSeconds — "room a cut leaves a word".
    assert_eq!(edge::WORD_PAD, 0.08);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.wordPadSeconds")
        .expect("the pad must be catalogued for Prepare, whose edge placement reads it");
    assert_eq!(row.from, "edges::WORD_PAD", "a row must name the constant its rule reads");
    assert_eq!(row.spelled, "0.08");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.08);

    // §10 §5.1 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(params::family("P.eng.wordPadSeconds"), params::Family::Eng);

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.wordPadSeconds").count(),
        1,
        "P.eng.wordPadSeconds catalogued more than once"
    );
}

/// S2: the pad opens the cut's START past the last word that stays, so the consonant is not shaved. With no
/// envelope there is nothing else to ask, so the pad is the whole answer.
#[test]
fn p_eng_wordpadseconds_s2_the_start_opens_past_the_word_that_stays() {
    // P.eng.wordPadSeconds: a word ending at 1.5 leaves 0.08 of room, so the cut starts at 1.58.
    let w = word("stays", 1.0, 1.5);
    let (marks, _) = edge::place_edges(vec![mark(3.0, 4.0, 0.0, 4.0)], &[w.clone()], none, &[]);
    assert_eq!(marks[0].s, 1.58, "a pad after the last word that stays");

    // Asked directly, the same room on the far side of the word — but only where there is a FENCE to open.
    // `end_after` pads past `w.e` when the limit leaves it width; with a silent envelope and no next word it
    // follows the sound instead, so the pad shows up via `place_edges`, which is the rule's real caller.
    let e = edges(8.0, &[]);
    assert!(e.end_after(&w, 4.0) >= w.e, "never short of the word that stays");
    // The pad itself is visible when the fence is wide but the sound has nothing to say: clamped to the
    // mark's own second above, and exactly `w.e + WORD_PAD` at the boundary case below.
    assert_eq!((w.e + edge::WORD_PAD).min(3.0), 1.58);
}

/// S3: the pad opens the cut's END before the first word that stays, on that side of the word — not
/// mirrored onto the wrong one.
#[test]
fn p_eng_wordpadseconds_s3_the_end_opens_before_the_word_that_stays() {
    // P.eng.wordPadSeconds: a word starting at 5.0 leaves 0.08 before it, so the cut resumes at 4.92.
    let words = [word("first", 5.0, 5.4)];
    let (marks, _) = edge::place_edges(vec![mark(1.0, 4.0, 5.0, 4.0)], &words, none, &[]);
    assert_eq!(marks[0].to, 4.92, "a pad before the retake's first word");
    assert!(marks[0].to < words[0].s, "the room sits BEFORE the word, not after it");

    // The direct call pads on the LEADING side of the same word, but only where there is a FENCE to open:
    // `start_before` returns `w.s - WORD_PAD` when the limit leaves it width and the sound has run out.
    // With a silent envelope and no fence width it answers the aligner's own time instead — which is why
    // `place_edges`, not this helper, is where the pad is asserted above.
    let e = edges(8.0, &[]);
    assert_eq!(e.start_before(&words[0], 5.0), 5.0, "no fence width, no pad");
    assert!(e.start_before(&words[0], 1.0) <= words[0].s, "never into the word that stays");
}

/// S4: the pad is a ROOM, not a minimum — a fence or a limit nearer than the pad wins — and it is a
/// different bound from the two neighbours that also sit near a cut edge.
#[test]
fn p_eng_wordpadseconds_s4_a_room_not_a_minimum_and_not_its_neighbours() {
    // A fence with no width never asks the sound: the aligner's own time is all there is, pad included.
    let e = edges(8.0, &[(1.0, 2.0, WORD)]);
    assert_eq!(e.end_after(&word("stays", 1.0, 1.5), 1.5), 1.5, "no fence width, no pad");
    assert_eq!(e.end_after(&word("stays", 1.0, 1.5), 1.2), 1.5, "not even past the limit");

    // Through the flow too: a mark whose start is inside the pad keeps its own second rather than being
    // pushed out to 1.58.
    let w = word("stays", 1.0, 1.5);
    let (marks, _) = edge::place_edges(vec![mark(1.52, 4.0, 0.0, 4.0)], &[w], none, &[]);
    assert_eq!(marks[0].s, 1.52, "the pad does not override where the mark already sat");

    // A stamp ON the retake's first word's own start opens the resume to the room the pad leaves before
    // that word (`again - WORD_PAD`); a stamp anywhere past the start sits beyond a word spoken between
    // the mark's end and itself, so the fence holds and the resume does not open there at all.
    let words = [word("first", 5.0, 5.4)];
    let (marks, _) = edge::place_edges(vec![mark(1.0, 4.6, 5.0, 4.0)], &words, none, &[]);
    assert_eq!(marks[0].to, 4.92, "a stamp one pad off keeps the room in front of its word");
    assert_eq!(marks[0].to, 5.0 - edge::WORD_PAD, "and that room is exactly P.eng.wordPadSeconds");

    // Same mark, stamp two tenths further on: past the pad, so no word is named and `to` is untouched.
    let (marks_far, _) = edge::place_edges(vec![mark(1.0, 4.6, 5.2, 4.0)], &words, none, &[]);
    assert_eq!(marks_far[0].to, 4.0, "0.2 s off is outside the pad, so nothing moves");

    // With a word spoken between, the fence decides instead of the pad: the gap cannot open at all, because
    // something WAS said there and only the words go.
    let between = [word("in between", 4.9, 4.98), word("first", 5.0, 5.4)];
    let (marks_said, _) = edge::place_edges(vec![mark(1.0, 4.6, 5.08, 4.0)], &between, none, &[]);
    assert_eq!(marks_said[0].to, 4.0, "something was said between, so the pad does not open");

    // Neighbours, asserted distinct so the three residues near a cut edge are not confused:
    // `edges::EDGE_PAD` (0.05) is what is left AFTER the envelope places an edge; WORD_PAD (0.08) is the
    // room left when there is no envelope to ask at all. Different questions, different numbers.
    assert_eq!(edge::EDGE_PAD, 0.05);
    assert_ne!(edge::WORD_PAD, edge::EDGE_PAD, "no-envelope room is not the post-envelope residue");

    // And `cut_select::MIN_SECONDS` (P.eng.minPieceSeconds, 0.04) asks whether a remainder is worth
    // keeping at all — nothing to do with how much room a cut gives a word.
    assert_eq!(cut_select::MIN_SECONDS, 0.04);
    assert_ne!(edge::WORD_PAD, cut_select::MIN_SECONDS, "worth-keeping is not room-left-for-a-word");
    assert_eq!(anywhere("P.eng.minPieceSeconds").from, "cut_select::MIN_SECONDS");

    // The pad's own row points at `edges`, not at either neighbour's constant.
    assert_eq!(anywhere("P.eng.wordPadSeconds").from, "edges::WORD_PAD");
}
