//! The sentence a run ends with — `spec/09-llm-and-tools.md` §10's last paragraph.
//!
//! "At the end of every run the log says it in one line per service: `>>> requests: llm 57 in 2h 13m
//! on the wire, 4m waiting its turn; audio 38 in 6m 10s; web 3 in 12s`." [`crate::request_timing`]
//! already tallies rows and spells that sentence; what lived nowhere was the *when* — which rows belong
//! to the run that just finished. This module owns that mark and the two doors around it: one taken when
//! ▶ starts a run, one spoken when the run closes.
//!
//! Marking a row count rather than filtering on `requests.tsv`'s `run` column is the whole decision
//! here. That column names the run's `llm/` page, and a page only exists once a chat call files itself
//! (§7): a run that spent its time in audio.cpp — ASR, alignment, TTS — never opened one, so every row it
//! wrote carries an empty run name and a filter would report such a run as having sent nothing at all.
//! Counting from where the file stood when the run began costs nothing and covers every service equally.

use std::cell::Cell;

use crate::request_timing::{self, ServiceTally};
use crate::requests::{self, Request};

thread_local! {
    /// How many rows `requests.tsv` held when this thread's run started. Zero with no project open,
    /// which is also the honest answer for a run whose rows could not be filed anywhere.
    static RUN_MARK: Cell<usize> = const { Cell::new(0) };
}

/// Note that a run has begun, so its closing line counts only what this run sends.
///
/// Called from [`crate::runqueue::start_run`], the one call every ▶ and ↻ makes. Reading the file is a
/// cost paid once per run, against a log §10 keeps precisely so a re-run can be compared with the last.
pub fn note_run_begins() {
    RUN_MARK.with(|mark| mark.set(rows_written()));
}

/// The run's own rows: everything appended after [`note_run_begins`] marked the file.
///
/// `None` when there is no project folder to read — the same absence that stopped the rows being written
/// in the first place, and answered the same way §00 rule 8 asks: the work stands, the record is missing.
pub fn run_rows() -> Option<Vec<Request>> {
    let tree = crate::web_request_log::session_tree()?;
    let all = requests::read(&tree).ok()?;
    let mark = RUN_MARK.with(|mark| mark.get());
    Some(all.into_iter().skip(mark).collect())
}

/// The closing sentence for this run, or `None` when there is nothing to say because there is no project.
///
/// An empty tally is still a sentence — [`request_timing::summary_line`] spells it `>>> requests: none`,
/// and §10 wants the line at the end of EVERY run, including one that touched nothing outside.
pub fn closing_line() -> Option<String> {
    let rows = run_rows()?;
    let tallies: Vec<ServiceTally> = request_timing::tally(&rows);
    Some(request_timing::summary_line(&tallies))
}

/// Say the closing sentence on the app log. Every run's ending calls this.
///
/// A project that cannot be read still owes the line: `>>> requests: none` reads as "nothing went
/// outside", which is what a reader needs to hear even when the real cause is an unreadable file.
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
