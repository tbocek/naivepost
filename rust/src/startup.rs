//! Which project a launch opens (spec/03-shell.md F0.6).
//!
//! Four answers in a fixed order: the file handed by the desktop, else what this root last had open, else the
//! working copy beside the root, else a blank session at that path. [`decide`] returns it and the window only
//! draws it, so all four steps are testable without a display (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use crate::{project, settings};

/// The working copy's name: the session nobody has named yet. S3 and S4 both spell it, and must spell it
/// the same or a launch opens one and saves into another (prototype `workName`).
pub const SESSION_NAME: &str = "session.naivepost";

/// `<root>/session.naivepost`, the working copy that belongs to this root.
pub fn session_dir(root: &Path) -> PathBuf {
    root.join(SESSION_NAME)
}

/// The folder this process has open (03-shell F0.6/F0.9). Process-wide, not per GTK thread, since worker
/// threads read it too.
static OPEN_DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// THE project folder: what the launch opened (F0.6), or what Open, New or a renaming Save took on since.
/// Before anything took one on (a test that builds no window), the working copy beside the working directory.
///
/// Every step asks this. Asking `session_dir(current_dir())` in 66 places left every step on the working
/// copy while the header named another project.
pub fn open_dir() -> PathBuf {
    open_dir_if_set().unwrap_or_else(|| session_dir(&std::env::current_dir().unwrap_or_default()))
}

/// [`open_dir`] when something took a folder on, `None` before that.
pub fn open_dir_if_set() -> Option<PathBuf> {
    OPEN_DIR.read().map(|held| held.clone()).unwrap_or_default()
}

/// Take `dir` on as the open project. Only the doors that open a project call it: the window's build, Open,
/// New, and a Save that renamed the folder.
pub fn set_open_dir(dir: &Path) {
    if let Ok(mut held) = OPEN_DIR.write() {
        *held = Some(dir.to_path_buf());
    }
}

/// What a launch opened, one variant per step, so a test can tell which question answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// S1: the desktop handed a file over, which is somebody asking for THAT project.
    Desktop { path: PathBuf },
    /// S2: `llm.conf` remembers what this root last had open and it is still on disk.
    Remembered { path: PathBuf },
    /// S3: the working copy beside the root.
    Session { path: PathBuf },
    /// S4: nothing to open, so a blank session at that path (a decision; the folder appears on first save).
    Blank { path: PathBuf },
}

impl Opened {
    /// Which door answered, in the words the launch logs (`>>> opening ...`).
    pub fn kind(&self) -> &'static str {
        match self {
            Opened::Desktop { .. } => "handed by the desktop",
            Opened::Remembered { .. } => "remembered for this root",
            Opened::Session { .. } => "the session beside the root",
            Opened::Blank { .. } => "a blank session",
        }
    }

    /// The folder this door opened. S4's is the path a later save writes into.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Opened::Desktop { path }
            | Opened::Remembered { path }
            | Opened::Session { path }
            | Opened::Blank { path } => path,
        }
    }

    /// Whether this launch opened something that EXISTS, worth remembering for the next launch's S2. A blank
    /// session creates nothing (S4); a handed folder that is not there has nothing to remember.
    pub fn opened_folder(&self) -> Option<std::path::PathBuf> {
        match self {
            Opened::Blank { .. } => None,
            named if named.path().exists() => Some(named.path().to_path_buf()),
            _ => None,
        }
    }
}

/// S2: the project `llm.conf` remembers for this root, if it is still on disk.
///
/// Absent, empty and unreadable all answer `None` without *forgetting* the entry: this takes `&Conf`
/// because an unmounted drive looks like a deletion, and dropping the name would make the absence permanent.
pub fn last_project(conf: &settings::Conf, root: &Path) -> Option<PathBuf> {
    let remembered = conf.projects.get(&root.to_string_lossy().to_string())?;
    if remembered.is_empty() {
        return None;
    }
    let path = PathBuf::from(remembered);
    path.exists().then_some(path)
}

/// The project folder a path names: a handed `naivepost.json` stands for its parent (§03-shell.md F0.9),
/// anything else already is the folder. Single-file adoption (`project.json`, `.adopting`) is dropped in
/// this rewrite (§8), so a handed `project.json` stays the plain path it is.
pub fn project_folder(path: &Path) -> PathBuf {
    if path.file_name().and_then(|name| name.to_str()) == Some(project::PROJECT_FILE) {
        return path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf());
    }
    path.to_path_buf()
}

/// S1 → S2 → S3 → S4, in that precedence and no other.
///
/// The first handed file wins even when it is not there: a double-click asks for THAT project, so nothing
/// falls back to S2 on a missing path; reading it is the caller's failure to report (prototype
/// `loadProjectFrom`). Only the first file counts: one window, one project. Recording which project this
/// launch opened is F0.9's `rememberProject`.
pub fn decide(root: &Path, conf: &settings::Conf, handed: &[PathBuf]) -> Opened {
    if let Some(path) = handed.first() {
        return Opened::Desktop {
            path: project_folder(path),
        };
    }

    if let Some(path) = last_project(conf, root) {
        return Opened::Remembered {
            path: project_folder(&path),
        };
    }

    let session = session_dir(root);
    if session.exists() {
        return Opened::Session { path: session };
    }

    Opened::Blank { path: session }
}
