//! F0.1's two timers and the run line for the status row (§03-shell.md F0.1/§2), the callers [`crate::shell`] and [`crate::runqueue`] need:
//! * narration autosave (400 ms after typing, [`crate::shell::NARRATION_AUTOSAVE`]): [`arm_autosave`] stands the timer and asks [`crate::shell::Pending`] (`due_at`, `writable`, `owe`), fired via [`crate::ui::narrate_text::note_typing`].
//! * the bar's idle pulse (§2) at [`crate::runqueue::PROGRESS_PULSE`]: [`arm_pulse`] runs until a real fraction appears and `paint_progress` takes the needle back.
//! * the run's words: both tracks' lines joined by `" · "` go to the status line; [`status_from_queue`] is the one place that puts them there.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4 as gtk;

use crate::runqueue::{self, Queue};
use crate::shell::NARRATION_AUTOSAVE;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};
use crate::ui::{narrate_page, narrate_text, window};

/// The live shell of the showing window, via the newest-slot rule.
fn live_shell() -> Option<Rc<RefCell<crate::shell::Shell>>> {
    WINDOW_PAGE.with(
        |WindowPage {
             window_shells: shells,
             ..
         }| shells.borrow().last().cloned(),
    )
}

/// Where the UI clock stands, as the absolute [`Duration`] the shell's `Pending` speaks in (see `window::mark_narration_owed`).
fn elapsed() -> Duration {
    let base = WINDOW_PAGE.with(|WindowPage { clock, .. }| {
        let instant = *clock.get_or_init(Instant::now);
        instant
    });
    base.elapsed() + WINDOW_PAGE.with(|WindowPage { shift, .. }| shift.get())
}

/// Move the clock forward by `extra` so a check can watch a beat come due without waiting 400 ms; never called by the program.
pub fn advance_clock_for_test(extra: Duration) {
    WINDOW_PAGE.with(|WindowPage { clock, .. }| {
        clock.get_or_init(Instant::now);
    });
    WINDOW_PAGE.with(|WindowPage { shift, .. }| shift.set(shift.get() + extra));
}

/// Whether a timer is worth standing: exactly when the shell owes a write and names when; keeps an untouched Narrate page from ticking.
pub fn needs_timer(owed: bool, due: Option<Duration>) -> bool {
    owed && due.is_some()
}

/// Stand the narration autosave for `window` if a write is owed and no timer is up. The deadline lives on the shell so a keystroke pushes it out without a second timer; the loop asks [`shell_beats`](crate::shell::Pending) whether anything is owed and whether its moment has come.
pub fn arm_autosave(window: &adw::ApplicationWindow) {
    let Some(shell) = live_shell() else {
        return;
    };
    let owed = shell.borrow().narration_pending.due_at();
    if !needs_timer(shell.borrow().narration_pending.owe(), owed) {
        return;
    }
    if WINDOW_PAGE.with(
        |WindowPage {
             autosave_standing: standing,
             ..
         }| standing.replace(true),
    ) {
        return;
    }
    let held = window.clone();
    glib::timeout_add_local(NARRATION_AUTOSAVE, move || {
        let Some(shell) = live_shell() else {
            WINDOW_PAGE.with(
                |WindowPage {
                     autosave_standing: standing,
                     ..
                 }| standing.set(false),
            );
            return glib::ControlFlow::Break;
        };
        if !shell.borrow().narration_pending.owe() {
            WINDOW_PAGE.with(
                |WindowPage {
                     autosave_standing: standing,
                     ..
                 }| standing.set(false),
            );
            return glib::ControlFlow::Break;
        }
        if !shell.borrow().narration_pending.writable(elapsed()) {
            return glib::ControlFlow::Continue;
        }
        // Due: write through the door that reads every row box; the next keystroke arms a fresh timer.
        flush_now(&held);
        WINDOW_PAGE.with(
            |WindowPage {
                 autosave_standing: standing,
                 ..
             }| standing.set(false),
        );
        glib::ControlFlow::Break
    });
}

/// Land the write the shell owes on the beat. Reads every row box first ([`narrate_text::flush_owed`]) and says where it wrote on the status line (§F4.7).
fn flush_now(window: &adw::ApplicationWindow) {
    let said = narrate_text::flush_owed(window);
    narrate_page::refresh(window);
    if let Some(said) = said {
        // The page's `say` is private to `ui`; the status line is reached through the switch's finder.
        if let Some(status) = window::find_status(glib::object::Cast::upcast_ref(window)) {
            status.set_text(&said);
        }
        window::log_line(&said);
    }
}

/// The live bar of the showing window, newest last, like [`live_shell`]; lets a test start a run as ▶ does (`runqueue::start_run`).
pub fn run_bar(_window: &adw::ApplicationWindow) -> Option<Rc<RefCell<crate::run::RunBar>>> {
    WINDOW_PAGE.with(
        |WindowPage {
             window_bar: bars, ..
         }| bars.borrow().last().cloned(),
    )
}

/// The queue's line for [`status_from_queue`]: the label to write and whether a run owns it now.
pub struct StatusLine<'a> {
    pub status: &'a gtk::Label,
    pub owned: bool,
}

/// Put the queue's joined track line on the status row (§2: the bar draws no text).
/// `owned` is false while a press is still being answered: ▶'s handler writes its own sentence after this runs and ▶✂'s "no longer watched" lands before it, so an unowned line is left alone.
pub fn status_from_queue(status: &gtk::Label, queue: &Queue, owned: bool) -> bool {
    if !owned {
        return false;
    }
    let line = queue.text();
    if line.is_empty() {
        return false;
    }
    status.set_text(&line);
    true
}

/// Whether this beat should pulse: a run is on and no track has a fraction yet ("a model thinking"). Separate from [`arm_pulse`] because `ProgressBar` has no pulse getter so a test observes this decision.
pub fn should_pulse(running: bool, fraction: f64) -> bool {
    running && fraction == 0.0
}

/// Stand the pulse timer for a run that just started; it ends itself when the run is over or something countable arrives. The id is kept so [`stop_pulse`] can take it down on ⏹.
pub fn arm_pulse(
    progress: &gtk::ProgressBar,
    bar: &Rc<RefCell<crate::run::RunBar>>,
    queue: &Rc<RefCell<Queue>>,
) {
    let (progress, bar, queue) = (progress.clone(), bar.clone(), queue.clone());
    let id = glib::timeout_add_local(runqueue::PROGRESS_PULSE, move || {
        let running = bar.borrow().running.is_some();
        let fraction = queue.borrow().fraction();
        if !should_pulse(running, fraction) {
            WINDOW_PAGE.with(|WindowPage { pulse: held, .. }| held.replace(None));
            return glib::ControlFlow::Break;
        }
        progress.pulse();
        glib::ControlFlow::Continue
    });
    WINDOW_PAGE.with(|WindowPage { pulse: held, .. }| held.replace(Some(id)));
}

/// Take down a standing pulse timer; ⏹ calls it. Returns whether one was there.
pub fn stop_pulse() -> bool {
    match WINDOW_PAGE.with(|WindowPage { pulse: held, .. }| held.take()) {
        Some(id) => {
            id.remove();
            true
        }
        None => false,
    }
}

/// Whether a pulse timer stands now. Test-only: reading the `Cell<Option<SourceId>>` needs ownership so this removes what it finds.
pub fn pulse_standing_for_test() -> bool {
    match WINDOW_PAGE.with(|WindowPage { pulse: held, .. }| held.take()) {
        Some(id) => {
            id.remove();
            true
        }
        None => false,
    }
}
