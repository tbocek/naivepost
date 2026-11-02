//! §05-cut#1-screen / §06-effects#2 — the band's PICTURE ROWS, split out of `cut_tracks` (which is
//! at its size budget). Everything here answers one question: which extracted frame of a recording
//! stands where on the row, and what pixels it may spend.

use std::path::PathBuf;
use std::sync::mpsc;

use gtk4::prelude::*;

use crate::ui::cut_colours::{FILM_GROUND, SLATE};
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::{clock, ui::cut_tracks};

/// Inventory §D's `thumbBatch 6`: the live tape's thumbnails are "decoded in a worker in batches of
/// 6 at the row height".
pub const THUMB_BATCH: usize = 6;

pub fn first_second(paths: &[std::path::PathBuf]) -> f64 {
    paths
        .first()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .and_then(clock::read_frame)
        .unwrap_or(0.0)
}

pub fn allow_frame_decoding() {
    CUT_PAGE.with(
        |CutPage {
             decode_frames: flag,
             ..
         }| flag.set(true),
    );
}

pub(crate) fn frame_decoding_allowed() -> bool {
    CUT_PAGE.with(
        |CutPage {
             decode_frames: flag,
             ..
         }| flag.get(),
    )
}

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
    let film_h = (rows.row_h - cut_tracks::row_wave_px()).max(1.0);
    let cell_w = (film_h * 16.0 / 9.0).max(8.0);
    let span = cut_tracks::band_span(scene);
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
                None => cut_tracks::fill(cr, SLATE, x, top, width, film_h, 1.0),
            }
            x += cell_w;
        }
    }
}

/// One picture cell: the slate placeholder first (so an unreadable or unasked-for file still leaves
/// a visible cell rather than a hole), then.
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
        if let Some(surface) = decoded(path, height.max(1.0) as i32) {
            let (sw, sh) = (surface.width() as f64, surface.height() as f64);
            if sw > 0.0 && sh > 0.0 {
                cr.save().ok();
                cr.rectangle(x, top, width, height);
                cr.clip();
                cr.translate(x, top);
                cr.scale(height / sh, height / sh);
                let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                let _ = cr.paint();
                cr.restore().ok();
            }
        }
        return;
    }
    // Whole pixels, at most the cell's own box: gdk-pixbuf scales on the way in, so nothing here
    // holds a 960 px frame in memory or does its own fit maths.
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

pub(crate) type ThumbKey = (PathBuf, i32);

/// What the worker hands back: the key, and the frame's premultiplied ARGB32 pixels with their
/// width and height (`None` when the file would not decode). Plain bytes, because a cairo surface
/// may only live on its thread.
type Decoded = Vec<(ThumbKey, Option<(Vec<u8>, i32, i32)>)>;

pub fn thumbs_for_test() -> (usize, usize) {
    (
        CUT_PAGE.with(|CutPage { thumbs, .. }| {
            thumbs.borrow().values().filter(|t| t.is_some()).count()
        }),
        CUT_PAGE.with(
            |CutPage {
                 picture_rows_asked: asked,
                 ..
             }| asked.borrow().len(),
        ),
    )
}

pub fn note_band(band: &gtk4::DrawingArea) {
    CUT_PAGE.with(|CutPage { bands, .. }| bands.borrow_mut().push(band.downgrade()));
}

pub fn repaint_bands() {
    CUT_PAGE.with(|CutPage { bands, .. }| {
        bands.borrow_mut().retain(|band| match band.upgrade() {
            Some(band) => {
                band.queue_draw();
                true
            }
            None => false,
        })
    });
}

/// The frame at `path` decoded at `height` px tall, if the worker has delivered it; otherwise asks
/// for it once.
fn decoded(path: &std::path::Path, height: i32) -> Option<cairo::ImageSurface> {
    let key: ThumbKey = (path.to_path_buf(), height);
    if let Some(found) = CUT_PAGE.with(|CutPage { thumbs, .. }| thumbs.borrow().get(&key).cloned())
    {
        return found;
    }
    if CUT_PAGE.with(
        |CutPage {
             picture_rows_asked: asked,
             ..
         }| asked.borrow_mut().insert(key.clone()),
    ) {
        ask_worker(key);
    }
    None
}

fn ask_worker(key: ThumbKey) {
    CUT_PAGE.with(|CutPage { worker, .. }| {
        let mut worker = worker.borrow_mut();
        if worker.is_none() {
            let (ask, asked) = mpsc::channel::<ThumbKey>();
            let (tell, told) = mpsc::channel::<Decoded>();
            std::thread::spawn(move || {
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
                        CUT_PAGE.with(|CutPage { thumbs, .. }| {
                            thumbs.borrow_mut().insert(key.clone(), surface)
                        });
                        CUT_PAGE.with(
                            |CutPage {
                                 picture_rows_asked: asked,
                                 ..
                             }| asked.borrow_mut().remove(&key),
                        );
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
            // A worker that is gone (it never is while the app runs) leaves the cell on its
            // placeholder.
            let _ = worker.send(key);
        }
    });
}

/// This pixbuf as a cairo ARgb32 surface, its rows copied one at a time so a JPEG whose `rowstride`
/// is padded past `width * n_channels` still lands on cairo's unpadded stride. gdk-pixbuf hands
/// back NON-premultiplied alpha and cairo wants it premultiplied, hence the per-pixel multiply.
/// Same rule as `produce_paint::raster`, kept local because the band owns its own pictures.
fn frame_raster(pixbuf: &gtk4::gdk_pixbuf::Pixbuf) -> Option<cairo::ImageSurface> {
    let (pixels, wide, high) = frame_pixels(pixbuf)?;
    surface_of(&pixels, wide, high)
}

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
