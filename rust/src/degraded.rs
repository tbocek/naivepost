//! §02-services.md §4 Degraded modes: what each missing piece does to a job.

use crate::roles::Job;
use crate::tools::{self, Tool};

pub use crate::tools::cutpass::SNAP_TOLERANCE_SECONDS;

/// A run of samples at or under this is silence. P.machine.silenceThresholdDB
pub const SILENCE_THRESHOLD_DB: f64 = -35.0;

/// Shorter than this and it is a gap between words, not somewhere to cut. P.machine.silenceMinSeconds
pub const SILENCE_MIN_SECONDS: f64 = 0.4;

/// What the Settings test asks when sd.cpp's own route does not answer — see [`settings_test_path`].
pub const SETTINGS_FALLBACK: &str = "/v1/models";

/// `finish_reason` for a reply that ran into the token ceiling and was cut off mid-answer.
pub const FINISH_LENGTH: &str = "length";

/// says what to do about it.
pub const CUTOFF_RETRY: &str = "your answer stopped in the middle -- it was too long to finish. \
                                Answer again with far fewer items";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutSource {
    /// Word timings: an edge can land between two words of one breath.
    Words,
    /// Waveform only: clean wherever a silence offers itself, and nowhere else.
    Waveform,
}

pub fn cut_source(aligner: &str) -> CutSource {
    if aligner.trim().is_empty() {
        CutSource::Waveform
    } else {
        CutSource::Words
    }
}

fn dbfs(peak: f64) -> f64 {
    if peak <= 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * peak.log10()
}

pub fn silence_spans(
    peaks: &[f64],
    hz: f64,
    threshold_db: f64,
    min_seconds: f64,
) -> Vec<(f64, f64)> {
    let mut spans = Vec::new();
    let mut run: Option<usize> = None;
    for (index, peak) in peaks.iter().enumerate() {
        let quiet = dbfs(*peak) <= threshold_db;
        match (quiet, run) {
            (true, None) => run = Some(index),
            (false, Some(from)) => {
                push_run(&mut spans, from, index, hz, min_seconds);
                run = None;
            }
            _ => {}
        }
    }
    if let Some(from) = run {
        push_run(&mut spans, from, peaks.len(), hz, min_seconds);
    }
    spans
}

/// Close one run of quiet samples into a span, dropping it when it is too short to cut in.
fn push_run(spans: &mut Vec<(f64, f64)>, from: usize, to: usize, hz: f64, min_seconds: f64) {
    let (start, end) = (from as f64 / hz, to as f64 / hz);
    if end - start >= min_seconds {
        spans.push((start, end));
    }
}

pub fn cut_points(wanted: &[f64], silences: &[(f64, f64)], tolerance: f64) -> Vec<Option<f64>> {
    wanted
        .iter()
        .map(|edge| {
            silences
                .iter()
                // The distance to a silence an edge is already inside is zero, so it wins over any
                // nearby one and the edge keeps the second it asked for.
                .map(|(from, to)| {
                    let distance = if edge >= from && edge <= to {
                        0.0
                    } else {
                        (edge - from).abs().min((edge - *to).abs())
                    };
                    (distance, edge.clamp(*from, *to))
                })
                .filter(|(distance, _)| *distance <= tolerance)
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(_, placed)| placed)
        })
        .collect()
}

pub fn web_available(firefox: &str, flatpak: bool) -> bool {
    if flatpak {
        return false;
    }
    let firefox = firefox.trim();
    !firefox.is_empty() && !firefox.eq_ignore_ascii_case("off")
}

pub fn offered_offline(job: Job, firefox: &str, flatpak: bool) -> Vec<Tool> {
    let offered = tools::offered(job);
    if web_available(firefox, flatpak) {
        return offered;
    }
    offered
        .into_iter()
        .filter(|tool| !matches!(tool, Tool::WebSearch | Tool::WebRead))
        .collect()
}

pub fn forgot_job(status: u16) -> bool {
    matches!(status, 404 | 410)
}

pub fn forgot_job_message(id: &str, status: u16) -> String {
    format!("sd.cpp forgot job {id} ({status}) -- it was probably restarted mid-draw")
}

pub fn can_draw(supported_modes: Option<&[&str]>) -> bool {
    match supported_modes {
        None => true,
        Some(modes) if modes.is_empty() => true,
        Some(modes) => modes.contains(&"draw"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOutcome {
    /// Video and thumbnail both there.
    Complete,
    /// The video is there; the thumbnail half did not happen. Carries why.
    ThumbnailFailed(String),
}

pub fn finish_render(sd_down: bool, thumbnail: Result<(), String>) -> (RenderOutcome, Vec<String>) {
    if let Err(reason) = thumbnail {
        // The status is inside the reason's text rather than beside it, so a forgotten job is spotted by
        // reading the sentence that came back — the only thing this module is given.
        let line = match parse_status(&reason) {
            Some(status) if forgot_job(status) => forgot_job_message(&parse_id(&reason), status),
            _ => format!("thumbnail failed -- {reason}"),
        };
        return (RenderOutcome::ThumbnailFailed(line.clone()), vec![line]);
    }
    if sd_down {
        let line =
            "sd.cpp is down -- the thumbnail was not drawn, the video is complete".to_string();
        return (RenderOutcome::ThumbnailFailed(line.clone()), vec![line]);
    }
    (RenderOutcome::Complete, Vec::new())
}

fn parse_status(reason: &str) -> Option<u16> {
    let mut rest = reason;
    while let Some(at) = rest.find("HTTP ") {
        let digits = &rest[at + 5..];
        let end = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        if let Ok(value) = digits[..end].parse::<u16>() {
            return Some(value);
        }
        rest = &digits[end..];
    }
    None
}

fn parse_id(reason: &str) -> String {
    match reason.find("job ") {
        Some(at) => {
            let rest = &reason[at + 4..];
            let end = rest.find([' ', '(']).unwrap_or(rest.len());
            rest[..end].to_string()
        }
        None => String::new(),
    }
}

pub fn settings_test_path(sd_probe_status: u16) -> Option<&'static str> {
    match sd_probe_status {
        404 => Some(SETTINGS_FALLBACK),
        _ => None,
    }
}

pub fn tools_refused(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if lower.contains("stop") || lower.contains("end of turn") {
        return false;
    }
    if crate::llm_retry::transport_death(&lower) {
        return false;
    }
    lower.contains("tool")
}

pub fn refused_field(status: u16) -> bool {
    matches!(status, 400 | 422)
}

pub fn refused_answer(status: u16, body: &str) -> bool {
    refused_field(status) && tools_refused(body)
}

pub fn refused_reason(status: u16, body: &str) -> String {
    format!(
        "the request with tools was answered {status}: {}",
        body.trim()
    )
}

/// What a `tools` problem means for the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolsFallback {
    RunAsOneJsonAnswer,
    FailCall,
    /// Not a tools problem; keep going with tools.
    KeepTools,
}

pub fn tools_fallback(error: &str, round: u32) -> ToolsFallback {
    if !tools_refused(error) {
        return ToolsFallback::KeepTools;
    }
    if round <= 1 {
        ToolsFallback::RunAsOneJsonAnswer
    } else {
        ToolsFallback::FailCall
    }
}

pub fn cut_off(finish_reason: &str) -> bool {
    finish_reason == FINISH_LENGTH
}

/// What to do with a reply that was cut off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterCutOff {
    KeepCalls,
    Retry(String),
}

pub fn after_cut_off(tools_active: bool, calls: usize) -> AfterCutOff {
    if tools_active && calls > 0 {
        AfterCutOff::KeepCalls
    } else {
        AfterCutOff::Retry(CUTOFF_RETRY.to_string())
    }
}
