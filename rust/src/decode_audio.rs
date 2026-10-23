//! F1.3 S1 — the decode every other Prepare step reads: `voice16k.wav`, mono 16 kHz.
//!
//! Nothing else in the program writes that file. Before this module a ▶ Prepare run reached
//! [`crate::run_stages`]' speech row, found no decoded audio, and said so — the ASR, the aligner, the
//! diarizer and both transcripts were never touched, because everything downstream counts samples of
//! THIS file ([`crate::transcribe::SAMPLE_RATE`] is the rate its word times are divided by). So this
//! is the first leg of the pass, on the same subprocess door as every other ffmpeg call
//! ([`crate::subprocess::run`], which logs the argv before starting it, §8).
//!
//! The output is a real wav (`pcm_s16le`), not the headerless stream the separation split decodes to:
//! §F1.3 names the file `voice16k.wav`, and ffprobe can read a duration out of a wav header, which is
//! how the pass knows the take's length before it asks anyone anything (S2's short-take test).

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::separate_media;
use crate::transcribe;

/// S1's arguments: no picture, 16 kHz, one channel, signed 16-bit samples in a wav.
///
/// The rate is [`transcribe::SAMPLE_RATE`] rather than a literal, because that constant is what turns
/// this file's sample numbers into seconds everywhere after this — writing `16000` here would let the
/// two drift, and a drifted rate shifts every word time by a percentage instead of breaking loudly.
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

/// S1's resume rule: the decode happens unless the file is already there. Its existence is the marker,
/// not its size — an empty recording decodes to a header-only wav, and re-decoding it on every run
/// would cost a full pass over the source for nothing.
pub fn already_decoded(tree: &Tree, lane: &str) -> bool {
    tree.voice16k_wav(lane).exists()
}

/// Decode `source` into `out`. Every failure names a path: "the decode failed" without the file says
/// nothing about which recording to look at (§Failure-is-specific).
///
/// A source with no audio stream fails here with ffmpeg's own sentence, which is the honest answer: the
/// app does not decide whether a file has sound, the tool that reads it does.
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

/// The decoded audio this row needs, making it first if it is not there. Returns the path every later
/// step reads ([`Tree::voice16k_wav`]), so no caller has to know whether work was done here.
///
/// `source` is the recording itself, resolved by the caller through the one path rule; `lane` is the
/// word the session files are named under, which is the row's own lane rather than the file's stem as
/// it happens to sit on disk.
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
