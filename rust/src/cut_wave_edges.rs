//! §02-services.md §4.1 — no aligner: the cut points come off the waveform.
//!
//! With word timings there is a word edge to aim at; without them there is only the envelope Prepare
//! cached in `cache/waves`. This module turns a dragged band's two ends into seconds the sound supports:
//! an end that already sits in a silence stays put, one near a silence moves to its nearer end, and an
//! end with no silence near it is `None` — "unable to cut between two words of one breath", said rather
//! than guessed at. The rules themselves (`silence_spans`, `cut_points`) are [`crate::degraded`]'s; this
//! is the reading of the cache they run on and the seam the Cut page's ＋ Add calls.
//!
//! A cut here still lands between two known sounds, which is the project rule the aligner path honours
//! through word times: the app never invents a second the audio does not support.

use crate::degraded::{self, CutSource};
use crate::layout::Tree;
use crate::wave;

/// §4.1's threshold in dBFS — a bucket quieter than this is a silence one may cut in. The same floor
/// §4's no-asr silence detector uses (`P.machine.silenceThresholdDb`), so the two routes to a silence
/// call the same audio quiet.
pub const SILENCE_THRESHOLD_DB: f64 = crate::degraded::SILENCE_THRESHOLD_DB;

/// §4.1 / `P.machine.silenceMinSeconds` — a run of quiet shorter than this is the gap between two
/// words, not a place to cut. Same floor the silence detector the ASR path runs uses.
pub const SILENCE_MIN_SECONDS: f64 = crate::degraded::SILENCE_MIN_SECONDS;

/// §4.1's reach: how near a silence a red line has to sit to be moved onto it. The same tolerance the
/// cut pass gives a word edge (`cut_pass::SNAP_TOLERANCE_SECONDS`, 5 s), so the degraded mode moves a
/// cut no further than the aligned one does.
pub const TOLERANCE_SECONDS: f64 = degraded::SNAP_TOLERANCE_SECONDS;

/// One lane's envelope, read the way every other wave reader does: the cache under `cache/waves`,
/// checked against the RECORDING's own stamps so a replaced file reads as a miss.
pub struct Envelope {
    /// The lane key the cache is filed under (the recording's name, extension dropped).
    pub lane: String,
    /// The session second this recording's first peak lands on — the clock Prepare itself uses
    /// ([`crate::fix_transcripts::placement`]), so a peak's second is a session second.
    pub at: f64,
    /// Peaks per second, from the cache header.
    pub hz: f64,
    /// The first channel's peaks; a stereo lane cuts on what the camera heard.
    pub peaks: Vec<u8>,
}

/// Read every footage row's cached envelope, in row order. `sources` is the project's source list;
/// a lane with no cache, a cache that does not match its recording, or a source whose file cannot be
/// stat-ed is left out rather than guessed at — the band then gets `None` ends, which is the mode
/// stating its limitation instead of inventing a cut.
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

/// The lane key a stored path's cache is filed under: the file name with its extension dropped, the
/// same key Prepare files `cache/waves/<lane>.wave` and `words/<lane>.json` under.
fn lane_key(stored: &str) -> &str {
    let name = stored.rsplit('/').next().unwrap_or(stored);
    match name.rfind('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    }
}

/// The recording's own size and mtime; the stamps a cache must match. Taken from the recording, never
/// from the cache file being checked, or the staleness test could not fail.
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
/// its peaks. The lane's own second zero is where its first peak lands, so every span slides by that.
pub fn silences_of(env: &Envelope) -> Vec<(f64, f64)> {
    let peaks: Vec<f64> = env.peaks.iter().map(|peak| f64::from(*peak)).collect();
    degraded::silence_spans(&peaks, env.hz, SILENCE_THRESHOLD_DB, SILENCE_MIN_SECONDS)
        .into_iter()
        .map(|(start, end)| (start + env.at, end + env.at))
        .collect()
}

/// §4.1 — the two ends of a band, each nudged onto the nearer end of the nearest silence on the row's
/// own lane, or `None` when no silence is near. Silences only from `lane`'s recording: a silence on a
/// second camera is not a silence in the row being cut.
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

/// The aligner named in the machine's settings, `""` when there is no config home or none is named —
/// which is the default, and so the waveform mode.
pub fn aligner_from_settings() -> String {
    match crate::settings::from_environment() {
        Some(paths) => crate::settings::read(&paths)
            .map(|conf| conf.align_model)
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// The lane a footage row's band belongs to: the footage sources in row order, nth one. `None` when
/// there is no such row — the caller then leaves the band's ends where they were drawn.
pub fn lane_for_row(sources: &[crate::project::Source], row: i32) -> Option<String> {
    sources
        .iter()
        .filter(|source| source.footage)
        .nth(usize::try_from(row).ok()?)
        .map(|source| lane_key(&source.path).to_string())
}
