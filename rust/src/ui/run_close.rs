//! F0.5 S4 — the one door a run closes through, for every way a run ends.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4 as gtk;
use gtk4::prelude::*;

use crate::run::{self, RunBar};
use crate::runqueue::{self, Queue};
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};
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

static SHARED_LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Add one line to the off-thread record. Only `window::note_unload` calls it: every other log door runs
/// on the main thread and goes straight into `window::log_line`'s own sink.
pub(crate) fn log(line: &str) {
    SHARED_LOG.lock().unwrap().push(line.to_string());
}

/// Close the run this window's bar holds, wherever its press left it: Narrate's and Produce's runs end on
/// their own worker's answer (§03-shell §1), long after the ▶ handler returned, and close through here.
pub fn paint_live() {
    let Some(main) = window::built_window() else {
        return;
    };
    let (Some(bar), Some(queue), Some((status, progress, log, view))) = (
        WINDOW_PAGE.with(
            |WindowPage {
                 window_bar: bars, ..
             }| bars.borrow().last().cloned(),
        ),
        WINDOW_PAGE.with(
            |WindowPage {
                 window_queue: slots,
                 ..
             }| slots.borrow().last().cloned(),
        ),
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
    WINDOW_PAGE
        .with(
            |WindowPage {
                 window_bar: bars, ..
             }| bars.borrow().last().cloned(),
        )
        .is_some_and(|bar| bar.borrow().running.is_some())
}

pub fn close_window_run() {
    let Some(bar) = WINDOW_PAGE.with(
        |WindowPage {
             window_bar: bars, ..
         }| bars.borrow().last().cloned(),
    ) else {
        return;
    };
    let main = window::main_window();
    let (Some(queue), Some(status)) = (
        WINDOW_PAGE.with(
            |WindowPage {
                 window_queue: slots,
                 ..
             }| slots.borrow().last().cloned(),
        ),
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
    close_step_run(&bar, &queue, &status, &progress, &log, &view);
}

/// Close the bookkeeping of the run this window's ▶ opened (§S4).
pub fn close_step_run(
    bar: &Rc<RefCell<RunBar>>,
    queue: &Rc<RefCell<Queue>>,
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
    crate::settings_probe::fire_unload(ended.audio_unload);
    // The pulse belongs to the run (§2: it pulses while a model thinks with nothing countable), so a
    // closed run takes it down rather than leaving a timer ticking on an idle bar.
    if !shell_beats::stop_pulse() {}
    paint_closed(bar, queue, status, progress, log, log_view);
    if !said.is_empty() && bar.borrow().running.is_none() {
        status.set_text(&said);
    }
    queue.borrow_mut().reset();
}

fn paint_closed(
    bar: &Rc<RefCell<RunBar>>,
    queue: &Rc<RefCell<Queue>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
) {
    let kept = log.is_expanded();
    let main = window::main_window();
    let (Some(play), Some(stop_)) = (window::play_button(&main), window::stop_button(&main)) else {
        return;
    };
    // A closed run leaves `run::controls` to pick the idle face and grey ⏹ -- one rule, so this cannot
    // disagree with what ▶'s own handler paints. The transport is the player's real one, read the same
    // way both handlers read it, so a started preview keeps ⏸ up after the run ends.
    let transport = window::preview_player().transport;
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
        bar.borrow()
            .running
            .as_ref()
            .map_or(kept, |run| run.log_expanded),
    );
}
