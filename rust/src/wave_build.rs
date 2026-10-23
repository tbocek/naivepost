//! F1.11's missing half: the producer of `cache/waves/<lane>.wave`.
//!
//! [`crate::edges`] can only place an edge on the sound if a wave cache exists to ask, and nothing in
//! the program wrote one — every live call to [`crate::edges::place_edges`] therefore got `None` and
//! the edges landed on aligned word times alone, which is exactly the state the spec calls a gap ("72
//! of the 114 proposed edges sit exactly on an aligned word time"). This module is the measurement that
//! closes it, following the prototype's `buildWave` (gui/cut_audio.go:346-410) because §F1.10/F1.11 say
//! the placement as measured beat every alternative tried and a rewrite must keep it as it is.
//!
//! The samples come from `voice16k.wav`, the file §F1.3 S1 already decodes for every row, rather than
//! from a second ffmpeg pass over the recording: at 16 kHz the fold to the cache's 200 Hz envelope is an
//! exact integer divide ([`SAMPLES_PER_BUCKET`]), so one decode serves both the word times and the
//! envelope, and a row never pays for the same decode twice.

use std::path::Path;

use crate::layout::Tree;
use crate::transcribe;
use crate::wave::{self, Wave};

/// Peaks per second the cache holds — §01 §6's header `hz`, the prototype's `waveHz`. A bucket is the
/// 5 ms §F1.11 counts silence in, which is why this rate and not another.
pub const PEAK_HZ: f64 = 200.0;

/// Samples of the decoded mono audio per envelope bucket: [`transcribe::SAMPLE_RATE`] ÷ [`PEAK_HZ`],
/// exact at 80. Derived rather than written so a change of decode rate moves the fold with it instead
/// of quietly mis-bucketing every envelope by a percentage.
pub const SAMPLES_PER_BUCKET: usize = (transcribe::SAMPLE_RATE as usize) / (PEAK_HZ as usize);

/// The byte a bucket's peak becomes: the loudest absolute sample scaled onto the byte scale the spec
/// names ("a byte = peak × 255 of full scale"). 32767 would otherwise land one short of the top byte.
fn peak_byte(top: i32) -> u8 {
    (((top as i64 * 255) / 32767).min(255)) as u8
}

/// Fold mono s16 samples into one byte per [`SAMPLES_PER_BUCKET`]: the loudest sample of each bucket,
/// absolute value, on the byte scale. Pure — no file, no clock — so the rule can be checked against a
/// hand-written list of samples.
///
/// A final partial bucket is kept: dropping it would shorten every envelope by up to 4.95 ms, and the
/// tail-following rule reads the last buckets of a word's end.
pub fn peaks_from_s16(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() / SAMPLES_PER_BUCKET + 1);
    for bucket in samples.chunks(SAMPLES_PER_BUCKET) {
        let top = bucket
            .iter()
            .map(|s| (s.unsigned_abs()) as i32)
            .max()
            .unwrap_or(0);
        out.push(peak_byte(top));
    }
    out
}

/// The s16 samples of the wav [`crate::decode_audio`] wrote (`pcm_s16le`, mono, 16 kHz).
///
/// Chunks are WALKED for `data` rather than assumed to sit at byte 12: ffmpeg writes a `LIST`/`INFO`
/// block after `fmt ` on some builds, and reading a fixed header would then fold metadata bytes into the
/// envelope — a wrong answer that looks like a quiet take. Anything unreadable, a non-RIFF file, a
/// missing `data` chunk or a truncated sample answers `Err` naming the file: §failure-is-specific, and
/// a zero-length envelope would silently leave every edge on its word time.
pub fn read_wav_samples(path: &Path) -> Result<Vec<i16>, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let why = |what: &str| format!("{}: {what}", path.display());
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(why("not a RIFF/WAVE file"));
    }
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let tag = &bytes[at..at + 4];
        // Chunk length is little-endian and padded to an even count; the data itself stops at the length.
        let len = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let body = at + 8;
        let end = body + len;
        if end > bytes.len() {
            return Err(why("a chunk runs past the end of the file"));
        }
        if tag == b"data" {
            let body = &bytes[body..end];
            if body.len() % 2 != 0 {
                return Err(why(
                    "an odd number of bytes where s16 samples were expected",
                ));
            }
            return Ok(body
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect());
        }
        // Even-padded step, and never zero: a malformed length must not spin here.
        at += 8 + len + (len % 2);
    }
    Err(why("no data chunk -- nothing was decoded into it"))
}

/// A recording's own size and mtime, the two stamps [`crate::wave::read`] checks a cache against. They
/// come from the recording and never from the cache file, because a staleness test that reads its own
/// subject cannot fail (see [`crate::edges::load`]'s note).
fn stamp(recording: &Path) -> Result<(i64, i64), String> {
    let meta =
        std::fs::metadata(recording).map_err(|err| format!("{}: {err}", recording.display()))?;
    let mtime = meta
        .modified()
        .map_err(|err| format!("{}: no modification time ({err})", recording.display()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| {
            format!(
                "{}: modification time before the epoch ({err})",
                recording.display()
            )
        })?
        .as_secs() as i64;
    Ok((meta.len() as i64, mtime))
}

/// Measure one lane's envelope into `cache/waves/<lane>.wave`, unless it is already there.
///
/// The resume marker is the cache itself: stat the recording, ask [`crate::wave::read`] whether a cache
/// applies to THAT size/mtime, and do nothing when it does — no work, no log line, so a run that resumes
/// does not re-measure every take it already measured. When it does not (no file, stale against a
/// replaced recording, unreadable), the decoded `voice16k.wav` is folded and written.
///
/// `source` is the recording itself (the thing stamped), `lane` the key Prepare files everything under —
/// the same key [`crate::joins::session_lanes`] and [`crate::hand_edit`] will later ask the cache with.
/// The announce line goes out BEFORE any work, like every other step's first line.
pub fn ensure(
    tree: &Tree,
    lane: &str,
    source: &Path,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let (size, mtime) = stamp(source)?;
    if let Ok(Some(_)) = wave::read(tree, lane, size, mtime) {
        return Ok(());
    }
    let decoded = tree.voice16k_wav(lane);
    log(&format!(
        ">>> envelope: measuring {lane} (reading {}, {} samples per bucket at {:.0} Hz)",
        decoded.display(),
        SAMPLES_PER_BUCKET,
        PEAK_HZ
    ));
    let samples = read_wav_samples(&decoded)?;
    let peaks = peaks_from_s16(&samples);
    if peaks.is_empty() {
        return Err(format!(
            "{}: the decoded audio holds no samples",
            decoded.display()
        ));
    }
    let wave = Wave {
        hz: PEAK_HZ,
        chans: vec![peaks],
    };
    wave::write(tree, lane, &wave, size, mtime)
}
