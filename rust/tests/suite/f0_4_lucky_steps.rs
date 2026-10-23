//! F0.4 S4/S5 — what the gears' chain makes of a step, and why the gears have a turning rule at all.
//!
//! `lucky_chain.rs` pins the state machine's arithmetic; `lucky_button_widgets.rs` pins that a click
//! reaches it. What neither could pin while the driver answered every step with "declined" is the two
//! translations the driver actually performs: a walked stage plan → an outcome, and a refusal sentence →
//! an outcome. Those are what make §S4's "a declining step is skipped, not waited for" and §S5's "a
//! failing step walks on, a stop ends" true in the program rather than only in `lucky.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use naivepost::exchanges;
use naivepost::lucky::StepOutcome;
use naivepost::project::{Project, Source};
use naivepost::run::{self, RunBar, Subprocesses};
use naivepost::run_stages::Drive;
use naivepost::runqueue::Queue;
use naivepost::shell::Shell;
use naivepost::ui::lucky_run;

/// A stage plan where nothing was planned: the step had no work, which §S4 calls a decline — carried
/// past, no tally entry, and NOT an end (unlike a stop).
#[test]
fn f0_4_s4_an_empty_stage_plan_is_a_decline_not_a_run() {
    assert_eq!(
        outcome_of(&Drive::Done { ran: 0, skipped: 0 }),
        StepOutcome::Declined,
        "a step with nothing to do declines"
    );
    // Skipped-only counts the same way: nothing spawned, so nothing to tally.
    assert_eq!(
        outcome_of(&Drive::Done { ran: 0, skipped: 3 }),
        StepOutcome::Declined,
        "tasks already on disk are still no work this run did"
    );
}

/// A stage plan that walked real tasks is a `Ran`, whatever its seconds — the end line's tally needs an
/// entry for it, and a step that ran but got zero seconds is still tallied, not skipped.
#[test]
fn f0_4_s4_a_walked_plan_is_ran_and_tallied_under_its_page_name() {
    let outcome = outcome_of(&Drive::Done { ran: 2, skipped: 1 });
    match outcome {
        StepOutcome::Ran { .. } => {}
        other => panic!("two real tasks is a run, got {other:?}"),
    }
    // And the chain does tally it: hand it back and the end line names the step that ran.
    let mut chain = naivepost::lucky::Chain::start(false).expect("idle press opens").0;
    chain.set_narration_off(true);
    chain.set_hand_edits(true);
    // Prepare runs, everything else skips itself, so the tally holds exactly Prepare.
    let first = chain.next_first();
    assert_eq!(
        first,
        naivepost::lucky::Advance::Run {
            page: naivepost::shell::Page::Prepare,
            name: "Prepare"
        },
        "the chain hands Prepare over first"
    );
    let mut lines: Vec<String> = Vec::new();
    let mut advance = chain.next(outcome);
    // Cut and Narrate were told to skip themselves; Produce is not skippable, so it is the one step
    // still handed over after Prepare, and it gets declined rather than run.
    let mut ran_after = Vec::new();
    while !matches!(advance, naivepost::lucky::Advance::End { .. }) {
        match advance {
            naivepost::lucky::Advance::Run { name, .. } => {
                ran_after.push(name.to_string());
                advance = chain.next(StepOutcome::Declined);
            }
            naivepost::lucky::Advance::Skipped { line } => {
                lines.push(line);
                advance = chain.next(StepOutcome::Skipped);
            }
            naivepost::lucky::Advance::Idle => break,
            naivepost::lucky::Advance::End { .. } => {}
        }
    }
    assert_eq!(ran_after, vec!["Produce".to_string()], "only Cut and Narrate skip themselves");
    if let naivepost::lucky::Advance::End { line, status } = advance {
        lines.push(line);
        lines.push(status);
    }
    let end_line = lines.join(" | ");
    // Every step was reached (two skipped, one declined after this one), so S6's first form is the
    // right ending — and it names the ONE step that actually ran, which is what the tally is for.
    assert!(
        end_line.contains("all done in") && end_line.contains("Prepare 0s"),
        "the walked step is tallied by name in the end line: {end_line}"
    );
    assert!(
        !end_line.contains("Cut 0s") && !end_line.contains("Narrate 0s"),
        "a skipped step earns no tally entry: {end_line}"
    );
}

/// S5: a failed stage is `Failed`, which the chain walks ON past, and a stopped walk is `Stopped`,
/// which ends the press. The two must not collapse into each other: one is a step's own bad day, the
/// other is the person pressing ⏹.
#[test]
fn f0_4_s5_a_failed_walk_walks_on_where_a_stopped_one_ends() {
    assert_eq!(
        outcome_of(&Drive::Failed { stage: "speech", error: "audio.cpp: not on PATH".into() }),
        StepOutcome::Failed,
        "a failure names itself in the log but is not a halt"
    );
    assert_eq!(
        outcome_of(&Drive::Stopped { ran: 1, left: 2 }),
        StepOutcome::Stopped,
        "⏹ between two tasks ends the whole press"
    );

    // The walk-on / end difference, through the chain the driver feeds.
    let mut walking = naivepost::lucky::Chain::start(false).expect("opens").0;
    walking.next_first();
    let after_failure = walking.next(StepOutcome::Failed);
    assert!(
        matches!(after_failure, naivepost::lucky::Advance::Run { .. }),
        "a failed step must not end the run: {after_failure:?}"
    );

    let mut halting = naivepost::lucky::Chain::start(false).expect("opens").0;
    halting.next_first();
    let after_stop = halting.next(StepOutcome::Stopped);
    assert!(
        matches!(after_stop, naivepost::lucky::Advance::End { .. }),
        "a stop must end the run: {after_stop:?}"
    );
}

/// A Narrate or Produce refusal sentence is a decline; anything else those bodies return means work
/// happened. The refusals are compared against the constants they come from, so a reworded spec fails
/// HERE rather than turning a skipped step into a tallied one.
#[test]
fn f0_4_s4_a_refusal_sentence_from_either_body_is_a_decline() {
    for refusal in [
        naivepost::cut::NO_CUT_YET,
        naivepost::narrate_run::NO_SESSION_TIMELINE,
        naivepost::run::PAUSING,
    ] {
        assert_eq!(
            sentence_outcome(refusal),
            StepOutcome::Declined,
            "{refusal} is a step refusing to start"
        );
    }
    let narration_off = naivepost::narrate_off::refuse_run(true).expect("the off sentence exists");
    assert_eq!(
        sentence_outcome(&narration_off),
        StepOutcome::Declined,
        "F4.8's narration-off refusal declines too"
    );
    // And a real sentence — what F5.1 says when it did render something — is a run.
    assert_eq!(
        sentence_outcome("rendering \u{2014} mp4, h264, subtitles burned in"),
        StepOutcome::Ran { seconds: 0 },
        "anything that is not one of the refusals is work done"
    );
}

/// S2: the gears turn because a CSS rule says so. Without `@keyframes` the class the driver adds is
/// inert — GTK parses a class with no rule as nothing at all — which is how F0.4's first round passed
/// every test while the button sat still. Pinned here so the rule cannot go missing again, and pinned
/// against the SAME constant the driver adds, so the two cannot drift apart.
#[test]
fn f0_4_s2_the_gears_have_a_turning_rule() {
    let css = lucky_run::gears_css();
    assert!(
        css.contains("@keyframes"),
        "GTK animates only from a @keyframes block: {css}"
    );
    assert!(
        css.contains(lucky_run::GEARS_TURNING_CLASS),
        "the rule styles the class on the gears ({}): {css}",
        lucky_run::GEARS_TURNING_CLASS
    );
    assert!(
        css.contains("animation:") && css.contains("infinite"),
        "the animation must run for as long as the class is on: {css}"
    );
    assert!(
        css.contains("rotate"),
        "\"the gears start turning\" is a rotation: {css}"
    );
}

/// The driver's own walk, over a project with real work: `drive_page` spawns, so the stages are the
/// program's and only the tool is swapped at the argv seam (the same shape tests/suite/run_stages.rs uses).
/// This is what proves the gears reach `run_stages` at all — before this round they never called it.
#[test]
fn f0_4_s4_the_chain_walks_a_real_stage_plan() {
    let dir = scratch("walk");
    let marker = dir.join("prepare-ran");
    let project = project_with_recording();
    let handles = handles();
    naivepost::runqueue::start_run(
        &mut handles.bar.borrow_mut(),
        &mut handles.queue.borrow_mut(),
        &mut handles.exchange_log.borrow_mut(),
        run::Step::Prepare,
        run::snapshot_sources(&project),
    );
    // A stage whose one task touches a file: if the walk happens, the file appears.
    let stages = vec![naivepost::run_stages::Stage {
        track: naivepost::runqueue::TRACK_STT,
        name: "speech",
        phase: 1,
        of: 1,
        kind: "chunk",
        tasks: vec![naivepost::run_stages::Task {
            label: "transcribing take".to_string(),
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), format!("touch {}", marker.display())],
            already_done: false,
            session: None,
        }],
    }];
    let drive = naivepost::run_stages::drive_walk(
        &stages,
        &mut handles.queue.borrow_mut(),
        || false,
        || false,
        |_| {},
        &mut handles.procs.borrow_mut(),
    );
    assert!(matches!(drive, Drive::Done { ran: 1, .. }), "the task ran: {drive:?}");
    assert!(marker.is_file(), "the walk really spawned the task");
    // ...and the mapping the gears apply to that very result is a `Ran`.
    assert_eq!(outcome_of(&drive), StepOutcome::Ran { seconds: 0 });
    let _ = std::fs::remove_dir_all(&dir);
}

/// Map a `Drive` the way the driver does — by calling the driver's own mapping, so there is no second
/// copy here to drift from the program's.
fn outcome_of(drive: &Drive) -> StepOutcome {
    lucky_run::outcome_of_drive(drive)
}

fn sentence_outcome(said: &str) -> StepOutcome {
    lucky_run::outcome_of_sentence(said)
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f04-lucky-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

fn project_with_recording() -> Project {
    Project {
        sources: vec![Source { path: "take.mp4".to_string(), footage: true, narrator: 0, sepvoice: false, tracks: vec![] }],
        ..Default::default()
    }
}

/// The window-shaped handles the driver needs. GTK must be initialised first, which is what
/// [`gtk4::init`] does here without an application: the driver holds widgets, and only their types
/// matter for a stage walk — no window is shown and no main loop runs.
fn handles() -> lucky_run::Handles {
    let _ = gtk4::init();
    lucky_run::Handles {
        bar: Rc::new(RefCell::new(RunBar::default())),
        shell: Rc::new(RefCell::new(Shell::default())),
        stack: adw::ViewStack::new(),
        guard: Rc::new(std::cell::Cell::new(false)),
        status: gtk4::Label::new(None),
        progress: gtk4::ProgressBar::new(),
        log: gtk4::Expander::new(None),
        log_view: gtk4::TextView::new(),
        queue: Rc::new(RefCell::new(Queue::new())),
        exchange_log: Rc::new(RefCell::new(exchanges::RunLog::new())),
        procs: Rc::new(RefCell::new(Subprocesses::default())),
    }
}
