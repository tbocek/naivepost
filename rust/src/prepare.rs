//! The left-hand frame controls and the two readouts of the Prepare page
//! (§04-prepare#1-screen).
//!
//! There is no Freq control any more: frames come out on the fixed `P.eng.frameGridSeconds` grid (F1.6), the
//! text gate (F1.6 S6) decides where the screen's text changed, and describe (F1.7) sends each change's first
//! frame and one every heartbeat between. The prototype's Freq set both what was extracted and what the model
//! saw; a 1 s Freq sent a slide that stayed up three minutes about 180 times.
//!
//! There is no Style control and no frame-size control in this rewrite. What the
//! prototype's Style dropdown chose is carried by `P.policy.markingPass` and
//! `P.policy.cutMode` (`Project::policy`, defaulting to retakes/model) and derived from
//! the User Context (F0.7); frames are stored at the video's own size and scaled to
//! `P.machine.describeFrameWidth` only on their way to the model.

use std::fs;
use std::time::SystemTime;

use crate::{
    exchanges,
    layout::Tree,
    project::{Project, Source},
    roles,
};

/// The step the cut-stage asks and the upload's pictures name frames at: one a second, on the extraction grid.
/// Fixed: Freq, which once set it, is gone (the text gate decides what describe sees).
pub const MODEL_FRAME_STEP: f64 = 1.0;

/// The extraction grid, `P.eng.frameGridSeconds`.
pub const EXTRACTION_GRID: f64 = 0.25;

/// Transcript lines per fixer request, `P.machine.fixBlockLines`: the divisor behind
/// the "L lines → K fixer" half of the Inputs readout.
pub const FIX_BLOCK_LINES: usize = 25;

/// The Outputs label beside the folder button (§1).
pub const OUTPUTS_LABEL: &str = "Prepare:";

/// What the folder button's tooltip names: this page's three working folders, which are
/// the three things a run of Prepare writes.
pub const OUTPUTS_TIP: &str = "prepare/inputs/ \u{2014} what came out of each file (audio, transcripts, \
frames); prepare/describe/ \u{2014} what was on screen; prepare/transcript/ \u{2014} everything on one \
clock";

/// The Inputs readout with nothing added yet (§04-prepare#6). It says where to go rather
/// than "no sources", because the bottom bar is not where the Add button is.
pub const NO_INPUTS: &str = "no input files \u{2014} add some above";

/// The same sentence's tooltip: what the label's arithmetic counted, which is nothing.
pub const NO_INPUTS_TIP: &str = "(no sources)";

/// An Outputs readout over an empty folder. Not "0 files, 0 B": a step that has never run
/// and one that ran and wrote nothing are different facts, and this is the first of them.
pub const NO_OUTPUTS: &str = "nothing yet";

/// How long ago a minute count is, in the fewest words that still date it (§04-prepare#6's
/// `"newest <ago>"`). The ladder stops counting minutes at an hour and hours at a day
/// because past those the question is which day, not how many.
pub fn ago(minutes: u64) -> String {
    if minutes < 1 {
        return "just now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes} min ago");
    }
    if minutes < 24 * 60 {
        return format!("{} h ago", minutes / 60);
    }
    if minutes < 48 * 60 {
        return "yesterday".to_string();
    }
    format!("{} days ago", minutes / (24 * 60))
}

/// One of the three Outputs readings: how many files and how big, with when the newest was
/// written on hover. The age is a tooltip because three of these sit side by side along the
/// bottom of the page — three full sentences would be a paragraph across it (§04-prepare#6).
///
/// `newest_minutes` is what the caller's walk actually measured; `None` keeps the tooltip
/// empty rather than inventing an age, since "newest just now" about a folder nobody timed
/// is a fact the app does not have.
pub fn outputs_readout(count: usize, bytes: u64, newest_minutes: Option<u64>) -> (String, String) {
    if count == 0 {
        return (NO_OUTPUTS.to_string(), String::new());
    }
    let one = if count == 1 { "file" } else { "files" };
    let label = format!("{count} {one}, {}", exchanges::size_of(bytes as usize));
    let tooltip = match newest_minutes {
        Some(minutes) => format!("newest {}", ago(minutes)),
        None => String::new(),
    };
    (label, tooltip)
}

/// What Prepare has read so far, counted over the whole session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub frames: usize,
    pub lines: usize,
}

impl Counts {
    /// The Inputs row's own sentence (§1), the two arrow pairs reading as work rather
    /// than as file counts because that is what they predict.
    pub fn readout(&self) -> String {
        inputs_readout(self)
    }
}

/// How many vision requests `frames` frames take: [`roles::vision_batches`], four a
/// call (`P.machine.describeFramesPerReq`). Named here so the readout says which
/// parameter it is counting against.
pub fn vision_calls(frames: usize) -> usize {
    roles::vision_batches(frames)
}

/// How many fixer requests `lines` transcript lines take, in blocks of
/// [`FIX_BLOCK_LINES`].
pub fn fixer_calls(lines: usize) -> usize {
    let per_block = FIX_BLOCK_LINES;
    lines / per_block + usize::from(!lines.is_multiple_of(per_block))
}

/// The Inputs readout (§1): "N frames → M vision · L lines → K fixer".
pub fn inputs_readout(counts: &Counts) -> String {
    format!(
        "{} frames \u{2192} {} vision \u{b7} {} lines \u{2192} {} fixer",
        counts.frames,
        vision_calls(counts.frames),
        counts.lines,
        fixer_calls(counts.lines)
    )
}

/// Count what is on disk for this session: frames out of each footage source (only
/// footage has frames — a recording that is only listened to gets none), and the lines
/// of each transcript. A missing file counts as nothing rather than as an error: this
/// row is read before a run has happened, when nothing is there yet.
pub fn count(tree: &Tree, project: &Project) -> Counts {
    let mut counts = Counts::default();
    for source in &project.sources {
        let lane = lane(source);
        if source.footage {
            counts.frames += count_files(&tree.frames_dir(&lane));
        }
        counts.lines += count_lines(&tree.transcript_tsv(&lane));
    }
    counts
}

/// The per-file tooltip: one line per source, in the project's own order, each with its
/// file name. A source with nothing counted still gets its line, so "the run has not
/// started" is visible per file rather than only as a zero total.
pub fn inputs_tip(tree: &Tree, project: &Project) -> String {
    project
        .sources
        .iter()
        .map(|source| {
            let lane = lane(source);
            let frames = if source.footage {
                count_files(&tree.frames_dir(&lane))
            } else {
                0
            };
            let lines = count_lines(&tree.transcript_tsv(&lane));
            let counts = Counts { frames, lines };
            format!("{}: {}", base_name(&source.path), counts.readout())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The Outputs count: what `prepare/` holds. §1 asks for one number beside the folder
/// button, and counting the page's own folder is the one figure that cannot lie about a
/// run that has not happened.
///
/// Kept as its own walk because §1's "Prepare:" + count row (and every test that reads it) counts
/// files; [`outputs_size`] walks the same tree for bytes and stamps, and the two must agree.
pub fn outputs_count(tree: &Tree) -> usize {
    count_files(&tree.prepare_dir())
}

/// The Outputs size pair §6 asks for: the bytes `prepare/` holds and how many minutes ago its newest
/// file was written. Walks the same folder as [`outputs_count`] so the two figures cannot disagree — a
/// readout that counted ten files and sized nine would be worse than no readout.
///
/// `None` for the age when nothing is there or its stamp could not be read: "newest just now" about a
/// folder nobody timed is a fact the app does not have, and [`outputs_readout`] leaves the tooltip empty
/// rather than inventing one.
pub fn outputs_size(tree: &Tree) -> (u64, Option<u64>) {
    let (bytes, newest) = walk_size(&tree.prepare_dir());
    let minutes = newest.map(|stamp| {
        // A stamp in the future (a clock that went backwards, a copied tree) is not an age: say
        // "just now" rather than printing a negative duration.
        let age = SystemTime::now()
            .duration_since(stamp)
            .unwrap_or_default()
            .as_secs();
        age / 60
    });
    (bytes, minutes)
}

/// `(bytes, newest mtime)` under `dir`, recursively — the twin of [`count_files`], same folder, same
/// tolerance for a folder that is not there (nothing to add).
fn walk_size(dir: &std::path::Path) -> (u64, Option<SystemTime>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (0, None);
    };
    let mut bytes = 0u64;
    let mut newest: Option<SystemTime> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let (sub_bytes, sub_newest) = walk_size(&path);
            bytes += sub_bytes;
            newest = match (newest, sub_newest) {
                (Some(held), Some(found)) => Some(held.max(found)),
                (held, found) => held.or(found),
            };
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        bytes += meta.len();
        newest = match newest {
            Some(held) => Some(held.max(meta.modified().unwrap_or(held))),
            None => meta.modified().ok(),
        };
    }
    (bytes, newest)
}

/// The folder name Prepare files a source's work under: the file's name without its
/// extension, which is also what §1's clash rule makes unique across sources.
fn lane(source: &Source) -> String {
    let name = base_name(&source.path);
    match name.rfind('.') {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

/// The file name for a tooltip: the middle goes when there is no room, but the
/// extension never does — `.mkv` against `.wav` is what distinguishes two rows.
fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Files under `dir`, counting recursively. A folder that is not there holds nothing,
/// which is the ordinary state before a run rather than an error.
fn count_files(dir: &std::path::Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().fold(0, |count, entry| {
        let path = entry.path();
        if path.is_dir() {
            count + count_files(&path)
        } else {
            count + 1
        }
    })
}

fn count_lines(path: &std::path::Path) -> usize {
    match fs::read_to_string(path) {
        Ok(text) => text.lines().filter(|line| !line.is_empty()).count(),
        Err(_) => 0,
    }
}
