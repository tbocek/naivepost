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

    // §1's second Tight spelling — "M s before THE NEXT LINE" — has to be reachable from real input, not
    // only from a hand-built `Fit`. Two lines the pack rung pushes together (the second placed at 0.5 but
    // held to the first's end + gap + tail) on a clip that ends inside the pushed run: the shortfall is
    // caused by the crowding, so the row blames the next line rather than the clip's end. The packed run
    // here starts line 2 at 3.8 and speaks to 6.8, so a 6.5 s clip has 0.3 s of speech left over it.
    let crowded = [(0usize, 0.3f64, 3.0), (1, 0.5, 3.0)];
    match narrate_screen::mirror_fit(6.5, &crowded) {
        Fit::Tight { next_line, .. } => assert!(next_line, "a pushed line blames the next line: see below"),
        other => panic!("expected Tight, got {other:?}"),
    }
    // Same two lines spread far enough apart that packing pushes nothing (line 2 sits at 4.0, past the
    // first's packed end): the same kind of tightness now has only the clip's end to blame.
    let spaced = [(0usize, 0.3f64, 3.0), (1, 4.0, 1.5)];
    match narrate_screen::mirror_fit(5.2, &spaced) {
        Fit::Tight {
            next_line: false, ..
        } => {}
        other => panic!("expected a Tight blaming the clip's end, got {other:?}"),
    }

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

// --- the render's own half of F4.3: the footage bound, the clip number, and what a line is measured by ---

use naivepost::cut::Seg;
use naivepost::narration::Entry;

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Default::default()
    }
}

fn entry(s: f64, e: f64, at: f64) -> Entry {
    Entry {
        s,
        e,
        at,
        text: "spoken words".into(),
        ..Default::default()
    }
}

fn no_fx(_: &Seg) -> Vec<naivepost::cut::Fx> {
    Vec::new()
}

#[test]
fn f4_3_s9_the_grow_stops_at_the_footage_not_at_the_ceiling() {
    // P.eng.narrationMaxExtendSeconds 4 (r::MAX_EXTEND_SECONDS), and §F4.3 step 1's other half: "never
    // past the footage". The run `f4_3_s3` grows silently — 5.5 s needed on a 2.0 s clip — here with its
    // footage held to 1.0 s: the grow stops at the tape, so what is left over must be said out loud.
    let lines = [(0usize, 0.3f64, 5.0)];
    let (placed, logs) = r::fit_in(&lines, 2.0, 1, 1.0);
    assert!(
        (placed[0].extend - 1.0).abs() < 1e-9,
        "the tape, not the ceiling, set the room: {:?}",
        placed[0].extend
    );
    assert_eq!(
        logs.len(),
        2,
        "a run the footage starved cannot fit in silence: {logs:?}"
    );
    assert!(
        logs.iter().any(|l| l.contains("moved") && l.contains("earlier")),
        "{logs:?}"
    );

    // The same shortage with the ceiling alone (§10's 4 s) swallows it whole and says nothing, which is
    // exactly what `footage_left` 0.0 means: the caller does not know, so the grow keeps its own bound.
    let (blind, blind_logs) = r::fit_in(&lines, 2.0, 1, 0.0);
    assert!(
        blind[0].extend > 3.4 && blind[0].extend < 3.6,
        "unknown footage keeps §10's ceiling: {:?}",
        blind[0].extend
    );
    assert!(blind_logs.is_empty(), "growth alone stays silent: {blind_logs:?}");
    // And `fit` — the page's door, which has no footage in hand — agrees with that reading.
    let (_page_placed, page_logs) = r::fit(&lines, 2.0);
    assert!(page_logs.is_empty(), "{page_logs:?}");
}

#[test]
fn f4_3_s10_the_ladder_log_names_the_clip_not_the_entry() {
    // Two kept clips; only the SECOND carries narration, and too much of it for the room it has. Every
    // fit sentence must count from the SEGMENT list ("clip 2"), because that is how the Cut page's rows
    // and every other line of this run number the same clip — an entry index would name a row nobody can
    // find. Compared against the builders themselves rather than retyped, so a wording change moves both.
    // One clip of footage carrying two lines placed 8 s into a 3 s clip: §10's 4 s of grow cannot cover
    // that, so the slide rung speaks — and it must count from the SEGMENT list ("clip 2"), because that is
    // how the Cut page's rows number the same clip; an entry index would name a row nobody can find.
    // Speech is 0.0 at plan time (no wav measured yet), so the overrun comes from where the lines were
    // PLACED, which is exactly what S5 later fills in without changing what was already said.
    let segs = [seg(0.0, 6.0), seg(6.0, 9.0)];
    // Placed 8 s into a 3 s clip: §10's 4 s of grow cannot cover it, so the slide rung speaks. Speech is
    // 0.0 at plan time (no wav measured yet), so placement is all the overrun there is here; `fit_in` below
    // covers the tempo rung, where a measured take does exist.
    let entries = [entry(6.0, 9.0, 8.0), entry(6.0, 9.0, 8.4)];
    let (clips, logs) = r::plan_clips(
        &segs,
        no_fx,
        "source",
        |_| true,
        &[],
        &entries,
        (1920, 1080),
        true,
        r::unknown_footage,
    );
    assert_eq!(clips.len(), 2, "both segments are long enough to survive S3");
    assert_eq!(clips[1].lines.len(), 2, "the run sits on the second clip");
    assert_eq!(clips[0].lines.len(), 0, "the first clip carries nothing and says nothing");
    assert_eq!(logs.len(), 1, "one slide, about clip 2: {logs:?}");
    let slide = logs[0].clone();
    assert!(slide.starts_with("clip 2: "), "{slide}");
    assert!(!slide.contains("entry"), "a person reads clip numbers, not indices: {slide}");

    // The same shortage through `fit_in` with the clip number the planner would pass reproduces the
    // sentence exactly, digits and all. A line placed at 8.0 on a 3 s clip overruns by 5.7 s; §10's 4 s of
    // grow absorbs part of that silently, and what the slide says out loud is the shortfall measured against
    // the grown room (3.0 + 4.0), floored by how far the run can go at all (back to the lead, 7.7 s).
    let (_, slid) = r::fit_in(&[(0usize, 8.0f64, 2.5)], 3.0, 2, 0.0);
    let slid_by = 8.0 + 2.5 + r::TAIL_SECONDS - (3.0 + r::MAX_EXTEND_SECONDS);
    assert!(
        slid.iter().any(|l| *l == r::moved_log(2, slid_by)),
        "expected {:?}, got {slid:?}",
        r::moved_log(2, slid_by)
    );
    // And the tempo rung, which needs a run that cannot be saved by sliding at all: it sits at the lead
    // already. The grow rung ran first and took §10's whole 4 s ceiling here, so the room the sentence
    // names is 2.0 + 4.0, and the speech it names is the packed run's end — gap and tail included, so
    // 7.3 s of need for 7.0 s of words. Both are read back off the ladder's own placements rather than
    // retyped, so the builder and the ladder are pinned to each other and not to one frozen run.
    let tight = [(0usize, 0.3f64, 7.0)];
    let (sped_placed, sped) = r::fit_in(&tight, 2.0, 2, 0.0);
    let said_need = r::run_end(&r::packed(&tight));
    let said_room = 2.0 + sped_placed[0].extend;
    let said_tempo = sped_placed[0].tempo;
    assert!(
        sped.iter()
            .any(|l| *l == r::sped_up_log(2, said_need, said_room, said_tempo)),
        "the builder and the ladder must agree to the digit: {sped:?}"
    );
}

#[test]
fn f4_3_s11_a_line_without_a_wav_takes_no_room() {
    // §F4.3 outright: "In the render a line with no synthesis has length 0, takes no room". The entry's
    // OWN span is not a stand-in — entry 1 spans 4 s of clip but speaks none of it, and giving it that
    // room is the bug this pins: the bed ducked for words nobody says, and every later cue pushed late.
    // `seconds_of` answers for entry 0 only, which is what a missing wav looks like from here.
    let entries = [entry(0.0, 3.0, 0.3), entry(0.0, 4.0, 3.5)];
    let seconds_of = |entry: &Entry| -> Option<f64> {
        (entry.at < 1.0).then_some(2.5)
    };
    let mut clip = r::Clip {
        no: 1,
        seg_s: 0.0,
        on_screen: 6.0,
        rate: 1.0,
        source: String::new(),
        frame: (1920, 1080),
        lanes: Vec::new(),
        cues: 0,
        camera_depth: 1.0,
        fx: Vec::new(),
        read_head: None,
        on_recording: false,
        src: (1920, 1080),
        scratch: std::path::PathBuf::new(),
        // Both lines as `plan_clips` leaves them before any wav is measured: length 0.
        lines: vec![
            r::Placed {
                entry_index: 0,
                at: 0.3,
                tempo: 1.0,
                extend: 0.0,
                text: String::new(),
                speech: 0.0,
            },
            r::Placed {
                entry_index: 1,
                at: 3.5,
                tempo: 1.0,
                extend: 0.0,
                text: String::new(),
                speech: 0.0,
            },
        ],
    };
    naivepost::produce_exec::fill_speech(std::slice::from_mut(&mut clip), &entries, &seconds_of);
    assert!((clip.lines[0].speech - 2.5).abs() < 1e-9, "{:?}", clip.lines[0]);
    assert!((clip.lines[1].speech - 0.0).abs() < 1e-9, "{:?}", clip.lines[1]);
    assert_ne!(
        clip.lines[1].speech,
        entries[1].e - entries[1].s,
        "a silent line must not inherit its entry's span"
    );
}

/// A canonical mono 16-bit PCM wav of `frames` samples at `rate`, optionally claiming more `data`
/// bytes than the file actually holds — the shape a truncated download leaves behind.
fn pcm_wav(rate: u32, frames: usize, lie_bytes: usize) -> Vec<u8> {
    let real = frames * 2;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + real + lie_bytes) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // one channel
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&((real + lie_bytes) as u32).to_le_bytes());
    out.extend_from_slice(&vec![0u8; real]);
    out
}

#[test]
fn f4_3_s12_speech_length_is_measured_from_the_wav_header() {
    let dir = std::env::temp_dir().join(format!("np-f43-wav-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    // §F4.2 pins the take format at 16-bit / 48 kHz / mono, so 48000 frames IS one second.
    let exact = dir.join("one-second.wav");
    std::fs::write(&exact, pcm_wav(48_000, 48_000, 0)).expect("write wav");
    let seconds = naivepost::voice_ref::wav_seconds(&exact).expect("a real wav measures");
    assert!((seconds - 1.0).abs() < 0.01, "48000 frames at 48 kHz: {seconds}");

    // Half a second, to show the arithmetic follows the data chunk and not the file's size: a header
    // and chunk tags ride on top of either.
    let half = dir.join("half-second.wav");
    std::fs::write(&half, pcm_wav(48_000, 24_000, 0)).expect("write wav");
    let half_seconds = naivepost::voice_ref::wav_seconds(&half).expect("measured");
    assert!((half_seconds - 0.5).abs() < 0.01, "{half_seconds}");

    // A header that lies about its size: the duration comes from the bytes that are really there.
    let ragged = dir.join("ragged.wav");
    std::fs::write(&ragged, pcm_wav(48_000, 12_000, 96_000)).expect("write wav");
    let ragged_seconds = naivepost::voice_ref::wav_seconds(&ragged).expect("measured");
    assert!(
        (ragged_seconds - 0.25).abs() < 0.01,
        "a truncated wav counts what exists, not what was promised: {ragged_seconds}"
    );

    // Not a wav, and no wav at all: `None`, which is how a line keeps length 0 instead of guessing.
    let junk = dir.join("not-a-wav.wav");
    std::fs::write(&junk, b"OggS some container that is not RIFF/WAVE").expect("write junk");
    assert_eq!(naivepost::voice_ref::wav_seconds(&junk), None);
    assert_eq!(naivepost::voice_ref::wav_seconds(&dir.join("missing.wav")), None);

    std::fs::remove_dir_all(&dir).ok();
}
