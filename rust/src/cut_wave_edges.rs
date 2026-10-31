//! §02-services.md §4.1 — no aligner: the cut points come off the waveform.

use crate::degraded::{self, CutSource};
use crate::layout::Tree;
use crate::wave;

/// §4.1's threshold in dBFS — a bucket quieter than this is a silence one may cut in.
pub const SILENCE_THRESHOLD_DB: f64 = crate::degraded::SILENCE_THRESHOLD_DB;

/// §4.1 / `P.machine.silenceMinSeconds` — a run of quiet shorter than this is the gap between two
/// words, not a place to cut.
pub const SILENCE_MIN_SECONDS: f64 = crate::degraded::SILENCE_MIN_SECONDS;

/// §4.1's reach: how near a silence a red line has to sit to be moved onto it.
pub const TOLERANCE_SECONDS: f64 = degraded::SNAP_TOLERANCE_SECONDS;

pub struct Envelope {
    pub lane: String,
    pub at: f64,
    pub hz: f64,
    pub peaks: Vec<u8>,
}

/// Read every footage row's cached envelope, in row order.
pub fn read_lanes(tree: &Tree, sources: &[crate::project::Source]) -> Vec<Envelope> {
    let named: Vec<(&str, &str)> = sources
        .iter()
        .map(|source| {
            (
                source.path.rsplit('/').next().unwrap_or(&source.path),
                source.path.as_str(),
            )
        })
        .collect();
    let offsets =
        crate::fix_transcripts::placement(&named.iter().map(|(name, _)| *name).collect::<Vec<_>>());
    named
        .iter()
        .zip(offsets)
        .filter_map(|((name, stored), at)| {
            let (size, mtime) = stat_recording(tree, stored)?;
            let wave = wave::read(tree, lane_key(name), size, mtime).ok()??;
            let peaks = wave.chans.first()?.clone();
            Some(Envelope {
                lane: lane_key(name).to_string(),
                at,
                hz: wave.hz,
                peaks,
            })
        })
        .collect()
}

fn lane_key(stored: &str) -> &str {
    let name = stored.rsplit('/').next().unwrap_or(stored);
    match name.rfind('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    }
}

/// The recording's own size and mtime; the stamps a cache must match. Taken from the recording,
/// never from the cache file being checked, or the staleness test could not fail.
fn stat_recording(tree: &Tree, stored: &str) -> Option<(i64, i64)> {
    let path = match stored.strip_prefix("project:") {
        Some(rel) => tree.dir().join(rel),
        None => std::path::PathBuf::from(stored),
    };
    let meta = std::fs::metadata(&path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((meta.len() as i64, mtime))
}

/// A lane's silences as (start, end) session seconds, from [`crate::degraded::silence_spans`] over
/// its peaks.
pub fn silences_of(env: &Envelope) -> Vec<(f64, f64)> {
    let peaks: Vec<f64> = env.peaks.iter().map(|peak| f64::from(*peak)).collect();
    degraded::silence_spans(&peaks, env.hz, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS)
        .into_iter()
        .map(|(start, end)| (start + env.at, end + env.at))
        .collect()
}

/// §4.1 — the two ends of a band, each nudged onto the nearer end of the nearest silence on the
/// row's own lane, or `None` when no silence is near.
pub fn band_edges(
    envs: &[Envelope],
    lane: &str,
    start: f64,
    end: f64,
) -> (Option<f64>, Option<f64>) {
    let Some(env) = envs.iter().find(|env| env.lane == lane) else {
        return (None, None);
    };
    let silences = silences_of(env);
    let both = degraded::cut_points(&[start, end], &silences, TOLERANCE_SECONDS);
    (both[0], both[1])
}

/// Whether this session's Cut page should snap ＋ Add's ends off the waveform: no aligner named in
/// `aligner`, which is exactly the box §4.1 means (empty = none).
pub fn waveform_mode(aligner: &str) -> bool {
    degraded::cut_source(aligner) == CutSource::Waveform
}

pub fn aligner_from_settings() -> String {
    match crate::settings::from_environment() {
        Some(paths) => crate::settings::read(&paths)
            .map(|conf| conf.align_model)
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// The lane a footage row's band belongs to: the footage sources in row order, nth one.
pub fn lane_for_row(sources: &[crate::project::Source], row: i32) -> Option<String> {
    sources
        .iter()
        .filter(|source| source.footage)
        .nth(usize::try_from(row).ok()?)
        .map(|source| lane_key(&source.path).to_string())
}
