//! F1.2's media legs: the ffmpeg work one voice split needs around the separation model.
//!
//! [`crate::separate`] holds the RULES (chunk ceiling, stem choice, loudness, the two products and the
//! row split) and takes a `join` closure it calls to get bytes; [`crate::separate_run`] owns the wire
//! half (upload, `POST /v1/tasks/run`, stem download). This module is the third piece, the one that
//! touches the recording itself, and it lives apart because `window.rs`-sized files may not grow here
//! and because these are four separate commands with four separate reasons to fail:
//!
//! * S1 — decode the source to 44.1 kHz stereo pcm ([`crate::separate::decode_plan`]);
//! * S2 — find the silences the chunk edges move to ([`silences`], same `-35 dB`/`0.4 s` pair
//!   [`crate::asr`] cuts its ASR pieces on, so one definition of "quiet" serves both);
//! * S3 — cut one piece out of that decode ([`crate::asr::cut_plan`]) and mix several stems into one
//!   rest ([`mix_filter`], which is [`crate::separate::amix`] as a real `filter_complex`);
//! * S4 — join the parts back, and for a video mux the rest with the picture copied and FLAC sound.
//!
//! Every command goes through [`crate::subprocess::run`], which logs the argv before starting it (§8),
//! so nothing here prints a command line of its own.

use std::path::{Path, PathBuf};

use crate::layout::Tree;
use crate::run_stages;
use crate::separate;

/// §inventory/prepare.md B.1: the split's scratch folder, `<base>.work` beside the products under
/// `stems/`. Removed on entry (a run killed mid-split leaves junk) and on success only — after a
/// failure the parts are what a person can look at, and the resume rule reads them.
pub fn work_dir(tree: &Tree, lane: &str) -> PathBuf {
    tree.stems_dir().join(format!("{lane}.work"))
}

/// The decoded audio a split cuts its chunks from: `<lane>.pcm` in the work dir, named by one rule so
/// `split` and any later reader agree on where S1's output sits.
pub fn decoded_file(work: &Path, lane: &str) -> PathBuf {
    work.join(format!("{lane}.pcm"))
}

/// Clear a work dir that an earlier run left behind. Absent is the normal case, not a warning.
pub fn clear_work(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("{}: {err}", dir.display())),
    }
}

/// The ffmpeg to run: the configured box when the settings name one, the plain tool name otherwise —
/// the same rule [`crate::produce_exec`] and the loudness leg already use, written once here.
pub fn program(configured: &str) -> String {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return run_stages::FFMPEG.to_string();
    }
    trimmed.to_string()
}

/// S1: the source decoded to 44.1 kHz stereo pcm, the format the separation models were trained on.
///
/// Written as `.pcm` rather than `.wav` because `decode_plan` ends in `-f s16le`, which is a headerless
/// raw stream: naming it `.wav` would put a wav extension on bytes no player will open. The seek inside
/// [`cut`] is exact on this format, which is why the chunks can be cut from one decode instead of
/// re-decoding the source per chunk.
pub fn decode(program: &str, source: &Path, out: &Path, log: &mut impl FnMut(&str)) -> Result<PathBuf, String> {
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

/// S2: where the recording is quiet long enough to cut in — ffmpeg's `silencedetect` means of
/// [`crate::asr::QUIET_DB`]/[`crate::asr::QUIET_MIN`], reused rather than restated so one threshold
/// decides both the ASR's cuts and the split's. Each silence is folded to its MIDPOINT, which is the
/// point farthest from either edge ([`crate::asr::Silence::midpoint`]'s reason), and that single number
/// is what [`crate::separate::chunk_plan`] moves a nominal edge to.
///
/// No silences is a normal answer, not an error: it only means the cuts land on the clock. A silencedetect
/// that fails outright is reported, because a silent-looking file whose measurement failed would
/// otherwise be cut mid-word with no trace of why.
///
/// The detector writes its report to ffmpeg's STDERR, while [`crate::subprocess::run`] hands a success
/// back as stdout only — so this leg reads the report itself through [`report`], rather than asking for
/// a tool run twice to get at text the first run already printed.
pub fn silences(program: &str, decoded: &Path) -> Result<Vec<f64>, String> {
    let filter = format!(
        "silencedetect=n={}dB:d={}",
        trim(asr_db()),
        trim(crate::asr::QUIET_MIN)
    );
    let body = report(program, decoded, &filter).map_err(|err| {
        format!("{}: silencedetect failed: {err}", decoded.display())
    })?;
    Ok(midpoints(&body))
}

/// Fold every `silence_start:`/`silence_end:` pair in a detector report to its midpoint, ignoring the
/// hundreds of other lines a long take produces. Split out from [`silences`] so the folding is testable
/// without a tool: the numbers below are exactly what ffmpeg prints.
pub fn midpoints(report: &str) -> Vec<f64> {
    let mut found: Vec<f64> = Vec::new();
    let mut open: Option<f64> = None;
    for line in report.lines() {
        if let Some(at) = silence_start(line) {
            // A second start without an end means the detector restarted; the earlier one is dropped
            // rather than folded, because its length is unknown.
            open = Some(at);
        }
        if let Some(end) = silence_end(line) {
            if let Some(start) = open.take() {
                found.push(crate::asr::Silence { start, end }.midpoint());
            }
        }
    }
    // A recording that ends quiet never prints `silence_end`, and the cut still belongs in that gap:
    // its start is as far from the last sound as anything measured here can say.
    if let Some(start) = open {
        found.push(start);
    }
    found.sort_by(f64::total_cmp);
    found
}

/// Run the detector over a file and hand back what it printed, stdout and stderr together (that is
/// where ffmpeg puts a filter's own report). No log sink: a long take produces hundreds of these lines
/// and the window's log is where a person looks for what happened, not for ffmpeg's chatter.
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
    // §8's subprocess rule says the command line is logged before it starts; `log_line` does that for
    // the legs that go through `run`, and this one prints nothing of its own while it measures.
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

/// One `silence_start: 12.34` line's number, `None` for every other line ffmpeg prints.
fn silence_start(line: &str) -> Option<f64> {
    let after = line.split_once("silence_start:")?.1.trim();
    after.split_whitespace().next()?.parse().ok()
}

/// The matching `silence_end:` number, same shape as [`silence_start`] because ffmpeg prints the pair
/// identically apart from the word.
fn silence_end(line: &str) -> Option<f64> {
    let after = line.split_once("silence_end:")?.1.trim();
    after.split_whitespace().next()?.parse().ok()
}

/// `P.machine.asrQuietDB`, read off the module that owns it so the two stages cannot drift apart.
fn asr_db() -> f64 {
    crate::asr::QUIET_DB
}

/// S3: one chunk cut out of the decode, named by its index so a kept work dir reads as the sequence it
/// was (the same numbering [`crate::asr::part`] uses for its own pieces).
pub fn chunk_name(index: usize) -> String {
    crate::asr::part(index)
}

/// The chunk's file name as the split uses it: `lecture-c00.wav`, not `c00.wav`. That name is what
/// lands in `requests.tsv`'s job column and in the upload's filename header, so a bare `c00.wav` says
/// which chunk it was but not of which recording — and a session with three sources reads as nonsense.
pub fn chunk_file(lane: &str, index: usize) -> String {
    format!("{lane}-{}", chunk_name(index))
}

/// S3: cut `[from, to)` out of the decode. `-ss` before `-i` is [`crate::asr::cut_plan`]'s rule and
/// the reason it is exact here: on the headerless pcm S1 wrote there is no keyframe to be a frame away
/// from.
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

/// S3's mix filter for a reply that sent several stems where the rest should be: each stem becomes an
/// input, [`crate::separate::amix`] mixes them with `normalize=0` (they were one recording a moment ago
/// and must sum back to it), and the result is mapped to the one output.
pub fn mix_filter(stems: usize) -> String {
    let mut filter = String::new();
    for index in 0..stems {
        filter.push_str(&format!("[{index}:a]"));
    }
    format!("{filter}{}", separate::amix(stems))
}

/// S3: mix N downloaded stems into one rest file. Called only when the reply named more than one stem
/// for the rest — a RoFormer answers `vocals` + `instrumental` and needs no mix at all, while an
/// HTDemucs answers four and the room is the sum of three of them.
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

/// S4: the parts of one half joined into the finished file, by the concat DEMUXER rather than the
/// `concat` filter: the parts are already the same format (they came off the same model answering the
/// same request shape), so no re-encode is needed and a copy keeps the bytes exactly as the server
/// returned them. The list file lives in the work dir next to the parts it names.
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

/// S4: a video's rest muxed with the ORIGINAL picture copied and FLAC sound — the footage keeps its
/// frames untouched (a re-encode would cost minutes and quality for nothing) and the audio stays
/// lossless because Cut will splice on it later.
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

/// A number without a tail of zeroes, matching [`crate::asr`]'s own spelling so a logged command line
/// reads the same however it was built.
fn trim(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    format!("{value}")
}
