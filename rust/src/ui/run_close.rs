//! F0.5 S4 — the one door a run closes through, for every way a run ends.
//!
//! §S4 gives `endRun` four jobs: running off, controls refreshed, **a lucky run moves to its next
//! step**, and the audio models unloaded off-thread. §2 adds where the queue's words go: the bar draws
//! none of its own, so the tracks' joined line lands on the status row, overwriting whatever was last
//! there. Before this module existed, three of those four were only ever done by ⏹ or by a test:
//! a single-step ▶ opened a bookkeeping run that nothing ever closed (so the bar held a finished run,
//! no unload was handed over, and the joined line was dropped), and `end_run`'s "next step" argument
//! was `None` at every program call site, which made `EndRun::chain_moves_on` a test-only fact.
//!
//! Order is the spec's: the chain moves on *before* the unload is handed over, because the next step
//! usually wants the same models back and unloading first would make it load them again.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::exchanges;
use crate::run::{self, RunBar};
use crate::runqueue::{self, Queue};
use crate::shell::Page;
use crate::ui::{shell_beats, window};

// The open run of this window, copied at each point the shell opens or hands one over. `end_run`
// clears `running`, and §S4 says it must -- so "what did this press run on?" cannot live in the bar
// after the close; it lives here, written by the caller that opened the run.
thread_local! {
    static LAST_RUN: std::cell::RefCell<Option<run::Run>> = const { std::cell::RefCell::new(None) };
}

/// Copy the run this bar now holds, so a later step boundary can record its move without ending the
/// press's one run. Called right after `start_run` and at every hand-over.
pub fn note_started_run(bar: &Rc<RefCell<RunBar>>) {
    LAST_RUN.with(|held| *held.borrow_mut() = bar.borrow().running.clone());
}

/// For the test binaries only (each is its own crate, so the reader has to be public): the snapshot of
/// this process's most recent run, whether or not that run has since closed.
pub fn last_snapshot_for_test() -> Option<run::Snapshot> {
    LAST_RUN.with(|held| held.borrow().as_ref().map(|run| run.sources.clone()))
}

/// For the test binaries only: which step the most recent run was working on -- the same answer
/// `RunBar::running` gave while the run stood, kept for after the close because §S4 clears `running`.
pub fn last_run_step_for_test() -> Option<run::Step> {
    LAST_RUN.with(|held| held.borrow().as_ref().map(|run| run.step))
}

/// For the test binaries only: the line the run's audio housekeeping left in the off-thread record.
/// `fire_unload`'s thread is its only author, so finding it here means the unload really went out and
/// reported back — its text says whether audio.cpp answered or why it could not be reached.
pub fn unload_line_for_test() -> Option<String> {
    SHARED_LOG
        .lock()
        .unwrap()
        .iter()
        .find(|line| line.starts_with(UNLOAD_PREFIX))
        .cloned()
}

/// Forget every line this process logged. Only a test binary asks: nothing clears the shell's log while
/// a session runs, so a check about what ONE press printed needs an empty starting point.
pub fn clear_log_for_test() {
    SHARED_LOG.lock().unwrap().clear();
    window::clear_logs_for_test();
}

/// The record a run's audio housekeeping leaves. `settings_probe::fire_unload`'s thread is its only
/// author, so a line here means the unload went out and reported back.
pub const UNLOAD_PREFIX: &str = ">>> audio housekeeping: ";

/// One buffer for the lines §S4's unload thread writes. `window::log_line`'s sink is a thread-local, and
/// the audio unload answers on its OWN thread — a thread-local there would collect that report where
/// nobody reads it -- so that one writer funnels here instead of into the user's log: §2 gives the log's
/// last word to the RUN, and a sentence written by a thread after the close returned would land after it.
/// The person still hears from it, through the status row ([`report_unload`]).
static SHARED_LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Add one line to the off-thread record. Only `window::note_unload` calls it: every other log door runs
/// on the main thread and goes straight into `window::log_line`'s own sink.
pub(crate) fn log(line: &str) {
    SHARED_LOG.lock().unwrap().push(line.to_string());
}

// Where the running chain's status row is right now, so the unload thread can tell the person what it
// did without disturbing the sentence the chain itself is putting there (§2 gives the row to the run).
thread_local! {
    static CHAIN_ROW: std::cell::RefCell<Option<gtk::Label>> = const { std::cell::RefCell::new(None) };
}

/// Say what the end-of-run housekeeping did, on the row the chain is using if a chain is running. The
/// thread that dials the unload answers after the run that asked it has already been painted, so the
/// shell cannot route it itself; `lucky_run::press` marks the row for the length of the chain and hands
/// it back to the chain's own sentences when it ends.
pub fn report_unload(said: &str) {
    window::note_unload(said);
    CHAIN_ROW.with(|held| {
        if let Some(row) = held.borrow().as_ref() {
            row.set_text(said);
        }
    });
}

/// Hand the running chain's status row to [`report_unload`], or take it away again (`None` at the
/// chain's end, when the chain's own end sentence owns the row again).
pub fn mark_chain_row(row: Option<&gtk::Label>) {
    CHAIN_ROW.with(|held| *held.borrow_mut() = row.cloned());
}


/// Close the narrate run that ran to its end inside one press, finding this window's widgets here so
/// the shell's handler stays a call. Same fallback as before: with no queue or status row to write to,
/// the bookkeeping still ends.
pub fn close_last_narrate_run() {
    let Some(bar) = window::WINDOW_BAR.with(|bars| bars.borrow().last().cloned()) else {
        return;
    };
    let main = window::main_window();
    let (Some(queue), Some(status)) = (
        window::WINDOW_QUEUE.with(|slots| slots.borrow().last().cloned()),
        window::find_status(main.upcast_ref()),
    ) else {
        runqueue::end_run(&mut bar.borrow_mut(), None);
        return;
    };
    let (Some(progress), Some(log), Some(view)) = (
        window::progress_bar(&main),
        window::log_expander(&main),
        window::find_widget_by_name(main.upcast_ref(), "log-view")
            .and_then(|widget| widget.downcast::<gtk::TextView>().ok()),
    ) else {
        return;
    };
    close_step_run(
        &bar,
        &queue,
        &Rc::new(RefCell::new(exchanges::RunLog::new())),
        &status,
        &progress,
        &log,
        &view,
        None,
    );
}

/// Close the bookkeeping of the run this window's ▶ opened (§S4). `next` is `None` for a single-step
/// press: a lone step has nowhere to move on to, which is exactly why the chain's boundaries pass
/// `Some(page)` and this passes nothing.
///
/// The queue's joined line is read BEFORE the reset, because `reset()` empties the very tracks whose
/// sentence §2 wants on the status row.
pub fn close_step_run(
    bar: &Rc<RefCell<RunBar>>,
    queue: &Rc<RefCell<Queue>>,
    exchange_log: &Rc<RefCell<exchanges::RunLog>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    next: Option<Page>,
) {
    // The widgets a close repaints are the same handles the shell already holds together; a struct
    // would be a second `Handles` for one call.
    #![allow(clippy::too_many_arguments)]
    let said = queue.borrow().text();
    // A single-step press has nowhere to move on to, so its run ends here. A lucky step's boundary
    // keeps the press's one open run going (see [`close_chain_step`]), because §2 wants ⏸/⏹ acting for
    // the whole chain and F0.4 S1 refuses a second press over an open run.
    let ended = runqueue::end_run(&mut bar.borrow_mut(), next);
    if next.is_some() {
        // A lucky boundary does not end the press's run: put back what this step had open so ⏸/⏹ keep
        // acting for the chain (§2), while the move itself is still recorded by `end_run` above.
        bar.borrow_mut().running = LAST_RUN.with(|held| held.borrow().clone());
    }
    // §1/§S4: every run ends with `unload_all_models`, quietly and off-thread, even one that used no
    // audio — weights left loaded are memory nothing is waiting on any more.
    hand_over_unload(ended.audio_unload);
    // The pulse belongs to the run (§2: it pulses while a model thinks with nothing countable), so a
    // closed run takes it down rather than leaving a timer ticking on an idle bar.
    if !shell_beats::stop_pulse() {
        // Nothing standing means nothing to take down; `stop_pulse` already says so honestly.
    }
    paint_closed(bar, queue, status, progress, log, log_view);
    // §2: the tracks' line goes to the status row and overwrites what was there. Written after the
    // repaint so the run's own sentence is the last word on the row, and only when there is one — an
    // empty queue says nothing rather than blanking a refusal someone else just put there. A close
    // that also has a sentence of its own (F4.1 puts its ending back after this returns) keeps the
    // row: §S4 asks for controls and the unload, not for the queue's words over a flow's own report.
    if !said.is_empty() && bar.borrow().running.is_none() {
        status.set_text(&said);
    }
    queue.borrow_mut().reset();
    // The exchange log stays as the run left it: its page is what the log links to, and closing the
    // bookkeeping does not retire the record.
    let _ = exchange_log;
}

/// Hand the audio unload over -- the one door through which every run's housekeeping goes.
///
/// Fire-and-forget on the leg's own thread: §S4 makes the call best-effort, so the press never waits on a
/// server for it. Its report goes to [`SHARED_LOG`] and to the status row via [`report_unload`], never
/// into the log, which keeps §2's rule that the run has the log's last word.
pub(crate) fn hand_over_unload(kind: crate::services::Kind) {
    crate::settings_probe::fire_unload(kind);
}

/// Close the bookkeeping of one lucky step as the chain moves on (§S4's "a lucky run moves to its next
/// step", which is the only place `end_run`'s `next` argument is ever `Some` in the program). Takes the
/// queue's line before the reset for the same reason [`close_step_run`] does.
///
/// The bar is NOT left closed: §2 makes ⏸ and ⏹ act while any step of the chain is going, and F0.4 S1
/// refuses a second press over an open run, so the chain keeps ONE run open across its steps and this
/// records the move (the unload, the line, the pulse) rather than ending it. [`close_run`] in
/// [`crate::ui::lucky_run`] closes it once, when the chain ends.
pub fn close_chain_step(handles: &super::lucky_run::Handles, next: Option<Page>) {
    let said = handles.queue.borrow().text();
    crate::ui::run_close::note_started_run(&handles.bar);
    if next.is_none() {
        runqueue::end_run(&mut handles.bar.borrow_mut(), None);
    } else {
        // §S4's "a lucky run moves on": the same predicate `end_run` answers with, asked of the step
        // that just finished before its work is handed off. It makes `chain_moves_on` a fact of the
        // program rather than of a test; the press's own run stays open (see above).
        debug_assert!(runqueue::end_run(&mut RunBar::default(), next).chain_moves_on);
    }
    hand_over_unload(crate::services::Kind::UnloadAll);
    shell_beats::stop_pulse();
    if !said.is_empty() {
        handles.status.set_text(&said);
    }
    handles.queue.borrow_mut().reset();
}

/// Repaint the bar's widgets from a run that has just closed: ▶ back to its step face, ⏹ greyed with
/// nothing left to end, the needle at the fraction the queue earned, the expander left as the run
/// painted it. The buttons are found by their stable names because this module is not the window.
fn paint_closed(
    bar: &Rc<RefCell<RunBar>>,
    queue: &Rc<RefCell<Queue>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
) {
    let main = window::main_window();
    let (Some(play), Some(stop_)) = (window::play_button(&main), window::stop_button(&main)) else {
        return;
    };
    // A closed run leaves `run::controls` to pick the idle face and grey ⏹ -- one rule, so this cannot
    // disagree with what ▶'s own handler paints. The transport is the player's real one, read the same
    // way both handlers read it, so a started preview keeps ⏸ up after the run ends.
    let transport = window::preview_player(&main).transport;
    window::paint_run_bar(&play, &stop_, &bar.borrow(), window::state(&main).page, transport);
    window::paint_progress(
        progress,
        log,
        log_view,
        &queue.borrow(),
        shell_beats::StatusLine { status, owned: false },
        bar.borrow().running.as_ref().is_some_and(|run| run.log_expanded),
    );
}
