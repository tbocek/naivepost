//! F1.1 ▶ Prepare — the step's own start flow, as plain data and rules.

use std::fs;

use crate::layout::Tree;
use crate::prepare;
use crate::project::{Project, Source};
use crate::services;
use crate::sources;

/// S1: ▶ with an empty list. Short because it sits in the status line beside a page that is already
/// telling the person where the Add button is not.
pub const NO_SOURCES: &str = "add at least one source";

pub fn no_sources_log() -> String {
    "!!! prepare refused -- the session has no sources".to_string()
}

pub const STOPPED: &str = "stopped \u{2014} finished work is kept";

/// S7 with no reason to give. Kept separate from `failed_named` because a failure that names
/// nothing must still say where to look rather than print an empty sentence.
pub const FAILED_BLANK: &str = "prepare failed \u{2014} see log";

/// S7's failure line. An empty reason falls back to [`FAILED_BLANK`] so the status is never blank
/// after a red run.
pub fn failed_status(reason: &str) -> String {
    if reason.trim().is_empty() {
        return FAILED_BLANK.to_string();
    }
    format!("prepare FAILED: {reason}")
}

pub fn refuse(project: &Project) -> Option<(String, String)> {
    if project.sources.is_empty() {
        return Some((no_sources_log(), NO_SOURCES.to_string()));
    }
    let (a, b) = sources::clash(project)?;
    Some((sources::clash_log(&a, &b), sources::clash_status(&a, &b)))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cleared {
    pub removed: Vec<String>,
    pub failures: Vec<(String, String)>,
}

impl Cleared {
    pub fn quiet(&self) -> bool {
        self.removed.is_empty() && self.failures.is_empty()
    }
}

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

pub fn restart_line(files: &[String]) -> String {
    format!(
        ">>> stopped last time \u{2014} describing from the start again: {} (scaled frames kept)",
        files.join(", ")
    )
}

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
/// failure.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// F1.2 — voice separation, only for rows carrying 🗣.
    Separate,
    /// F1.3 (speech) and F1.6 (frames), running in parallel.
    Ingest,
    /// F1.7 describe → F1.8 fix → F1.9/F1.10 marking.
    Understand,
}

pub fn share(phase: Phase, separation_asked: bool) -> (f64, f64) {
    match phase {
        Phase::Separate if separation_asked => (0.0, 0.10),
        Phase::Separate => (0.0, 0.0),
        Phase::Ingest => (0.10, 0.30),
        Phase::Understand => (0.30, 1.0),
    }
}

pub fn separation_asked(project: &Project) -> bool {
    project
        .sources
        .iter()
        .any(|source| source.sepvoice && sources::separable(source))
}

/// S5: what the run needs standing before it starts.
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

fn serves(models: &[services::AudioModel], wanted_id: &str, task: &str) -> bool {
    models
        .iter()
        .any(|model| model.id == wanted_id && (model.task.is_empty() || model.task == task))
}

pub fn preflight(
    paths: &crate::settings::Paths,
    tree: &Option<Tree>,
    project: &Project,
) -> Option<String> {
    let conf = crate::settings::read(paths).unwrap_or_default();
    let endpoint = services::audio_endpoint(&conf, |name| std::env::var(name).ok());
    let healthy = crate::settings_probe::get(
        tree,
        services::Server::Audio,
        &endpoint,
        services::Kind::Health,
        "health",
        None,
    )
    .is_ok();
    if !healthy {
        return preflight_error(false, false, false, false, false);
    }
    let models = crate::audio_task::catalog(tree, paths, "Prepare").unwrap_or_default();
    let asked = separation_asked(project);
    preflight_error(
        true,
        serves(&models, services::asr_model(&conf), "asr"),
        serves(&models, services::diar_model(&conf), "diar"),
        asked,
        serves(&models, services::sep_model(&conf), "sep"),
    )
}

pub fn finished_log() -> Vec<String> {
    vec![
        ">>> prepare wrote:".to_string(),
        ">>>   prepare/inputs/".to_string(),
        ">>>   prepare/describe/".to_string(),
        ">>>   prepare/transcript/".to_string(),
    ]
}

pub fn finished_status(files: usize) -> String {
    format!("prepared \u{2014} {files} files")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    Prepared {
        files: usize,
    },
    Stopped,
    /// A stage failed, naming itself and the reason (§Failure-is-specific).
    Failed {
        reason: String,
    },
}

pub fn ending_of(drive: &crate::run_stages::Drive, tree: &Tree) -> Ending {
    match drive {
        crate::run_stages::Drive::Done { .. } => Ending::Prepared {
            files: crate::prepare::outputs_count(tree),
        },
        crate::run_stages::Drive::Stopped { .. } => Ending::Stopped,
        crate::run_stages::Drive::Failed { error, .. } => Ending::Failed {
            reason: error.clone(),
        },
    }
}

pub fn ending_lines(ending: &Ending) -> Vec<String> {
    match ending {
        Ending::Prepared { .. } => finished_log(),
        // The line `run_stages::say_outcome` already printed as the walk unwound, so a stop adds
        // nothing new to the log: §S7's sentence belongs to the run's last word, the status row.
        Ending::Stopped => vec![],
        Ending::Failed { reason } => vec![failed_status(reason)],
    }
}

pub fn ending_status(ending: &Ending) -> String {
    match ending {
        Ending::Prepared { files } => finished_status(*files),
        Ending::Stopped => STOPPED.to_string(),
        Ending::Failed { reason } => failed_status(reason),
    }
}

/// The whole start sequence, S1 through S4, as one call the UI can make and a test can drive
/// without a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    Refused {
        log: String,
        status: String,
    },
    SaveFailed {
        error: String,
    },
    Started {
        lines: Vec<String>,
        cleared: Cleared,
    },
}

pub fn begin(project: &Project, tree: &Tree) -> Start {
    if let Some((log, status)) = refuse(project) {
        return Start::Refused { log, status };
    }
    // S2: the project is saved before the first stage touches disk.
    if let Err(error) = crate::project::save(project, tree.dir()) {
        return Start::SaveFailed { error };
    }
    let armed = crate::stop_legs::take_describe_restart();
    let cleared = if armed {
        clear_stopped_describe(tree, project)
    } else {
        Cleared::default()
    };
    let mut lines = clear_lines(&cleared);
    // §3: the run stamps `inputs/meta.env` — §1's marker that the sources were read — before any
    // stage touches them.
    if let Some(refusal) = crate::prepare_data::stamp_meta(tree, project) {
        lines.push(refusal);
    }
    lines.extend(start_log(project, &prepare::count(tree, project)));
    Start::Started { lines, cleared }
}

fn lane(source: &Source) -> String {
    let name = base_name(&source.path);
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
