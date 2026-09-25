//! Opening a project the user picked (spec/03-shell.md F0.9).
//!
//! The chooser hands over a path; what happens next is a rule with an order, not a click handler:
//! the folder and its output folder are settled **first**, then the project is read and applied, so
//! every line that load produces belongs to the folder being opened rather than the one being left
//! behind. Two refusals sit on that path — an old single-file project, and anything that simply will
//! not open — and they are different messages for different reasons (see [`classify`]).
//!
//! Nothing here touches a widget. The window draws what these functions return and forwards the
//! picker's answer (spec/00-principles.md §5).

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
///
/// The double hyphen is the spec's literal form; the prototype adopted such a file into a folder,
/// which is exactly what the rewrite stopped doing, so the message says what happened rather than
/// papering over it.
pub fn single_file_refusal(file: &str) -> String {
    format!("!!! {file} is an old single-file project -- not supported")
}

/// S3: the log line for a source whose file has gone. Dropped, not deleted: the project keeps the
/// record of what was there, the session stops pretending it can read it.
pub fn dropped_line(file: &str) -> String {
    format!("!!! {file} is not there any more -- dropped from the session")
}

/// S1: the folder a picked path names. A path ending in `naivepost.json` opens the folder holding
/// it, because that is how a file manager hands a project over when the user picks the file inside.
/// Delegates to F0.6's rule so the two paths through which a project arrives agree.
pub fn folder_for(picked: &Path) -> PathBuf {
    startup::project_folder(picked)
}

/// What the picker's answer turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// S1: a folder to open — whether or not it holds a readable project yet is S3/S4's question.
    Openable(PathBuf),
    /// S2: a regular file that is not a project folder. Carries both halves of the refusal so the
    /// caller logs one and shows the other without re-deriving either.
    NotAFolder {
        target: PathBuf,
        log: String,
        status: &'static str,
    },
}

/// S1 → S2: sort the picked path into "open it" and "this is not a project".
///
/// The two refusals are deliberately different and must not be merged:
/// * A **regular file** that is not `<x>.naivepost/naivepost.json` is the pre-folder single-file
///   project — S2's message, status `not a project folder`. It is named as such because the user
///   has a file they used to open, and saying "old single-file project" tells them why this build
///   cannot take it.
/// * A **folder with no `naivepost.json`** is not an old project at all; it is a folder that will
///   not open, and it takes S4's generic failure (`CANNOT_OPEN`) via [`apply`]'s error rather than
///   S2's. Calling an empty directory an "old single-file project" would send the user looking for
///   a legacy format they never had.
pub fn classify(picked: &Path, is_file: impl Fn(&Path) -> bool) -> Choice {
    let folder = folder_for(picked);
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

/// Why an open did not happen. One shape for both refusals and for a failed read, so the shell's
/// error path is one branch: log `log`, show `status`.
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
///
/// `root` and `out` are filled before the project is read (S3's ordering rule): the output folder is
/// the project folder itself (§01 §1 — the folder *is* the output folder), so settling them first
/// means a later failure still leaves the shell pointing at the right place rather than mid-switch.
/// `project` carries everything the apply list names — sources, `interval`, `language`, prompts,
/// `context`, `policy`, the narration flag (`no_narration`), the reference flag
/// (`reference_sources`), the hints folded into the prompts by [`project::load_report`], `produce`
/// and `publish` — as one value the pages read, rather than field by field here.
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
///
/// `exists` answers "is this file still where the project says it is". It is a parameter rather than
/// a direct filesystem call so the drop rule can be tested against a fake as well as a real tree;
/// the UI passes `Path::exists`.
pub fn apply(dir: &Path, exists: impl Fn(&Path) -> bool) -> Result<Applied, OpenFailure> {
    // --- S3, part one: the paths, before anything is read -------------------
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

    // --- S3, part two: read it, and keep what the load had to say ----------
    let (mut project, mut lines) =
        project::load_report(&root).map_err(|err| OpenFailure::could_not_open(&err))?;

    // Sources whose file has gone are dropped from the session, one line each. Reported once per
    // missing file: the same source listed twice would print the same complaint twice.
    let mut dropped = Vec::new();
    let kept: Vec<_> = project
        .sources
        .drain(..)
        .filter(|source| {
            let present = exists(&layout_resolve(&root, &source.path));
            if !present {
                dropped.push(source.path.clone());
            }
            present
        })
        .collect();
    project.sources = kept;
    for path in &dropped {
        lines.push(dropped_line(path));
    }

    Ok(Applied {
        root,
        out,
        project,
        lines,
        dropped,
    })
}

/// Where a stored source path lives on disk. `project:` is the project folder, an absolute path is
/// itself, anything else hangs off the project too — the same rule [`crate::layout::resolve`]
/// applies, with the project folder standing in for the application root because opening a project
/// does not change which root the launch started under.
fn layout_resolve(project_dir: &Path, written: &str) -> PathBuf {
    match written.strip_prefix("project:") {
        Some(rel) => project_dir.join(rel),
        None if written.starts_with('/') => PathBuf::from(written),
        None => project_dir.join(written),
    }
}

/// S4: migrate old folder names and hand back the lines that say what moved.
///
/// Both passes run: `pass_one` renames the numbered `stepN` folders, `pass_two` folds `inputs/`
/// and `understand/{describe,transcript}` under `prepare/`. Failures are returned alongside rather
/// than swallowed — a folder that could not be renamed is worth one line even though the project
/// still opens.
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
    /// The session folder — the `PROJECT_<n>_ROOT` half.
    pub root: String,
    /// The project file inside it — the `PROJECT_<n>_FILE` half.
    pub file: String,
}

/// S4: record this project against this root, following the `PROJECT_<n>` convention
/// [`crate::settings`] numbers by folder order.
///
/// Data-level on purpose: writing `llm.conf` is [`crate::settings`]'s job (its writer quotes and
/// renders the whole store), and this module must not reach for `$HOME` — the tests drive it with a
/// map they own. The caller hands the pair to settings' existing writer unchanged.
///
/// Re-opening the same root replaces its entry rather than adding a second one: two entries for one
/// folder would make the remembered list disagree with itself.
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

/// S1 → S3: classify then apply, so the shell calls one thing. `finish` and `remember` stay outside
/// so each of those steps can be tested on its own — and so a caller that only wants to know whether
/// a path is openable never triggers a rename on disk.
pub fn open(picked: &Path, is_file: impl Fn(&Path) -> bool, exists: impl Fn(&Path) -> bool) -> Result<Applied, OpenFailure> {
    match classify(picked, is_file) {
        Choice::NotAFolder { log, status, .. } => Err(OpenFailure { log, status }),
        Choice::Openable(folder) => apply(&folder, exists),
    }
}
