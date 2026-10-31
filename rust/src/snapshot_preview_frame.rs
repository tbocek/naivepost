//! The `06-preview` screen's second image: the preview FRAMED, painted on its own at a legible size the way
//! `snapshot_lane` paints the lane on its own.

use std::path::Path;

use gtk4 as gtk;

use crate::cut::Cut;
use crate::ui::cut_preview_frame::{self, Held};

/// The frame's shape in the spec's own picture (`spec/img/06-preview.png` is a 16:9 frame): not a tuning value,
/// just the box the two images are compared at.
const FRAME_W: f64 = 960.0;
const FRAME_H: f64 = 540.0;

/// Is this screen's SUBJECT the framed preview? §06-effects#2's second picture (`spec/img/06-preview.png`) is the
/// frame during the zoom and the caption -- record, dimmed surround, stroked camera rect, fitted caption, a stop's
/// still -- not the page with a blue plate in it, so the frame takes `<name>.png` and the page goes beside it.
pub fn is_frame_subject(screen: &str) -> bool {
    screen == "06-preview"
}

/// Paint the paused preview of `cut` at second `t` into `out`, with `frame` as the picture behind it. Returns the
/// log sentence naming what was drawn, so a wrong staging shows up in the run's output rather than as a blank PNG.
pub fn write_png(
    cut: &Cut,
    t: f64,
    frame: Option<&Path>,
    playing: bool,
    out: &Path,
) -> Result<String, String> {
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, FRAME_W as i32, FRAME_H as i32)
            .map_err(|why| format!("the preview surface: {why}"))?;
    let mut drawn = String::new();
    let mut plate_said = String::new();
    {
        let cr =
            cairo::Context::new(&surface).map_err(|why| format!("the preview context: {why}"))?;
        draw_into(
            &cr,
            cut,
            t,
            frame,
            playing,
            &mut drawn,
            &mut plate_said,
            FRAME_W,
        );
    }
    let mut file = std::fs::File::create(out).map_err(|why| format!("{}: {why}", out.display()))?;
    surface
        .write_to_png(&mut file)
        .map_err(|why| format!("writing the preview: {why}"))?;
    if !plate_said.is_empty() {
        drawn.push_str(&format!(" | plate: {plate_said}"));
    }
    let (file_w, file_h) = crate::snapshot_face::png_size(out);
    // The staged frame's pixels decide the fitted box, exactly as they do inside `draw_into`: same path, same
    // door, so the rect printed here is the rect that was painted.
    let source = frame
        .and_then(|path| gtk::gdk_pixbuf::Pixbuf::from_file(path).ok())
        .map(|pixbuf| (pixbuf.width() as f64, pixbuf.height() as f64))
        .unwrap_or((FRAME_W, FRAME_H));
    let fit = cut_preview_frame::fit_size(source.0, source.1, FRAME_W, FRAME_H);
    let rect = {
        let state = cut_preview_frame::describe_at(
            cut,
            t,
            playing,
            false,
            fit,
            &Held {
                index: None,
                box_px: None,
            },
        );
        cut_preview_frame::camera_rect_px(&cut_preview_frame::observe(&state, None).camera, fit)
    };
    drawn.push_str(&format!(
        " | file {file_w}x{file_h} px on disk, camera rect {:.0},{:.0} {:.0}x{:.0} of the {:.0}x{:.0} fitted picture",
        rect.0, rect.1, rect.2, rect.3, fit.0, fit.1
    ));
    Ok(drawn)
}

fn plate_on_rect(
    cr: &cairo::Context,
    cut: &Cut,
    t: f64,
    rect: (f64, f64, f64, f64),
    room_px: f64,
) -> Option<String> {
    // same door the live panel's refresh uses (`fx_lane::plate_debt_at` + `fx_lane::plate_for`), and its room is
    // measured through `fx_lane::room_px` exactly as the page measures its own plate's.
    let text = crate::fx_lane::plate_for(
        crate::fx_lane::plate_debt_at(&cut.fx, t),
        crate::fx_lane::room_px(room_px, 0.0),
    )?;
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(PLATE_FONT_PX);
    let extents = cr.text_extents(&text).ok()?;
    let box_w = extents.width() + 2.0 * PLATE_PAD_X;
    let box_h = extents.height() + 2.0 * PLATE_PAD_Y;
    let x =
        (rect.0 + rect.2 / 2.0 - box_w / 2.0).clamp(rect.0, (rect.0 + rect.2 - box_w).max(rect.0));
    let y =
        (rect.1 + rect.3 / 2.0 - box_h / 2.0).clamp(rect.1, (rect.1 + rect.3 - box_h).max(rect.1));
    // Dark plate, white words: it sits over the picture, and this reads on either ground.
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.78);
    cr.rectangle(x, y, box_w, box_h);
    let _ = cr.fill();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(
        x + PLATE_PAD_X - extents.x_bearing(),
        y + PLATE_PAD_Y - extents.y_bearing(),
    );
    let _ = cr.show_text(&text);
    Some(text)
}

/// The plate's own type size and padding in the 960x540 frame: big enough to read at the size the two images are
/// compared at, small enough that one line fits inside a zoomed camera rect.
const PLATE_FONT_PX: f64 = 22.0;
const PLATE_PAD_X: f64 = 10.0;
const PLATE_PAD_Y: f64 = 6.0;

/// Paint into `cr` (given to `write_png`, and public so a check can drive the same door the shot uses).
pub fn draw(
    cr: &cairo::Context,
    cut: &Cut,
    t: f64,
    frame: Option<&Path>,
    playing: bool,
    said: &mut String,
) {
    let mut unused = String::new();
    draw_into(cr, cut, t, frame, playing, said, &mut unused, FRAME_W);
}

/// The same paint with a second out-param for the plate's words, so `write_png` can print them.
#[allow(clippy::too_many_arguments)] // five inputs plus two out-params and the plate's room: the plate's room cannot
                                     // be derived here (only the caller knows what plate it is painting), and a struct for it would be one field.
fn draw_into(
    cr: &cairo::Context,
    cut: &Cut,
    t: f64,
    frame: Option<&Path>,
    playing: bool,
    said: &mut String,
    plate_said: &mut String,
    plate_room_px: f64,
) {
    // The recorded picture first, contain-fitted exactly as the panel's `GtkPicture` fits it: every fraction the
    // camera and the caption carry is measured against THAT box, so the fit has to be computed before the ink.
    let picture = frame.and_then(|path| gtk::gdk_pixbuf::Pixbuf::from_file(path).ok());
    let source = picture
        .as_ref()
        .map(|pixbuf| (pixbuf.width() as f64, pixbuf.height() as f64))
        .unwrap_or((FRAME_W, FRAME_H));
    let fit = cut_preview_frame::fit_size(source.0, source.1, FRAME_W, FRAME_H);
    let (ox, oy) = ((FRAME_W - fit.0) / 2.0, (FRAME_H - fit.1) / 2.0);
    // Letterbox black first: the panel's own plate is dark, and painting the ground keeps "dimmed" and "outside
    // the picture" from being the same colour and hiding where the frame ends.
    cr.set_source_rgb(0.0, 0.0, 0.0);
    let _ = cr.paint();
    if let Some(pixbuf) = picture.as_ref() {
        // The file, through the preview's own raster door (path in, cairo surface out) — the same conversion the
        // svg overlay uses, so a staged jpg and a staged png land as the same pixels.
        let raster = frame.and_then(cut_preview_frame::raster_surface_for_test);
        let _ = cr.save();
        cr.translate(ox, oy);
        if let Some(surface) = raster {
            cr.scale(
                fit.0 / surface.width() as f64,
                fit.1 / surface.height() as f64,
            );
            let _ = cr.set_source_surface(&surface, 0.0, 0.0);
            let _ = cr.paint();
        } else {
            // No surface (a path gdk-pixbuf could not hand back): the pixbuf's bytes go through here instead, and
            // the picture still shows rather than leaving the shot a black plate.
            cr.scale(
                fit.0 / pixbuf.width() as f64,
                fit.1 / pixbuf.height() as f64,
            );
            if let Some(surface) = raster_of(pixbuf) {
                let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                let _ = cr.paint();
            }
        }
        let _ = cr.restore();
    }
    // Every mark is painted in the fitted box's px, which is what `fit` means to the painter — so the whole frame
    // is translated once and the two halves can be handed the plain `describe`/`observe` pair they already take.
    let _ = cr.save();
    cr.translate(ox, oy);
    let held = Held {
        index: None,
        box_px: None,
    };
    let frame_state = cut_preview_frame::describe_at(cut, t, playing, false, fit, &held);
    let seen = cut_preview_frame::observe(&frame_state, None);
    if let Some(still) = cut_preview_frame::stop_px(&cut.fx, t, fit) {
        cut_preview_frame::paint_still(cr, still, frame);
    }
    if playing {
        cut_preview_frame::paint_playing(cr, &frame_state, &seen);
    } else {
        cut_preview_frame::paint_paused(cr, &frame_state, &seen);
        // …and the plate that goes WITH the stroked rect (§2: "rect stroked white with a plate"), which only the
        // live panel's label can supply — see [`plate_on_rect`].
        let rect = cut_preview_frame::camera_rect_px(&seen.camera, fit);
        *plate_said = plate_on_rect(cr, cut, t, rect, plate_room_px)
            .unwrap_or_else(|| "none (no debt worth a line here)".to_string());
    }
    let _ = cr.restore();
    *said = format!(
        "{} preview at {:.1}s: dim {}, {} overlay(s), rect {:.1} px, stop still: {}",
        if playing { "playing" } else { "paused" },
        t,
        seen.held.dim_alpha,
        seen.held.overlays.len(),
        seen.held.rect_stroke_px,
        cut_preview_frame::stop_px(&cut.fx, t, fit).is_some(),
    );
}

/// A pixbuf's pixels as a cairo surface. `cut_preview_frame::raster_surface_for_test` is the same door and takes a
/// PATH; this is the fallback for a frame whose file path gdk-pixbuf did not keep (a staged PNG always keeps one,
/// so in practice this is the second door, not the first).
fn raster_of(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> Option<cairo::ImageSurface> {
    let (w, h) = (pixbuf.width(), pixbuf.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let channels = pixbuf.n_channels().max(3) as usize;
    let stride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let has_alpha = pixbuf.has_alpha();
    let out_stride = (w * 4) as usize;
    let mut pixels = vec![0u8; out_stride * h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let from = y * stride + x * channels;
            if from + 2 >= bytes.len() {
                break;
            }
            let (r, g, b) = (
                bytes[from] as u32,
                bytes[from + 1] as u32,
                bytes[from + 2] as u32,
            );
            let a = if has_alpha {
                bytes[from + 3] as u32
            } else {
                255
            };
            let word = (b * a / 255) | ((g * a / 255) << 8) | ((r * a / 255) << 16) | (a << 24);
            let at = y * out_stride + x * 4;
            pixels[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    cairo::ImageSurface::create_for_data(pixels, cairo::Format::ARgb32, w, h, out_stride as i32)
        .ok()
}
