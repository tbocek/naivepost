//! §02-services.md §4.1 — where the Cut page's ＋ Add gets its snap points with no aligner. Split out of
//! `window.rs`, which is at its size budget: this holds the reading side (which project, which lane,
//! which cached envelope) and nothing else; the rules are [`crate::cut_wave_edges`]'s.

use crate::cut_select;
use crate::layout;
use crate::startup;

/// The folder the open project lives in, the same stand-in Rescan, New and Save use (§the window's
/// `session_reads`): the project file's own folder, so a source path and a `cache/waves` file read
/// from the tree built on it belong to the same project. `None` when nothing is open.
fn open_project_dir() -> Option<std::path::PathBuf> {
    let dir = startup::open_dir();
    dir.join(crate::project::PROJECT_FILE)
        .is_file()
        .then_some(dir)
}

/// This session's project folder as both a [`layout::Tree`] and its loaded project, or `None` when
/// either is missing — no folder, no project file, a project file that will not parse.
fn session_project() -> Option<(layout::Tree, crate::project::Project)> {
    let dir = open_project_dir()?;
    let tree = layout::Tree::new(&dir).ok()?;
    let project = crate::project::load(&dir).ok()?;
    Some((tree, project))
}

/// §02-services.md §4.1 — the snap points ＋ Add may use with no aligner: the dragged band's own two
/// ends, each moved onto the near end of a silence in the row's cached envelope. A point is only
/// handed over when the sound supports it; a band with no silence near an end keeps that end where it
/// was drawn, because inventing a cut in the middle of a word is a click. With word timings (an
/// aligner named) there is nothing here to offer and the empty list means exactly that.
pub fn snap_points(band: Option<&cut_select::Selection>) -> Vec<f64> {
    let Some(band) = band else {
        return Vec::new();
    };
    let cut_select::Scope::Footage { row } = band.scope else {
        return Vec::new();
    };
    if !crate::cut_wave_edges::waveform_mode(&crate::cut_wave_edges::aligner_from_settings()) {
        return Vec::new();
    }
    let Some((tree, project)) = session_project() else {
        return Vec::new();
    };
    let Some(lane) = crate::cut_wave_edges::lane_for_row(&project.sources, row as i32) else {
        return Vec::new();
    };
    let envs = crate::cut_wave_edges::read_lanes(&tree, &project.sources);
    let (start, end) = crate::cut_wave_edges::band_edges(&envs, &lane, band.start, band.end);
    [start, end].into_iter().flatten().collect()
}
