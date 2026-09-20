//! Which project a launch opens (spec/03-shell.md F0.6).
//!
//! Four answers in a fixed order, decided here rather than in `main`: the file handed by the
//! desktop, else what this root last had open, else the working copy beside the root, else a blank
//! session at that same path. The window only draws what [`decide`] returns, so all four steps are
//! testable without a display (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use crate::{project, settings};

/// The working copy's name: the session nobody has named yet. A constant because S3 and S4 both
/// spell it, and they have to spell it the same or a launch opens one and saves into another
/// (prototype `workName`, gui/project.go:205).
pub const SESSION_NAME: &str = "session.naivepost";

/// `<root>/session.naivepost` — the working copy that belongs to this root.
pub fn session_dir(root: &Path) -> PathBuf {
    root.join(SESSION_NAME)
}

/// What a launch opened, one variant per step so the caller — and a test — can tell which question
/// answered rather than only what path came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// S1: the desktop handed a file over, which is somebody asking for THAT project.
    Desktop { path: PathBuf },
    /// S2: `llm.conf` remembers what this root last had open and it is still on disk.
    Remembered { path: PathBuf },
    /// S3: the working copy beside the root.
    Session { path: PathBuf },
    /// S4: nothing to open, so a blank session at that path. A decision rather than a file — this
    /// module writes nothing, and the folder appears when the session is first saved.
    Blank { path: PathBuf },
}

/// S2: the project `llm.conf` remembers for this root, if it is still on disk.
///
/// Absent, empty and unreadable all answer `None`, and none of them *forgets* the entry: this takes
/// `&Conf` rather than `&mut` because an external drive that is not mounted this morning has the
/// same shape as a deletion, and dropping the name then would make a temporary absence permanent.
pub fn last_project(conf: &settings::Conf, root: &Path) -> Option<PathBuf> {
    let remembered = conf.projects.get(&root.to_string_lossy().to_string())?;
    if remembered.is_empty() {
        return None;
    }
    let path = PathBuf::from(remembered);
    path.exists().then_some(path)
}

/// The project folder a path names. A double-clicked or handed `naivepost.json`
/// is somebody asking for the project, and a project is a folder (§03-shell.md
/// F0.9: "a path naming `naivepost.json` opens its folder"), so the file stands
/// for its parent; anything else already is the folder.
///
/// Single-file adoption — `<root>/project.json` at start, or a file handed over,
/// staged through `<name>.data` and `.adopting` — is prototype-only and dropped
/// in this rewrite: §8 lists it under what the rewrite does not do, so a handed
/// `project.json` stays the plain path it is.
pub fn project_folder(path: &Path) -> PathBuf {
    if path.file_name().and_then(|name| name.to_str()) == Some(project::PROJECT_FILE) {
        return path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf());
    }
    path.to_path_buf()
}

/// S1 → S2 → S3 → S4, in that precedence and no other.
///
/// The first handed file wins even when it is not there: a double-click is somebody asking for
/// THAT project, and quietly opening whatever was open last instead is worse than an opener that
/// says it could not read the one they named — so nothing here falls back to S2 on a missing path,
/// and reading it is the caller's failure to report (prototype `loadProjectFrom`). Only the first
/// file counts: one window, one project.
///
/// Which project this launch opened is not recorded here either; that is F0.9's `rememberProject`.
pub fn decide(root: &Path, conf: &settings::Conf, handed: &[PathBuf]) -> Opened {
    if let Some(path) = handed.first() {
        return Opened::Desktop { path: project_folder(path) };
    }

    if let Some(path) = last_project(conf, root) {
        return Opened::Remembered { path: project_folder(&path) };
    }

    let session = session_dir(root);
    if session.exists() {
        return Opened::Session { path: session };
    }

    Opened::Blank { path: session }
}
