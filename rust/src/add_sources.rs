//! Adding sources from Prepare (spec/03-shell.md F0.12).

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::layout::{self, Tree};
use crate::project::{Project, Source};
use crate::source_probe::Tool;

/// The two programs a repair asks (F0.12 S2b), and the leg that starts them: the page passes
/// [`crate::source_probe::run_ffprobe`], a test its stub.
pub struct Repair<'a> {
    pub ffprobe: &'a str,
    pub ffmpeg: &'a str,
    pub run: &'a Tool,
}

/// What became of one file of the pick (F0.12 S2, S2b).
#[derive(Debug, Clone, PartialEq)]
pub enum Copied {
    /// Not a source, already inside the project, or already in `sources/` as it should be.
    Skipped,
    /// Byte for byte.
    Copied,
    /// Its header held no length, so the copy was written again around the same pictures and sound;
    /// `seconds` is the length the copy's header now gives.
    Repaired { seconds: f64 },
    /// Its header holds no length and writing it again did not give one: copied byte for byte, and
    /// why the repair came to nothing.
    Unrepaired(String),
}

impl Copied {
    /// The log's sentence for a file that needed mending; nothing for one that did not.
    pub fn said(&self, name: &str) -> Option<String> {
        match self {
            Copied::Repaired { seconds } => Some(format!(
                ">>> {name}: the recording was never closed (no length in its header) -- the copy in sources/ is written again around the same {}, {} long",
                if is_video(Path::new(name)) {
                    "pictures and sound"
                } else {
                    "sound"
                },
                crate::tools::mm_ss(*seconds)
            )),
            Copied::Unrepaired(why) => Some(format!(
                "!!! {name}: no length in its header and the repair gave none ({why}) -- copied as it is; the steps that need its length will say so"
            )),
            Copied::Skipped | Copied::Copied => None,
        }
    }
}

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
    // S0's second half, and only for a copy: referencing in place needs no folder, so it is never
    // refused for the want of one.
    let tree = Tree::new(dir).ok();
    let sources = dir.join("sources");
    if copy {
        fs::create_dir_all(&sources).map_err(|_| NO_SOURCES_DIR.to_string())?;
    }

    if copy {
        for file in files {
            copy_in(file, &tree, dir, &sources, None)?;
        }
    }

    Ok(add_rows(project, root, dir, files, copy))
}

/// S3 → S5 alone: the rows for files whose bytes are already where they belong. The window copies on
/// a worker ([`copy_one`]) and calls this as each file lands, so a repaired copy is never written over.
pub fn add_rows(
    project: &mut Project,
    root: &Path,
    dir: &Path,
    files: &[PathBuf],
    copy: bool,
) -> Added {
    let sources = dir.join("sources");
    let sizes: Vec<u64> = files.iter().map(|file| size(file)).collect();
    let total: u64 = sizes.iter().sum();
    let mut progress = Vec::new();
    let mut done = 0u64;
    for size in sizes {
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

    Added {
        added,
        asked,
        paths,
        progress,
        status,
    }
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
/// With `repair`, a recording whose header holds no length is mended on its way in (S2b).
pub fn copy_one(file: &Path, dir: &Path, repair: Option<&Repair>) -> Result<Copied, String> {
    let tree = Tree::new(dir).ok();
    let sources = dir.join("sources");
    copy_in(file, &tree, dir, &sources, repair)
}

/// What a file's header says about its length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Header {
    Seconds(f64),
    /// ffprobe reads the file and its header holds no length (`N/A`): a recorder killed mid-write,
    /// whose container was never closed.
    NoLength,
    /// ffprobe cannot read the file at all: nothing a repair could start from.
    Unreadable,
}

/// Ask ffprobe for the header's length.
pub fn header(repair: &Repair, path: &Path) -> Header {
    let shown = path.display().to_string();
    let args = [
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "default=nw=1:nk=1",
        shown.as_str(),
    ];
    match (repair.run)(repair.ffprobe, &args) {
        Err(_) => Header::Unreadable,
        Ok(answer) => match answer.trim().parse::<f64>() {
            Ok(seconds) if seconds > 0.0 => Header::Seconds(seconds),
            _ => Header::NoLength,
        },
    }
}

/// S2b's ffmpeg arguments: every stream written again into a closed container, nothing decoded.
/// FLAC is the exception: its length lives in a header a stream copy carries over as it was, so its
/// samples are packed again, which loses nothing.
pub fn repair_args(from: &Path, to: &Path) -> Vec<String> {
    let mut args: Vec<String> = ["-v", "error", "-y", "-i"].map(String::from).to_vec();
    args.push(from.display().to_string());
    args.extend(["-map", "0"].map(String::from));
    if extension(from) == ".flac" {
        args.extend(["-c:a", "flac"].map(String::from));
    } else {
        args.extend(["-c", "copy"].map(String::from));
    }
    args.push(to.display().to_string());
    args
}

/// `.part` then rename, so a copy interrupted by a pulled drive or a full disk never looks like a
/// finished file.
fn copy_in(
    file: &Path,
    tree: &Option<Tree>,
    dir: &Path,
    sources: &Path,
    repair: Option<&Repair>,
) -> Result<Copied, String> {
    if !is_media(file) || inside(file, dir) {
        return Ok(Copied::Skipped);
    }
    let name = file
        .file_name()
        .ok_or_else(|| format!("{} has no name to copy it under", file.display()))?;
    let to = sources.join(name);
    if fs::metadata(&to).is_ok_and(|had| had.len() == size(file)) {
        return Ok(Copied::Skipped);
    }
    let part = match tree {
        Some(tree) => tree.copy_part(&name.to_string_lossy()),
        None => part_beside(&to),
    };

    // S2b: only a file that can be read at all and says no length is mended; one ffprobe cannot
    // open is copied as it is and the step that needs it says so.
    let mut unrepaired = None;
    if let Some(repair) = repair.filter(|repair| header(repair, file) == Header::NoLength) {
        // A mended copy is a different size from its recording, so the size rule above never
        // matches it: it is recognised by having the length its recording lacks.
        if to.exists() && !newer(file, &to) && matches!(header(repair, &to), Header::Seconds(_)) {
            return Ok(Copied::Skipped);
        }
        // ffmpeg picks the container by the name's ending, so the mended file is written beside
        // the `.part` under the recording's own ending.
        let mended = mended_beside(&part, file);
        let asked = repair_args(file, &mended);
        let flags: Vec<&str> = asked.iter().map(String::as_str).collect();
        let ran = (repair.run)(repair.ffmpeg, &flags);
        match (ran, header(repair, &mended)) {
            (Ok(_), Header::Seconds(seconds)) => {
                fs::rename(&mended, &to).map_err(|err| format!("{}: {err}", to.display()))?;
                return Ok(Copied::Repaired { seconds });
            }
            (ran, _) => {
                let _ = fs::remove_file(&mended);
                unrepaired = Some(match ran {
                    Err(err) => err.lines().last().unwrap_or("ffmpeg failed").to_string(),
                    Ok(_) => "the file written again has no length either".to_string(),
                });
            }
        }
    }

    let mut in_ =
        BufReader::new(File::open(file).map_err(|err| format!("{}: {err}", file.display()))?);
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
    Ok(match unrepaired {
        Some(why) => Copied::Unrepaired(why),
        None => Copied::Copied,
    })
}

/// Whether the recording was written after the copy: then the copy is of an older file.
fn newer(file: &Path, copy: &Path) -> bool {
    let stamp = |path: &Path| fs::metadata(path).and_then(|meta| meta.modified()).ok();
    match (stamp(file), stamp(copy)) {
        (Some(file), Some(copy)) => file > copy,
        _ => true,
    }
}

/// `<part>.<the recording's ending>`: in flight like the `.part`, and named so ffmpeg knows the container.
fn mended_beside(part: &Path, file: &Path) -> PathBuf {
    let mut mended = part.as_os_str().to_os_string();
    mended.push(extension(file));
    PathBuf::from(mended)
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
