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

/// Move the window's Outputs row onto `dir` the way an Open does — `finish`'s two public legs, the save
/// target and the readout repaint, without the parts that need a full `Applied`. §1 badge 15 asks which
/// folder the row means, and an Open is what decides that in the program. Test-only: the real Open path
/// goes through [`finish`] itself.
#[allow(dead_code)] // called by tests only; production opens a project through `finish`
pub fn move_row_for_test(dir: &std::path::Path, project: &Project) {
    crate::ui::save_apply::set_open_dir(dir);
    let window = crate::ui::window::main_window();
    crate::ui::window::paint_readouts_for(&window, project, dir);
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
    // F2.10: the Cut page's rows come from what this PROJECT films — its footage files' name stamps, the
    // hand corrections in `cut.shift` and each file's probed length — so the session slot is re-asked here,
    // after the open took the folder on and before any page reads its rows.
    // §7 (`the page is rebuilt only when Prepare's output changed`) lives inside that door: an open takes on a
    // whole new folder, so it forgets the previous project's fingerprint first — otherwise tonight's
    // `session.tsv` matching last night's byte for byte would skip a rebuild the new project actually needs.
    crate::ui::cut_rebuild::forget();
    crate::ui::cut_session::refresh_page_spans(&crate::ui::window::newest_review_cut());
    // Then the two pages that own their own repaint, each reading the state it renders. The Narration tick
    // is seeded from the opened project when its widgets are built, so an open has to hand the flag over
    // explicitly here — both pages copy it into their own state rather than re-reading `naivepost.json`
    // while they paint (§08 §A's Game-audio row and §15's "no narration" clause are one fact). The write
    // is a no-op when the file already says what the opened project says, so this costs nothing on an open
    // that changed nothing.
    crate::ui::narrate_page::set_narration_off(&window, project.no_narration);
    crate::ui::produce_page::note_narration_off(project.no_narration);
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
