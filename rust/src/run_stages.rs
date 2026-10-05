//! F0.5 S2/S3 — the walk that puts a step's work on the bar and runs it under the bookkeeping.
//!
//! `runqueue` holds the vocabulary (`job`/`push`/`take`/`prog`/`done`, `checkpoint`) and nothing
//! else walks it: before this module a ▶ press opened the run, printed its lines, and left the
//! progress bar at zero with no task ever taken. §F0.5 says *every* step uses this bookkeeping, so
//! one place turns a step into stages of tasks and drives them in the order the spec draws:
//! `qJob → qPush → (checkpoint → qTake → run → prog)* → qDone`.
//!
//! Two rules decide everything here:
//! - **The checkpoint is between tasks, never inside one** (§S3/S4). A pause therefore costs one
//!   [`runqueue::CHECKPOINT_POLL`] per poll and a stop lands in the gap, where nothing half-written
//!   can be corrupted and a killed subprocess is not a failure.
//! - **A task is taken whatever becomes of it** (§S2's parenthetical). Work already on disk from an
//!   earlier run is still a task this run is done with; skipping the `take` would stall the bar at
//!   the resume point forever.
//!
//! The commands are real argv built by the modules that own each tool ([`crate::frames`],
//! [`crate::asr`], [`crate::align`]) and spawned through [`crate::subprocess`], which logs the
//! command as a shell would take it *before* running it. Nothing here invents a command line and
//! nothing here answers for a tool that did not run: if ffmpeg is not on PATH the error names it.

use crate::prepare_run;
use crate::project::Project;
use crate::run::{self, Subprocesses};
use crate::runqueue::{self, Queue};
use crate::subprocess;
use std::path::Path;

/// The program every stage spawns. Same single name the produce tree writes
/// ([`crate::produce_exec::FFMPEG`]): one place holds the tool's name so a configured path
/// replaces it here rather than at every call site.
pub const FFMPEG: &str = "ffmpeg";

/// One unit of subprocess work: what to run, and what the bar says while it runs.
#[derive(Debug, Clone)]
pub struct Task {
    /// Two or three words for the bar — "extracting frames", never a filename (§2).
    pub label: String,
    /// The program to spawn.
    pub program: String,
    /// Its arguments, exactly as a shell would take them.
    pub args: Vec<String>,
    /// Already on disk from an earlier run: still taken, never run (§S2's resume rule).
    pub already_done: bool,
    /// The session folder this row belongs to, for a dial that has to read and write project files.
    /// `None` means the working folder's own session, which is what a shell walk means. Only the dial
    /// rows set it; a spawned task is given its paths in its argv.
    pub session: Option<String>,
}

impl Task {
    /// A task that must run.
    pub fn run_it(label: &str, program: &str, args: Vec<String>) -> Self {
        Task { label: label.to_string(), program: program.to_string(), args, already_done: false, session: None }
    }

    /// A task whose output already exists: counted, taken, and left alone.
    pub fn already_there(label: &str) -> Self {
        Task {
            label: label.to_string(),
            program: String::new(),
            args: Vec::new(),
            already_done: true,
            session: None,
        }
    }
}

/// One job on one track of the bar: the stage's name, which of the run's jobs it is, its tasks.
#[derive(Debug, Clone)]
pub struct Stage {
    pub track: usize,
    pub name: &'static str,
    pub phase: usize,
    pub of: usize,
    /// What `qPush` calls this kind of task ("frame", "clip", "file").
    pub kind: &'static str,
    pub tasks: Vec<Task>,
}

/// How a walked plan ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drive {
    /// Every stage finished. `lines` are what the walk logged, newest last.
    Done { ran: usize, skipped: usize },
    /// §S3: ⏹ was up at a checkpoint, so the walk stopped between tasks. Not a failure.
    Stopped { ran: usize, left: usize },
    /// A task failed. The error names the program, because "a step failed" three stages in says
    /// nothing about which box to look at (§Failure-is-specific).
    Failed { stage: &'static str, error: String },
}

/// Say what a walked step came to, in the log. A failure names the stage and the reason (§Failure-is-
/// specific: "a step failed" three stages in says nothing about which box to look at); a stop says how
/// far it got, because that is what the next ▶ resumes from. A finished step says NOTHING -- F0.2 S4:
/// one step gets no "all done" line. Returns whether anything was logged.
pub fn say_outcome(drive: &Drive, mut log: impl FnMut(&str)) -> bool {
    match drive {
        Drive::Done { .. } => false,
        Drive::Stopped { ran, left } => {
            log(&format!(">>> stopped after {ran} task(s), {left} not run"));
            true
        }
        Drive::Failed { stage, error } => {
            log(&format!("!!! {stage} failed \u{2014} {error}"));
            true
        }
    }
}

/// The work one page step consists of, planned from the live project.
///
/// Frames come from [`crate::frames::scene_pass_plan`] — the scene-detect pass every Prepare run
/// starts with, one task per recording source. Speech comes from [`crate::asr::cut_plan`], one
/// chunk per recording, over the span the project covers. Both builders return real ffmpeg argv, so
/// a plan made from an empty project yields no tasks rather than a made-up one: an empty session
/// has no work, and saying so with an empty queue is honest in a way a canned success is not.
pub fn plan(step: run::Step, project: &Project) -> Vec<Stage> {
    match step {
        // Prepare: frames first (the scene pass), then speech over each recording.
        run::Step::Prepare => {
            let mut stages = Vec::new();
            let frames = crate::frames_run::stage(project);
            if !frames.tasks.is_empty() {
                stages.push(frames);
            }
            let speech = speech_stage(project);
            if !speech.tasks.is_empty() {
                stages.push(speech);
            }
            // The rows that ticked ✂: split off the voice, same dial door, another stage.
            let separation = crate::separate_run::stage(project);
            if !separation.tasks.is_empty() {
                stages.push(separation);
            }
            // §F1.7 describe → §F1.8 fix come after the ingest stages: understanding is what reads
            // the frames and the transcript the stages above just produced.
            let describe = crate::describe_call::stage(project);
            if !describe.tasks.is_empty() {
                stages.push(describe);
            }
            // §F1.8 fix after §F1.7 describe: the respelling reads the transcript ingest wrote, and
            // §04 S6's understand phase runs describe → fix → marking in that order.
            let fix = crate::fix_call::stage(project);
            if !fix.tasks.is_empty() {
                stages.push(fix);
            }
            stages
        }
        // Cut's suggest step reads what Prepare wrote: alignment happens in Prepare's speech pass
        // through [`crate::audio_task`], so the step asks this module for nothing.
        run::Step::Suggest => Vec::new(),
        // Narrate and Produce own their queues and their spawner already (F4.1 / F5.1), so they ask
        // this module for nothing rather than being driven twice over the same work.
        run::Step::Narrate | run::Step::Produce => Vec::new(),
    }
}

/// The marker [`drive_walk`] reads as “dial the vision server for this row” rather than “spawn this”.
/// A second marker rather than a flag on [`AUDIO_TASK`] because the audio rows already carry one of
/// their own (`separate_run::SEPARATE_MARK`) and a third kind would make the branch read arguments
/// for a question the row can answer by its own program name.
pub const VISION_TASK: &str = "naivepost:vision";

/// The marker [`drive_walk`] reads as “dial the text model for this row” rather than “spawn this”: the
/// clean-transcript job of §2, one chat request per block of lines.
pub const TEXT_TASK: &str = "naivepost:text";

/// The marker [`drive_walk`] reads as “dial the servers for this row” rather than “spawn this”.
pub const AUDIO_TASK: &str = "naivepost:audio";

/// Speech recognition: one row per recording, each a DIAL of audio.cpp through [`crate::audio_task`]
/// rather than a spawned process — the chunking, the cut of each chunk and the upload are the door's
/// (§F1.3), so the bar only counts the ask. [`drive_walk`] reads [`AUDIO_TASK`] and calls instead of
/// forking; the row carries the source and the session language, which is all the dial needs.
fn speech_stage(project: &Project) -> Stage {
    let tasks = project
        .sources
        .iter()
        .filter(|source| !source.footage)
        .map(|source| Task {
            label: format!("transcribing {}", crate::audio_task::source_lane(&source.path)),
            program: AUDIO_TASK.to_string(),
            args: vec![source.path.clone(), project.language.clone()],
            already_done: false,
            session: None,
        })
        .collect();
    Stage { track: runqueue::TRACK_STT, name: "speech", phase: 1, of: 2, kind: "chunk", tasks }
}

// Align's clip cut lived here until F1.5: one `align::clip_plan` ffmpeg call per recording, whose
// output nothing read — the aligner is asked over the network in Prepare's speech pass now, and Cut's
// suggest step reads the `words.aligned.json` that pass wrote. Dead argv and its chunk length went with
// it; `align::clip_plan` is still used, by `audio_task::align_plan` reading back the note.

/// Which phase of Prepare's bar a stage belongs to (§S6), by the name [`plan`] gives it.
///
/// Keyed off the stage's own name rather than its position because `plan` already drops stages with no
/// work in them — a session with nothing to separate has no separation stage at all, and an index would
/// then point at the wrong slice. The names matched are exactly the ones `plan(Step::Prepare, …)`
/// builds: `frames`, `speech` (ingest, run in parallel per §S6), `separation`, `describe`, `fix`;
/// anything else (`align`, or a stage added later) answers `None` and keeps the whole bar.
pub fn phase_of_stage(name: &str) -> Option<prepare_run::Phase> {
    match name {
        "separation" => Some(prepare_run::Phase::Separate),
        "frames" | "speech" => Some(prepare_run::Phase::Ingest),
        "describe" | "fix" => Some(prepare_run::Phase::Understand),
        _ => None,
    }
}

/// §S6's numbers for one plan: `(base, share)` per stage, in `plan`'s order so the walk can zip them
/// by position. Each stage gets its own phase's slice — separation 0–10 % when asked, ingest to 30 %,
/// understand 30–100 % — and a stage outside those phases keeps the whole bar.
pub fn phase_slices(stages: &[Stage], separation_asked: bool) -> Vec<(f64, f64)> {
    stages
        .iter()
        .map(|stage| {
            phase_of_stage(stage.name)
                .map(|phase| prepare_run::share(phase, separation_asked))
                .unwrap_or((0.0, 1.0))
        })
        .collect()
}

/// The page step's own walk, from the live window state.
///
/// Reads the run's `paused`/`stop_flag` fresh at every checkpoint through the same `Rc`s the ⏹
/// handler writes, which is what makes a pause park this walk and a stop end it in the gap between
/// two tasks. Narrate and Produce return no stages ([`plan`]) because those steps drive their own
/// queues already; driving them here would count the same work twice onto one bar.
pub fn drive_page(
    step: run::Step,
    project: &Project,
    queue: &QueueHolder,
    bar: &BarHolder,
    procs: &ProcsHolder,
) -> Drive {    let stages = plan(step, project);
    if stages.is_empty() {
        return Drive::Done { ran: 0, skipped: 0 };
    }
    let paused = { let bar = bar.clone(); move || bar.borrow().running.as_ref().is_some_and(|r| r.paused) };
    let stopped = { let bar = bar.clone(); move || bar.borrow().stop_flag };
    // §S6: hand the walk the bar's slices, so a Prepare run moves 0–10 / 10–30 / 30–100 instead of
    // reporting each stage as a whole bar. Empty for every other step, which is what they do today.
    let slices = if step == run::Step::Prepare {
        phase_slices(&stages, prepare_run::separation_asked(project))
    } else {
        Vec::new()
    };
    drive_walk_with_slices(
        &stages,
        &mut queue.borrow_mut(),
        paused,
        stopped,
        crate::ui::window::log_line,
        &mut procs.borrow_mut(),
        &slices,
    )
}

/// The three pieces of window state a page walk needs, as the shared handles the shell holds.
type QueueHolder = std::rc::Rc<std::cell::RefCell<Queue>>;
type BarHolder = std::rc::Rc<std::cell::RefCell<run::RunBar>>;
type ProcsHolder = std::rc::Rc<std::cell::RefCell<Subprocesses>>;

/// Walk a prepared plan under the run's bookkeeping: the S2 sequence per stage, with §S3's
/// checkpoint asked between every task.
///
/// `paused` and `stop_flag` are read fresh at each checkpoint rather than captured once, because both
/// change mid-run: ⏸ parks the walk for one poll at a time and ⏹ ends it in the gap between two
/// tasks, which is the only place either can safely land.
pub fn drive_walk<FN, FS>(
    stages: &[Stage],
    queue: &mut Queue,
    paused: FN,
    stop_flag: FS,
    log: impl FnMut(&str),
    procs: &mut Subprocesses,
) -> Drive
where
    FN: Fn() -> bool,
    FS: Fn() -> bool,
{
    drive_walk_with_slices(stages, queue, paused, stop_flag, log, procs, &[])
}

/// [`drive_walk_with_slices`] under its test name: the slices are the only thing this differs by, and
/// a private helper cannot be reached from an integration test.
pub fn drive_walk_with_slices_for_test<FN, FS>(
    stages: &[Stage],
    queue: &mut Queue,
    paused: FN,
    stop_flag: FS,
    log: impl FnMut(&str),
    procs: &mut Subprocesses,
    slices: &[(f64, f64)],
) -> Drive
where
    FN: Fn() -> bool,
    FS: Fn() -> bool,
{
    drive_walk_with_slices(stages, queue, paused, stop_flag, log, procs, slices)
}

/// [`drive_walk`] with §S6's phase slices handed alongside the stages: one `(base, share)` per stage,
/// applied through `Queue::phase` just before that stage's jobs are queued. An empty slice list — or a
/// short one — leaves the bar unscaled, which is what every non-Prepare step wants.
fn drive_walk_with_slices<FN, FS>(
    stages: &[Stage],
    queue: &mut Queue,
    paused: FN,
    stop_flag: FS,
    mut log: impl FnMut(&str),
    procs: &mut Subprocesses,
    slices: &[(f64, f64)],
) -> Drive
where
    FN: Fn() -> bool,
    FS: Fn() -> bool,
{
    // The slices pair with the stages by position; `phase_slices` keeps `plan`'s order for that reason.
    let walked: Vec<(&Stage, Option<(f64, f64)>)> = if slices.len() == stages.len() {
        stages
            .iter()
            .zip(slices)
            .map(|(stage, &slice)| (stage, Some(slice)))
            .collect()
    } else {
        stages.iter().map(|stage| (stage, None)).collect()
    };
    let mut ran = 0usize;
    let mut skipped = 0usize;
    for (stage, slice) in walked {
        if let Some((base, share)) = slice {
            queue.phase(base, share);
        }
        queue.job(stage.track, stage.name, stage.phase, stage.of);
        queue.push(stage.track, stage.tasks.len(), stage.kind);
        let total = stage.tasks.len();
        // The walk's own position. A stop reports THIS, not an arithmetic guess off the loop index:
        // a run stopped between two tasks leaves exactly the tasks that were never reached.
        let mut progress = Progress::at_start(total);
        for task in stage.tasks.iter() {
            // §S3: ask before every task. A pause waits one poll and asks again; a stop leaves now.
            if !wait_for_go(&paused, &stop_flag) {
                return Drive::Stopped { ran, left: progress.left };
            }
            // §S2: taken whatever happens next — including a task already on disk.
            queue.take(stage.track);
            if task.already_done {
                skipped += 1;
                progress = progress.advanced();
                queue.prog(stage.track, position(progress.done, total), &task.label);
                continue;
            }
            // The audio tasks are dials, not processes: same checkpoint, same `take`, same bar, and the
            // work is a request rather than a fork. The door answers with the lines it logged.
            if task.program == AUDIO_TASK {
                // Same dial door, two different asks: the walk does not decide which, the row says.
                let worked = if task.args.first().map(String::as_str) == Some(crate::separate_run::SEPARATE_MARK) {
                    crate::separate_run::run_task(task, &mut log)
                } else {
                    dial_task(task, &mut log)
                };
                if let Err(error) = worked {
                    return Drive::Failed { stage: stage.name, error };
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, position(progress.done, total), &task.label);
                continue;
            }
            // The frames rows are this module's own dial too (S2's sweep plus one ffmpeg per frame is far
            // more than one spawn can carry), so the walk calls instead of forking; `frames_run::stage`
            // marks them with the program word it reads.
            if task.program == crate::frames_run::FRAMES_TASK {
                if let Err(error) = crate::frames_run::run_task(task, &mut log) {
                    return Drive::Failed { stage: stage.name, error };
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, position(progress.done, total), &task.label);
                continue;
            }
            // The vision rows are a chat request rather than an audio task: same checkpoint, same
            // `take`, different door. `drive_walk` does not decide which, the row says by its program.
            if task.program == VISION_TASK {
                if let Err(error) = crate::describe_call::run_task(task, &mut log) {
                    return Drive::Failed { stage: stage.name, error };
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, position(progress.done, total), &task.label);
                continue;
            }
            // The fix rows are a chat request too, same checkpoint and same door shape: the row says
            // which job by its program, `drive_walk` does not decide.
            if task.program == TEXT_TASK {
                if let Err(error) = crate::fix_call::run_task(task, &mut log) {
                    return Drive::Failed { stage: stage.name, error };
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, position(progress.done, total), &task.label);
                continue;
            }
            // Fix's two session rows (S1's placement, S5-S7's merge) are dials of the same stage: the
            // row says which by its program, `drive_walk` does not decide.
            if task.program == crate::fix_session::PLACEMENT_TASK
                || task.program == crate::fix_session::SESSION_TASK
            {
                let worked = if task.program == crate::fix_session::PLACEMENT_TASK {
                    crate::fix_session::run_placement(task, &mut log)
                } else {
                    crate::fix_session::run_session(task, &mut log)
                };
                if let Err(error) = worked {
                    return Drive::Failed { stage: stage.name, error };
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, position(progress.done, total), &task.label);
                continue;
            }
            if let Err(why) = spawn_task(task, procs, &mut log) {
                return Drive::Failed { stage: stage.name, error: why };
            }
            ran += 1;
            progress = progress.advanced();
            queue.prog(stage.track, position(progress.done, total), &task.label);
        }
        queue.done(stage.track, 1.0);
    }
    Drive::Done { ran, skipped }
}

/// How far a walk has got: tasks finished, and how many are still ahead of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub left: usize,
}

impl Progress {
    /// Nothing done yet, `total` still ahead.
    pub fn at_start(total: usize) -> Self {
        Progress { done: 0, left: total }
    }

    /// One task finished: it moves from `left` into `done`.
    pub fn advanced(self) -> Self {
        Progress { done: self.done + 1, left: self.left.saturating_sub(1) }
    }
}

/// Ask the checkpoint until it lets the task go; false when the stop won.///
/// Pause polls at [`runqueue::CHECKPOINT_POLL`] (§S3's 200 ms) rather than blocking, because the
/// GUI thread has to stay alive to see ⏹ at all, and rather than spinning, because a parked run
/// should cost nothing.
fn wait_for_go(paused: &impl Fn() -> bool, stop_flag: &impl Fn() -> bool) -> bool {
    loop {
        match runqueue::checkpoint(paused(), stop_flag()) {
            runqueue::Checkpoint::Ran => return true,
            runqueue::Checkpoint::Stopped => return false,
            runqueue::Checkpoint::Waited => std::thread::sleep(runqueue::CHECKPOINT_POLL),
        }
    }
}

/// The fraction after `done` of `total` tasks, guarded against an empty stage.
fn position(done: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        done as f64 / total as f64
    }
}

/// Run one dial task: the recording's speech, alignment and diarization over audio.cpp, at the
/// addresses and model ids the settings file says right now (§1's re-read-per-request). The duration is
/// the one the machine can be asked for — ffprobe on the decoded pcm, whose absence is the honest
/// reason a row gets nothing rather than a silent zero.
fn dial_task(task: &Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let paths = crate::settings::from_environment().ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let tree = crate::layout::Tree::new(crate::startup::session_dir(&std::env::current_dir().unwrap_or_default())).ok();
    let source = task.args.first().cloned().unwrap_or_default();
    let lane = crate::audio_task::source_lane(&source);
    let conf = crate::settings::read(&paths).unwrap_or_default();
    let language = task.args.get(1).cloned().unwrap_or_default();
    let _ = &conf;
    // §F1.3 S1 first: the row's recording becomes `voice16k.wav` if it is not there yet, because every
    // step of the pass (and the duration probed below) counts samples of THAT file.
    let Some(tree) = tree else {
        return Err("no project folder to write the transcript into".to_string());
    };
    let source_path = crate::layout::resolve(tree.dir(), tree.dir(), &source);
    // A row whose recording is not on disk stops here, before anything is asked of ffmpeg: §Failure-is-
    // specific names the file that is missing rather than blaming the tool that never got to run.
    if !source_path.exists() {
        return Err(format!("{lane}: no decoded audio to transcribe -- {} is not there", source_path.display()));
    }
    let pcm = crate::decode_audio::ensure(&tree, &lane, &source_path, &conf.ffmpeg, log)?;
    let duration = probe_duration(&conf.ffmpeg, &pcm, log)?;
    let passage = crate::audio_task::transcribe_source(&tree, &paths, &lane, duration, &language, log)?;
    for line in &passage.logs {
        log(line);
    }
    Ok(())
}

/// Seconds of an audio file, by ffprobe — the tool §1 names for reading, taken from the same folder as
/// the configured ffmpeg ([`crate::services::ffprobe_for`]'s rule), and its command goes to the log
/// before it runs like every other subprocess line.
///
/// Public because the frames door asks the same question of a video (§F1.6 S3 needs the take's length to
/// know where its grid stops), and one ffprobe rule belongs in one place.
pub fn probe_duration(configured_ffmpeg: &str, path: &Path, log: &mut impl FnMut(&str)) -> Result<f64, String> {
    let ffprobe = crate::services::ffprobe_for(configured_ffmpeg, "ffprobe");
    let shown = ["-show_entries", "format=duration", "-of", "default=nw=1:nk=1", &path.display().to_string()];
    let out = subprocess::run(&ffprobe, &shown, |line| log(line))?
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{}: ffprobe answered no duration", path.display()))?;
    Ok(out)
}

/// Spawn one task, registering its pid so ⏹ can reach it, and keep `subprocess`'s logging rule:
/// the command goes to the log before it runs, and a failure carries the command plus its tail.
///
/// `subprocess::run` cannot serve here because `.output()` reaps the child before any caller could
/// register it, leaving F0.3 nothing to kill. This spawns the same program with piped stdio, records
/// the pid the moment it exists, waits, and formats its error through the same
/// [`subprocess::failure`] the rest of the tree uses.
pub fn spawn_task(task: &Task, procs: &mut Subprocesses, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let owned: Vec<&str> = task.args.iter().map(String::as_str).collect();
    log(&subprocess::log_line(&task.program, &owned));
    let child = subprocess::spawn_piped(&task.program, &owned)
        .map_err(|err| format!("{} failed: {err}", subprocess::shown(&task.program, &owned)))?;
    procs.register(child.id());
    let out = child
        .wait_with_output()
        .map_err(|err| subprocess::failure(&task.program, &owned, &err.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Err(subprocess::failure(&task.program, &owned, &combined))
}
