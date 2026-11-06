//! §03-shell.md §1 badge **15**: the Outputs row's folder button. A press opens the page's current output folder ([`super::log_link::open`]).
//! The folder is re-read from the shell at press time, since a switch repaints the tooltip.

use adw::prelude::*;
use gtk4 as gtk;

use crate::layout::Tree;
use crate::shell::{Page, Shell};
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// The opening leg, injected so a test can see which folder a press asks for.
pub type Opener = std::rc::Rc<dyn Fn(std::path::PathBuf)>;

/// The page the row is naming, read off the newest window's shell; `None` when no window is up.
fn shell_now() -> Option<std::rc::Rc<std::cell::RefCell<Shell>>> {
    WINDOW_PAGE.with(
        |WindowPage {
             window_shells: shells,
             ..
         }| shells.borrow().last().cloned(),
    )
}

/// The folder the button should open now; `None` when no session folder is held.
pub fn folder_now() -> Option<std::path::PathBuf> {
    folder_in(&crate::startup::open_dir_if_set()?)
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

thread_local! {
    /// The last dialog a press put up, for a test to read where it starts.
    static LAST_DIALOG: std::cell::RefCell<Option<gtk::FileDialog>> = const { std::cell::RefCell::new(None) };
}

/// The folder in the same file dialog the top bar's Open uses, started in it: what the app can see is what it
/// shows, wherever the app runs (first rewrite: it handed the folder to the desktop, and in the dev container,
/// with no desktop to hand it to, the press did nothing). A file picked in it is opened as a log link is.
pub fn folder_opener() -> Opener {
    std::rc::Rc::new(|dir: std::path::PathBuf| {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dialog = gtk::FileDialog::builder()
            .title(format!("Outputs \u{2014} {name}"))
            .accept_label("Open")
            .build();
        dialog.set_initial_folder(Some(&gio::File::for_path(&dir)));
        LAST_DIALOG.with(|last| *last.borrow_mut() = Some(dialog.clone()));
        dialog.open(
            None::<&gtk::Window>,
            None::<&gio::Cancellable>,
            move |chosen: Result<gio::File, glib::Error>| {
                let Some(path) = chosen.ok().and_then(|file| file.path()) else {
                    return; // dismissed
                };
                if let Ok(uri) = glib::filename_to_uri(&path, None) {
                    if let Err(why) = super::log_link::open(&uri) {
                        super::window_logs::log_line(&format!(
                            "!!! could not open {}: {why}",
                            path.display()
                        ));
                    }
                }
            },
        );
    })
}

/// The dialog the last press put up. Test-only by name.
pub fn last_dialog_for_test() -> Option<gtk::FileDialog> {
    LAST_DIALOG.with(|last| last.borrow().clone())
}
