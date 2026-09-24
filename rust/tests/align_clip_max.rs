// §10-parameters · P.eng.alignClipMaxSeconds — sanity ceiling on a returned second (120 s; prototype
// `alignClipMax`, gui/align.go:230, applied at gui/align.go:220). Some aligners write a bare `start`
// that holds SAMPLES and never say so. Below the ceiling a bare number is seconds; past it the number
// cannot be seconds for any clip this app sends, so it is divided by the sample rate instead. Without
// this rule such an answer would place every word hours into the recording.
//
// What this file pins: the value and its single catalogue row; a bare number past the ceiling being read
// as samples; the comparison being strict so the ceiling itself still means seconds; the ceiling sitting
// above every legitimate value so a real "second" can never trip it while the suffixed keys are never
// ceiling-checked at all; and the neighbouring constants that share its neighbourhood in `align.rs`,
// told apart by home rather than by proximity.
//
// Everything here is pure arithmetic on the public `align::parse_words` -- no UI, no files. The
// time-reading helper `align::time()` is private, so each spelling is asserted through the parse that
// uses it and converted back with `transcribe::seconds`.

use naivepost::align;
use naivepost::params;
use naivepost::transcribe::{self, SAMPLE_RATE};
use serde_json::json;

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

/// A complete entry whose `start` is set to `value`; `end` is pinned at 1.0 s so the entry parses and
/// only the start's reading is under test. `parse_words` skips an entry that lacks EITHER time, so a
/// bare `{"word": ..., "start": ...}` alone yields no words at all -- hence the explicit end.
fn starting_at(value: f64) -> serde_json::Value {
    json!({ "word": "a", "start": value, "end": 1.0 })
}

/// One entry's `start`, read the way the aligner reads it, in seconds.
fn start_secs(entry: serde_json::Value) -> f64 {
    let body = json!({ "words": [entry] });
    let words = align::parse_words(&body).expect("one word with a start");
    transcribe::seconds(words[0].start_sample)
}

/// S1: `P.eng.alignClipMaxSeconds` = 120, held by `align::PLAIN_AS_SAMPLES`, catalogued once under
/// the family §10 files it in.
#[test]
fn p_eng_alignclipmaxseconds_s1_the_ceiling_is_120_and_catalogued_once() {
    // P.eng.alignClipMaxSeconds -- "sanity ceiling on a returned second".
    assert_eq!(align::PLAIN_AS_SAMPLES, 120.0);

    let row = params::prepare()
        .into_iter()
        .find(|param| param.id == "P.eng.alignClipMaxSeconds")
        .expect("the ceiling must be catalogued for Prepare, whose align pass reads it");
    assert_eq!(
        row.from,
        "align::PLAIN_AS_SAMPLES",
        "a row must name the constant its rule reads (the prototype called it alignClipMax)"
    );
    // num() takes the integer branch for a whole value, which is how §10 spells this one: "120".
    assert_eq!(row.spelled, "120");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 120.0);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.alignClipMaxSeconds"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists -- one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.alignClipMaxSeconds").count(),
        1,
        "P.eng.alignClipMaxSeconds catalogued more than once"
    );
}

/// S2: past the ceiling the bare number was not seconds. This is the whole point of the rule -- an
/// aligner that writes `start` meaning samples but does not say so is rescued by nothing else.
#[test]
fn p_eng_alignclipmaxseconds_s2_past_the_ceiling_the_number_was_samples() {
    // 32000 and 48000 are far past 120, so they are read as samples: 2.0 s and 3.0 s. Taken at face
    // value as seconds they would have placed the word eight hours into the recording.
    let word = align::parse_words(&json!({
        "words": [{ "word": "a", "start": 32000.0, "end": 48000.0 }]
    }))
    .expect("a mislabelled sample count still parses");
    assert_eq!(transcribe::seconds(word[0].start_sample), 2.0, "start read as samples");
    assert_eq!(transcribe::seconds(word[0].end_sample), 3.0, "end read as samples");
    // Stored as samples, so the rescue is exact rather than a rounded approximation.
    assert_eq!(word[0].start_sample, 32_000);
    assert_eq!(word[0].end_sample, 48_000);

    // Both keys get the treatment independently: a start below the ceiling stays seconds while an end
    // above it is divided. The rule judges each number on its own size, not on its sibling.
    let mixed = align::parse_words(&json!({
        "words": [{ "word": "a", "start": 5.0, "end": 32000.0 }]
    }))
    .expect("mixed units parse");
    assert_eq!(transcribe::seconds(mixed[0].start_sample), 5.0, "the small one stayed seconds");
    assert_eq!(transcribe::seconds(mixed[0].end_sample), 2.0, "the large one became samples");
}

/// S3: at and below the ceiling a bare number means seconds, and the comparison is strict -- "past the
// ceiling" excludes the ceiling itself, as with `align::CUT_SEEK`'s reach.
#[test]
fn p_eng_alignclipmaxseconds_s3_at_or_below_the_ceiling_it_means_seconds() {
    assert_eq!(start_secs(starting_at(1.0)), 1.0, "a plain second");
    assert_eq!(
        start_secs(starting_at(60.0)),
        60.0,
        "60 s is well inside the ceiling and is NOT divided"
    );

    // The boundary, measured: 120.0 sits ON the ceiling and `>` is strict, so it survives as seconds.
    assert_eq!(
        start_secs(starting_at(120.0)),
        120.0,
        "exactly the ceiling is not past it"
    );
    // One tenth over crosses, and is then divided by the sample rate. Note the value that comes back is
    // NOT 120.1/16000: the time is converted to whole samples on the way in (`to_sample`, rounding to
    // the nearest sample), so 0.00750625 s becomes 120 samples and reads back as exactly 0.0075 s.
    // The sub-sample part of the original number is lost, which is fine -- one tenth of a second at
    // this scale is far below anything a cut could land on.
    let crossed = start_secs(starting_at(120.1));
    assert_eq!(crossed, 120.0 / SAMPLE_RATE as f64, "divided by the rate, rounded to a sample");
    assert!((crossed - 0.0075).abs() < 1e-12, "0.0075 s after sample rounding: got {crossed}");
    assert_ne!(crossed, 120.1, "it was not kept as seconds");

    // The two sides of the boundary differ by four orders of magnitude, which is what makes the strict
    // comparison safe: nothing legitimately lands within a tenth of a second of the ceiling.
    assert!(
        (120.0 / crossed) > 10_000.0,
        "at 120.0 vs 120.1 the readings diverge hugely: {} vs {}",
        120.0,
        crossed
    );
}

/// S4: the ceiling sits above every legitimate value, so a real "second" can never trip it -- and it is
/// checked ONLY on the bare key, never on a key that already states its unit.
#[test]
fn p_eng_alignclipmaxseconds_s4_no_legitimate_second_reaches_the_ceiling() {
    // The longest clip the app sends is WINDOW (60 s, P.machine.alignChunkMaxSeconds); after F1.5's
    // halving the floor is MIN_PIECE (15 s). So the largest legitimate bare seconds on any request is
    // 60 -- half the ceiling, which is what "comfortably past" means here.
    assert_eq!(align::WINDOW, 60.0);
    assert_eq!(align::MIN_PIECE, 15.0);
    assert!(
        align::WINDOW < align::PLAIN_AS_SAMPLES,
        "the longest clip is under the ceiling: {} < {}",
        align::WINDOW,
        align::PLAIN_AS_SAMPLES
    );
    assert!(
        align::MIN_PIECE < align::PLAIN_AS_SAMPLES,
        "and so is the halved floor"
    );
    assert_eq!(
        align::PLAIN_AS_SAMPLES / align::WINDOW,
        2.0,
        "exactly double the longest clip"
    );

    // A legitimate 60 s bare value therefore reads as seconds, untouched.
    assert_eq!(start_secs(starting_at(align::WINDOW)), 60.0);

    // The ceiling applies ONLY to the bare key. A key that says "sample" is divided unconditionally, so
    // 1_920_000 samples (= 120 s) reads as 120 s with no ceiling involved...
    let stated_samples = align::parse_words(&json!({
        "words": [{ "word": "a", "start_sample": 1_920_000u64, "end_sample": 1_920_001u64 }]
    }))
    .expect("stated samples parse");
    assert!(
        (transcribe::seconds(stated_samples[0].start_sample) - 120.0).abs() < 1e-9,
        "a stated sample count is divided regardless of size"
    );
    // ...and a stated millisecond count likewise: 120_000 ms = 120 s, no ceiling consulted.
    let stated_ms = align::parse_words(&json!({
        "words": [{ "word": "a", "start_ms": 120_000.0, "end_ms": 121_000.0 }]
    }))
    .expect("stated millis parse");
    assert!(
        (transcribe::seconds(stated_ms[0].start_sample) - 120.0).abs() < 1e-9,
        "120000 ms reads as 120 s without the ceiling"
    );
    // Contrast with the bare spelling of the same instant, which DOES cross and gets divided -- proof the
    // ceiling only ever acts on the ambiguous key.
    assert_ne!(
        start_secs(starting_at(120_000.0)),
        120.0,
        "the same number written bare is treated as samples"
    );

    // And the error path: the ceiling rescues a MISLABELLED number, not a MISSING one. An entry with no
    // time keys at all is skipped, and a body with no usable words errors.
    let missing = align::parse_words(&json!({ "words": [{ "word": "a" }] }));
    assert_eq!(missing.unwrap_err(), "no words in the answer", "a missing time is not rescued");
}

/// S5: the neighbours. Several constants live next door in `align.rs` or share a magnitude; the ids and
/// homes are what keep the rules apart.
#[test]
fn p_eng_alignclipmaxseconds_s5_neighbours_by_home_not_by_proximity() {
    // P.machine.alignChunkMaxSeconds (60, `align::WINDOW`) caps the CLIP SENT to an aligner. This
    // item's 120 caps the NUMBER RETURNED. Related by the ratio above, different questions entirely.
    let chunk_max = params::find("P.machine.alignChunkMaxSeconds").expect("alignChunkMaxSeconds is catalogued");
    assert_eq!(chunk_max.from, "align::WINDOW");
    assert_eq!(chunk_max.spelled, "60");
    assert_ne!(chunk_max.from, "align::PLAIN_AS_SAMPLES", "distinct homes for distinct rules");
    assert!(align::WINDOW < align::PLAIN_AS_SAMPLES);

    // align::PAD (0.25) and align::CUT_SEEK (4.0) belong to the pad and reach rules, not to unit
    // inference -- orders of magnitude smaller than the ceiling.
    assert_eq!(align::PAD, 0.25);
    assert_eq!(align::CUT_SEEK, 4.0);
    assert!(align::PAD < align::CUT_SEEK, "the pad is smaller than the reach");
    assert!(
        align::CUT_SEEK < align::PLAIN_AS_SAMPLES,
        "and both are dwarfed by the ceiling: {} << {}",
        align::CUT_SEEK,
        align::PLAIN_AS_SAMPLES
    );

    // P.eng.asrSampleRate (16000, `transcribe::SAMPLE_RATE`) is the divisor the rescue uses: the
    // ceiling decides THAT a number is samples, the rate decides how many seconds they are.
    let rate = params::find("P.eng.asrSampleRate").expect("asrSampleRate is catalogued");
    assert_eq!(rate.from, "transcribe::SAMPLE_RATE");
    assert_eq!(SAMPLE_RATE, 16_000);
    // Round-trip a rescued value: 32000 written bare -> 2.0 s -> 32000 samples again.
    let rescued = align::parse_words(&json!({ "words": [{ "word": "a", "start": 32000.0, "end": 32001.0 }] }))
        .expect("rescued");
    let as_seconds = transcribe::seconds(rescued[0].start_sample);
    assert_eq!(as_seconds, 32_000.0 / SAMPLE_RATE as f64, "divided by the rate");
    assert_eq!(rescued[0].start_sample, 32_000, "and lands back on the original sample count");

    // The ceiling is far larger than the rate's per-second scale in the other direction too: a bare
    // number equal to SAMPLE_RATE (16000) is past the ceiling and reads as 1.0 s, not 16000 s.
    assert!(
        SAMPLE_RATE as f64 > align::PLAIN_AS_SAMPLES,
        "one second's worth of samples always trips the ceiling"
    );
    assert_eq!(start_secs(starting_at(16_000.0)), 1.0);
}
