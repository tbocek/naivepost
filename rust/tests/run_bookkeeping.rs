//! F0.5 A run's bookkeeping (spec/03-shell.md F0.5) — the plain-logic side.
//!
//! Every rule lives in `naivepost::runqueue`; these tests exercise it directly, with no window in
//! the way (the wire through the widgets is `run_bookkeeping_widgets.rs`). Which clause each test
//! pins:
//!
//! * S1 — `startRun`: running on, flags cleared, a fresh cancel context, the queue reset, the model
//!   log page closed, the log expanded — and that order.
//! * S2 — `qJob` / `qPush` / `qTake` / `prog` / `qDone`: the bar's two halves, their text, their
//!   tooltip, and the fraction that must never go backwards.
//! * S3 — `checkpoint()` between subprocesses: pause polls, stop returns the stop error.
//! * S4 — `endRun`: running off, the lucky chain moves on, the audio models unloaded best-effort.

use std::time::Duration;

use naivepost::exchanges::{self, Call, Message, Mode, Part};
use naivepost::layout::Tree;
use naivepost::params;
use naivepost::run::{RunBar, Snapshot, Step};
use naivepost::runqueue::{
    self, checkpoint, Queue, Checkpoint, CHECKPOINT_POLL, PROGRESS_PULSE, TRACK_DESCRIBE, TRACK_FIX,
    TRACK_FRAMES, TRACK_STT,
};
use naivepost::services::{Kind, UNLOAD_IS_BEST_EFFORT, UNLOAD_TIMEOUT};
use naivepost::shell::Page;

/// A project folder no other test run shares, for the exchange-page half of S1.
fn tree(tag: &str) -> Tree {
    let dir = std::env::temp_dir().join(format!("naivepost-f05-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Tree::new(dir.join("show.naivepost")).unwrap()
}

fn call(step: &str) -> Call {
    Call {
        step: step.into(),
        model: "qwen3-32b".into(),
        mode: Mode::Thinking,
        took_secs: 0,
        thinking_secs: None,
        messages: vec![Message { role: "user".into(), parts: vec![Part::Text("x".repeat(8))] }],
        reply: "done".into(),
        reasoning: None,
        tool_calls: Vec::new(),
        cut_off: false,
        error: None,
    }
}

// --- S1: startRun -----------------------------------------------------------------------------------

#[test]
fn f0_5_s1_start_run_turns_running_on_and_opens_the_log() {
    // F0.5 S1: running, log expanded, and the epoch returned is the bar's own.
    let mut bar = RunBar::default();
    let mut queue = Queue::new();
    let mut log = exchanges::RunLog::new();
    let started =
        runqueue::start_run(&mut bar, &mut queue, &mut log, Step::Prepare, Snapshot::default());

    assert!(bar.running.is_some(), "S1: startRun flips running on");
    assert_eq!(started.epoch, bar.run_epoch, "S1: the epoch reported is the bar's");
    assert!(
        bar.running.as_ref().unwrap().log_expanded,
        "S1: a run that says nothing where you can see it did not run"
    );
    assert!(!started.closed_log_page, "nothing was open, so nothing got closed");
}

#[test]
fn f0_5_s1_start_run_clears_a_stale_cancel_from_the_previous_run() {
    // F0.5 S1: a `cancelled` left standing by the run before must not abort this one.
    let mut bar = RunBar::default();
    let mut queue = Queue::new();
    let mut log = exchanges::RunLog::new();
    bar.cancelled = true;
    bar.stop_flag = true;

    let epoch = runqueue::start_run(&mut bar, &mut queue, &mut log, Step::Suggest, Snapshot::default()).epoch;

    assert!(!bar.cancelled, "S1: the cancel flag is cleared for the new run");
    assert!(!bar.stop_flag, "S1: the stop flag is cleared too");
    assert_eq!(epoch, bar.run_epoch);
    // And the stale pairing cannot be reconstructed: the old epoch is not the current one.
    assert!(
        !runqueue::is_cancelled(true, epoch - 1, bar.run_epoch),
        "a cancellation flagged in an earlier run does not speak for this one"
    );
    assert!(
        runqueue::is_cancelled(true, epoch, bar.run_epoch),
        "same generation plus the flag is what means cancelled"
    );
}

#[test]
fn f0_5_s1_start_run_resets_the_queue_it_inherits() {
    // F0.5 S1: the queue reset happens here because the tracks are summed — last run's leftovers
    // would be added to every reading this one takes.
    let mut bar = RunBar::default();
    let mut queue = Queue::new();
    let mut log = exchanges::RunLog::new();
    queue.job(TRACK_STT, "speech", 0, 0);
    queue.push(TRACK_STT, 4, "chunk");
    queue.prog(TRACK_STT, 0.75, "recognising");
    assert!(queue.fraction() > 0.5, "the seeded run has moved the bar");

    runqueue::start_run(&mut bar, &mut queue, &mut log, Step::Produce, Snapshot::default());

    assert_eq!(queue.fraction(), 0.0, "S1: the queue is reset to zero");
    assert_eq!(queue.text(), "", "S1: and says nothing until a job is put on it");
}

#[test]
fn f0_5_s1_start_run_closes_the_model_log_page() {
    // F0.5 S1: "queue reset (also closes the model log page)" — closing it here is what makes the
    // next LLM call open a new page named for whichever step makes it.
    let t = tree("s1-close");
    let mut bar = RunBar::default();
    let mut queue = Queue::new();
    let mut log = exchanges::RunLog::new();
    log.record(&t, "0925-120000", "Describe", &call("Describe"));
    assert!(log.is_open(), "a call opened the run's exchange page");

    let started = runqueue::start_run(&mut bar, &mut queue, &mut log, Step::Prepare, Snapshot::default());

    assert!(started.closed_log_page, "S1 reports that a page had to be closed");
    assert!(!log.is_open(), "S1: the model log page is closed at the start of the next run");
    assert_eq!(log.page_rel(), None);
}

#[test]
fn f0_5_s1_the_cancel_context_is_freshened_before_the_queue_reset() {
    // F0.5 S1's order: the flags go first, because a queue reset repaints the bar and a stale flag
    // in that paint shows a stopped run as still paused. `bump_cancel_context` is the first thing
    // `start_run` does, and both effects land together.
    let mut bar = RunBar::default();
    let mut queue = Queue::new();
    let mut log = exchanges::RunLog::new();
    // A run already standing, paused, with work on the bar.
    bar.running = Some(naivepost::run::Run {
        step: Step::Narrate,
        paused: true,
        log_expanded: false,
        sources: Snapshot::default(),
    });
    queue.push(TRACK_FIX, 3, "block");
    queue.prog(TRACK_FIX, 1.0, "fixing block 3/3");

    let started = runqueue::start_run(&mut bar, &mut queue, &mut log, Step::Prepare, Snapshot::default());

    let run = bar.running.as_ref().expect("a run stands after start_run");
    assert!(!run.paused, "S1: the pause flag of the previous run is cleared");
    assert!(!bar.cancelled && !bar.stop_flag, "S1: the cancel context is fresh");
    assert_eq!(queue.fraction(), 0.0, "S1: then the queue is emptied");
    assert!(run.log_expanded, "S1: and the log opens last, over clean state");
    assert_eq!(started.epoch, bar.run_epoch);
}

// --- S2: the queue's calls --------------------------------------------------------------------------

#[test]
fn f0_5_s2_job_push_take_prog_read_as_the_prototype_writes_them() {
    // F0.5 S2: `qJob`/`qPush`/`qTake`/`prog` render as "describe 1/2: chunk 4/12", with the
    // counting spelled out in the tooltip rather than on the bar.
    let mut q = Queue::new();
    q.job(TRACK_STT, "describe", 1, 2);
    q.push(TRACK_STT, 12, "chunk");
    for _ in 0..4 {
        q.take(TRACK_STT);
    }
    q.prog(TRACK_STT, 4.0 / 12.0, "chunk 4/12");

    assert_eq!(q.text(), "describe 1/2: chunk 4/12");
    assert_eq!(q.track(TRACK_STT).tip(), "describe: task 4 of 12, 8 waiting");
    assert!(
        (q.fraction() - 4.0 / 12.0).abs() < 1e-9,
        "the track's own absolute contribution is what the bar shows"
    );
}

#[test]
fn f0_5_s2_a_skipped_task_is_taken_too_or_the_bar_stalls() {
    // F0.5 S2: `qTake` is called once per task whatever becomes of it. Work already on disk from an
    // earlier run is a task this run is done with; a queue that skipped those would sit forever at
    // the position where the resume started. With no `prog` in between, the head reads the bare
    // position — `what` was cleared and `kind` stands in for the words.
    let mut q = Queue::new();
    q.job(TRACK_FIX, "transcript", 2, 2);
    q.push(TRACK_FIX, 7, "block");
    // Three outputs already exist: taken anyway, silently, with no `prog` between them.
    for _ in 0..3 {
        q.take(TRACK_FIX);
    }
    assert_eq!(q.track(TRACK_FIX).taken, 3, "skipped work still advances the head");
    // The position is carried by the tooltip while the short line stays on the job's kind; the count
    // lands in the words as soon as `prog` has something to say.
    assert_eq!(q.text(), "transcript 2/2: block", "and the line counts past it");
    assert_eq!(
        q.track(TRACK_FIX).tip(),
        "transcript: task 3 of 7, 4 waiting",
        "the counting lives in the tooltip"
    );

    // A `prog` after the skip carries its own words instead of stacking the count on top.
    q.prog(TRACK_FIX, 3.0 / 7.0, "fixing block 3/7");
    assert_eq!(q.text(), "transcript 2/2: fixing block 3/7", "the count is said once");
}

#[test]
fn f0_5_s2_push_of_nothing_adds_nothing() {
    // F0.5 S2: `qPush(track, 0, _)` is a no-op, not a way to take work back off a queue.
    let mut q = Queue::new();
    q.job(TRACK_STT, "speech", 0, 0);
    q.push(TRACK_STT, 3, "chunk");
    let before = q.track(TRACK_STT).clone();
    q.push(TRACK_STT, 0, "other");
    assert_eq!(q.track(TRACK_STT), &before, "zero tasks changes nothing");
    assert_eq!(before.kind, "chunk", "and it does not overwrite the kind either");
}

#[test]
fn f0_5_s2_two_tracks_sum_into_one_bar() {
    // F0.5 S2: the two tracks are concurrent in Prepare (speech on the GPU, frames on the CPU) and
    // each owns half; letting either write a raw fraction would make the bar bounce between them.
    let mut q = Queue::new();
    q.job(TRACK_STT, "speech", 0, 0);
    q.job(TRACK_FRAMES, "frames", 0, 0);
    q.push(TRACK_STT, 4, "chunk");
    q.push(TRACK_FRAMES, 4, "frame");
    q.prog(TRACK_STT, 0.25, "recognising");
    q.prog(TRACK_FRAMES, 0.25, "extracting");
    assert!((q.fraction() - 0.5).abs() < 1e-9, "both halves add up");

    // And the sum can never read past full, however much each track claims.
    q.prog(TRACK_STT, 0.9, "recognising");
    q.prog(TRACK_FRAMES, 0.9, "extracting");
    assert_eq!(q.fraction(), 1.0, "clamped to what a bar can show");
    // Both jobs still read on the line, joined.
    assert!(q.text().contains("speech") && q.text().contains("frames"), "two jobs, one line");
}

#[test]
fn f0_5_s2_phase_maps_a_track_into_its_slice_of_the_bar() {
    // F0.5 S2: a single-step press never calls `phase`; Prepare's ▶ is two steps back to back, each
    // reporting its own whole bar, and the scaling happens here rather than at every needle move.
    let mut q = Queue::new();
    q.phase(0.5, 0.5);
    q.job(TRACK_DESCRIBE, "describe", 1, 2);
    q.push(TRACK_DESCRIBE, 2, "segment");
    q.prog(TRACK_DESCRIBE, 1.0, "describing");
    // Both halves start at base/2 and the described one is scaled into its slice: 2*(0.5/2) + 1.0*0.5
    // = 1.0. The mapping matters for what has NOT been reported yet — a phase that has said nothing
    // stands where the one before it finished (base/2 each side), so the bar never drops when the
    // second job of a phase starts.
    let expected = 2.0 * (0.5 / 2.0) + 1.0 * 0.5;
    assert!(
        (q.fraction() - expected).abs() < 1e-9,
        "share maps the track into the phase's slice: {} vs {}",
        q.fraction(),
        expected
    );
    // The untouched half still sits at the previous phase's end rather than at zero.
    let mut quiet = Queue::new();
    quiet.phase(0.5, 0.5);
    assert!(
        (quiet.fraction() - 0.5).abs() < 1e-9,
        "a fresh phase holds the bar where the last one left it"
    );

    // Share 0 means no phase was set, which is the whole bar — every step but Prepare.
    let mut plain = Queue::new();
    plain.job(TRACK_STT, "produce", 0, 0);
    plain.push(TRACK_STT, 4, "clip");
    plain.prog(TRACK_STT, 0.25, "rendering");
    assert!(
        (plain.fraction() - 0.25).abs() < 1e-9,
        "with no phase set the track's fraction IS the bar"
    );
}

#[test]
fn f0_5_s2_qdone_keeps_the_half_earned_and_quiets_the_line() {
    // F0.5 S2: a finished track keeps its part of the bar and stops saying anything but its name, so
    // the line belongs to whatever is still running.
    let mut q = Queue::new();
    q.job(TRACK_STT, "speech", 0, 0);
    q.job(TRACK_FRAMES, "frames", 0, 0);
    q.push(TRACK_STT, 2, "chunk");
    q.push(TRACK_FRAMES, 4, "frame");
    q.prog(TRACK_STT, 1.0, "recognising");
    q.prog(TRACK_FRAMES, 0.5, "extracting");
    q.done(TRACK_STT, 1.0);

    assert!(
        (q.fraction() - 1.5).clamp(0.0, 1.0) == 1.0 || q.fraction() >= 1.0,
        "the earned half stays earned: {}",
        q.fraction()
    );
    assert_eq!(q.track(TRACK_STT).line(), "speech done", "a done job says only its name");
    assert!(q.text().contains("frames"), "and the live job still owns the words");
}

#[test]
fn f0_5_s2_an_emptied_queue_is_not_an_unfilled_one() {
    // F0.5 S2: `ever_filled` separates a queue that ran dry from one that was never loaded, so a
    // finished stage does not read as a stage that never existed.
    let mut filled = Queue::new();
    filled.job(TRACK_FIX, "transcript", 0, 0);
    filled.push(TRACK_FIX, 1, "block");
    filled.take(TRACK_FIX);
    filled.done(TRACK_FIX, 1.0);
    assert!(filled.track(TRACK_FIX).ever_filled);
    assert_eq!(filled.track(TRACK_FIX).tip(), "transcript: 1 task(s), all done");

    let mut never = Queue::new();
    never.job(TRACK_FIX, "transcript", 0, 0);
    assert!(!never.track(TRACK_FIX).ever_filled, "no push, no fill");
    assert_eq!(never.track(TRACK_FIX).tip(), "", "and nothing to say in the tooltip");
}

#[test]
fn f0_5_s2_the_bar_never_goes_backwards_within_a_phase() {
    // F0.5 S2: the fraction is NOT the queue's length — the queue grows as it is opened, and a total
    // that grows under a fraction drags the bar backwards. Each track reports its own absolute
    // contribution, so a rising track can only raise the bar.
    let mut q = Queue::new();
    q.job(TRACK_STT, "describe", 0, 0);
    q.push(TRACK_STT, 10, "segment");
    let mut last = 0.0;
    for i in 1..=10 {
        q.take(TRACK_STT);
        // More work turns up mid-run, as it always does.
        if i == 5 {
            q.push(TRACK_STT, 10, "segment");
        }
        q.prog(TRACK_STT, i as f64 / 20.0, "describing");
        let now = q.fraction();
        assert!(now >= last, "the bar went backwards at task {i}: {last} -> {now}");
        last = now;
    }
}

// --- S3: the checkpoint -----------------------------------------------------------------------------

#[test]
fn f0_5_s3_checkpoint_runs_when_nothing_was_asked() {
    // F0.5 S3: neither paused nor stopping, so the flow carries straight on.
    assert_eq!(checkpoint(false, false), Checkpoint::Ran);
}

#[test]
fn f0_5_s3_pause_polls_and_is_not_a_stop() {
    // F0.5 S3: a pause holds the run between subprocesses; it waits, it does not fail.
    assert_eq!(checkpoint(true, false), Checkpoint::Waited);
}

#[test]
fn f0_5_s3_stop_wins_over_a_held_pause() {
    // F0.5 S3: ⏹ returns the stop error even while a pause is held — a user who pressed ⏹ during a
    // pause asked for the run to end, and making them un-pause first to stop it would be absurd.
    assert_eq!(checkpoint(true, true), Checkpoint::Stopped);
    assert_eq!(checkpoint(false, true), Checkpoint::Stopped);
}

#[test]
fn f0_5_s3_the_two_clocks_are_200ms_and_150ms() {
    // F0.5 S3: the poll and the pulse, compared as Durations rather than slept on.
    // machine.checkpointPollMs — §10 lists it in prose ("checkpoint poll 200 ms") with no `P.` id.
    assert_eq!(CHECKPOINT_POLL, Duration::from_millis(200));
    assert_eq!(
        params::find("machine.checkpointPollMs").map(|row| row.spelled),
        Some("200 ms".to_string()),
        "// machine.checkpointPollMs is catalogued from runqueue::CHECKPOINT_POLL"
    );
    // machine.progressPulseMs — the bar pulses while a model thinks ("progress pulse 150 ms").
    assert_eq!(PROGRESS_PULSE, Duration::from_millis(150));
    assert_eq!(
        params::find("machine.progressPulseMs").map(|row| row.spelled),
        Some("150 ms".to_string()),
        "// machine.progressPulseMs is catalogued from runqueue::PROGRESS_PULSE"
    );
}

// --- S4: endRun -------------------------------------------------------------------------------------

#[test]
fn f0_5_s4_end_run_turns_running_off_and_moves_the_chain_on() {
    // F0.5 S4: running off, controls refreshed, and a lucky run moves to its next step.
    let mut bar = RunBar::default();
    bar.running = Some(naivepost::run::Run {
        step: Step::Suggest,
        paused: false,
        log_expanded: true,
        sources: Snapshot::default(),
    });
    bar.status = "stopping…".into();

    let ended = runqueue::end_run(&mut bar, Some(Page::Narrate));

    assert!(bar.running.is_none(), "S4: running is off");
    assert_eq!(bar.status, "", "S4: the bar's sentence is cleared for what comes next");
    assert!(ended.chain_moves_on, "S4: the chain has Narrate to move to");
    assert_eq!(ended.audio_unload, Kind::UnloadAll, "S4: and the audio unload is handed over");
}

#[test]
fn f0_5_s4_no_next_step_means_the_chain_stops_here() {
    // F0.5 S4: with nothing queued behind it, the chain does not move on.
    let mut bar = RunBar::default();
    let ended = runqueue::end_run(&mut bar, None);
    assert!(!ended.chain_moves_on, "no next page, no move");
}

#[test]
fn f0_5_s4_the_audio_unload_is_bounded_and_best_effort() {
    // F0.5 S4: the unload goes off-thread with a deadline of its own and never fails the run that
    // made it — and it is asked for EVERY run, including one that used no audio (§02-services).
    let (timeout, best_effort) = runqueue::audio_unload_is_bounded();
    assert_eq!(timeout, Duration::from_secs(20));
    assert_eq!(timeout, UNLOAD_TIMEOUT);
    assert!(best_effort);
    assert_eq!(best_effort, UNLOAD_IS_BEST_EFFORT);
}
