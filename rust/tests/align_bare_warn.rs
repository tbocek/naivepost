// §10-parameters · P.eng.alignBareWarnSeconds — voiced audio under no word before a warning (10 s;
// prototype `alignBareWarn`, gui/align.go:679, applied at gui/align.go:580). A misaligned transcript
// still reads as words, so the only visible sign is voiced audio with nothing over it -- which the cut
// then treats as footage where nothing was said and drops. The floor says when that is worth shouting
// about; the share beside it (`align::BARE_SHARE`) says whether it is a real error or just rounding.
//
// What this file pins: the value and its single catalogue row; the floor being inclusive at exactly this
// many seconds and refusing one hundredth under it; the floor alone not being enough without the share;
// the zero-spoken guard and the small-recording cases where this floor binds rather than the share; and
// the neighbouring constants in `align.rs` that belong to other rules, told apart by home.
//
// Everything here is pure arithmetic on `bare_warns` / `bare_warning` -- no UI, no files.

use naivepost::align;
use naivepost::params;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

/// S1: `P.eng.alignBareWarnSeconds` = 10, held by `align::BARE_WARN`, catalogued once under the
/// family §10 files it in.
#[test]
fn p_eng_alignbarewarnseconds_s1_the_floor_is_10_and_catalogued_once() {
    // P.eng.alignBareWarnSeconds -- "voiced audio under no word before a warning".
    assert_eq!(align::BARE_WARN, 10.0);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.alignBareWarnSeconds")
        .expect("the bare-audio floor must be catalogued for Prepare, whose align pass reads it");
    assert_eq!(
        row.from,
        "align::BARE_WARN",
        "a row must name the constant its rule reads"
    );
    // num() takes the integer branch for a whole value, which is how §10 spells this one: "10".
    assert_eq!(row.spelled, "10");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 10.0);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.alignBareWarnSeconds"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists -- one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.alignBareWarnSeconds").count(),
        1,
        "P.eng.alignBareWarnSeconds catalogued more than once"
    );
}

/// S2: the floor itself, measured at the boundary. It is inclusive, so a bare total of exactly this
/// many seconds already warns.
#[test]
fn p_eng_alignbarewarnseconds_s2_the_floor_is_inclusive_at_ten_seconds() {
    // 10.0 >= BARE_WARN and 10.0 >= 0.08 * 100 = 8.0: both conditions met at the round number, which
    // is why the fixture uses 10.0 rather than 10.1 -- the boundary itself has to warn.
    assert!(
        align::bare_warns(10.0, 100.0),
        "exactly the floor warns"
    );

    // One hundredth under it does not, even though the share test alone passes comfortably
    // (9.99 >= 8.0). So what refuses here is this constant, not the share.
    assert!(
        !align::bare_warns(9.99, 100.0),
        "under the floor stays quiet even with the share satisfied"
    );
    assert!(
        9.99 >= align::BARE_SHARE * 100.0,
        "the share condition alone would have passed -- the floor is what stopped it"
    );

    // The gap between the two fixtures is a hundredth of a second, and it flips the answer. Nothing in
    // a real recording sits that close to the floor by accident, so the inclusivity is safe either way.
    assert!(!align::bare_warns(0.0, 100.0), "nothing bare never warns");
}

/// S3: the floor alone is not enough -- the share has to cross too. This constant is only half the rule.
#[test]
fn p_eng_alignbarewarnseconds_s3_clearing_the_floor_is_not_enough_without_the_share() {
    // A long session with a small bare share: thirty seconds of bare audio in a thousand spoken clears
    // this floor three times over and still says nothing, because 30.0 < 0.08 * 1000 = 80.0. Ten bare
    // seconds of a four-hour session is rounding; that is what the share is for.
    assert!(
        !align::bare_warns(30.0, 1000.0),
        "thirty of a thousand is three percent -- rounding, not an error"
    );
    // Same recording with the share crossed: eighty of a thousand is eight percent, and now it warns.
    assert!(
        align::bare_warns(80.0, 1000.0),
        "eight percent of a long session warns"
    );
    // Both cleared this floor, so the difference between them is entirely the share's doing.
    assert!(
        30.0 >= align::BARE_WARN && 80.0 >= align::BARE_WARN,
        "both clear the absolute floor"
    );
    assert!(
        30.0 < align::BARE_SHARE * 1000.0 && 80.0 >= align::BARE_SHARE * 1000.0,
        "and only the second clears the share"
    );

    // align::BARE_SHARE (0.08) is the second condition -- named here because this rule reads it, but it
    // is its own ledger item and gets its own row in its own round, so nothing is asserted about its id.
    assert_eq!(align::BARE_SHARE, 0.08);
}

/// S4: the zero-spoken guard, and the short recordings where this floor is the binding constraint
/// rather than the share.
#[test]
fn p_eng_alignbarewarnseconds_s4_zero_spoken_never_warns_and_small_clips_bind_on_the_floor() {
    // No spoken audio means nothing to compare against: `spoken > 0.0` is tested first, so a large bare
    // figure against zero spoken raises nothing. An empty transcript cannot be judged misaligned.
    assert!(
        !align::bare_warns(50.0, 0.0),
        "nothing spoken raises no warning"
    );
    assert!(!align::bare_warns(10.0, 0.0), "not even at exactly the floor");

    // On a tiny clip the floor binds first: ten bare seconds of twelve spoken is most of the recording
    // and warns (10 >= 10 and 10 >= 0.96).
    assert!(align::bare_warns(10.0, 12.0), "ten bare of twelve spoken is a real error");

    // Nine bare seconds of ten clears the share easily (9 >= 0.8) but not this floor -- so it stays
    // quiet. The pair differs only in the seconds, which is what makes the floor's own effect visible
    // rather than attributable to the share.
    assert!(
        !align::bare_warns(9.0, 10.0),
        "nine of ten clears the share but not the floor"
    );
    assert!(
        9.0 >= align::BARE_SHARE * 10.0,
        "the share alone would have warned -- the floor kept it quiet"
    );
    assert_ne!(
        align::bare_warns(10.0, 12.0),
        align::bare_warns(9.0, 10.0),
        "one second of bare audio decides it, through this constant"
    );
}

/// S5: the neighbours. Several constants live next door in `align.rs`; the ids and homes are what keep
/// the rules apart.
#[test]
fn p_eng_alignbarewarnseconds_s5_neighbours_by_home_not_by_number() {
    // align::BARE_SHARE (0.08) is a proportion, this item's 10 is a duration -- independent magnitudes
    // combined by AND, never multiplied into one threshold. A bare 10 against a spoken 10 warns
    // (10 >= 10 and 10 >= 0.8), which a product-threshold of 0.8 would have decided differently.
    assert!(align::BARE_WARN > align::BARE_SHARE, "seconds dwarf the share");
    assert!(
        align::bare_warns(10.0, 10.0),
        "all of the spoken audio bare: both tests pass independently"
    );
    assert!(
        align::BARE_WARN * align::BARE_SHARE < align::BARE_WARN,
        "their product is not the threshold -- the two are checked separately"
    );

    // align::PAD (0.25) belongs to the pad rule and align::WINDOW (60) to the chunk window; neither
    // has anything to do with warning. They bracket this constant in size without sharing its job.
    assert_eq!(align::PAD, 0.25);
    assert_eq!(align::WINDOW, 60.0);
    assert!(
        align::PAD < align::BARE_WARN && align::BARE_WARN < align::WINDOW,
        "the floor sits between the pad and the window, doing neither of their jobs"
    );
    // Distinct homes in the catalogue: each rule reads its own constant.
    assert_eq!(
        params::find("P.eng.alignSoundPadSeconds").map(|r| r.from),
        Some("align::PAD"),
        "the pad's row names the pad"
    );
    assert_eq!(
        params::find("P.machine.alignChunkMaxSeconds").map(|r| r.from),
        Some("align::WINDOW"),
        "the window's row names the window"
    );
    assert_eq!(
        params::find("P.eng.alignBareWarnSeconds").map(|r| r.from),
        Some("align::BARE_WARN"),
        "and this floor's row names the floor"
    );

    // The warning text reports the same number the floor decided on: a bare of 10 against 100 spoken is
    // spelled out as such, so the log line and the rule cannot drift.
    let text = align::bare_warning(10.0, 100.0);
    assert!(text.starts_with("!!! align: "), "{text}");
    assert!(text.contains("10 s"), "the floor's number appears in the message: {text}");
    assert!(text.contains("100 s"), "and so does the spoken total: {text}");
    assert!(
        text.contains("no word over it"),
        "and what it means: {text}"
    );
}
