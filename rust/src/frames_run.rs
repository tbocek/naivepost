//! The frames door: §F1.6's media legs, the way [`crate::audio_task`] owns audio.cpp's and
//! [`crate::separate_run`] owns separation's.
//!
//! [`crate::frames`] holds every rule of the pass — which marker means "already done", what ffmpeg is
//! asked for the scene sweep, which seconds a frame comes from, how the names are spelled, how many
//! workers — and takes its legs as closures, exactly as `asr`/`align`/`transcribe` do. Nothing in that
//! module runs a process. This file is the half that does: it probes the video's length, sweeps it at
//! four frames a second through `scdet`, hands the scores to `frames::run`, and for each chunk runs
//! `frames::extract_plan`'s commands so the pictures exist on disk before the `.frames` marker says
//! they do.
//!
//! The row is a DIAL, not a spawned process: like the speech and describe rows it carries a marker
//! program ([`FRAMES_TASK`]) that [`crate::run_stages::drive_walk`] reads as "call this" rather than
//! "fork this", because one row is many ffmpeg calls (one sweep plus one per frame) whose output has to
//! be read, not discarded.

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::project::Project;
use crate::settings::{self, Paths};
use crate::{frames, layout, requests, run_stages, subprocess};

/// The marker [`crate::run_stages::drive_walk`] reads as “run this row's frames pass” rather than
/// “spawn this”: the fourth of the dial kinds, beside `AUDIO_TASK`, `VISION_TASK` and `TEXT_TASK`.
pub const FRAMES_TASK: &str = "naivepost:frames";

/// One footage row per video, each a dial of this module. The index rides along because a frame's name
/// is its second on the FILE's clock (§F1.6 S4), which is the source's offset on the session clock, and
/// an offset can only be computed over the whole session — so the door recomputes the placement and
/// picks this row's entry out by position, exactly as [`crate::describe_call::stage`] passes it.
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

/// The walk's door for one row: resolve the settings folder and the session exactly as the app does, then
/// run the pass. A row naming no source, or one whose video is gone, stops with the reason named — §Failure-
/// is-specific would rather say which file is missing than blame the tool that never got to run.
pub fn run_task(task: &crate::run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let paths = settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let session = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
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

/// The whole pass for one video: probe, sweep, extract, mark. Returns what [`crate::frames`] recorded so
/// a caller can count frames without walking the folder; the lines it produced go to `log` here, the same
/// forwarding `audio_task::transcribe_asr` does for the ASR's outcome.
pub fn stage_pass(
    tree: &Tree,
    paths: &Paths,
    lane: &str,
    source_path: &Path,
    index: usize,
    log: &mut impl FnMut(&str),
) -> Result<frames::Outcome, String> {
    let conf = settings::read(paths).unwrap_or_default();
    // The configured box when Settings names one, the bare name otherwise — the rule every ffmpeg call
    // site in the program uses (`separate_media::program`).
    let ffmpeg = crate::separate_media::program(&conf.ffmpeg);
    let duration = run_stages::probe_duration(&conf.ffmpeg, source_path, log)?;
    let start = clock_start(tree, index);
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    // S1 first, before anything is asked of ffmpeg: the marker is this stage's resume marker, and a pass
    // that matched it owes the run one sentence and no processes — not even the sweep, whose scores are
    // what `scenes.tsv` already records.
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
        return Ok(frames::Outcome {
            skipped: true,
            ..Default::default()
        });
    }
    // Counts the chunks as they are handed over, so the door can put `frames::progress_log`'s string in
    // front of the ffmpeg calls it stands for. The total is the number of chunks the plan makes, which is
    // why it is filled in by `stage_pass` once the seconds are known rather than here.
    let next_chunk = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let chunk_total = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));

    // S2: the sweep over the WHOLE video. `metadata=print:file=-` puts every frame's score on stdout,
    // which is what `subprocess::run` hands back, so the returned string IS the score report; nothing
    // else about this call is kept, since `-f null` throws the pixels away.
    let mut sweep: Vec<String> = vec!["-i".to_string(), source_path.display().to_string()];
    sweep.extend(frames::scene_pass_plan(frames::SCENE_THRESHOLD));
    let owned: Vec<&str> = sweep.iter().map(String::as_str).collect();
    let scores = subprocess::run(&ffmpeg, &owned, &mut *log)?;
    // How many chunks the plan will make, so each one's progress line can say `i/n` like `frames::run`
    // records it — the door logs those lines itself (in front of the ffmpeg calls they stand for), so it
    // has to know the denominator the same way the rule does.
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

    let dir = frames_dir(tree, lane);
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
                &ffmpeg, &input, lane, seconds, &dir, start, cpus, &step, log,
            )
        },
    )?;
    for line in &outcome.logs {
        log(line);
    }
    Ok(outcome)
}

/// This row's first second on the session clock. `fix_transcripts::placement` needs the WHOLE session —
/// leaving one file out moves zero and slides every other file — so the placement is computed over every
/// source name in project order and this row's entry is picked out by position. A project that cannot be
/// read, or a row past the end of it, starts at zero: an unstamped file sits at the session's start
/// anyway (§8), and inventing an offset would name frames at seconds no camera saw.
fn clock_start(tree: &Tree, index: usize) -> f64 {
    let Ok(project) = crate::project::load(tree.dir()) else {
        return 0.0;
    };
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    crate::fix_transcripts::placement(&names)
        .into_iter()
        .nth(index)
        .unwrap_or(0.0)
}

/// One chunk's worth of frames, extracted for real: `frames::extract_plan`'s command per frame, each
/// carrying `-progress pipe:1` so ffmpeg's own idea of how far it got reaches us (S3), parsed through
/// [`frames::progress`] and said once per chunk rather than per line.
///
/// The frames of one chunk go out on scoped threads bounded by the spec's worker count, because one
/// ffmpeg per frame on a 250 ms grid is a lot of short processes and they spend their time decoding
/// independently of each other. Each thread collects its own log lines into its own `Vec` and they are
/// appended in order after the join: `log` is `&mut dyn FnMut` and cannot cross a thread boundary, and
/// interleaving raw lines from four ffmpeg runs into one log would read as noise.
#[allow(clippy::too_many_arguments)]
fn extract_chunk(
    ffmpeg: &str,
    input: &str,
    lane: &str,
    seconds: &[f64],
    dir: &Path,
    start: f64,
    cpus: usize,
    chunk_step: &str,
    log: &mut impl FnMut(&str),
) -> Result<usize, String> {
    if seconds.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    let plans = frames::extract_plan(input, seconds, dir, start);
    // The pass-level chunks are already `frames::workers` wide; inside one chunk the same clamp applies
    // to its own frames, so a two-frame chunk does not spawn twenty threads.
    let threads =
        frames::workers(cpus, seconds.len().max(frames::TINY_JOB_FRAMES + 1)).min(plans.len());
    let mut written = 0usize;
    let mut notes: Vec<String> = Vec::new();
    // The per-chunk progress goes first: `frames::run` records it as each chunk is handed over, and this
    // door's own lines about that chunk belong beside it rather than after every ffmpeg in the stage
    // (which is what draining `Outcome.steps` at the end would have done).
    log(chunk_step);
    // Every frame's path, taken from its own argv before `-progress` is appended to the call below:
    // `extract_plan` ends each command with the file it writes, and that file's existence afterwards is
    // the only proof the frame exists.
    let expected: Vec<String> = plans
        .iter()
        .map(|plan| plan.last().cloned().unwrap_or_default())
        .collect();
    // Scoped so the borrow of `plans` outlives every thread, and a failing ffmpeg is carried back as a
    // value rather than unwinding through another worker.
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        // Chunk the (command, expected file) pairs together so a worker always knows which file its own
        // command was supposed to write.
        let pairs: Vec<(&Vec<String>, &String)> = plans.iter().zip(expected.iter()).collect();
        let share = pairs.chunks(pairs.len().div_ceil(threads.max(1)));
        for group in share {
            let ffmpeg = ffmpeg.to_string();
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
                    // `-progress` goes last: ffmpeg takes it as an output option, so it belongs beside the
                    // file it reports on (which is the argument just before it).
                    argv.extend(["-progress".to_string(), "pipe:1".to_string()]);
                    let owned: Vec<&str> = argv.iter().map(String::as_str).collect();
                    match subprocess::run(&ffmpeg, &owned, |line| lines.push(line.to_string())) {
                        Ok(stdout) => {
                            // `-progress` prints `key=value` lines all the way to the end; the last one
                            // that carries a time is how far this frame's ffmpeg got.
                            for line in stdout.lines() {
                                if let Some(at) = frames::progress(line) {
                                    furthest = Some(furthest.map_or(at, |seen: f64| seen.max(at)));
                                }
                            }
                            // A command that exits 0 having written nothing is a failure, not a success:
                            // the file's existence is the only proof the frame exists.
                            if Path::new(&out).exists() {
                                count += 1;
                            } else {
                                failure = Some(format!("{out}: ffmpeg wrote no frame"));
                                break;
                            }
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

/// The folder one source's frames live in — [`Tree::frames_dir`], the same folder that holds `.frames`
/// and `scenes.tsv`, named here so the door's own path rule is readable beside the calls that use it.
pub fn frames_dir(tree: &Tree, lane: &str) -> PathBuf {
    tree.frames_dir(lane)
}
