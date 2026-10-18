//! §F0.5 / §7 — the window's log: the launch lines held until a window exists, the thread-local sink
//! every flow writes its `→→` notes and `‡‡‡` refusals into, and the readers a test asserts against.
//! Split out of `window.rs`, which is past its size budget; its doors are re-exported through `window`, so
//! no caller's path changed.

use std::cell::RefCell;

thread_local! {
    /// The window's log lines, standing in for F0.5's log expander: `>>> moved the output folder
    /// to …` and the `!!!` refusals have nowhere to be drawn until that row exists, and a test has
    /// to be able to see that they were said at all.
    static LOGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The launch's own log lines (§7's desktop registration), held until a window exists to read them.
static PENDING_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// §7's launch lines: `main` produces them while there is still no log to write into, so they wait in
/// [`PENDING_LOGS`] and the first [`build_window`] drains them through [`note_startup`].
pub fn note_startup(lines: &[String]) {
    if let Ok(mut pending) = PENDING_LOGS.lock() {
        pending.extend(lines.iter().cloned());
    }
    for line in PENDING_LOGS.lock().map(|mut p| p.drain(..).collect::<Vec<_>>()).unwrap_or_default() {
        log_line(&line);
    }
}

pub(crate) fn log_line(line: &str) {
    LOGS.with(|logs| logs.borrow_mut().push(line.to_string()));
}

/// Empty the log sinks. Only a test binary asks: nothing clears the shell's log while a session runs.
pub fn clear_logs_for_test() {
    LOGS.with(|logs| logs.borrow_mut().clear());
}

/// What the log holds right now, for the test that must say WHY a write did not land rather than only that
/// it did not: a failed `narrate/pitch.txt` write is invisible on disk but named in the log.
pub fn logs_for_test() -> Vec<String> {
    LOGS.with(|logs| logs.borrow().clone())
}

/// The one line a finished run leaves about its audio housekeeping. Called from the unload thread,
/// so it goes through the same log sink as everything else; `log_line` itself stays `pub(crate)`
/// because only this crate's own flows are allowed to write to the user's log.
pub fn note_unload(said: &str) {
    // Written straight to the shared buffer: this runs on the unload's own thread, whose thread-local
    // sink no reader ever sees.
    crate::ui::run_close::log(&format!("{}{said}", crate::ui::run_close::UNLOAD_PREFIX));
}

/// Every line the window has logged this session, oldest first.
///
/// The thread-local is THE record: every reader counts lines in it (a migration note checked as
/// `before + 1` counts exactly what it added). §S4's unload answers on its own thread and does NOT join
/// this record -- its news lives in [`crate::ui::run_close::SHARED_LOG`] and on the status row, because a
/// sentence written after the close returned would land after the run's last word (§2).
pub fn window_logs() -> Vec<String> {
    LOGS.with(|logs| logs.borrow().clone())
}
