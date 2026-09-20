//! Frames per video (spec/04-prepare.md F1.6): what happens between a footage source and
//! `prepare/inputs/frames/<source>/`.
//!
//! A scene pass finds where the picture changes; frames then come out on a fixed grid that restarts
//! at every one of those changes, so the first picture of a new slide is always there and the Cut
//! timeline has a picture down to its deepest zoom. As in [`crate::asr`] and [`crate::align`], no
//! process is spawned and no socket opened: both ffmpeg invocations are returned as arguments for the
//! caller to run, and the scene pass's answer arrives as its log text — a step that decided inside a
//! tool call could never be tested (spec/00-principles.md §5).

use std::path::Path;

use crate::clock::frame_name;
use crate::layout::Tree;
use crate::prepare::EXTRACTION_GRID;
use crate::requests::{self, SceneChange};

/// The extraction grid. `P.eng.frameGridSeconds` — fixed and independent of Freq, which is
/// [`crate::prepare`] holding the same number: at the deepest zoom (240 px/s) one frame is 60 px, so
/// the picture band has no black gaps.
pub const GRID: f64 = EXTRACTION_GRID;
/// A scene change is a frame whose scdet score is over this, on the 4-per-second stream.
/// `P.machine.sceneThreshold` (measured on the ETH lecture: 6 of 8 slide changes found at 1.0 where
/// the source-rate pass found 3 — the transitions are animated, so no single frame-to-frame step
/// stands out).
pub const SCENE_THRESHOLD: f64 = 1.0;
/// Changes nearer than this merge into the first. `P.machine.sceneMinGapSeconds` — an animated
/// transition scores over the threshold for several frames running, and each extra one would restart
/// the grid and cost a frame nobody asked for.
pub const SCENE_MIN_GAP: f64 = 0.5;
/// The scene pass looks at four frames a second. `fps=4` in [`scene_pass_plan`]: enough to catch an
/// animated transition, few enough that a four-hour session is not forty thousand frames of work.
pub const SCENE_FPS: u32 = 4;
/// …down-scaled to this width for the analysis only. The delivered frames are never scaled (S3): this
/// stream is thrown away the moment its scores are read, and a smaller picture makes scdet's mean
/// absolute difference cheaper without changing which frames stand out.
pub const SCENE_WIDTH: u32 = 640;
/// Parallel extraction, clamped: fewer than two workers buys nothing on a machine with cores to spare,
/// and more than eight stops being ffmpeg processes and starts being disk contention.
pub const WORKERS_MIN: usize = 2;
pub const WORKERS_MAX: usize = 8;
/// A job this small is one worker's: each worker is a fresh ffmpeg process (and its own seek into the
/// file), so splitting twenty frames across four of them spends more on start-up than it saves. The
/// spec says only "one for a tiny job"; twenty frames is under a second of extraction either way.
pub const TINY_JOB_FRAMES: usize = 20;

/// S1: does the marker say these are the frames this grid and threshold would produce? Anything else
/// — no marker, another grid, another threshold — means the pass runs again, because a frame set built
/// at another threshold is missing exactly the changes this one would have found.
pub fn marker_matches(found: Option<(f64, f64)>, grid: f64, threshold: f64) -> bool {
    match found {
        Some((found_grid, found_threshold)) => {
            (found_grid - grid).abs() < 1e-9 && (found_threshold - threshold).abs() < 1e-9
        }
        None => false,
    }
}

/// S1: the skip, said out loud. The spec writes the parenthesised part as `…`, so it names the two
/// numbers the marker holds — which is also what tells a user why this run was quick and the last one
/// was not.
pub fn already_log(base: &str, grid: f64, threshold: f64) -> String {
    format!(">>> [{base}] frames already extracted ({grid} s grid, {threshold} scene threshold), skipping")
}

/// S2: ffmpeg's arguments for the whole-video scene pass. The caller appends its own `-i <source>`.
///
/// `scdet=threshold=0` on the filter so *every* frame's score is reported and `P.machine.sceneThreshold`
/// is applied by [`scene_changes`] — one threshold rule then serves both this pass and any other caller
/// of the same scores. `metadata=print:file=-` is how `lavfi.scd.score` and its `pts_time` reach us;
/// `-f null -` throws the pixels away, since only the scores are wanted. Note that the 640-wide scale
/// belongs to this throwaway stream alone: nothing here scales the frames a project keeps (S3), which
/// stay at the video's own size.
pub fn scene_pass_plan(threshold: f64) -> Vec<String> {
    let _ = threshold;
    // The threshold is deliberately not passed to ffmpeg: `scdet=threshold=0` reports every frame's
    // score, and [`scene_changes`] applies `P.machine.sceneThreshold` to those scores. One threshold
    // rule then serves this pass and any other caller of the same numbers, and a change is judged the
    // same way whether it arrived from scdet's own log line or from the printed metadata.
    vec![
        "-vf".to_string(),
        format!("fps={SCENE_FPS},scale={SCENE_WIDTH}:-2,scdet=threshold=0,metadata=print:file=-"),
        "-an".to_string(),
        "-f".to_string(),
        "null".to_string(),
    ]
}

/// Seconds without a tail of zeroes: `0.25`, not `0.250000`.
fn trim_number(secs: f64) -> String {
    if (secs - secs.round()).abs() < 1e-9 {
        return format!("{}", secs.round() as i64);
    }
    let text = format!("{secs:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// S2: the scores out of ffmpeg's `metadata=print` output, as `(time, score)` pairs.
///
/// The shape is one frame per block — a `frame:N pts:… pts_time:T` line then its metadata tags — but
/// ffmpeg has printed the tags on their own lines and inline after the frame line in different builds,
/// so this walks the text once, remembering the last `pts_time` seen and pairing it with each
/// `lavfi.scd.score` that follows. A score with no time before it is dropped rather than guessed at:
/// a change placed at the wrong second restarts the grid in the wrong place.
pub fn parse_scores(log: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut at: Option<f64> = None;
    for line in log.lines() {
        if let Some(second) = line
            .split_whitespace()
            .find_map(|field| field.strip_prefix("pts_time:").or(field.strip_prefix("pts_time=")))
        {
            at = second.parse().ok();
        }
        // `tag:lavfi.scd.score=58.310` is the printed form; a bare key=value is what a caller piping
        // the metadata through its own filter would see.
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
///
/// A change is a frame whose score is *over* the threshold — 1.0 is not over 1.0, which is what makes
/// `P.machine.sceneThreshold` a bound rather than an estimate. Changes nearer than `min_gap` to the one
/// kept before them merge into it, keeping the earlier: that is where the change started, and the grid
/// restarting there covers the whole transition. The start of the file is always a scene start, but it
/// is not a row — §1's file holds "time and score of each change", and the first frame has no score, so
/// [`frame_seconds`] adds that start itself rather than the file claiming a change that never happened.
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

/// S3: the seconds to pull a frame from. The video's own start and every change are scene starts; in
/// each one, its first frame at its own second and then one every `grid` until the next change, which
/// is where the grid restarts — so the first picture of a new slide is always there rather than up to
/// a grid step late. Nothing past `duration` is asked for, since ffmpeg would answer such a seek with
/// an empty output file.
///
/// "A take shorter than one grid step → its first frame alone" is read as being about the *take*: a
/// recording of 0.1 s yields one frame and nothing else ([`short_take_seconds`]). Not about each scene,
/// because that would make `P.machine.sceneMinGapSeconds`'s merge pointless — an animated transition
/// scores over `P.machine.sceneThreshold` for several frames running, and dropping the frames after
/// such a change is exactly the picture the grid exists to have.
pub fn frame_seconds(changes: &[f64], duration: f64, grid: f64) -> Vec<f64> {
    // The whole take is shorter than one step of the grid: it has no timeline to place pictures on,
    // so its first frame is its only one.
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
        // The scene runs to the next change, or to the end of the file.
        let end = starts.get(index + 1).copied().unwrap_or(duration);
        if *start >= duration || end <= *start {
            continue;
        }
        // Its first frame, then one a grid step at a time. The count is computed from the length
        // rather than accumulated by adding `grid` each turn, so a long scene does not drift off the
        // grid through rounding.
        let steps = ((end - start) / grid).floor();
        for step in 0..=steps.max(0.0) as u64 {
            let at = start + grid * step as f64;
            if at >= duration {
                break;
            }
            // A step landing on the next change belongs to that scene, which starts there too: one
            // frame, named once.
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

/// S3: how many extraction processes to run. `clamp(CPU/4, 2, 8)` — four cores per ffmpeg because each
/// one decodes on its own threads — and one for a job too small to be worth splitting (see
/// [`TINY_JOB_FRAMES`]). A machine that reports no cores still gets the floor rather than zero workers,
/// which would extract nothing and log success.
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
    // Even chunks rather than full ones with a runt: one worker taking forty frames while seven take
    // five is a pass that waits for the forty.
    let each = indexes.len().div_ceil(workers);
    indexes.chunks(each).map(|chunk| chunk.to_vec()).collect()
}

/// S4: a frame's file name — its exact second on the file's own clock, `YYYY-MM-DD_HH-MM-SS.mmm.jpg`.
///
/// The instant alone names it, never a position in a sorted list: extracting one more frame must not
/// rename the ones already there. And no `-n` collision suffix is needed, because a 250 ms grid and a
/// millisecond stamp cannot ask for the same name twice ([`crate::clock`] keeps that rule).
pub fn frame_file(start: f64, at: f64) -> String {
    format!("{}.jpg", frame_name(start + at))
}

/// S3/S4: ffmpeg's arguments, one command per chunk of frames. Each frame is seeked with `-ss` before
/// its own `-i`, which on a keyframe-sparse screen recording lands on the exact second rather than the
/// nearest one, and written with `-frames:v 1`. There is no `scale`, no `-s` and no `fps` anywhere in
/// here: every frame is at the video's own size, and the smaller copies the model sees are made by F1.7.
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
                "2".to_string(),
                out.to_string_lossy().into_owned(),
            ]
        })
        .collect()
}

/// S3: how far along an extraction is, out of one line of ffmpeg's `-progress` output. Recorded rather
/// than drawn — F0.5 owns the bar, as [`crate::asr`] records its own step strings for.
pub fn progress(line: &str) -> Option<f64> {
    let line = line.trim();
    // ffmpeg prints `out_time_us` in current builds and `out_time_ms` in older ones, where the value is
    // microseconds anyway — a millisecond reading would put the bar a thousand times too fast.
    let (key, value) = line.split_once('=')?;
    let value = value.trim().parse::<f64>().ok()?;
    match key {
        "out_time_us" | "out_time_ms" => Some(value / 1_000_000.0),
        _ => None,
    }
}

/// What one pass produced. `skipped` is S1's marker match: nothing was extracted and nothing on disk
/// was touched.
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
///
/// `scene_log` is the whole-video pass's output ([`scene_pass_plan`]'s command run by the caller);
/// `extract` is handed one chunk's seconds and answers how many frames it wrote — the real ▶ runs
/// [`extract_plan`]'s commands through [`crate::subprocess::run`] across [`workers`] threads, a test
/// counts calls. `start` is this source's offset on the session clock ([`crate::clock`]), which is what
/// puts a frame's name on the file's own clock rather than on the session's.
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

    // S1: the marker is this stage's resume marker, so a match means the frames on disk are these
    // frames and nothing here is rewritten.
    let found = requests::read_frames_marker(tree, source)?;
    if marker_matches(found, grid, threshold) {
        out.skipped = true;
        out.logs.push(already_log(base, grid, threshold));
        return Ok(out);
    }

    // S2: the whole video in one pass — a change near the end is as much a scene start as one at the
    // beginning, and a chunked scene pass could not see across its own edges.
    let changes = scene_changes(&parse_scores(scene_log), threshold, SCENE_MIN_GAP);
    requests::write_scenes(tree, source, &changes)?;
    out.changes = changes.len();

    // S3: the grid, restarted at every change.
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

    // S5: the marker last, after every frame exists. Written earlier it would be a resume marker for
    // work that was never done — and unlike the prototype's `.interval`, this one promises scenes too.
    requests::write_frames_marker(tree, source, grid, threshold)?;
    out.frames = seconds.iter().map(|at| frame_file(start, *at)).collect();
    out.logs.push(done_log(base, written, changes.len()));
    Ok(out)
}

/// S3: what the progress bar says while the extraction runs. Recorded rather than drawn.
pub fn progress_log(chunk: usize, workers: usize) -> String {
    format!("extracting frames {chunk}/{workers}")
}
