//! F1.2's media legs: the ffmpeg work one voice split needs around the separation model.
//!
//! [`crate::separate`] holds the rules and takes a `join` closure; [`crate::separate_run`] owns the wire half.
//! This module touches the recording itself, in four commands:
//!
//! * S1 — decode the source to 44.1 kHz stereo pcm ([`crate::separate::decode_plan`]);
//! * S2 — find the silences the chunk edges move to ([`silences`], the `-35 dB`/`0.4 s` pair [`crate::asr`] uses);
//! * S3 — cut one piece out of that decode ([`crate::asr::cut_plan`]) and mix stems into one rest ([`mix_filter`]);
//! * S4 — join the parts back, and for a video mux the rest with the picture copied and FLAC sound.
//!
//! Every command goes through [`crate::subprocess::run`], which logs the argv (§8).

use std::path::{Path, PathBuf};

use crate::asr::trim_number as trim;
use crate::layout::Tree;
use crate::run_stages;
use crate::separate;

/// §inventory/prepare.md B.1: the split's scratch folder, `<base>.work` beside the products under `stems/`.
/// Removed on entry and on success only: after a failure the parts are what a person can look at, and the
/// resume rule reads them.
pub fn work_dir(tree: &Tree, lane: &str) -> PathBuf {
    tree.stems_dir().join(format!("{lane}.work"))
}

/// The decoded audio a split cuts its chunks from: `<lane>.pcm` in the work dir.
pub fn decoded_file(work: &Path, lane: &str) -> PathBuf {
    work.join(format!("{lane}.pcm"))
}

/// Clear a work dir that an earlier run left behind. Absent is the normal case.
pub fn clear_work(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("{}: {err}", dir.display())),
    }
}

/// The ffmpeg to run: the configured box when the settings name one, else the plain tool name.
pub fn program(configured: &str) -> String {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return run_stages::FFMPEG.to_string();
    }
    trimmed.to_string()
}

/// S1: the source decoded to 44.1 kHz stereo pcm, the format the separation models were trained on.
///
/// `.pcm` rather than `.wav`: `decode_plan` ends in headerless `-f s16le`. The seek inside [`cut`] is exact
/// on it, so the chunks come from one decode.
pub fn decode(
    program: &str,
    source: &Path,
    out: &Path,
    log: &mut impl FnMut(&str),
) -> Result<PathBuf, String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let shown_out = out.display().to_string();
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(),
        source.display().to_string(),
    ];
    args.extend(separate::decode_plan());
    args.push(shown_out.clone());
    let owned: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::run(program, &owned, log)
        .map_err(|err| format!("{}: {err}", out.display()))?;
    if out.exists() {
        return Ok(out.to_path_buf());
    }
    Err(format!("{shown_out}: the decode produced nothing"))
}

/// S2: where the recording is quiet long enough to cut in: `silencedetect` with [`crate::asr::QUIET_DB`]/
/// [`crate::asr::QUIET_MIN`], so one threshold decides both. Each silence is folded to its MIDPOINT, the
/// point farthest from either edge, which [`crate::separate::chunk_plan`] moves a nominal edge to.
///
/// No silences is a normal answer (cuts land on the clock). A silencedetect that fails outright is reported,
/// or a file whose measurement failed would be cut mid-word with no trace of why.
///
/// The detector writes to STDERR while [`crate::subprocess::run`] returns stdout only, so this reads the
/// report itself through [`report`].
pub fn silences(program: &str, decoded: &Path) -> Result<Vec<f64>, String> {
    let filter = format!(
        "silencedetect=n={}dB:d={}",
        trim(asr_db()),
        trim(crate::asr::QUIET_MIN)
    );
    let body = report(program, decoded, &filter)
        .map_err(|err| format!("{}: silencedetect failed: {err}", decoded.display()))?;
    Ok(midpoints(&body))
}

/// S2 for the ASR and the aligner: the quiet stretches themselves, not midpoints. [`crate::asr`]'s cuts and
/// [`crate::align`]'s voiced-second shares need both edges. Same tool, threshold and report as [`silences`].
pub fn spans(program: &str, decoded: &Path) -> Result<Vec<crate::asr::Silence>, String> {
    let filter = format!(
        "silencedetect=n={}dB:d={}",
        trim(asr_db()),
        trim(crate::asr::QUIET_MIN)
    );
    let body = report(program, decoded, &filter)
        .map_err(|err| format!("{}: silencedetect failed: {err}", decoded.display()))?;
    Ok(fold_spans(&body))
}

/// Pair every `silence_start:`/`silence_end:` in a detector report into a span, ignoring the rest of ffmpeg's output.
pub fn fold_spans(report: &str) -> Vec<crate::asr::Silence> {
    let mut found: Vec<crate::asr::Silence> = Vec::new();
    let mut open: Option<f64> = None;
    for line in report.lines() {
        if let Some(at) = silence_start(line) {
            // A second start with no end means the detector restarted; the earlier one is dropped (length unknown).
            open = Some(at);
        }
        if let Some(end) = silence_end(line) {
            if let Some(start) = open.take() {
                found.push(crate::asr::Silence { start, end });
            }
        }
    }
    // A recording that ends quiet never prints `silence_end`; its last gap is left out, as inventing an end
    // would move a cut past the last sound.
    found.sort_by(|a, b| a.start.total_cmp(&b.start));
    found
}

/// Fold every `silence_start:`/`silence_end:` pair in a detector report to its midpoint.
/// Split out of [`silences`] so the folding is testable without a tool.
pub fn midpoints(report: &str) -> Vec<f64> {
    let mut found: Vec<f64> = Vec::new();
    let mut open: Option<f64> = None;
    for line in report.lines() {
        if let Some(at) = silence_start(line) {
            // A second start without an end means the detector restarted; the earlier one is dropped (length unknown).
            open = Some(at);
        }
        if let Some(end) = silence_end(line) {
            if let Some(start) = open.take() {
                found.push(crate::asr::Silence { start, end }.midpoint());
            }
        }
    }
    // A recording that ends quiet never prints `silence_end`, and the cut still belongs in that gap.
    if let Some(start) = open {
        found.push(start);
    }
    found.sort_by(f64::total_cmp);
    found
}

/// Run the detector over a file and hand back what it printed, stdout and stderr together (ffmpeg puts a
/// filter's report on stderr). No log sink: hundreds of lines would drown the window's log.
fn report(program: &str, decoded: &Path, filter: &str) -> Result<String, String> {
    use std::process::{Command, Stdio};
    let args = [
        "-i",
        &decoded.display().to_string() as &str,
        "-af",
        filter,
        "-f",
        "null",
        "-",
    ];
    // §8: the command line is logged before it starts; `log_line` does that for this leg, which prints nothing else.
    let _ = crate::subprocess::log_line(program, &args);
    let out = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| crate::subprocess::failure(program, &args, &err.to_string()))?;
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    if out.status.success() {
        return Ok(combined);
    }
    Err(crate::subprocess::failure(program, &args, &combined))
}

/// One `silence_start: 12.34` line's number, `None` for every other line.
fn silence_start(line: &str) -> Option<f64> {
    let after = line.split_once("silence_start:")?.1.trim();
    after.split_whitespace().next()?.parse().ok()
}

/// The matching `silence_end:` number.
fn silence_end(line: &str) -> Option<f64> {
    let after = line.split_once("silence_end:")?.1.trim();
    after.split_whitespace().next()?.parse().ok()
}

/// `P.machine.asrQuietDB`, read off the module that owns it so the two stages cannot drift apart.
fn asr_db() -> f64 {
    crate::asr::QUIET_DB
}

/// The chunk's file name as the split uses it: `lecture-c00.wav`, not `c00.wav`. The name lands in
/// `requests.tsv`'s job column and the upload's filename header, so it must say which recording it is of.
pub fn chunk_file(lane: &str, index: usize) -> String {
    format!("{lane}-{}", crate::asr::part(index))
}

/// S3: cut `[from, to)` out of the decode. `-ss` before `-i` is [`crate::asr::cut_plan`]'s rule and exact
/// here: the headerless pcm has no keyframes.
pub fn cut(
    program: &str,
    decoded: &Path,
    from: f64,
    to: f64,
    out: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let mut args: Vec<String> = vec!["-y".to_string()];
    args.extend(crate::asr::cut_plan(from, to));
    args.push("-i".to_string());
    args.push(decoded.display().to_string());
    args.push(out.display().to_string());
    let owned: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::run(program, &owned, log)
        .map(|_| ())
        .map_err(|err| format!("{}: {err}", out.display()))?;
    if out.exists() {
        return Ok(());
    }
    Err(format!("{}: the cut produced nothing", out.display()))
}

/// S3's mix filter for a reply that sent several stems where the rest should be: each stem is an input,
/// [`crate::separate::amix`] mixes them with `normalize=0` (they must sum back to one recording).
pub fn mix_filter(stems: usize) -> String {
    let mut filter = String::new();
    for index in 0..stems {
        filter.push_str(&format!("[{index}:a]"));
    }
    format!("{filter}{}", separate::amix(stems))
}

/// S3: mix N downloaded stems into one rest file, only when the reply named more than one stem for the rest
/// (a RoFormer needs no mix; an HTDemucs answers four).
pub fn mix(
    program: &str,
    stems: &[PathBuf],
    out: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    if stems.len() < 2 {
        return Err("mixing needs more than one stem".to_string());
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let mut args: Vec<String> = vec!["-y".to_string()];
    for stem in stems {
        args.push("-i".to_string());
        args.push(stem.display().to_string());
    }
    args.push("-filter_complex".to_string());
    args.push(mix_filter(stems.len()));
    args.push("-map".to_string());
    args.push("[out]".to_string());
    args.push(out.display().to_string());
    let owned: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::subprocess::run(program, &owned, log)
        .map(|_| ())
        .map_err(|err| format!("{}: {err}", out.display()))
}

/// S4: the parts of one half joined by the concat DEMUXER, not the `concat` filter: the parts are already the
/// same format, so a copy keeps the server's bytes. The list file lives in the work dir.
pub fn join_parts(
    program: &str,
    parts: &[PathBuf],
    out: &Path,
    work: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    if parts.is_empty() {
        return Err("nothing to join -- no part came back".to_string());
    }
    if parts.len() == 1 {
        std::fs::copy(&parts[0], out).map_err(|err| format!("{}: {err}", out.display()))?;
        return Ok(());
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let list = work.join("concat.txt");
    let body: String = parts
        .iter()
        .map(|part| format!("file '{}'\n", part.display()))
        .collect();
    std::fs::write(&list, body).map_err(|err| format!("{}: {err}", list.display()))?;
    let args = [
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        &list.display().to_string() as &str,
        "-c",
        "copy",
        &out.display().to_string() as &str,
    ];
    crate::subprocess::run(program, &args, log)
        .map(|_| ())
        .map_err(|err| format!("{}: {err}", out.display()))
}

/// S4: a video's rest muxed with the ORIGINAL picture copied and FLAC sound (no re-encode; lossless
/// because Cut will splice on it later).
pub fn mux_video(
    program: &str,
    source: &Path,
    rest_audio: &Path,
    out: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let args = [
        "-y",
        "-i",
        &source.display().to_string() as &str,
        "-i",
        &rest_audio.display().to_string() as &str,
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-c:v",
        "copy",
        "-c:a",
        "flac",
        &out.display().to_string() as &str,
    ];
    crate::subprocess::run(program, &args, log)
        .map(|_| ())
        .map_err(|err| format!("{}: {err}", out.display()))
}
