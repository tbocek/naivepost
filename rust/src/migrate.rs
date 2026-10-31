//! The two passes an old project folder goes through (§03-shell.md §8), as plain functions over a folder:
//! `step1..step6` renamed for their steps, then Prepare's work moved under `prepare/`. One line per move;
//! what cannot be moved stays and the log says so.

use std::fs;
use std::path::Path;

/// One folder that moved, by name relative to the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: String,
    pub to: String,
}

/// The log line one move earns (a silently renamed folder costs somebody an afternoon).
pub fn moved_line(moved: &Moved) -> String {
    format!(">>> moved {}/ to {}/", moved.from, moved.to)
}

/// What went wrong with one move, worded so the files' location is in it: the folder is left where it was.
pub fn failure_line(moved: &Moved, err: &str) -> String {
    format!(
        "!!! could not move {}/ to {}/: {err} -- the files are still under the old name",
        moved.from, moved.to
    )
}

/// Pass one: `step1`…`step6` become the folders named for their steps.
pub const PASS_ONE: [(&str, &str); 6] = [
    ("step1", "inputs"),
    ("step2", "understand"),
    ("step3", "cut"),
    ("step4", "narrate"),
    ("step5", "produce"),
    ("step6", "publish"),
];

/// Pass two: Prepare's work, which sat beside `understand/`, goes under `prepare/` (§1). The
/// `{describe,transcript}` pair comes first on purpose: pass one renames `step2` to `understand`, so this
/// pass empties that folder and its halves must be out before the leftovers check.
pub const PASS_TWO: [(&str, &str); 3] = [
    ("understand/describe", "prepare/describe"),
    ("understand/transcript", "prepare/transcript"),
    ("inputs", "prepare/inputs"),
];

/// One rename, logged either way. `Ok` means it happened; `Err` means the folder is untouched.
fn move_folder(dir: &Path, from: &str, to: &str) -> Result<Moved, (Moved, String)> {
    let moved = Moved {
        from: from.into(),
        to: to.into(),
    };
    let (source, target) = (dir.join(from), dir.join(to));
    if !source.is_dir() {
        return Err((moved, "there is nothing under the old name".into()));
    }
    // A name already in use is not ours to overwrite: pass one made `inputs`.
    if target.exists() {
        return Err((moved, format!("{} already exists", target.display())));
    }
    if let Some(parent) = target.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return Err((moved, err.to_string()));
        }
    }
    match fs::rename(&source, &target) {
        Ok(()) => Ok(moved),
        Err(err) => Err((moved, err.to_string())),
    }
}

/// A pass over a table of renames: what moved, and one line per failure. A name already carrying the new
/// spelling is skipped silently (this runs on every open); only a real io error or a target in use earns a
/// line. The table's order is the migration's: pass two moves `inputs/` before removing the emptied `understand/`.
fn pass(dir: &Path, pairs: &[(&str, &str)]) -> (Vec<Moved>, Vec<String>) {
    let mut moved = Vec::new();
    let mut failures = Vec::new();
    for (from, to) in pairs {
        match move_folder(dir, from, to) {
            Ok(one) => moved.push(one),
            Err((moved, err)) if dir.join(&moved.from).is_dir() => {
                failures.push(failure_line(&moved, &err))
            }
            Err(_) => {}
        }
    }
    (moved, failures)
}

pub fn pass_one(dir: &Path) -> (Vec<Moved>, Vec<String>) {
    pass(dir, &PASS_ONE)
}

/// `inputs/` and `understand/{describe,transcript}` → under `prepare/`, then the emptied `understand/` removed.
pub fn pass_two(dir: &Path) -> (Vec<Moved>, Vec<String>) {
    let (mut moved, failures) = pass(dir, &PASS_TWO);
    // A plain `remove_dir`, not recursive: anything still in there is not ours to throw away.
    if fs::remove_dir(dir.join("understand")).is_ok() {
        moved.push(Moved {
            from: "understand".into(),
            to: String::new(),
        });
    }
    (moved, failures)
}

/// The log lines a whole migration produced, one per move. `pass_two`'s removal of the emptied
/// `understand/` is not a move and gets no line.
pub fn log_lines(moved: &[Moved]) -> Vec<String> {
    moved
        .iter()
        .filter(|moved| !moved.to.is_empty())
        .map(moved_line)
        .collect()
}
