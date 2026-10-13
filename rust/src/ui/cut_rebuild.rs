//! §05-cut#7-rules bullet 8 — `the page is rebuilt only when Prepare's output changed`.
//!
//! Recomputing the Cut page's session spans is not free: [`crate::ui::cut_session::for_open_session`] reloads
//! the project and runs ffprobe over every footage file (cached per path, but a rescan invalidates nothing and
//! a fresh open has no cache at all), then the row list, the fold badges and the painter are all rebuilt from
//! the answer. Doing that on every beat of a run, or on every refresh whose scan re-read something the Cut page
//! never looks at, is work whose answer is known in advance.
//!
//! So the identity of Prepare's output is asked instead: `transcript/session.tsv` is what Prepare writes and
//! what the rows are laid out from, so if that file has neither changed size nor been touched since the last
//! time the page looked, there is nothing new to draw. Size + mtime from one `fs::metadata` call — cheap enough
//! to ask on every refresh, and it never decodes the file (§7's other clause: `nothing decodes inside a draw`).
//!
//! Absent output is its OWN state, distinct from "unchanged": a session Prepare has not produced yet means the
//! page has never drawn this, so the first look counts as changed and the next does not. Otherwise a project
//! opened before its first Prepare would rebuild forever, which is exactly the cost this module exists to avoid.

use crate::layout;

/// Prepare's output identity: `(size_bytes, mtime_secs)` of `transcript/session.tsv`, or `None` when the file
/// is not there — "no output yet", which is a different answer from "same as last time" and must not be folded
/// into it. A metadata call that fails for another reason (permissions, a vanished folder) reads as absent too:
/// either way there is nothing here to draw from.
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

thread_local! {
    /// The fingerprint the page was last built from. `Some(None)` means "this state has been looked at and there
    /// is no output" — distinct from `None`, which means nobody has looked yet. Collapsing those two would make
    /// a session with nothing prepared either rebuild on every beat or never notice its first Prepare.
    static SEEN: std::cell::RefCell<Option<Option<(u64, u64)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Has Prepare's output changed since the page last looked? True the first time there is output (or none), false
/// while the file stays as it was, true again the moment its size or mtime moves or it disappears and returns.
/// A caller that gets `false` skips its recompute entirely — skipping is the normal case, not an error.
pub fn output_changed(tree: &layout::Tree) -> bool {
    let now = fingerprint(tree);
    // One slot rather than a map keyed by window: the page this guards belongs to the newest window, which is
    // how `window.rs` keeps every one of its own thread-locals (newest slot wins), and a rebuild is asked for at
    // most once per refresh of whichever window is up. A map would only accumulate folders whose window closed.
    SEEN.with(|seen| {
        let previous = *seen.borrow();
        *seen.borrow_mut() = Some(now);
        previous.is_none_or(|was| was != now)
    })
}

/// Forget what was seen: the next [`output_changed`] answers true whatever the file looks like. Used when a
/// folder is taken over wholesale (an open, a rescan) and the page must redraw even if tonight's `session.tsv`
/// happens to match the last project's byte for byte.
pub fn forget() {
    SEEN.with(|seen| *seen.borrow_mut() = None);
}
