//! F1.1 ▶ Prepare — the step's own start flow, as plain data and rules.
//!
//! Everything ▶ does before a stage runs lives here: the two refusals, the save, the clear of a run
//! that died inside Describe, the opening log lines, the preflight question, the bar's phase
//! boundaries and the three ways a run can end. The UI only forwards the press and prints what these
//! return, which is what lets every rule be tested without a display, a server or ffmpeg.
//!
//! Deliberate boundary: NOTHING in this module shells out or opens a socket. S5 answers from
//! booleans handed in, and S6 only owns where each phase starts and stops on the bar — the work
//! inside those bounds is F1.2 (separation), F1.3 + F1.6 (ingest) and F1.7–F1.10 (understand),
//! whose rounds fill them. A runner here would be structure for its own sake until those land.

use std::fs;
use std::path::Path;

use crate::layout::Tree;
use crate::prepare;
use crate::project::{Project, Source};
use crate::sources;

/// S1: ▶ with an empty list. Short because it sits in the status line beside a page that is already
/// telling the person where the Add button is not.
pub const NO_SOURCES: &str = "add at least one source";

/// S1's log line for the same refusal. The status says what to do; the log says why the press was
/// answered at all, so a run that never started is still visible in the log afterwards.
pub fn no_sources_log() -> String {
    "!!! prepare refused -- the session has no sources".to_string()
}

/// S7: ⏹ landed mid-run. Not a failure and not a success: what finished stays on disk and the next
/// ▶ resumes off it, which is the whole resume rule (§00-principles).
pub const STOPPED: &str = "stopped \u{2014} finished work is kept";

/// S7 with no reason to give. Kept separate from [`failed_named`] because a failure that names
/// nothing must still say where to look rather than print an empty sentence.
pub const FAILED_BLANK: &str = "prepare failed \u{2014} see log";

/// S7's failure line. An empty reason falls back to [`FAILED_BLANK`] so the status is never blank
/// after a red run — a blank status reads as if nothing happened.
pub fn failed_status(reason: &str) -> String {
    if reason.trim().is_empty() {
        return FAILED_BLANK.to_string();
    }
    format!("prepare FAILED: {reason}")
}

/// S1: why ▶ refuses, if it refuses. `None` means the press may go through.
///
/// Two refusals and their order is the whole rule: no sources beats a name clash, because a clash
/// between zero files is not a fact about anything. Returns the log line and the status sentence
/// separately because §1 gives them different words — the log names the folder the two rows would
/// share, the status tells the person to rename one.
pub fn refuse(project: &Project) -> Option<(String, String)> {
    if project.sources.is_empty() {
        return Some((no_sources_log(), NO_SOURCES.to_string()));
    }
    let (a, b) = sources::clash(project)?;
    Some((sources::clash_log(&a, &b), sources::clash_status(&a, &b)))
}

/// What one source's stopped-inside-Describe state looked like, so the caller can report it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cleared {
    /// Files removed, named relative to the describe folder for the log.
    pub removed: Vec<String>,
    /// Files that would not unlink, each with its error.
    pub failures: Vec<(String, String)>,
}

impl Cleared {
    /// Nothing was stopped inside Describe, so there is nothing to say about it.
    pub fn quiet(&self) -> bool {
        self.removed.is_empty() && self.failures.is_empty()
    }
}

/// S3: the last run died inside Describe. Its `events.tsv`/`state.txt` describe a middle nobody can
/// trust, so both are removed and the description starts over — while the scaled frames stay, since
/// they are correct regardless of where the description stopped and re-extracting them costs a
/// minute per video for nothing.
///
/// Only those two files are touched. `.llmframes` and `frames_dir` are deliberately left alone: a
/// wider sweep here would delete exactly the thing the spec says to keep.
///
/// A failed unlink is NOT a failed run. The line says so and the run resumes from disk, because the
/// person's recording is fine and one unreadable scratch file is not a reason to stop everything.
pub fn clear_stopped_describe(tree: &Tree, project: &Project) -> Cleared {
    let mut cleared = Cleared::default();
    for source in &project.sources {
        let lane = lane(source);
        for (file, label) in [
            (tree.events_tsv(&lane), "events.tsv"),
            (tree.describe_state(&lane), "state.txt"),
        ] {
            if !file.exists() {
                continue;
            }
            match fs::remove_file(&file) {
                Ok(()) => cleared.removed.push(label.to_string()),
                Err(err) => cleared.failures.push((label.to_string(), err.to_string())),
            }
        }
    }
    cleared
}

/// S3's line when the clear worked: what was stopped, that it starts again, and what was kept.
pub fn restart_line(files: &[String]) -> String {
    format!(
        ">>> stopped last time \u{2014} describing from the start again: {} (scaled frames kept)",
        files.join(", ")
    )
}

/// S3's line when a file would not go away. The run continues off disk rather than failing.
pub fn clear_failed_line(failures: &[(String, String)]) -> String {
    let listed: Vec<String> = failures
        .iter()
        .map(|(file, err)| format!("{file}: {err}"))
        .collect();
    format!(
        ">>> could not clear the last run ({}) -- resuming it",
        listed.join(", ")
    )
}

/// Every log line S3 produces for this project, in order: the restart notice first, then any
/// failure. Empty when nothing was stopped inside Describe.
pub fn clear_lines(cleared: &Cleared) -> Vec<String> {
    let mut lines = Vec::new();
    if !cleared.removed.is_empty() {
        lines.push(restart_line(&cleared.removed));
    }
    if !cleared.failures.is_empty() {
        lines.push(clear_failed_line(&cleared.failures));
    }
    lines
}

/// S4: the run's opening lines, before any stage speaks.
///
/// Four things, in this order: how many inputs, each one by name, what they add up to on the bar,
/// and whether a session context goes with them. The context line is omitted entirely at zero
/// characters rather than printing "(0 characters)" — §S4's parenthetical is about sending the
/// context, and an empty context sends nothing.
///
/// `inputs` is what `prepare::count` measured; passing it in keeps this function free of disk reads
/// so a test can hand it a number instead of a tree full of files.
pub fn start_log(project: &Project, inputs: &prepare::Counts) -> Vec<String> {
    let mut lines = vec![format!(
        ">>> prepare: {} input file(s)",
        project.sources.len()
    )];
    for source in &project.sources {
        lines.push(format!(">>>   {}", base_name(&source.path)));
    }
    lines.push(format!(">>> prepare: {}", prepare::inputs_readout(inputs)));
    let chars = project.context.chars().count();
    if chars > 0 {
        lines.push(format!(
            ">>> prepare: sending the session context from Prepare ({chars} characters)"
        ));
    }
    lines
}

/// S6: the phases of the bar. Which stage fills each one is named in the comment beside it; this
/// module only owns the boundaries, so a retuned share is one edit in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// F1.2 — voice separation, only for rows carrying 🗣.
    Separate,
    /// F1.3 (speech) and F1.6 (frames), running in parallel.
    Ingest,
    /// F1.7 describe → F1.8 fix → F1.9/F1.10 marking.
    Understand,
}

/// Where a phase sits on the bar, as fractions of the whole.
///
/// Separation is zero-width when nobody asked for it, so the bar moves straight from 0 to ingest
/// instead of crawling through an empty tenth of the run. That is the difference between a bar
/// that lies about waiting and one that does not.
pub fn share(phase: Phase, separation_asked: bool) -> (f64, f64) {
    match phase {
        Phase::Separate if separation_asked => (0.0, 0.10),
        Phase::Separate => (0.0, 0.0),
        Phase::Ingest => (0.10, 0.30),
        Phase::Understand => (0.30, 1.0),
    }
}

/// Whether this project asks for separation at all: any row with 🗣 ticked that can still be split
/// (a half of a previous split has no voice left to take off).
pub fn separation_asked(project: &Project) -> bool {
    project
        .sources
        .iter()
        .any(|source| source.sepvoice && sources::separable(source))
}

/// S5: what the run needs standing before it starts. Answered from facts handed in, so the rule is
/// testable with no audio.cpp anywhere near it — the caller asks the servers and passes the results.
///
/// The order is the order a person would want told: the server itself first (nothing works without
/// it), then the two models every run needs, then separation, and only when it was asked for. It
/// names the FIRST missing thing rather than a list, because the first one is the only one they can
/// act on while the rest are still unknown.
pub fn preflight_error(
    audio_ok: bool,
    asr_served: bool,
    diar_served: bool,
    separation_asked: bool,
    separation_served: bool,
) -> Option<String> {
    if !audio_ok {
        return Some("audio.cpp is not answering".to_string());
    }
    if !asr_served {
        return Some("the ASR model is not served".to_string());
    }
    if !diar_served {
        return Some("the diarization model is not served".to_string());
    }
    if separation_asked && !separation_served {
        return Some("the voice separation model is not served".to_string());
    }
    None
}

/// S7: the success log. Three folders, because those are the three things a run of Prepare wrote,
/// and naming them is how the person finds the output without opening the file manager blind.
pub fn finished_log() -> Vec<String> {
    vec![
        ">>> prepare wrote:".to_string(),
        ">>>   prepare/inputs/".to_string(),
        ">>>   prepare/describe/".to_string(),
        ">>>   prepare/transcript/".to_string(),
    ]
}

/// S7: the status line beside it.
pub fn finished_status(files: usize) -> String {
    format!("prepared \u{2014} {files} files")
}

/// S2: save the project before anything runs, so a crash mid-run leaves a project that still knows
/// what it was pointed at. Returns the failure text rather than swallowing it: a project that could
/// not be saved is worth saying out loud before a long run starts.
pub fn save_project(project: &Project, dir: &Path) -> Result<(), String> {
    crate::project::save(project, dir)
}

/// The whole start sequence, S1 through S4, as one call the UI can make and a test can drive
/// without a window. Refusal short-circuits; everything else returns the lines to print in order.
///
/// `describe_cleared` comes back so the caller (and the tests) can see whether S3 fired without
/// re-walking the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// S1: the press was refused. Log line first, status second.
    Refused { log: String, status: String },
    /// S2 failed: the project would not save, so nothing was started.
    SaveFailed { error: String },
    /// Started. `lines` is what to print, in order: S3's notices then S4's opening.
    Started {
        lines: Vec<String>,
        cleared: Cleared,
    },
}

/// Run S1→S4. `tree` is the project's own folder, used for the S3 clear and the S4 count; `dir` is
/// where the project file is written.
pub fn begin(project: &Project, tree: &Tree) -> Start {
    // S1.
    if let Some((log, status)) = refuse(project) {
        return Start::Refused { log, status };
    }
    // S2: the project is saved before the first stage touches disk.
    if let Err(error) = save_project(project, tree.dir()) {
        return Start::SaveFailed { error };
    }
    // S3, then S4.
    let cleared = clear_stopped_describe(tree, project);
    let mut lines = clear_lines(&cleared);
    lines.extend(start_log(project, &prepare::count(tree, project)));
    Start::Started { lines, cleared }
}

/// The lane Describe files a source's work under — the file name minus its extension. Mirrors the
/// private `prepare::lane`; repeated rather than widened into a public API because changing one
/// without the other would put this clear in the wrong folder, which is exactly what the tests below
/// pin.
fn lane(source: &Source) -> String {
    let name = base_name(&source.path);
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

/// The file name for a log line: the path's last part. `sources::base_name` is private, so this
/// local copy keeps the log printing names rather than full paths.
fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
