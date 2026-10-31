//! Noticing what changed on disk (spec/03-shell.md F0.11).
//!
//! The source list *is* the session, so a rescan cannot re-read it from a folder: it notices that a file has gone and says which (a quietly vanished row leaves a render missing a camera angle). The readouts, Cut's rebuild and Narrate's re-read happen here, not in a click handler (spec/00-principles.md §5).

use std::path::Path;

use crate::layout::Tree;
use crate::{cut, layout, narration, project::Project};

/// S3's sentence, said whatever the scan found.
pub const RESCANNED: &str = "rescanned";

/// S1's line for one vanished source, by the stored path (what the row shows).
pub fn dropped(path: &str) -> String {
    format!("!!! dropped {path} -- it is no longer there")
}

/// What S2 refreshed, one variant per clause of its sentence so a test can pin the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refreshed {
    Readouts,
    CutRebuild,
    NarrateReRead,
}

/// What the scan found and what it re-read, in the form the window draws.
#[derive(Debug, PartialEq)]
pub struct Rescan {
    /// The sources that went, by their stored path, in list order.
    pub dropped: Vec<String>,
    pub logs: Vec<String>,
    /// The source given slot 1 because slot 1 lost its holder.
    pub reassigned: Option<String>,
    pub refreshed: Vec<Refreshed>,
    pub cut: Option<cut::Cut>,
    pub narration: Option<narration::Narration>,
    pub status: &'static str,
}

/// S1 → S2 → S3. `dir` is the project folder a `project:` path hangs off; a session with no folder passes the root (`layout::resolve`).
pub fn rescan(project: &mut Project, root: &Path, dir: &Path, tree: Option<&Tree>) -> Rescan {
    // S1: keep what is still there, and name what is not.
    let (gone, kept): (Vec<_>, Vec<_>) = project
        .sources
        .clone()
        .into_iter()
        .partition(|source| !is_there(root, dir, &source.path));
    let gone_paths: Vec<String> = gone.iter().map(|source| source.path.clone()).collect();
    let logs: Vec<String> = gone_paths.iter().map(|path| dropped(path)).collect();
    project.sources = kept;

    // Voices are not exclusive any more, so a dropped row frees nothing to hand on.
    let reassigned: Option<String> = None;

    // S2: the three refreshes. Cut and Narrate are re-read from disk, so they notice a deleted file; with no project folder only the readouts are real, and `None` says that.
    let refreshed = if tree.is_some() {
        vec![
            Refreshed::Readouts,
            Refreshed::CutRebuild,
            Refreshed::NarrateReRead,
        ]
    } else {
        vec![Refreshed::Readouts]
    };
    let cut = tree.and_then(|tree| cut::load(tree).ok());
    let narration_file = tree.and_then(|tree| narration::load(tree).ok());

    Rescan {
        refreshed,
        dropped: gone_paths,
        logs,
        reassigned,
        cut,
        narration: narration_file,
        status: RESCANNED,
    }
}

/// Is this stored path still on disk? The one path rule, so every source kind is asked the same.
fn is_there(root: &Path, dir: &Path, written: &str) -> bool {
    layout::resolve(root, dir, written).exists()
}
