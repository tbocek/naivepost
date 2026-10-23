//! §03-shell.md §1 S9: the narration write a close owes, split out of `window.rs` (which is past its
//! size budget).
//!
//! `narrate_text::flush_owed` reads the Narrate page's process-global state, which belongs to whichever
//! page was refreshed last. A window closed while its own tab was showing never left the tab, so the
//! switch that normally carries the two copies together (`carry_into_held_record`) never ran: what this
//! window was shown lives in its held record, and only that reaches disk here. Both are written, newest
//! copy last, because typing lands in the page's state and a tab leave writes the held record.

use crate::layout;
use crate::narration;

/// Write the owed narration for `window`, then settle the flag so nothing writes it twice.
pub(crate) fn flush_held_narration(window: &adw::ApplicationWindow) {
    let _ = window;
    let owed = crate::ui::window::WINDOW_SHELLS.with(|shells| {
        shells
            .borrow()
            .last()
            .map(|shell| shell.borrow().narration_pending.owe())
            .unwrap_or(false)
    });
    if !owed {
        return;
    }
    let Some(dir) =
        crate::ui::window_line_step::SESSION_DIRS.with(|held| held.borrow().last().cloned())
    else {
        return;
    };
    let Some(tree) = layout::Tree::new(&dir).ok() else {
        return;
    };
    // `silent` is the file's to keep, never invented here: it records the clips a delete remembered as
    // deliberately silent, and a flush of text must not erase them.
    let silent = narration::load(&tree)
        .map(|existing| existing.silent)
        .unwrap_or_default();
    let page_lines = crate::ui::narrate_page::read_state().entries;
    let held_lines = crate::ui::window_selection::HELD_NARRATION.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|held| held.borrow().entries.clone())
    });
    // The page's copy first, this window's held record over it: the held record is what a tab leave would
    // have written, so where both hold text that one is the window's own answer.
    for entries in [page_lines, held_lines.unwrap_or_default()] {
        if entries.is_empty() {
            continue;
        }
        let _ = narration::save(
            &narration::Narration {
                entries,
                silent: silent.clone(),
            },
            &tree,
        );
    }
    if let Some(shell) =
        crate::ui::window::WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned())
    {
        shell.borrow_mut().narration_pending.flush();
    }
    // The flag just settled has to reach disk before the close hook's own project write runs: that write
    // saves the LIVE project, whose `no_narration` was set by a tick through the same held handle. If
    // this flush ran first and nothing rewrote the file, the naivepost.json on disk would still show the
    // pre-tick value -- which is what §7 F4.8 node 2 pins (the tick survives a reopen).
    let _ = crate::ui::window_line_step::flush_session_on_close();
}
