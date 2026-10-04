//! Applying a save the chooser just named (spec/03-shell.md F0.10 S3/S4).
//!
//! [`crate::save_as`] decides everything about the write — same name or rename, what moved, what
//! refused and where the files still are. What it cannot decide is which folder the WINDOW believes
//! is open: Save renames that folder, so the memory has to move with it or the next autosave, the
//! next readout and the next Save aim at a path that no longer exists. Those window-side steps live
//! here, in the order the flowchart runs them, and `ui::window`'s Save handler only forwards the
//! chooser's answer through [`save_named`] — the same seam `ui::open_folder` is for Open, so a test
//! drives the real flow without answering a dialog it cannot answer.
//!
//! The order inside [`apply`] is the spec's arrow order: log the move, then move the memory, then
//! say "project saved". The memory goes before the sentence because the sentence is what a person
//! reads last, and everything that runs after it must already see the new folder.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4 as gtk;

use crate::project::Project;
use crate::save_as::{self, Saved};

thread_local! {
    /// The folder this window has open — what Save saves into and what a rename moves. Newest slot,
    /// the rule every per-window piece here follows. `None` until a window seeds it, which is also
    /// what makes [`open_dir`] an honest question rather than a guess from the working directory.
    /// Recomputing it at each door instead of keeping a slot would leave a Save that renamed the
    /// folder followed by an autosave that wrote into the path it just moved away from.
    static OPEN_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The folder the window believes is open, if it knows. Read back by the tests that ask whether a
/// rename or a refusal moved it; inside the program the only reader is [`save_target`] below.
pub fn open_dir() -> Option<PathBuf> {
    OPEN_DIR.with(|slot| slot.borrow().clone())
}

/// Set what the window believes is open. Crate-visible rather than public: only the doors that take
/// on a project (the first build, Open, a Save that renamed it) may answer this, so the slot cannot
/// be pointed somewhere no flow went.
pub(crate) fn set_open_dir(dir: &Path) {
    OPEN_DIR.with(|slot| *slot.borrow_mut() = Some(dir.to_path_buf()));
}

/// Take on a folder for the snapshot renderer only. `--snapshot` draws a window without going through
/// Open, so nothing has answered [`open_dir`] and an ending would count the working copy beside the
/// launch instead of the folder it is meant to show.
pub fn note_open_for_snapshot(dir: &Path) {
    set_open_dir(dir);
}

/// Where Save goes when it has no folder of its own yet: the working copy beside the launch's
/// directory. A fallback only -- every window `build_window` drew has already answered [`open_dir`].
fn launch_session_dir() -> PathBuf {
    crate::startup::session_dir(&std::env::current_dir().unwrap_or_default())
}

/// The folder to save into: the one this window took on, or the launch's working copy if no window
/// ever answered. The door a caller uses, so the fallback is written once.
pub(crate) fn save_target() -> PathBuf {
    open_dir().unwrap_or_else(launch_session_dir)
}

/// S3 + S4: the save happened. Log what moved, move the memory to the new folder, say so.
///
/// `Saved.path` is where the project file now IS, which for a same-name save is the folder it was
/// already in — so this is the one door for both endings of S3. Reached only through [`save_named`],
/// which is why nothing outside this module calls it by name.
fn apply(saved: &Saved, status: &gtk::Label) {
    for line in &saved.logs {
        crate::ui::window::log_line(line);
    }
    set_open_dir(&saved.path);
    status.set_text(&saved.status);
}

/// A save that did not happen. Both refusals leave through here — S1's "stop the run first" and S3's
/// `!!! could not move …` — because both are said on the status line AND logged: the `!!!` sentence
/// names where the files still are, which is news the log has to keep even after the status line
/// has moved on to the next press.
pub fn refused(reason: &str, status: &gtk::Label) {
    status.set_text(reason);
    crate::ui::window::log_line(reason);
}

/// The chooser's answer, or a test's stand-in for it: [`crate::save_as`]'s decision applied to the
/// window. `session` is the window's own live handle — the same `Rc` every page reads — so what is
/// written is tonight's session and not the copy the window was built from.
///
/// Public for the same reason `ui::open_folder` is: the folder chooser is the desktop's sheet and a
/// headless test never answers it, so the assertions have to land on real state rather than on a
/// dialog nobody pressed. Returns the outcome as well as saying it, so a caller (and a test) can tell
/// a save from a rename without re-reading the log.
pub fn save_named(
    session: &Rc<RefCell<Project>>,
    status: &gtk::Label,
    open: &Path,
    named: &str,
) -> Result<Saved, String> {
    match save_as::save_as(&session.borrow(), open, named) {
        Ok(saved) => {
            apply(&saved, status);
            // The autosave wrote into the folder a rename just moved away from. Re-installing the
            // SAME handle at its new home is what stops the next tick creating the old path again --
            // and it forgets the last write, which is right: nothing is on disk at the new path yet as
            // far as that `Autosave` knows. The handle has to be the window's own, so the tick keeps
            // writing tonight's session rather than a copy taken here.
            crate::project_autosave::install(Rc::clone(session), saved.path.clone());
            Ok(saved)
        }
        Err(reason) => {
            refused(&reason, status);
            Err(reason)
        }
    }
}
