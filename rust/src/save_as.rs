//! Saving the project under a name of its own (spec/03-shell.md F0.10).

use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    new_project,
    project::{self, Project},
};

/// S1: naming the project names the folder a run is writing into, which is the one thing a save
/// must not move out from under it. The em dash is the spec's.
pub const RUN_REFUSAL: &str =
    "stop the run first \u{2014} saving under a new name moves the folder it is writing into";
/// S2's title, on the dialog that names the project.
pub const TITLE: &str = "Save the project";
/// S4's sentence, which both ways of finishing say.
pub const SAVED: &str = "project saved";

/// S1 refused, S2 otherwise — the caller opens the chooser and hands the answer to [`save_as`].
pub fn press(running: bool) -> Result<(), &'static str> {
    if running {
        return Err(RUN_REFUSAL);
    }
    Ok(())
}

fn count_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().fold(0, |n, entry| {
        let path = entry.path();
        if path.is_dir() {
            n + count_files(&path)
        } else {
            n + 1
        }
    })
}

/// What the save did: where the project lives now, the status line's sentence, and the log's lines.
#[derive(Debug, PartialEq, Eq)]
pub struct Saved {
    pub path: PathBuf,
    pub status: String,
    pub logs: Vec<String>,
}

/// S3 + S4: write it, moving the folder first when the name changed.
pub fn move_failure(from: &Path, to: &Path, files: usize, err: &str) -> String {
    format!(
        "!!! could not move the output folder to {}: {err} -- the {files} file(s) are still in {}",
        to.display(),
        from.display()
    )
}

pub fn save_as(project: &Project, from: &Path, named: &str) -> Result<Saved, String> {
    let to = from
        .parent()
        .unwrap_or(from)
        .join(new_project::with_suffix(named));

    // The same name is the ordinary save: nothing moves, only the file changes.
    if to == from {
        project::save(project, &to)?;
        return Ok(Saved {
            path: to,
            status: SAVED.to_string(),
            logs: Vec::new(),
        });
    }

    let mut logs = Vec::new();
    // A project saved before it was ever run has no outputs to move, which is the common case and
    // not worth a line about.
    let n = count_files(from);
    if n > 0 {
        if let Err(err) = fs::rename(from, &to) {
            return Err(move_failure(from, &to, n, &err.to_string()));
        }
        logs.push(format!(">>> moved the output folder to {}", to.display()));
    }

    project::save(project, &to)?;
    Ok(Saved {
        path: to,
        status: SAVED.to_string(),
        logs,
    })
}
