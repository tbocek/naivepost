//! §05-cut#7-rules bullet 8 — `the page is rebuilt only when Prepare's output changed`.

use crate::layout;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};

pub fn fingerprint(tree: &layout::Tree) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(tree.session_tsv()).ok()?;
    let secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some((meta.len(), secs))
}

/// Has Prepare's output changed since the page last looked? True the first time there is output (or none), false
/// while the file stays as it was, true again the moment its size or mtime moves or it disappears and returns.
pub fn output_changed(tree: &layout::Tree) -> bool {
    let now = fingerprint(tree);
    // One slot rather than a map keyed by window: the page this guards belongs to the newest window, which is
    // how `window.rs` keeps every one of its own thread-locals (newest slot wins), and a rebuild is asked for at
    // most once per refresh of whichever window is up. A map would only accumulate folders whose window closed.
    CUT_PAGE.with(|CutPage { seen, .. }| {
        let previous = *seen.borrow();
        *seen.borrow_mut() = Some(now);
        previous.is_none_or(|was| was != now)
    })
}

/// Forget what was seen: the next [`output_changed`] answers true whatever the file looks like. Used when a
/// folder is taken over wholesale (an open, a rescan) and the page must redraw even if tonight's `session.tsv`
/// happens to match the last project's byte for byte.
pub fn forget() {
    CUT_PAGE.with(|CutPage { seen, .. }| *seen.borrow_mut() = None);
}
