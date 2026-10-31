//! The sentence a run ends with — `spec/09-llm-and-tools.md` §10's last paragraph.

use std::cell::Cell;

use crate::request_timing::{self, ServiceTally};
use crate::requests::{self, Request};

thread_local! {
    /// How many rows `requests.tsv` held when this thread's run started. Zero with no project open,
    /// which is also the honest answer for a run whose rows could not be filed anywhere.
    static RUN_MARK: Cell<usize> = const { Cell::new(0) };
}

/// Note that a run has begun, so its closing line counts only what this run sends.
pub fn note_run_begins() {
    RUN_MARK.with(|mark| mark.set(rows_written()));
}

/// The run's own rows: everything appended after [`note_run_begins`] marked the file.
pub fn run_rows() -> Option<Vec<Request>> {
    let tree = crate::web_request_log::session_tree()?;
    let all = requests::read(&tree).ok()?;
    let mark = RUN_MARK.with(|mark| mark.get());
    Some(all.into_iter().skip(mark).collect())
}

/// The closing sentence for this run, or `None` when there is nothing to say because there is no project.
pub fn closing_line() -> Option<String> {
    let rows = run_rows()?;
    let tallies: Vec<ServiceTally> = request_timing::tally(&rows);
    Some(request_timing::summary_line(&tallies))
}

/// Say the closing sentence on the app log. Every run's ending calls this.
pub fn log_closing_line() {
    let line = match closing_line() {
        Some(line) => line,
        // No project to read, or an unreadable one: §10 still wants the sentence, and `none` is the
        // truth about what this run can show.
        None => request_timing::summary_line(&[]),
    };
    crate::ui::window::log_line(&line);
}

/// Rows the file holds now (its header is not a row, which is [`requests::read`]'s rule, not ours).
fn rows_written() -> usize {
    match crate::web_request_log::session_tree() {
        Some(tree) => requests::read(&tree)
            .map(|rows| rows.len())
            .unwrap_or_default(),
        None => 0,
    }
}
