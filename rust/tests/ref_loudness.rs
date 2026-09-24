// §10-parameters · P.eng.refLoudness — reference levelling ("I −16, TP −1.5, LRA 7"; prototype
// `refLoud`, gui/narrate_voice.go:208, rationale at gui/narrate_voice.go:204-207). The voice
// reference is levelled before the model ever hears it, because a clone is only as loud as its source:
// a quietly spoken take, level-matched against already-loudnorm'd game audio, comes back as somebody
// murmuring. Single-pass (dynamic) `loudnorm` is used because the takes come from different minutes of
// the session rather than one continuous stretch, and the loudness range is kept tight because it is
// one person talking, not a mixed program.
//
// What this file pins: the value and its single catalogue row; the three knobs read back out of the
// constant itself; how far this target sits from the FINAL MIX's target and why in each dimension; that
// the filter carries no rate work because the sibling constant pins the rate; and the neighbouring
// controls in the render, told apart by home.

use naivepost::narrate_data;
use naivepost::params;
use naivepost::produce_render;
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

/// The three numbers a `loudnorm` filter sets, keyed by the name ffmpeg uses.
fn knobs(filter: &str) -> Vec<(String, f64)> {
    let tail = filter.split("loudnorm=").nth(1).unwrap_or_default();
    tail.split(':')
        .filter_map(|pair| {
            let mut halves = pair.splitn(2, '=');
            let key = halves.next()?.to_string();
            let value = halves.next()?.parse::<f64>().ok()?;
            Some((key, value))
        })
        .collect()
}

/// One knob's value by name, panicking if the filter does not set it.
fn knob(filter: &str, name: &str) -> f64 {
    knobs(filter)
        .into_iter()
        .find(|(key, _)| key == name)
        .unwrap_or_else(|| panic!("{filter} sets no {name}"))
        .1
}

/// S1: `P.eng.refLoudness` = "I -16, TP -1.5, LRA 7", held by `narrate_data::REF_LOUDNESS`,
/// catalogued once under the family §10 files it in.
#[test]
fn p_eng_refloudness_s1_the_target_is_i16_tp15_lra7_and_catalogued_once() {
    // P.eng.refLoudness -- "reference levelling".
    assert_eq!(narrate_data::REF_LOUDNESS, "loudnorm=I=-16:TP=-1.5:LRA=7");

    let row = row_for("P.eng.refLoudness");
    assert_eq!(
        row.from,
        "narrate_data::REF_LOUDNESS",
        "a row must name the constant its rule reads (the prototype called it refLoud)"
    );
    // ASCII hyphen-minus, matching how the existing P.eng.loudness row spells its target: the spelling
    // is generated from the filter string, which ffmpeg needs in ASCII anyway. §10 prints a typographic
    // minus, which is presentation only.
    assert_eq!(row.spelled, "I -16, TP -1.5, LRA 7");
    assert!(!row.spelled.contains('\u{2212}'), "no unicode minus in a generated row");

    // §10 files it among the engineering constants, so the prefix answers to Eng.
    assert_eq!(
        params::family("P.eng.refLoudness"),
        params::Family::Eng,
        "§10 files it among the engineering constants"
    );

    // And exactly one row for it across all five lists -- one row, one home.
    assert_eq!(
        all_rows().iter().filter(|param| param.id == "P.eng.refLoudness").count(),
        1,
        "P.eng.refLoudness catalogued more than once"
    );
}

/// S2: all three numbers read back out of the constant, and nothing else is being asked of the filter.
#[test]
fn p_eng_refloudness_s2_three_knobs_read_back_out_of_the_constant() {
    // These are the only three knobs loudnorm is being asked to set for the reference: integrated
    // loudness, true peak, and loudness range.
    let parsed = knobs(narrate_data::REF_LOUDNESS);
    assert_eq!(parsed.len(), 3, "exactly three pairs: {parsed:?}");
    assert_eq!(knob(narrate_data::REF_LOUDNESS, "I"), -16.0, "integrated loudness");
    assert_eq!(knob(narrate_data::REF_LOUDNESS, "TP"), -1.5, "true peak ceiling");
    assert_eq!(knob(narrate_data::REF_LOUDNESS, "LRA"), 7.0, "loudness range");

    // The catalogue row says each of them, so a reader gets all three without opening the filter.
    let spelled = row_for("P.eng.refLoudness").spelled;
    assert!(spelled.contains("I -16"), "{spelled}");
    assert!(spelled.contains("TP -1.5"), "{spelled}");
    assert!(spelled.contains("LRA 7"), "{spelled}");
    // And in the order the filter declares them, since the row is generated from the string rather than
    // written by hand.
    assert_eq!(
        spelled,
        parsed
            .iter()
            .map(|(key, value)| format!("{key} {value}"))
            .collect::<Vec<_>>()
            .join(", "),
        "the row is the filter, respaced"
    );
}

/// S3: distinct from the FINAL MIX's target, and by an explained amount in each dimension.
#[test]
fn p_eng_refloudness_s3_two_decibels_quieter_than_the_mix_and_a_tighter_range() {
    let reference = narrate_data::REF_LOUDNESS;
    let mix = produce_render::LOUDNORM;
    assert_ne!(reference, mix, "two targets, two rules");

    // 2 dB quieter than the final mix. Levelling the reference below the delivery target leaves the mix
    // headroom to bring the narration up without leaning on the limiter.
    assert_eq!(knob(reference, "I"), -16.0);
    assert_eq!(knob(mix, "I"), -14.0);
    assert_eq!(
        knob(mix, "I") - knob(reference, "I"),
        2.0,
        "the reference sits exactly 2 dB under the mix's integrated target"
    );

    // Same true-peak ceiling: both stay clear of digital full scale by the same margin, so neither
    // clips however the material is later turned up.
    assert_eq!(
        knob(reference, "TP"),
        knob(mix, "TP"),
        "identical peak headroom"
    );
    assert_eq!(knob(reference, "TP"), -1.5);

    // A range four units tighter. The reference is one steady voice being cloned, so a narrow LRA tells
    // the model "one speaker" instead of a program with quiet passages and loud ones.
    assert_eq!(knob(reference, "LRA"), 7.0);
    assert_eq!(knob(mix, "LRA"), 11.0);
    assert_eq!(
        knob(mix, "LRA") - knob(reference, "LRA"),
        4.0,
        "the reference's range is 4 units tighter than the mix's"
    );

    // Different homes in the catalogue: the mix's row reads the render's constant, this one the
    // reference's.
    assert_eq!(row_for("P.eng.loudness").from, "produce_render::LOUDNORM");
    assert_eq!(row_for("P.eng.refLoudness").from, "narrate_data::REF_LOUDNESS");
    assert_ne!(
        row_for("P.eng.loudness").from,
        row_for("P.eng.refLoudness").from,
        "two loudness targets, two owners"
    );
}

/// S4: the filter carries no rate work -- pinning the sample rate is the sibling constant's job.
#[test]
fn p_eng_refloudness_s4_no_rate_work_in_the_filter() {
    // The mix's filter resamples first so loudnorm's two passes see a constant rate. The reference's
    // does not: its rate is pinned separately, because loudnorm internally works at 192 kHz and writing
    // the reference at that rate would earn the extensible WAV header the audio server refuses.
    assert!(
        !narrate_data::REF_LOUDNESS.contains("aresample"),
        "the reference filter leaves the rate alone: {}",
        narrate_data::REF_LOUDNESS
    );
    assert!(
        produce_render::LOUDNORM.contains("aresample"),
        "the mix's filter does its own resample: {}",
        produce_render::LOUDNORM
    );

    // Which is exactly what REF_SAMPLE_RATE exists to settle.
    assert_eq!(narrate_data::REF_SAMPLE_RATE, "48000");
    assert!(
        narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap() < 192_000.0,
        "pinned well under loudnorm's internal rate"
    );

    // Both constants belong to the same module and are separate rows with separate ids.
    let rate_row = row_for("P.eng.refSampleRate");
    let loud_row = row_for("P.eng.refLoudness");
    assert!(rate_row.from.starts_with("narrate_data::"), "{}", rate_row.from);
    assert!(loud_row.from.starts_with("narrate_data::"), "{}", loud_row.from);
    assert_ne!(rate_row.id, loud_row.id, "different rows");
    assert_ne!(rate_row.from, loud_row.from, "different constants");
}

/// S5: the neighbours. Other audio controls sit near this one; the ids and homes keep them apart.
#[test]
fn p_eng_refloudness_s5_neighbours_by_home_not_by_number() {
    // The clip limiter is a hard ceiling, not a loudness target: it caps peaks rather than aiming at an
    // integrated level, so it has none of the three knobs this rule sets.
    let limiter = row_for("P.eng.clipLimiter");
    assert_eq!(limiter.from, "produce_render::LIMITER");
    assert!(
        produce_render::LIMITER.contains("alimiter") || produce_render::LIMITER.contains("limit="),
        "a limiter control: {}",
        produce_render::LIMITER
    );
    assert!(
        knobs(produce_render::LIMITER).is_empty(),
        "the limiter sets no loudnorm knobs, so it cannot be confused with a loudness target: {:?}",
        knobs(produce_render::LIMITER)
    );
    assert_ne!(limiter.from, row_for("P.eng.refLoudness").from, "different homes");
    assert_ne!(limiter.from, row_for("P.eng.loudness").from, "and different from the mix's");

    // The two sample rates: transcription quality versus clone quality.
    let asr_rate = row_for("P.eng.asrSampleRate");
    let ref_rate = row_for("P.eng.refSampleRate");
    assert_eq!(asr_rate.from, "transcribe::SAMPLE_RATE");
    assert_eq!(ref_rate.from, "narrate_data::REF_SAMPLE_RATE");
    assert_ne!(asr_rate.from, ref_rate.from, "two rates, two owners");
    assert!(
        narrate_data::REF_SAMPLE_RATE.parse::<f64>().unwrap() > transcribe::SAMPLE_RATE as f64,
        "the reference is written well above transcription quality"
    );

    // The reference levelling row is not the final mix row, in either direction.
    assert_ne!(
        row_for("P.eng.refLoudness").id,
        row_for("P.eng.loudness").id,
        "reference levelling and the final mix are separate ledger items"
    );
    assert_eq!(row_for("P.eng.refLoudness").id, "P.eng.refLoudness");
    assert_eq!(row_for("P.eng.loudness").id, "P.eng.loudness");
}
