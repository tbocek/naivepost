//! The working copy's timer and the close flush (spec/01-project-and-files.md §2 S5–S6).
//!
//! §2's other two sentences belong here: the project is marshalled every 2 s
//! (`project::AUTOSAVE_INTERVAL`) and written only when the bytes differ from the last write —
//! which is `Autosave::save_if_changed`, whose `last_written` is what makes a tick a no-op — and the
//! project is flushed on window close. What was missing is the program path: nothing ever *started*
//! the tick, so an unclean exit lost everything since the last deliberate save, and closing flushed
//! only the play-line position and the narration text — never the project itself. This module holds
//! the rule; `ui/window.rs` only starts it, keeps its slot pointed at the live session, and calls
//! the flush.
//!
//! The window is the only thing that can answer "which project folder?" while a session is open, so
//! a slot here names both the live handle and the folder to write into, installed together.

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

/// One window's autosave state: the live session, where to write it, and the `Autosave` whose
/// `last_written` is §2's "only when the bytes differ" — carried between ticks, so a session that
/// has not changed since the last write costs nothing, which is exactly what the timer cannot do
/// through a fresh `Autosave` each time.
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
    /// The bytes on disk already match, so nothing was written (§2's "only when they differ").
    Unchanged,
    /// `naivepost.json` was written.
    Wrote,
    /// The write failed, with the reason §0 asks to be named rather than swallowed.
    Failed(String),
}

thread_local! {
    /// One slot per window built since this process started; the NEWEST is the live one, which is
    /// the same rule `ui::window`'s own per-window slots follow. A window that never installs one
    /// (a dialog, a test that builds no window) simply leaves the previous answer standing.
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

/// A slot pointed at a session and a folder, the same state `install` puts the newest slot in —
/// named for who uses it: a test drives `tick` through one instead of waiting two seconds on the
/// timer `start` installs. The program never calls it.
pub fn slot_for_test(session: &Rc<RefCell<Project>>, dir: &Path) -> Rc<RefCell<Ticker>> {
    Rc::new(RefCell::new(Ticker {
        session: Some(Rc::clone(session)),
        dir: dir.to_path_buf(),
        autosave: project::Autosave::new(),
    }))
}

/// The working copy's folder: `session.naivepost` beside the working root, the same stand-in Rescan,
/// New, Save and the tab-switch flush use, so every flow agrees on where the session lives (§1).
/// Always a path — §2's flush has to answer the question even before the folder exists, because
/// writing it is what creates it.
pub fn session_dir() -> PathBuf {
    crate::startup::open_dir()
}

/// A flow that just changed the session and wants it on disk NOW (F0.11's pruned list), written
/// through the newest slot's [`project::Autosave`] marker rather than beside it. The marker is what
/// knows the bytes on disk; writing with a fresh one every time left the timer's copy stale, so the
/// next tick (or the close flush) wrote the project file a second time after the flow had already
/// said it was done -- and a second flow sharing that folder then read a file someone else had
/// rewritten. Returns whether the file was written.
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
///
/// The source is deliberately never stopped: §2's autosave runs for as long as the window does, and
/// a `Continue(false)` would silently end the tick after one refused write.
pub fn start() {
    glib::timeout_add_local(project::AUTOSAVE_INTERVAL, || {
        tick(&newest_slot());
        glib::ControlFlow::Continue
    });
}

/// One tick, answering what it did: split out so a test can drive the rule without waiting two
/// seconds for a timer, and so the assertions land on the same `Ticker` the timer reads.
pub fn tick(slot: &Rc<RefCell<Ticker>>) -> Tick {
    // The timeout runs on the main context, which is the thread every widget handler runs on, so a
    // borrow here cannot meet a live one.
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

/// §2 S6: the close flush — `naivepost.json` goes to disk whatever the timer last wrote. §2 says
/// the flush must not be skippable, which is why this goes through `project::save` and not through an
/// `Autosave` that could call the bytes unchanged: a session whose first tick has not landed yet, or
/// one saved deliberately and then edited, must still reach the file on the way out.
///
/// A failure is logged where the log reaches and answered `false`, because the two records whose home
/// is NOT the project file — the play-line position and any half-typed narration row — are flushed by
/// `window.rs`'s close hook itself, through the doors their flows already use, and one refused write
/// must not lose the others (§0's failure rule: specific and local).
///
/// `project` and `dir` come in as arguments rather than off the newest slot: the close hook holds the
/// window it is closing, and a test can hand a project that is NOT what the slot holds and see which
/// of the two reaches the file.
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

/// Point the newest slot at a session and the folder it belongs to, and forget the last write: a
/// freshly opened or freshly made project has nothing on disk to compare against, so the next tick
/// writes it once whatever it holds. Called wherever a window takes on a project — the first build,
/// an Open, a New — so the timer always writes what the user is actually working on. Both go in
/// together for that reason: a slot with no folder would save into whatever the previous window chose.
pub fn install(session: Session, dir: PathBuf) {
    let slot = newest_slot();
    let mut slot = slot.borrow_mut();
    slot.session = Some(session);
    slot.dir = dir;
    slot.autosave = project::Autosave::new();
}
