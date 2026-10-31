//! Adding sources from Prepare (spec/03-shell.md F0.12).

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::layout::{self, Tree};
use crate::project::{Project, Source};

/// S0's refusal.
pub const RUN_REFUSAL: &str = "a run is already active \u{2014} stop it first (\u{23f9})";
/// S0's abandonment when `sources/` cannot be made: nothing is added, and the log has the reason.
pub const NO_SOURCES_DIR: &str = "could not make the project's sources folder \u{2014} see log";
/// S1's chooser title.
pub const CHOOSER_TITLE: &str = "Add sources";
/// S1's filter name, and the only kinds a source can be.
pub const FILTER_NAME: &str = "Audio and video";

/// What may be added at all, in the spec's order: eight audio kinds then six video ones.
pub const MEDIA_EXT: [&str; 14] = [
    ".flac", ".wav", ".mp3", ".m4a", ".aac", ".ogg", ".opus", ".wma", ".mp4", ".mkv", ".mov",
    ".webm", ".avi", ".ts",
];

/// The video kinds: a new row of these defaults to footage, because that is what a video is for
/// here — the row's own toggle is for the case where only its sound is wanted.
pub const VIDEO_EXT: [&str; 6] = [".mp4", ".mkv", ".mov", ".webm", ".avi", ".ts"];

/// S0's first half: whether the press may happen at all.
pub fn press(running: bool) -> Result<(), &'static str> {
    if running {
        return Err(RUN_REFUSAL);
    }
    Ok(())
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|ext| format!(".{}", ext.to_string_lossy().to_lowercase()))
        .unwrap_or_default()
}

/// S1/S3: is this a kind a source can be?
pub fn is_media(path: &Path) -> bool {
    MEDIA_EXT.contains(&extension(path).as_str())
}

/// S3: does a new row of this file default to footage?
pub fn is_video(path: &Path) -> bool {
    VIDEO_EXT.contains(&extension(path).as_str())
}

/// S2's progress in bytes, over all the files of one copy: `total` is the sum of their sizes taken
/// up front, so the bar means "how much of this import is on disk", which is what a slow card is
/// waiting on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

impl Progress {
    /// The bar's fraction.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.done as f64 / self.total as f64
    }
}

/// What one press added, in the form the window draws.
#[derive(Debug, PartialEq, Eq)]
pub struct Added {
    /// Rows that went in: `asked` minus the duplicates and the non-media.
    pub added: usize,
    /// Files the chooser was handed.
    pub asked: usize,
    /// The stored paths of the rows that went in, in the one path rule.
    pub paths: Vec<String>,
    pub progress: Vec<Progress>,
    pub status: String,
}

/// S2 → S5.
pub fn add(
    project: &mut Project,
    root: &Path,
    dir: &Path,
    files: &[PathBuf],
    copy: bool,
) -> Result<Added, String> {
    // S2's total comes from every file asked for, taken before the first byte moves.
    let sizes: Vec<u64> = files.iter().map(|file| size(file)).collect();
    let total: u64 = sizes.iter().sum();

    // S0's second half, and only for a copy: referencing in place needs no folder, so it is never
    // refused for the want of one.
    let tree = Tree::new(dir).ok();
    let sources = dir.join("sources");
    if copy {
        fs::create_dir_all(&sources).map_err(|_| NO_SOURCES_DIR.to_string())?;
    }

    let mut progress = Vec::new();
    let mut done = 0u64;
    for (file, size) in files.iter().zip(sizes) {
        if copy {
            copy_in(file, &tree, dir, &sources)?;
        }
        done += size;
        progress.push(Progress { done, total });
    }

    let mut paths = Vec::new();
    let mut already = 0usize;
    for file in files {
        if !is_media(file) {
            already += 1;
            continue;
        }
        let stored = if copy && !inside(file, dir) {
            sources.join(file.file_name().unwrap_or_default())
        } else {
            file.clone()
        };
        let written = layout::write_path(root, dir, &stored);
        if project.sources.iter().any(|row| row.path == written) {
            already += 1;
            continue;
        }
        project.sources.push(Source {
            path: written.clone(),
            footage: is_video(file),
            narrator: 1,
            ..Default::default()
        });
        paths.push(written);

        let parent = layout::write_path(
            root,
            dir,
            &file.parent().map(Path::to_path_buf).unwrap_or_default(),
        );
        if is_video(file) {
            project.vid_dir = Some(parent);
        } else {
            project.aud_dir = Some(parent);
        }
    }

    let added = paths.len();
    let asked = files.len();
    let status = status(added, already);

    Ok(Added {
        added,
        asked,
        paths,
        progress,
        status,
    })
}

/// S5's status.
pub fn status(added: usize, already: usize) -> String {
    let asked = added + already;
    match (added, already) {
        (0, _) => "already in the session \u{2014} nothing added".to_string(),
        (_, 0) => format!("added {added} source(s)"),
        _ => format!("added {added} of {asked} \u{2014} the rest were already in"),
    }
}

fn inside(file: &Path, dir: &Path) -> bool {
    file.strip_prefix(dir).is_ok()
}

/// S2's copy of one file, per file rather than for the whole pick: F0.12's bar reports the bytes as
/// they land, so the seam in `ui` needs to drive one file at a time and still use this rule alone.
pub fn copy_one(file: &Path, dir: &Path) -> Result<(), String> {
    let tree = Tree::new(dir).ok();
    let sources = dir.join("sources");
    copy_in(file, &tree, dir, &sources)
}

/// `.part` then rename, so a copy interrupted by a pulled drive or a full disk never looks like a
/// finished file.
fn copy_in(file: &Path, tree: &Option<Tree>, dir: &Path, sources: &Path) -> Result<(), String> {
    if !is_media(file) || inside(file, dir) {
        return Ok(());
    }
    let name = file
        .file_name()
        .ok_or_else(|| format!("{} has no name to copy it under", file.display()))?;
    let to = sources.join(name);
    if fs::metadata(&to).is_ok_and(|had| had.len() == size(file)) {
        return Ok(());
    }

    let mut in_ =
        BufReader::new(File::open(file).map_err(|err| format!("{}: {err}", file.display()))?);
    let part = match tree {
        Some(tree) => tree.copy_part(&name.to_string_lossy()),
        None => part_beside(&to),
    };
    let mut out = File::create(&part).map_err(|err| format!("{}: {err}", part.display()))?;
    // The writer is a scope of its own: `io::copy` must have let go of `out` before the flush and
    // the rename, or the `.part` would still be open when it is moved onto its real name.
    let copied = {
        let mut buffered = BufWriter::new(&mut out);
        io::copy(&mut in_, &mut buffered).and_then(|written| buffered.flush().map(|()| written))
    };
    if let Err(err) = copied {
        drop(out);
        let _ = fs::remove_file(&part);
        return Err(format!("{}: {err}", file.display()));
    }
    drop(out);
    fs::rename(&part, &to).map_err(|err| format!("{}: {err}", to.display()))?;
    Ok(())
}

fn part_beside(to: &Path) -> PathBuf {
    let mut part = to.as_os_str().to_os_string();
    part.push(".part");
    PathBuf::from(part)
}

/// A file's size, or nothing: a file that cannot be stat'd contributes no bytes to the bar and is
/// reported by the copy itself.
fn size(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}
