//! §03-shell.md F0.3 Press ⏹ — the rule, without a window. What one press of ⏹ stops, in what
//! order, and what it says while doing it, held as plain data so the precedence can be tested at all
//! (spec/00-principles.md §5). The prototype's `stopClicked` (gui/runbar.go:114-136) is the shape
//! this mirrors: stop the page's preview first, then bring the run down, and never treat a child
//! killed by that stop as a failure.

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
