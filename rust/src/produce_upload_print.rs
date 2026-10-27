//! F5.6 S7: printing the words onto the picture.
//!
//! The plain picture is what sd.cpp drew (or what a chosen frame gave); the finished `thumbnail.png` is that
//! picture with the marked texts and then the title printed over it. The lettering is a LOCAL cairo draw —
//! §F5.6 says a picked frame costs "no model, no GPU", and decision 34 makes reprinting a title a redraw of
//! letters only, never a second image call. One lettering backs it, [`draw_text_box`] over the Cut page's fitter
//! ([`crate::fx_text::fit`]), so a thumbnail's title and an effect's caption can never disagree about how big
//! those letters get or how they look.

use std::path::Path;

use crate::project::{TextMark, TitleBox};

/// Print the marked texts, each in its own box, and the title in its band over `png_in`, and write the result to
/// `png_out` (the prototype's `drawPubTexts`): one pass from the plain picture, so the two kinds of words always
/// land together and a reprint never prints over letters already there.
///
/// The base is read as PNG because that is what the drawing step stores (`output_format: png`, §F5.6 S6) and
/// the only format cairo reads here. Every box is in fractions of the picture, exactly as the Cut page stores a
/// text effect's box. The first rewrite squeezed the marked texts into slices of the TITLE band and drew white
/// letters with a soft shadow; the prototype prints each text in its own box, in the Text effect's lettering.
pub fn print_words(
    png_in: &Path,
    png_out: &Path,
    texts: &[TextMark],
    title: &str,
    band: &TitleBox,
) -> Result<(), String> {
    let surface = read_png(png_in)?;
    let (wide, high) = (f64::from(surface.width()), f64::from(surface.height()));
    if wide <= 0.0 || high <= 0.0 {
        return Err(format!("{} has no pixels", png_in.display()));
    }
    {
        let context = cairo::Context::new(&surface).map_err(|why| format!("{why}"))?;
        for text in texts.iter().filter(|t| !t.text.trim().is_empty()) {
            let (x, y, w, h) = box_px(text.cx, text.cy, text.wf, text.hf, wide, high);
            draw_text_box(&context, &text.text, x, y, w, h, 1.0);
        }
        if !title.trim().is_empty() {
            let (x, y, w, h) = box_px(band.cx, band.cy, band.wf, band.hf, wide, high);
            draw_text_box(&context, title.trim(), x, y, w, h, 1.0);
        }
    }
    write_png(&surface, png_out)
}

/// A box stored as centre and size fractions, in the pixels of a `wide`×`high` picture.
fn box_px(cx: f64, cy: f64, wf: f64, hf: f64, wide: f64, high: f64) -> (f64, f64, f64, f64) {
    crate::fx_text::Box_ { cx, cy, wf, hf }.to_px(wide, high)
}

/// THE lettering: a Text effect's words in their box, as the prototype's `drawFxText` draws them for the Cut
/// preview, the thumbnail's title and its marked texts alike. Fitted by [`crate::fx_text::fit`] (the largest size
/// where the lines fit the box), bold sans, every line centred on the box's middle; a dark edge round the letters,
/// then the white letters. The edge is [`crate::fx_text::EDGE_STEPS`] copies of the words on a circle of
/// [`crate::fx_text::edge_radius`], drawn opaque into ONE group that is composited once at
/// [`crate::fx_text::EDGE_ALPHA`]: copies painted at that alpha each compound where they overlap and band into
/// several edges round one letter. Both passes run over all the lines, edge then fill, so a descender never lands
/// on the next line's edge.
pub fn draw_text_box(cr: &cairo::Context, text: &str, x: f64, y: f64, w: f64, h: f64, alpha: f64) {
    let (size, lines) = crate::fx_text::fit(text, w, h);
    if size <= 0.0 || lines.is_empty() || alpha <= 0.01 {
        return;
    }
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(size);
    let base = crate::fx_text::baselines(y, h, size, lines.len());
    let mid = x + w / 2.0;
    let d = crate::fx_text::edge_radius(size);
    let left = |line: &str| match cr.text_extents(line) {
        Ok(extents) => mid - extents.x_advance() / 2.0,
        Err(_) => x,
    };
    cr.push_group();
    cr.set_source_rgb(0.0, 0.0, 0.0);
    for (line, &baseline) in lines.iter().zip(base.iter()) {
        if line.trim().is_empty() {
            continue;
        }
        let at = left(line);
        for step in 0..crate::fx_text::EDGE_STEPS {
            let angle = std::f64::consts::TAU * step as f64 / crate::fx_text::EDGE_STEPS as f64;
            cr.move_to(at + d * angle.cos(), baseline + d * angle.sin());
            let _ = cr.show_text(line);
        }
    }
    if cr.pop_group_to_source().is_ok() {
        let _ = cr.paint_with_alpha(crate::fx_text::EDGE_ALPHA * alpha);
    }
    cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
    for (line, &baseline) in lines.iter().zip(base.iter()) {
        if line.trim().is_empty() {
            continue;
        }
        cr.move_to(left(line), baseline);
        let _ = cr.show_text(line);
    }
}

/// Read one PNG into a surface cairo can draw on.
fn read_png(path: &Path) -> Result<cairo::ImageSurface, String> {
    let file = std::fs::File::open(path).map_err(|why| format!("{}: {why}", path.display()))?;
    cairo::ImageSurface::create_from_png(&mut std::io::BufReader::new(file))
        .map_err(|why| format!("{}: {why}", path.display()))
}

/// Write a surface out as PNG, creating the folder if the render has not made it yet.
fn write_png(surface: &cairo::ImageSurface, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    }
    let file = std::fs::File::create(path).map_err(|why| format!("{}: {why}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    surface
        .write_to_png(&mut writer)
        .map_err(|why| format!("{}: {why}", path.display()))
}

/// Make a flat picture of the given size — what the drawing step leaves behind before any lettering. TEST
/// seam only: it stands in for sd.cpp's answer, so its name says who calls it.
pub fn flat_png_for_test(path: &Path, wide: u32, high: u32, shade: f64) -> Result<(), String> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, wide as i32, high as i32)
        .map_err(|why| format!("{why}"))?;
    {
        let context = cairo::Context::new(&surface).map_err(|why| format!("{why}"))?;
        context.set_source_rgb(shade, shade, shade);
        let _ = context.paint();
    }
    write_png(&surface, path)
}

/// The shape a PNG on disk actually has, read back through the same decoder the print pass uses. A test that
/// asks "did the crop land on the video's shape?" needs the file's own edges, not what was asked of ffmpeg.
/// TEST seam only: every program path that needs a picture's shape goes through cairo or ffprobe itself.
pub fn png_shape_for_test(path: &Path) -> Option<(u32, u32)> {
    let surface = read_png(path).ok()?;
    Some((
        surface.width().max(0) as u32,
        surface.height().max(0) as u32,
    ))
}
