//! §05-cut#1-screen / §06-effects#2 — the band's PICTURE ROWS, split out of `cut_tracks` (which is at its
//! size budget). Everything here answers one question: which extracted frame of a recording stands where on the
//! row, and what pixels it may spend.
//!
//! Two rules shape it. §1/§F1.6 says the grid is Prepare's own (`intervalSeconds`, restarted at each scene
//! change) and "the band has a picture at every zoom", so the spacing is MEASURED from the frame names rather
//! than assumed. §7 says a draw must never block on a decode, so the live page decodes in a worker (inventory §D:
//! "decoded in a worker in batches of 6 at the row height"): a cell paints the frame once it is decoded, and the
//! slate placeholder until then, and each landed batch repaints the band. A snapshot process decodes in the draw
//! instead ([`allow_frame_decoding`]): it has one picture to write and nothing waiting on it. The first port drew
//! placeholders on the live page and never decoded at all, so the app's tape had no pictures.
//!
//! The row's bottom belongs to the wave strip (`Rows::wave_top`), so a cell covers the FILM part only; who owns
//! which pixels is decided once, here and in `cut_wave_rows::draw_row_waves`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;

use gtk4::prelude::*;

use crate::ui::cut_colours::{FILM_GROUND, SLATE};
use crate::{clock, ui::cut_tracks};

/// Inventory §D's `thumbBatch 6`: the live tape's thumbnails are "decoded in a worker in batches of 6 at the row
/// height". The worker takes up to this many waiting frames, decodes them, and hands them back together, so the
/// band repaints once per batch rather than once per frame.
pub const THUMB_BATCH: usize = 6;

/// The first frame's own instant — the row's zero, which [`path_x`] subtracts. `0.0` when the list has nothing to
/// measure, which leaves an unparsable name where it always landed: at the row's start.
pub fn first_second(paths: &[std::path::PathBuf]) -> f64 {
    paths
        .first()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .and_then(clock::read_frame)
        .unwrap_or(0.0)
}

thread_local! {
    /// §7: "a draw must never block on a decode", so the LIVE page's picture row draws placeholder cells and
    /// reads no pixels at all. A headless snapshot is not that page: it has one frame to write, its frames are
    /// already on disk, and nothing waits on the decode — so `snapshot_lane`/`snapshot_effects` set this once and
    /// say so at their own call site. Set-only because a snapshot process is one-shot: there is no second page to
    /// return to a non-decoding state.
    static DECODE_FRAMES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Let THIS process's band decodes happen. Only a snapshot bench calls it; a live page never does, which is what
/// keeps §7 true where it matters (see [`DECODE_FRAMES`]).
pub fn allow_frame_decoding() {
    DECODE_FRAMES.with(|flag| flag.set(true));
}

pub(crate) fn frame_decoding_allowed() -> bool {
    DECODE_FRAMES.with(|flag| flag.get())
}

/// The picture rows: the filmed ground of each recorded row, then one thumbnail per row rather than one per
/// frame. §1 says the band has a picture at every zoom, and the prototype's gap between frames is its own
/// frame spacing, not something the band should reproduce as black; one representative frame per row is what
/// a shot can be compared against. Row 0's frames are the ones shown, which is the row the preview watches.
pub fn draw_picture_rows(
    cr: &cairo::Context,
    scene: &cut_tracks::Scene,
    rows: cut_tracks::Rows,
    tape: f64,
) {
    for run in scene.runs() {
        let (x0, width) = cut_tracks::band_span_px(scene, run.0, run.1);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        cut_tracks::fill(
            cr,
            FILM_GROUND,
            x0,
            rows.pictures,
            width,
            rows.row_h * scene.row_count.max(1) as f64,
            1.0,
        );
    }
    // A film strip: each recording's stretch of its row tiled with cells at the picture's own shape (the row's
    // height × 16:9), each showing the frame at the second the cell stands on. The second comes from the band's
    // own span (folds included), and the frame from its name: a frame is named on its FILE's clock, so the file's
    // first frame plus the second inside the file is the name looked up, the same rule the preview uses
    // (`cut_picture::refresh`). Cells one frame-step wide (4 px at 1 s and 4 px/s), stretched to the row's
    // height, read as a barcode; and only the first recording's list used to be drawn at all.
    // The cell is the FILM part of the row: the wave strip owns its bottom (`Rows::wave_top`), and drawing under
    // it would have the two painters disagree about who owns those pixels — the strip is painted next.
    let film_h = (rows.row_h - cut_tracks::row_wave_px()).max(1.0);
    let cell_w = (film_h * 16.0 / 9.0).max(8.0);
    let span = cut_tracks::band_span(scene);
    // The live page cuts each cell's picture from the recording itself on the fly (`cut_film_cache`), on a grid
    // that follows the zoom; Prepare's frames stand in until it lands. A snapshot (which decodes in its draw and has
    // one picture to write) paints Prepare's frames only, so its pictures do not depend on a background cut.
    let on_the_fly = !frame_decoding_allowed();
    let step = crate::ui::cut_film_cache::step_for(cell_w / scene.pps.max(1e-6));
    let no_frames: Vec<PathBuf> = Vec::new();
    for (index, recording) in scene.recordings.iter().enumerate() {
        let paths = scene
            .thumbs
            .iter()
            .find(|(base, _)| *base == recording.base)
            .map_or(&no_frames, |(_, paths)| paths);
        let file = if on_the_fly {
            crate::ui::cut_record_play::recording_file(&recording.base)
        } else {
            None
        };
        if paths.is_empty() && file.is_none() {
            continue;
        }
        let top = rows.pictures + scene.row_of(index) as f64 * rows.row_h;
        let zero = first_second(paths);
        let (x0, x1) = (
            cut_tracks::band_x(scene, recording.start),
            cut_tracks::band_x(scene, recording.end),
        );
        // From the first cell that reaches the scroll: cells left of it are off the band.
        let skipped = ((scene.origin - x0).max(0.0) / cell_w).floor();
        let mut x = x0 + skipped * cell_w;
        while x < x1 && x < tape {
            let width = cell_w.min(x1 - x);
            let t = span.t_at(x + width / 2.0);
            let cut = file.as_ref().and_then(|file| {
                let second = crate::ui::cut_film_cache::snapped(t - recording.start, step);
                crate::ui::cut_film_cache::picture_at(file, second)
            });
            let shown = cut.or_else(|| cut_tracks::frame_near(paths, zero + (t - recording.start)));
            match shown {
                Some(path) => paint_frame_cell(cr, &path, x, top, width, film_h),
                // Nothing cut yet and no Prepare frame: the slate, so the cell reads as waiting, not as black film.
                None => cut_tracks::fill(cr, SLATE, x, top, width, film_h, 1.0),
            }
            x += cell_w;
        }
    }
}

/// One picture cell: the slate placeholder first (so an unreadable or unasked-for file still leaves a visible cell
/// rather than a hole), then — ONLY when this process asked for decoding — the frame's own pixels, decoded AT THE
/// CELL'S SIZE so one pass costs a thumbnail's worth of work rather than a full-frame decode per cell.
fn paint_frame_cell(
    cr: &cairo::Context,
    path: &std::path::Path,
    x: f64,
    top: f64,
    width: f64,
    height: f64,
) {
    cut_tracks::fill(cr, SLATE, x, top, width, height, 1.0);
    if !frame_decoding_allowed() {
        // The live page: the worker's decode if it has landed, else ask for it and keep the placeholder.
        if let Some(surface) = decoded(path, height.max(1.0) as i32) {
            let (sw, sh) = (surface.width() as f64, surface.height() as f64);
            if sw > 0.0 && sh > 0.0 {
                cr.save().ok();
                cr.rectangle(x, top, width, height);
                cr.clip();
                cr.translate(x, top);
                // At the frame's own shape, scaled to the row's height; a cell narrower than the frame (a
                // recording's last one) shows its left part.
                cr.scale(height / sh, height / sh);
                let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                let _ = cr.paint();
                cr.restore().ok();
            }
        }
        return;
    }
    // Whole pixels, at most the cell's own box: gdk-pixbuf scales on the way in, so nothing here holds a 960 px
    // frame in memory or does its own fit maths. NOT aspect-preserved on purpose: a cell is far taller than it is
    // wide, and preserving the shape collapses a thumbnail to a ~10 px strip along the row's top edge — which is
    // how the row read as one flat line. Stretched into the cell, every frame shows the same bands the spec's
    // film strip shows, and each cell still sits at its own second.
    let (cw, ch) = (width.max(1.0) as i32, height.max(1.0) as i32);
    let Ok(pixbuf) = gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(path, cw, ch, false) else {
        return;
    };
    let Some(surface) = frame_raster(&pixbuf) else {
        return;
    };
    let _ = cr.set_source_surface(&surface, x, top);
    let _ = cr.paint();
}

// ---- the live page's decodes: a worker, a cache, a repaint ---------------------------------------

/// One decode asked of the worker: a frame, at a row height.
type ThumbKey = (PathBuf, i32);

/// What the worker hands back: the key, and the frame's premultiplied ARGB32 pixels with their width and height
/// (`None` when the file would not decode). Plain bytes, because a cairo surface may only live on its thread.
type Decoded = Vec<(ThumbKey, Option<(Vec<u8>, i32, i32)>)>;

thread_local! {
    /// Decoded frames by (file, row height): `Some` once landed, `None` for a file that would not decode.
    static THUMBS: RefCell<HashMap<ThumbKey, Option<cairo::ImageSurface>>> = RefCell::new(HashMap::new());
    /// Asked of the worker and not landed yet, so a repaint does not ask again.
    static ASKED: RefCell<HashSet<ThumbKey>> = RefCell::new(HashSet::new());
    /// The worker's inbox; started on the first ask.
    static WORKER: RefCell<Option<mpsc::Sender<ThumbKey>>> = const { RefCell::new(None) };
    /// The band surfaces to repaint when decodes land (see [`note_band`]).
    static BANDS: RefCell<Vec<glib::WeakRef<gtk4::DrawingArea>>> = const { RefCell::new(Vec::new()) };
}

/// How many thumbnails the live page holds decoded, and how many it has asked for and is still waiting on.
pub fn thumbs_for_test() -> (usize, usize) {
    (
        THUMBS.with(|thumbs| thumbs.borrow().values().filter(|t| t.is_some()).count()),
        ASKED.with(|asked| asked.borrow().len()),
    )
}

/// Register a band surface: it is repainted when a batch of decodes lands.
pub fn note_band(band: &gtk4::DrawingArea) {
    BANDS.with(|bands| bands.borrow_mut().push(band.downgrade()));
}

/// Repaint every registered band: a batch of decodes, or of pictures cut on the fly (`cut_film_cache`), landed.
pub fn repaint_bands() {
    BANDS.with(|bands| {
        bands.borrow_mut().retain(|band| match band.upgrade() {
            Some(band) => {
                band.queue_draw();
                true
            }
            None => false,
        })
    });
}

/// The frame at `path` decoded at `height` px tall, if the worker has delivered it; otherwise asks for it once.
fn decoded(path: &std::path::Path, height: i32) -> Option<cairo::ImageSurface> {
    let key: ThumbKey = (path.to_path_buf(), height);
    if let Some(found) = THUMBS.with(|thumbs| thumbs.borrow().get(&key).cloned()) {
        return found;
    }
    if ASKED.with(|asked| asked.borrow_mut().insert(key.clone())) {
        ask_worker(key);
    }
    None
}

/// Hand one decode to the worker, starting it (and the loop that collects its answers) on the first ask.
fn ask_worker(key: ThumbKey) {
    WORKER.with(|worker| {
        let mut worker = worker.borrow_mut();
        if worker.is_none() {
            let (ask, asked) = mpsc::channel::<ThumbKey>();
            let (tell, told) = mpsc::channel::<Decoded>();
            std::thread::spawn(move || {
                // One batch: the next ask, waited for, and up to `THUMB_BATCH - 1` more already waiting.
                while let Ok(first) = asked.recv() {
                    let mut keys = vec![first];
                    while keys.len() < THUMB_BATCH {
                        match asked.try_recv() {
                            Ok(key) => keys.push(key),
                            Err(_) => break,
                        }
                    }
                    let batch: Decoded = keys
                        .into_iter()
                        .map(|key| {
                            // Scaled on the way in, aspect kept, at the row's height: a thumbnail's worth of work.
                            let pixels = gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(
                                &key.0, -1, key.1, true,
                            )
                            .ok()
                            .and_then(|pixbuf| frame_pixels(&pixbuf));
                            (key, pixels)
                        })
                        .collect();
                    if tell.send(batch).is_err() {
                        return;
                    }
                }
            });
            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                let mut landed = false;
                while let Ok(batch) = told.try_recv() {
                    for (key, pixels) in batch {
                        let surface = pixels.and_then(|(bytes, w, h)| surface_of(&bytes, w, h));
                        THUMBS.with(|thumbs| thumbs.borrow_mut().insert(key.clone(), surface));
                        ASKED.with(|asked| asked.borrow_mut().remove(&key));
                        landed = true;
                    }
                }
                if landed {
                    repaint_bands();
                }
                glib::ControlFlow::Continue
            });
            *worker = Some(ask);
        }
        if let Some(worker) = worker.as_ref() {
            // A worker that is gone (it never is while the app runs) leaves the cell on its placeholder.
            let _ = worker.send(key);
        }
    });
}

/// This pixbuf as a cairo ARgb32 surface, its rows copied one at a time so a JPEG whose `rowstride` is padded
/// past `width * n_channels` still lands on cairo's unpadded stride. gdk-pixbuf hands back NON-premultiplied
/// alpha and cairo wants it premultiplied, hence the per-pixel multiply. Same rule as `produce_paint::raster`,
/// kept local because the band owns its own pictures.
fn frame_raster(pixbuf: &gtk4::gdk_pixbuf::Pixbuf) -> Option<cairo::ImageSurface> {
    let (pixels, wide, high) = frame_pixels(pixbuf)?;
    surface_of(&pixels, wide, high)
}

/// The pixbuf's pixels as cairo's premultiplied ARGB32 bytes, unpadded: the half of [`frame_raster`] that may run
/// on the decode worker.
pub(crate) fn frame_pixels(pixbuf: &gtk4::gdk_pixbuf::Pixbuf) -> Option<(Vec<u8>, i32, i32)> {
    let (wide, high) = (pixbuf.width(), pixbuf.height());
    if wide <= 0 || high <= 0 {
        return None;
    }
    let channels = pixbuf.n_channels();
    if channels < 3 {
        return None;
    }
    let src_stride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let out_stride = wide as usize * 4;
    let mut pixels = vec![0u8; out_stride * high as usize];
    for row in 0..high as usize {
        let from = &bytes[row * src_stride..row * src_stride + wide as usize * channels as usize];
        let to = &mut pixels[row * out_stride..(row + 1) * out_stride];
        for column in 0..wide as usize {
            let source = column * channels as usize;
            let target = column * 4;
            to[target] = from[source + 2];
            to[target + 1] = from[source + 1];
            to[target + 2] = from[source];
            let alpha = if channels == 4 { from[source + 3] } else { 255 };
            if alpha < 255 {
                to[target] = (to[target] as u16 * alpha as u16 / 255) as u8;
                to[target + 1] = (to[target + 1] as u16 * alpha as u16 / 255) as u8;
                to[target + 2] = (to[target + 2] as u16 * alpha as u16 / 255) as u8;
            }
            to[target + 3] = alpha;
        }
    }
    Some((pixels, wide, high))
}

/// Unpadded ARGB32 bytes as a cairo surface; the half of [`frame_raster`] that runs on the window's thread.
fn surface_of(pixels: &[u8], wide: i32, high: i32) -> Option<cairo::ImageSurface> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, wide, high).ok()?;
    let stride = surface.stride() as usize;
    let row = wide as usize * 4;
    {
        let mut target = surface.data().ok()?;
        for y in 0..high as usize {
            let from = pixels.get(y * row..(y + 1) * row)?;
            target
                .get_mut(y * stride..y * stride + row)?
                .copy_from_slice(from);
        }
    }
    surface.status().ok()?;
    Some(surface)
}
