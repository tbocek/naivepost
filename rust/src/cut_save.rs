//! §05-cut#7-rules bullet 7 — `cut.json is written by one function`.
//!
//! [`crate::cut::save`] is the format writer: it serialises a `Cut`, applies §3's first-save migration, and puts
//! the bytes at the one path `layout::Tree` knows. What was missing was the OTHER half of the sentence — every
//! editing door had to reach the file through that writer, and each had worked out the session folder for itself
//! (`startup::session_dir(current_dir())` + `layout::Tree::new`) or not at all. Doors that repeated the path
//! logic drifted; doors that skipped it left an edit on screen and nowhere else, so reopening the project lost
//! it. This module is that one path: resolve the session, hand the cut to `cut::save`, nothing else.
//!
//! Which doors call it: every door that ENDS an act of the hand — ＋ Add / | Split / － Remove / ⌦, a fold toggle,
//! ⇲ Lane, a camera/hearing change, an effect placement (zoom, text, drawing, speed, volume, label), a trim's
//! release, a slide's release, and a landed suggestion. A mid-drag update does NOT go through here: subtask 1's
//! non-pushing trim path writes state without persisting per mouse-move, because a file write per tick is I/O in
//! the drag loop and would leave the file holding whichever edge the pointer happened to be over when it stalled.

use crate::cut::{self, Cut};
use crate::{layout, startup};

thread_local! {
    /// Is a drag in flight, so `cut.json` must NOT be written? §7's mid-drag rule lives HERE rather than at
    /// each caller, because a tick that forgot the rule is a file write per mouse-move.
    static GESTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Write `cut_` to this session's `cut.json` through [`cut::save`] — the one door §7 means. Errors come back as
/// the string `cut::save` produced rather than being swallowed, so a caller can say where it failed instead of
/// leaving a silent difference between the page and the disk.
pub fn write_cut(cut_: &Cut) -> Result<(), String> {
    if gesture_in_flight() {
        return Ok(());
    }
    let root = std::env::current_dir().unwrap_or_default();
    write_cut_for_test(&root, cut_)
}

/// The same write with the project root named — TEST SEAM: the program's doors take the process working
/// directory through [`write_cut`], exactly as `add_files` and the source rows do, while a check points at a
/// temp `.naivepost` instead of moving cwd (which would move every other page's resolution with it).
pub fn write_cut_for_test(root: &std::path::Path, cut_: &Cut) -> Result<(), String> {
    if gesture_in_flight() {
        return Ok(());
    }
    let dir = startup::session_dir(root);
    let tree = layout::Tree::new(&dir).map_err(|err| err.to_string())?;
    cut::save(cut_, &tree)
}

/// §7 (`its existence unlocks Narrate and Produce`): the gate is the FILE, asked of the same resolution the/// writer uses, so a page cannot believe it has a cut that is not on disk. [`crate::cut::exists`] stays the
/// predicate over a `Tree`; this is the version that knows where the tree is.
pub fn exists() -> bool {
    let root = std::env::current_dir().unwrap_or_default();
    exists_for_test(&root)
}

/// The write into a session folder the CALLER already resolved. The suggest flow is the case that needs it:
/// §F2.14 S1 refuses on "no session", so that flow resolves the folder itself (and a test may point it at a
/// temp `.naivepost` through `set_session_for_test`), and the write has to land in exactly that folder rather
/// than in whatever `current_dir()` says. Still the one writer — same `cut::save`, same mid-drag refusal.
pub fn write_cut_at(cut_: &Cut, tree: &layout::Tree) -> Result<(), String> {
    if gesture_in_flight() {
        return Ok(());
    }
    cut::save(cut_, tree)
}

/// [`exists`] with the project root named — TEST SEAM (`_for_test` in the sense that only a check names a
/// folder): the program's doors ask [`exists`], which takes the process working directory like every other
/// page's resolution, while a check points at its own temp `.naivepost` without moving cwd.
pub fn exists_for_test(root: &std::path::Path) -> bool {
    let dir = startup::session_dir(root);
    match layout::Tree::new(&dir) {
        Ok(tree) => cut::exists(&tree),
        Err(_) => false,
    }
}

/// A gesture's live tick writes state, not the file (§7's `cut.json is written by one function` read at the
/// door that knows which tick it is). Doors that update state call this on the way; [`write_cut`] refuses while
/// it is set, so a tick cannot reach the disk by forgetting to ask. [`end_gesture`] clears it at the release,
/// and the release's own write then goes through.
pub fn begin_gesture() {
    GESTURE.with(|cell| cell.set(true));
}

/// The gesture's release: allow writes again. The caller writes afterwards, which is the point — the file ends
/// holding the state the hand LEFT rather than whichever tick the pointer stalled on.
pub fn end_gesture() {
    GESTURE.with(|cell| cell.set(false));
}

/// Is a gesture in flight, so a write should be refused? §7's one-writer rule read from the writer's side: the
/// history skips the same ticks (`one Undo per drag`), and now one question answers both.
pub fn gesture_in_flight() -> bool {
    GESTURE.with(|cell| cell.get())
}
