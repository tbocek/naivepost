//! F0.4 "I'm feeling lucky" — the press's own body: what each step of the chain actually does.
//!
//! [`crate::lucky`] is the state machine (what the press means); this module is the driver that feeds it
//! real outcomes, so every sentence in §S3-S6 is paid for by work that happened. It lives apart from
//! `window.rs` because that file is past its size budget; `wire_lucky` only hands its handles over and
//! calls [`press`].
//!
//! The four bodies are the same ones ▶ reaches for on each page, not a second version of them:
//! Prepare and Cut walk their stages through [`crate::run_stages::drive_page`] (the ▶ call at
//! `window.rs`'s `Started` branch), Narrate goes through [`window::press_narrate_run`] (F4.1) and
//! Produce through [`super::produce_page::press_produce_run`] (F5.1). A step that has nothing to do
//! declines, which §S4 carries past rather than waiting for.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use gtk4 as gtk;
use gtk4::prelude::{ButtonExt, Cast, WidgetExt};

use crate::exchanges;
use crate::hand_edit;
use crate::layout;
use crate::lucky::{self, Advance, StepOutcome};
use crate::project::Project;
use crate::run::{self, RunBar, Subprocesses};
use crate::run_stages::Drive;
use crate::runqueue::{self, Queue};
use crate::shell::Shell;
use crate::ui::window;

/// The CSS class the gears' BUTTON carries while a run computes. One const because the driver and
/// [`paint_run_bar`](crate::ui::window_bench::paint_run_bar) both set it and tests read it: two spellings
/// would let the button stop turning without any test noticing.
pub const RUNNING_CLASS: &str = "lucky-running";
/// The class that actually rotates: the gears IMAGE inside the button, not the button. §2 says the GEARS
/// turn, and animating the button node would spin the words "I'm feeling lucky" along with them.
pub const GEARS_TURNING_CLASS: &str = "lucky-gears-turning";

/// The gears image inside the lucky button. GTK 4's Button has no image property, so `window.rs` builds
/// the face as a box holding this node; §2 turns the GEARS, not the words beside them.
pub fn gears_of(button: &gtk::Button) -> Option<gtk::Image> {
    let face = button.child()?.downcast::<gtk::Box>().ok()?;
    face.first_child()
        .and_then(|child| child.downcast::<gtk::Image>().ok())
}

/// The words on the lucky button (§2 names the control by them): found by name, because a GTK 4 Button
/// with a child of its own carries no label of its own and the face box holds the gears beside them.
pub fn words_of(button: &gtk::Button) -> Option<String> {
    button
        .child()?
        .downcast::<gtk::Box>()
        .ok()?
        .last_child()
        .and_then(|child| child.downcast::<gtk::Label>().ok())
        .map(|label| label.text().to_string())
}

/// Put the gears on or off the turning state. The button keeps [`RUNNING_CLASS`] as the state marker every
/// door and test reads; the animation goes on its gears ([`GEARS_TURNING_CLASS`]) so only they move.
pub fn set_turning(button: &gtk::Button, turning: bool) {
    if turning {
        button.add_css_class(RUNNING_CLASS);
    } else {
        button.remove_css_class(RUNNING_CLASS);
    }
    if let Some(image) = gears_of(button) {
        if turning {
            image.add_css_class(GEARS_TURNING_CLASS);
        } else {
            image.remove_css_class(GEARS_TURNING_CLASS);
        }
    }
}

/// S2: the rule that makes the gears turn. GTK 4 animates a widget with a CSS `@keyframes` block plus
/// an `animation` shorthand (the library ships the same shape for its own spinner), and a class with no
/// rule behind it is inert: the driver has been adding [`RUNNING_CLASS`] since F0.4's first round and
/// nothing moved, because no such rule existed anywhere in `src/` or `assets/`. The rule now keys on
/// [`GEARS_TURNING_CLASS`], the gears image [`set_turning`] decorates, so the words beside them stay put.
///
/// A rotation rather than a pulse: the spec's word is "the gears start turning", and one full turn per
/// 1.6 s reads as machinery rather than as a blink. Installed once beside the lane's own classes, at
/// application priority, so a theme cannot override the animation away.
pub fn gears_css() -> String {
    format!(
        "@keyframes lucky-gears {{ from {{ transform: rotate(0deg); }} to {{ transform: rotate(360deg); }} }}\n\
         .{GEARS_TURNING_CLASS} {{ animation: lucky-gears 1.6s linear infinite; }}\n"
    )
}

/// Everything the driver needs, as the handles the shell already holds. Owned `Rc`s rather than borrows
/// because `connect_clicked` keeps its closure, and a closure cannot hold a borrow of the frame that made
/// it — `wire_lucky` clones what it is handed and moves the clones in.
#[derive(Clone)]
pub struct Handles {
    pub bar: Rc<RefCell<RunBar>>,
    pub shell: Rc<RefCell<Shell>>,
    pub stack: adw::ViewStack,
    pub guard: Rc<std::cell::Cell<bool>>,
    pub status: gtk::Label,
    pub progress: gtk::ProgressBar,
    pub log: gtk::Expander,
    pub log_view: gtk::TextView,
    pub queue: Rc<RefCell<Queue>>,
    pub exchange_log: Rc<RefCell<exchanges::RunLog>>,
    pub procs: Rc<RefCell<Subprocesses>>,
}

/// One press of the gears. `button` gets disabled and styled while the chain walks, and handed back
/// either way.
pub fn press(button: &gtk::Button, handles: &Handles, project: &Project) {
    // S1: busy comes from the same bar ▶ and ⏹ answer for, so the three buttons cannot disagree.
    let busy = handles.bar.borrow().running.is_some() || handles.bar.borrow().stop_flag;
    let (mut chain, opening) = match lucky::Chain::start(busy) {
        Ok(started) => started,
        Err(refusal) => {
            handles.status.set_text(refusal);
            return;
        }
    };
    window::log_line(&opening);

    // S4's two skip conditions, both read from live state rather than passed in: F4.8's Narration tick
    // for Narrate, and §F1.12's mtime question for Cut. `hand_edit::before_cut` is NOT the door here —
    // it remakes and writes `retakes.tsv` as a side effect, which is ▶'s remedy, whereas the chain only
    // skips and must change nothing. So the flag comes from `hand_edit::edited`, the pure half of the
    // same rule, over the same two files. No session folder means no text anyone could have edited.
    chain.set_narration_off(project.no_narration);
    // The pages carry the same switch as their own state (§08 §A hides Produce's Game-audio row behind it,
    // §F4.8 greys Narrate's controls), so the chain hands it to both before its first step lands on either
    // page: a lucky run that skipped Narrate because there is no narration must not find Produce still
    // drawing the game-volume slider.
    crate::ui::produce_page::note_narration_off(project.no_narration);
    chain.set_hand_edits(session_hand_edits());
    chain.set_hand_edits(session_hand_edits());

    // F0.5 S1: the chain is a run like any other, so it opens the bookkeeping the same way.
    let asked = window::live_project();
    runqueue::start_run(
        &mut handles.bar.borrow_mut(),
        &mut handles.queue.borrow_mut(),
        &mut handles.exchange_log.borrow_mut(),
        run::step(handles.shell.borrow().page),
        run::snapshot_sources(&asked),
    );
    // F0.5 S2 while the run works: this press's first step names its own job on the queue before the
    // first paint, so §2's joined track line is already on the status row when the page switches rather
    // than only when the stage's first task reports. Later steps are named at their hand-over below.
    // The phase is left alone on purpose: naming a job is not claiming work, and a step that pushes
    // nothing must leave the needle where it was (S4's end leaves an untouched bar at zero).
    handles.queue.borrow_mut().job(crate::runqueue::TRACK_FRAMES, "prepare", 0, 1);
    window::paint_progress(
        &handles.progress,
        &handles.log,
        &handles.log_view,
        &handles.queue.borrow(),
        crate::ui::shell_beats::StatusLine { status: &handles.status, owned: true },
        true,
    );
    // §2: with nothing countable yet the bar pulses until a track reports.
    crate::ui::shell_beats::arm_pulse(&handles.progress, &handles.bar, &handles.queue);

    // S2: the gears turn for the length of the chain, and the button cannot be pressed again meanwhile.
    button.set_sensitive(false);
    set_turning(button, true);
    // §S4's unload answers on its own thread, after the step that asked for it has been painted, so the
    // chain lends its status row to that line for its length (§2 gives the row to the run, and the
    // housekeeping line is the run's own news). Taken back before the end sentence is written.
    crate::ui::run_close::mark_chain_row(Some(&handles.status));

    let mut advance = chain.next_first();
    // Whether a step has been handed over and not yet closed: the first one opens the press's run, and
    // every later hand-over closes the previous one (§S4's "moves on"). `closed` says whether the end
    // line already did the last close, so the press never closes twice.
    let mut started_step = false;
    let mut closed = false;
    loop {
        match advance {
            Advance::Run { page, name } => {
                // §S4: the step that just finished closes its own bookkeeping and MOVES ON to this one,
                // so `end_run` is handed the page about to run -- the only place in the program where a
                // run has somewhere to move on to. The unload goes over after the move, because the next
                // step usually wants the same models back.
                if started_step {
                    crate::ui::run_close::close_chain_step(handles, Some(page));
                    // §2 at the hand-over: the queue was just reset for this step, so name its job before
                    // its work starts -- the line on the row is this step's, not the last one's.
                    handles
                        .queue
                        .borrow_mut()
                        .job(crate::runqueue::TRACK_FRAMES, &name.to_lowercase(), 0, 1);
                }
                started_step = true;
                // F0.1's synchronous move: the step's work belongs to the widgets of its own page.
                handles.guard.set(true);
                handles.stack.set_visible_child_name(page.label());
                handles.guard.set(false);
                handles.shell.borrow_mut().page = page;
                window::log_line(&lucky::step_line(name));
                let outcome = run_step(page, handles, &asked);
                // A ⏹ that landed inside the step outranks whatever the step reported: S5 ends on it.
                advance = if bar_wants_stop(handles) {
                    chain.next(StepOutcome::Stopped)
                } else {
                    chain.next(outcome)
                };
            }
            Advance::Skipped { line } => {
                window::log_line(&line);
                // A skip closes no running step; keep walking.
                advance = chain.next(StepOutcome::Skipped);
            }
            Advance::End { line, status: short } => {
                window::log_line(&line);
                handles.status.set_text(&short);
                // The end sentence goes into the log while the run still stands, so it is painted in the
                // expander the user can see; the close that follows is what takes the log down (S4).
                paint_chain_step(handles);
                close_run(handles);
                closed = true;
                break;
            }            Advance::Idle => break,
        }
        // ⏹ pressed between two steps: S5 ends the run here rather than starting the next one.
        if bar_wants_stop(handles) {
            if let Advance::End { line, status: short } = chain.next(StepOutcome::Stopped) {
                window::log_line(&line);
                handles.status.set_text(&short);
                close_run(handles);
            }
            break;
        }
    }

    // The chain is over either way: give the button back. The last step's bookkeeping is closed here
    // unless the end line already closed it (`close_run`), which is why `press` tracks the close.
    if !closed {
        paint_chain_step(handles);
        crate::ui::run_close::close_chain_end(handles);
    }
    button.set_sensitive(true);
    set_turning(button, false);
    // The chain's own end sentence owns the row from here; the housekeeping line stays in the log only.
    crate::ui::run_close::mark_chain_row(None);
}

/// Whether ⏹ has been pressed since the last checkpoint. Read fresh each time because the stop arrives
/// from another handler while this loop is inside a step.
fn bar_wants_stop(handles: &Handles) -> bool {
    handles.bar.borrow().stop_flag
}

/// Close the run's bookkeeping when the chain ends early (an end line landed mid-walk).
fn close_run(handles: &Handles) {
    crate::ui::run_close::close_chain_end(handles);
}

/// Paint the chain's widgets with the run still standing -- the log open and filled with everything the
/// chain has logged, the needle and the row as the queue has them. `run_close` owns the door because the
/// close that follows it is the other half of the same moment.
pub(crate) fn paint_chain_step(handles: &Handles) {
    crate::ui::run_close::paint_chain_step(handles);
}
/// Is this session's `final.txt` newer than its marks? Same two files and same comparison §F1.12 uses;
/// unreadable mtimes answer "no" because there is then no edit that could be lost.
///
/// `pub`: the question is the program's (only [`press`] asks it), and a test that wants to know what the
/// chain will decide re-asks THIS rather than reimplementing the mtime rule.
pub fn session_hand_edits() -> bool {
    let dir = crate::startup::session_dir(&std::env::current_dir().unwrap_or_default());
    let Ok(tree) = layout::Tree::new(&dir) else {
        return false;
    };
    hand_edit::edited(mtime(&tree.final_txt()), mtime(&tree.retakes_tsv()))
}

fn mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|meta| meta.modified().ok())
}

/// Run one page's step for real and say what became of it (§S4/S5). Timed here because the chain totals
/// its own durations from what it is told, and a step that ran is worth its seconds in the end line.
///
/// The four bodies are the ones ▶ reaches on each page, so a step never runs twice over: Prepare and Cut
/// walk their stages ([`crate::run_stages::plan`] gives them work), Narrate goes through F4.1's own
/// forwarder and Produce through F5.1's, both of which answer a refusal by returning the refusal.
fn run_step(page: crate::shell::Page, handles: &Handles, asked: &Project) -> StepOutcome {
    let started_at = Instant::now();
    match run::step(page) {
        run::Step::Prepare | run::Step::Suggest => stamp(
            outcome_of_drive(&walk_step(page, handles, asked)),
            started_at,
        ),
        run::Step::Narrate => stamp(
            outcome_of_sentence(&window::press_narrate_run(&window::main_window())),
            started_at,
        ),
        run::Step::Produce => stamp(
            outcome_of_sentence(&super::produce_page::press_produce_run(&window::main_window())),
            started_at,
        ),
    }
}

/// Put the measured duration on an outcome that earned one.
fn stamp(outcome: StepOutcome, started_at: Instant) -> StepOutcome {
    match outcome {
        StepOutcome::Ran { .. } => StepOutcome::Ran { seconds: whole_seconds(started_at) },
        other => other,
    }
}

/// Walk one page's stage plan under the run's bookkeeping and say what came of it in the log. Split out
/// so a test can watch the SAME walk the gears do, over a project of its own, without a window.
pub fn walk_step(
    page: crate::shell::Page,
    handles: &Handles,
    asked: &Project,
) -> Drive {
    let drive = crate::run_stages::drive_page(
        run::step(page),
        asked,
        &handles.queue,
        &handles.bar,
        &handles.procs,
    );
    // S5's failing step logs its own reason and the run walks on (§03's note on the prototype).
    crate::run_stages::say_outcome(&drive, window::log_line);
    drive
}

/// Map a walked stage plan onto the chain's vocabulary. An empty plan is a decline — the step had
/// nothing to do, so it earns no tally entry and the chain carries past it (§S4).
///
/// Both mappings are `pub` rather than private: they are the only place a step's result becomes chain
/// vocabulary, and a test that reimplemented them would pin its own copy instead of the program's.
pub fn outcome_of_drive(drive: &Drive) -> StepOutcome {
    match drive {
        Drive::Done { ran: 0, .. } => StepOutcome::Declined,
        Drive::Done { .. } => StepOutcome::Ran { seconds: 0 },
        Drive::Failed { .. } => StepOutcome::Failed,
        Drive::Stopped { .. } => StepOutcome::Stopped,
    }
}

/// Map a Narrate/Produce sentence onto the chain's vocabulary. Both bodies answer a refusal by returning
/// the refusal itself (`narrate_run::refuse` / `produce_flow::refuse`'s sentences, verbatim), so those
/// exact strings are what a decline looks like from here; anything else means work was done.
/// `pub` for the reason stated on [`outcome_of_drive`].
pub fn outcome_of_sentence(said: &str) -> StepOutcome {
    if declined_sentence(said) {
        return StepOutcome::Declined;
    }
    StepOutcome::Ran { seconds: 0 }
}

/// The sentences that mean "this step refused to start", compared exactly against the constants those
/// refusals come from (the narration-off one is built by `narrate_off`, not a const). A reworded refusal
/// fails a test here instead of silently counting as a finished step.
fn declined_sentence(said: &str) -> bool {
    let said = said.trim_start_matches('!').trim();
    [crate::cut::NO_CUT_YET, crate::narrate_run::NO_SESSION_TIMELINE, crate::run::PAUSING]
        .contains(&said)
        || crate::narrate_off::refuse_run(true).is_some_and(|reason| reason == said)
}

fn whole_seconds(started_at: Instant) -> u64 {
    started_at.elapsed().as_secs()
}
