//! Applying a project that ＋ New just created (spec/03-shell.md F0.8 S6).
//!
//! `new_project::create` writes the folder and answers with what to say; this is the half that makes
//! the window actually BE that project: the log line, the session swap, the remembered pair, the
//! status sentence, and the folder the source choosers should open at next. It lives here rather than
//! in `window.rs` because that file is past its size budget and none of this needs a widget — it takes
//! the same handles the Open flow takes and forwards to the same doors.
//!
//! The order matters: the log first, so the line belongs to the folder being left behind only if it
//! names it (it does — `Created::log` carries the NEW path), then the swap, then the memory, then the
//! status, which is what a person reads last.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;

use crate::new_project::Created;
use crate::project::Project;

/// Make the new project this window's session and say so.
///
/// `session` is the window's own live handle — the same `Rc` every page reads — so replacing its
/// contents IS the refresh: Prepare's rows show the empty session without being rebuilt here, exactly
/// as [`crate::ui::window::open_folder`] relies on for Open.
pub fn apply(created: &Created, session: &Rc<RefCell<Project>>, status: &gtk::Label) {
    // S6's log line, verbatim from `Created`: "the session is empty; outputs on disk are untouched"
    // is the promise that nothing was deleted, and it has to be in the log, not only implied.
    crate::ui::window::log_line(&created.log);

    // S6's "apply the blank project": the swap, before the status, so a reader who looks at the page
    // after reading the status finds the empty session already there rather than one still to come.
    *session.borrow_mut() = created.project.clone();

    // S6 remembers the project for this root through the SAME door Open uses, so both paths write the
    // `PROJECT_<n>` pair the settings' writer owns. A conf that cannot be read costs the memory, never
    // the new project — the session is already switched, and saying so is the whole remedy.
    let file = crate::open_project::project_file_in(&created.path);
    REMEMBERED.with(|list| {
        match crate::settings::from_environment().map(|paths| {
            crate::machine_store::persist_project(
                &paths,
                &mut list.borrow_mut(),
                &created.path,
                &file,
            )
        }) {
            Some(Ok(_)) | None => {}
            Some(Err(why)) => crate::ui::window::log_line(&format!(
                "!!! could not remember this project -- {why}"
            )),
        }
    });

    // S6's status: "new project — <base>".
    status.set_text(&created.status);

    // S6's chooser rule. `None` means the project went under the root, whose folders nobody chose, so
    // the previous default deliberately stands rather than being cleared.
    remember_chooser(created.chooser.clone());
}

/// Where the source choosers open next: the folder of the last project created outside the root, or
/// `None` when nothing was created outside it and the defaults already point right.
pub fn chooser_dir() -> Option<PathBuf> {
    CHOOSER.with(|slot| slot.borrow().clone())
}

/// Record (or deliberately leave) the chooser folder. Private: only [`apply`] decides it, so a test
/// cannot make the program claim a folder nobody chose.
fn remember_chooser(dir: Option<PathBuf>) {
    if let Some(dir) = dir {
        CHOOSER.with(|slot| *slot.borrow_mut() = Some(dir));
    }
}

thread_local! {
    /// What this launch has created, per root — the pairs `settings` writes as `PROJECT_<n>_ROOT` /
    /// `PROJECT_<n>_FILE`. Same shape and same newest-slot rule as Open's list: one window per binary
    /// in the tests, and the real app has one live session at a time.
    static REMEMBERED: std::cell::RefCell<std::collections::BTreeMap<String, String>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };

    /// The folder S6 says the choosers follow. Thread-local for the same reason every other piece of
    /// this window's state is: it belongs to the session, not to the process's arguments.
    static CHOOSER: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}
