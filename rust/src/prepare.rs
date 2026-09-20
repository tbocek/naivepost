//! The left-hand frame controls and the two readouts of the Prepare page
//! (§04-prepare#1-screen).
//!
//! Freq is `P.project.frameInterval`: how often a frame goes to the describe model,
//! counted from each scene change. It no longer sets extraction — frames come out on
//! the fixed `P.eng.frameGridSeconds` grid restarted at every scene change (F1.6), so
//! the Cut timeline has a picture down to its deepest zoom, and Freq only decides
//! which of those frames the model sees (F1.7). That distinction is the whole reason
//! the two constants below are separate numbers: changing Freq moves what the model is
//! shown and must never move what is on disk.
//!
//! There is no Style control and no frame-size control in this rewrite. What the
//! prototype's Style dropdown chose is carried by `P.policy.markingPass` and
//! `P.policy.cutMode` (`Project::policy`, defaulting to retakes/model) and derived from
//! the User Context (F0.7); frames are stored at the video's own size and scaled to
//! `P.machine.describeFrameWidth` only on their way to the model.

use std::fs;

use crate::{
    layout::Tree,
    project::{self, Project, Source},
    roles,
};

/// The Freq stops (§1: 0.25, 0.5, 1s, 2s, 3s, 4s, 5s). `P.project.frameInterval`, whose
/// own bounds are [`project::INTERVAL_MIN`] and [`project::INTERVAL_MAX`].
pub const FREQ_STOPS: [f64; 7] = [0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0];

/// What a project with no opinion sends: one frame to the model a second.
pub const FREQ_DEFAULT: f64 = 1.0;

/// The extraction grid, `P.eng.frameGridSeconds` — fixed, and independent of Freq.
pub const EXTRACTION_GRID: f64 = 0.25;

/// Transcript lines per fixer request, `P.machine.fixBlockLines`: the divisor behind
/// the "L lines → K fixer" half of the Inputs readout.
pub const FIX_BLOCK_LINES: usize = 25;

/// Language's tooltip (§1). The prototype's warning is kept because it is the reason
/// the control exists at all, and "Empty means en" because an empty box is not a
/// missing setting.
pub const LANGUAGE_TIP: &str = "The language the recording is spoken in \u{2014} the wrong one \
transcribes into gibberish. Empty means en.";

/// The Outputs label beside the folder button (§1).
pub const OUTPUTS_LABEL: &str = "Prepare:";

/// What the folder button's tooltip names: this page's three working folders, which are
/// the three things a run of Prepare writes.
pub const OUTPUTS_TIP: &str = "prepare/inputs/ \u{2014} what came out of each file (audio, transcripts, \
frames); prepare/describe/ \u{2014} what was on screen; prepare/transcript/ \u{2014} everything on one \
clock";

/// Freq as a stop prints it: a fraction of a second as the bare number, a second and up
/// with its unit.
pub fn freq_label(secs: f64) -> String {
    if secs < 1.0 {
        return format!("{secs}");
    }
    if secs == secs.trunc() {
        return format!("{}s", secs as i64);
    }
    format!("{secs}s")
}

/// Whether this is one of the stops. Freq is a choice, not a number box: an unlisted
/// interval would be sent to the model as a claim about frames that were never picked.
pub fn freq_is_stop(secs: f64) -> bool {
    FREQ_STOPS.contains(&secs)
}

/// Set Freq, refusing anything off the list and leaving the project untouched then.
pub fn set_freq(project: &mut Project, secs: f64) -> bool {
    if !freq_is_stop(secs) {
        return false;
    }
    project.interval = secs;
    true
}

/// What the Language box shows when it is empty: the language that will actually be used.
pub fn language_label(code: &str) -> String {
    if code.trim().is_empty() {
        return project::Project::default().language;
    }
    code.to_string()
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
    lines / per_block + usize::from(lines % per_block != 0)
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
            let frames = if source.footage { count_files(&tree.frames_dir(&lane)) } else { 0 };
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
pub fn outputs_count(tree: &Tree) -> usize {
    count_files(&tree.prepare_dir())
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
