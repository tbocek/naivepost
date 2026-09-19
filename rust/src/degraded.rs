//! §02-services.md §4 Degraded modes: what each missing piece does to a job.
//!
//! Six degradations, and the rule that runs through all of them is that a degraded run is still a run —
//! it answers with less and says so, rather than failing. So nothing here reaches a server or a process:
//! these are the decisions the callers make once a probe or an error has come back, kept plain enough to
//! test without either. Reuse over duplication: the web-tool question routes through [`crate::tools`],
//! the aligner question through [`crate::services`], and Flatpak's desktop-entry/icon/MIME rule already
//! lives in [`crate::settings`] (§7) — this module adds no second copy of it.

use crate::roles::Job;
use crate::tools::{self, Tool};

/// Reused rather than re-spelled: an edge is "near" a silence by the same reach the cut pass uses.
/// P.policy.snapToleranceSeconds
pub use crate::tools::cutpass::SNAP_TOLERANCE_SECONDS;

/// A run of samples at or under this is silence. P.machine.silenceThresholdDB
pub const SILENCE_THRESHOLD_DB: f64 = -35.0;

/// Shorter than this and it is a gap between words, not somewhere to cut. P.machine.silenceMinSeconds
pub const SILENCE_MIN_SECONDS: f64 = 0.4;

/// What the Settings test asks when sd.cpp's own route does not answer — see [`settings_test_path`].
pub const SETTINGS_FALLBACK: &str = "/v1/models";

/// `finish_reason` for a reply that ran into the token ceiling and was cut off mid-answer.
pub const FINISH_LENGTH: &str = "length";

/// The prototype's correction for a truncated answer (`gui/llm.go`'s `cutOff`), kept as one sentence so
/// the model sees the same words it saw in the prototype: the first half names what happened, the second
/// says what to do about it.
pub const CUTOFF_RETRY: &str = "your answer stopped in the middle -- it was too long to finish. \
                                Answer again with far fewer items";

/// §4.1 — where a cut edge may come from. With no aligner there are no word timings, so the waveform is
/// all there is to aim at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutSource {
    /// Word timings: an edge can land between two words of one breath.
    Words,
    /// Waveform only: clean wherever a silence offers itself, and nowhere else.
    Waveform,
}

/// §4.1 — which of the two the settings give. An empty or blank aligner is "none": the box defaults to
/// none and [`crate::services::pick_aligner`] answers `None` when the server's list holds no candidate,
/// and both mean the same thing here. A configured id means word timings are available.
pub fn cut_source(aligner: &str) -> CutSource {
    if aligner.trim().is_empty() {
        CutSource::Waveform
    } else {
        CutSource::Words
    }
}

/// A peak's level in dBFS: 20·log10 of the amplitude, so a full-scale peak is 0.0 and silence is minus
/// infinity. A zero peak is silence rather than `log(0)`, so it is answered directly — treating it as
/// very negative would be right by accident and wrong for any other tiny value.
fn dbfs(peak: f64) -> f64 {
    if peak <= 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * peak.log10()
}

/// §4.1 — the silences in a waveform, as (start, end) in seconds. Maximal runs at or under
/// `threshold_db`; a run shorter than `min_seconds` is not silence, it is the gap between two words, and
/// cutting there is exactly what §4.1 says this mode cannot do. `hz` is the peak rate — one sample per
/// entry of `peaks`, which is what [`crate::wave`] caches — so a run's length in samples divides by it.
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

/// §4.1 — where each wanted edge may actually be cut. An edge inside a silence stays put; one within
/// `tolerance` of a silence moves to its nearer end, which is the same reach the cut pass gives a word
/// edge. `None` is the mode's own limitation stated honestly: "unable to cut between two words of one
/// breath" — with no word timings there is nothing between them to aim at, and inventing an edge there
/// would be a click.
pub fn cut_points(
    wanted: &[f64],
    silences: &[(f64, f64)],
    tolerance: f64,
) -> Vec<Option<f64>> {
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

/// §4.2 and §4.5 — whether the web tools can be offered at all. Inside Flatpak there is no browser to
/// reach, and the firefox box says `off` (or is left empty) when the user has none. Everything else means
/// a path or command worth trying.
pub fn web_available(firefox: &str, flatpak: bool) -> bool {
    if flatpak {
        return false;
    }
    let firefox = firefox.trim();
    !firefox.is_empty() && !firefox.eq_ignore_ascii_case("off")
}

/// §4.2 — the job's tools with the web ones gone. This is "the model writes only what the material says"
/// made structural rather than promised: a tool that is never offered cannot be reached for, so there is
/// no prompt rule for the model to break and nothing to refuse it back. Order is `tools::offered`'s, so a
/// job's catalogue reads the same with and without a browser apart from the two missing rows.
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

/// §4.3 — a 404 or 410 from an sd.cpp job poll means the server does not know the job any more, which in
/// practice means it was restarted mid-draw. Any other status is an answer about a job that still exists.
pub fn forgot_job(status: u16) -> bool {
    matches!(status, 404 | 410)
}

/// §4.3 — the sentence for that, in the spec's own words: what was lost, with its status, and the reason
/// a user can act on. A bare "404" would be logged and mean nothing a week later.
pub fn forgot_job_message(id: &str, status: u16) -> String {
    format!("sd.cpp forgot job {id} ({status}) -- it was probably restarted mid-draw")
}

/// §4.3 — whether a server can draw. A server that reports no `supported_modes` at all is assumed able
/// to: the field is newer than most builds, and refusing to draw on its absence would break every install
/// that predates it. A list that names `draw` says yes; any other list is the server saying what it does
/// have, and `draw` is not in it.
pub fn can_draw(supported_modes: Option<&[&str]>) -> bool {
    match supported_modes {
        None => true,
        Some(modes) if modes.is_empty() => true,
        Some(modes) => modes.iter().any(|mode| *mode == "draw"),
    }
}

/// How a render ended. There is no third case: §4.3 says the render completes when sd.cpp is down, so a
/// failed thumbnail is a completed render with something to say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOutcome {
    /// Video and thumbnail both there.
    Complete,
    /// The video is there; the thumbnail half did not happen. Carries why.
    ThumbnailFailed(String),
}

/// §4.3 — what a render with a broken image half reports. `Err` from sd.cpp never fails the render: the
/// video is finished and must be kept, so this returns the failure as one log line naming the thumbnail
/// and the reason, and leaves the caller's render successful. A status that [`forgot_job`] recognises gets
/// its own sentence, because "job abc failed" would hide the restart behind a generic word.
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
        let line = "sd.cpp is down -- the thumbnail was not drawn, the video is complete".to_string();
        return (RenderOutcome::ThumbnailFailed(line.clone()), vec![line]);
    }
    (RenderOutcome::Complete, Vec::new())
}

/// The first status an sd.cpp sentence carries, spelled `HTTP <code>` — the form the prototype's client
/// writes. Reading only that form is deliberate: a bare four-digit run in a sentence is more likely to be a
/// byte count or part of a job id than a status, and mistaking one would rename a plain failure as a
/// forgotten job. `None` when there is no status at all, which is the ordinary case for "connection refused".
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

/// The job id in a sentence, which is whatever follows `job `. Empty when the reason does not name one —
/// the message then reads "forgot job  (…)", which is as honest as the input was.
fn parse_id(reason: &str) -> String {
    match reason.find("job ") {
        Some(at) => {
            let rest = &reason[at + 4..];
            let end = rest.find(|c: char| c == ' ' || c == '(').unwrap_or(rest.len());
            rest[..end].to_string()
        }
        None => String::new(),
    }
}

/// §4.3 — what else the Settings test asks. The sd.cpp route answering normally (or failing for a reason
/// that is not "no such endpoint") means there is nothing to fall back to, so `None`. A 404 from it means
/// something OpenAI-shaped is listening on that port instead of sd.cpp — a tenant, a proxy — and its model
/// list is what names it. Anything else stays unexplained rather than guessed at.
pub fn settings_test_path(sd_probe_status: u16) -> Option<&'static str> {
    match sd_probe_status {
        404 => Some(SETTINGS_FALLBACK),
        _ => None,
    }
}

/// §4.4 — an error that means "this server does not do `tools`". It has to name tools, because every other
/// failure is a different problem with a different fix. And it must not be a stop: a stop is the model
/// finishing its turn, which is an answer rather than a rejection of the schema — treating it as a refusal
/// would drop a whole job onto the JSON path because the model had nothing more to say.
pub fn tools_refused(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    if lower.contains("stop") || lower.contains("end of turn") {
        return false;
    }
    lower.contains("tool")
}

/// What a `tools` problem means for the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolsFallback {
    /// §4.4 — run the job as one JSON answer, with the prototype's parsers and retry turns.
    RunAsOneJsonAnswer,
    /// §4.4 — a refusal after the first round fails the call: half a tool conversation cannot be redone as
    /// one answer without throwing away what was already asked for.
    FailCall,
    /// Not a tools problem; keep going with tools.
    KeepTools,
}

/// §4.4 — which of those. Rounds count from 1. A refusal on the first round is cheap: nothing has been
/// asked for yet, so the job can be answered in one JSON reply instead. On any later round the call fails,
/// because by then the conversation is already a tool conversation and §4 says so rather than quietly
/// changing shape mid-job.
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

/// §4.6 — whether a reply was cut off at the token limit. Only `length` says that: "stop" is a finished
/// answer, and anything else is somebody else's problem. A truncated reply still has text in it — that is
/// what makes it dangerous — so this does not look at `has_text`; the parameter is there for the caller
/// that wants to log how much of an answer survived.
pub fn cut_off(finish_reason: &str, has_text: bool) -> bool {
    let _ = has_text;
    finish_reason == FINISH_LENGTH
}

/// What to do with a reply that was cut off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterCutOff {
    /// §4.6 — with tools, everything already called stands: the calls were answered and their results are
    /// in the conversation, so the job has them whatever the last reply looked like.
    KeepCalls,
    /// §4.6 — without tools there is no partial result to keep, only a truncated answer to reject.
    Retry(String),
}

/// §4.6 — which of those. `calls` is how many tool calls this job has already made; zero means the tools
/// path produced nothing to stand, so even with tools active the answer has to be asked for again.
pub fn after_cut_off(tools_active: bool, calls: usize) -> AfterCutOff {
    if tools_active && calls > 0 {
        AfterCutOff::KeepCalls
    } else {
        AfterCutOff::Retry(CUTOFF_RETRY.to_string())
    }
}
