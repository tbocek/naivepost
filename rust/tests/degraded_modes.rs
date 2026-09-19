//! §02-services.md §4 Degraded modes.
//!
//! Each test is one of the spec's six bullets, and each asserts on a returned value rather than on a log:
//! a degradation counts as handled when the job still answers, so what matters is the decision the module
//! reports. Numbers come from `degraded`'s own constants, and every fixture states its precondition in
//! the module's formula before it is used, so a fixture that stops being a fixture fails at the fixture.

use naivepost::degraded::{
    self, CUTOFF_RETRY, CutSource, RenderOutcome, ToolsFallback, AfterCutOff, FINISH_LENGTH,
    SETTINGS_FALLBACK, SILENCE_MIN_SECONDS, SILENCE_THRESHOLD_DB,
};
use naivepost::roles::{self, Job};
use naivepost::services;
use naivepost::settings;
use naivepost::tools::{self, Tool};
use std::path::Path;

/// The status an sd.cpp failure carries, which the prototype's client spells `HTTP <code>` — the only form
/// [`parse_status`] reads, since a bare four-digit run in a sentence is more likely to be a byte count or an
/// id. `finish_render` gets a sentence rather than a struct, so this is how a forgotten job is recognised.
fn forgotten(id: &str, status: u16) -> String {
    format!("the draw for job {id} came back with HTTP {status}")
}

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
        r#"{"data":[{"id":"nemotron-asr","task":"asr"},{"id":"index-tts2","task":"tts"}]}"#,
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
    assert!(short_samples / hz < SILENCE_MIN_SECONDS, "{} s", short_samples / hz);

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
fn sec_02_services_4_degraded_modes_s4_no_browser_means_the_web_tools_are_not_offered() {
    // §4.2 — no firefox, or inside Flatpak.
    assert!(!degraded::web_available("off", false));
    assert!(!degraded::web_available("OFF", false));
    assert!(!degraded::web_available("", false));
    assert!(!degraded::web_available("   ", false));
    assert!(!degraded::web_available("/usr/bin/firefox", true), "Flatpak");
    assert!(degraded::web_available("/usr/bin/firefox", false));

    // A web job loses exactly the two web tools and keeps everything else, in order.
    let job = Job::Narrate;
    assert!(roles::web_tools(job), "the fixture must be a web job");
    let with = tools::offered(job);
    let without = degraded::offered_offline(job, "off", false);
    assert!(with.contains(&Tool::WebSearch) && with.contains(&Tool::WebRead));
    assert!(!without.contains(&Tool::WebSearch));
    assert!(!without.contains(&Tool::WebRead));
    let expected: Vec<Tool> = with
        .iter()
        .copied()
        .filter(|tool| !matches!(tool, Tool::WebSearch | Tool::WebRead))
        .collect();
    assert_eq!(without, expected, "order is offered()'s minus the two rows");

    // Flatpak is the same rule by another route.
    assert_eq!(
        degraded::offered_offline(job, "/usr/bin/firefox", true),
        without
    );

    // A job that never had web tools is untouched.
    let quiet = Job::CleanTranscript;
    assert!(!roles::web_tools(quiet));
    assert_eq!(
        degraded::offered_offline(quiet, "off", false),
        tools::offered(quiet)
    );
}

#[test]
fn sec_02_services_4_degraded_modes_s5_sd_cpp_down_still_completes_the_render() {
    // §4.3 — the render completes; the thumbnail half fails and is logged.
    let (outcome, logs) = degraded::finish_render(true, Ok(()));
    assert_eq!(
        outcome,
        RenderOutcome::ThumbnailFailed(
            "sd.cpp is down -- the thumbnail was not drawn, the video is complete".to_string()
        )
    );
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("thumbnail"), "{}", logs[0]);

    // A thumbnail that failed for its own reason names that reason and still completes.
    let (outcome, logs) = degraded::finish_render(false, Err("png could not be written".to_string()));
    match &outcome {
        RenderOutcome::ThumbnailFailed(line) => {
            assert!(line.contains("thumbnail") && line.contains("png could not be written"), "{line}");
        }
        RenderOutcome::Complete => panic!("a failed thumbnail is never Complete"),
    }
    assert_eq!(logs.len(), 1);

    // Both halves fine: complete, nothing to log.
    let (outcome, logs) = degraded::finish_render(false, Ok(()));
    assert_eq!(outcome, RenderOutcome::Complete);
    assert!(logs.is_empty(), "{logs:?}");
}

#[test]
fn sec_02_services_4_degraded_modes_s6_a_job_the_server_forgot_is_named() {
    // §4.3 — 404 and 410 mean the job is gone; every other status is an answer about a live job.
    assert!(degraded::forgot_job(404));
    assert!(degraded::forgot_job(410));
    for status in [200u16, 400, 401, 500, 503] {
        assert!(!degraded::forgot_job(status), "{status} is not a forgotten job");
    }

    // The spec's sentence, verbatim.
    assert_eq!(
        degraded::forgot_job_message("abc", 410),
        "sd.cpp forgot job abc (410) -- it was probably restarted mid-draw"
    );
    // And a render whose thumbnail failed with that status logs the same sentence, not a generic one. The
    // reason arrives as a sentence carrying the id and the status, which is all this module is given — so
    // it has to find both in the text, and this checks what it finds.
    let (outcome, logs) = degraded::finish_render(false, Err(forgotten("abc", 404)));
    assert!(logs.len() == 1 && logs[0].contains("sd.cpp forgot job abc (404)"), "{logs:?}");
    match outcome {
        RenderOutcome::ThumbnailFailed(line) => assert_eq!(line, logs[0]),
        RenderOutcome::Complete => panic!("the thumbnail did fail"),
    }
}

#[test]
fn sec_02_services_4_degraded_modes_s7_no_supported_modes_is_assumed_able_to_draw() {
    // §4.3 — the field is newer than most builds, so its absence is not a refusal.
    assert!(degraded::can_draw(None));
    assert!(degraded::can_draw(Some(&[])));
    assert!(degraded::can_draw(Some(&["draw", "blur"])));
    assert!(!degraded::can_draw(Some(&["blur"])), "a list that does not name draw");

    // The Settings test falls back to /v1/models only when sd.cpp's own route says it is not there.
    assert_eq!(SETTINGS_FALLBACK, "/v1/models");
    assert_eq!(degraded::settings_test_path(404), Some(SETTINGS_FALLBACK));
    for status in [200u16, 401, 410, 500] {
        assert_eq!(degraded::settings_test_path(status), None, "{status}");
    }
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
}

#[test]
fn sec_02_services_4_degraded_modes_s9_a_tools_refusal_later_fails_the_call() {
    // §4.4 — a `tools` error on a later round fails the call rather than changing shape mid-job.
    let refusal = "unknown field tools in this request";
    for round in [2u32, 3, degraded::SNAP_TOLERANCE_SECONDS as u32 * 2] {
        assert_eq!(degraded::tools_fallback(refusal, round), ToolsFallback::FailCall);
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
fn sec_02_services_4_degraded_modes_s10_a_cut_off_reply_keeps_the_calls_already_made() {
    // §4.6 — only `length` says cut off.
    assert_eq!(FINISH_LENGTH, "length");
    assert!(degraded::cut_off(FINISH_LENGTH, false));
    assert!(!degraded::cut_off("stop", true));
    assert!(!degraded::cut_off("", false));

    // With tools and calls already answered, everything already called stands.
    assert_eq!(degraded::after_cut_off(true, 3), AfterCutOff::KeepCalls);
}

#[test]
fn sec_02_services_4_degraded_modes_s11_without_tools_a_cut_off_reply_is_asked_again() {
    // §4.6 — the prototype's retry turn.
    let retry = degraded::after_cut_off(false, 0);
    match &retry {
        AfterCutOff::Retry(message) => {
            assert!(message.contains("far fewer items"), "{message}");
            assert_eq!(message, CUTOFF_RETRY);
        }
        AfterCutOff::KeepCalls => panic!("nothing was called, so nothing stands"),
    }
    // Tools active but no call yet: the same — there is no partial result to keep.
    assert_eq!(degraded::after_cut_off(true, 0), retry);
}

#[test]
fn sec_02_services_4_degraded_modes_s12_flatpak_keeps_everything_in_the_sandbox() {
    // §4.5 — the voices folder is in the sandbox's data dir. settings::voices_folder is §7's rule and is
    // reused here rather than restated: one place decides where the voice library lives.
    let data = Path::new("/home/user/.var/app/ch.bocek.naivepost/data");
    assert_eq!(
        settings::voices_folder("", true, data),
        data.join("voices")
    );
    // Outside Flatpak it is the machine's model folder, not somewhere under the data dir.
    let outside = settings::voices_folder("", false, data);
    assert!(!outside.starts_with(data), "{outside:?}");

    // And the web tools route through one predicate, so there is no second Flatpak rule to drift from it.
    assert!(!degraded::web_available("/usr/bin/firefox", true));
    let desktop = include_str!("../src/degraded.rs");
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
    assert_eq!(degraded::cut_points(&[13.9], &silences, 0.0), vec![Some(13.9)]);

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

#[test]
fn sec_02_services_4_degraded_modes_s14_every_degradation_at_once_is_survivable() {
    // No aligner, no browser, sd.cpp down, a tools refusal on round one, and a reply cut off at the limit.
    let logs: Vec<String> = Vec::new();

    // 1. Cut points come off the waveform: one edge lands inside a silence and keeps its own second, the
    // other is refused because nothing near it can be cut in.
    assert_eq!(degraded::cut_source(""), CutSource::Waveform);
    let silences = degraded::silence_spans(&[1.0, 0.0, 0.0, 1.0], 1.0, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS);
    let points = degraded::cut_points(&[2.5, 300.0], &silences, degraded::SNAP_TOLERANCE_SECONDS);
    assert_eq!(points[0], Some(2.5), "the edge inside a silence was kept");
    assert_eq!(points[1], None, "and the one that could not, was refused");

    // 2. No web tools; the job still has its own.
    let offered = degraded::offered_offline(Job::UploadText, "", false);
    assert!(!offered.contains(&Tool::WebSearch));
    assert!(!offered.is_empty(), "the job is not left with nothing to do");

    // 3. The tools refusal sends the job to one JSON answer.
    assert_eq!(
        degraded::tools_fallback("tools are not supported here", 1),
        ToolsFallback::RunAsOneJsonAnswer
    );

    // 4. That answer came back cut off, and no call had been made: ask again, shorter.
    assert!(degraded::cut_off(FINISH_LENGTH, true));
    let retry = degraded::after_cut_off(false, 0);
    assert!(matches!(retry, AfterCutOff::Retry(_)));

    // 5. sd.cpp never came up: the thumbnail half fails, the render does not.
    let (outcome, render_logs) = degraded::finish_render(true, Ok(()));
    let mut logs = logs;
    logs.extend(render_logs);
    assert!(matches!(outcome, RenderOutcome::ThumbnailFailed(_)));
    assert_eq!(logs.len(), 1, "{logs:?}");

    // Nothing above failed the job: two facts to log (the thumbnail, and the shorter answer that was
    // asked for), one refused cut point, and a render that completed.
    let summary = format!("{outcome:?} {} {retry:?}", logs.join("; "));
    assert!(summary.contains("thumbnail"), "{summary}");
}
