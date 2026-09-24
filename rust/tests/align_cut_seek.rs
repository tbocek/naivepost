// §10-parameters · P.eng.alignCutSeekSeconds — how far an alignment cut slides to a silence (4 s;
// prototype `alignCutSeek`, gui/align.go:670, passed at gui/align.go:355). The aligner used to borrow
// F1.4's chunker reach (`asr::SEEK_MAX`, 20 s); §10 gives it its own, and much smaller one, because a
// chunk cut that moves costs only a join while an alignment cut moves the whole window a stretch of
// words is timed against.
//
// What this file pins: the value and its single catalogue row; a silence inside the reach taking the
// cut; the reach being a bound rather than a wish (and what the old wider reach would have done); the
// ceiling clamping the effective reach below the constant on a short window; and the neighbouring
// constants that share a number or sit near it, told apart by home.
//
// Everything here is pure arithmetic on `asr::cut_points` / `align::pieces` — no UI, no files. Note
// the filter in `cut_points` is STRICT (`(mid - want).abs() < seek`), so a silence midpoint sitting
// exactly `seek` away is NOT taken; fixtures are placed strictly inside.

use naivepost::align;
use naivepost::asr::{self, Silence};
use naivepost::params;

/// Every list that carries rows, chained across all five pages so uniqueness is checked against the
/// whole catalogue rather than one page (`tests/hands_off.rs`'s `row()` makes the same choice).
fn all_rows() -> Vec<params::Param> {
    params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .collect()
}

/// The aligner's own cut over a recording: the ASR's ceiling with the aligner's reach.
fn cut(duration: f64, silences: &[Silence]) -> Vec<f64> {
    asr::cut_points(duration, silences, align::WINDOW, align::CUT_SEEK)
}

/// The same ceiling with some other reach, for showing the reach itself is what binds.
fn cut_reach(duration: f64, silences: &[Silence], seek: f64) -> Vec<f64> {
    asr::cut_points(duration, silences, align::WINDOW, seek)
}

/// S1: `P.eng.alignCutSeekSeconds` = 4, held by `align::CUT_SEEK`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_aligncutseekseconds_s1_the_reach_is_4_and_catalogued_once() {
    // P.eng.alignCutSeekSeconds — "how far an alignment cut slides to a silence".
    assert_eq!(align::CUT_SEEK, 4.0);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.alignCutSeekSeconds")
        .expect("the alignment cut's reach must be catalogued for Prepare, whose align pass reads it");
    assert_eq!(
        row.from,
        "align::CUT_SEEK",
        "a row must name the constant its rule reads"
    );
    // num() drops the `.0` on a whole value, which is exactly how §10 spells this one: "4".
    assert_eq!(row.spelled, "4");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 4.0);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.alignCutSeekSeconds"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists — one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.alignCutSeekSeconds").count(),
        1,
        "P.eng.alignCutSeekSeconds catalogued more than once"
    );
}

/// S2: a silence whose midpoint falls inside the reach takes the cut off its nominal place.
#[test]
fn p_eng_aligncutseekseconds_s2_a_silence_inside_the_reach_takes_the_cut() {
    // pieces = ceil(240/(60 - 2*4)) = ceil(4.615) = 5, step = 240/5 = 48.0, so the nominal wants
    // are 48, 96, 144, 192. The silence [50, 54] has midpoint 52.0, 4.0 past 48 -- but the filter
    // is strict `<`, so that exact distance is NOT taken. Move it to [49, 52]: midpoint 50.5, 2.5
    // away, comfortably inside, and the first interior edge lands there.
    let edges = cut(240.0, &[Silence { start: 49.0, end: 52.0 }]);
    assert_eq!(edges.first().copied(), Some(0.0), "the recording starts at zero: {edges:?}");
    assert_eq!(edges.last().copied(), Some(240.0), "and ends where it was: {edges:?}");
    assert!(
        edges.iter().any(|e| (*e - 50.5).abs() < 1e-9),
        "the cut slid onto the silence midpoint 50.5: {edges:?}"
    );
    assert!(
        !edges.iter().any(|e| (*e - 48.0).abs() < 1e-9),
        "the nominal 48.0 was replaced, not kept alongside: {edges:?}"
    );
    // Only that one cut moved: the other three wants had no silence near them.
    for untouched in [96.0, 144.0, 192.0] {
        assert!(
            edges.iter().any(|e| (*e - untouched).abs() < 1e-9),
            "{untouched} stayed nominal: {edges:?}"
        );
    }
    assert_eq!(edges.len(), 6, "five pieces, six edges: {edges:?}");

    // The exact boundary, measured: [50, 54] has midpoint 52.0, precisely 4.0 from the nominal 48.0.
    // `cut_points` filters with a strict `<`, so that distance does NOT count and the cut stays at
    // 48.0 -- the reach is "under 4 s", not "up to 4 s".
    let exact = cut(240.0, &[Silence { start: 50.0, end: 54.0 }]);
    assert!(
        exact.iter().any(|e| (*e - 48.0).abs() < 1e-9),
        "exactly CUT_SEEK away is outside a strict `<`: {exact:?}"
    );
    assert!(
        !exact.iter().any(|e| (*e - 52.0).abs() < 1e-9),
        "the boundary midpoint was not slid onto: {exact:?}"
    );
}

/// S3: the reach is a bound. A silence further off than 4 s buys nothing, even though the old
/// borrowed reach would have gone to it.
#[test]
fn p_eng_aligncutseekseconds_s3_beyond_the_reach_the_nominal_cut_stands() {
    // Same fixture, silence [52.5, 53.5] -> midpoint 53.0, 5.0 past the nominal 48.0: out of reach,
    // so the cut stays at 48.0 and the silence is ignored entirely.
    let far = cut(240.0, &[Silence { start: 52.5, end: 53.5 }]);
    assert!(
        far.iter().any(|e| (*e - 48.0).abs() < 1e-9),
        "the nominal 48.0 stands when nothing is in reach: {far:?}"
    );
    assert!(
        !far.iter().any(|e| (*e - 53.0).abs() < 1e-9),
        "the unreachable midpoint was not slid onto: {far:?}"
    );

    // The reachable version of the same fixture does slide, so placement decides.
    let near = cut(240.0, &[Silence { start: 49.0, end: 52.0 }]);
    assert_ne!(near, far, "moving the silence 2.5 s decides whether the cut slides");

    // Measured against the chunker's own reach on the same fixture: `asr::SEEK_MAX` (20) trips the
    // walk-past guard at this ceiling (`2*seek >= limit` zeroes the reach), so it cuts dead even at
    // 40.0 steps and never slides toward any silence. That is a third answer, not the aligner's --
    // another reason §10 keeps the two numbers apart rather than sharing one.
    let borrowed = asr::cut_points(240.0, &[Silence { start: 52.5, end: 53.5 }], align::WINDOW, asr::SEEK_MAX);
    assert_eq!(
        borrowed,
        vec![0.0, 40.0, 80.0, 120.0, 160.0, 200.0, 240.0],
        "the chunker's 20 s reach trips its own guard here and cuts even, sliding nowhere"
    );
    assert_ne!(borrowed, far, "same fixture, two reaches, two cuts");

    // And the reach really is the bound: ask for 5.0 instead of 4.0 and the very same midpoint that
    // was refused above (53.0, 5.0 past the nominal under CUT_SEEK... strictly inside 5.0) is taken.
    let wider = cut_reach(240.0, &[Silence { start: 52.5, end: 53.5 }], 5.5);
    assert!(
        wider.iter().any(|e| (*e - 53.0).abs() < 1e-9),
        "a 5.5 s reach takes what 4 s leaves alone: {wider:?}"
    );
    assert_ne!(wider, far, "one and a half seconds of reach decide whether this silence is used");
}

/// S4: the ceiling clamps the reach, so on a short window the constant is not what actually applies.
#[test]
fn p_eng_aligncutseekseconds_s4_a_short_ceiling_clamps_the_reach_below_the_constant() {
    // `cut_points` trims the reach twice: `seek.min(limit/6.0)` then `seek.min(step/3.0)`. At a 12 s
    // ceiling that first clamp alone cuts 4.0 down to 2.0, so the silence [9,12] with midpoint 10.5
    // sitting 3.0 off the nominal 7.5 is out of reach and the edge stays nominal.
    let small = asr::cut_points(60.0, &[Silence { start: 9.0, end: 12.0 }], 12.0, align::CUT_SEEK);
    assert!(
        small.iter().any(|e| (*e - 7.5).abs() < 1e-9),
        "the nominal 7.5 stands at a 12 s ceiling: {small:?}"
    );
    assert!(
        !small.iter().any(|e| (*e - 10.5).abs() < 1e-9),
        "midpoint 10.5 is 3.0 off, past the clamped 2.0 reach: {small:?}"
    );
    // Proof the clamp is what stopped it: the same silence IS taken when the requested reach is inside
    // the clamped value.
    let live = asr::cut_points(60.0, &[Silence { start: 9.0, end: 12.0 }], 12.0, 1.0);
    assert!(
        live.iter().any(|e| (*e - 10.5).abs() < 1e-9),
        "a 1.0 s reach (under the 2.0 clamp) reaches it: {live:?}"
    );

    // At the real ceiling the constant survives both trims untouched, so CUT_SEEK really is the reach.
    assert!(
        align::CUT_SEEK <= align::WINDOW / 6.0,
        "4 <= 60/6 = 10: unclamped at the align window"
    );
    assert!(2.0 * align::CUT_SEEK < align::WINDOW, "and never trips the walk-past guard either");
}

/// S5: the neighbours. Several constants share or nearly share this number; the ids and homes are what
/// keep the rules apart.
#[test]
fn p_eng_aligncutseekseconds_s5_neighbours_by_home_not_by_number() {
    // P.eng.asrCutSeekSeconds (20, `asr::SEEK_MAX`): the chunker's reach. Much wider, because a
    // chunk cut that slides costs only a join, while an alignment cut moves the whole window that a
    // stretch of words is timed against.
    let asr_seek = params::find("P.eng.asrCutSeekSeconds").expect("asrCutSeekSeconds is catalogued");
    assert_eq!(asr_seek.spelled, "20");
    assert_eq!(asr_seek.from, "asr::SEEK_MAX");
    assert!(
        align::CUT_SEEK < asr::SEEK_MAX,
        "the aligner slides far less far than the chunker"
    );

    // P.machine.alignChunkMaxSeconds (60) and P.machine.alignChunkMinSeconds (15): the window this
    // cut tiles, and the floor it halves down to. The reach is sized to stay live at that floor --
    // `cut_points` stops sliding once the reach is half the window, so 4 s still works at 15 s
    // (2*4 = 8 < 15) where the chunker's 20 s would have given up on sliding entirely.
    assert_eq!(align::WINDOW, 60.0);
    assert_eq!(align::MIN_PIECE, 15.0);
    assert_eq!(
        params::find("P.machine.alignChunkMinSeconds").map(|r| r.from),
        Some("align::MIN_PIECE"),
        "the halving floor is catalogued from the same constant"
    );
    assert!(
        2.0 * align::CUT_SEEK < align::MIN_PIECE,
        "8 < 15: the reach keeps sliding when the window halves to its floor"
    );
    assert!(
        2.0 * asr::SEEK_MAX >= align::MIN_PIECE,
        "the chunker's reach would stop sliding at that floor -- which is fine for chunks, not for align"
    );

    // align::PAD (0.25) is a different thing again: how much of the quiet either side of a window's
    // sound goes up with it, so a word fading below the silence threshold is not clipped. It has no
    // §10 row of its own in this tree (`grep PAD src/params.rs` finds only edges::WORD_PAD and
    // cut_review::REVIEW_PAD_SECONDS), so nothing is asserted about an id for it -- only that it is
    // not a cut reach.
    assert_eq!(align::PAD, 0.25);
    assert_ne!(align::CUT_SEEK, align::PAD, "a pad is not a search distance");

    // P.eng.sepCutSeekSeconds exists in the spec (20, `sepCutSeek`) but has NO row in params.rs yet,
    // so nothing is asserted about it here -- that item's round will come.
}
