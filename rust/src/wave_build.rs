//! F1.11's missing half: the producer of `cache/waves/<lane>.wave`.

use std::path::Path;

use crate::layout::Tree;
use crate::transcribe;
use crate::wave::{self, Wave};

pub const PEAK_HZ: f64 = 200.0;

pub const SAMPLES_PER_BUCKET: usize = (transcribe::SAMPLE_RATE as usize) / (PEAK_HZ as usize);

fn peak_byte(top: i32) -> u8 {
    (((top as i64 * 255) / 32767).min(255)) as u8
}

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
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect());
        }
        // Even-padded step, and never zero: a malformed length must not spin here.
        at += 8 + len + (len % 2);
    }
    Err(why("no data chunk -- nothing was decoded into it"))
}

/// `count` s16 samples of the same wav from sample `from` on, read in place: a seek, not the whole file. Fewer where the
/// file ends first. F1.11's hiss rule reads a few hundred milliseconds around one edge this way.
pub fn read_wav_span(path: &Path, from: usize, count: usize) -> Result<Vec<i16>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let why = |what: String| format!("{}: {what}", path.display());
    let mut file = std::fs::File::open(path).map_err(|err| why(err.to_string()))?;
    let mut head = [0u8; 12];
    file.read_exact(&mut head)
        .map_err(|err| why(err.to_string()))?;
    if &head[..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err(why("not a RIFF/WAVE file".into()));
    }
    loop {
        let mut chunk = [0u8; 8];
        file.read_exact(&mut chunk)
            .map_err(|_| why("no data chunk -- nothing was decoded into it".into()))?;
        let len = u64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]));
        if &chunk[..4] == b"data" {
            let have = (len / 2) as usize;
            let from = from.min(have);
            let count = count.min(have - from);
            file.seek(SeekFrom::Current(from as i64 * 2))
                .map_err(|err| why(err.to_string()))?;
            let mut bytes = vec![0u8; count * 2];
            file.read_exact(&mut bytes)
                .map_err(|err| why(err.to_string()))?;
            return Ok(bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect());
        }
        file.seek(SeekFrom::Current((len + len % 2) as i64))
            .map_err(|err| why(err.to_string()))?;
    }
}

pub(crate) fn stamp(recording: &Path) -> Result<(i64, i64), String> {
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
