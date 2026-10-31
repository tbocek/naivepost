//! Applying a save the chooser just named (spec/03-shell.md F0.10 S3/S4).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;

use crate::project::Project;
use crate::save_as::{self, Saved};

/// The folder to save into: the one this window took on, or the launch's working copy if no window
/// ever answered. The door a caller uses, so the fallback is written once.
pub(crate) fn save_target() -> PathBuf {
    crate::startup::open_dir_if_set().unwrap_or_else(crate::startup::open_dir)
}

/// S3 + S4: the save happened. Log what moved, move the memory to the new folder, say so.
fn apply(saved: &Saved, status: &gtk::Label) {
    for line in &saved.logs {
        crate::ui::window::log_line(line);
    }
    crate::startup::set_open_dir(&saved.path);
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
