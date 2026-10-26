//! F0.5 S4 — the one door a run closes through, for every way a run ends.
//!
//! §S4 gives `endRun` three jobs: running off, controls refreshed, and the audio models unloaded
//! off-thread. §2 adds where the queue's words go: the bar draws
//! none of its own, so the tracks' joined line lands on the status row, overwriting whatever was last
//! there. Before this module existed, three of those four were only ever done by ⏹ or by a test:
//! a single-step ▶ opened a bookkeeping run that nothing ever closed (so the bar held a finished run,
//! no unload was handed over, and the joined line was dropped).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::exchanges;
use crate::run::{self, RunBar};
use crate::runqueue::{self, Queue};
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
/// The person reads it in the housekeeping record ([`unload_line_for_test`] is its test reader).
static SHARED_LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Add one line to the off-thread record. Only `window::note_unload` calls it: every other log door runs
/// on the main thread and goes straight into `window::log_line`'s own sink.
pub(crate) fn log(line: &str) {
    SHARED_LOG.lock().unwrap().push(line.to_string());
}

/// Close the narrate run that ran to its end inside one press, finding this window's widgets here so
/// the shell's handler stays a call. Same fallback as before: with no queue or status row to write to,
/// the bookkeeping still ends.
pub fn close_last_narrate_run() {
    close_window_run();
}

/// Close the run this window's bar holds, wherever its press left it: Narrate's and Produce's runs end on
/// their own worker's answer (§03-shell §1), long after the ▶ handler returned, and close through here.
/// Paint the run as it stands now: the bar's fraction and tooltip, the status line from the queue while a run is
/// open, and the log on screen when it is open. The worker poll ([`crate::ui::background`]) calls it whenever a
/// worker's lines or notes land, so a run walking on a worker is seen as it goes (§03-shell §1). The first
/// worker version painted only when the press ended: the log stayed empty and the bar still for the whole walk.
pub fn paint_live() {
    let Some(main) = window::built_window() else {
        return;
    };
    let (Some(bar), Some(queue), Some((status, progress, log, view))) = (
        window::WINDOW_BAR.with(|bars| bars.borrow().last().cloned()),
        window::WINDOW_QUEUE.with(|slots| slots.borrow().last().cloned()),
        live_widgets(&main),
    ) else {
        return;
    };
    let running = bar.borrow().running.is_some();
    crate::ui::window_bench::paint_progress(
        &progress,
        &log,
        &view,
        &queue.borrow(),
        crate::ui::shell_beats::StatusLine {
            status: &status,
            owned: running,
        },
        log.is_expanded(),
    );
}

/// The four widgets a live paint draws on, for this window. Searched for once per window and kept: the paint
/// runs at every worker note, and walking the whole window for the same four widgets each time held the window
/// thread for over 3 s on a real project's Cut page (hang dump, 2026-10-26).
type LiveWidgets = (gtk::Label, gtk::ProgressBar, gtk::Expander, gtk::TextView);

thread_local! {
    static LIVE: RefCell<Option<(glib::WeakRef<adw::ApplicationWindow>, LiveWidgets)>> =
        const { RefCell::new(None) };
    /// How many times the four were searched for, for the test that pins "once per window".
    static LIVE_SEARCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many times [`paint_live`] has searched the window for its widgets.
pub fn live_searches_for_test() -> usize {
    LIVE_SEARCHES.with(std::cell::Cell::get)
}

fn live_widgets(main: &adw::ApplicationWindow) -> Option<LiveWidgets> {
    let held = LIVE.with(|slot| {
        slot.borrow().as_ref().and_then(|(window, widgets)| {
            // Still this window's: a page that rebuilds the bottom band leaves the kept widgets detached, and a
            // paint on them would change nothing anyone sees. The log's text view is not asked: a folded expander
            // takes its body out of the window, and it is the expander's own child either way.
            let rooted = |widget: &gtk::Widget| {
                widget
                    .root()
                    .and_then(|root| root.downcast::<adw::ApplicationWindow>().ok())
                    .as_ref()
                    == Some(main)
            };
            (window.upgrade().as_ref() == Some(main)
                && rooted(widgets.0.upcast_ref())
                && rooted(widgets.1.upcast_ref())
                && rooted(widgets.2.upcast_ref()))
            .then(|| widgets.clone())
        })
    });
    if held.is_some() {
        return held;
    }
    LIVE_SEARCHES.with(|count| count.set(count.get() + 1));
    let found = (
        window::find_status(main.upcast_ref())?,
        window::progress_bar(main)?,
        window::log_expander(main)?,
        window::find_widget_by_name(main.upcast_ref(), "log-view")?
            .downcast::<gtk::TextView>()
            .ok()?,
    );
    LIVE.with(|slot| *slot.borrow_mut() = Some((main.downgrade(), found.clone())));
    Some(found)
}

/// Whether the window holds an open run right now.
pub fn run_open() -> bool {
    window::WINDOW_BAR
        .with(|bars| bars.borrow().last().cloned())
        .is_some_and(|bar| bar.borrow().running.is_some())
}

pub fn close_window_run() {
    let Some(bar) = window::WINDOW_BAR.with(|bars| bars.borrow().last().cloned()) else {
        return;
    };
    let main = window::main_window();
    let (Some(queue), Some(status)) = (
        window::WINDOW_QUEUE.with(|slots| slots.borrow().last().cloned()),
        window::find_status(main.upcast_ref()),
    ) else {
        runqueue::end_run(&mut bar.borrow_mut());
        return;
    };
    let (Some(progress), Some(log), Some(view)) = (
        window::progress_bar(&main),
        window::log_expander(&main),
        window::find_widget_by_name(main.upcast_ref(), "log-view")
            .and_then(|widget| widget.downcast::<gtk::TextView>().ok()),
    ) else {
        // No log widgets to repaint (a window built without them): the run still ends, or the bar would stay
        // open on a run nothing is doing.
        runqueue::end_run(&mut bar.borrow_mut());
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
    );
}

/// Close the bookkeeping of the run this window's ▶ opened (§S4).
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
) {
    // The widgets a close repaints are the same handles the shell already holds together; a struct
    // would be a second `Handles` for one call.
    #![allow(clippy::too_many_arguments)]
    let said = queue.borrow().text();
    // §10: the run says what it sent outside, in one line per service, as its last word in the log.
    // Said BEFORE anything is unwound so the sentence belongs to this run and not the next one.
    crate::request_run::log_closing_line();
    let ended = runqueue::end_run(&mut bar.borrow_mut());
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
/// server for it. Its report goes to [`SHARED_LOG`] through `window::note_unload`, never
/// into the log, which keeps §2's rule that the run has the log's last word.
pub(crate) fn hand_over_unload(kind: crate::services::Kind) {
    crate::settings_probe::fire_unload(kind);
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
    // A run's lines stay where the run put them: the log keeps whatever openness it had, because nothing
    // in §F0.5 or in the prototype closes it when a run ends (S1 opens it, and the user folds it), and
    // the body is repainted from the log sink so lines written after the last mid-run repaint -- the
    // audio housekeeping line above all -- are shown rather than dropped by a repaint of an empty buffer.
    let kept = log.is_expanded();
    let main = window::main_window();
    let (Some(play), Some(stop_)) = (window::play_button(&main), window::stop_button(&main)) else {
        return;
    };
    // A closed run leaves `run::controls` to pick the idle face and grey ⏹ -- one rule, so this cannot
    // disagree with what ▶'s own handler paints. The transport is the player's real one, read the same
    // way both handlers read it, so a started preview keeps ⏸ up after the run ends.
    let transport = window::preview_player(&main).transport;
    window::paint_run_bar(
        &play,
        &stop_,
        &bar.borrow(),
        window::state(&main).page,
        transport,
    );
    if kept {
        // The person's place kept: the newest line while they follow it, where they were otherwise.
        super::log_paint::repaint_lines(log_view, &window::window_logs());
    }
    window::paint_progress(
        progress,
        log,
        log_view,
        &queue.borrow(),
        shell_beats::StatusLine {
            status,
            owned: false,
        },
        // The openness kept above: the run is closed here, and reading "is the run's log open" off a closed
        // run said no and folded the log at every run's end, against this function's own rule.
        bar.borrow()
            .running
            .as_ref()
            .map_or(kept, |run| run.log_expanded),
    );
}
