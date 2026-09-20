//! The data Prepare writes (spec/04-prepare.md §3 → spec/01-project-and-files.md §1): the `prepare/`
//! tree and the one file that records what was read, `inputs/meta.env`.
//!
//! [`crate::layout`] already spells every path in §1's block, so this module adds only what a path cannot
//! say: which recording a run chose, and what the whole set of files is as data a test can walk.

use std::path::Path;

use crate::add_sources::is_video;
use crate::layout::{self, Tree};
use crate::project::Project;
use crate::requests;
use crate::sources;

/// `SCALE`, as this rewrite spells it.
///
/// §1 lists `INTERVAL, SCALE always`, but §7 makes `frame_scale` prototype-only — "ignored on load, never
/// written" — because frames are kept at the video's own size and [F1.7](spec) scales only on the way to
/// the model. So there is one scale, and the file says what it is rather than carrying a preset the app no
/// longer has: dropping the key would break the scripts §1 writes this file for.
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

/// A number the way §1's file wants it: `1`, not `1.0`. The file is for humans and scripts, and every
/// reader of a `.env` would have to trim the tail itself.
fn number(value: f64) -> String {
    let text = value.to_string();
    match text.strip_suffix(".0") {
        Some(whole) if whole.parse::<f64>().is_ok() => whole.to_string(),
        _ => text,
    }
}

/// S2: the pairs `meta.env` holds — §1's own conditionals, in its own order.
///
/// `INTERVAL` and `SCALE` are always there; the video pair only when there is footage and the audio pair
/// only when narrator 1 resolves. Which footage row is *the* one is not spelled out, so it is the first
/// row with 🎥 ticked whose file is a video: that is the row the rest of Prepare treats as primary, and
/// an unticked video is footage for nobody. Likewise slot 1's holder is the narrator's own microphone
/// ([`sources::narrator_of`]), which is what `AUDIO_*` has always named.
///
/// Paths go through [`layout::write_path`], so a project file keeps §1's one path rule — `project:` for
/// what lives inside it, root-relative under the application root, absolute otherwise.
pub fn meta(project: &Project, root: &Path, tree_dir: &Path) -> Vec<(String, String)> {
    let mut pairs = vec![
        ("INTERVAL".to_string(), number(project.interval)),
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

/// S2: write `inputs/meta.env`. Its existence is what §1 reads as "the sources were read", so a rewrite
/// replaces the file rather than adding to it — the latest read is the only one that means anything.
pub fn write_meta(tree: &Tree, project: &Project, root: &Path) -> Result<(), String> {
    requests::write_meta(tree, &meta(project, root, tree.dir()))
}

/// S1: everything §1's block says Prepare writes, as `(what it is, where under prepare/)`.
///
/// Every path comes from a [`Tree`] accessor, so this list is the contract and its own check — §1 spells
/// the file, `Tree` says where it lives — and one test can walk the whole set instead of it being remembered
/// in prose. The scratch folders are in it because §1 names them as scratch while running (`asr/c00.wav…`,
/// `diar/`); when they are removed is a flow rule (F1.x), not a layout question.
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
        ("the fixed words of a recording", tree.transcript_fixed_tsv(source)),
        ("the fixed words of a microphone", tree.commentary_fixed_tsv(source)),
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
