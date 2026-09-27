//! F4.3 · Fit a line to its clip — the row's warning mirrors the render's ladder.
//!
//! The five numbers this section is made of (§10, and what `produce_render` holds them under):
//!   P.eng.narrationLeadSeconds      0.3  r::LEAD_SECONDS / narrate_screen::SPEECH_LEAD_SECONDS
//!   P.eng.narrationGapSeconds       0.3  r::GAP_SECONDS
//!   P.eng.narrationTailSeconds      0.2  r::TAIL_SECONDS == narrate_screen::SPEECH_TAIL_SECONDS
//!   P.eng.narrationMaxExtendSeconds 4    r::MAX_EXTEND_SECONDS
//!   P.eng.narrationMaxTempo         1.25 r::MAX_TEMPO
//!
//! Logic only, no display: every assertion goes through `narrate_screen::mirror_fit`, which calls
//! `produce_render::fit` itself, so the page and the render are checked against one ladder rather than two
//! copies of it.

use naivepost::narrate_screen::{self, Fit};
use naivepost::produce_render as r;

/// The page's own speech estimate for N characters at the default rate, so a test can state a fit in seconds
/// without re-deriving chars-per-second by hand.
fn spoken(chars: usize) -> f64 {
    narrate_screen::page_speech_seconds(chars, narrate_screen::SPEECH_CHARS_PER_SECOND)
}

#[test]
fn f4_3_s1_lines_pack_from_the_lead_and_never_before_the_previous_end_plus_the_gap() {
    // P.eng.narrationLeadSeconds 0.3: a line placed at 0.0 (or anywhere before the lead) starts AT the lead.
    let (placed, logs) = r::fit(&[(0, 0.0, 2.0)], 10.0);
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].at, r::LEAD_SECONDS, "the first line never starts before the lead");
    assert!(logs.is_empty(), "{logs:?}");

    // P.eng.narrationGapSeconds 0.3 + P.eng.narrationTailSeconds 0.2: the second line is held to the
    // previous end + gap + tail even though it was placed earlier than that.
    let (placed, _) = r::fit(&[(0, 0.0, 2.0), (1, 0.5, 2.0)], 20.0);
    assert_eq!(placed[0].at, 0.3);
    assert_eq!(
        placed[1].at,
        0.3 + 2.0 + r::GAP_SECONDS + r::TAIL_SECONDS,
        "the second line may not crowd the first"
    );
    assert_eq!(placed[1].tempo, 1.0, "packing alone changed nothing else");

    // A clip with room for both answers Fits: no warning for lines the ladder did not have to touch.
    assert_eq!(
        narrate_screen::mirror_fit(20.0, &[(0, 0.0, 2.0), (1, 0.5, 2.0)]),
        Fit::Fits
    );
}

#[test]
fn f4_3_s2_a_line_that_fits_wears_no_warning() {
    // 2 s of speech on a 10 s clip leaves 7.8 s of slack after the tail: nothing to say.
    let fit = narrate_screen::mirror_fit(10.0, &[(0, 0.3, 2.0)]);
    assert_eq!(fit, Fit::Fits);
    assert_eq!(narrate_screen::fit_warning(fit), None);
    assert!(!narrate_screen::row_is_red(fit));

    // An empty run is the same case taken to its limit: no lines, nothing to warn about.
    assert_eq!(narrate_screen::mirror_fit(10.0, &[]), Fit::Fits);
    assert_eq!(narrate_screen::fit_warning(Fit::Fits), None);
}

#[test]
fn f4_3_s3_growing_the_clip_within_the_extend_bound_still_reads_as_fitting() {
    // P.eng.narrationMaxExtendSeconds 4: needs more than the clip by less than the extend bound, so the
    // render simply grows the clip. Growing is free of logs, so the row stays silent about it too.
    // (A bigger need — 7 s on a 2 s clip — runs past the ceiling and lands on the tempo rung instead.)
    let lines = [(0, 0.3, 5.0)];
    let (placed, logs) = r::fit(&lines, 2.0);
    assert!(placed[0].extend > 0.0, "{placed:?}");
    assert!(placed[0].extend < r::MAX_EXTEND_SECONDS, "{placed:?}");
    assert!(logs.is_empty(), "growth says nothing: {logs:?}");
    assert_eq!(placed[0].tempo, 1.0, "nothing was sped up when growth was enough");

    // ...and the mirror agrees: growing alone is not a remedy worth warning about, since the render fixes it
    // silently. What the row may still say is that there is no breath left after the last word — `Tight`,
    // never `Overruns`: this run does get cut shorter than it speaks.
    let fit = narrate_screen::mirror_fit(2.0, &lines);
    match fit {
        Fit::Tight { speech, .. } => assert_eq!(speech, 5.0),
        other => panic!("expected Tight, got {other:?}"),
    }
    assert_eq!(narrate_screen::fit_warning(fit).unwrap().contains("sped up"), false);
    // 30 chars at the page's default rate is 2 s of speech on a 1 s clip: 0.5 s over, inside the extend
    // bound, and again nothing about tempo.
    let page_lines = [(0usize, 0.3f64, spoken(30))];
    assert_eq!(spoken(30), 2.0);
    let page_fit = narrate_screen::mirror_fit(1.0, &page_lines);
    assert!(!matches!(page_fit, Fit::Overruns { .. }), "{page_fit:?}");
}

#[test]
fn f4_3_s4_a_slide_fix_shows_the_moved_earlier_warning_not_a_sped_up_one() {
    // Past room + extend, but sliding the whole run back down to the lead is enough: the ladder logs the move
    // only. Two lines starting late on a short clip, so the slide has somewhere to go — and their total
    // speech stays under what MAX_TEMPO can carry in the grown clip, so no tempo rung is reached.
    let lines = [(0, 12.0, 2.5)];
    let (_placed, logs) = r::fit(&lines, 10.0);
    assert!(
        logs.iter().any(|l| l.contains("moved") && l.contains("earlier")),
        "expected the slide sentence: {logs:?}"
    );
    assert!(
        !logs.iter().any(|l| l.contains("sped up")),
        "a slide-only fix must not claim tempo: {logs:?}"
    );

    let fit = narrate_screen::mirror_fit(10.0, &lines);
    match fit {
        Fit::Overruns { sped_up, .. } => assert!(!sped_up, "{fit:?}"),
        other => panic!("expected Overruns, got {other:?}"),
    }
    let warning = narrate_screen::fit_warning(fit).expect("a slid run warns");
    assert!(warning.contains("moved earlier"), "{warning}");
    assert!(!warning.contains("sped up"), "{warning}");
    assert!(narrate_screen::row_is_red(fit));
}

#[test]
fn f4_3_s5_the_tempo_step_shows_the_sped_up_warning() {
    // Beyond what a slide can fix: the run sits at the lead already, so the ladder reaches MAX_TEMPO
    // (P.eng.narrationMaxTempo 1.25) and says both sentences.
    let lines = [(0, 0.3, 7.0)];
    let (_placed, logs) = r::fit(&lines, 2.0);
    assert!(
        logs.iter().any(|l| l.contains("moved") && l.contains("earlier")),
        "{logs:?}"
    );
    assert!(
        logs.iter().any(|l| l.contains("sped up")),
        "the tempo rung must be logged: {logs:?}"
    );

    let fit = narrate_screen::mirror_fit(2.0, &lines);
    match fit {
        Fit::Overruns { sped_up, past } => {
            assert!(sped_up, "{fit:?}");
            assert!(past.is_finite() && past >= 0.0, "{fit:?}");
        }
        other => panic!("expected Overruns, got {other:?}"),
    }
    let warning = narrate_screen::fit_warning(fit).expect("a sped-up run warns");
    assert!(
        warning.contains("moved earlier and sped up"),
        "both remedies in one sentence: {warning}"
    );
    assert!(narrate_screen::row_is_red(fit));
}

#[test]
fn f4_3_s6_speech_length_is_the_wav_when_it_exists_else_chars_over_a_rate_clamped_to_8_28() {
    // §F4.3: `characters / the narration's own measured rate (default 15 chars/s, clamped 8..28)`.
    // The wav half is the render's rule (a line with no synthesis has length 0.0 and takes no room); the
    // page half is this arithmetic, bounded by CHAR_RATE_MIN/CHAR_RATE_MAX.
    assert_eq!(narrate_screen::SPEECH_CHARS_PER_SECOND, 15.0, "the default rate");
    assert_eq!(narrate_screen::CHAR_RATE_MIN, 8.0);
    assert_eq!(narrate_screen::CHAR_RATE_MAX, 28.0);

    assert_eq!(spoken(30), 2.0, "30 chars at 15/s is 2 s");
    assert_eq!(narrate_screen::char_rate_clamped(15.0), 15.0, "inside the band: unchanged");

    // Fast and slow rates are pulled back to the bounds, which shows in the seconds they yield:
    // 280 chars at 40/s would be 7.0 s, but clamped to 28/s it is 10.0 s.
    assert_eq!(
        narrate_screen::page_speech_seconds(280, 40.0),
        280.0 / narrate_screen::CHAR_RATE_MAX
    );
    assert_eq!(narrate_screen::page_speech_seconds(280, 40.0), 10.0);
    // 80 chars at 2/s would be 40.0 s; clamped to 8/s it is 10.0 s.
    assert_eq!(
        narrate_screen::page_speech_seconds(80, 2.0),
        80.0 / narrate_screen::CHAR_RATE_MIN
    );
    assert_eq!(narrate_screen::page_speech_seconds(80, 2.0), 10.0);

    // A rate with no usable value falls back to the default rather than dividing by zero.
    assert_eq!(narrate_screen::char_rate_clamped(0.0), narrate_screen::SPEECH_CHARS_PER_SECOND);
    assert_eq!(
        narrate_screen::char_rate_clamped(f64::NAN),
        narrate_screen::SPEECH_CHARS_PER_SECOND
    );
}

#[test]
fn f4_3_s7_what_still_overruns_after_the_ladder_is_cut_by_the_clips_end() {
    // A line longer than room + extend + slide + max tempo can offers: the render speeds to the limit and
    // cuts the rest at the clip's end.
    let lines = [(0, 0.3, 12.0)];
    let (placed, logs) = r::fit(&lines, 2.0);
    assert_eq!(placed[0].tempo, r::MAX_TEMPO, "never faster than P.eng.narrationMaxTempo");
    assert!(placed[0].tempo <= 1.25 + 1e-9);
    assert!(
        logs.iter().any(|l| l.contains("sped up")),
        "{logs:?}"
    );

    let fit = narrate_screen::mirror_fit(2.0, &lines);
    match fit {
        Fit::Overruns { past, sped_up } => {
            assert!(sped_up, "{fit:?}");
            assert!(past.is_finite(), "{fit:?}");
            assert!(past >= 0.0, "the shortfall is floored at the clip's end, never negative");
            // The ladder grew the clip to its 4 s ceiling, so the room it could fill is 2.0 + 4.0; the run
            // still wants 12.3 s and speaks it in 9.6 s at 1.25x from the lead, leaving 8.1 s past the
            // clip's own end for the splice to cut.
            assert!(past > 8.0 && past < 8.2, "the part the ladder could not remove: {past}");
        }
        other => panic!("expected Overruns, got {other:?}"),
    }
    assert!(narrate_screen::row_is_red(fit));
}

#[test]
fn f4_3_s8_the_mirror_never_invents_a_shortfall() {
    // Empty slice: nothing written, nothing warned.
    assert_eq!(narrate_screen::mirror_fit(0.0, &[]), Fit::Fits);
    assert_eq!(narrate_screen::mirror_fit(10.0, &[]), Fit::Fits);

    // `past` never goes negative, whatever the shape: sweep clips and runs and take the floor as proof.
    for clip in [0.5f64, 1.0, 3.0, 7.5, 12.0, 30.0] {
        for speech in [0.5f64, 2.0, 5.0, 9.0, 20.0] {
            let fit = narrate_screen::mirror_fit(clip, &[(0, 0.3, speech)]);
            if let Fit::Overruns { past, .. } = fit {
                assert!(past.is_finite() && past >= 0.0, "clip {clip}s speech {speech}s -> {fit:?}");
            }
        }
    }
}
