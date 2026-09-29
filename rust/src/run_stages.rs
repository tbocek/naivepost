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

use crate::project::Project;
use crate::run::{self, Subprocesses};
use crate::runqueue::{self, Queue};
use crate::subprocess;

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
}

impl Task {
    /// A task that must run.
    pub fn run_it(label: &str, program: &str, args: Vec<String>) -> Self {
        Task { label: label.to_string(), program: program.to_string(), args, already_done: false }
    }

    /// A task whose output already exists: counted, taken, and left alone.
    pub fn already_there(label: &str) -> Self {
        Task {
            label: label.to_string(),
            program: String::new(),
            args: Vec::new(),
            already_done: true,
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
            let frames = frames_stage(project);
            if !frames.tasks.is_empty() {
                stages.push(frames);
            }
            let speech = speech_stage(project);
            if !speech.tasks.is_empty() {
                stages.push(speech);
            }
            stages
        }
        // Cut's suggest step reads what Prepare wrote; its own subprocess work is the align clip cut.
        run::Step::Suggest => align_stage(project).into_iter().collect(),
        // Narrate and Produce own their queues and their spawner already (F4.1 / F5.1), so they ask
        // this module for nothing rather than being driven twice over the same work.
        run::Step::Narrate | run::Step::Produce => Vec::new(),
    }
}

/// The scene-detect pass: one task per recording source, argv straight from `frames::scene_pass_plan`.
fn frames_stage(project: &Project) -> Stage {
    let args = crate::frames::scene_pass_plan(0.0);
    let tasks = project
        .sources
        .iter()
        .filter(|source| source.footage)
        .map(|source| {
            let mut argv = vec!["-i".to_string(), source.path.clone()];
            argv.extend(args.iter().cloned());
            Task::run_it("scening frames", FFMPEG, argv)
        })
        .collect();
    Stage { track: runqueue::TRACK_FRAMES, name: "frames", phase: 1, of: 2, kind: "frame", tasks }
}

/// Speech recognition: one chunk per recording, cut with `asr::cut_plan`.
fn speech_stage(project: &Project) -> Stage {
    let tasks = project
        .sources
        .iter()
        .filter(|source| !source.footage)
        .map(|source| {
            let mut argv = crate::asr::cut_plan(0.0, CHUNK_SECONDS);
            argv.push("-i".to_string());
            argv.push(source.path.clone());
            Task::run_it("recognising", FFMPEG, argv)
        })
        .collect();
    Stage { track: runqueue::TRACK_STT, name: "speech", phase: 1, of: 2, kind: "chunk", tasks }
}

/// Align's clip cut, one per recording pair, from `align::clip_plan`.
fn align_stage(project: &Project) -> Option<Stage> {
    let tasks: Vec<Task> = project
        .sources
        .iter()
        .filter(|source| !source.footage)
        .map(|source| {
            let mut argv = crate::align::clip_plan(0.0, CHUNK_SECONDS);
            argv.push("-i".to_string());
            argv.push(source.path.clone());
            Task::run_it("aligning", FFMPEG, argv)
        })
        .collect();
    if tasks.is_empty() {
        return None;
    }
    Some(Stage {
        track: runqueue::TRACK_STT,
        name: "align",
        phase: 1,
        of: 1,
        kind: "clip",
        tasks,
    })
}

/// How much of a recording one speech/align chunk covers. Chosen so a chunk stays inside the ASR
/// window §02 gives the shipped models (≤ 300 s) without needing the halving ladder.
const CHUNK_SECONDS: f64 = 300.0;

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
) -> Drive {
    let stages = plan(step, project);
    if stages.is_empty() {
        return Drive::Done { ran: 0, skipped: 0 };
    }
    let paused = { let bar = bar.clone(); move || bar.borrow().running.as_ref().is_some_and(|r| r.paused) };
    let stopped = { let bar = bar.clone(); move || bar.borrow().stop_flag };
    drive_walk(&stages, &mut queue.borrow_mut(), paused, stopped, crate::ui::window::log_line, &mut procs.borrow_mut())
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
    mut log: impl FnMut(&str),
    procs: &mut Subprocesses,
) -> Drive
where
    FN: Fn() -> bool,
    FS: Fn() -> bool,
{
    let mut ran = 0usize;
    let mut skipped = 0usize;
    for stage in stages {
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
