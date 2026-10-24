//! §05-cut#1-screen / §06-effects#2 — the band's PICTURE ROWS, split out of `cut_tracks` (which is at its
//! size budget). Everything here answers one question: which extracted frame of a recording stands where on the
//! row, and what pixels it may spend.
//!
//! Two rules shape it. §1/§F1.6 says the grid is Prepare's own (`intervalSeconds`, restarted at each scene
//! change) and "the band has a picture at every zoom", so the spacing is MEASURED from the frame names rather
//! than assumed. §7 says a draw must never block on a decode, so the live page draws placeholder cells and only
//! a snapshot process opts into reading pixels ([`allow_frame_decoding`]).
//!
//! The row's bottom belongs to the wave strip (`Rows::wave_top`), so a cell covers the FILM part only; who owns
//! which pixels is decided once, here and in `draw_row_wave_strips`.

use crate::{clock, ui::cut_tracks};

/// How many thumbnails one pass of the band takes off the list to show: inventory §D's `thumbBatch 6`
/// ("decoded in a worker in batches of 6 at the row height"). The prototype hands a loader six files at a time
/// so a long row costs six decodes a round and not the row's whole list; this band shows one representative
/// frame per row and reads no pixels at all (a draw must never block on a decode), so the number here bounds
/// how far one pass looks into the frame list — six seconds of tape at Prepare's own 250 ms grid — which is
/// where the prototype's batch and this page's cheap draw meet. §6 lists it, so it is a constant and a row.
pub const THUMB_BATCH: usize = 6;

/// The slice one batch covers: at most [`THUMB_BATCH`] paths starting at `start`, empty past the end. A plain
/// function of a list and an index so the batch's shape can be tested without a window or a decode.
pub fn batch_of(paths: &[std::path::PathBuf], start: usize) -> &[std::path::PathBuf] {
    let from = start.min(paths.len());
    let to = from.saturating_add(THUMB_BATCH).min(paths.len());
    &paths[from..to]
}

/// The seconds between two entries of a row's frame list, MEASURED from the list's own stamps: Prepare's grid is
/// `intervalSeconds` (§10), which is a project setting rather than a constant — a session extracted every 10 s and
/// one extracted every 250 ms both arrive as names, and the only honest answer is the gap the names show. Read
/// through `clock::read_frame`, the same parse the row's x placement uses, so a cell's width and its position come
/// from one rule. A list with nothing to measure (none, or one entry) answers 0, which tells the caller to take the
/// FIRST batch rather than aim at a grid it cannot know.
pub fn frame_step_seconds(paths: &[std::path::PathBuf]) -> f64 {
    let stamp = |path: &std::path::Path| {
        path.file_name()
            .and_then(|n| n.to_str())
            .and_then(clock::read_frame)
    };
    let first = paths.first().and_then(|p| stamp(p));
    let second = paths.get(1).and_then(|p| stamp(p));
    let (Some(first), Some(second)) = (first, second) else {
        return 0.0;
    };
    (second - first).max(0.0)
}

/// A thumbnail's x, on the folded span: frames are named by their second, so the file name is the position.
/// An unparsable name draws at the row's start rather than nowhere.
///
/// The stamp is read by `clock::read_frame` — the same parser [`frame_step_seconds`] measures the grid with — MINUS
/// `first`, the first frame of this row's own list. A name is a wall-clock instant (§B.3
/// `‹YYYY-MM-DD_HH-MM-SS.mmm›.jpg`) while the band's x axis is seconds OF THE SESSION, so the two only agree once
/// the row's own zero is subtracted; a session recorded at 17:25 lays its first cell at 0 either way. Reading
/// `11..19` as `HH:MM:SS` by hand instead (the way this function used to) looks for digits where the separators are,
/// always fails, and every cell fell back to x = 0 — which is how the lane shot's picture row came out one flat
/// band under the gutter.
pub fn path_x(scene: &cut_tracks::Scene, path: &std::path::Path, first: f64) -> f64 {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(clock::read_frame)
        .map(|seconds| cut_tracks::band_x(scene, seconds - first))
        .unwrap_or(0.0)
}

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

fn frame_decoding_allowed() -> bool {
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
            (0.8, 0.8, 0.82),
            x0,
            rows.pictures,
            width,
            rows.row_h * scene.row_count.max(1) as f64,
            1.0,
        );
    }
    let Some((_, paths)) = scene.thumbs.first() else {
        return;
    };
    // §1/§2: "the band has a picture at every zoom", and §2's own picture is a ROW OF PICTURES, not a grey plate
    // with one mark in it. So this pass draws EVERY frame whose second lands inside the visible tape, each at the
    // x its own name stamps (`path_x`), as a cell one grid step wide at the row's height — which is what a thumbnail
    // is: a small picture of that moment, side by side with the next. The pixels come from the staged JPEG through
    // gdk-pixbuf; a decode in a draw is exactly what §7 forbids on the live page, so this only happens where the
    // frames are already on disk and few (a snapshot's own frames are listed by `frame_paths`).
    // When no file can be read, the cell falls back to the slate placeholder the old mark used, so a row never goes
    // blank because a decoder was unhappy.
    let at = (scene.origin / scene.pps).max(0.0);
    let step = frame_step_seconds(paths);
    let first = if step > 0.0 {
        (at / step).floor() as usize
    } else {
        0
    };
    let width = (step * scene.pps).max(2.0);
    let first_second = first_second(paths);
    // The cell is the FILM part of the row: the wave strip owns its bottom (`Rows::wave_top`), and drawing under
    // it would have the two painters disagree about who owns those pixels — the strip is painted next.
    let film_h = (rows.row_h - cut_tracks::ROW_WAVE_PX).max(1.0);
    // §D's `thumbBatch 6` bounds ONE loader round, not the row: a pass keeps taking batches until the cells it is
    // being handed have run past the right edge of the tape, or the row would stop after six frames however wide
    // the view is.
    let mut start = first;
    loop {
        let batch = batch_of(paths, start);
        if batch.is_empty() {
            break;
        }
        let mut past_edge = false;
        for path in batch {
            let x = path_x(scene, path, first_second);
            if x >= tape {
                past_edge = true;
                break;
            }
            if x + width >= 0.0 {
                paint_frame_cell(cr, path, x, rows.pictures, width, film_h);
            }
        }
        if past_edge {
            break;
        }
        start += THUMB_BATCH;
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
    cut_tracks::fill(cr, (0.65, 0.68, 0.72), x, top, width, height, 1.0);
    if !frame_decoding_allowed() {
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

/// This pixbuf as a cairo ARgb32 surface, its rows copied one at a time so a JPEG whose `rowstride` is padded
/// past `width * n_channels` still lands on cairo's unpadded stride. gdk-pixbuf hands back NON-premultiplied
/// alpha and cairo wants it premultiplied, hence the per-pixel multiply. Same rule as `produce_paint::raster`,
/// kept local because the band owns its own pictures.
fn frame_raster(pixbuf: &gtk4::gdk_pixbuf::Pixbuf) -> Option<cairo::ImageSurface> {
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
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, wide, high).ok()?;
    {
        let mut target = surface.data().ok()?;
        if target.len() < pixels.len() {
            return None;
        }
        target[..pixels.len()].copy_from_slice(&pixels);
    }
    surface.status().ok()?;
    Some(surface)
}
