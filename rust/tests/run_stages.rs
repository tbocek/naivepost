//! F0.5 S2/S3 walked, not just declared (spec/03-shell.md F0.5).
//!
//! `run_bookkeeping.rs` proves the vocabulary one call at a time. What it cannot show is the walk:
//! that a step's tasks actually go through `qJob → qPush → (checkpoint → qTake → run → prog)* →
//! qDone`, that the checkpoint is asked *between* subprocesses so ⏹ lands in the gap, and that a
//! task whose output already exists is still taken. Those are `naivepost::run_stages`' rules, and
//! they need real processes to be worth anything: every task here runs `/bin/sh -c`, which exists in
//! every image this suite runs in, so the spawn path is the program's own and only the tool is
//! swapped at the seam.

use std::time::{Duration, Instant};

use naivepost::project::Project;
use naivepost::run::{self, RunBar, Subprocesses};
use naivepost::runqueue::{Queue, CHECKPOINT_POLL, TRACK_FRAMES, TRACK_STT};
use naivepost::run_stages::{self, Stage, Task};

/// The counting happens by side effect on the stage's argv: each task writes its own marker file, so
/// "was this task run?" is answered by the filesystem rather than by trusting the driver's word.
fn marker_task(dir: &std::path::Path, name: &str) -> Task {
    let stamp = dir.join(format!("{name}.ran"));
    Task {
        label: format!("task {name}"),
        program: "/bin/sh".to_string(),
        args: vec!["-c".to_string(), format!("touch {}", stamp.display())],
        already_done: false,
    }
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f05-walk-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

/// S2: the whole sequence, in order, over two real tasks — job named, tasks pushed, each taken,
/// progress reported per task, the track closed with `done`.
#[test]
fn f0_5_s2_drive_runs_job_push_take_prog_done_in_that_order() {
    let dir = scratch("order");
    let stages = vec![Stage {
        track: TRACK_STT,
        name: "speech",
        phase: 1,
        of: 2,
        kind: "chunk",
        tasks: vec![marker_task(&dir, "one"), marker_task(&dir, "two")],
    }];
    let mut queue = Queue::new();
    let mut procs = Subprocesses::default();
    let drive = run_stages::drive_walk(
        &stages,
        &mut queue,
        || false,
        || false,
        |_| {},
        &mut procs,
    );
    assert_eq!(drive, run_stages::Drive::Done { ran: 2, skipped: 0 });
    // Both tasks really ran: their marker files exist.
    assert!(dir.join("one.ran").exists(), "the first task spawned");
    assert!(dir.join("two.ran").exists(), "the second task spawned");
    let track = queue.track(TRACK_STT);
    assert_eq!(track.job, "speech", "qJob named the stage");
    assert_eq!(track.queued, 2, "qPush put both tasks on the track");
    assert_eq!(track.taken, 2, "every task was taken, none skipped");
    assert!(track.done_job, "qDone closed the track");
    assert_eq!(queue.fraction(), 1.0, "a finished stage fills its half of the bar");
}

/// S2's parenthetical: a task skipped because its output exists is TAKEN anyway, or the bar stalls
/// at the resume point forever.
#[test]
fn f0_5_s2_a_task_skipped_for_existing_output_is_taken_too() {
    let dir = scratch("skipped");
    let mut first = marker_task(&dir, "fresh");
    let second = Task::already_there("already on disk");
    let stages = vec![Stage {
        track: TRACK_FRAMES,
        name: "frames",
        phase: 1,
        of: 1,
        kind: "frame",
        tasks: vec![first.clone(), second],
    }];
    let mut queue = Queue::new();
    let mut procs = Subprocesses::default();
    let drive = run_stages::drive_walk(
        &stages,
        &mut queue,
        || false,
        || false,
        |_| {},
        &mut procs,
    );
    assert_eq!(drive, run_stages::Drive::Done { ran: 1, skipped: 1 });
    let track = queue.track(TRACK_FRAMES);
    assert_eq!(track.queued, 2, "both were pushed");
    assert_eq!(track.taken, 2, "the already-done task was taken as well — that is the stall fix");
    let _ = &mut first;
}

/// S3: the checkpoint is asked before EVERY task, so a stop set after the first one leaves the rest
/// unstarted rather than running into them.
#[test]
fn f0_5_s3_checkpoint_is_asked_between_every_task() {
    let dir = scratch("stop");
    let tasks = vec![
        marker_task(&dir, "a"),
        marker_task(&dir, "b"),
        marker_task(&dir, "c"),
    ];
    let stages = vec![Stage {
        track: TRACK_STT,
        name: "speech",
        phase: 1,
        of: 1,
        kind: "chunk",
        tasks,
    }];
    // Stop becomes true once the first task has left its marker: the next checkpoint must see it.
    let seen = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let probe = dir.clone();
    let counting = seen.clone();
    let stopping = move || {
        if probe.join("a.ran").exists() {
            counting.set(counting.get() + 1);
            return true;
        }
        false
    };
    let mut queue = Queue::new();
    let mut procs = Subprocesses::default();
    let drive = run_stages::drive_walk(
        &stages,
        &mut queue,
        || false,
        stopping,
        |_| {},
        &mut procs,
    );
    match drive {
        run_stages::Drive::Stopped { ran, left } => {
            assert_eq!(ran, 1, "only the task that started before the stop got done");
            assert_eq!(left, 2, "the two later tasks were never begun");
        }
        other => panic!("a stop between tasks must end the walk, got {other:?}"),
    }
    assert!(!dir.join("b.ran").exists(), "nothing after the stop ran");
    assert!(!dir.join("c.ran").exists(), "nothing after the stop ran");
    assert!(seen.get() >= 1, "the checkpoint was re-asked between tasks");
    let track = queue.track(TRACK_STT);
    assert_eq!(track.taken, 1, "only what ran was taken");
}

/// S3: a pause costs one poll each time and then lets the run go on — neither a block nor a spin.
#[test]
fn f0_5_s3_a_pause_polls_and_then_runs() {
    let dir = scratch("pause");
    let stages = vec![Stage {
        track: TRACK_STT,
        name: "speech",
        phase: 1,
        of: 1,
        kind: "chunk",
        tasks: vec![marker_task(&dir, "paused-then-run")],
    }];
    // Paused for the first two checkpoints, then released.
    let polls = std::cell::Cell::new(0usize);
    let paused = move || {
        polls.set(polls.get() + 1);
        polls.get() <= 2
    };
    let mut queue = Queue::new();
    let mut procs = Subprocesses::default();
    let started = Instant::now();
    let drive = run_stages::drive_walk(
        &stages,
        &mut queue,
        paused,
        || false,
        |_| {},
        &mut procs,
    );
    let waited = started.elapsed();
    assert_eq!(drive, run_stages::Drive::Done { ran: 1, skipped: 0 });
    assert!(
        waited >= CHECKPOINT_POLL,
        "a parked run waited at least one 200 ms poll, it waited {waited:?}"
    );
    assert!(
        waited < Duration::from_secs(5),
        "and it did not hang: {waited:?}"
    );
    assert!(dir.join("paused-then-run.ran").exists(), "the task ran once released");
}

/// S4 / §Failure-is-specific: a task whose program is missing fails with the program's own name in
/// the error, so the log says which box to look at instead of leaving a mystery three stages in.
#[test]
fn f0_5_s4_a_failed_task_names_its_program_and_is_not_a_mystery() {
    let stages = vec![Stage {
        track: TRACK_STT,
        name: "speech",
        phase: 1,
        of: 1,
        kind: "chunk",
        tasks: vec![Task::run_it(
            "recognising",
            "definitely-not-installed-ffmpeg",
            vec!["-i".to_string(), "take.wav".to_string()],
        )],
    }];
    let mut queue = Queue::new();
    let mut procs = Subprocesses::default();
    let drive = run_stages::drive_walk(
        &stages,
        &mut queue,
        || false,
        || false,
        |_| {},
        &mut procs,
    );
    match drive {
        run_stages::Drive::Failed { stage, error } => {
            assert_eq!(stage, "speech", "the failing stage is named");
            assert!(
                error.contains("definitely-not-installed-ffmpeg"),
                "the error names the program that would not start: {error}"
            );
        }
        other => panic!("a missing program must fail the walk, got {other:?}"),
    }
}

/// The plan itself is built from the live project through the real argv builders, not hand-written:
/// an empty session plans no work, and a session with recordings plans ffmpeg tasks whose arguments
/// came out of `frames::scene_pass_plan` / `asr::cut_plan`.
#[test]
fn f0_5_s2_the_plan_comes_from_the_project_through_the_real_argv_builders() {
    let empty = Project::default();
    assert!(
        run_stages::plan(run::Step::Prepare, &empty).is_empty(),
        "an empty session has no work to put on the bar"
    );

    let mut project = Project::default();
    project.sources.push(naivepost::project::Source {
        path: "sources/recording.mkv".to_string(),
        footage: true,
        narrator: 1,
        sepvoice: false,
        tracks: vec![],
    });
    project.sources.push(naivepost::project::Source {
        path: "sources/talk.wav".to_string(),
        footage: false,
        narrator: 2,
        sepvoice: false,
        tracks: vec![],
    });
    let stages = run_stages::plan(run::Step::Prepare, &project);
    assert_eq!(stages.len(), 2, "frames and speech, one stage each");
    assert_eq!(stages[0].track, TRACK_FRAMES, "frames ride their own half of the bar");
    assert_eq!(stages[1].track, TRACK_STT, "speech rides the other half");
    assert_eq!(stages[0].tasks[0].program, run_stages::FFMPEG, "the frames pass is ffmpeg's");
    // The scene-detect filter string comes straight out of frames.rs, unchanged.
    let joined = stages[0].tasks[0].args.join(" ");
    assert!(joined.contains("scdet"), "the planned command carries the scene detector: {joined}");
    assert!(
        stages[1].tasks[0].args.join(" ").contains("-ss"),
        "the speech chunk was cut with asr's own plan"
    );
    // Narrate and Produce drive their own queues, so they ask for nothing here rather than being
    // counted twice onto the same bar.
    assert!(run_stages::plan(run::Step::Narrate, &project).is_empty());
    assert!(run_stages::plan(run::Step::Produce, &project).is_empty());
}

/// `drive_page` is the window's entry point: with a session that has work it walks the plan, and the
/// run's own flags are what stops it — read off the bar at each checkpoint, not captured at start.
#[test]
fn f0_5_s3_drive_page_reads_the_run_flags_at_each_checkpoint() {
    let mut project = Project::default();
    project.sources.push(naivepost::project::Source {
        path: "sources/talk.wav".to_string(),
        footage: false,
        narrator: 1,
        sepvoice: false,
        tracks: vec![],
    });
    let queue = std::rc::Rc::new(std::cell::RefCell::new(Queue::new()));
    let bar = std::rc::Rc::new(std::cell::RefCell::new(RunBar::default()));
    let procs = std::rc::Rc::new(std::cell::RefCell::new(Subprocesses::default()));
    // No stop up: the walk runs (and fails on the absent ffmpeg, naming it, which is the honest answer).
    let drive = run_stages::drive_page(run::Step::Prepare, &project, &queue, &bar, &procs);
    match drive {
        run_stages::Drive::Failed { stage, error } => {
            assert!(matches!(stage, "frames" | "speech"), "a real stage failed: {stage}");
            assert!(error.contains("ffmpeg"), "it named the missing tool: {error}");
        }
        other => panic!("with no ffmpeg installed the walk must report that, got {other:?}"),
    }
    // With the stop flag already up, the very first checkpoint ends the walk: nothing ran.
    bar.borrow_mut().stop_flag = true;
    let stopped = run_stages::drive_page(run::Step::Prepare, &project, &queue, &bar, &procs);
    match stopped {
        run_stages::Drive::Stopped { ran, .. } => assert_eq!(ran, 0, "a stopped run starts nothing"),
        other => panic!("⏹ up must stop the walk, got {other:?}"),
    }
}
