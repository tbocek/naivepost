// §10-parameters · P.eng.sepLopsidedDB — a stem this much quieter is dropped (10 dB; prototype
// `sepLopsidedDB`, gui/separate.go:339, applied at gui/separate.go:329). When the separation model
// hears a recording as voice nearly throughout -- other players talking in game chat, say -- the half
// named for the room comes back with next to nothing in it. This constant says how far the rest has to
// fall under the mix before that gets said out loud.
//
// What "dropped" means here: the note is what lets the USER discard the empty half by hand. The app
// marks the split and leaves both stems on disk -- nothing is deleted, only marked -- so this threshold
// gates a message rather than a deletion.
//
// What this file pins: the value and its single catalogue row; the threshold being inclusive at exactly
// this many dB; the direction of the comparison and the gap the note quotes back; unmeasurable levels
// saying nothing at all; and how the note is wired into the loudness line, beside the unrelated absolute
// silence threshold.
//
// `separate::mean_db` is private, so every level reading goes through the public `lopsided` /
// `loudness_report`.

use naivepost::asr;
use naivepost::params;
use naivepost::separate;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{all_rows};

/// S1: `P.eng.sepLopsidedDB` = 10, held by `separate::LOPSIDED_DB`, catalogued once under the
/// family §10 files it in.
#[test]
fn p_eng_seplopsideddb_s1_the_threshold_is_10_and_catalogued_once() {
    // P.eng.sepLopsidedDB -- "a stem this much quieter is dropped".
    assert_eq!(separate::LOPSIDED_DB, 10.0);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.sepLopsidedDB")
        .expect("the lop-sided split threshold must be catalogued for Prepare, whose split pass reads it");
    assert_eq!(
        row.from,
        "separate::LOPSIDED_DB",
        "a row must name the constant its rule reads (the prototype called it sepLopsidedDB)"
    );
    // num() takes the integer branch for a whole value, which is how §10 spells this one: "10".
    assert_eq!(row.spelled, "10");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 10.0);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.sepLopsidedDB"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists -- one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.sepLopsidedDB").count(),
        1,
        "P.eng.sepLopsidedDB catalogued more than once"
    );
}

/// S2: the threshold is inclusive. A rest exactly this many dB under the mix already earns the note.
#[test]
fn p_eng_seplopsideddb_s2_the_threshold_is_inclusive_at_exactly_ten_decibels() {
    // -20 dB mix against -30 dB rest is a gap of precisely LOPSIDED_DB, and the guard is `gap <
    // LOPSIDED_DB`, so the boundary itself warns. Unlike align::CUT_SEEK's reach, which is a strict
    // `<` and excludes its own value, this threshold includes it.
    assert_eq!(
        (-20.0f64) - (-30.0f64),
        separate::LOPSIDED_DB,
        "the fixture sits exactly on the boundary"
    );
    assert!(
        separate::lopsided("-20.0 dB", "-30.0 dB").is_some(),
        "exactly ten decibels down earns the note"
    );

    // One tenth of a decibel short of the threshold and there is nothing to say.
    assert!(
        separate::lopsided("-20.0 dB", "-29.9 dB").is_none(),
        "9.9 dB is a normal split"
    );
    assert!(
        (-20.0f64) - (-29.9f64) < separate::LOPSIDED_DB,
        "and the arithmetic shows why it falls short"
    );

    // Further down keeps warning -- the threshold is a floor, not a band.
    assert!(separate::lopsided("-20.0 dB", "-45.0 dB").is_some(), "25 dB down still warns");
}

/// S3: only a quieter rest counts, and the note quotes the measured gap rather than the constant.
#[test]
fn p_eng_seplopsideddb_s3_only_a_quieter_rest_counts_and_the_gap_is_what_gets_said() {
    // A rest LOUDER than the mix is not lop-sided in the sense this rule cares about: the gap is
    // negative, comfortably under the threshold.
    assert!(
        separate::lopsided("-30.0 dB", "-20.0 dB").is_none(),
        "a louder rest never triggers the note"
    );
    // Equal levels give a zero gap, likewise nothing to report.
    assert!(
        separate::lopsided("-20.0 dB", "-20.0 dB").is_none(),
        "an even split says nothing"
    );

    // Thirteen decibels down: the note quotes 13 dB, the number the meters measured, NOT the 10 dB
    // threshold that decided it. A user reading the log learns the actual balance of this recording.
    let note = separate::lopsided("-20.0 dB", "-33.0 dB").expect("13 dB down is worth saying");
    assert!(note.contains("13 dB under the mix"), "the measured gap is quoted: {note}");
    assert!(!note.contains("10 dB"), "the deciding threshold is not printed: {note}");
    // And the note points at the remedy rather than leaving the reader to work it out.
    assert!(note.contains("voice half"), "the remedy names the other half: {note}");
    assert!(
        note.contains("half named for the room has little in it"),
        "and says what went wrong: {note}"
    );

    // The reported number tracks the fixture, so it is genuinely computed from the levels.
    let deeper = separate::lopsided("-20.0 dB", "-40.0 dB").expect("20 dB down");
    assert!(deeper.contains("20 dB under the mix"), "a deeper gap reports 20: {deeper}");
    assert!(!deeper.contains("13 dB"), "and not the previous fixture's number: {deeper}");
}

/// S4: an unmeasurable level says nothing, so the threshold is never consulted.
#[test]
fn p_eng_seplopsideddb_s4_an_unmeasurable_level_never_reaches_the_threshold() {
    // Empty and non-numeric levels return None on every combination. A missing number is not evidence of
    // anything, so the function bails at the parse and no comparison against 10 dB ever happens.
    assert!(separate::lopsided("", "-31.0 dB").is_none(), "no mix level says nothing");
    assert!(separate::lopsided("-20.0 dB", "n/a").is_none(), "no rest level says nothing");
    assert!(separate::lopsided("", "").is_none(), "neither level says nothing");
    assert!(
        separate::lopsided("quiet", "silent").is_none(),
        "words where numbers belong are not a measurement"
    );

    // The parse is the reason, not the size of the gap: had "n/a" parsed, the comparison would have
    // been reached. It does not parse, so the threshold plays no part in these results.
    assert!(separate::lopsided("-20.0 dB", "n/a").is_none());
    assert!(
        separate::lopsided("n/a", "-20.0 dB").is_none(),
        "either side unparseable gives the same answer"
    );

    // The parser accepts a trailing percent, which some volumedetect spellings carry: "-20.0%" parses as
    // -20.0, so a 15 dB gap between two such readings still crosses the threshold and is reported.
    let percent = separate::lopsided("-20.0%", "-35.0%").expect("percent spellings parse");
    assert!(percent.contains("15 dB under the mix"), "15 dB from the percent pair: {percent}");
    // And below the threshold in the same spelling, nothing is said.
    assert!(
        separate::lopsided("-20.0%", "-25.0%").is_none(),
        "5 dB in percent spelling is still a normal split"
    );
}

/// S5: the neighbours and the wiring. This constant rides on the loudness line and must not be confused
/// with the absolute silence threshold.
#[test]
fn p_eng_seplopsideddb_s5_wiring_and_neighbours_by_home_not_by_number() {
    // The note is appended by loudness_report only when the gap crosses. Crossing case: gap 10.
    let crossed = separate::loudness_report("lecture.mkv", "-20.0 dB", "-21.0 dB", "-30.0 dB");
    assert!(
        crossed.ends_with("cut from the original instead of the split."),
        "the remedy closes the report: {crossed}"
    );
    assert!(crossed.starts_with(">>> [lecture.mkv] the mix averaged -20.0 dB"), "{crossed}");

    // Not-crossing case: gap 5, so the report is the plain log line and NOTHING more.
    let base = separate::loudness_log("lecture.mkv", "-20.0 dB", "-21.0 dB", "-25.0 dB");
    let plain = separate::loudness_report("lecture.mkv", "-20.0 dB", "-21.0 dB", "-25.0 dB");
    assert_eq!(plain, base, "under the threshold the report is just the log line");
    assert!(!plain.contains("lop"), "no warning smuggled in: {plain}");

    // Distinct homes among the separation constants.
    let chunk = params::find("P.machine.sepChunkMaxSeconds").expect("sepChunkMaxSeconds is catalogued");
    assert_eq!(chunk.from, "separate::CHUNK_MAX_SECONDS");
    assert_eq!(chunk.spelled, "300");
    let lopsided_row = params::find("P.eng.sepLopsidedDB").expect("sepLopsidedDB is catalogued");
    assert_ne!(
        lopsided_row.from, chunk.from,
        "a chunk size and a loudness threshold are different rules"
    );
    assert_eq!(separate::CHUNK_MAX_SECONDS, 300.0);

    // separate::SEEK_WINDOW (20.0) has no §10 row of its own in this tree, so nothing is asserted
    // about an id for it -- only that it is the larger of the two seconds-valued constants here.
    assert_eq!(separate::SEEK_WINDOW, 20.0);
    assert!(
        separate::SEEK_WINDOW > separate::LOPSIDED_DB,
        "ordering only; SEEK_WINDOW has no catalogue row"
    );

    // asr::QUIET_DB (-35.0) is an ABSOLUTE level -- how quiet a stretch must be to count as silence.
    // This constant is a RELATIVE gap between two stems. Different kinds of quantity, different homes.
    let quiet = params::find("P.machine.silenceThresholdDB").expect("silenceThresholdDB is catalogued");
    assert_eq!(quiet.from, "asr::QUIET_DB");
    assert!(asr::QUIET_DB < 0.0, "the silence threshold is an absolute level below zero");
    assert!(
        separate::LOPSIDED_DB > 0.0,
        "this one is a positive difference between two measurements"
    );
    assert_ne!(lopsided_row.from, quiet.from, "distinct homes for distinct rules");

    // Never interchangeable: a -35 dB rest under a -40 dB mix has both stems sitting below the silence
    // threshold, yet the gap is only 5 dB, so nothing is lop-sided and no note is made.
    assert!(
        asr::QUIET_DB >= -35.0 && asr::QUIET_DB <= -35.0,
        "both fixtures here are at or under the silence level"
    );
    assert!(
        separate::lopsided("-40.0 dB", "-35.0 dB").is_none(),
        "two silent-ish stems 5 dB apart are not lop-sided"
    );
    // Same two levels read the other way round: still 5 dB, still nothing.
    assert!(separate::lopsided("-35.0 dB", "-40.0 dB").is_none(), "direction matters too");
}
