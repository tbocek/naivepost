//! Frames per video (spec/04-prepare.md F1.6): what happens between a footage source and
//! `prepare/inputs/frames/<source>/`.

use std::path::Path;

use crate::clock::frame_name;
use crate::layout::Tree;
use crate::prepare::EXTRACTION_GRID;
use crate::requests::{self, SceneChange};

/// The extraction grid.
pub const GRID: f64 = EXTRACTION_GRID;
/// A scene change is a frame whose scdet score is over this, on the 4-per-second stream.
pub const SCENE_THRESHOLD: f64 = 1.0;
/// Changes nearer than this merge into the first.
pub const SCENE_MIN_GAP: f64 = 0.5;
/// The scene pass looks at four frames a second.
pub const SCENE_FPS: u32 = 4;
/// …down-scaled to this width for the analysis only.
pub const SCENE_WIDTH: u32 = 640;
/// Parallel extraction, clamped: fewer than two workers buys nothing on a machine with cores to spare,
/// and more than eight stops being ffmpeg processes and starts being disk contention.
pub const WORKERS_MIN: usize = 2;
pub const WORKERS_MAX: usize = 8;
/// A job this small is one worker's: each worker is a fresh ffmpeg process (and its own seek into the
/// file), so splitting twenty frames across four of them spends more on start-up than it saves.
pub const TINY_JOB_FRAMES: usize = 20;
/// Frames per worker below which extraction is one ffmpeg however many cores there are.
pub const WORKER_MIN_FRAMES: usize = 4;
/// The width the frames actually sent to the describe model are scaled to for `.llmframes/`: measured
/// on the models Prepare uses, 896 px sits inside what they take.
pub const LLM_FRAME_WIDTH: u32 = 896;
pub const STORED_FRAME_JPEG_QUALITY: u32 = 2;

/// S1: does the marker say these are the frames this grid and threshold would produce?
pub fn marker_matches(found: Option<(f64, f64)>, grid: f64, threshold: f64) -> bool {
    match found {
        Some((found_grid, found_threshold)) => {
            (found_grid - grid).abs() < 1e-9 && (found_threshold - threshold).abs() < 1e-9
        }
        None => false,
    }
}

/// S1: the skip, said out loud.
pub fn already_log(base: &str, grid: f64, threshold: f64) -> String {
    format!(">>> [{base}] frames already extracted ({grid} s grid, {threshold} scene threshold), skipping")
}

/// S2: ffmpeg's arguments for the whole-video scene pass.
pub fn scene_pass_plan() -> Vec<String> {
    // The threshold is deliberately not passed to ffmpeg: `scdet=threshold=0` reports every frame's
    // score, and [`scene_changes`] applies `P.machine.sceneThreshold` to those scores.
    vec![
        "-vf".to_string(),
        format!("fps={SCENE_FPS},scale={SCENE_WIDTH}:-2,scdet=threshold=0,metadata=print:file=-"),
        "-an".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ]
}

fn trim_number(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        return format!("{}", secs.round() as i64);
    }
    let text = format!("{secs:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Where the video's pictures end, from the scene sweep (S2): its last scored frame.
pub fn picture_end(container: f64, scores: &[(f64, f64)]) -> f64 {
    match scores.iter().map(|(at, _)| *at).reduce(f64::max) {
        Some(last) => (last + 1e-6).min(container),
        None => container,
    }
}

/// S2: the scores out of ffmpeg's `metadata=print` output, as `(time, score)` pairs.
pub fn parse_scores(log: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut at: Option<f64> = None;
    for line in log.lines() {
        if let Some(second) = line.split_whitespace().find_map(|field| {
            field
                .strip_prefix("pts_time:")
                .or(field.strip_prefix("pts_time="))
        }) {
            at = second.parse().ok();
        }
        let score = line
            .split_whitespace()
            .find_map(|field| field.rsplit_once("lavfi.scd.score="))
            .map(|(_, value)| value);
        if let (Some(at), Some(score)) = (at, score) {
            if let Ok(score) = score.parse::<f64>() {
                out.push((at, score));
            }
        }
    }
    out
}

/// S2: the scene changes, `time\tscore` rows for `scenes.tsv`.
pub fn scene_changes(scored: &[(f64, f64)], threshold: f64, min_gap: f64) -> Vec<SceneChange> {
    let mut over: Vec<(f64, f64)> = scored
        .iter()
        .filter(|(_, score)| *score > threshold)
        .copied()
        .collect();
    over.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut kept: Vec<SceneChange> = Vec::new();
    for (time, score) in over {
        match kept.last() {
            Some(last) if time - last.time < min_gap => continue,
            _ => kept.push(SceneChange { time, score }),
        }
    }
    kept
}

/// S3: the seconds to pull a frame from.
pub fn frame_seconds(changes: &[f64], duration: f64, grid: f64) -> Vec<f64> {
    if duration <= grid {
        return match duration > 0.0 {
            true => vec![0.0],
            false => Vec::new(),
        };
    }
    let mut starts: Vec<f64> = vec![0.0];
    starts.extend(changes.iter().filter(|at| **at > 0.0 && **at < duration));
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<f64> = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(duration);
        if *start >= duration || end <= *start {
            continue;
        }
        let steps = ((end - start) / grid).floor();
        for step in 0..=steps.max(0.0) as u64 {
            let at = start + grid * step as f64;
            if at >= duration {
                break;
            }
            if out.last().is_some_and(|last| (*last - at).abs() < 1e-9) {
                continue;
            }
            out.push(at);
        }
    }
    out
}

/// S3: whether this take is shorter than one step of the grid, in which case its first frame is its
/// only one ([`frame_seconds`] explains why the rule is about the take and not about each scene).
pub fn short_take_seconds(duration: f64, grid: f64) -> bool {
    duration > 0.0 && duration <= grid
}

/// S3: how many extraction processes to run.
pub fn workers(cpus: usize, jobs: usize) -> usize {
    if jobs <= TINY_JOB_FRAMES {
        return 1;
    }
    (cpus / 4).clamp(WORKERS_MIN, WORKERS_MAX)
}

/// S3: the frames each worker takes — contiguous chunks, none empty, in order, so every frame is
/// extracted by exactly one worker and the names come back in the plan's own order.
pub fn chunks(indexes: &[usize], workers: usize) -> Vec<Vec<usize>> {
    if indexes.is_empty() {
        return Vec::new();
    }
    let workers = workers.max(1).min(indexes.len());
    let each = indexes.len().div_ceil(workers);
    indexes.chunks(each).map(|chunk| chunk.to_vec()).collect()
}

/// S4: a frame's file name — its exact second on the file's own clock, `YYYY-MM-DD_HH-MM-SS.mmm.jpg`.
pub fn frame_file(start: f64, at: f64) -> String {
    format!("{}.jpg", frame_name(start + at))
}

/// S3/S4: ffmpeg's arguments, one command per chunk of frames.
pub fn extract_plan(input: &str, seconds: &[f64], dir: &Path, start: f64) -> Vec<Vec<String>> {
    seconds
        .iter()
        .map(|at| {
            let out = dir.join(frame_file(start, *at));
            vec![
                "-ss".to_string(),
                trim_number(*at),
                "-i".to_string(),
                input.to_string(),
                "-frames:v".to_string(),
                "1".to_string(),
                "-q:v".to_string(),
                STORED_FRAME_JPEG_QUALITY.to_string(),
                out.to_string_lossy().into_owned(),
            ]
        })
        .collect()
}

/// S3: how far along an extraction is, out of one line of ffmpeg's `-progress` output.
pub fn progress(line: &str) -> Option<f64> {
    let line = line.trim();
    let (key, value) = line.split_once('=')?;
    let value = value.trim().parse::<f64>().ok()?;
    match key {
        "out_time_us" | "out_time_ms" => Some(value / 1_000_000.0),
        _ => None,
    }
}

/// What one pass produced.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub logs: Vec<String>,
    /// What the progress bar was told, in order — F0.5 owns the bar.
    pub steps: Vec<String>,
    /// Every frame's file name, in the plan's order.
    pub frames: Vec<String>,
    /// The rows of `scenes.tsv`, which is not counting the start of the file as a change.
    pub changes: usize,
    pub skipped: bool,
    pub workers: usize,
}

/// S5: what a pass that ran says when it is over.
pub fn done_log(base: &str, frames: usize, changes: usize) -> String {
    format!(">>> [{base}] {frames} frames, {changes} scene changes")
}

/// S1-S5 for one footage source.
pub fn run<Ex>(
    tree: &Tree,
    source: &str,
    base: &str,
    duration: f64,
    start: f64,
    threshold: f64,
    grid: f64,
    cpus: usize,
    scene_log: &str,
    mut extract: Ex,
) -> Result<Outcome, String>
where
    Ex: FnMut(&[f64]) -> Result<usize, String>,
{
    let mut out = Outcome::default();

    let found = requests::read_frames_marker(tree, source)?;
    if marker_matches(found, grid, threshold) {
        out.skipped = true;
        out.logs.push(already_log(base, grid, threshold));
        return Ok(out);
    }

    let changes = scene_changes(&parse_scores(scene_log), threshold, SCENE_MIN_GAP);
    requests::write_scenes(tree, source, &changes)?;
    out.changes = changes.len();

    let seconds = frame_seconds(
        &changes.iter().map(|change| change.time).collect::<Vec<_>>(),
        duration,
        grid,
    );
    let workers = workers(cpus, seconds.len());
    out.workers = workers;
    let mut written = 0usize;
    for chunk in chunks(&(0..seconds.len()).collect::<Vec<_>>(), workers) {
        out.steps.push(progress_log(out.steps.len() + 1, workers));
        // This worker's share of the frames, as seconds: the caller runs `extract_plan`'s commands for
        // exactly these and answers how many files it wrote.
        let taken: Vec<f64> = chunk.iter().map(|at| seconds[*at]).collect();
        written += extract(&taken)?;
    }

    requests::write_frames_marker(tree, source, grid, threshold)?;
    out.frames = seconds.iter().map(|at| frame_file(start, *at)).collect();
    out.logs.push(done_log(base, written, changes.len()));
    Ok(out)
}

/// S3: what the progress bar says while the extraction runs.
pub fn progress_log(chunk: usize, workers: usize) -> String {
    format!("extracting frames {chunk}/{workers}")
}
