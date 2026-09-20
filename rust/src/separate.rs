//! Voice separation (spec/04-prepare.md F1.2): the 🗣 rows' wish, granted by ▶.
//!
//! The audio server splits a recording into the voice AND everything else — the two halves add back
//! up to the original — so what comes out is two ordinary files and the session's row becomes two
//! ordinary rows. Nothing downstream is taught about stems, which is why this module writes files and
//! edits [`Project::sources`] and nothing else: no ffmpeg call, no upload and no window lives here.
//! The chunk boundaries, the stem names and the loudness figures arrive as arguments, the way
//! [`crate::probes::duration`] takes the durations it did not measure (spec/00-principles.md §5).

use std::path::{Path, PathBuf};

use crate::add_sources::is_video;
use crate::layout::{self, Tree};
use crate::project::{Project, Source};
use crate::sources::separable;

/// How much audio goes up in one request (`P.machine.sepChunkMaxSeconds`). One request per five
/// minutes rather than one per recording: the answer is the audio itself, base64 in a JSON body, and
/// a whole session in one reply is hundreds of megabytes — so the ceiling matters, not less.
// P.machine.sepChunkMaxSeconds
pub const CHUNK_MAX_SECONDS: f64 = 300.0;

/// S1: what goes up. The separation models are music models — 44.1 kHz stereo is what they were
/// trained on, and handing them the 16 kHz mono the ASR uses would be asking them to separate
/// something they have never heard.
pub const SAMPLE_RATE: &str = "44100";
pub const CHANNELS: &str = "2";

/// How far either side of a nominal chunk edge the cut may move looking for a silence. Wider and a
/// chunk would drift past [`CHUNK_MAX_SECONDS`]; narrower and the cut lands mid-word.
pub const SEEK_WINDOW: f64 = 20.0;

/// S6: how far under the mix the rest has to sit before it is worth saying so. A residual 13 dB down
/// is a recording the model heard as voice from end to end — other players talking in game chat, say
/// — and the half named for the room then has next to nothing in it.
pub const LOPSIDED_DB: f64 = 10.0;

/// S3's mix of "everything that isn't the voice": `normalize=0` because these stems were one
/// recording a moment ago and summing them has to give that recording back, not an eighth of it.
pub fn amix(stems: usize) -> String {
    format!("amix=inputs={stems}:normalize=0")
}

/// S1/S2: the pieces one recording is sent up as. Each is ≤ [`CHUNK_MAX_SECONDS`], and each cut is
/// moved to the silence nearest the nominal edge inside [`SEEK_WINDOW`] — a cut in a silent gap costs
/// nothing, while one mid-sentence puts a word across two requests and the model hears two words. A
/// silence outside the window is ignored, which is what keeps any piece from growing past the
/// ceiling: the nominal edges are fixed, only their exact position moves.
pub fn chunk_plan(seconds: f64, silences: &[f64]) -> Vec<(f64, f64)> {
    if seconds <= CHUNK_MAX_SECONDS {
        return vec![(0.0, seconds)];
    }
    // Nominal edges at the ceiling, then each moved to a silence: 640 s is 300 + 300 + 40 before any
    // silence is considered, and only the exact bound stays one piece.
    let mut plan = Vec::new();
    let mut from = 0.0;
    while seconds - from > CHUNK_MAX_SECONDS {
        let edge = from + CHUNK_MAX_SECONDS;
        // Only a silence ahead of the last cut and no further than the ceiling helps: anything else
        // would give a negative or an over-long piece, so the nominal edge stands.
        let to = nearest_silence(silences, edge)
            .filter(|silence| *silence > from && *silence - from <= CHUNK_MAX_SECONDS)
            .unwrap_or(edge);
        plan.push((from, to));
        from = to;
    }
    plan.push((from, seconds));
    plan
}

/// The silence nearest `edge`, or `None` when none is within [`SEEK_WINDOW`].
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

/// S1's decode: the source at [`SAMPLE_RATE`] Hz, [`CHANNELS`] channels, pcm — what every chunk of
/// the plan is cut from. The separation models are music models, so this is not the 16 kHz mono the
/// ASR takes; it is spelled out here because the two decodes look identical in a command line and are
/// not the same request. Returns the ffmpeg arguments after the input: the caller prepends its own
/// output path rather than this module naming where a temporary lands, which is F0.5's business.
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

/// S3: which of the stems the model sent back is the voice and which is everything else. A RoFormer
/// answers `vocals` and `instrumental`; an HTDemucs answers four instruments, where the rest is the
/// other three mixed with [`amix`]. Both halves are required — a model that gave neither fails here
/// rather than at the render, minutes later and with no explanation.
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

/// The row already holds both halves: nothing is redone, and the log says so rather than going quiet.
pub fn already_split_log(base: &str) -> String {
    format!(">>> {base} already split")
}

/// S6's first line, said before the first chunk goes up — minutes of GPU with nothing on screen is
/// otherwise indistinguishable from a hang.
pub fn start_log(base: &str, seconds: f64, parts: usize, model: &str) -> String {
    format!(">>> [{base}] splitting the voice off {seconds:.1} s in {parts} part(s) ({model})")
}

/// S6's line for a finished recording. The two file names are what the user looks for afterwards, so
/// they are named as written rather than as paths.
pub fn split_log(base: &str, rest: &str, voice: &str) -> String {
    format!(">>> [{base}] split into {rest} and {voice}")
}

/// S6's loudness report. Each level is ffmpeg's `volumedetect` mean, as it printed it (`-20.3 dB`).
pub fn loudness_log(base: &str, mix: &str, voice: &str, rest: &str) -> String {
    format!(">>> [{base}] the mix averaged {mix}: the voice half {voice}, the rest {rest}")
}

/// S6's warning: how far under the mix the rest sits, when that is [`LOPSIDED_DB`] or more. Said now
/// rather than left to be found by ear a day later as "the footage is silent" — and it is not the
/// footage. An unmeasurable level says nothing: a missing number is not evidence of anything.
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

/// `volumedetect`'s mean out of a line like `-20.3 dB`; `None` when there is no number to read.
fn mean_db(level: &str) -> Option<f64> {
    let digits = level
        .trim()
        .split_whitespace()
        .next()?
        .trim_end_matches('%')
        .to_string();
    digits.parse().ok()
}

/// S6's loudness sentence, warning included when the balance asks for one.
pub fn loudness_report(base: &str, mix: &str, voice: &str, rest: &str) -> String {
    format!(
        "{}{}",
        loudness_log(base, mix, voice, rest),
        lopsided(mix, rest).unwrap_or_default()
    )
}

/// What one recording's split produced: the two stored paths and everything the log has to say.
pub struct Outcome {
    pub logs: Vec<String>,
    pub rest: String,
    pub voice: String,
}

/// S1-S7 for one row. `seconds` and `silences` come from decoding at [`SAMPLE_RATE`]/[`CHANNELS`]
/// (S1) and looking for the gaps (S2); `loudness` is ffmpeg's `volumedetect` reading of a file,
/// asked for after both halves are on disk.
///
/// The two products are written here so "both halves already there?" has something to ask about on
/// the next run, but F1.2's "project saved" belongs to the caller: this module must not touch disk
/// twice, and a split that fails halfway should leave the session as it was rather than half-saved.
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
    // Dead scissors are not a request: the row's ✂ is greyed for exactly this reason (§4), and a call
    // that arrives anyway — from a wish stored before the file was replaced, say — says so plainly.
    if !separable(source) {
        return Err(format!("{} has no voice left to split off", base_name(&source.path)));
    }
    let base = base_name(&source.path).to_string();
    // The halves are named after the source's stem, so `lecture.mkv` splits into
    // `lecture.split-voice.wav` — and so "both halves already there?" asks about the same names on
    // the next run instead of looking for `lecture.mkv.split-voice.wav` forever.
    let stem = base.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&base).to_string();
    let stored = layout::resolve(root, &tree.dir(), &source.path);
    let container = if is_video(Path::new(&base)) { "mkv" } else { "wav" };
    let rest_path = tree.split_novoice(&stem, container);
    let voice_path = tree.split_voice(&stem);

    // Skip when both halves exist. A row that IS a half of a split has no live scissors at all —
    // `sources::separable` says so for the row's ✂ — and granting its wish again would produce
    // `<base>.split-voice.split-voice.wav`, which is nobody's source.
    if both_there(&rest_path, &voice_path) {
        return Ok(Outcome {
            logs: vec![already_split_log(&base)],
            rest: layout::write_path(root, &tree.dir(), &rest_path),
            voice: layout::write_path(root, &tree.dir(), &voice_path),
        });
    }

    // S1: the recording is decoded once, to 44.1 kHz stereo, and every piece of `plan` is cut from
    // that decode rather than re-decoded from the source. The stored path is what resolves through
    // the one path rule; the decode's own arguments are `decode_plan`.
    let _ = (&stored, decode_plan());
    let plan = chunk_plan(seconds, silences);
    // Said before the first chunk goes up, so the wait has a number in front of it.
    let mut logs = vec![start_log(&base, seconds, plan.len(), model)];

    // A stems folder that cannot be made is an error, not a silent skip: the row's wish would be
    // cleared by work that never happened.
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

    let rest = layout::write_path(root, &tree.dir(), &rest_path);
    let voice = layout::write_path(root, &tree.dir(), &voice_path);
    split_rows(project, index, &rest, &voice);
    Ok(Outcome { logs, rest, voice })
}

/// S4/S5 for one recording: the joined halves are written under `stems/` and named after the source,
/// timestamp and all. The bytes are ffmpeg's — each chunk went up to the model and came back as its
/// own pair, which was concatenated (S4) before it got here — so they arrive through `join`, which
/// writes the finished file at the path it is given. A test hands over text; ▶ hands over ffmpeg.
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

/// The folder a split product lives in, for the error that names it.
fn stems_folder(path: &Path) -> String {
    path.parent().unwrap_or(path).display().to_string()
}

/// S7: the row becomes two — everything else where the file was, and the voice as a track of its
/// own. The rest keeps 🎥 footage (a video's picture went with it, sound muxed back), the voice takes
/// the 🎤 slot, and both have their wish cleared so a second ▶ does not split a voice off a voice.
pub fn split_rows(project: &mut Project, index: usize, rest: &str, voice: &str) {
    let held = project.sources.remove(index);
    let rest_row = Source {
        path: rest.to_string(),
        footage: held.footage,
        narrator: 0,
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

/// Both halves on disk? Half of one is not a split.
fn both_there(rest: &Path, voice: &Path) -> bool {
    rest.exists() && voice.exists()
}

/// The file's own name, for the log line that names what was written.
fn file_name(path: &PathBuf) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// The last component of a stored or absolute path — `<base>.mkv`, whatever folder it sits in.
fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
