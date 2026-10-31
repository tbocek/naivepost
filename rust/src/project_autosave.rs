//! The working copy's timer and the close flush (spec/01-project-and-files.md §2 S5–S6).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use glib;

use crate::project::{self, Project};

/// §2 S5's wording, kept here so the status line and a test quote the same sentence.
pub const SAVED_STATUS: &str = "saved \u{2014} autosave";

/// A window's own handle on the live session, so a slot can be installed without threading one
/// through every place a project is opened or replaced.
pub type Session = Rc<RefCell<Project>>;

#[derive(Default)]
pub struct Ticker {
    session: Option<Session>,
    dir: PathBuf,
    autosave: project::Autosave,
}

/// What the last tick did, so a test can see the refusal as well as the write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tick {
    /// Nothing installed yet, or no folder to write into: a window that has not taken on a project.
    Nothing,
    Unchanged,
    Wrote,
    /// The write failed, with the reason §0 asks to be named rather than swallowed.
    Failed(String),
}

thread_local! {
    static SLOTS: RefCell<Vec<Rc<RefCell<Ticker>>>> = const { RefCell::new(Vec::new()) };
}

/// The newest slot, created empty on first use so `start` always has somewhere to put a session.
fn newest_slot() -> Rc<RefCell<Ticker>> {
    SLOTS.with(|slots| {
        let mut slots = slots.borrow_mut();
        if let Some(slot) = slots.last() {
            return Rc::clone(slot);
        }
        let slot = Rc::new(RefCell::new(Ticker::default()));
        slots.push(Rc::clone(&slot));
        slot
    })
}

pub fn slot_for_test(session: &Rc<RefCell<Project>>, dir: &Path) -> Rc<RefCell<Ticker>> {
    Rc::new(RefCell::new(Ticker {
        session: Some(Rc::clone(session)),
        dir: dir.to_path_buf(),
        autosave: project::Autosave::new(),
    }))
}

/// The working copy's folder: `session.naivepost` beside the working root, the same stand-in Rescan,
/// New, Save and the tab-switch flush use, so every flow agrees on where the session lives (§1).
pub fn session_dir() -> PathBuf {
    crate::startup::open_dir()
}

pub fn write_session_now(project: &Project, dir: &Path) -> bool {
    let slot = newest_slot();
    let mut slot = slot.borrow_mut();
    if slot.dir.as_os_str().is_empty() {
        slot.dir = dir.to_path_buf();
    }
    match slot.autosave.save_if_changed(project, dir) {
        Ok(wrote) => wrote,
        Err(err) => {
            crate::ui::window::log_line(&format!(
                "!!! could not write the pruned source list -- {err}"
            ));
            false
        }
    }
}

/// Begin ticking for this window's session. Called once per window, after its pages exist, so the
/// first tick lands `interval` seconds after the window came up rather than during its build.
pub fn start() {
    glib::timeout_add_local(project::AUTOSAVE_INTERVAL, || {
        tick(&newest_slot());
        glib::ControlFlow::Continue
    });
}

/// One tick, answering what it did: split out so a test can drive the rule without waiting two
/// seconds for a timer, and so the assertions land on the same `Ticker` the timer reads.
pub fn tick(slot: &Rc<RefCell<Ticker>>) -> Tick {
    let mut slot = slot.borrow_mut();
    let Some(session) = slot.session.clone() else {
        return Tick::Nothing;
    };
    if slot.dir.as_os_str().is_empty() {
        return Tick::Nothing;
    }
    // The handle is re-read every tick: an Open swaps the session inside the same `Rc`, so the slot
    // holds the `Rc`, never a clone of the value it pointed at.
    let held = session.borrow();
    // The folder is taken out before the write: `slot` is already borrowed mutably for `autosave`,
    // and a second `&slot.dir` inside the same call is a borrow the checker cannot allow.
    let dir = slot.dir.clone();
    let said = match slot.autosave.save_if_changed(&held, &dir) {
        Ok(true) => Some(SAVED_STATUS),
        Ok(false) => None,
        Err(err) => Some(&format!("autosave failed: {err}")[..]),
    };
    let outcome = match &said {
        Some(text) if *text == SAVED_STATUS => Tick::Wrote,
        Some(text) => Tick::Failed(text.to_string()),
        None => Tick::Unchanged,
    };
    if let Some(line) = said {
        crate::ui::window::log_line(line);
    }
    outcome
}

pub fn flush_on_close(project: &Project, dir: &Path) -> bool {
    if dir.as_os_str().is_empty() {
        return false;
    }
    match project::save(project, dir) {
        Ok(()) => true,
        Err(err) => {
            crate::ui::window::log_line(&format!("autosave on close failed: {err}"));
            false
        }
    }
}

/// The newest slot, the one `install` and every flow write go through — named `_for_test` because only
/// a test reads it back; the program reaches it through `tick`'s own timeout.
pub fn newest_slot_for_test() -> Rc<RefCell<Ticker>> {
    newest_slot()
}

pub fn install(session: Session, dir: PathBuf) {
    let slot = newest_slot();
    let mut slot = slot.borrow_mut();
    slot.session = Some(session);
    slot.dir = dir;
    slot.autosave = project::Autosave::new();
}
