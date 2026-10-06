//! Applying a project that Open just read (spec/03-shell.md F0.9 S4).
//!
//! [`crate::open_project`] decides everything about the read — which folder the pick names, what the
//! file said, what had to be dropped, what moved. What it could not do is the last three steps of S4,
//! because they are window work: put the pair in `llm.conf` so the next launch reopens THIS project,
//! repaint every page from the session that just changed, and say on the status line what happened.
//! This module holds those three, in that order, and `ui::window::open_folder` calls it after the swap.
//!
//! The order is the spec's own arrow order (migrate → refresh → remember), with the status last
//! because it is what a person reads last. Remembering goes through
//! [`crate::machine_store::persist_project`], the ONE door that writes `llm.conf`; a conf that cannot
//! be read or written costs the memory and never the open, but it is said in the log rather than
//! swallowed (spec/00-principles.md: failure is specific and local).

use gtk4 as gtk;

use crate::open_project::Applied;
use crate::project::Project;

/// S4's status for a successful open, `<event> — <base>` like `new project — <base>`
/// ([`crate::new_project`]). DECISION: the spec gives S4 no success wording; the bare folder name read
/// as a label, not an event. The em dash matches [`crate::open_project::CANNOT_OPEN`], the spec's own.
pub fn opened_status(base: &str) -> String {
    format!("opened \u{2014} {base}")
}

/// Finish an open: refresh every page, remember the project for this root, say it.
///
/// `applied.root` is the folder just opened, which is NOT where the window's readouts point until the
/// repaint below moves them. Every repaint here goes through the same door a tab switch uses, so the
/// pages show the opened project rather than a state someone remembered to poke.
pub(crate) fn finish(applied: &Applied, project: &Project, status: &gtk::Label) {
    let window = crate::ui::window::main_window();
    let opened = applied.root.clone();
    // F0.10: from here on Save works on the folder just opened, not on the one the launch handed
    // over -- an open is the window taking on a project, and the save must follow it.
    crate::ui::save_apply::set_open_dir(&opened);
    // And the autosave follows it: its slot holds whatever folder a previous save named, and writing
    // the opened session into THAT would recreate a folder some earlier rename moved away from.
    crate::project_autosave::install(crate::ui::window::session_handle(&window), opened.clone());

    // S4 "refresh every page". The tabs first: whether a page is greyed is asked of the OPENED project
    // (`shell::lock`), so a page whose prerequisites vanished greys before anything reads its rows.
    crate::ui::window::paint_tabs_for(&window, project);
    crate::ui::window::paint_readouts_for(&window, project, &opened);
    // F2.10: the Cut page's rows come from what this PROJECT films — its footage files' name stamps, the
    // hand corrections in `cut.shift` and each file's probed length — so the session slot is re-asked here,
    // after the open took the folder on and before any page reads its rows.
    crate::ui::cut_session::refresh_page_spans(&crate::ui::window::newest_review_cut());
    // Then the two pages that own their own repaint, each reading the state it renders.
    crate::ui::narrate_page::refresh(&window);
    crate::ui::produce_page::refresh(&window);

    // S4 "remember the project for this root", through the one door that writes the conf. A refusal
    // costs the memory and never the open, but it is said in the log rather than swallowed: without it
    // the next launch silently reopens whatever was remembered before.
    let file = crate::open_project::project_file_in(&applied.out);
    match crate::ui::window::remember_opened(&applied.root, &file) {
        Ok(remembered) => crate::ui::window::log_line(&format!(">>> opened {}", remembered.file)),
        Err(why) => crate::ui::window::log_line(&format!(
            "!!! could not remember this project -- {why}"
        )),
    }

    // S4's status, last: "opened — <base>".
    let base = opened
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project opened".to_string());
    status.set_text(&opened_status(&base));
}
