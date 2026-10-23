//! §03-shell.md §1 badge **15** — the Outputs row's folder button, and what its press does.
//!
//! §1 draws the button ("Outputs: folder button and count") and F0.1 S4 keeps it pointing at the page's
//! own output folder (`Shell::output_dir`, the directory `Shell::outputs` just counted) — but the press had
//! no handler, so the button was a picture of a control. Pressing it opens that folder the way a log link
//! opens its page ([`super::log_link::open`]: GTK's launcher first, `xdg-open` as §1's named fallback).
//!
//! The folder is not remembered at wire time: a switch repaints the tooltip to the new page's folder, so the
//! press re-reads the shell and finds the folder the row is naming NOW. That read is a door, not a copy of
//! the rule — [`folder_now`] asks the same `Shell` the painter just asked. A press with nothing behind it says
//! so on the log rather than doing nothing silently (failure is specific and local).
//!
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout::Tree;
use crate::shell::{Page, Shell};

/// The opening leg, injected so a test can see WHICH folder a press asks for without launching anything
/// (`log_link::open` hands a portal result back late or not at all, so it proves nothing about the path).
pub type Opener = std::rc::Rc<dyn Fn(std::path::PathBuf)>;

/// The page the row is currently naming, read back off the newest window's shell — the same slot every
/// other §1 door here reads. `None` when no window is up.
fn shell_now() -> Option<std::rc::Rc<std::cell::RefCell<Shell>>> {
    crate::ui::window::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned())
}

/// The folder the Outputs button should open right now: the shown page's own output directory, or `None`
/// when no session folder is held yet (a window built before any flow took one on).
pub fn folder_now() -> Option<std::path::PathBuf> {
    folder_in(&crate::ui::save_apply::open_dir()?)
}

/// The shown page's output directory inside `dir`, or `None` when `dir` is not a project folder. The one
/// place the page→folder ladder is read, so every door and press asks the same question.
fn folder_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let tree = Tree::new(dir).ok()?;
    let page = shell_now()
        .map(|shell| shell.borrow().page)
        .unwrap_or(Page::START);
    Some(
        Shell {
            page,
            ..Shell::default()
        }
        .output_dir(&tree),
    )
}

/// The folder the row names for `page` inside `dir` — the same ladder `window_bench::paint_readouts` drew
/// the row's tooltip from, so the press and the tooltip cannot disagree about which page they mean.
pub fn folder_for(dir: &std::path::Path, page: Page) -> Option<std::path::PathBuf> {
    let tree = Tree::new(dir).ok()?;
    Some(
        Shell {
            page,
            ..Shell::default()
        }
        .output_dir(&tree),
    )
}

/// Press: resolve the folder, then hand it to `open`. Nothing to open is a log line, not silence.
pub fn press(open: &Opener) {
    match folder_now() {
        Some(dir) => open(dir),
        None => super::window_logs::log_line("!!! no output folder to open yet -- nothing is open"),
    }
}

/// Wire §1 badge **15** through the default opener. Inactive until there is a session (the same rule
/// `window_bench::paint_readouts` draws), so a press on a window with no project does nothing at all.
pub fn wire(button: &gtk::Button) {
    wire_with(button, &folder_opener());
}

/// Wire with an opener a test can watch. Same call from the program; the seam only exists so the press is
/// checkable without a file manager.
pub fn wire_with(button: &gtk::Button, open: &Opener) {
    let open = std::rc::Rc::clone(open);
    let weak = button.clone().downgrade();
    button.connect_clicked(move |_| {
        let Some(button) = glib::object::WeakRef::upgrade(&weak) else {
            return;
        };
        let Some(button) = button.downcast_ref::<gtk::Button>() else {
            return;
        };
        if !button.is_sensitive() {
            return;
        }
        press(&open);
    });
}

/// §1's opener for a folder: the log link's launcher seam over a `file://` URI, its failure a log line.
pub fn folder_opener() -> Opener {
    std::rc::Rc::new(
        |dir: std::path::PathBuf| match glib::filename_to_uri(&dir, None) {
            Ok(uri) => {
                if let Err(why) = super::log_link::open(&uri) {
                    let name = dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    super::window_logs::log_line(&format!("!!! could not open {name}: {why}"));
                }
            }
            Err(err) => super::window_logs::log_line(&format!(
                "!!! could not open the output folder: {err}"
            )),
        },
    )
}
