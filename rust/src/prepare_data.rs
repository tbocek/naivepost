//! The data Prepare writes (spec/04-prepare.md §3 → spec/01-project-and-files.md §1): the `prepare/`
//! tree and the one file that records what was read, `inputs/meta.env`.
//!
//! [`crate::layout`] spells every path in §1's block; this adds what a path cannot say: which recording a run chose, and the whole set of files as data a test can walk.

use std::path::{Path, PathBuf};

use crate::add_sources::is_video;
use crate::layout::{self, Tree};
use crate::project::Project;
use crate::requests;
use crate::sources;

/// `SCALE`, as this rewrite spells it: §1 lists `INTERVAL, SCALE always`, but §7 makes `frame_scale` prototype-only (frames keep the video's own size, [F1.7](spec) scales on the way to the model),
/// so the file says `native` rather than dropping the key, which would break the scripts §1 writes it for.
pub const SCALES: &str = "native";

/// The keys `meta.env` carries, in §1's order.
pub const META_KEYS: [&str; 6] = [
    "INTERVAL",
    "SCALE",
    "VIDEO_FILE",
    "VIDEO_BASE",
    "AUDIO_FILE",
    "AUDIO_BASE",
];

/// S1: what one recording's row is called in the files — its base name, without the extension.
fn base(stored: &str) -> String {
    let name = stored.rsplit('/').next().unwrap_or(stored);
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].to_string(),
        _ => name.to_string(),
    }
}

/// A number the way §1's file wants it: `1`, not `1.0` (readers of a `.env` would otherwise trim the tail).
fn number(value: f64) -> String {
    let text = value.to_string();
    match text.strip_suffix(".0") {
        Some(whole) if whole.parse::<f64>().is_ok() => whole.to_string(),
        _ => text,
    }
}

/// S2: the pairs `meta.env` holds — §1's own conditionals, in its own order.
///
/// `INTERVAL` and `SCALE` are always there (`INTERVAL` is the extraction grid the frames sit on); the video pair only with footage, the audio pair only when narrator 1 resolves.
/// *The* footage row is the first with 🎥 ticked whose file is a video (the row the rest of Prepare treats as primary); slot 1's holder is the narrator's microphone ([`sources::narrator_of`]).
///
/// Paths go through [`layout::write_path`] (§1's one path rule: `project:` inside the project, root-relative under the application root, absolute otherwise).
pub fn meta(project: &Project, root: &Path, tree_dir: &Path) -> Vec<(String, String)> {
    let mut pairs = vec![
        (
            "INTERVAL".to_string(),
            number(crate::prepare::EXTRACTION_GRID),
        ),
        ("SCALE".to_string(), SCALES.to_string()),
    ];
    let mut stored = |key_file: &str, key_base: &str, path: &str| {
        let resolved = layout::resolve(root, tree_dir, path);
        pairs.push((
            key_file.to_string(),
            layout::write_path(root, tree_dir, &resolved),
        ));
        pairs.push((key_base.to_string(), base(path)));
    };

    if let Some(video) = project
        .sources
        .iter()
        .filter(|source| source.footage)
        .find(|source| is_video(Path::new(source.path.rsplit('/').next().unwrap_or(""))))
    {
        stored("VIDEO_FILE", "VIDEO_BASE", &video.path);
    }
    if let Some(audio) = sources::narrator_of(project, 1) {
        stored("AUDIO_FILE", "AUDIO_BASE", audio);
    }
    pairs
}

/// S2: write `inputs/meta.env`. Its existence is §1's "the sources were read" marker, so a rewrite replaces the file.
pub fn write_meta(tree: &Tree, project: &Project, root: &Path) -> Result<(), String> {
    requests::write_meta(tree, &meta(project, root, tree.dir()))
}

/// The application root `meta.env`'s paths are written against: the folder the session sits in (§F1.1's `startup::session_dir(cwd)`); a caller that knows its root passes it to [`write_meta`].
pub fn default_root() -> PathBuf {
    crate::startup::open_dir()
}

/// §3 (▶ Prepare): stamp `inputs/meta.env` at the start of a run, before any stage reads a source.
///
/// §1 treats this file as the sources-were-read marker. The spec gives no failure rule for a marker, so by Failure-is-specific a `meta.env` that will not write does NOT refuse the press:
/// the sentence comes back for the log. `Ok(None)` is success: S4's own lines already report the inputs.
pub fn stamp_meta(tree: &Tree, project: &Project) -> Option<String> {
    write_meta(tree, project, &default_root())
        .err()
        .map(|error| format!(">>> could not write meta.env -- {error}"))
}

/// S1: everything §1's block says Prepare writes, as `(what it is, where under prepare/)`.
///
/// Every path comes from a [`Tree`] accessor, so this list is the contract and its own check, and one test can walk the set. Scratch folders (`asr/c00.wav…`, `diar/`) are included as §1 names them; when they are removed is a flow rule (F1.x).
pub fn paths(tree: &Tree, source: &str) -> Vec<(&'static str, std::path::PathBuf)> {
    [
        ("the sources were read", tree.meta_env()),
        ("mono 16 kHz audio", tree.voice16k_wav(source)),
        ("words, plain text", tree.transcript_txt(source)),
        ("words with times", tree.transcript_tsv(source)),
        ("the transcript to read", tree.transcript_srt(source)),
        ("ASR word times", tree.words_json(source)),
        ("the chunks asked for", tree.asrchunks_json(source)),
        ("aligned word times", tree.words_aligned_json(source)),
        ("diarized turns", tree.turns_json(source)),
        ("ASR scratch", tree.asr_scratch(source)),
        ("diarization scratch", tree.diar_scratch(source)),
        ("scene changes", tree.scenes_tsv(source)),
        ("the grid and threshold used", tree.frames_marker(source)),
        ("one row per frame", tree.events_tsv(source)),
        ("where describing got to", tree.describe_state(source)),
        ("the frames sent, scaled", tree.llm_frames_dir(source)),
        (
            "the fixed words of a recording",
            tree.transcript_fixed_tsv(source),
        ),
        (
            "the fixed words of a microphone",
            tree.commentary_fixed_tsv(source),
        ),
        ("captions for a video", tree.subtitles_srt(source)),
        ("the merged timeline", tree.session_tsv()),
        ("the timeline as the cut reads it", tree.session_txt()),
        ("how far apart the recordings start", tree.offsets_tsv()),
        ("the marks", tree.retakes_tsv()),
        ("the words of the finished video", tree.final_txt()),
    ]
    .into_iter()
    .collect()
}

/// The same list as paths under `prepare/`, which is what §1's block spells.
pub fn written(tree: &Tree, source: &str) -> Vec<(&'static str, String)> {
    let prepare = tree.prepare_dir();
    paths(tree, source)
        .into_iter()
        .map(|(what, path)| {
            let rel = path
                .strip_prefix(&prepare)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            (what, rel)
        })
        .collect()
}
