//! §05-cut#1-screen "Tracks": "each row's first audio track as a strip below it". The strip is the row's own
//! sound read off `cache/waves/<base>.wave` (§01 §6) and drawn the way the prototype draws its paired strip
//! (`go/cut_audio.go` `drawWaveCell`): a dim ground where the recording runs, the meter's zero as a baseline,
//! one column per pixel standing up from it on the IEC 60268-18 meter curve, and a darker cap on the skyline.
//!
//! The cache is checked against the RECORDING it was measured from (`wave::read` takes the source's size and
//! mtime), so this finds the source the project names for the row's base and stamps it the way
//! `wave_build::ensure` stamped it. The first port asked with the cache file's own length and no mtime, so every
//! read was a miss and the strip was a flat line. A decoded wave is held per cache file and re-read only when
//! that file changes, so a draw ten times a second while ▶ runs reads no disk.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use crate::ui::cut_tracks::{self, Rows, Scene};
use crate::{audio_task, layout, wave};

/// The strip's voice under the pictures: the same wave turned down (the prototype's `dim`).
const STRIP_ALPHA: f64 = 0.55;
/// Where the recording runs, so an empty stretch and a silent one look different.
const GROUND: (f64, f64, f64) = (0.16, 0.17, 0.2);
/// The meter's zero.
const BASELINE: (f64, f64, f64) = (0.35, 0.6, 1.0);
/// The columns.
const COLUMN: (f64, f64, f64) = (0.29, 0.62, 1.0);
/// The skyline along the columns' tops.
const CAP: (f64, f64, f64) = (0.09, 0.27, 0.52);

thread_local! {
    /// Decoded waves by cache file, with the cache file's mtime they were read at (`None`: not readable).
    static WAVES: RefCell<HashMap<PathBuf, (SystemTime, Option<Rc<wave::Wave>>)>> = RefCell::new(HashMap::new());
    /// Each project's sources by base, with the project file's mtime they were read at.
    static SOURCES: RefCell<HashMap<PathBuf, (SystemTime, HashMap<String, PathBuf>)>> = RefCell::new(HashMap::new());
}

/// The strips of every recording on the band, each under its own row's pictures.
pub fn draw_row_waves(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let tree = tree();
    let span = cut_tracks::band_span(scene);
    let h = cut_tracks::ROW_WAVE_PX;
    for (index, recording) in scene.recordings.iter().enumerate() {
        let top = rows.wave_top(scene.row_of(index));
        let x0 = cut_tracks::band_x(scene, recording.start).max(scene.origin);
        let x1 = cut_tracks::band_x(scene, recording.end).min(tape);
        if x1 <= x0 {
            continue;
        }
        cut_tracks::fill(cr, GROUND, x0, top, x1 - x0, h, STRIP_ALPHA);
        let bot = (top + h - 1.0).round() + 0.5;
        cr.set_source_rgba(BASELINE.0, BASELINE.1, BASELINE.2, 0.35 * STRIP_ALPHA);
        cr.set_line_width(1.0);
        cr.move_to(x0, bot);
        cr.line_to(x1, bot);
        let _ = cr.stroke();
        let Some(wave) = tree
            .as_ref()
            .and_then(|tree| wave_of(tree, &recording.base))
        else {
            continue;
        };
        let Some(peaks) = wave.chans.first() else {
            continue;
        };
        let full = h - 2.0;
        let mut heights: Vec<(f64, f64)> = Vec::new();
        cr.set_source_rgba(COLUMN.0, COLUMN.1, COLUMN.2, STRIP_ALPHA);
        let mut x = x0.floor();
        while x < x1 {
            // The second the pixel stands on, through the folds the band is laid out by, on the file's clock.
            let from = span.t_at(x) - recording.start;
            let to = (span.t_at(x + 1.0) - recording.start).max(from + 1.0 / scene.pps.max(1e-9));
            let level = meter(peak(peaks, wave.hz, from, to));
            if level > 0.0 {
                let column = (level * full).max(1.0);
                cr.rectangle(x, bot - column, 1.0, column);
                heights.push((x, column));
            }
            x += 1.0;
        }
        let _ = cr.fill();
        cr.set_source_rgba(CAP.0, CAP.1, CAP.2, STRIP_ALPHA);
        for (x, column) in heights.into_iter().filter(|(_, column)| *column >= 3.0) {
            cr.rectangle(x, bot - column, 1.0, 1.0);
        }
        let _ = cr.fill();
    }
}

/// The loudest byte between two file-local seconds, as a fraction of full scale. A column covers many buckets
/// zoomed out (the loudest of them: an average would fade the wave as you zoom out) and a part of one zoomed in.
pub fn peak(peaks: &[u8], hz: f64, from: f64, to: f64) -> f64 {
    if peaks.is_empty() || to <= 0.0 {
        return 0.0;
    }
    let first = (from.max(0.0) * hz).floor() as usize;
    let last = ((to * hz).ceil() as usize).max(first + 1).min(peaks.len());
    peaks
        .get(first..last)
        .and_then(|bytes| bytes.iter().max())
        .map(|byte| f64::from(*byte) / 255.0)
        .unwrap_or(0.0)
}

/// The IEC 60268-18 meter curve over -70..0 dBFS, stretched so the quietest byte the envelope can hold (1/255)
/// sits on the strip's floor and full scale reaches its top (`go/cut_audio.go` `iecScale`).
pub fn meter(amp: f64) -> f64 {
    let floor = iec(1.0 / 255.0);
    ((iec(amp) - floor) / (1.0 - floor)).clamp(0.0, 1.0)
}

fn iec(amp: f64) -> f64 {
    if amp <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * amp.log10();
    if db < -70.0 {
        0.0
    } else if db < -60.0 {
        (db + 70.0) * 0.0025
    } else if db < -50.0 {
        (db + 60.0) * 0.005 + 0.025
    } else if db < -40.0 {
        (db + 50.0) * 0.0075 + 0.075
    } else if db < -30.0 {
        (db + 40.0) * 0.015 + 0.15
    } else if db < -20.0 {
        (db + 30.0) * 0.02 + 0.3
    } else {
        ((db + 20.0) * 0.025 + 0.5).min(1.0)
    }
}

/// The project whose caches the band reads: a snapshot's fixture first, else the open project.
fn tree() -> Option<layout::Tree> {
    crate::ui::cut_picture::snapshot_frame_tree()
        .or_else(|| layout::Tree::new(crate::startup::open_dir()).ok())
}

/// `base`'s decoded wave, read once per change of its cache file and checked against its recording.
fn wave_of(tree: &layout::Tree, base: &str) -> Option<Rc<wave::Wave>> {
    let file = tree.wave(base);
    let changed = std::fs::metadata(&file)
        .and_then(|meta| meta.modified())
        .ok()?;
    if let Some(held) = WAVES.with(|waves| {
        waves
            .borrow()
            .get(&file)
            .filter(|(at, _)| *at == changed)
            .map(|(_, wave)| wave.clone())
    }) {
        return held;
    }
    // The recording the project names for the base; a project that names none (a staged shot, whose envelope
    // `snapshot_lane::stage_envelope` folds straight from decoded audio) is stamped against that decoded audio.
    let read = source_of(tree.dir(), base)
        .or_else(|| Some(tree.voice16k_wav(base)))
        .and_then(|source| crate::wave_build::stamp(&source).ok())
        .and_then(|(size, mtime)| wave::read(tree, base, size, mtime).ok().flatten())
        .map(Rc::new);
    WAVES.with(|waves| waves.borrow_mut().insert(file, (changed, read.clone())));
    read
}

/// The recording the project names for `base`: the source whose file stem is the base Prepare files under.
fn source_of(dir: &Path, base: &str) -> Option<PathBuf> {
    let project_file = dir.join("naivepost.json");
    let changed = std::fs::metadata(&project_file)
        .and_then(|meta| meta.modified())
        .ok()?;
    let held = SOURCES.with(|sources| {
        sources
            .borrow()
            .get(dir)
            .filter(|(at, _)| *at == changed)
            .map(|(_, by_base)| by_base.get(base).cloned())
    });
    if let Some(found) = held {
        return found;
    }
    let root = std::env::current_dir().unwrap_or_default();
    let by_base: HashMap<String, PathBuf> = crate::project::load(dir)
        .map(|project| {
            project
                .sources
                .iter()
                .map(|source| {
                    (
                        audio_task::source_lane(&source.path),
                        layout::resolve(&root, dir, &source.path),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let found = by_base.get(base).cloned();
    SOURCES.with(|sources| {
        sources
            .borrow_mut()
            .insert(dir.to_path_buf(), (changed, by_base))
    });
    found
}
