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
use naivepost::run_stages::{self, Stage, Task};
use naivepost::runqueue::{Queue, CHECKPOINT_POLL, TRACK_FRAMES, TRACK_STT};

/// The counting happens by side effect on the stage's argv: each task writes its own marker file, so
/// "was this task run?" is answered by the filesystem rather than by trusting the driver's word.
fn marker_task(dir: &std::path::Path, name: &str) -> Task {
    let stamp = dir.join(format!("{name}.ran"));
    Task {
        label: format!("task {name}"),
        program: "/bin/sh".to_string(),
        args: vec!["-c".to_string(), format!("touch {}", stamp.display())],
        already_done: false,
        session: None,
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
    let drive = run_stages::drive_walk(&stages, &mut queue, || false, || false, |_| {}, &mut procs);
    assert_eq!(drive, run_stages::Drive::Done { ran: 2, skipped: 0 });
    // Both tasks really ran: their marker files exist.
    assert!(dir.join("one.ran").exists(), "the first task spawned");
    assert!(dir.join("two.ran").exists(), "the second task spawned");
    let track = queue.track(TRACK_STT);
    assert_eq!(track.job, "speech", "qJob named the stage");
    assert_eq!(track.queued, 2, "qPush put both tasks on the track");
    assert_eq!(track.taken, 2, "every task was taken, none skipped");
    assert!(track.done_job, "qDone closed the track");
    assert_eq!(
        queue.fraction(),
        1.0,
        "a finished stage fills its half of the bar"
    );
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
    let drive = run_stages::drive_walk(&stages, &mut queue, || false, || false, |_| {}, &mut procs);
    assert_eq!(drive, run_stages::Drive::Done { ran: 1, skipped: 1 });
    let track = queue.track(TRACK_FRAMES);
    assert_eq!(track.queued, 2, "both were pushed");
    assert_eq!(
        track.taken, 2,
        "the already-done task was taken as well — that is the stall fix"
    );
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
    let drive = run_stages::drive_walk(&stages, &mut queue, || false, stopping, |_| {}, &mut procs);
    match drive {
        run_stages::Drive::Stopped { ran, left } => {
            assert_eq!(
                ran, 1,
                "only the task that started before the stop got done"
            );
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
    let drive = run_stages::drive_walk(&stages, &mut queue, paused, || false, |_| {}, &mut procs);
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
    assert!(
        dir.join("paused-then-run.ran").exists(),
        "the task ran once released"
    );
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
    let drive = run_stages::drive_walk(&stages, &mut queue, || false, || false, |_| {}, &mut procs);
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
    // §04 S6's order: ingest first (frames on one half of the bar, speech on the other), then the
    // understanding stages that read what those wrote: the text gate and describe over the footage row
    // (the gate always planned, judged by the main model when no decision model is set), fix over both.
    assert_eq!(
        stages.len(),
        5,
        "frames, speech, textgate, describe, fix: one stage each"
    );
    let names: Vec<&str> = stages.iter().map(|stage| stage.name).collect();
    assert_eq!(names[2..4], ["textgate", "describe"], "{names:?}");
    assert_eq!(
        stages[2].tasks[0].program,
        naivepost::text_gate::TEXT_GATE_TASK,
        "the gate is planned without a decision model"
    );
    assert_eq!(
        stages[0].track, TRACK_FRAMES,
        "frames ride their own half of the bar"
    );
    assert_eq!(stages[1].track, TRACK_STT, "speech rides the other half");
    // The frames row is a DIAL of this module, not one spawned command: §F1.6 S2's scene sweep plus one
    // ffmpeg per frame is far more than a single task can carry, and the sweep's scores have to be read
    // rather than discarded. `frames_run::stage` says so with its program word, as the speech and
    // describe rows do.
    assert_eq!(
        stages[0].tasks[0].program,
        naivepost::frames_run::FRAMES_TASK,
        "the frames pass is a dial of frames_run, not a spawned cut"
    );
    // And the door's own argv builder is what the sweep will run, unchanged (S2): four frames a second,
    // the throwaway 640-wide scale, and the detector reporting every score.
    let sweep = naivepost::frames::scene_pass_plan(naivepost::frames::SCENE_THRESHOLD).join(" ");
    assert!(
        sweep.contains("fps=4") && sweep.contains("scdet"),
        "the planned sweep is the scene detector: {sweep}"
    );
    assert_eq!(
        stages[1].tasks[0].program,
        run_stages::AUDIO_TASK,
        "speech is a dial of audio.cpp, not a spawned cut"
    );
    let heard: Vec<&str> = stages[1]
        .tasks
        .iter()
        .map(|task| task.args[0].as_str())
        .collect();
    assert_eq!(
        heard,
        ["sources/recording.mkv", "sources/talk.wav"],
        "F1.3 S0: the video is heard as well as the microphone"
    );
    // The fix stage rides after describe. Its rows are the same kind of work -- a chat request -- except
    // for §F1.8's two session rows that bracket them: S1's placement first (offsets before any block
    // goes out) and S5-S7's merge last, both dials of `fix_session` rather than spawns.
    let fix_programs: Vec<&str> = stages[4]
        .tasks
        .iter()
        .map(|task| task.program.as_str())
        .collect();
    assert_eq!(
        fix_programs.first().copied(),
        Some(naivepost::fix_session::PLACEMENT_TASK),
        "the fix stage opens with S1's placement row: {fix_programs:?}"
    );
    assert_eq!(
        fix_programs.get(1).copied(),
        Some(run_stages::TEXT_TASK),
        "the fix rows are a chat dial, named by their own program: {fix_programs:?}"
    );
    assert_eq!(
        fix_programs.last().copied(),
        Some(naivepost::fix_session::SESSION_TASK),
        "the fix stage closes with S5-S7's merge row: {fix_programs:?}"
    );
    // The row carries what the door needs and nothing it has to compute: the recording and the language.
    // Where each chunk is cut is the door's own plan (§F1.3), pinned in `tests/suite/audio_task_dial.rs`'s
    // `..._s1_asr_is_asked_per_chunk_with_its_own_cut`.
    assert_eq!(
        stages[1].tasks[1].args,
        vec![
            "sources/talk.wav".to_string(),
            project.policy.language.value.clone()
        ],
        "the speech row names its source and the session language"
    );
    // Narrate and Produce drive their own queues, so they ask for nothing here rather than being
    // counted twice onto the same bar.
    assert!(run_stages::plan(run::Step::Narrate, &project).is_empty());
    assert!(run_stages::plan(run::Step::Produce, &project).is_empty());
}

/// `drive_worker` is the walk Prepare's worker runs: with a session that has work it walks the plan, and the
/// run's own flags are what stops it, mirrored off the bar (`mirror_switches`) and read at each checkpoint,
/// not captured at start.
#[test]
fn f0_5_s3_drive_worker_reads_the_run_flags_at_each_checkpoint() {
    let mut project = Project::default();
    project.sources.push(naivepost::project::Source {
        path: "sources/talk.wav".to_string(),
        footage: false,
        narrator: 1,
        sepvoice: false,
        tracks: vec![],
    });
    let mut bar = RunBar::default();
    run_stages::mirror_switches(&bar);
    // No stop up: the walk runs and fails at the first row it cannot do — speech is a dial now, and with
    // no decoded audio in the session there is nothing to ask audio.cpp about. Naming that reason is the
    // honest answer, exactly as naming an absent ffmpeg was while this row spawned a cut.
    let mut stages_seen: Vec<String> = Vec::new();
    let drive = run_stages::drive_worker(
        run::Step::Prepare,
        &project,
        Queue::new(),
        &mut |_, stage| stages_seen.push(stage.to_string()),
    );
    assert!(
        !stages_seen.is_empty(),
        "the walk shows the window its queue and stage as it goes"
    );
    match drive {
        run_stages::Drive::Failed { stage, error } => {
            assert!(
                matches!(stage, "frames" | "speech"),
                "a real stage failed: {stage}"
            );
            assert!(
                error.contains("no decoded audio") || error.contains("ffmpeg"),
                "it named why the row could not run: {error}"
            );
        }
        other => panic!("with nothing to transcribe the walk must report that, got {other:?}"),
    }
    // With the stop flag already up, the very first checkpoint ends the walk: nothing ran.
    bar.stop_flag = true;
    run_stages::mirror_switches(&bar);
    let stopped =
        run_stages::drive_worker(run::Step::Prepare, &project, Queue::new(), &mut |_, _| {});
    bar.stop_flag = false;
    run_stages::mirror_switches(&bar);
    match stopped {
        run_stages::Drive::Stopped { ran, .. } => {
            assert_eq!(ran, 0, "a stopped run starts nothing")
        }
        other => panic!("⏹ up must stop the walk, got {other:?}"),
    }
}

/// F1.3 S0: a project of videos alone is transcribed, one speech row per video. The first rewrite heard only
/// the rows that were not footage, so 07-news (19 videos) got no transcript and its fix wrote empty files.
#[test]
fn f1_3_s0_every_source_is_heard_a_videos_own_sound_too() {
    let mut project = Project::default();
    for name in ["a.mkv", "b.mkv"] {
        project.sources.push(naivepost::project::Source {
            path: format!("sources/{name}"),
            footage: true,
            ..Default::default()
        });
    }
    let stages = run_stages::plan(run::Step::Prepare, &project);
    let speech = stages
        .iter()
        .find(|stage| stage.name == "speech")
        .expect("a speech stage for a project of videos");
    let heard: Vec<&str> = speech
        .tasks
        .iter()
        .map(|task| task.args[0].as_str())
        .collect();
    assert_eq!(heard, ["sources/a.mkv", "sources/b.mkv"]);
}
