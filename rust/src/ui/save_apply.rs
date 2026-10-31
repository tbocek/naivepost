//! Applying a save the chooser just named (spec/03-shell.md F0.10 S3/S4).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;

use crate::project::Project;
use crate::save_as::{self, Saved};

/// The folder the window believes is open, if it knows: [`crate::startup::open_dir_if_set`], the one slot
/// every step reads. Read back by the tests that ask whether a rename or a refusal moved it.
pub fn open_dir() -> Option<PathBuf> {
    crate::startup::open_dir_if_set()
}

pub(crate) fn set_open_dir(dir: &Path) {
    crate::startup::set_open_dir(dir);
}

pub fn note_open_for_snapshot(dir: &Path) {
    set_open_dir(dir);
}

/// Where Save goes when it has no folder of its own yet: the working copy beside the launch's
/// directory. A fallback only -- every window `build_window` drew has already answered [`open_dir`].
fn launch_session_dir() -> PathBuf {
    crate::startup::open_dir()
}

/// The folder to save into: the one this window took on, or the launch's working copy if no window
/// ever answered. The door a caller uses, so the fallback is written once.
pub(crate) fn save_target() -> PathBuf {
    open_dir().unwrap_or_else(launch_session_dir)
}

/// S3 + S4: the save happened. Log what moved, move the memory to the new folder, say so.
fn apply(saved: &Saved, status: &gtk::Label) {
    for line in &saved.logs {
        crate::ui::window::log_line(line);
    }
    set_open_dir(&saved.path);
    crate::ui::window_line_step::rehome_window(&saved.path);
    status.set_text(&saved.status);
}

pub fn refused(reason: &str, status: &gtk::Label) {
    status.set_text(reason);
    crate::ui::window::log_line(reason);
}

pub fn save_named(
    session: &Rc<RefCell<Project>>,
    status: &gtk::Label,
    open: &Path,
    named: &str,
) -> Result<Saved, String> {
    match save_as::save_as(&session.borrow(), open, named) {
        Ok(saved) => {
            apply(&saved, status);
            crate::project_autosave::install(Rc::clone(session), saved.path.clone());
            Ok(saved)
        }
        Err(reason) => {
            refused(&reason, status);
            Err(reason)
        }
    }
}
