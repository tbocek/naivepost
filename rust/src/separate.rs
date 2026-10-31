//! Voice separation (spec/04-prepare.md F1.2): the 🗣 rows' wish, granted by ▶.
//! The audio server splits a recording into the voice and the rest (they add back to the original); the row becomes two ordinary rows. This module only writes files and edits [`Project::sources`]; chunk boundaries, stem names and loudness arrive as arguments (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use crate::add_sources::is_video;
use crate::layout::{self, Tree};
use crate::project::{Project, Source};
use crate::sources::separable;

/// Audio per request (`P.machine.sepChunkMaxSeconds`): a whole session in one base64 reply would be hundreds of megabytes.
// P.machine.sepChunkMaxSeconds
pub const CHUNK_MAX_SECONDS: f64 = 300.0;

/// S1: what goes up. The separation models are trained on 44.1 kHz stereo, not the ASR's 16 kHz mono.
pub const SAMPLE_RATE: &str = "44100";
pub const CHANNELS: &str = "2";

/// How far either side of a nominal chunk edge the cut may move to find a silence; wider drifts past [`CHUNK_MAX_SECONDS`], narrower cuts mid-word.
pub const SEEK_WINDOW: f64 = 20.0;

/// S6: how far under the mix the rest must sit to be worth a note. §10 files this as `P.eng.sepLopsidedDB` (10) and `params::prepare()` rows it from this constant. The app marks the split and keeps both halves rather than dropping one.
pub const LOPSIDED_DB: f64 = 10.0;

/// S3's mix of "everything that isn't the voice": `normalize=0` so the sum gives the recording back.
pub fn amix(stems: usize) -> String {
    format!("amix=inputs={stems}:normalize=0")
}

/// S1/S2: the pieces one recording is sent up as, each ≤ [`CHUNK_MAX_SECONDS`], each cut moved to the nearest silence inside [`SEEK_WINDOW`]. Nominal edges are fixed, so no piece grows past the ceiling.
pub fn chunk_plan(seconds: f64, silences: &[f64]) -> Vec<(f64, f64)> {
    if seconds <= CHUNK_MAX_SECONDS {
        return vec![(0.0, seconds)];
    }
    // Nominal edges at the ceiling, then each moved to a silence (640 s is 300 + 300 + 40).
    let mut plan = Vec::new();
    let mut from = 0.0;
    while seconds - from > CHUNK_MAX_SECONDS {
        let edge = from + CHUNK_MAX_SECONDS;
        // Only a silence after the last cut and within the ceiling helps; otherwise the nominal edge stands.
        let to = nearest_silence(silences, edge)
            .filter(|silence| *silence > from && *silence - from <= CHUNK_MAX_SECONDS)
            .unwrap_or(edge);
        plan.push((from, to));
        from = to;
    }
    plan.push((from, seconds));
    plan
}

fn nearest_silence(silences: &[f64], edge: f64) -> Option<f64> {
    silences
        .iter()
        .filter(|silence| (**silence - edge).abs() <= SEEK_WINDOW)
        .min_by(|a, b| {
            (**a - edge)
                .abs()
                .partial_cmp(&(**b - edge).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

/// S1's decode: the source at [`SAMPLE_RATE`] Hz, [`CHANNELS`] channels, pcm; not the ASR's 16 kHz mono. Returns the ffmpeg arguments after the input; the caller prepends the output path (F0.5).
pub fn decode_plan() -> Vec<String> {
    vec![
        "-vn".into(),
        "-ar".into(),
        SAMPLE_RATE.into(),
        "-ac".into(),
        CHANNELS.into(),
        "-f".into(),
        "s16le".into(),
    ]
}

/// S3: which returned stem is the voice and which the rest. RoFormer gives `vocals` and `instrumental`; HTDemucs gives four, the rest being the other three mixed with [`amix`]. A model giving neither fails here, not at the render.
pub fn pick_stems(ids: &[String]) -> Result<(String, Vec<String>), String> {
    let is = |id: &str, want: &str| id.eq_ignore_ascii_case(want);
    let voice = ids
        .iter()
        .find(|id| is(id, "vocals") || is(id, "voice"))
        .cloned()
        .ok_or_else(|| format!("stems {} -- none of them is the voice", ids.join(", ")))?;
    if let Some(instrumental) = ids.iter().find(|id| is(id, "instrumental")) {
        return Ok((voice, vec![instrumental.clone()]));
    }
    let rest: Vec<String> = ids.iter().filter(|id| **id != voice).cloned().collect();
    if rest.is_empty() {
        return Err(format!(
            "only {:?} came back -- the recording without the voice is the other half of this, and \
             there is no other half",
            voice
        ));
    }
    Ok((voice, rest))
}

pub fn already_split_log(base: &str) -> String {
    format!(">>> {base} already split")
}

pub fn start_log(base: &str, seconds: f64, parts: usize, model: &str) -> String {
    format!(">>> [{base}] splitting the voice off {seconds:.1} s in {parts} part(s) ({model})")
}

pub fn split_log(base: &str, rest: &str, voice: &str) -> String {
    format!(">>> [{base}] split into {rest} and {voice}")
}

pub fn loudness_log(base: &str, mix: &str, voice: &str, rest: &str) -> String {
    format!(">>> [{base}] the mix averaged {mix}: the voice half {voice}, the rest {rest}")
}

/// S6's warning when the rest sits [`LOPSIDED_DB`] or more under the mix. An unmeasurable level says nothing.
pub fn lopsided(mix: &str, rest: &str) -> Option<String> {
    let gap = mean_db(mix)? - mean_db(rest)?;
    if gap < LOPSIDED_DB {
        return None;
    }
    Some(format!(
        " -- the rest is {gap:.0} dB under the mix: the model heard this recording as voice nearly \
         throughout, so the half named for the room has little in it. Hear the voice half where the \
         game is wanted, or cut from the original instead of the split."
    ))
}

/// The `volumedetect` mean out of a line like `-20.3 dB`; `None` when no number. Public because [`crate::separate_run`] parses through it.
pub fn mean_db(level: &str) -> Option<f64> {
    let digits = level
        .split_whitespace()
        .next()?
        .trim_end_matches('%')
        .to_string();
    digits.parse().ok()
}

pub fn loudness_report(base: &str, mix: &str, voice: &str, rest: &str) -> String {
    format!(
        "{}{}",
        loudness_log(base, mix, voice, rest),
        lopsided(mix, rest).unwrap_or_default()
    )
}

pub struct Outcome {
    pub logs: Vec<String>,
    pub rest: String,
    pub voice: String,
}

/// S6/S7's ending once both halves are on disk: loudness report, "split into" line, and the row turned into two. `logs` is what the caller said getting here and comes back as the whole log for this row.
pub fn finish(
    project: &mut Project,
    tree: &Tree,
    root: &Path,
    index: usize,
    logs: &[String],
    mix: &str,
    mut loudness: impl FnMut(&Path) -> String,
) -> Outcome {
    let base = project
        .sources
        .get(index)
        .map(|source| base_name(&source.path).to_string())
        .unwrap_or_default();
    let stem = base
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&base)
        .to_string();
    let container = if is_video(Path::new(&base)) {
        "mkv"
    } else {
        "wav"
    };
    let rest_path = tree.split_novoice(&stem, container);
    let voice_path = tree.split_voice(&stem);
    let mut logs = logs.to_vec();
    let voice_level = loudness(&voice_path);
    let rest_level = loudness(&rest_path);
    logs.push(loudness_report(&base, mix, &voice_level, &rest_level));
    logs.push(split_log(
        &base,
        &file_name(&rest_path),
        &file_name(&voice_path),
    ));
    let rest = layout::write_path(root, tree.dir(), &rest_path);
    let voice = layout::write_path(root, tree.dir(), &voice_path);
    split_rows(project, index, &rest, &voice);
    Outcome { logs, rest, voice }
}

/// "Both halves already there?" for the run path to ask before any work. A row that IS a half has no live scissors (`sources::separable`); splitting again would make `<base>.split-voice.split-voice.wav`. `Some` means nothing to do and no row split.
pub fn already_split(project: &Project, tree: &Tree, root: &Path, index: usize) -> Option<Outcome> {
    let source = project.sources.get(index)?;
    let base = base_name(&source.path).to_string();
    let stem = base
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&base)
        .to_string();
    let container = if is_video(Path::new(&base)) {
        "mkv"
    } else {
        "wav"
    };
    let rest_path = tree.split_novoice(&stem, container);
    let voice_path = tree.split_voice(&stem);
    if !both_there(&rest_path, &voice_path) {
        return None;
    }
    Some(Outcome {
        logs: vec![already_split_log(&base)],
        rest: layout::write_path(root, tree.dir(), &rest_path),
        voice: layout::write_path(root, tree.dir(), &voice_path),
    })
}

/// S1-S7 for one row. `seconds`/`silences` come from decoding at [`SAMPLE_RATE`]/[`CHANNELS`] (S1) and gap search (S2); `loudness` is ffmpeg's `volumedetect` of a file.
/// The products are written here; F1.2's "project saved" belongs to the caller, so a failed split leaves the session as it was.
#[allow(clippy::too_many_arguments)] // the split's inputs are exactly what §F1.2 S1-S6 names, and a struct here would only rename them
pub fn separate(
    project: &mut Project,
    tree: &Tree,
    root: &Path,
    index: usize,
    seconds: f64,
    silences: &[f64],
    model: &str,
    mut join: impl FnMut(&Path) -> Result<(), String>,
    mut loudness: impl FnMut(&Path) -> String,
) -> Result<Outcome, String> {
    let source = project
        .sources
        .get(index)
        .ok_or_else(|| "no such row to split".to_string())?;
    // Dead scissors are not a request (the row's ✂ is greyed, §4); a stale wish arriving anyway is told so.
    if !separable(source) {
        return Err(format!(
            "{} has no voice left to split off",
            base_name(&source.path)
        ));
    }
    let base = base_name(&source.path).to_string();
    // Halves are named after the source's stem (`lecture.split-voice.wav`) so "both halves already there?" finds them next run.
    let stem = base
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&base)
        .to_string();
    let stored = layout::resolve(root, tree.dir(), &source.path);
    let container = if is_video(Path::new(&base)) {
        "mkv"
    } else {
        "wav"
    };
    let rest_path = tree.split_novoice(&stem, container);
    let voice_path = tree.split_voice(&stem);

    // Skip when both halves exist ([`already_split`], also asked first by the run path).
    if let Some(done) = already_split(project, tree, root, index) {
        return Ok(done);
    }

    // S1: decoded once to 44.1 kHz stereo, every piece of `plan` cut from that decode; the arguments are `decode_plan`, run by [`crate::separate_media::decode`].
    let plan = chunk_plan(seconds, silences);
    let mut logs = vec![start_log(&base, seconds, plan.len(), model)];

    // A stems folder that cannot be made is an error; the wish would be cleared by work that never happened.
    write_halves(&mut join, &rest_path, &voice_path)?;

    let mix = loudness(&stored);
    let voice_level = loudness(&voice_path);
    let rest_level = loudness(&rest_path);
    logs.push(loudness_report(&base, &mix, &voice_level, &rest_level));
    logs.push(split_log(
        &base,
        &file_name(&rest_path),
        &file_name(&voice_path),
    ));

    let rest = layout::write_path(root, tree.dir(), &rest_path);
    let voice = layout::write_path(root, tree.dir(), &voice_path);
    split_rows(project, index, &rest, &voice);
    Ok(Outcome { logs, rest, voice })
}

/// S4/S5: the joined halves are written under `stems/`, named after the source. The bytes arrive through `join`, which writes the file at the given path (a test hands text; ▶ hands ffmpeg).
fn write_halves(
    join: &mut impl FnMut(&Path) -> Result<(), String>,
    rest_path: &Path,
    voice_path: &Path,
) -> Result<(), String> {
    std::fs::create_dir_all(rest_path.parent().unwrap_or(rest_path))
        .map_err(|err| format!("{}: {err}", stems_folder(rest_path)))?;
    join(voice_path)?;
    join(rest_path)
}

fn stems_folder(path: &Path) -> String {
    path.parent().unwrap_or(path).display().to_string()
}

/// S7: the row becomes two: the rest where the file was, and the voice as a track of its own. The rest keeps 🎥 footage, both keep the row's voice, and both have their wish cleared so a second ▶ does not split a voice off a voice.
pub fn split_rows(project: &mut Project, index: usize, rest: &str, voice: &str) {
    let held = project.sources.remove(index);
    let rest_row = Source {
        path: rest.to_string(),
        footage: held.footage,
        narrator: held.narrator,
        sepvoice: false,
        tracks: vec![],
    };
    let voice_row = Source {
        path: voice.to_string(),
        footage: false,
        narrator: held.narrator,
        sepvoice: false,
        tracks: vec![],
    };
    project.sources.insert(index, voice_row);
    project.sources.insert(index, rest_row);
}

fn both_there(rest: &Path, voice: &Path) -> bool {
    rest.exists() && voice.exists()
}

fn file_name(path: &PathBuf) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
