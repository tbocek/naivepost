// §10-parameters · P.eng.alignBareShare — share of voiced audio under no word before a warning (0.08;
// prototype `alignBareShare`, gui/align.go:679, applied at gui/align.go:580). The seconds floor
// beside it (`align::BARE_WARN`) says how much bare audio is worth mentioning; this constant says how
// much of THAT SESSION's speech the bare audio has to be. Without the relative half, a four-hour
// recording mis-transcribing a few percent would shout about rounding, and a twenty-second clip losing
// most of its speech would be judged by the same absolute yardstick as a trivial slip in a long one.
//
// What this file pins: the value and its single catalogue row; the boundary where the share lands
// exactly on the floor and where one more second of speech moves that floor above the bare total; the
// share being relative rather than a fixed allowance; the two conditions being ANDed so neither raises a
// warning alone; and the neighbouring constants in `align.rs` that are sub-1 or unrelated, told apart
// by home.
//
// Everything here is pure arithmetic on `bare_warns` / `bare_warning` -- no UI, no files.

use naivepost::align;
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

/// S1: `P.eng.alignBareShare` = 0.08, held by `align::BARE_SHARE`, catalogued once under the family
/// §10 files it in.
#[test]
fn p_eng_alignbareshare_s1_the_share_is_008_and_catalogued_once() {
    // P.eng.alignBareShare -- "share of voiced audio under no word before a warning".
    assert_eq!(align::BARE_SHARE, 0.08);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.alignBareShare")
        .expect("the bare-audio share must be catalogued for Prepare, whose align pass reads it");
    assert_eq!(
        row.from,
        "align::BARE_SHARE",
        "a row must name the constant its rule reads"
    );
    // num() keeps the decimal for a non-whole value, which is how §10 spells this one: "0.08".
    assert_eq!(row.spelled, "0.08");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 0.08);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.alignBareShare"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists -- one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.alignBareShare").count(),
        1,
        "P.eng.alignBareShare catalogued more than once"
    );
}

/// S2: the share's own boundary, inclusive. 125 s of speech puts this share exactly on the seconds
/// floor, so ten bare seconds warns; one more second of speech lifts the required amount above ten and
/// the same bare total no longer reaches it.
#[test]
fn p_eng_alignbareshare_s2_the_share_lands_exactly_on_the_floor_at_125_seconds() {
    // 0.08 * 125.0 = 10.0 exactly: the share asks for precisely what BARE_WARN already requires, so a
    // bare total of 10.0 satisfies both and the inclusive `>=` lets it through.
    assert_eq!(align::BARE_SHARE * 125.0, 10.0, "the share meets the floor exactly at 125 s");
    assert!(
        align::bare_warns(10.0, 125.0),
        "ten bare of one hundred twenty-five spoken warns"
    );

    // One further second of speech raises the ask to 10.08, which ten bare seconds do not reach. The
    // seconds floor is still cleared (10 >= 10), so what refuses here is this constant alone.
    assert!(
        !align::bare_warns(10.0, 126.0),
        "one more second of speech lifts the share's ask above the bare total"
    );
    assert!(
        align::BARE_SHARE * 126.0 > 10.0,
        "the share now asks for more than ten seconds: {}",
        align::BARE_SHARE * 126.0
    );
    assert!(
        10.0 >= align::BARE_WARN,
        "and the absolute floor is still satisfied -- the share is what stopped it"
    );
}

/// S3: the share is relative, not a fixed allowance. The same bare figure is decided by how much speech
/// there was, which is what lets one constant cover a twenty-second clip and a four-hour session.
#[test]
fn p_eng_alignbareshare_s3_doubling_the_session_doubles_the_bare_audio_required() {
    // Twenty bare of two hundred fifty spoken is exactly eight percent, and warns.
    assert!(align::bare_warns(20.0, 250.0), "twenty of 250 is eight percent");
    // Twenty of 251 falls just short -- the ask moved to 20.08 while the bare total stood still.
    assert!(!align::bare_warns(20.0, 251.0), "251 s of speech asks for 20.08");
    assert!(
        align::BARE_SHARE * 251.0 > 20.0,
        "the ask grew with the session: {}",
        align::BARE_SHARE * 251.0
    );

    // Double everything and the shape repeats: forty of five hundred warns, forty of 501 does not.
    assert!(align::bare_warns(40.0, 500.0), "forty of 500 is eight percent too");
    assert!(!align::bare_warns(40.0, 501.0), "and 501 s asks for 40.08");

    // So the bare audio needed for a warning scales with the session rather than staying put: twice the
    // speech means twice the bare seconds before anything is said. That proportionality is the whole
    // reason a seconds-only rule could not serve both a short clip and a long recording.
    assert_eq!(
        align::BARE_SHARE * 500.0,
        2.0 * (align::BARE_SHARE * 250.0),
        "double the speech, double the ask"
    );
}

/// S4: the two conditions are ANDed -- the share cannot raise a warning on its own, nor can the floor.
#[test]
fn p_eng_alignbareshare_s4_neither_half_of_the_rule_warns_alone() {
    // The share passes comfortably here -- eight of a hundred is exactly eight percent -- but eight
    // seconds is under the absolute floor, so nothing is said.
    assert!(
        8.0 >= align::BARE_SHARE * 100.0,
        "the share condition alone is satisfied"
    );
    assert!(8.0 < align::BARE_WARN, "and the seconds floor is not");
    assert!(!align::bare_warns(8.0, 100.0), "so the AND refuses");

    // From the other side, last round's case: thirty bare of a thousand clears the floor three times
    // over but is only three percent, so the share holds it back.
    assert!(30.0 >= align::BARE_WARN, "the floor is cleared here");
    assert!(30.0 < align::BARE_SHARE * 1000.0, "the share is not");
    assert!(!align::bare_warns(30.0, 1000.0), "so the AND refuses again");

    // Together: each half passing alone is not enough, and both passing together is.
    assert!(align::bare_warns(20.0, 250.0), "both halves satisfied warns");
    assert_ne!(
        align::bare_warns(20.0, 250.0),
        align::bare_warns(8.0, 100.0),
        "same rule, one half missing, different answer"
    );

    // The zero-spoken guard belongs to the guard, not to the share: with no speech the share's product is
    // zero, which any bare figure would clear, so `spoken > 0.0` is what actually refuses.
    assert_eq!(align::BARE_SHARE * 0.0, 0.0, "the share would let anything pass against no speech");
    assert!(!align::bare_warns(1.0, 0.0), "the guard is what stops it, not the share");
}

/// S5: the neighbours. Several constants sit near this number in `align.rs`; the ids and homes are what
/// keep the rules apart.
#[test]
fn p_eng_alignbareshare_s5_neighbours_by_home_not_by_number() {
    // The paired constant: same rule, different kind of quantity. Both have their own row and their own
    // spelling, and neither substitutes for the other.
    let warn = params::find("P.eng.alignBareWarnSeconds").expect("alignBareWarnSeconds is catalogued");
    let share = params::find("P.eng.alignBareShare").expect("alignBareShare is catalogued");
    assert_eq!(warn.from, "align::BARE_WARN");
    assert_eq!(share.from, "align::BARE_SHARE");
    assert_ne!(warn.from, share.from, "two constants, two homes");
    assert_eq!(warn.spelled, "10");
    assert_eq!(share.spelled, "0.08");
    assert!(
        align::BARE_WARN > align::BARE_SHARE,
        "seconds dwarf a dimensionless proportion"
    );

    // align::PAD (0.25) is also sub-1 in this module but is SECONDS and belongs to the pad rule. They
    // are not interchangeable: the share always multiplies a total, the pad never does.
    assert_eq!(align::PAD, 0.25);
    assert!(align::PAD > align::BARE_SHARE, "both small, different kinds");
    assert!(
        align::PAD * 100.0 != align::BARE_SHARE * 100.0,
        "swapping them would change every threshold the share decides"
    );
    // The pad's row names the pad, not this ratio.
    assert_eq!(
        params::find("P.eng.alignSoundPadSeconds").map(|r| r.from),
        Some("align::PAD"),
        "the pad's row points at the pad"
    );

    // align::CUT_SEEK (4.0) and align::WINDOW (60.0) belong to the cut's reach and the chunk window;
    // asserted for order only, since they share no computation with this constant.
    assert_eq!(align::CUT_SEEK, 4.0);
    assert_eq!(align::WINDOW, 60.0);
    assert!(align::CUT_SEEK < align::BARE_WARN, "ordering only");
    assert!(align::BARE_WARN < align::WINDOW, "ordering only");

    // The share decides but is never spelled out: the log reports the measured seconds, not the ratio
    // that decided them.
    let text = align::bare_warning(12.0, 100.0);
    assert!(text.starts_with("!!! align: "), "{text}");
    assert!(text.contains("12 s"), "the bare total is reported: {text}");
    assert!(text.contains("100 s"), "and the spoken total: {text}");
    assert!(
        !text.contains("0.08"),
        "the deciding ratio is not printed: {text}"
    );
}
