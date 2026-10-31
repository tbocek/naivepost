//! Opening a project the user picked (spec/03-shell.md F0.9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::project::{self, Project, PROJECT_FILE};
use crate::{migrate, startup};

/// S1: the chooser's title, §F0.9's own words.
pub const TITLE: &str = "Open a project";

/// S4: the status line when a project would not open. The em dash is the spec's.
pub const CANNOT_OPEN: &str = "could not open that project \u{2014} see log";

/// S2: the status line for a file from before project folders.
pub const NOT_A_PROJECT: &str = "not a project folder";

/// S2: the log line naming the file that cannot be adopted any more.
pub fn single_file_refusal(file: &str) -> String {
    format!("!!! {file} is an old single-file project -- not supported")
}

pub fn dropped_line(file: &str) -> String {
    format!("!!! {file} is not there any more -- dropped from the session")
}

/// What the picker's answer turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// S1: a folder to open — whether or not it holds a readable project yet is S3/S4's question.
    Openable(PathBuf),
    NotAFolder {
        target: PathBuf,
        log: String,
        status: &'static str,
    },
}

/// S1 → S2: sort the picked path into "open it" and "this is not a project".
pub fn classify(picked: &Path, is_file: impl Fn(&Path) -> bool) -> Choice {
    let folder = startup::project_folder(picked);
    if folder == picked && is_file(picked) {
        // Same path, and it is a file: nothing to descend into. That is the single-file case.
        return Choice::NotAFolder {
            target: picked.to_path_buf(),
            log: single_file_refusal(&picked.display().to_string()),
            status: NOT_A_PROJECT,
        };
    }
    Choice::Openable(folder)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenFailure {
    pub log: String,
    pub status: &'static str,
}

impl OpenFailure {
    /// S4: a read that failed carries the reason verbatim behind the `!!!` prefix.
    fn could_not_open(err: &str) -> Self {
        Self {
            log: format!("!!! {err}"),
            status: CANNOT_OPEN,
        }
    }
}

/// What opening a project put in place.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub root: PathBuf,
    pub out: PathBuf,
    pub project: Project,
    /// Everything worth logging, in the order it happened: the load report first, then the drops.
    pub lines: Vec<String>,
    /// The sources that were dropped, by the path the project stored.
    pub dropped: Vec<String>,
}

/// S3: set the project path and output folder first, then read and apply the project.
pub fn apply(dir: &Path, exists: impl Fn(&Path) -> bool) -> Result<Applied, OpenFailure> {
    // The project folder is also the output folder; nothing else stores where work goes.
    let root = dir.to_path_buf();
    let out = dir.to_path_buf();

    if !root.join(PROJECT_FILE).is_file() {
        // A folder with no project file: S4's generic refusal, not S2's (see `classify`).
        return Err(OpenFailure::could_not_open(&format!(
            "{}: no {PROJECT_FILE} in it",
            root.display()
        )));
    }

    let (mut project, mut lines) =
        project::load_report(&root).map_err(|err| OpenFailure::could_not_open(&err))?;

    // Sources whose file has gone are dropped from the session, one line each. Reported once per
    // missing file: the same source listed twice would print the same complaint twice.
    let mut dropped = Vec::new();
    let kept: Vec<_> = project
        .sources
        .drain(..)
        .filter(|source| {
            let present = source_present(&root, &source.path, &exists);
            if !present {
                dropped.push(source.path.clone());
            }
            present
        })
        .collect();
    project.sources = kept;
    // One line per missing FILE, so a source spelled twice in the project prints the complaint once.
    let mut said: Vec<&String> = Vec::new();
    for path in &dropped {
        if !said.contains(&path) {
            said.push(path);
            lines.push(dropped_line(path));
        }
    }
    crate::sources::clean_on_load(&mut project, &mut lines);
    // …and the same auto-fill for the load that dropped slot 1's holder instead of stripping a tag: the
    // rows above were already dropped as missing, so a strip may never have happened and slot 1 can still
    // be empty. One door again, so the seatings cannot disagree about who is the narrator.

    Ok(Applied {
        root,
        out,
        project,
        lines,
        dropped,
    })
}

fn layout_resolve(project_dir: &Path, written: &str) -> PathBuf {
    match written.strip_prefix("project:") {
        Some(rel) => project_dir.join(rel),
        None if written.starts_with('/') => PathBuf::from(written),
        None => project_dir.join(written),
    }
}

pub fn source_present(project_dir: &Path, written: &str, present: &impl Fn(&Path) -> bool) -> bool {
    present(&layout_resolve(project_dir, written))
}

/// S4: migrate old folder names and hand back the lines that say what moved.
pub fn finish(applied: &Applied) -> Migration {
    let (one, fail_one) = migrate::pass_one(&applied.root);
    let (two, fail_two) = migrate::pass_two(&applied.root);
    let mut moved = one;
    moved.extend(two);
    let mut failures = fail_one;
    failures.extend(fail_two);
    Migration {
        lines: migrate::log_lines(&moved),
        failures,
    }
}

/// What the folder-name migration did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Migration {
    /// `>>> moved step1/ to prepare/describe/` and friends.
    pub lines: Vec<String>,
    /// One line per folder that would not move, already prefixed for the log.
    pub failures: Vec<String>,
}

/// What this root should remember as its last-open project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    /// The folder the app was launched from (F0.6 S2 keys its memory by it) — the `PROJECT_<n>_ROOT` half.
    pub root: String,
    /// The project file inside it — the `PROJECT_<n>_FILE` half.
    pub file: String,
}

pub fn remember(
    projects: &mut BTreeMap<String, String>,
    root: &Path,
    project_file: &Path,
) -> Remembered {
    let entry = Remembered {
        root: root.display().to_string(),
        file: project_file.display().to_string(),
    };
    projects.insert(entry.root.clone(), entry.file.clone());
    entry
}

/// The project file inside an opened folder.
pub fn project_file_in(dir: &Path) -> PathBuf {
    dir.join(PROJECT_FILE)
}

pub fn open(
    picked: &Path,
    is_file: impl Fn(&Path) -> bool,
    exists: impl Fn(&Path) -> bool,
) -> Result<Applied, OpenFailure> {
    match classify(picked, is_file) {
        Choice::NotAFolder { log, status, .. } => Err(OpenFailure { log, status }),
        Choice::Openable(folder) => apply(&folder, exists),
    }
}
