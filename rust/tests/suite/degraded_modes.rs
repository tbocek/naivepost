//! §02-services.md §4 Degraded modes.
//!
//! Each test is one of the spec's six bullets, and each asserts on a returned value rather than on a log:
//! a degradation counts as handled when the job still answers, so what matters is the decision the module
//! reports. Numbers come from `degraded`'s own constants, and every fixture states its precondition in
//! the module's formula before it is used, so a fixture that stops being a fixture fails at the fixture.

use naivepost::degraded::{
    self, CutSource, ToolsFallback, SILENCE_MIN_SECONDS, SILENCE_THRESHOLD_DB,
};
use naivepost::services;
use naivepost::settings;
use std::path::Path;

#[test]
fn sec_02_services_4_degraded_modes_s1_no_aligner_cuts_off_the_waveform() {
    // §4.1 — the aligner box defaults to empty, and an empty box is "no aligner".
    assert_eq!(degraded::cut_source(""), CutSource::Waveform);
    assert_eq!(degraded::cut_source("   "), CutSource::Waveform);
    assert_eq!(degraded::cut_source("qwen3-aligner"), CutSource::Words);

    // The real path into that: a server whose list holds nothing for `align` gives no aligner, which is
    // what the settings end up holding. Parsed from a body rather than hand-built, so a change to the
    // catalog shape lands here too.
    let models = services::parse_models(
        r#"{"data":[{"id":"qwen3-asr","task":"asr"},{"id":"index-tts2","task":"tts"}]}"#,
    );
    assert_eq!(models.len(), 2);
    let aligner = services::pick_aligner("", &models).unwrap_or_default();
    assert_eq!(degraded::cut_source(&aligner), CutSource::Waveform);

    // With an aligner declared for the task, cut points come from word timings instead.
    let with = services::parse_models(r#"{"data":[{"id":"qwen3-aligner","task":"align"}]}"#);
    let aligner = services::pick_aligner("", &with).expect("an aligner is offered");
    assert_eq!(degraded::cut_source(&aligner), CutSource::Words);
}

#[test]
fn sec_02_services_4_degraded_modes_s2_a_silence_is_a_long_enough_quiet_run() {
    // The rate is derived from the parameter so this holds whatever it says: a run of `long_samples`
    // samples is exactly twice the minimum (a silence) and one of `short_samples` is half of it (the gap
    // between two words). P.machine.silenceMinSeconds
    let long_samples = 4.0;
    let hz = long_samples / (SILENCE_MIN_SECONDS * 2.0);
    let short_samples = (hz * SILENCE_MIN_SECONDS / 2.0).round().max(1.0);
    let quiet_long = vec![0.0_f64; long_samples as usize];
    let loud = vec![1.0_f64; 6];
    let quiet_short = vec![0.0_f64; short_samples as usize];
    let mut peaks = quiet_long.clone();
    peaks.extend_from_slice(&loud);
    peaks.extend_from_slice(&quiet_short);
    peaks.push(1.0);
    // The short run really is under the minimum at this rate, or it would survive too.
    assert!(
        short_samples / hz < SILENCE_MIN_SECONDS,
        "{} s",
        short_samples / hz
    );

    let spans = degraded::silence_spans(&peaks, hz, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS);
    assert_eq!(spans.len(), 1, "the short run must not count: {spans:?}");
    assert!((spans[0].0 - 0.0).abs() < 1e-9, "{spans:?}");
    assert!(
        (spans[0].1 - SILENCE_MIN_SECONDS * 2.0).abs() < 1e-9,
        "{spans:?}"
    );

    // A zero peak is silence, and silence is not NaN: log(0) would have poisoned the span.
    let zeros = [0.0, 0.0, 0.0, 0.0];
    let spans = degraded::silence_spans(&zeros, hz, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS);
    assert_eq!(spans.len(), 1);
    assert!(!spans[0].0.is_nan() && !spans[0].1.is_nan(), "{spans:?}");
    assert!(
        (spans[0].1 - 4.0 / hz).abs() < 1e-9,
        "four samples at {hz} Hz: {spans:?}"
    );

    // A run that reaches the end of the waveform is still a run.
    let mut tail = vec![1.0];
    tail.extend_from_slice(&quiet_long);
    let spans = degraded::silence_spans(&tail, hz, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS);
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert!((spans[0].0 - 1.0 / hz).abs() < 1e-9, "{spans:?}");

    // The threshold itself decides: at 0 dBFS everything is quiet, so the whole waveform is one span.
    let all = degraded::silence_spans(&peaks, hz, 0.0, SILENCE_MIN_SECONDS);
    assert_eq!(all.len(), 1, "one span over all of it: {all:?}");
}

#[test]
fn sec_02_services_4_degraded_modes_s3_an_edge_with_no_silence_nearby_is_refused() {
    let silences = [(10.0, 14.0)];
    // Inside a silence the edge keeps the second it asked for; within tolerance it moves to the nearer
    // end; with nothing near it there is no cut to make. P.policy.snapToleranceSeconds
    let tolerance = degraded::SNAP_TOLERANCE_SECONDS;
    assert!(tolerance > 0.0);
    let points = degraded::cut_points(&[12.0, 15.0, 100.0], &silences, tolerance);
    // Inside.
    assert_eq!(points[0], Some(12.0));
    // One second past the end, well inside tolerance: it lands on the end.
    assert_eq!(points[1], Some(14.0));
    // Nothing within reach. This None is §4.1's "unable to cut between two words of one breath": with no
    // word timings there is no second between them to aim at, and inventing one is a click.
    assert_eq!(points[2], None);

    // The nearer end wins when both are in reach.
    let points = degraded::cut_points(&[9.6], &silences, tolerance);
    assert_eq!(points[0], Some(10.0));

    // No silences at all: nothing is cuttable.
    let points = degraded::cut_points(&[1.0, 2.0], &[], tolerance);
    assert_eq!(points, vec![None, None]);
}

#[test]
fn sec_02_services_4_degraded_modes_s8_a_tools_refusal_on_round_one_runs_as_one_json_answer() {
    // §4.4 — an error on the first round that is not a stop.
    let refusal = "the server rejected the request: tools are not supported";
    assert!(degraded::tools_refused(refusal));
    assert_eq!(
        degraded::tools_fallback(refusal, 1),
        ToolsFallback::RunAsOneJsonAnswer
    );

    // A stop is the model finishing, not a schema rejection: it must not send a job down the JSON path.
    assert!(!degraded::tools_refused("stop"));
    assert!(!degraded::tools_refused("end of turn"));
    assert_eq!(
        degraded::tools_fallback("stop", 1),
        ToolsFallback::KeepTools
    );

    // A DEAD server is not a server refusing the `tools` field either. §2's fallback is for a
    // server that ANSWERED and objected to the request; a wire that never spoke must never be
    // talked into a DIFFERENT request, because re-asking a dead wire is what turned §3's one
    // retry into three asks on a box that is down. `connection refused` already read as no; the
    // case that could not is a RESET, whose real transport string (OpenSSL, and ureq's body-read
    // wrapper around it) carries the words inside a longer sentence with a trailing
    // ` (os error 104)` — a shape the word-boundary matcher must still catch. The string below
    // also carries the letters of the noun the fallback looks for, which is what makes the guard
    // in `tools_refused` load-bearing rather than decorative.
    let dead_wire = "failed, transport error: connection reset by peer (os error 104)";
    assert!(
        naivepost::llm_retry::transport_death(dead_wire),
        "the ladder's own door reads this as a dead transport"
    );
    assert!(
        !degraded::tools_refused(dead_wire),
        "a reset is a transport death, not a tools refusal"
    );
    assert_eq!(
        degraded::tools_fallback(dead_wire, 1),
        ToolsFallback::KeepTools,
        "and it asks for no fallback body: the wire, not the request, is the problem"
    );
}

#[test]
fn sec_02_services_4_degraded_modes_s9_a_tools_refusal_later_fails_the_call() {
    // §4.4 — a `tools` error on a later round fails the call rather than changing shape mid-job.
    let refusal = "unknown field tools in this request";
    for round in [2u32, 3, degraded::SNAP_TOLERANCE_SECONDS as u32 * 2] {
        assert_eq!(
            degraded::tools_fallback(refusal, round),
            ToolsFallback::FailCall
        );
    }
    // Round 1 is the only round that falls back.
    assert_eq!(
        degraded::tools_fallback(refusal, 1),
        ToolsFallback::RunAsOneJsonAnswer
    );

    // Anything that is not about tools keeps going with tools, on any round.
    for round in [1u32, 5] {
        assert_eq!(
            degraded::tools_fallback("connection refused", round),
            ToolsFallback::KeepTools
        );
    }
}

#[test]
fn sec_02_services_4_degraded_modes_s12_flatpak_keeps_everything_in_the_sandbox() {
    // §4.5 — the voices folder is in the sandbox's data dir. settings::voices_folder is §7's rule and is
    // reused here rather than restated: one place decides where the voice library lives.
    let data = Path::new("/home/user/.var/app/ch.bocek.naivepost/data");
    assert_eq!(
        settings::voices_folder("", "", true, data),
        data.join("voices")
    );
    // Outside Flatpak it is the machine's model folder, not somewhere under the data dir.
    let outside = settings::voices_folder("", "", false, data);
    assert!(!outside.starts_with(data), "{outside:?}");

    // And the web tools route through one predicate, so there is no second Flatpak rule to drift from it.
    assert!(!degraded::web_available("/usr/bin/firefox", true));
    let desktop = include_str!("../../src/degraded.rs");
    let prose: String = desktop
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect();
    assert!(
        !prose.to_lowercase().contains("desktop"),
        "the desktop-entry rule belongs to §7's settings module, not here"
    );
}

#[test]
fn sec_02_services_4_degraded_modes_s13_the_parameters_this_item_uses() {
    // P.machine.silenceThresholdDB / P.machine.silenceMinSeconds
    assert_eq!(SILENCE_THRESHOLD_DB, -35.0);
    assert_eq!(SILENCE_MIN_SECONDS, 0.4);

    // A tolerance of zero means only an edge already inside a silence can be cut there — the reach is the
    // caller's to give and this module does not add one of its own. P.policy.snapToleranceSeconds
    let silences = [(10.0, 14.0)];
    let points = degraded::cut_points(&[14.1], &silences, 0.0);
    assert_eq!(points, vec![None]);
    assert_eq!(
        degraded::cut_points(&[13.9], &silences, 0.0),
        vec![Some(13.9)]
    );

    // A run exactly at the minimum is silence; a hair under it is not.
    let peaks = [0.0, 0.0];
    assert_eq!(
        degraded::silence_spans(&peaks, 5.0, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS).len(),
        1,
        "2 samples at 5 Hz is exactly 0.4 s"
    );
    assert!(
        degraded::silence_spans(&peaks, 6.0, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS).is_empty(),
        "2 samples at 6 Hz is 0.33 s, under the minimum"
    );
}
