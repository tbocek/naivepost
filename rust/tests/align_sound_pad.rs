// §10-parameters · P.eng.alignSoundPadSeconds — sound kept around an alignment piece (0.25 s;
// prototype `alignSoundPad`, gui/align.go:674, used at gui/align.go:591 and :653). A word fades out
// below the silence threshold before it is really over, so a piece takes 0.25 s of the quiet on either
// side of its speech with it. The same constant answers both places that spend it: `trim()`, which pads
// the sounding stretch of a window, and `bare_voiced()`, which pads every word before asking what is
// left uncovered.
//
// What this file pins: the value and its single catalogue row; the pad reaching into the quiet on both
// sides by exactly this number; the pad being clamped to the window so it can never widen a piece past
// where the recording is; the same pad showing up as measured bare seconds; and the neighbouring pads
// and windows that are easy to confuse with it, told apart by home rather than by number.
//
// Everything here is pure arithmetic on `align::trim` / `align::bare_voiced` -- no UI, no files.

use naivepost::align;
use naivepost::asr::Silence;
use naivepost::params;
use naivepost::requests::Word;
use naivepost::transcribe::SAMPLE_RATE;

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

/// A word spoken between two seconds, in the samples the ASR sidecar carries.
fn word(text: &str, from: f64, to: f64) -> Word {
    Word {
        word: text.to_string(),
        start_sample: (from * SAMPLE_RATE as f64) as u64,
        end_sample: (to * SAMPLE_RATE as f64) as u64,
    }
}

/// S1: `P.eng.alignSoundPadSeconds` = 0.25, held by `align::PAD`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_alignsoundpadseconds_s1_the_pad_is_a_quarter_second_and_catalogued_once() {
    // P.eng.alignSoundPadSeconds -- "sound kept around an alignment piece".
    assert_eq!(align::PAD, 0.25);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.alignSoundPadSeconds")
        .expect("the alignment pad must be catalogued for Prepare, whose align pass reads it");
    assert_eq!(
        row.from,
        "align::PAD",
        "a row must name the constant its rule reads"
    );
    // num() keeps the decimal for a non-whole value, which is how §10 spells this one: "0.25".
    assert_eq!(row.spelled, "0.25");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.25);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.alignSoundPadSeconds"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists -- one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.alignSoundPadSeconds").count(),
        1,
        "P.eng.alignSoundPadSeconds catalogued more than once"
    );
}

/// S2: the pad itself -- `trim()` reaches into the quiet on each side of the sounding stretch by
/// exactly PAD, so a piece is wider than the speech inside it.
#[test]
fn p_eng_alignsoundpadseconds_s2_the_pad_reaches_into_the_quiet_on_both_sides() {
    // Quiet from 0..5 and from 25..30 leaves one sounding stretch, 5.0..25.0. Each end takes PAD of
    // the neighbouring quiet with it, so the piece is 4.75..25.25.
    let silences = [
        Silence { start: 0.0, end: 5.0 },
        Silence { start: 25.0, end: 30.0 },
    ];
    let piece = align::trim((0.0, 30.0), &silences).expect("something has sound");
    assert_eq!(piece, (4.75, 25.25), "both ends padded by PAD: {piece:?}");

    // Each delta is exactly the item's number -- not a rounding of it, not some other slack.
    assert_eq!(5.0 - piece.0, align::PAD, "the head took PAD of the leading quiet");
    assert_eq!(piece.1 - 25.0, align::PAD, "the tail took PAD of the trailing quiet");
    // Which makes the piece longer than the speech it carries by twice the pad.
    assert_eq!(
        (piece.1 - piece.0) - (25.0 - 5.0),
        2.0 * align::PAD,
        "the piece is the speech plus a pad at each end"
    );
}

/// S3: the pad never leaves the window. Where the speech starts or ends close enough to an edge, the
/// `.max(t0)` / `.min(t1)` in `trim()` stops the pad there instead of inventing audio outside the
/// recording.
#[test]
fn p_eng_alignsoundpadseconds_s3_the_pad_is_clamped_to_the_window() {
    // Head clamp: silence starts at 0.1, so a sliver of sound (0.0..0.1) survives at the very front.
    // first = 0.0 and (0.0 - 0.25).max(0.0) must land on 0.0, never negative.
    let head = align::trim((0.0, 30.0), &[Silence { start: 0.1, end: 5.0 }]).expect("sliver counts");
    assert_eq!(head.0, 0.0, "the pad cannot reach before the window opens: {head:?}");
    // The tail still gets its full pad (sound runs to 30.0, so it too is clamped, not exceeded).
    assert!(head.1 <= 30.0, "nothing extends past the window's end: {head:?}");

    // Tail clamp: sounding 5.0..29.9 wants 29.9 + 0.25 = 30.15, but t1 is 30.0.
    let tail = align::trim(
        (0.0, 30.0),
        &[
            Silence { start: 0.0, end: 5.0 },
            Silence { start: 29.9, end: 30.0 },
        ],
    )
    .expect("something has sound");
    assert_eq!(tail.1, 30.0, "the pad is cut off at the window's end, not 30.15: {tail:?}");
    assert_ne!(tail.1, 30.15, "the unclamped sum would have been 30.15: {tail:?}");
    assert_eq!(tail.0, 5.0 - align::PAD, "and the head still got its full pad: {tail:?}");

    // Both fixtures: the result stays inside [t0, t1].
    for piece in [head, tail] {
        assert!(piece.0 >= 0.0 && piece.1 <= 30.0, "piece outside its window: {piece:?}");
    }
}

/// S4: the same pad spent in `bare_voiced`. The plan asked for "a 1.0 s silence between two words
/// leaves 0.5 s bare", and that is not what the function returns -- worth writing down why, because it
/// changes how the value has to be pinned.
///
/// `bare_voiced` widens each word's coverage by PAD (align.rs:487-490), then walks the WHOLE
/// recording (head before the first word, tail after the last, align.rs:496-508) summing
/// `voiced(silences, leg_start, leg_end)` over the legs no word covers. `voiced` measures SOUND inside
/// a range -- everything except the listed silences (align.rs:204-223). So the total is
/// uncovered-and-sounding time, and a pause listed as silent contributes NOTHING to it: the pad exists
/// so the quiet around speech is not mistaken for missing words, not to shave pauses down. Measured
/// directly: `voiced(&[Silence{3.0, 5.0}], 3.25, 4.75)` = 0.0 -- the window sits wholly inside a
/// listed silence, so there is no sound in it to call bare.
///
/// What PAD controls is the SIZE of the uncovered windows, and through them how much of the
/// recording's SOUND escapes coverage. List nothing as quiet and every second sounds, so bare is simply
/// the time outside the padded words -- where this item's number shows up cleanly.
///
/// (One trap: `voiced` walks its silence list assuming it is already sorted and does not sort it, so a
/// descending list returns wrong totals. Every fixture here lists ascending.)
/// Measured on the fixture below (words at 1.0..3.0 and 5.0..7.0, duration 8.0, nothing listed quiet),
/// bare comes back 3.0: the inter-word gap 3.0..5.0 loses PAD at each end to the two pads and exposes
/// 1.5, the tail past the last pad exposes 0.75, and the head exposes 0.75 -- word A's pad opens at
/// 0.75, so 0.0..0.75 is uncovered sound. 1.5 + 0.75 + 0.75 = 3.0.
#[test]
fn p_eng_alignsoundpadseconds_s4_the_same_pad_shows_up_as_bare_seconds(){
    // Fixture: duration 8.0, two words at 1.0..3.0 and 5.0..7.0, NOTHING listed as quiet so all eight
    // seconds count as sounding. Padded coverage is [0.75, 3.25] and [4.75, 7.25], giving three
    // uncovered legs: the head 0.0..0.75, the gap 3.25..4.75, and the tail 7.25..8.0. Bare is their
    // sum, 0.75 + 1.5 + 0.75 = 3.0. The gap is `gap - 2*PAD` because a quarter of this item's number
    // is claimed at each of its ends by the neighbouring words' pads.
    let w = vec![word("one", 1.0, 3.0), word("two", 5.0, 7.0)];
    let all_sound = align::bare_voiced(&w, 8.0, &[]);
    assert!(
        (all_sound - 3.0).abs() < 1e-9,
        "with nothing listed quiet, bare is every second outside the padded words: got {all_sound}"
    );
    assert!(
        ((0.75 + ((5.0 - 3.0) - 2.0 * align::PAD) + 0.75) - all_sound).abs() < 1e-9,
        "the three legs add up through PAD"
    );

    // Counter-check tying the deduction to PAD: list the inter-word gap as silence (3.0..5.0) and
    // nothing else. That leg now holds no sound, so it drops out and bare falls to 1.5 -- the two outer
    // slivers. What came off is exactly `gap - 2*PAD`, the ground the pads had left exposed there.
    let mid_quiet = align::bare_voiced(&w, 8.0, &[Silence { start: 3.0, end: 5.0 }]);
    assert!((mid_quiet - 1.5).abs() < 1e-9, "listing the gap removes its share: got {mid_quiet}");
    assert!(
        (all_sound - mid_quiet - ((5.0 - 3.0) - 2.0 * align::PAD)).abs() < 1e-9,
        "what came off is the gap minus one PAD per side: {all_sound} -> {mid_quiet}"
    );

    // The mirror on the tail: list 7.0..8.0 as quiet instead and the gap keeps sounding, so bare is
    // 3.0 - 0.75 = 2.25. The amount that disappears is always the sounding part of the leg just
    // listed as silent -- never a fixed allowance taken off the pause itself.
    let tail_quiet = align::bare_voiced(&w, 8.0, &[Silence { start: 7.0, end: 8.0 }]);
    assert!((tail_quiet - 2.25).abs() < 1e-9, "quieting the tail removes its 0.75: got {tail_quiet}");
    assert!((all_sound - tail_quiet - 0.75).abs() < 1e-9, "exactly the tail's worth: {all_sound} -> {tail_quiet}");

    // And the pad's actual job, measured: quiet next to speech must not read as a missing word. List
    // everything outside the two words as silent and bare is 0, since no uncovered stretch contains
    // sound. This is why a real recording with natural pauses between phrases measures clean.
    let hushed = align::bare_voiced(
        &w,
        8.0,
        &[
            Silence { start: 0.0, end: 1.0 },
            Silence { start: 3.0, end: 5.0 },
            Silence { start: 7.0, end: 8.0 },
        ],
    );
    assert_eq!(hushed, 0.0, "nothing sounding outside the words leaves nothing bare: got {hushed}");
}

/// S5: the neighbours. Several constants sit near this number or share the word "pad"; the ids and
/// homes are what keep the rules apart.
#[test]
fn p_eng_alignsoundpadseconds_s5_neighbours_by_home_not_by_number() {
    // P.eng.wordPadSeconds (0.08, `edges::WORD_PAD`) is the room a CUT leaves a word, so the splice
    // does not shave one -- a different job at a different stage, three times smaller.
    let word_pad = params::find("P.eng.wordPadSeconds").expect("wordPadSeconds is catalogued");
    assert_eq!(word_pad.from, "edges::WORD_PAD");
    assert_eq!(word_pad.spelled, "0.08");
    assert!(
        align::PAD != naivepost::edges::WORD_PAD,
        "the alignment pad and the cut pad are separate numbers"
    );
    assert!(
        align::PAD > naivepost::edges::WORD_PAD,
        "an alignment piece keeps far more quiet about it than a cut leaves a word"
    );

    // P.machine.alignChunkMaxSeconds (60) / `align::WINDOW` and P.machine.alignChunkMinSeconds (15)
    // / `align::MIN_PIECE`: the window this pad is taken around. Small enough that it can never
    // swallow a piece, even after the window halves to its floor.
    assert_eq!(align::WINDOW, 60.0);
    assert_eq!(align::MIN_PIECE, 15.0);
    assert_eq!(
        params::find("P.machine.alignChunkMinSeconds").map(|r| r.from),
        Some("align::MIN_PIECE"),
        "the halving floor is catalogued from the same constant"
    );
    assert!(
        2.0 * align::PAD < align::MIN_PIECE,
        "0.5 << 15: a padded piece never eats its own window"
    );

    // P.eng.alignCutSeekSeconds (4, `align::CUT_SEEK`) decides WHERE a piece starts; PAD decides how
    // much quiet travels with it. Different questions, and the pad is much the smaller of the two --
    // otherwise the pad would be large enough to overlap the reach that placed the edge.
    let cut_seek = params::find("P.eng.alignCutSeekSeconds").expect("alignCutSeekSeconds is catalogued");
    assert_eq!(cut_seek.from, "align::CUT_SEEK");
    assert!(
        align::PAD < align::CUT_SEEK,
        "the pad is smaller than the reach that placed the edge it pads: {} < {}",
        align::PAD,
        align::CUT_SEEK
    );
}
