//! The frames door: §F1.6's media legs, as [`crate::audio_task`] owns audio.cpp's.
//!
//! [`crate::frames`] holds every rule of the pass and takes its legs as closures; nothing there runs a process. This file does: it probes the video's length, sweeps it at
//! four frames a second through `scdet`, hands the scores to `frames::run`, and runs `frames::extract_plan`'s commands per chunk so the pictures exist before the `.frames` marker says so.
//!
//! The row is a DIAL, not a spawned process: its marker program ([`FRAMES_TASK`]) tells [`crate::run_stages::drive_walk`] to call rather than fork, since one row is many ffmpeg calls whose output has to be read.

use std::path::Path;

use crate::layout::Tree;
use crate::project::Project;
use crate::settings::{self, Paths};
use crate::{frames, layout, requests, run_stages, subprocess};

/// The marker [`crate::run_stages::drive_walk`] reads as “run this row's frames pass”: the fourth dial kind, beside `AUDIO_TASK`, `VISION_TASK` and `TEXT_TASK`.
pub const FRAMES_TASK: &str = "naivepost:frames";

/// One footage row per video, each a dial of this module. The index rides along because a frame's name is its second on the FILE's clock (§F1.6 S4),
/// which needs the offset over the whole session, so the door recomputes the placement and picks this row's entry by position, as [`crate::describe_call::stage`].
pub fn stage(project: &Project) -> run_stages::Stage {
    let tasks = project
        .sources
        .iter()
        .enumerate()
        .filter(|(_, source)| source.footage)
        .map(|(index, source)| run_stages::Task {
            label: format!("framing {}", crate::audio_task::source_lane(&source.path)),
            program: FRAMES_TASK.to_string(),
            args: vec![source.path.clone(), index.to_string()],
            already_done: false,
            session: None,
        })
        .collect();
    run_stages::Stage {
        track: crate::runqueue::TRACK_FRAMES,
        name: "frames",
        phase: 1,
        of: 2,
        kind: "frame",
        tasks,
    }
}

/// The walk's door for one row: resolve the settings folder and session as the app does, then run the pass. A row naming no source, or whose video is gone, stops naming the missing file (§Failure-is-specific).
pub fn run_task(task: &crate::run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let paths = settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = crate::startup::open_dir();
    let tree = Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to frame into", session.display()))?;
    let source = task
        .args
        .first()
        .cloned()
        .ok_or("the frames row names no source")?;
    let index: usize = task
        .args
        .get(1)
        .and_then(|text| text.parse().ok())
        .ok_or("the frames row names no place on the session clock")?;
    let lane = crate::audio_task::source_lane(&source);
    let source_path = layout::resolve(tree.dir(), tree.dir(), &source);
    if !source_path.exists() {
        return Err(format!(
            "{lane}: no video to frame -- {} is not there",
            source_path.display()
        ));
    }
    stage_pass(&tree, &paths, &lane, &source_path, index, log)?;
    Ok(())
}

/// The whole pass for one video: probe, sweep, extract, mark. Returns what [`crate::frames`] recorded; its lines go to `log` here, as `audio_task::transcribe_asr` does.
pub fn stage_pass(
    tree: &Tree,
    paths: &Paths,
    lane: &str,
    source_path: &Path,
    index: usize,
    log: &mut impl FnMut(&str),
) -> Result<frames::Outcome, String> {
    let conf = settings::read(paths).unwrap_or_default();
    // The configured ffmpeg, else the bare name (as `separate_media::program`).
    let ffmpeg = crate::separate_media::program(&conf.ffmpeg);
    let duration = run_stages::probe_duration(&conf.ffmpeg, source_path, log)?;
    let start = clock_start(tree, index);
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    // S1 first: the marker is the resume marker, and a matching pass owes one sentence and no processes, not even the sweep.
    if frames::marker_matches(
        requests::read_frames_marker(tree, lane)?,
        frames::GRID,
        frames::SCENE_THRESHOLD,
    ) {
        log(&frames::already_log(
            lane,
            frames::GRID,
            frames::SCENE_THRESHOLD,
        ));
        let renamed = rename_session_named(tree, lane, session_zero(tree))?;
        if renamed > 0 {
            log(&format!(
                ">>> [{lane}] {renamed} frame(s) renamed from session seconds to the recording's own date and time"
            ));
        }
        return Ok(frames::Outcome {
            skipped: true,
            ..Default::default()
        });
    }
    // Counts chunks as they are handed over, so the door can print `frames::progress_log`'s string before the ffmpeg calls it stands for; the total is filled in once the seconds are known.
    let next_chunk = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let chunk_total = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));

    // S2: the sweep over the WHOLE video. `metadata=print:file=-` puts every frame's score on stdout, which `subprocess::run` hands back; `-f null` throws the pixels away.
    let mut sweep: Vec<String> = vec!["-i".to_string(), source_path.display().to_string()];
    sweep.extend(frames::scene_pass_plan());
    let owned: Vec<&str> = sweep.iter().map(String::as_str).collect();
    let scores = subprocess::run(&ffmpeg, &owned, &mut *log)?;
    // The grid runs to the last picture the sweep saw, not to the end of the sound.
    let duration = frames::picture_end(duration, &frames::parse_scores(&scores));
    // How many chunks the plan will make, so each progress line can say `i/n` as `frames::run` records it.
    let changes_for_count = frames::scene_changes(
        &frames::parse_scores(&scores),
        frames::SCENE_THRESHOLD,
        frames::SCENE_MIN_GAP,
    );
    let planned = frames::frame_seconds(
        &changes_for_count
            .iter()
            .map(|change| change.time)
            .collect::<Vec<_>>(),
        duration,
        frames::GRID,
    );
    chunk_total.store(
        frames::chunks(
            &(0..planned.len()).collect::<Vec<_>>(),
            frames::workers(cpus, planned.len()),
        )
        .len()
        .max(1),
        std::sync::atomic::Ordering::SeqCst,
    );

    let dir = tree.frames_dir(lane);
    let input = source_path.display().to_string();
    let outcome = frames::run(
        tree,
        lane,
        lane,
        duration,
        start,
        frames::SCENE_THRESHOLD,
        frames::GRID,
        cpus,
        &scores,
        |seconds| {
            // One chunk of the pass: its progress line first, then the extractions it stands for.
            let step = frames::progress_log(
                next_chunk.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                chunk_total.load(std::sync::atomic::Ordering::SeqCst),
            );
            extract_chunk(
                &ffmpeg, &input, lane, seconds, &dir, start, duration, cpus, &step, log,
            )
        },
    )?;
    for line in &outcome.logs {
        log(line);
    }
    Ok(outcome)
}

/// This row's first second on the session clock. `fix_transcripts::placement` needs the WHOLE session (leaving one file out slides every other), so it is computed over every
/// source in project order and this row's entry picked by position. An unreadable project, or a row past its end, starts at zero (§8): inventing an offset would name frames at seconds no camera saw.
fn clock_start(tree: &Tree, index: usize) -> f64 {
    let Ok(project) = crate::project::load(tree.dir()) else {
        return 0.0;
    };
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    let clock = crate::clock::clock(&names);
    if index >= names.len() {
        return clock.zero;
    }
    clock.zero + clock.offset(index)
}

/// The session's zero (the earliest recording's stamp), for turning frame names back into session seconds ([`crate::clock::frame_session_seconds`]); 0 when unreadable or unstamped.
pub fn session_zero(tree: &Tree) -> f64 {
    let Ok(project) = crate::project::load(tree.dir()) else {
        return 0.0;
    };
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    crate::clock::clock(&names).zero
}

/// A folder an old extraction left with session seconds printed as 1970 dates: renamed in place to real instants (session second plus zero), scaled copies too, so a resumed run does not re-extract.
/// Answers how many were renamed.
fn rename_session_named(tree: &Tree, lane: &str, zero: f64) -> Result<usize, String> {
    if zero < crate::clock::SESSION_NAMED_BEFORE {
        return Ok(0);
    }
    let mut renamed = 0usize;
    for (dir, shift) in [
        (tree.frames_dir(lane), zero),
        (tree.llm_frames_dir(lane), f64::NAN),
    ] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        // The scaled copies are named from their folder's first picture (`describe::scaled_name`).
        let shift = if shift.is_nan() {
            crate::describe_scale::name_base(tree, lane)
        } else {
            shift
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".jpg") else {
                continue;
            };
            let Some(at) = crate::clock::read_frame(stem) else {
                continue;
            };
            if at >= crate::clock::SESSION_NAMED_BEFORE {
                continue;
            }
            let to = dir.join(format!("{}.jpg", crate::clock::frame_name(at + shift)));
            std::fs::rename(entry.path(), &to)
                .map_err(|why| format!("{}: {why}", entry.path().display()))?;
            renamed += 1;
        }
    }
    Ok(renamed)
}

/// One chunk's frames, extracted for real: `frames::extract_plan`'s command per frame, each with `-progress pipe:1` (S3) parsed through [`frames::progress`] and said once per chunk.
///
/// Frames go out on scoped threads bounded by the spec's worker count (one ffmpeg per frame on a 250 ms grid is many short processes). Each thread collects its own log lines, appended in order after the join:
/// `log` is `&mut dyn FnMut` and cannot cross threads, and interleaved raw lines would read as noise.
#[allow(clippy::too_many_arguments)]
fn extract_chunk(
    ffmpeg: &str,
    input: &str,
    lane: &str,
    seconds: &[f64],
    dir: &Path,
    start: f64,
    end: f64,
    cpus: usize,
    chunk_step: &str,
    log: &mut impl FnMut(&str),
) -> Result<usize, String> {
    if seconds.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    let plans = frames::extract_plan(input, seconds, dir, start);
    // Pass-level chunks are already `frames::workers` wide; the same clamp inside a chunk keeps a two-frame chunk from spawning twenty threads.
    let threads =
        frames::workers(cpus, seconds.len().max(frames::TINY_JOB_FRAMES + 1)).min(plans.len());
    let mut written = 0usize;
    let mut notes: Vec<String> = Vec::new();
    // The per-chunk progress goes first, beside this door's own lines about that chunk.
    log(chunk_step);
    // Every frame's path, taken from its argv before `-progress` is appended: the file's existence afterwards is the only proof the frame exists.
    let expected: Vec<String> = plans
        .iter()
        .map(|plan| plan.last().cloned().unwrap_or_default())
        .collect();
    // Scoped so the borrow of `plans` outlives every thread; a failing ffmpeg is carried back as a value.
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        // Chunk (command, expected file) pairs together so a worker knows which file its command should write.
        let pairs: Vec<(&Vec<String>, &String)> = plans.iter().zip(expected.iter()).collect();
        let share = pairs.chunks(pairs.len().div_ceil(threads.max(1)));
        for group in share {
            let ffmpeg = ffmpeg.to_string();
            let lane_name = lane.to_string();
            let group: Vec<(&Vec<String>, String)> = group
                .iter()
                .map(|(plan, out)| (*plan, (*out).clone()))
                .collect();
            handles.push(scope.spawn(move || {
                let mut lines: Vec<String> = Vec::new();
                let mut count = 0usize;
                let mut furthest: Option<f64> = None;
                let mut failure: Option<String> = None;
                for (plan, out) in group {
                    let mut argv = plan.clone();
                    // `-progress` goes last: ffmpeg takes it as an output option, beside the file it reports on.
                    argv.extend(["-progress".to_string(), "pipe:1".to_string()]);
                    let owned: Vec<&str> = argv.iter().map(String::as_str).collect();
                    match subprocess::run(&ffmpeg, &owned, |line| lines.push(line.to_string())) {
                        Ok(stdout) => {
                            // `-progress` prints `key=value` lines to the end; the last one carrying a time is how far this ffmpeg got.
                            for line in stdout.lines() {
                                if let Some(at) = frames::progress(line) {
                                    furthest = Some(furthest.map_or(at, |seen: f64| seen.max(at)));
                                }
                            }
                            // A command that exits 0 having written nothing is a failure.
                            if Path::new(&out).exists() {
                                count += 1;
                            } else if near_the_end(plan, end) {
                                // Past the last picture by under a second (a video stream stopping a frame short of the sweep's last score): said, not failed.
                                lines.push(format!(
                                    ">>> [{lane_name}] no picture at the very end ({}) -- skipped",
                                    Path::new(&out)
                                        .file_name()
                                        .map(|name| name.to_string_lossy().to_string())
                                        .unwrap_or_default()
                                ));
                            } else {
                                failure = Some(format!("{out}: ffmpeg wrote no frame"));
                                break;
                            }
                        }
                        Err(_) if near_the_end(plan, end) => {
                            lines.push(format!(
                                ">>> [{lane_name}] no picture at the very end -- skipped"
                            ));
                        }
                        Err(why) => {
                            failure = Some(why);
                            break;
                        }
                    }
                }
                (count, furthest, lines, failure)
            }));
        }
        for handle in handles {
            let (count, furthest, lines, failure) = handle
                .join()
                .map_err(|_| "a frame extraction thread panicked".to_string())?;
            notes.extend(lines);
            if let Some(why) = failure {
                return Err(why);
            }
            written += count;
            if let Some(at) = furthest {
                notes.push(format!(
                    ">>> [{lane}] {count} frames extracted (ffmpeg {at:.1} s)"
                ));
            }
        }
        Ok::<(), String>(())
    })?;
    for note in &notes {
        log(note);
    }
    Ok(written)
}

/// Whether this extraction asks for a second within the last second of the take (`-ss <s>` against `end`, the take's picture end): the only place a missing picture is the video's short end.
fn near_the_end(plan: &[String], end: f64) -> bool {
    plan.iter()
        .position(|arg| arg == "-ss")
        .and_then(|at| plan.get(at + 1))
        .and_then(|second| second.parse::<f64>().ok())
        .is_some_and(|second| second + 1.0 >= end)
}
