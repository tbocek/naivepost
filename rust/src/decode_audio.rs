//! F1.3 S1 — the decode every other Prepare step reads: `voice16k.wav`, mono 16 kHz.
//!
//! Nothing else writes that file: the ASR, aligner, diarizer and both transcripts count samples of THIS file ([`crate::transcribe::SAMPLE_RATE`] is the rate its word times are divided by). It is the first leg of the pass, on the same
//! subprocess door as every ffmpeg call ([`crate::subprocess::run`], which logs the argv first, §8).
//!
//! The output is a real wav (`pcm_s16le`), not the headerless stream separation decodes to: ffprobe reads a duration from a wav header, which is how the pass knows the take's length first (S2's short-take test).

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::separate_media;
use crate::transcribe;

/// S1's arguments: no picture, 16 kHz, one channel, signed 16-bit samples in a wav.
///
/// The rate is [`transcribe::SAMPLE_RATE`], not a literal: it turns sample numbers into seconds everywhere after, and a drifted rate would shift every word time by a percentage instead of breaking loudly.
pub fn decode_plan() -> Vec<String> {
    vec![
        "-vn".into(),
        "-ar".into(),
        transcribe::SAMPLE_RATE.to_string(),
        "-ac".into(),
        "1".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
    ]
}

/// S1's resume rule: the decode happens unless the file is already there. Existence is the marker, not size (an empty recording decodes to a header-only wav).
pub fn already_decoded(tree: &Tree, lane: &str) -> bool {
    tree.voice16k_wav(lane).exists()
}

/// Decode `source` into `out`. Every failure names a path (§Failure-is-specific).
///
/// A source with no audio stream fails with ffmpeg's own sentence: the tool that reads it decides whether a file has sound.
pub fn decode(
    configured_ffmpeg: &str,
    source: &Path,
    out: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        source.display().to_string(),
    ];
    args.extend(decode_plan());
    args.push(out.display().to_string());
    let owned: Vec<&str> = args.iter().map(String::as_str).collect();
    // Same rule as every other ffmpeg call site: the configured box when the settings name one.
    crate::subprocess::run(&separate_media::program(configured_ffmpeg), &owned, log)
        .map_err(|err| format!("{}: {err}", out.display()))?;
    if out.exists() {
        return Ok(());
    }
    Err(format!("{}: the decode produced nothing", out.display()))
}

/// The decoded audio this row needs, making it first if absent. Returns the path every later step reads ([`Tree::voice16k_wav`]).
///
/// `source` is the recording, resolved by the caller through the one path rule; `lane` is the row's own lane, not the file's stem as it sits on disk.
pub fn ensure(
    tree: &Tree,
    lane: &str,
    source: &Path,
    configured_ffmpeg: &str,
    log: &mut impl FnMut(&str),
) -> Result<PathBuf, String> {
    let out = tree.voice16k_wav(lane);
    if already_decoded(tree, lane) {
        return Ok(out);
    }
    if !source.exists() {
        return Err(format!(
            "{}: the recording named by the row is not there",
            source.display()
        ));
    }
    decode(configured_ffmpeg, source, &out, log)?;
    Ok(out)
}
