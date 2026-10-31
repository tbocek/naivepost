//! Applying a project that ＋ New just created (spec/03-shell.md F0.8 S6).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4 as gtk;

use crate::new_project::Created;
use crate::project::Project;

/// Make the new project this window's project and say so.
pub fn apply(created: &Created, _session: &Rc<RefCell<Project>>, status: &gtk::Label) {
    let window = crate::ui::window_project::reopen_window_on(&created.project, &created.path);
    let status = crate::ui::window::find_status(gtk::prelude::Cast::upcast_ref(&window))
        .unwrap_or_else(|| status.clone());
    // S6's log line, verbatim from `Created`: "the session is empty; outputs on disk are untouched"
    // is the promise that nothing was deleted, and it has to be in the log, not only implied. Logged
    // into the NEW window, which is the one a person reads.
    crate::ui::window::log_line(&created.log);

    // S6 remembers the project for this root through the SAME door Open uses, so both paths write the
    // `PROJECT_<n>` pair the settings' writer owns. A conf that cannot be read costs the memory, never
    // the new project — the session is already switched, and saying so is the whole remedy.
    let file = crate::open_project::project_file_in(&created.path);
    let launch_root = std::env::current_dir().unwrap_or_default();
    REMEMBERED.with(|list| {
        match crate::settings::from_environment().map(|paths| {
            crate::machine_store::persist_project(
                &paths,
                &mut list.borrow_mut(),
                &launch_root,
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
