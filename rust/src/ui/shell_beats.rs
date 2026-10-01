//! F0.1's two timers and the run line that belongs on the status row.
//!
//! Three promises in §03-shell.md F0.1/§2 are decided by state in [`crate::shell`] and
//! [`crate::runqueue`] but need a caller to exist:
//!
//! * **the narration autosave** — "autosaved 400 ms after typing" ([`crate::shell::NARRATION_AUTOSAVE`]).
//!   [`crate::shell::Pending`] answers *when* a write may land (`due_at`, `writable`) and *whether* one is
//!   owed (`owe`); somebody has to stand the timer and ask. That is [`arm_autosave`], fired from the row box's
//!   own `changed` handler through [`crate::ui::narrate_text::note_typing`].
//! * **the bar's idle pulse** — "A model thinking with nothing countable pulses the bar" (§2, line 58), at
//!   [`crate::runqueue::PROGRESS_PULSE`]. [`arm_pulse`] stands that timer when ▶ starts a run and stops itself
//!   the moment there is a real fraction to show, which is when [`crate::ui::window`]'s `paint_progress`
//!   takes the needle back.
//! * **the run's words** — §2: the two tracks' lines, joined by `" · "`, go to the **status line**, not to
//!   the bar (which draws no text of its own). [`status_from_queue`] is the one place that puts them there,
//!   called from both painters that read the queue.
//!
//! It lives apart from `window.rs` because that file is past its size budget; `window.rs` only calls in.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4 as gtk;

use crate::runqueue::{self, Queue};
use crate::shell::NARRATION_AUTOSAVE;
use crate::ui::{narrate_page, narrate_text, window};

/// Read the live shell of the window that is showing, through the same newest-slot rule every other
/// window-level piece of `window.rs` uses.
fn live_shell() -> Option<Rc<RefCell<crate::shell::Shell>>> {
    window::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned())
}

thread_local! {
    /// The clock every beat in this file is measured against: real elapsed time, read from an `Instant`
    /// fixed when the UI first asks, plus whatever [`advance_clock_for_test`] added. The shell's `Pending`
    /// stores its deadline on this one clock and a check moves it forward, so writer and marker agree --
    /// which is what lets a test watch a beat elapse without sleeping for it. The shell's `Pending` stores absolute values of it, so a second
    /// clock would make every `writable` answer "not yet".
    static CLOCK: std::cell::OnceCell<Instant> = const { std::cell::OnceCell::new() };

    /// Added to the real clock by [`advance_clock_for_test`], and nothing else: the program never touches it.
    static SHIFT: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::ZERO) };

    /// Whether an autosave timer is already standing. Typing re-arms the beat rather than the timer:
    /// one timer asks the shell repeatedly until the beat is due, so a burst of keystrokes still costs
    /// one write.
    static AUTOSAVE_STANDING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// The pulse timer currently standing, if any — see [`arm_pulse`] / [`stop_pulse`].
    static PULSE: std::cell::Cell<Option<glib::SourceId>> = const { std::cell::Cell::new(None) };
}

/// Where this process's UI clock stands, as the absolute [`Duration`] the shell's `Pending` speaks in. The
/// deadline a write is owed by is stamped on this clock (`window::mark_narration_owed` -> `Pending::touched`),
/// so the marker that raises a beat and the timer that lands it read one clock -- an `Instant` per call would
/// make every `writable` answer "not yet".
fn elapsed() -> Duration {
    let base = CLOCK.with(|clock| {
        let instant = *clock.get_or_init(Instant::now);
        instant
    });
    base.elapsed() + SHIFT.with(|shift| shift.get())
}

/// Move this module's clock forward by `extra`, so a check can watch a beat come due without waiting out
/// the 400 ms for real. The program never calls it; the standing timer asks its own `writable` question
/// either way, on the deadline the shell already holds.
pub fn advance_clock_for_test(extra: Duration) {
    CLOCK.with(|clock| {
        clock.get_or_init(Instant::now);
    });
    SHIFT.with(|shift| shift.set(shift.get() + extra));
}

/// Whether a timer is worth standing at all: it is, exactly when the shell says a write is owed and names
/// the moment it may land. A shell with nothing pending needs no timer, which is what keeps an untouched
/// Narrate page from ticking forever.
pub fn needs_timer(owed: bool, due: Option<Duration>) -> bool {
    owed && due.is_some()
}

/// Stand the narration autosave for `window`, if the shell owes a write and no timer is already up.
///
/// The timer carries no deadline of its own -- the beat was stamped on the shell when the write became owed,
/// which is what lets a keystroke push it out without standing a second timer. So the loop here only asks
/// [`shell_beats`](crate::shell::Pending) twice: is anything still owed, and has its moment come.
pub fn arm_autosave(window: &adw::ApplicationWindow) {
    let Some(shell) = live_shell() else {
        return;
    };
    let owed = shell.borrow().narration_pending.due_at();
    if !needs_timer(shell.borrow().narration_pending.owe(), owed) {
        return;
    }
    if AUTOSAVE_STANDING.with(|standing| standing.replace(true)) {
        return;
    }
    let held = window.clone();
    glib::timeout_add_local(NARRATION_AUTOSAVE, move || {
        let Some(shell) = live_shell() else {
            AUTOSAVE_STANDING.with(|standing| standing.set(false));
            return glib::ControlFlow::Break;
        };
        if !shell.borrow().narration_pending.owe() {
            AUTOSAVE_STANDING.with(|standing| standing.set(false));
            return glib::ControlFlow::Break;
        }
        if !shell.borrow().narration_pending.writable(elapsed()) {
            // Not due yet: the beat was pushed out by typing, so walk on and ask again.
            return glib::ControlFlow::Continue;
        }
        // The beat is due: write through the door that reads every row box, then let the next keystroke
        // arm a fresh timer -- its own, on this module's clock, not a test's timer.
        flush_now(&held);
        AUTOSAVE_STANDING.with(|standing| standing.set(false));
        glib::ControlFlow::Break
    });
}

/// Land the write the shell owes, on the beat. Reads every row box first ([`narrate_text::flush_owed`]'s
/// rule) and says where it wrote, through the page's own channel: §F4.7 gives that sentence to the status
/// line, so "autosaved 400 ms after typing" is visible rather than silent.
fn flush_now(window: &adw::ApplicationWindow) {
    let said = narrate_text::flush_owed(window);
    narrate_page::refresh(window);
    if let Some(said) = said {
        // The page's `say` is private to `ui`; the status line is reachable through the same finder the
        // switch uses, so the beat's own sentence is visible rather than silent.
        if let Some(status) = window::find_status(glib::object::Cast::upcast_ref(window)) {
            status.set_text(&said);
        }
        window::log_line(&said);
    }
}

/// The live bar of the window that is showing, newest last — the same slot rule as [`live_shell`]. Held so
/// a test can start a run the way ▶ does (`runqueue::start_run`) instead of poking a private flag.
pub fn run_bar(_window: &adw::ApplicationWindow) -> Option<Rc<RefCell<crate::run::RunBar>>> {
    window::WINDOW_BAR.with(|bars| bars.borrow().last().cloned())
}

/// Where a run's words land: the status label, and whether the run owns that line right now.
/// The queue's line, ready for [`status_from_queue`]: the label to write, and whether a run owns it now.
/// One argument so every `paint_progress` call site gains a single line, not two.
pub struct StatusLine<'a> {
    pub status: &'a gtk::Label,
    pub owned: bool,
}

/// Put the queue's joined track line on the status row (§2: the bar draws no text of its own; the lines
/// belong to the status line, where a run overwrites whatever was last there).
///
/// `owned` says whether the run owns the line right now. It does not while a press is still being
/// answered: ▶'s handler writes its own sentence (F0.2's empty line on opening, F1.12's refusal) after
/// this runs, and ▶✂'s "no longer watched" lands before it, so an unowned line is left alone rather than
/// blanked or overwritten by what this press said.
pub fn status_from_queue(status: &gtk::Label, queue: &Queue, owned: bool) {
    if !owned {
        return;
    }
    let line = queue.text();
    if !line.is_empty() {
        status.set_text(&line);
    }
}

/// Pulse the bar while a run holds nothing countable. `true` when this beat should pulse: a run is on and
/// no track has reported a fraction yet, which is exactly "a model thinking".
///
/// Kept separate from [`arm_pulse`] because GTK's `ProgressBar` offers no way to read a pulse back — a test
/// cannot observe `pulse()`, so it observes this decision instead, and the timer runs on the same rule.
pub fn should_pulse(running: bool, fraction: f64) -> bool {
    running && fraction == 0.0
}

/// Stand the pulse timer for a run that just started on this bar. It ends itself as soon as the run is over
/// or something countable arrives, after which `paint_progress`'s `set_fraction` owns the needle again.
///
/// The id is remembered so [`stop_pulse`] can take the timer down when ⏹ ends the run: a pulse left standing
/// would outlive the run it belongs to and keep ticking on an idle bar.
pub fn arm_pulse(progress: &gtk::ProgressBar, bar: &Rc<RefCell<crate::run::RunBar>>, queue: &Rc<RefCell<Queue>>) {
    let (progress, bar, queue) = (progress.clone(), bar.clone(), queue.clone());
    let id = glib::timeout_add_local(runqueue::PROGRESS_PULSE, move || {
        let running = bar.borrow().running.is_some();
        let fraction = queue.borrow().fraction();
        if !should_pulse(running, fraction) {
            PULSE.with(|held| held.replace(None));
            return glib::ControlFlow::Break;
        }
        progress.pulse();
        glib::ControlFlow::Continue
    });
    PULSE.with(|held| held.replace(Some(id)));
}

/// Take down a standing pulse timer, if one is up. ⏹ calls it: the run is over, so nothing may keep the
/// bar moving after it. Returns whether a timer was actually there, which is what makes the call testable.
pub fn stop_pulse() -> bool {
    match PULSE.with(|held| held.take()) {
        Some(id) => {
            id.remove();
            true
        }
        None => false,
    }
}
