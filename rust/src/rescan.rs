//! Noticing what changed on disk (spec/03-shell.md F0.11).
//!
//! The source list *is* the session, so a rescan cannot re-read it from a folder: what it can do is
//! notice that a file has gone and say which one. A row that quietly disappeared is how a render
//! comes out missing a camera angle nobody can account for. Everything else about a rescan — the
//! readouts, Cut's rebuild, Narrate's re-read — is re-reading what is on disk, so it happens here
//! rather than in a click handler (spec/00-principles.md §5).

use std::path::Path;

use crate::layout::Tree;
use crate::{cut, layout, narration, project::Project};

/// S3's sentence, said whatever the scan found.
pub const RESCANNED: &str = "rescanned";

/// S1's line for one vanished source. The path is the stored one — what the row shows — because
/// that is the name the user recognises, not the absolute path it happened to resolve to.
pub fn dropped(path: &str) -> String {
    format!("!!! dropped {path} -- it is no longer there")
}

/// What S2 refreshed, one variant per clause of its sentence so a test can pin the list instead of
/// eyeballing a UI.
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

/// S1 → S2 → S3. `dir` is the project folder whose files a `project:` path hangs off; a session
/// with no folder of its own passes the root, which is what `layout::resolve` does with one.
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

    // S2: the three refreshes. Cut and Narrate are re-read from disk, which is also what lets them
    // notice a file that was deleted — narration the page still shows after its folder went is the
    // bug this flow exists to avoid. With no project folder there is nothing on disk to re-read, so
    // only the readouts are real; `None` says that rather than inventing an empty file.
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

/// Is this stored path still on disk? The one path rule, so a `project:`-relative, root-relative or
/// absolute source is asked the same question.
fn is_there(root: &Path, dir: &Path, written: &str) -> bool {
    layout::resolve(root, dir, written).exists()
}
