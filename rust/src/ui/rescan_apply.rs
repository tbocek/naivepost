//! Applying what a Rescan just found (spec/03-shell.md F0.11 S1–S3).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4 as gtk;

use crate::project::Project;
use crate::rescan::{self, Refreshed};
use crate::shell::Shell;
use crate::ui::window;

/// Every step one press of ⟳ takes, in the spec's order.
pub(crate) fn apply(
    found: &rescan::Rescan,
    scanned: &Project,
    session: &Rc<RefCell<Project>>,
    dir: &Path,
    tree: Option<&crate::layout::Tree>,
    status: &gtk::Label,
    shell: &Rc<RefCell<Shell>>,
) {
    // S1: one line per dropped row, named by the stored path the row shows.
    for line in &found.logs {
        window::log_line(line);
    }

    if tree.is_some() {
        crate::project_autosave::write_session_now(&session.borrow(), dir);
    }

    // §1's third case, and the ONLY thing the lock question gates: a page whose prerequisites vanished
    // while it was open goes back to Prepare in silence (F0.1 S2's bounce is only for a click). The
    // refresh below is NOT behind this test -- a normal press with nothing vanishing still refreshes.
    if crate::shell::lock(shell.borrow().page, scanned).is_some() {
        let (scan_tree, cut, mut narration) = window::session_reads();
        if let Some(held) = window::held_narration() {
            *held.borrow_mut() = narration.clone();
        }
        let page = shell.borrow().page;
        shell.borrow_mut().switch(
            page,
            crate::shell::Move::Vanished,
            scanned,
            scan_tree.as_ref(),
            &cut.segs,
            &mut narration.entries,
        );
        // The stack shows the page, so the shell's answer has to reach it or the window keeps drawing the
        // page that just lost its base. The guard is GTK's own re-entrancy latch, the one `wire_switching`
        // uses for a bounce.
        let latch = window::switch_latch();
        if let Some(stack) = window::view_stack(&window::main_window()) {
            if let Some(latch) = latch.as_ref() {
                latch.set(true);
            }
            stack.set_visible_child_name(shell.borrow().page.label());
            if let Some(latch) = latch.as_ref() {
                latch.set(false);
            }
        }
        window::paint_tabs_for(&window::main_window(), scanned);
    }

    // S2: "refresh every tab's readouts · Cut rebuilds · Narrate re-reads its file", driven by what the
    // scan actually re-read rather than by whether a page happened to become locked.
    for step in &found.refreshed {
        match step {
            Refreshed::Readouts => {
                window::paint_readouts_for(&window::main_window(), scanned, dir);
            }
            Refreshed::CutRebuild => {
                let main = window::main_window();
                if let Some(cut) = &found.cut {
                    window::seed_review_cut(cut);
                }
                crate::ui::cut_cameras::refresh_camera_rows(&main);
                window::refresh_effects_lane(&main);
            }
            Refreshed::NarrateReRead => {
                let main = window::main_window();
                let lines = found.narration.clone().unwrap_or_default();
                if let Some(held) = window::held_narration() {
                    *held.borrow_mut() = lines.clone();
                }
                // The page draws its rows from its OWN published state, not from the holder above, so
                // re-seeding the holder alone leaves lines whose recording is gone sitting on screen.
                crate::ui::narrate_page::publish_lines(lines.entries.clone());
                crate::ui::narrate_page::refresh(&main);
            }
        }
    }

    // S3, last: said whatever the scan found, including a scan that dropped nothing.
    status.set_text(found.status);
}
