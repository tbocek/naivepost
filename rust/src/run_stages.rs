//! F0.5 S2/S3 — the walk that puts a step's work on the bar and runs it under the bookkeeping.

use crate::prepare_run;
use crate::project::Project;
use crate::run::{self, Subprocesses};
use crate::runqueue::{self, Queue};
use crate::subprocess;
use std::path::Path;

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
    pub session: Option<String>,
}

impl Task {
    /// A task that must run.
    pub fn run_it(label: &str, program: &str, args: Vec<String>) -> Self {
        Task {
            label: label.to_string(),
            program: program.to_string(),
            args,
            already_done: false,
            session: None,
        }
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
pub fn plan(step: run::Step, project: &Project) -> Vec<Stage> {
    match step {
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
            let separation = crate::separate_run::stage(project);
            if !separation.tasks.is_empty() {
                stages.push(separation);
            }
            // §F1.6 S6, the text gate: every recording judged before any is described, so a decision model
            // and a describe model sharing one server are each loaded once. Always planned: the main model judges when no
            // decision model is set.
            let gate = crate::text_gate::stage(project);
            if !gate.tasks.is_empty() {
                stages.push(gate);
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
        run::Step::Suggest => Vec::new(),
        run::Step::Narrate | run::Step::Produce => Vec::new(),
    }
}

/// The marker [`drive_walk`] reads as “dial the vision server for this row” rather than “spawn this”.
pub const VISION_TASK: &str = "naivepost:vision";

/// The marker [`drive_walk`] reads as “dial the text model for this row” rather than “spawn this”: the
/// clean-transcript job of §2, one chat request per block of lines.
pub const TEXT_TASK: &str = "naivepost:text";

/// The marker [`drive_walk`] reads as “dial the servers for this row” rather than “spawn this”.
pub const AUDIO_TASK: &str = "naivepost:audio";

fn speech_stage(project: &Project) -> Stage {
    let tasks = project
        .sources
        .iter()
        .map(|source| Task {
            label: format!(
                "transcribing {}",
                crate::audio_task::source_lane(&source.path)
            ),
            program: AUDIO_TASK.to_string(),
            args: vec![source.path.clone(), project.policy.language.value.clone()],
            already_done: false,
            session: None,
        })
        .collect();
    Stage {
        track: runqueue::TRACK_STT,
        name: "speech",
        phase: 1,
        of: 2,
        kind: "chunk",
        tasks,
    }
}

/// Which phase of Prepare's bar a stage belongs to (§S6), by the name [`plan`] gives it.
pub fn phase_of_stage(name: &str) -> Option<prepare_run::Phase> {
    match name {
        "separation" => Some(prepare_run::Phase::Separate),
        "frames" | "speech" => Some(prepare_run::Phase::Ingest),
        "textgate" | "describe" | "fix" => Some(prepare_run::Phase::Understand),
        _ => None,
    }
}

pub fn phase_slices(stages: &[Stage], separation_asked: bool) -> Vec<(f64, f64)> {
    let phases: Vec<Option<prepare_run::Phase>> = stages
        .iter()
        .map(|stage| phase_of_stage(stage.name))
        .collect();
    phases
        .iter()
        .enumerate()
        .map(|(index, phase)| {
            let Some(phase) = phase else {
                return (0.0, 1.0);
            };
            let (start, end) = prepare_run::share(*phase, separation_asked);
            let width = end - start;
            let same = phases
                .iter()
                .filter(|other| *other == &Some(*phase))
                .count()
                .max(1);
            let before = phases[..index]
                .iter()
                .filter(|other| *other == &Some(*phase))
                .count();
            let each = width / same as f64;
            (start + each * before as f64, each)
        })
        .collect()
}

static PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STOPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Copy the bar's pause and stop where the walk reads them. Called by every press that changes either.
pub fn mirror_switches(bar: &run::RunBar) {
    use std::sync::atomic::Ordering;
    PAUSED.store(
        bar.running.as_ref().is_some_and(|running| running.paused),
        Ordering::SeqCst,
    );
    STOPPED.store(bar.stop_flag, Ordering::SeqCst);
}

/// How often a parked walk looks at ⏸ and ⏹ again (§03-shell §2's "poll 200 ms").
const PARK_POLL: std::time::Duration = std::time::Duration::from_millis(200);

static PARKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Hold the walk for as long as it is paused. False when it was stopped instead, parked or not.
pub fn park_while(paused: impl Fn() -> bool, stopped: impl Fn() -> bool) -> bool {
    use std::sync::atomic::Ordering;
    while paused() && !stopped() {
        PARKED.store(true, Ordering::SeqCst);
        std::thread::sleep(PARK_POLL);
    }
    PARKED.store(false, Ordering::SeqCst);
    !stopped()
}

/// [`park_while`] on the run's own ⏸ and ⏹: a walk's checkpoint for a step that drives itself.
pub fn park_at_checkpoint() -> bool {
    use std::sync::atomic::Ordering;
    park_while(
        || PAUSED.load(Ordering::SeqCst),
        || STOPPED.load(Ordering::SeqCst) || crate::cancel_leg::cancelled(),
    )
}

/// Whether the walk is standing at a checkpoint on ⏸, rather than still finishing what it was doing.
pub fn parked() -> bool {
    PARKED.load(std::sync::atomic::Ordering::SeqCst)
}

/// What a step that drives itself is doing, for the window to paint: the sentence and, once there is one to give, how
/// far along the bar it is.
static PROGRESS: std::sync::Mutex<Option<(String, Option<f64>)>> = std::sync::Mutex::new(None);

pub fn report_progress(status: &str, fraction: Option<f64>) {
    if let Ok(mut held) = PROGRESS.lock() {
        *held = Some((status.to_string(), fraction));
    }
}

/// The clip an encode is on: its sentence, where its share of the bar starts, how big the share is, and how many seconds
/// of picture it writes. The stats ffmpeg prints as it goes fill the share ([`encoded_to`]).
static ENCODING: std::sync::Mutex<Option<(String, f64, f64, f64)>> = std::sync::Mutex::new(None);

/// A clip encode starts: report where it starts, and let [`encoded_to`] move the bar through its share.
pub fn encoding(status: &str, from: f64, share: f64, seconds: f64) {
    if let Ok(mut held) = ENCODING.lock() {
        *held = Some((status.to_string(), from, share, seconds));
    }
    report_progress(status, Some(from));
}

/// The encodes are over: a later ffmpeg's stats (the join, the mux) move nothing.
pub fn encoding_done() {
    if let Ok(mut held) = ENCODING.lock() {
        *held = None;
    }
}

/// The running clip encode has written `t` seconds: the bar goes that far through the clip's share.
pub fn encoded_to(t: f64) {
    let held = ENCODING.lock().ok().and_then(|held| held.clone());
    if let Some((status, from, share, seconds)) = held {
        let done = if seconds > 0.0 { (t / seconds).clamp(0.0, 1.0) } else { 0.0 };
        report_progress(&status, Some(from + share * done));
    }
}

/// The newest report, taken: the next look finds nothing until the walk says something new.
pub fn take_progress() -> Option<(String, Option<f64>)> {
    PROGRESS.lock().ok().and_then(|mut held| held.take())
}

static RUNNING_CHILDREN: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

/// List a child a run on a worker has started, so ⏹ reaches it. Every listed child is unlisted once reaped.
pub fn list_child(pid: u32) {
    if let Ok(mut running) = RUNNING_CHILDREN.lock() {
        running.push(pid);
    }
}

/// Take a reaped child off the list ⏹ signals: the pid may be somebody else's from here on.
pub fn unlist_child(pid: u32) {
    if let Ok(mut running) = RUNNING_CHILDREN.lock() {
        running.retain(|listed| *listed != pid);
    }
}

/// Every child a worker's walk has running now, taken off the list: what ⏹ signals.
pub fn take_running_children() -> Vec<u32> {
    RUNNING_CHILDREN
        .lock()
        .map(|mut held| std::mem::take(&mut *held))
        .unwrap_or_default()
}

pub fn drive_worker(
    step: run::Step,
    project: &Project,
    queue: Queue,
    on_queue: &mut dyn FnMut(&Queue, &str),
) -> Drive {
    use std::sync::atomic::Ordering;
    let stages = plan(step, project);
    if stages.is_empty() {
        return Drive::Done { ran: 0, skipped: 0 };
    }
    // §S6: hand the walk the bar's slices, so a Prepare run moves 0–10 / 10–30 / 30–100 instead of
    // reporting each stage as a whole bar. Empty for every other step, which is what they do today.
    let slices = if step == run::Step::Prepare {
        phase_slices(&stages, prepare_run::separation_asked(project))
    } else {
        Vec::new()
    };
    let mut queue = queue;
    let mut procs = Subprocesses::default();
    walk(
        &stages,
        &mut queue,
        || PAUSED.load(Ordering::SeqCst),
        || STOPPED.load(Ordering::SeqCst),
        crate::ui::window::log_line,
        &mut procs,
        &slices,
        on_queue,
    )
}

/// Walk a prepared plan under the run's bookkeeping: the S2 sequence per stage, with §S3's
/// checkpoint asked between every task.
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
    walk(
        stages,
        queue,
        paused,
        stop_flag,
        log,
        procs,
        &[],
        &mut |_, _| {},
    )
}

/// [`walk`] with its `on_queue` hook, under its test name: what the window would be shown after every change of
/// the bar, which is how a test sees a stage move the bar inside one task.
pub fn drive_walk_watched_for_test(
    stages: &[Stage],
    queue: &mut Queue,
    log: impl FnMut(&str),
    slices: &[(f64, f64)],
    on_queue: &mut dyn FnMut(&Queue, &str),
) -> Drive {
    walk(
        stages,
        queue,
        || false,
        || false,
        log,
        &mut Subprocesses::default(),
        slices,
        on_queue,
    )
}

/// [`walk`] with slices, under its test name: the slices are the only thing this differs by, and
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
    walk(
        stages,
        queue,
        paused,
        stop_flag,
        log,
        procs,
        slices,
        &mut |_, _| {},
    )
}

#[allow(clippy::too_many_arguments)] // the walk's whole state: stages, bar, switches, log, children, slices, hook
fn walk<FN, FS>(
    stages: &[Stage],
    queue: &mut Queue,
    paused: FN,
    stop_flag: FS,
    mut log: impl FnMut(&str),
    procs: &mut Subprocesses,
    slices: &[(f64, f64)],
    on_queue: &mut dyn FnMut(&Queue, &str),
) -> Drive
where
    FN: Fn() -> bool,
    FS: Fn() -> bool,
{
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
        on_queue(queue, stage.name);
        let total = stage.tasks.len();
        // The walk's own position. A stop reports THIS, not an arithmetic guess off the loop index:
        // a run stopped between two tasks leaves exactly the tasks that were never reached.
        let mut progress = Progress::at_start(total);
        // A describe stage counts its requests over the whole stage, not per recording: what there is and how
        // much is done, both updated as they change. The total is each row's planned chunks, summed before the
        // first row runs, and a row whose own plan comes out different (or that sends nothing) corrects it.
        let mut planned: Vec<usize> = stage
            .tasks
            .iter()
            .map(|task| {
                if task.program == VISION_TASK {
                    crate::describe_call::planned_requests(task)
                } else {
                    0
                }
            })
            .collect();
        let mut requests_total: usize = planned.iter().sum();
        let mut requests_done = 0usize;
        let mut side_by_side_until = 0usize;
        let mut slots_asked = false;
        for (index, task) in stage.tasks.iter().enumerate() {
            if index < side_by_side_until {
                continue;
            }
            if !slots_asked && crate::rows_at_once::starts_run(stage, index) {
                slots_asked = true;
                if let Some((at_once, range)) = crate::rows_at_once::at_once_for(stage, index) {
                    side_by_side_until = range.end;
                    let rows = range.len();
                    match crate::rows_at_once::walk(
                        stage,
                        range,
                        at_once,
                        progress.done,
                        queue,
                        &paused,
                        &stop_flag,
                        &mut log,
                        on_queue,
                    ) {
                        crate::rows_at_once::Walked::Done {
                            ran: walked,
                            skipped: passed,
                        } => {
                            ran += walked;
                            skipped += passed;
                            for _ in 0..rows {
                                progress = progress.advanced();
                            }
                        }
                        crate::rows_at_once::Walked::Stopped { ran: walked, left } => {
                            return Drive::Stopped {
                                ran: ran + walked,
                                left: left + total - side_by_side_until,
                            };
                        }
                        crate::rows_at_once::Walked::Failed(error) => {
                            return Drive::Failed {
                                stage: stage.name,
                                error,
                            };
                        }
                    }
                    continue;
                }
            }
            let label = if total > 1 {
                format!("{} of {total}, {}", index + 1, task.label)
            } else {
                task.label.clone()
            };
            // What the task before this one finished is shown before this one starts.
            on_queue(queue, stage.name);
            // §S3: ask before every task. A pause waits one poll and asks again; a stop leaves now.
            if !wait_for_go(&paused, &stop_flag) {
                return Drive::Stopped {
                    ran,
                    left: progress.left,
                };
            }
            // §S2: taken whatever happens next — including a task already on disk.
            queue.take(stage.track);
            // The status names the task that is running, from its first moment: before, the line kept the label
            // of the task that had just ENDED (Prepare said "building the word list" for the minutes the joins
            // repair after it took).
            queue.prog(stage.track, stage_at(stage, progress.done), &label);
            on_queue(queue, stage.name);
            if task.already_done {
                skipped += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            if task.program == AUDIO_TASK {
                let worked = if task.args.first().map(String::as_str)
                    == Some(crate::separate_run::SEPARATE_MARK)
                {
                    crate::separate_run::run_task(task, &mut log)
                } else {
                    dial_task(task, &mut log)
                };
                if let Err(error) = worked {
                    return ended_by(&stop_flag, stage.name, error, ran, progress.left);
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            if task.program == crate::frames_run::FRAMES_TASK {
                if let Err(error) = crate::frames_run::run_task(task, &mut log) {
                    return ended_by(&stop_flag, stage.name, error, ran, progress.left);
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            // The text gate's rows (F1.6 S6) are chat requests too, a stage of their own before describe.
            if task.program == crate::text_gate::TEXT_GATE_TASK {
                let done = progress.done;
                let mut reached = |at: crate::text_gate::Reached| {
                    report_within(
                        queue,
                        on_queue,
                        stage,
                        &label,
                        done,
                        at.fraction(),
                        &at.words(),
                    );
                };
                if let Err(error) = crate::text_gate::run_task(task, &mut log, &mut reached) {
                    return ended_by(&stop_flag, stage.name, error, ran, progress.left);
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            if task.program == VISION_TASK {
                let done = progress.done;
                let before = requests_done;
                let mut answered_here = None;
                let mut reached = |answered: usize, row_total: usize| {
                    if row_total != planned[index] {
                        requests_total = requests_total + row_total - planned[index];
                        planned[index] = row_total;
                    }
                    answered_here = Some(answered);
                    let within = if row_total > 0 {
                        answered as f64 / row_total as f64
                    } else {
                        1.0
                    };
                    let words = format!("{} of {requests_total} requests", before + answered);
                    report_within(queue, on_queue, stage, &label, done, within, &words);
                };
                let described = crate::describe_call::run_task(task, &mut log, &mut reached);
                match answered_here {
                    Some(answered) => requests_done = before + answered,
                    None => {
                        requests_total -= planned[index];
                        planned[index] = 0;
                    }
                }
                if let Err(error) = described {
                    return ended_by(&stop_flag, stage.name, error, ran, progress.left);
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            // §F1.13's word list is a dial of the same fix stage: it reads the files the rows above just
            // wrote and contacts no model, so the row only costs the bar one task.
            if task.program == crate::word_list::WORDS_TASK {
                if let Err(error) = crate::word_list::run_task(task, &mut log) {
                    return ended_by(&stop_flag, stage.name, error, ran, progress.left);
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            if task.program == TEXT_TASK {
                if let Err(error) = crate::fix_call::run_task(task, &mut log) {
                    return ended_by(&stop_flag, stage.name, error, ran, progress.left);
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            if task.program == crate::fix_session::PLACEMENT_TASK
                || task.program == crate::fix_session::SESSION_TASK
            {
                let done = progress.done;
                let mut reached = |within: f64, words: &str| {
                    report_within(queue, on_queue, stage, &label, done, within, words);
                };
                let worked = if task.program == crate::fix_session::PLACEMENT_TASK {
                    crate::fix_session::run_placement(task, &mut log)
                } else {
                    crate::fix_session::run_session(task, &mut log, &mut reached)
                };
                if let Err(error) = worked {
                    return ended_by(&stop_flag, stage.name, error, ran, progress.left);
                }
                ran += 1;
                progress = progress.advanced();
                queue.prog(stage.track, stage_at(stage, progress.done), &label);
                continue;
            }
            if let Err(why) = spawn_task(task, procs, &mut log) {
                return ended_by(&stop_flag, stage.name, why, ran, progress.left);
            }
            ran += 1;
            progress = progress.advanced();
            queue.prog(stage.track, stage_at(stage, progress.done), &label);
        }
        queue.done(stage.track, 1.0);
        on_queue(queue, stage.name);
    }
    Drive::Done { ran, skipped }
}

/// A row that came back with an error. Under ⏹ the error is the stop's own doing (the dial was cancelled), so the run was stopped, not failed.
fn ended_by(
    stopped: &dyn Fn() -> bool,
    stage: &'static str,
    error: String,
    ran: usize,
    left: usize,
) -> Drive {
    if stopped() {
        Drive::Stopped { ran, left }
    } else {
        Drive::Failed { stage, error }
    }
}

pub const MERGE_ROW_SHARE: f64 = 0.75;

/// Where the bar stands in `stage` with `done` of its tasks finished, 0..1. Tasks share the stage evenly, except
/// that a last task running the marking pass has [`MERGE_ROW_SHARE`] of it.
pub fn stage_at(stage: &Stage, done: usize) -> f64 {
    let total = stage.tasks.len();
    let merges_last = stage
        .tasks
        .last()
        .is_some_and(|task| task.program == crate::fix_session::SESSION_TASK);
    if !merges_last || total < 2 {
        return position(done, total);
    }
    let before = total - 1;
    if done <= before {
        (1.0 - MERGE_ROW_SHARE) * done as f64 / before as f64
    } else {
        1.0
    }
}

pub(crate) fn report_within(
    queue: &mut Queue,
    on_queue: &mut dyn FnMut(&Queue, &str),
    stage: &Stage,
    label: &str,
    done: usize,
    within: f64,
    words: &str,
) {
    let at = stage_at(stage, done)
        + within.clamp(0.0, 1.0) * (stage_at(stage, done + 1) - stage_at(stage, done));
    queue.prog(stage.track, at, &format!("{label} -- {words}"));
    on_queue(queue, stage.name);
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
        Progress {
            done: 0,
            left: total,
        }
    }

    /// One task finished: it moves from `left` into `done`.
    pub fn advanced(self) -> Self {
        Progress {
            done: self.done + 1,
            left: self.left.saturating_sub(1),
        }
    }
}

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

fn dial_task(task: &Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let paths = crate::settings::from_environment()
        .ok_or("nowhere to keep the settings -- neither XDG_CONFIG_HOME nor HOME is set")?;
    let tree = crate::layout::Tree::new(crate::startup::open_dir()).ok();
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
        return Err(format!(
            "{lane}: no decoded audio to transcribe -- {} is not there",
            source_path.display()
        ));
    }
    let pcm = crate::decode_audio::ensure(&tree, &lane, &source_path, &conf.ffmpeg, log)?;
    crate::wave_build::ensure(&tree, &lane, &source_path, log)?;
    let duration = probe_duration(&conf.ffmpeg, &pcm, log)?;
    let passage =
        crate::audio_task::transcribe_source(&tree, &paths, &lane, duration, &language, log)?;
    for line in &passage.logs {
        log(line);
    }
    Ok(())
}

pub fn probe_duration(
    configured_ffmpeg: &str,
    path: &Path,
    log: &mut impl FnMut(&str),
) -> Result<f64, String> {
    let ffprobe = crate::services::ffprobe_for(configured_ffmpeg, "ffprobe");
    let shown = [
        "-show_entries",
        "format=duration",
        "-of",
        "default=nw=1:nk=1",
        &path.display().to_string(),
    ];
    let out = subprocess::run(&ffprobe, &shown, |line| log(line))?
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{}: ffprobe answered no duration", path.display()))?;
    Ok(out)
}

/// Spawn one task, registering its pid so ⏹ can reach it, and keep `subprocess`'s logging rule:
/// the command goes to the log before it runs, and a failure carries the command plus its tail.
pub fn spawn_task(
    task: &Task,
    procs: &mut Subprocesses,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let owned: Vec<&str> = task.args.iter().map(String::as_str).collect();
    log(&subprocess::log_line(&task.program, &owned));
    let child = subprocess::spawn_piped(&task.program, &owned)
        .map_err(|err| format!("{} failed: {err}", subprocess::shown(&task.program, &owned)))?;
    let pid = child.id();
    procs.register(pid);
    list_child(pid);
    let out = child.wait_with_output();
    unlist_child(pid);
    let out = out.map_err(|err| subprocess::failure(&task.program, &owned, &err.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Err(subprocess::failure(&task.program, &owned, &combined))
}
