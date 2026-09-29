//! §03-shell.md F0.3 Press ⏹ — the rule, without a window. What one press of ⏹ stops, in what
//! order, and what it says while doing it, held as plain data so the precedence can be tested at all
//! (spec/00-principles.md §5). The prototype's `stopClicked` (gui/runbar.go:114-136) is the shape
//! this mirrors: stop the page's preview first, then bring the run down, and never treat a child
//! killed by that stop as a failure.

use std::os::unix::process::ExitStatusExt;
use std::time::Duration;

use naivepost::run::{self, RunBar, Subprocesses, Transport};
use naivepost::shell::Page;

/// A bar with a run under way, paused or not, as S1/S3 find it.
fn bar_with_run(step: run::Step, paused: bool) -> RunBar {
    RunBar {
        running: Some(run::Run {
            step,
            paused,
            log_expanded: true,
            sources: Default::default(),
        }),
        status: "describe 1/2: chunk 4/12".to_string(),
        ..Default::default()
    }
}

/// S1: a playing preview and a run going at once — one press ends both. The transport is reported
/// stopped, but because S1 falls through rather than returning, the status ends on "stopping…" and
/// the run comes down with it.
#[test]
fn f0_3_s1_stops_the_page_transport_and_still_stops_the_run() {
    let mut bar = bar_with_run(run::Step::Suggest, false);
    let mut procs = Subprocesses::default();
    procs.register(4101);

    // Cut has a preview (transport_for answers Some), and it is playing.
    let playing = Transport { playing: true, started: true };
    let stopped = bar.press_stop(Page::Cut, playing, false, &mut procs);

    assert_eq!(
        stopped.status(),
        run::STOPPING,
        "S1 falls through to the run, so the line reads 'stopping…', not 'playback stopped'"
    );
    assert_ne!(
        bar.status,
        run::PLAYBACK_STOPPED,
        "the playback sentence is overwritten by the run's — one press, both stopped"
    );
    assert!(bar.stop_flag, "the run was stopped too, not only the playback");
    assert!(bar.cancelled, "and its context cancelled, so model/audio calls abort");
    assert_eq!(stopped, run::Stopped::RunStopped { killed: vec![4101] });
    assert!(procs.is_empty(), "the registry was drained");
}

/// S1 also covers a preview that was started and parked rather than playing: `Transport::cued`.
#[test]
fn f0_3_s1_a_cued_preview_counts_as_playback_too() {
    let mut bar = bar_with_run(run::Step::Narrate, false);
    let mut procs = Subprocesses::default();

    // started but not playing == cued.
    let cued = Transport { playing: false, started: true };
    assert!(cued.cued(), "the fixture really is the cued case");
    let stopped = bar.press_stop(Page::Narrate, cued, false, &mut procs);

    assert_eq!(stopped.status(), run::STOPPING, "fall-through still applies");
    assert!(bar.stop_flag, "the run came down with the parked preview");
}

/// S1's other half: Prepare and Produce have no page preview at all (`transport_for` answers None), so
/// a `playing` flag there is somebody else's video and must not be treated as this page's transport.
/// The run half still stops. This keeps the S1 check from being written as a blanket
/// `transport.playing` that would claim Prepare owns a preview.
#[test]
fn f0_3_s1_prepare_page_has_no_transport() {
    for page in [Page::Prepare, Page::Produce] {
        assert!(
            run::transport_for(page, Transport { playing: true, started: true }).is_none(),
            "{page:?} has no page transport"
        );

        let mut bar = bar_with_run(run::step(page), false);
        let mut procs = Subprocesses::default();
        let stopped = bar.press_stop(
            page,
            Transport { playing: true, started: true },
            false,
            &mut procs,
        );

        // It stopped the run, not a transport: the run's sentence, and the stop flag set.
        assert_eq!(stopped.status(), run::STOPPING, "{page:?}: the run half still acts");
        assert!(bar.stop_flag, "{page:?}: the run stopped regardless of the playing flag");
    }

    // And with no run either, a playing flag on Prepare changes nothing at all: no transport to stop.
    let mut bare = RunBar::default();
    let mut procs = Subprocesses::default();
    assert_eq!(
        bare.press_stop(Page::Prepare, Transport { playing: true, started: true }, false, &mut procs),
        run::Stopped::NothingToDo,
        "Prepare ignores a preview: there is nothing here for ⏹ to end"
    );
}

/// S2: neither a preview nor a run. The press is ignored — no flag, no cancel, and the status line
/// keeps what it already said rather than being cleared.
#[test]
fn f0_3_s2_no_run_is_nothing_more() {
    let mut bar = RunBar {
        running: None,
        status: "narration written: 42 lines".to_string(),
        ..Default::default()
    };
    let before = bar.status.clone();
    let mut procs = Subprocesses::default();
    procs.register(7);

    let stopped = bar.press_stop(Page::Cut, Transport::default(), false, &mut procs);

    assert_eq!(stopped, run::Stopped::NothingToDo);
    assert_eq!(stopped.status(), "", "S2 says nothing");
    assert!(!bar.stop_flag, "no run, so no stop flag");
    assert!(!bar.cancelled, "nothing to cancel");
    assert_eq!(procs.len(), 1, "the registry was not touched by an idle press");
    assert_eq!(bar.status, before, "the existing sentence survives, uncleared");
}

/// S3: the run is brought down — flag set, context cancelled, every registered child handed off to be
/// killed, and the pause cleared because a stop is not a pause.
#[test]
fn f0_3_s3_flag_cancel_and_kill_all() {
    let mut bar = bar_with_run(run::Step::Prepare, true);
    let mut procs = Subprocesses::default();
    procs.register(9001);
    procs.register(9002);
    assert_eq!(procs.len(), 2);

    let stopped = bar.press_stop(Page::Prepare, Transport::default(), false, &mut procs);

    assert!(bar.stop_flag, "S3 sets the stop flag");
    assert!(bar.cancelled, "S3 cancels the run context");
    let run = bar.running.as_ref().expect("the run record stays until the flow unwinds (F0.5)");
    assert!(!run.paused, "a stop is not a pause: the pause flag is cleared");
    assert_eq!(
        stopped,
        run::Stopped::RunStopped { killed: vec![9001, 9002] },
        "both children are handed to the caller to signal"
    );
    assert!(procs.is_empty(), "drained, so the next ⏹ cannot kill a recycled pid twice");
    assert_eq!(bar.status, run::STOPPING, "the status line reads 'stopping…'");
    // The same press reaches the legs that are already on the wire: the shared switch is what an
    // HTTP call reads with no borrow on this bar (§S3's "aborts model and audio calls").
    assert!(
        naivepost::cancel_leg::cancelled_at(bar.run_epoch),
        "S3 cancels THIS run's generation on the thread-shared switch"
    );
}

/// S4: a subprocess that died because we killed it is not a failure, and the stop only lands in the gap
/// between subprocesses rather than mid-call.
#[test]
fn f0_3_s4_stopped_subprocess_is_not_a_failure() {
    // Killed with the stop flag up: the user asked for it.
    assert!(run::stopped_is_not_failure(true, true));
    // Same exit with the flag clear: a real crash, and must read as one.
    assert!(!run::stopped_is_not_failure(false, true));
    // Nothing killed: nothing to excuse either way.
    assert!(!run::stopped_is_not_failure(true, false));
    assert!(!run::stopped_is_not_failure(false, false));

    // Where the stop takes effect: only between subprocesses. Mid-call it waits, for the same reason
    // ▶ pauses "after the current stage" — interrupting halfway leaves a half-written file.
    assert!(run::stop_takes_effect_between_subprocesses(true, false));
    assert!(!run::stop_takes_effect_between_subprocesses(true, true));
    assert!(!run::stop_takes_effect_between_subprocesses(false, false));
}

/// S5: a stop that reached Describe arms exactly one restart-from-the-start; reading it clears it.
#[test]
fn f0_3_s5_a_stop_inside_describe_arms_a_restart_from_the_start() {
    let mut bar = bar_with_run(run::Step::Prepare, false);
    let mut procs = Subprocesses::default();

    let stopped = bar.press_stop(Page::Prepare, Transport::default(), true, &mut procs);
    assert_eq!(stopped.status(), run::STOPPING, "the stop itself behaves the same");
    assert!(bar.describe_restarts(), "armed by the stop inside Describe");
    assert!(
        !bar.describe_restarts(),
        "cleared on read, so one stop affects exactly one Prepare run"
    );

    // A stop outside Describe never arms it.
    let mut elsewhere = bar_with_run(run::Step::Narrate, false);
    let mut idle = Subprocesses::default();
    let _ = elsewhere.press_stop(Page::Narrate, Transport::default(), false, &mut idle);
    assert!(!elsewhere.describe_restarts(), "no Describe involved, no restart armed");
}

// ---- the legs that leave the process: F0.3 S3's kill, S4's excuse, S5's restart ----

/// S3: a registered child is really terminated, not merely listed. The proof is the wait status:
/// `sleep 60` died of a signal rather than running out its minute, so something outside this process
/// ended it.
#[test]
fn f0_3_s3_stop_terminates_a_registered_child_for_real() {
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep 60");
    let pid = child.id();

    let mut bar = bar_with_run(run::Step::Prepare, false);
    let mut procs = Subprocesses::default();
    procs.register(pid);

    // The press drains the registry and hands the pids over; the leg signals them.
    let stopped = bar.press_stop(Page::Prepare, Transport::default(), false, &mut procs);
    let killed = match &stopped {
        run::Stopped::RunStopped { killed } => killed.clone(),
        other => panic!("expected the run to stop, got {other:?}"),
    };
    let signalled = naivepost::stop_legs::signal(&killed);
    assert_eq!(signalled, vec![pid], "the pid was actually signalled");
    assert!(procs.is_empty(), "and the registry was drained by the press");

    // The child is gone, killed by the signal rather than by finishing its own sleep. Polled with
    // `try_wait` rather than blocked on `wait`: if nothing had been signalled this would hang for the
    // full minute, and a test that hangs proves less than one that says "still alive".
    let status = wait_killed(&mut child);
    assert!(status.code().is_none(), "a signalled death carries no exit code: {status:?}");
    // Which signal depends on whether the target took SIGTERM or had to be killed outright; either one
    // means ⏹'s leg reached it, and neither is the exit code `sleep` would have on finishing.
    assert!(
        matches!(status.signal(), Some(libc::SIGTERM) | Some(libc::SIGKILL)),
        "it was ended by a signal from the stop leg, got {status:?}"
    );
}

/// Poll a child until it has exited, up to five seconds, and report why it did.
///
/// std's `Child::wait_timeout` is unstable in this toolchain, so the same thing by hand: `try_wait` is
/// non-blocking and returns `Ok(None)` while the child still runs.
fn wait_killed(child: &mut std::process::Child) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait().expect("try_wait on the child") {
            Some(status) => return status,
            None if std::time::Instant::now() >= deadline => panic!(
                "child {} was still running after 5 s — nothing signalled it",
                child.id()
            ),
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Same effect through the one call `wire_stop` makes, so what the button reaches is what is tested.
#[test]
fn f0_3_s3_the_button_leg_terminates_every_registered_child() {
    let first = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn first sleep");
    let second = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn second sleep");
    let (one, two) = (first.id(), second.id());

    let bar = bar_with_run(run::Step::Prepare, false);
    let mut procs = Subprocesses::default();
    procs.register(one);
    procs.register(two);

    let killed = naivepost::stop_legs::stop_registered(&mut procs);
    assert_eq!(killed.len(), 2, "both were signalled: {killed:?}");
    assert!(procs.is_empty(), "drained, so a second press cannot kill a recycled pid");

    for mut child in [first, second] {
        let status = wait_killed(&mut child);
        assert!(
            matches!(status.signal(), Some(libc::SIGTERM) | Some(libc::SIGKILL)),
            "ended by the stop leg's signal: {status:?}"
        );
    }
}

/// S4: a child that was already gone when the stop reached it is neither reported nor a failure — and
/// signalling it twice never raises an error either way.
#[test]
fn f0_3_s4_a_child_already_gone_is_not_reported_and_not_a_failure() {
    let mut child = std::process::Command::new("sleep")
        .arg("0.1")
        .spawn()
        .expect("spawn a short sleep");
    let pid = child.id();
    // Let it finish on its own, so the pid is dead before anything signals it.
    let finished = child.wait().expect("the short sleep finishes");
    assert!(finished.success(), "it ended by itself, unaided");

    let mut bar = bar_with_run(run::Step::Prepare, false);
    let mut procs = Subprocesses::default();
    procs.register(pid);
    let stopped = bar.press_stop(Page::Prepare, Transport::default(), false, &mut procs);
    let killed = match &stopped {
        run::Stopped::RunStopped { killed } => killed.clone(),
        other => panic!("expected the run to stop, got {other:?}"),
    };

    // First pass: the pid is already reaped, so nothing is reported as signalled.
    let first_pass = naivepost::stop_legs::signal(&killed);
    assert!(first_pass.is_empty(), "a gone pid is not claimed as killed: {first_pass:?}");
    // And again: still nothing, and no error surfaced anywhere.
    let second_pass = naivepost::stop_legs::signal(&killed);
    assert!(second_pass.is_empty(), "signalling a gone pid twice stays quiet: {second_pass:?}");
    // The rule S4 states, applied to the same fact: with the stop flag up, the death reads as stopped.
    assert!(run::stopped_is_not_failure(true, true));
}

/// S5: the arming comes from the stage seam, not from a literal the caller remembers to pass.
#[test]
fn f0_3_s3_describe_stage_is_what_the_press_reads() {
    assert!(!naivepost::stop_legs::describe_stage(), "outside the stage");
    naivepost::stop_legs::enter_describe();
    assert!(naivepost::stop_legs::describe_stage(), "inside the stage");

    // A press while inside arms the restart, reading the seam rather than being told.
    let mut bar = bar_with_run(run::Step::Prepare, false);
    let mut procs = Subprocesses::default();
    let _ = bar.press_stop(
        Page::Prepare,
        Transport::default(),
        naivepost::stop_legs::describe_stage(),
        &mut procs,
    );
    assert!(bar.describe_restarts(), "armed through the stage seam");

    naivepost::stop_legs::leave_describe();
    assert!(!naivepost::stop_legs::describe_stage(), "closed again");
    // After leaving, the same press does not arm: a stale flag would arm every later stop.
    let mut later = bar_with_run(run::Step::Prepare, false);
    let _ = later.press_stop(
        Page::Prepare,
        Transport::default(),
        naivepost::stop_legs::describe_stage(),
        &mut procs,
    );
    assert!(!later.describe_restarts(), "leaving the stage really closed it");
}

/// S5's consequence, all four arms. Armed and marker-present are two ways of saying the same thing —
/// the middle of the description cannot be trusted — so either one alone starts the description over:
/// a ⏹ cut through Describe (`armed`), or the last run left its working files unfinished on disk
/// (`resume_marker_present`, the lane's `events.tsv` / `state.txt` F1.1 S3 reads). Only with neither
/// is there nothing to restart. What starting over keeps (scaled frames, extracted frames,
/// transcripts) is what spec/00-principles #4 protects; only the untrustworthy event stream goes.
#[test]
fn f0_3_s5_an_armed_stop_describes_from_the_start() {
    use naivepost::stop_legs::describes_from_the_start as from_start;
    // Armed by a stop inside Describe, nothing on disk: start from the beginning.
    assert!(from_start(true, false), "armed with nothing on disk: start over");
    // Armed AND a marker left behind: still starts over — both say the middle is untrusted.
    assert!(from_start(true, true), "armed with a marker: start over");
    // A marker without the arming says the same about the files: start over.
    assert!(from_start(false, true), "marker present, not armed: start over");
    // Neither: nothing was stopped inside Describe, so nothing restarts.
    assert!(!from_start(false, false), "nothing at all: nothing to restart");
}

/// S5: the arming is consumed on read, so one stop affects exactly one Prepare run — the same
/// contract `RunBar::describe_restarts` holds for the bar's own flag.
#[test]
fn f0_3_s5_one_armed_stop_arms_exactly_one_run() {
    assert!(!naivepost::stop_legs::take_describe_restart(), "starts unarmed");
    naivepost::stop_legs::arm_describe_restart();
    assert!(naivepost::stop_legs::take_describe_restart(), "the armed stop is taken up once");
    assert!(
        !naivepost::stop_legs::take_describe_restart(),
        "and never again: the arming does not leak into a third run"
    );
}
