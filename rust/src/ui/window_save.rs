//! F0.8 / §1 Save door: where a press of Save goes, and the dialog that names the project.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::Project;
use crate::run;
use crate::save_as;
use crate::ui::save_apply;

/// Where a press of Save goes: [`save_as::press`] decides whether it may happen; the chooser only names the project.
/// Reads the LIVE session (swapped by [`wire_new`] and [`wire_open`]), not the `&Project` the window was built from.
pub(crate) fn wire_save(
    save_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    save_.connect_clicked(move |_| {
        if let Err(reason) = save_as::press(bar.borrow().running.is_some()) {
            save_apply::refused(reason, &status);
            return;
        }
        let open = save_apply::save_target();
        ask_save_project(&status, &session, &open);
    });
}

/// S2: the dialog that names the project. `select_folder` because a project is a folder (01 §1); starts as the open folder's name.
fn ask_save_project(status: &gtk::Label, session: &Rc<RefCell<Project>>, open: &Path) {
    let dialog = gtk::FileDialog::builder()
        .title(save_as::TITLE)
        .accept_label("Save")
        .build();
    if let Some(dir) = open.parent() {
        dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    }
    dialog.set_initial_name(open.file_name().and_then(|name| name.to_str()));
    let status = status.clone();
    let open = open.to_path_buf();
    let session = session.clone();
    dialog.select_folder(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            let Ok(file) = chosen else {
                return; // dismissed: nothing was asked for, so nothing changed
            };
            let named = file
                .path()
                .unwrap_or_default()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            save_apply::save_named(&session, &status, &open, &named).ok();
        },
    );
}
