//! §03-shell.md §1 badge **15**: the Outputs row's folder button. A press opens the page's current output folder ([`super::log_link::open`]).
//! The folder is re-read from the shell at press time, since a switch repaints the tooltip.

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout::Tree;
use crate::shell::{Page, Shell};

/// The opening leg, injected so a test can see which folder a press asks for.
pub type Opener = std::rc::Rc<dyn Fn(std::path::PathBuf)>;

/// The page the row is naming, read off the newest window's shell; `None` when no window is up.
fn shell_now() -> Option<std::rc::Rc<std::cell::RefCell<Shell>>> {
    crate::ui::window::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned())
}

/// The folder the button should open now; `None` when no session folder is held.
pub fn folder_now() -> Option<std::path::PathBuf> {
    folder_in(&crate::ui::save_apply::open_dir()?)
}

/// The shown page's output directory inside `dir`, or `None` when `dir` is not a project folder.
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

/// Same ladder as `window_bench::paint_readouts`, so press and tooltip agree.
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

pub fn press(open: &Opener) {
    match folder_now() {
        Some(dir) => open(dir),
        None => super::window_logs::log_line("!!! no output folder to open yet -- nothing is open"),
    }
}

/// Wire badge **15** through the default opener; inactive until there is a session.
pub fn wire(button: &gtk::Button) {
    wire_with(button, &folder_opener());
}

/// Wire with an opener a test can watch.
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
