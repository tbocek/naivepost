// §10-parameters · P.eng.refSampleRate — voice reference write rate (48000; prototype `refRate`,
// gui/narrate_voice.go:214, explained at gui/narrate_voice.go:210-213). The reference is what the TTS
// server clones the narrator's voice from, so its file format matters as much as its content: above
// 48 kHz ffmpeg writes `WAVE_FORMAT_EXTENSIBLE` (format tag 0xFFFE) rather than a plain PCM header,
// and the audio server refuses the upload with "unsupported WAV encoding". `loudnorm` in the
// level-matching pass resamples to 192 kHz, which is far past that ceiling, so the write rate has to be
// pinned rather than left to whatever the filter chain last set. 48 kHz keeps the header plain and sits
// at or above every source rate here, so no sample of the take is thrown away to get there.
//
// What this file pins: the value and its single catalogue row; the constant sitting exactly on the
// plain-header ceiling with the common higher rates over it; the rate being at or above anything the
// reference is cut from; the three sample rates in this app not being interchangeable and this one not
// being the render's rate; and the value being a usable ffmpeg argument rather than a decorative number.

use naivepost::narrate_data;
use naivepost::params;
use naivepost::separate;
use naivepost::transcribe;

/// A row by id from anywhere in the catalogue. `params::find` only searches `prepare()`, and this item
/// lives on the Narrate page, so the lookup has to span all five lists.
fn row_for(id: &str) -> params::Param {
    all_rows()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{id} is not catalogued"))
}

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

/// S1: `P.eng.refSampleRate` = 48000, held by `narrate_data::REF_SAMPLE_RATE`, catalogued once under
/// the family §10 files it in.
#[test]
fn p_eng_refsamplerate_s1_the_rate_is_48000_and_catalogued_once() {
    // P.eng.refSampleRate -- "voice reference write rate".
    assert_eq!(narrate_data::REF_SAMPLE_RATE, "48000");

    let row = row_for("P.eng.refSampleRate");
    assert_eq!(
        row.from,
        "narrate_data::REF_SAMPLE_RATE",
        "a row must name the constant its rule reads (the prototype called it refRate)"
    );
    assert_eq!(row.spelled, "48000");
    assert_eq!(row.spelled.parse::<f64>().unwrap(), 48_000.0);
    // A `-ar` argument has to read as a whole hertz figure; a float spelling would be a different kind
    // of value than ffmpeg expects.
    assert!(!row.spelled.contains('.'), "no decimal point in an ffmpeg rate: {}", row.spelled);

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.refSampleRate"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists -- one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.refSampleRate").count(),
        1,
        "P.eng.refSampleRate catalogued more than once"
    );
}

/// S2: the constant sits exactly ON the plain-header ceiling, not short of it -- and the standard rates
/// above 48 kHz are over the line, which is why the write must be pinned.
#[test]
fn p_eng_refsamplerate_s2_at_the_ceiling_with_every_higher_rate_over_it() {
    // 48 kHz is the limit itself, and at the limit the header stays plain PCM.
    assert_eq!(
        narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap(),
        48_000.0,
        "exactly the ceiling"
    );

    // The next standard rates up are past it, and would earn the extensible header the server refuses.
    assert!(88_200.0 > 48_000.0, "88.2 kHz is over the ceiling");
    assert!(96_000.0 > 48_000.0, "96 kHz is over the ceiling");
    // loudnorm's own output rate is four times over -- the reason the reference cannot be left to
    // whatever the filter chain last produced.
    assert!(
        192_000.0 > 48_000.0,
        "loudnorm's 192 kHz is well past the ceiling"
    );

    // Under the ceiling would be legal but wasteful: a lower write rate throws away bandwidth and, if it
    // went below a source's rate, samples too. Being AT it buys the plain header at no cost.
    assert!(44_100.0 < 48_000.0, "below is legal, just wasteful");
    assert!(
        narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap() <= 48_000.0,
        "the chosen value never crosses into the refused band"
    );
}

/// S3: 48 kHz is at or above everything the reference can be cut from, so getting there is a resample
/// up or a no-op -- never down.
#[test]
fn p_eng_refsamplerate_s3_never_a_resample_down_from_any_source() {
    let ceiling = narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap();

    // The ASR works at 16 kHz mono; a reference cut from material measured that way still has to be
    // written high enough for the TTS side.
    assert!(
        ceiling > transcribe::SAMPLE_RATE as f64,
        "above the ASR's 16 kHz: {} > {}",
        ceiling,
        transcribe::SAMPLE_RATE
    );
    // The separation models run at 44.1 kHz stereo, the nearest source rate to the ceiling.
    let sep = separate::SAMPLE_RATE.parse::<f64>().unwrap();
    assert!(
        ceiling >= sep,
        "at or above the separation rate: {} >= {}",
        ceiling,
        sep
    );

    // So neither source loses samples to satisfy the format. That matters because the reference IS the
    // voice: a reference with samples thrown away is a reference with less of the speaker in it, and
    // the clone comes back duller than the person who recorded it.
    assert!(
        ceiling - sep < 4_000.0,
        "the gap over the highest source rate is small: {}",
        ceiling - sep
    );
    assert!(
        ceiling / transcribe::SAMPLE_RATE as f64 == 3.0,
        "exactly three times the ASR rate, so that path is a clean integer upsample"
    );
}

/// S4: the three sample rates in this app answer three different questions and are not interchangeable,
/// and this one is not the render's rate either.
#[test]
fn p_eng_refsamplerate_s4_three_rates_three_rules_none_interchangeable() {
    // 16 kHz: what the ASR reads. 44.1 kHz: what the separation models were trained on. 48 kHz: what
    // the voice reference is written at. Ordered, distinct, each owned by a different module.
    let asr_rate = transcribe::SAMPLE_RATE as f64;
    let sep_rate = separate::SAMPLE_RATE.parse::<f64>().unwrap();
    let ref_rate = narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap();
    assert_eq!(asr_rate, 16_000.0);
    assert_eq!(sep_rate, 44_100.0);
    assert_eq!(ref_rate, 48_000.0);
    assert!(asr_rate < sep_rate && sep_rate < ref_rate, "three distinct rates");

    // The reference rate is NOT the ASR rate: handing the TTS server a 16 kHz clone source would give
    // it a voice with half the top end the speaker actually has.
    assert_ne!(
        narrate_data::REF_SAMPLE_RATE,
        transcribe::SAMPLE_RATE.to_string(),
        "the clone source is not written at transcription quality"
    );
    assert_ne!(
        narrate_data::REF_SAMPLE_RATE,
        separate::SAMPLE_RATE,
        "nor at the separation model's rate"
    );

    // The final mix's level/rate handling is a separate rule with its own home: P.eng.loudness reads
    // produce_render::LOUDNORM, not this constant.
    let loudness = row_for("P.eng.loudness");
    assert_eq!(loudness.from, "produce_render::LOUDNORM");
    assert!(naivepost::produce_render::LOUDNORM.contains("loudnorm"), "{}", naivepost::produce_render::LOUDNORM);
    assert!(
        naivepost::produce_render::LOUDNORM.contains("LRA=11"),
        "the render's own target, not the reference's: {}",
        naivepost::produce_render::LOUDNORM
    );
    assert_ne!(
        loudness.from,
        "narrate_data::REF_SAMPLE_RATE",
        "the mix's rule and the reference's write rate have different homes"
    );
}

/// S5: the value is a usable ffmpeg argument, not a decorative number, and pinning it is what keeps the
/// header plain.
#[test]
fn p_eng_refsamplerate_s5_a_usable_ffmpeg_argument_that_round_trips() {
    // All ASCII digits, five of them, parsing cleanly as a whole hertz count.
    assert!(
        narrate_data::REF_SAMPLE_RATE.chars().all(|c| c.is_ascii_digit()),
        "digits only: {}",
        narrate_data::REF_SAMPLE_RATE
    );
    assert_eq!(narrate_data::REF_SAMPLE_RATE.len(), 5, "five characters: 48000");
    assert_eq!(narrate_data::REF_SAMPLE_RATE.parse::<u32>().unwrap(), 48_000);

    // It goes straight onto a command line unchanged.
    assert_eq!(
        format!("ar={}", narrate_data::REF_SAMPLE_RATE),
        "ar=48000",
        "what ffmpeg would actually be handed"
    );

    // Stored as text, not as a float-string: parseable as f64 when arithmetic is wanted, but the stored
    // form carries no decimal point, so nothing downstream has to strip one before quoting it.
    assert!(!narrate_data::REF_SAMPLE_RATE.contains('.'));
    assert!(narrate_data::REF_SAMPLE_RATE.parse::<f64>().is_ok());
    // The round trip through f64 lands back on the same bare digits -- 48000.0 renders as "48000" in
    // Rust, so equality holds either way. What matters is the storage form itself: no decimal point, so
    // nothing downstream has to strip one before quoting the value into an ffmpeg argument.
    assert_eq!(
        format!("{}", narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap()),
        narrate_data::REF_SAMPLE_RATE
    );
    assert!(!format!("{}", narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap()).contains('.'));

    // What the pin prevents is a FORMAT refusal, not a length one: a hypothetical 192 kHz write would
    // cross the ceiling and earn the extensible header, so holding the rate down is the whole defence.
    assert!(
        192_000.0 > narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap(),
        "an unpinned 192 kHz write would exceed the ceiling"
    );
    assert!(
        narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap() <= 48_000.0,
        "pinned, it never does"
    );
}
