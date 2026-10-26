//! Applying what a Rescan just found (spec/03-shell.md F0.11 S1–S3).
//!
//! [`crate::rescan`] decides everything about the scan — which source's file is gone, who gets slot 1
//! now that its holder went, and which of the three refreshes there was anything on disk to re-read.
//! What it cannot do is the window half: say each dropped row in the log, write the pruned list back so
//! the next launch does not show the dead row again, bounce a page whose prerequisites vanished, and
//! repaint every tab from what the scan re-read. Those steps live here, in the spec's arrow order, and
//! `ui::window::wire_rescan` only forwards the answer through [`apply`] — the same seam
//! [`super::open_apply`] is for Open, split out of `window.rs`, which is past its size budget.
//!
//! The order is the flowchart's: name the drops, persist the pruned list, send a vanished page back to
//! Prepare in silence, refresh every tab, and only then say "rescanned" — the status is what a person
//! reads last, and S3 says it whatever the scan found.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4 as gtk;

use crate::project::Project;
use crate::rescan::{self, Refreshed};
use crate::shell::Shell;
use crate::ui::window;

/// Every step one press of ⟳ takes, in the spec's order.
///
/// `scanned` is the session AFTER the drop — the list the lock question, the readouts and the write-back
/// all have to be asked of. Asking the copy `build_window` was handed would answer about sources that
/// are already gone, which is how a rescan used to leave the Cut page counting a deleted file.
/// Reached only through `wire_rescan`: the button's press is the one door §1 gives this flow.
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

    // The pruned list goes back to disk, THROUGH the window's autosave slot rather than beside it: the
    // slot's `Autosave` marker is what knows whether the file already holds this list. Without it a
    // plain write left the timer holding a stale marker, and the next tick (or the close flush) wrote
    // the project file again after this flow said it was done -- which is what made another flow's
    // project file disagree with what it had just written. A folder with no project file of its own has
    // nothing to rewrite; a failed write costs the persistence and never the refresh, and is said.
    if tree.is_some() {
        crate::project_autosave::write_session_now(&session.borrow(), dir);
    }

    // §1's third case, and the ONLY thing the lock question gates: a page whose prerequisites vanished
    // while it was open goes back to Prepare in silence (F0.1 S2's bounce is only for a click). The
    // refresh below is NOT behind this test -- a normal press with nothing vanishing still refreshes.
    if crate::shell::lock(shell.borrow().page, scanned).is_some() {
        let (scan_tree, cut, mut narration) = window::session_reads();
        // The scan rewrote the folder, so the window's held lines are re-seeded from it here: a refit
        // over narration that no longer belongs to this project would write someone else's words back
        // over the ones just scanned.
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
        // The shell's own switch door is already on the stack when a rescan moves it: `set_visible_child_name`
        // fires `notify::visible-child`, and that handler reads the shell. Going through the same latch
        // `wire_switching` uses for its bounce keeps that handler from re-entering while `apply` holds the
        // shell borrowed -- which would panic in `Shell::switch` instead of moving the page.
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
                // The strip paints from the window's held cut, so seeding without refreshing leaves
                // the old clip list on screen -- the deleted camera would still be in tonight's shot.
                if let Some(cut) = &found.cut {
                    window::seed_review_cut(&main, cut);
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
                // This is the page's publish door -- the same one the flows and `--snapshot` use.
                crate::ui::narrate_page::publish_lines(lines.entries.clone());
                crate::ui::narrate_page::refresh(&main);
            }
        }
    }

    // S3, last: said whatever the scan found, including a scan that dropped nothing.
    status.set_text(found.status);
}
