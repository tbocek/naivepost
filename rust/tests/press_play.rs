//! §03-shell.md F0.2 Press ▶ — what one press of the run bar's single button means.
//!
//! One button, four meanings, and the flow is a precedence: S1 a run under way (toggle pause),
//! else S2 this page's transport playing or started (toggle it), else S3 snapshot the sources and
//! S4 run this page's step alone. Every one of those decisions lives in [`naivepost::run`]; the ▶
//! on screen only draws [`run::controls`] and forwards a click to [`run::RunBar::press`], so all of
//! it is testable without a window (spec/00-principles.md §5).

use naivepost::cut;
use naivepost::project::{Project, Source};
use naivepost::run::{
    self as run, Controls, Finish, Pressed, Run, RunBar, Snapshot, Step, Transport, NARRATOR_SLOTS,
};
use naivepost::shell::Page;

/// The step S4 started, panicking with the whole press in the message when the press was a pause or
/// a transport toggle instead. A test helper, not a rule: the rules are all in [`run`].
fn step_of(pressed: &Pressed) -> Step {
    match pressed {
        Pressed::Started { step, .. } => *step,
        _ => panic!("the press was {pressed:?}, which started no step"),
    }
}

/// The snapshot S3 took alongside that step, `None` when no step started.
fn sources_of(pressed: &Pressed) -> Option<&Snapshot> {
    match pressed {
        Pressed::Started { sources, .. } => Some(sources),
        _ => None,
    }
}

/// A project with footage — the state a Cut page is unlocked in.
fn with_footage() -> Project {
    Project {
        sources: vec![Source {
            path: "cam.mp4".to_string(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// A project with no footage at all: the step's own refusal (F2.14's `NO_CUT_YET`) is what ▶ gets
/// to hear on it, not a lock — §1 keeps Narrate and Produce unlocked for exactly that reason.
fn without_footage() -> Project {
    Project {
        sources: vec![Source {
            path: "voice.wav".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// A run under way, working on `step`, not paused.
fn running(step: Step) -> Run {
    Run {
        step,
        paused: false,
        log_expanded: true,
        sources: Default::default(),
    }
}

/// The bar with a run under way, as S1 finds it.
fn bar_with_run(step: Step) -> RunBar {
    RunBar {
        running: Some(running(step)),
        status: "describe 1/2: chunk 4/12".to_string(),
        ..Default::default()
    }
}

#[test]
fn f0_2_s1_a_run_under_way_toggles_pause() {
    // S1: a run is going, so ▶ *is* the pause button. It says which of the two it just did, in the
    // spec's own words, and stops there — pausing is not a new run.
    let project = with_footage();
    let mut bar = bar_with_run(Step::Prepare);

    // A preview playing on the page does not steal the button from a run: S1 comes first.
    let playing = Transport {
        playing: true,
        started: true,
    };
    let pressed = bar.press(Page::Cut, playing, &project);
    assert_eq!(pressed, Pressed::ToggledPause { paused: true });
    // The ellipsis is the spec's, and it matters: the stage in hand finishes first.
    assert_eq!(bar.status, "pausing after the current stage\u{2026}");
    assert_eq!(bar.status, run::PAUSING);

    let run = bar.running.as_ref().expect("pausing keeps the run");
    assert!(run.paused, "the first press paused it");
    assert_eq!(run.step, Step::Prepare, "pausing does not change what is running");
    assert!(run.log_expanded, "the log stays where the run put it");

    let pressed = bar.press(Page::Cut, playing, &project);
    assert_eq!(pressed, Pressed::ToggledPause { paused: false });
    assert_eq!(bar.status, "resumed");
    assert_eq!(bar.status, run::RESUMED);
    assert_eq!(
        bar.running.as_ref().expect("still running").step,
        Step::Prepare
    );
}

#[test]
fn f0_2_s1_beats_s2_and_takes_no_fresh_snapshot() {
    // S1's "stop" is what keeps a pause from being a start: with a run under way the transport is
    // left alone and the sources are *not* re-read, because the run works on the snapshot it was
    // started with even while the list behind it changes.
    let mut project = with_footage();
    let mut bar = bar_with_run(Step::Narrate);
    bar.running.as_mut().unwrap().sources = run::Snapshot {
        footage: vec!["cam.mp4".to_string()],
        ..Default::default()
    };

    let pressed = bar.press(
        Page::Cut,
        Transport {
            playing: true,
            started: true,
        },
        &project,
    );
    assert!(
        matches!(pressed, Pressed::ToggledPause { .. }),
        "a run under way owns the button, not the preview: {pressed:?}"
    );

    project.sources.push(Source {
        path: "cam2.mp4".to_string(),
        footage: true,
        ..Default::default()
    });
    bar.press(Page::Cut, Transport::default(), &project);
    assert_eq!(
        bar.running.as_ref().unwrap().sources.footage,
        vec!["cam.mp4".to_string()],
        "a pause must not re-snapshot the sources out from under the run"
    );
}

#[test]
fn f0_2_s2_playing_preview_owns_the_button() {
    // S2: on Cut or Narrate, once a preview has been started the button belongs to it — playing
    // means pause it, started-but-parked (cued) means resume it. Either way no step starts.
    let project = with_footage();

    let mut bar = RunBar::default();
    let pressed = bar.press(
        Page::Cut,
        Transport {
            playing: true,
            started: true,
        },
        &project,
    );
    assert_eq!(pressed, Pressed::ToggledTransport { resume: false });
    assert!(bar.running.is_none(), "▶ toggled the preview, so no run began");

    // The voice sample on Narrate, parked part way through: ▶ resumes rather than restarts.
    let mut bar = RunBar::default();
    let cued = Transport {
        playing: false,
        started: true,
    };
    assert!(cued.cued(), "started and not playing is a cue, not playback");
    let pressed = bar.press(Page::Narrate, cued, &project);
    assert_eq!(pressed, Pressed::ToggledTransport { resume: true });
    assert!(bar.running.is_none());

    // A transport nobody has started yet is not the button's business — S3/S4 run instead.
    let mut bar = RunBar::default();
    let pressed = bar.press(Page::Cut, Transport::default(), &project);
    assert!(
        matches!(pressed, Pressed::Started { .. }),
        "an unstarted preview must not swallow ▶: {pressed:?}"
    );
}

#[test]
fn f0_2_s2_pages_without_a_transport_run_their_step() {
    // Only Cut and Narrate have a preview. Prepare and Produce ignore a playing transport —
    // [`run::transport_for`] answers `None` for them, which is why their ▶ runs the step.
    let project = with_footage();
    let playing = Transport {
        playing: true,
        started: true,
    };
    assert!(
        run::transport_for(Page::Prepare, playing).is_none(),
        "Prepare has no transport"
    );
    assert!(
        run::transport_for(Page::Produce, playing).is_none(),
        "Produce has no transport"
    );
    assert_eq!(run::transport_for(Page::Cut, playing), Some(playing));
    assert_eq!(run::transport_for(Page::Narrate, playing), Some(playing));

    for (page, want) in [
        (Page::Prepare, Step::Prepare),
        (Page::Produce, Step::Produce),
    ] {
        let mut bar = RunBar::default();
        let pressed = bar.press(page, playing, &project);
        assert_eq!(
            step_of(&pressed),
            want,
            "{} has no preview, so a playing one is not its business",
            page.label()
        );
    }
}

#[test]
fn f0_2_s3_the_sources_are_snapshotted() {
    // S3: the sources as they were when ▶ was pressed. Footage and voice split by the same question
    // the page asks; narrator slots 1..=4 by slot, and a slot number out of range belongs to neither.
    let project = Project {
        sources: vec![
            Source {
                path: "cam.mp4".to_string(),
                footage: true,
                narrator: 1,
                ..Default::default()
            },
            Source {
                path: "voice.wav".to_string(),
                narrator: 3,
                ..Default::default()
            },
            Source {
                path: "screen.mp4".to_string(),
                footage: true,
                ..Default::default()
            },
            Source {
                path: "mystery.bin".to_string(),
                narrator: 9,
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let mut bar = RunBar::default();
    let pressed = bar.press(Page::Prepare, Transport::default(), &project);
    let sources = sources_of(&pressed).expect("S3 hands back the snapshot it took");

    assert_eq!(sources.footage, vec!["cam.mp4", "screen.mp4"]);
    assert_eq!(sources.voice, vec!["voice.wav", "mystery.bin"]);
    assert_eq!(sources.narrators.len(), NARRATOR_SLOTS);
    assert_eq!(sources.narrators[0].as_deref(), Some("cam.mp4"));
    assert_eq!(sources.narrators[1], None, "slot 2 is held by nobody");
    assert_eq!(sources.narrators[2].as_deref(), Some("voice.wav"));
    assert_eq!(sources.narrators[3], None, "slot 9 is no slot at all");
    assert_eq!(
        bar.running.as_ref().unwrap().sources,
        *sources,
        "the run works on this copy, not on the project"
    );

    // A second press takes a fresh snapshot: it is the next run's own list of sources, and the one
    // before it keeps whatever it started with.
    let mut grew = project.clone();
    grew.sources.push(Source {
        path: "cam2.mp4".to_string(),
        footage: true,
        ..Default::default()
    });
    bar.finish(Finish::Done);
    let pressed = bar.press(Page::Prepare, Transport::default(), &grew);
    assert_eq!(sources_of(&pressed).unwrap().footage.len(), 3);
}

#[test]
fn f0_2_s4_each_page_runs_its_own_step_alone() {
    // S4: the visible tab's step, and only that one — no "all done" for a single step (asserted in
    // its own test below), and one run under way with the log opened for it.
    let project = with_footage();
    let want = [
        (Page::Prepare, Step::Prepare, "F1.1"),
        (Page::Cut, Step::Suggest, "F2.14"),
        (Page::Narrate, Step::Narrate, "F4.1"),
        (Page::Produce, Step::Produce, "F5.1"),
    ];
    assert_eq!(Page::all().len(), want.len());

    for (page, step, flow) in want {
        assert_eq!(run::step(page), step, "{}'s step", page.label());

        let mut bar = RunBar::default();
        let pressed = bar.press(page, Transport::default(), &project);
        assert_eq!(step_of(&pressed), step);
        // The flow each step belongs to, named the way the spec's links name it.
        assert!(
            step.flow().contains(flow),
            "{} should be {flow}: {}",
            step.label(),
            step.flow()
        );

        let run = bar.running.as_ref().expect("exactly one run");
        assert_eq!(run.step, step);
        assert!(!run.paused, "a run just started is not paused");
        assert!(run.log_expanded, "startRun expands the log (F0.5 S1)");
        assert_eq!(bar.status, "", "the run overwrites the status line itself");
    }

    // The words on screen for each step, which the log line and the status share.
    assert_eq!(Step::Prepare.label(), "Prepare");
    assert_eq!(Step::Suggest.label(), "Cut");
    assert_eq!(Step::Narrate.label(), "Narrate");
    assert_eq!(Step::Produce.label(), "Produce");
}

#[test]
fn f0_2_s4_a_refusal_reaches_the_status_line() {
    // S4: "its own refusals apply". The step refuses at its own start; the bar only carries the
    // sentence to the status line and ends the run.
    let project = without_footage();
    let mut bar = RunBar::default();
    bar.press(Page::Narrate, Transport::default(), &project);

    let lines = bar.finish(Finish::Refused {
        reason: cut::NO_CUT_YET.to_string(),
    });
    assert!(
        lines.iter().any(|line| line == cut::NO_CUT_YET),
        "the refusal must be said, not swallowed: {lines:?}"
    );
    assert_eq!(bar.status, cut::NO_CUT_YET);
    assert!(bar.running.is_none(), "a refused step is not still running");
    assert!(!run::announced_all_done(&lines));
}

#[test]
fn f0_2_s4_no_all_done_line_for_one_step() {
    // S4's last sentence: no "all done" line for one step — that sentence belongs to F0.4's chain,
    // which is the only thing that ran all four.
    let mut bar = bar_with_run(Step::Narrate);
    let lines = bar.finish(Finish::Done);
    assert_eq!(lines, vec!["Narrate done".to_string()]);
    assert!(!run::announced_all_done(&lines));
    assert!(bar.running.is_none(), "the run is over");

    // A bar with no run at all still answers: a finish that arrives late says "<empty> done" rather
    // than panicking, because the callback that posts it cannot know the run already ended.
    let mut empty = RunBar::default();
    let lines = empty.finish(Finish::Done);
    assert!(!run::announced_all_done(&lines), "{lines:?}");
    assert!(empty.running.is_none());
}

#[test]
fn f0_2_play_tooltip_and_pause_icon() {
    // §2's wording for the one button, in each of its states. ▶ becomes ⏸ rather than a second
    // button beside it, so these are the only tooltips the button ever has.
    let idle: Controls = run::controls(&None, None);
    assert_eq!(idle.icon, run::PLAY_ICON);
    assert_eq!(idle.tooltip, "Run this step \u{2014} or resume what is paused");
    assert_eq!(idle.tooltip, run::PLAY_TOOLTIP);
    assert!(!idle.stop_sensitive, "nothing to stop");

    let busy = running(Step::Prepare);
    let busy = Some(busy);
    let drawn = run::controls(&busy, None);
    assert_eq!(drawn.icon, run::PAUSE_ICON);
    assert_eq!(drawn.tooltip, "Pause");
    assert!(drawn.stop_sensitive);

    // A paused run has nothing to pause, so the button shows ▶ again — and ⏹ still has the run.
    let mut parked = running(Step::Suggest);
    parked.paused = true;
    let drawn = run::controls(&Some(parked), None);
    assert_eq!(drawn.icon, run::PLAY_ICON);
    assert_eq!(drawn.tooltip, run::PLAY_TOOLTIP);
    assert!(drawn.stop_sensitive, "a paused run still needs a way to end");

    // A preview playing with no run under way: the same button pauses it.
    let drawn = run::controls(
        &None,
        Some(Transport {
            playing: true,
            started: true,
        }),
    );
    assert_eq!(drawn.icon, run::PAUSE_ICON);
    assert_eq!(drawn.tooltip, run::PAUSE_TOOLTIP);
    assert!(drawn.stop_sensitive);

    // A preview parked part way through has nothing to pause but plenty to stop.
    let drawn = run::controls(
        &None,
        Some(Transport {
            playing: false,
            started: true,
        }),
    );
    assert_eq!(drawn.icon, run::PLAY_ICON);
    assert!(drawn.stop_sensitive);
}
