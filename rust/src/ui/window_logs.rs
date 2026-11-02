//! §F0.5 / §7 — the window's log: the launch lines held until a window exists, the thread-local sink
//! every flow writes its `→→` notes and `‡‡‡` refusals into, and the readers a test asserts against.
//! Its doors are re-exported through `window`.

use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// The launch's own log lines (§7's desktop registration), held until a window exists to read them.
static PENDING_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// §7's launch lines: `main` produces them while there is still no log to write into, so they wait in
/// [`PENDING_LOGS`] and the first `build_window` drains them through [`note_startup`].
pub fn note_startup(lines: &[String]) {
    if let Ok(mut pending) = PENDING_LOGS.lock() {
        pending.extend(lines.iter().cloned());
    }
    for line in PENDING_LOGS
        .lock()
        .map(|mut p| p.drain(..).collect::<Vec<_>>())
        .unwrap_or_default()
    {
        log_line(&line);
    }
}

pub(crate) fn log_line(line: &str) {
    // A worker's line waits for the window thread: this thread-local is the window thread's record (§03-shell §1).
    if crate::ui::background::on_worker() {
        crate::ui::background::hold_line(line);
        return;
    }
    WINDOW_PAGE.with(|WindowPage { logs, .. }| logs.borrow_mut().push(line.to_string()));
}

/// Empty the log sinks. Only a test binary asks: nothing clears the shell's log while a session runs.
pub fn clear_logs_for_test() {
    WINDOW_PAGE.with(|WindowPage { logs, .. }| logs.borrow_mut().clear());
}

/// What the log holds right now, for a test that must say WHY a write did not land (a failed `narrate/pitch.txt` write is invisible on disk but named in the log).
pub fn logs_for_test() -> Vec<String> {
    WINDOW_PAGE.with(|WindowPage { logs, .. }| logs.borrow().clone())
}

/// The one line a finished run leaves about its audio housekeeping, from the unload thread via the same log sink; `log_line` stays `pub(crate)` so only this crate's flows write to the user's log.
pub fn note_unload(said: &str) {
    // Written straight to the shared buffer: the unload's own thread has a thread-local sink no reader sees.
    crate::ui::run_close::log(&format!("{}{said}", crate::ui::run_close::UNLOAD_PREFIX));
}

/// Every line the window has logged this session, oldest first.
///
/// The thread-local is THE record: every reader counts lines in it. §S4's unload answers on its own thread and does NOT join it: its news lives in `crate::ui::run_close::SHARED_LOG`,
/// since a sentence written after the close returned would land after the run's last word (§2).
pub fn window_logs() -> Vec<String> {
    WINDOW_PAGE.with(|WindowPage { logs, .. }| logs.borrow().clone())
}

/// A write a person's action asked for failed (§03-shell §7: never silent). Said in the log at once and put on the status line once the handler has returned (an idle), so its own sentence cannot paint over it. On a worker only the log line is said.
pub(crate) fn say_failure(line: &str) {
    log_line(line);
    if crate::ui::background::on_worker() {
        return;
    }
    let shown = line.trim_start_matches('!').trim_start().to_string();
    glib::idle_add_local_once(move || {
        if let Some(window) = crate::ui::window::built_window() {
            if let Some(status) =
                crate::ui::window::find_status(gtk4::prelude::Cast::upcast_ref(&window))
            {
                status.set_text(&shown);
            }
        }
    });
}

/// Write the cut a press changed, saying a failed write (§03-shell §7). A session with no project folder keeps the cut in memory only.
pub(crate) fn save_cut_or_say(cut_: &crate::cut::Cut) {
    if crate::layout::Tree::new(crate::startup::open_dir()).is_err() {
        return;
    }
    if let Err(why) = crate::cut_save::write_cut(cut_) {
        say_failure(&format!("!!! could not save the cut -- {why}"));
    }
}
