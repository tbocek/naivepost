//! Applying a project that Open just read (spec/03-shell.md F0.9 S4).

use gtk4 as gtk;

use crate::open_project::Applied;
use crate::project::Project;

pub fn opened_status(base: &str) -> String {
    format!("opened \u{2014} {base}")
}

#[allow(dead_code)] // called by tests only; production opens a project through `finish`
pub fn move_row_for_test(dir: &std::path::Path, project: &Project) {
    crate::ui::save_apply::set_open_dir(dir);
    let window = crate::ui::window::main_window();
    crate::ui::window::paint_readouts_for(&window, project, dir);
}

/// Finish an open: refresh every page, remember the project for this root, say it.
pub(crate) fn finish(applied: &Applied, project: &Project, status: &gtk::Label) {
    let window = crate::ui::window::main_window();
    let opened = applied.root.clone();
    // F0.10: from here on Save works on the folder just opened, not on the one the launch handed
    // over -- an open is the window taking on a project, and the save must follow it.
    crate::ui::save_apply::set_open_dir(&opened);
    // §12 §4's walk-back door reads the session through `cut_suggest_land`'s own slot, so taking a project
    // on installs it there too: ▶ Suggest must cut from THIS folder's words, not from whatever folder an
    // earlier window left in the slot.
    crate::ui::window_line_step::push_session_tree(crate::layout::Tree::new(&opened).ok());
    // And the autosave follows it: its slot holds whatever folder a previous save named, and writing
    // the opened session into THAT would recreate a folder some earlier rename moved away from.
    crate::project_autosave::install(crate::ui::window::session_handle(&window), opened.clone());

    // S4 "refresh every page". The tabs first: whether a page is greyed is asked of the OPENED project
    // (`shell::lock`), so a page whose prerequisites vanished greys before anything reads its rows.
    crate::ui::window::paint_tabs_for(&window, project);
    crate::ui::window::paint_readouts_for(&window, project, &opened);
    crate::ui::cut_rebuild::forget();
    crate::ui::cut_session::refresh_page_spans(&crate::ui::window::newest_review_cut());
    crate::ui::narrate_page::refresh(&window);
    crate::ui::produce_page::refresh(&window);

    let file = crate::open_project::project_file_in(&applied.out);
    let launch_root = std::env::current_dir().unwrap_or_default();
    match crate::ui::window::remember_opened(&launch_root, &file) {
        Ok(remembered) => crate::ui::window::log_line(&format!(">>> opened {}", remembered.file)),
        Err(why) => {
            crate::ui::window::log_line(&format!("!!! could not remember this project -- {why}"))
        }
    }

    let base = opened
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project opened".to_string());
    status.set_text(&opened_status(&base));
}
