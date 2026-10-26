//! §08 items 1, 3, 6 and 10: the four Produce presses that ask the person for a file. ＋ Add image… and a slot's
//! Change… open an image chooser over the session's own frames; ⤓ export and ⤓ Save video open a save dialog on
//! the page's default name, `<project>-thumbnail.jpg` and `<project>.<container>`. Each answer goes through one
//! door ([`answer_image`], [`answer_export`], [`answer_save_video`]) that a test calls the way the dialog's
//! response does; the dialogs themselves are counted ([`asks_for_test`]) by the real openers, so a test proves the
//! button raised one without anyone answering it.
//!
//! The first rewrite had no chooser and no dialog: Add image took the newest frame (or made up
//! `produce/publish/frame-N.jpg`), Change… a stand-in, and both ⤓ wrote straight to the default name.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::produce_page;
use crate::ui::produce_press_wiring;

thread_local! {
    /// How many times each dialog was raised, by name: "image", "export", "save-video".
    static ASKS: std::cell::RefCell<std::collections::HashMap<&'static str, usize>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// How many times the named dialog was raised on this thread: "image", "export" or "save-video".
pub fn asks_for_test(which: &str) -> usize {
    ASKS.with(|asks| asks.borrow().get(which).copied().unwrap_or(0))
}

fn count(which: &'static str) {
    ASKS.with(|asks| *asks.borrow_mut().entry(which).or_insert(0) += 1);
}

/// Where the image chooser opens: the first footage recording's frames folder, else the page's publish folder,
/// else the project itself ("Add image… to edit one of your own frames", §A's empty state).
fn image_folder() -> PathBuf {
    let tree = crate::ui::produce_languages::session_tree();
    produce_press_wiring::frame_source_stems()
        .iter()
        .map(|stem| tree.frames_dir(stem))
        .find(|dir| dir.is_dir())
        .or_else(|| Some(tree.publish_dir()).filter(|dir| dir.is_dir()))
        .unwrap_or_else(|| tree.dir().to_path_buf())
}

/// ＋ Add image… (`slot` None) or slot `slot`'s Change… (`Some`): an image chooser over [`image_folder`]; the pick
/// goes to [`answer_image`].
#[allow(deprecated)] // the same GtkFileChooser API every other chooser in the app keeps
pub fn open_image_chooser(window: &adw::ApplicationWindow, slot: Option<usize>) {
    count("image");
    let title = match slot {
        None => "Add an image",
        Some(_) => "Change this image",
    };
    let chooser = gtk::FileChooserNative::builder()
        .title(title)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    let _ = chooser.set_current_folder(Some(&gio::File::for_path(image_folder())));
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Images"));
    for ext in ["jpg", "jpeg", "png", "webp", "bmp"] {
        filter.add_pattern(&format!("*.{ext}"));
        filter.add_pattern(&format!("*.{}", ext.to_uppercase()));
    }
    chooser.add_filter(&filter);
    let win = window.clone();
    chooser.connect_response(move |chooser, response| {
        if response != gtk::ResponseType::Accept {
            return;
        }
        if let Some(picked) = chooser.file().and_then(|file| file.path()) {
            answer_image(&win, slot, &picked);
        }
    });
    chooser.show();
}

/// The image chooser's answer: the picked file as the row's entry ([`entry_for`]), added at the end
/// (`slot` None) or put in slot `slot`'s place, through the same doors the page always used. Answers the
/// status the press says.
pub fn answer_image(window: &adw::ApplicationWindow, slot: Option<usize>, picked: &Path) -> String {
    let entry = entry_for(picked);
    match slot {
        None => produce_page::press_add_image(window, &entry),
        Some(index) => produce_page::press_change_image(window, index, &entry),
    }
}

/// How a picked picture is written in the row: one of the session's own frames as the moment it shows
/// (`frame:<seconds>`, the form the crop, the request and the cut-taken images use), anything else as a path
/// (`project:` inside the project, absolute outside: §1's one path rule).
pub fn entry_for(picked: &Path) -> String {
    let tree = crate::ui::produce_languages::session_tree();
    let same = |path: &Path| {
        path == picked || std::fs::canonicalize(path).ok() == std::fs::canonicalize(picked).ok()
    };
    let zero = crate::frames_run::session_zero(&tree);
    for stem in produce_press_wiring::frame_source_stems() {
        for (seconds, path) in
            crate::produce_upload_media::listed_frames(&tree.frames_dir(&stem), zero)
        {
            if same(&path) {
                return format!("frame:{seconds:.3}");
            }
        }
    }
    crate::layout::write_path(tree.dir(), tree.dir(), picked)
}

/// A save dialog opened on `default` (its folder and its name), the accepted path handed to `answer`.
#[allow(deprecated)] // the same GtkFileChooser API every other chooser in the app keeps
fn open_save_dialog(
    window: &adw::ApplicationWindow,
    title: &str,
    default: &str,
    answer: impl Fn(&adw::ApplicationWindow, &str) + 'static,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title(title)
        .modal(true)
        .action(gtk::FileChooserAction::Save)
        .build();
    let default = Path::new(default);
    if let Some(folder) = default.parent().filter(|folder| folder.is_dir()) {
        let _ = chooser.set_current_folder(Some(&gio::File::for_path(folder)));
    }
    if let Some(name) = default.file_name() {
        chooser.set_current_name(&name.to_string_lossy());
    }
    let win = window.clone();
    chooser.connect_response(move |chooser, response| {
        if response != gtk::ResponseType::Accept {
            return;
        }
        if let Some(path) = chooser.file().and_then(|file| file.path()) {
            answer(&win, &path.display().to_string());
        }
    });
    chooser.show();
}

/// ⤓ export: a save dialog on `<project>-thumbnail.jpg`, then [`answer_export`].
pub fn open_export_dialog(window: &adw::ApplicationWindow) {
    count("export");
    let default = produce_press_wiring::offered_export_name();
    open_save_dialog(window, "Export the thumbnail", &default, |window, path| {
        answer_export(window, path)
    });
}

/// The export dialog's answer: the ladder writes there, `.jpg` forced, on a worker. The path is this press's alone
/// (taken before the worker starts, then let go), so the next dialog opens on the page's own default again.
pub fn answer_export(window: &adw::ApplicationWindow, path: &str) {
    produce_press_wiring::answer_path(Some(path));
    produce_press_wiring::export_from_button(window);
    produce_press_wiring::answer_path(None);
}

/// ⤓ Save video: a save dialog on `<project>.<container>` beside the render, then [`answer_save_video`].
pub fn open_save_video_dialog(window: &adw::ApplicationWindow) {
    count("save-video");
    produce_press_wiring::answer_path(None);
    let default = produce_press_wiring::save_destination();
    open_save_dialog(window, "Save the video", &default, |window, path| {
        answer_save_video(window, path)
    });
}

/// The save dialog's answer: the video copied there, on a worker; the path is this press's alone, as for export.
pub fn answer_save_video(window: &adw::ApplicationWindow, path: &str) {
    produce_press_wiring::answer_path(Some(path));
    produce_press_wiring::save_video_on_worker(window);
    produce_press_wiring::answer_path(None);
}
