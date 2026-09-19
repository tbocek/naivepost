//! Saving the project under a name of its own (spec/03-shell.md F0.10).
//!
//! One flow, two very different writes: the same name only rewrites `naivepost.json`, while a new
//! name moves the whole folder — the transcripts, frames and renders live beside the file and are
//! named by it, so a Save As that left them behind would point tonight's work at yesterday's
//! seconds. The decision is here and the chooser only picks a name (spec/00-principles.md §5).

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

/// Files under `dir`, however deep. Recursive because a project that has run keeps its frames in
/// `frames/<source>/…`: counting one level would call it empty and skip the move, which is the one
/// mistake this flow cannot make. An unreadable subtree counts as nothing rather than an error —
/// the rename below is what decides, and it does not care what could not be listed.
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
///
/// A rename rather than a copy because it is instant on one filesystem and cannot half-happen —
/// nobody presses Save expecting gigabytes of frames to be duplicated. It is also the whole check:
/// renaming onto a folder that already has work in it fails, onto an empty one succeeds, which is
/// exactly the answer wanted in both cases. When it will not go through — across filesystems, or
/// into a name in use — nothing is written and the files stay where they are with the log saying
/// where, because a save that silently moved work would be worse than one that did not happen. The
/// spec draws that as a refusal: no "project saved" over a folder still sitting somewhere else.
pub fn save_as(project: &Project, from: &Path, named: &str) -> Result<Saved, String> {
    let to = from.parent().unwrap_or(from).join(new_project::with_suffix(named));

    // The same name is the ordinary save: nothing moves, only the file changes.
    if to == from {
        project::save(project, &to)?;
        return Ok(Saved { path: to, status: SAVED.to_string(), logs: Vec::new() });
    }

    let mut logs = Vec::new();
    // A project saved before it was ever run has no outputs to move, which is the common case and
    // not worth a line about.
    let n = count_files(from);
    if n > 0 {
        if let Err(err) = fs::rename(from, &to) {
            return Err(format!(
                "!!! could not move the output folder to {}: {err} -- the {n} file(s) are still in {}",
                to.display(),
                from.display()
            ));
        }
        logs.push(format!(">>> moved the output folder to {}", to.display()));
    }

    project::save(project, &to)?;
    Ok(Saved { path: to, status: SAVED.to_string(), logs })
}
