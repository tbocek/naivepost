//! §07-narrate#4-parameters-used — every parameter the Narrate page may be tuned by, spelled for the
//! catalogue in its own file because [`crate::params`] is at its size budget. The section moved whole:
//! its ids, their spellings and their order are exactly what `params.rs` used to build here, and nothing
//! in this module owns a number — every row reads the constant its rule reads, so moving a bound in that
//! module moves the row with it and a row that stops matching its home fails
//! `tests/suite/narrate_parameters.rs`.
//!
//! No GTK, and nothing computed: like the rest of `params`, this only spells what lives elsewhere.

use crate::narrate_data;
use crate::narrate_pass;
use crate::narrate_preview;
use crate::narrate_screen;
use crate::narrate_tts;
use crate::params::{loudnorm_spelled, num, param, Param};
use crate::produce_render;
use crate::project;
use crate::voice_ref;

/// Every number the Narrate page draws or waits with, in §1's order: the take band first (its floor, its label
/// bound, its geometry), then the pitch slider, then the two clocks and the two debounces.
///
/// The ids are of two kinds, and each says so. `P.eng.*` rows are §10's own; the bare `narrate.*` ones are the
/// numbers §10 lists only inside its prose line (`Narrate take band: ruler 12, lane 56, max 200 px/s, click slop 3
/// px`, and §07's `Engineering: tick 100 ms · seek debounce 120 ms · autosave 400 ms`), which have no `P.` row to
/// inherit — the same bare-prefix rule §06-effects#6 took for `machine.` and `preview.` values. They are spelled as
/// §10 spells them so a reader comparing this list against that line is comparing the same words.
///
/// Every §4 item now has a row. Two of them are catalogued under an id other than the one §10's table prints,
/// and say so rather than being quietly moved: `P.eng.speechCharsPerSecond` (§10:194, "15 (8..28)") lives under
/// the id §07-narrate#1-screen chose, `narrate.speechCharsPerSecond`, with its band spelled in the same row;
/// and F4.3's packing, growing and speeding (`P.eng.narrationLeadSeconds`, `P.eng.narrationGapSeconds`,
/// `P.eng.narrationMaxTempo`) are catalogued here because §07's §4 names them, while the rule that reads them is
/// the render's ([`produce_render::fit`]) — the same one-home-per-number rule [`effects`] gives for values whose
/// section differs from their owner's.
pub fn rows() -> Vec<Param> {
    vec![
        // --- the take band (§1's number 10) ---------------------------------------------------------
        param(
            "P.eng.takeMinSeconds",
            num(narrate_screen::TAKE_MIN_SECONDS),
            "narrate_screen::TAKE_MIN_SECONDS",
        ),
        param(
            "narrate.takeLabelPx",
            num(narrate_screen::TAKE_LABEL_MIN_PX),
            "narrate_screen::TAKE_LABEL_MIN_PX",
        ),
        // §10:133 lists the band's geometry by area rather than as rows, so these four carry no `P.` id.
        param(
            "narrate.bandRulerPx",
            num(narrate_screen::BAND_RULER_PX),
            "narrate_screen::BAND_RULER_PX",
        ),
        param(
            "narrate.bandLanePx",
            num(narrate_screen::BAND_LANE_PX),
            "narrate_screen::BAND_LANE_PX",
        ),
        param(
            "narrate.bandMaxPps",
            num(narrate_screen::BAND_MAX_PPS),
            "narrate_screen::BAND_MAX_PPS",
        ),
        param(
            "narrate.clickSlopPx",
            num(narrate_screen::CLICK_SLOP_PX),
            "narrate_screen::CLICK_SLOP_PX",
        ),
        // --- the pitch slider (§1's number 13) ------------------------------------------------------
        // §10 gives one row to the whole range, so the row spells both ends and the step between them.
        param(
            "P.eng.pitchRangeSemitones",
            format!(
                "{}..+{}, step {}",
                num(narrate_screen::PITCH_MIN_SEMITONES),
                num(narrate_screen::PITCH_MAX_SEMITONES),
                num(narrate_screen::PITCH_STEP)
            ),
            "narrate_screen::PITCH_MIN_SEMITONES + PITCH_MAX_SEMITONES + PITCH_STEP",
        ),
        // --- what the page waits for (§1's numbers 5, 9, 20) -----------------------------------------
        param(
            "narrate.tickMs",
            narrate_screen::TICK_MS.to_string(),
            "narrate_screen::TICK_MS",
        ),
        param(
            "narrate.seekDebounceMs",
            narrate_screen::SEEK_DEBOUNCE_MS.to_string(),
            "narrate_screen::SEEK_DEBOUNCE_MS",
        ),
        // The autosave is §10:133's `typing debounce 400 ms`, and the timer itself is the shell's.
        param(
            "narrate.autosaveMs",
            crate::shell::NARRATION_AUTOSAVE.as_millis().to_string(),
            "shell::NARRATION_AUTOSAVE",
        ),
        // --- what a row says about a line that is not spoken yet (§1's numbers 15, 21) -----------------
        param(
            "P.eng.narrationTailSeconds",
            num(narrate_screen::SPEECH_TAIL_SECONDS),
            "narrate_screen::SPEECH_TAIL_SECONDS",
        ),
        // §C.2's fallback rate and §F4.3's clamp band, in the one row §10 gives them: the estimate is what both
        // the row's warning and the transport ＋'s refusal are built on until a take exists to measure, and once
        // it does, `narrate_page::speech_for` measures the rate off the wav and pushes it through 8..28.
        param(
            "narrate.speechCharsPerSecond",
            speech_rate_spelled(),
            "narrate_screen::SPEECH_CHARS_PER_SECOND + CHAR_RATE_MIN + CHAR_RATE_MAX",
        ),
        // F4.7's "within 1 s of a line": the window in which ＋ jumps to a line instead of adding one.
        param(
            "narrate.addNearSeconds",
            num(narrate_screen::ADD_NEAR_SECONDS),
            "narrate_screen::ADD_NEAR_SECONDS",
        ),
        // --- what F4.2's call asks for (§10 puts these in §1's machine table and §2's policy table, and the
        // narration call is the only reader of all four: how far a clip's block reaches, and the word budget
        // it prints) -----------------------------------------------------------------------------------------------
        param(
            "P.machine.narrationContextSeconds",
            num(narrate_pass::CONTEXT_SECONDS),
            "narrate_pass::CONTEXT_SECONDS",
        ),
        param(
            "P.policy.narrationMinWords",
            num(narrate_pass::MIN_WORDS as f64),
            "narrate_pass::MIN_WORDS",
        ),
        param(
            "P.policy.narrationMaxWords",
            num(narrate_pass::MAX_WORDS as f64),
            "narrate_pass::MAX_WORDS",
        ),
        param(
            "P.policy.narrationWordsPerSecond",
            num(narrate_pass::WORDS_PER_SECOND),
            "narrate_pass::WORDS_PER_SECOND",
        ),
        // --- what one line is spoken with -----------------------------------------------------------------------
        // §10 spells this default as "project language", so the row spells that; the constant named is the
        // fallback the prototype hard-coded, which is all a project without a language can be given.
        param(
            "P.policy.ttsLanguage",
            "project language".to_string(),
            narrate_tts::LANGUAGE_SOURCE,
        ),
        // --- P.eng.emotionAlpha (§4, and the same alpha the frozen cache key spells) ------------------------
        // F4.4 sends it for every take that is not an exact emotion vector; it rides in the request's
        // `options`, and §4 lists it among the parameters this page's narration uses.
        param(
            "P.eng.emotionAlpha",
            narrate_tts::EMOTION_ALPHA.to_string(),
            "narrate_tts::EMOTION_ALPHA",
        ),
        // --- what the preview holds and how far it looks ahead (§10 lines 191 and 193; F4.5 is the flow that
        // reads both: a boundary held while a line speaks, and where ▶ drops the picture for a line) -----
        param(
            "P.eng.narrationMaxExtendSeconds",
            num(narrate_preview::MAX_EXTEND_SECONDS),
            "narrate_preview::MAX_EXTEND_SECONDS",
        ),
        param(
            "P.eng.narrationRunInSeconds",
            num(narrate_screen::AUDITION_LEAD_SECONDS),
            "narrate_screen::AUDITION_LEAD_SECONDS",
        ),
        // --- F4.1 S4's toggle: off (the default) ▶ writes a line only for clips that have none; on,
        // every clip's line is rewritten. Spelled as §10 spells it, read from the project's own
        // default. §07's own §4 list does not name this id — it appears in that chapter only in F4.1's
        // flow diagram — so `narrate_parameters.rs`'s foreign-row whitelist carries it with that note.
        param(
            "P.policy.narrationRewrite",
            if project::Policy::default().narration_rewrite.value {
                "on".to_string()
            } else {
                "off".to_string()
            },
            "project::Policy::default — narration_rewrite",
        ),
        // §10's row for the rate the voice reference is written at: one constant answers both this row
        // and the write, so the catalogue cannot drift from what the server will accept.
        param(
            "P.eng.refSampleRate",
            narrate_data::REF_SAMPLE_RATE.to_string(),
            "narrate_data::REF_SAMPLE_RATE",
        ),
        // §10's row for the level the voice reference is written at: spelled straight out of the
        // filter the reference write applies, so the row cannot drift from what ffmpeg is sent.
        param(
            "P.eng.refLoudness",
            loudnorm_spelled(narrate_data::REF_LOUDNESS),
            "narrate_data::REF_LOUDNESS",
        ),
        // --- what the render does with a line that does not fit (§07 §4 names all three; the rule that reads
        // them is `produce_render::fit`, so each names its owner rather than this page) -----------------------
        param(
            "P.eng.narrationLeadSeconds",
            num(produce_render::LEAD_SECONDS),
            "produce_render::LEAD_SECONDS",
        ),
        param(
            "P.eng.narrationGapSeconds",
            num(produce_render::GAP_SECONDS),
            "produce_render::GAP_SECONDS",
        ),
        param(
            "P.eng.narrationMaxTempo",
            num(produce_render::MAX_TEMPO),
            "produce_render::MAX_TEMPO",
        ),
        // --- the automatic voice reference (§F4.6 S2): every one read by `voice_ref::plan_reference` --------
        param(
            "P.eng.refMinTakeSeconds",
            num(voice_ref::REF_MIN_TAKE_SECONDS),
            "voice_ref::REF_MIN_TAKE_SECONDS",
        ),
        param(
            "P.eng.refMinWordsPerSecond",
            num(voice_ref::REF_MIN_WORDS_PER_SECOND),
            "voice_ref::REF_MIN_WORDS_PER_SECOND",
        ),
        param(
            "P.machine.refWantSeconds",
            num(voice_ref::REF_WANT_SECONDS),
            "voice_ref::REF_WANT_SECONDS",
        ),
        param(
            "P.machine.refTakeMax",
            voice_ref::REF_TAKE_MAX.to_string(),
            "voice_ref::REF_TAKE_MAX",
        ),
        // --- §08 §4's fitting bound, read where the render applies it (the rest of F4.3 lives in §7) -------
        param(
            "P.eng.refPadSeconds",
            num(voice_ref::REF_PAD_SECONDS),
            "voice_ref::REF_PAD_SECONDS",
        ),
    ]
}

/// §10 spells this row as `15 (8..28)`: the default a line is estimated at, and the band a MEASURED rate is
/// clamped into (`narrate_screen::char_rate_clamped`). Spelling all three from the constants keeps the row from
/// drifting from either half of the rule.
fn speech_rate_spelled() -> String {
    format!(
        "{} ({}..{})",
        num(narrate_screen::SPEECH_CHARS_PER_SECOND),
        num(narrate_screen::CHAR_RATE_MIN),
        num(narrate_screen::CHAR_RATE_MAX)
    )
}
