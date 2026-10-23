//! F5.6 S7: printing the words onto the picture.
//!
//! The plain picture is what sd.cpp drew (or what a chosen frame gave); the finished `thumbnail.png` is that
//! picture with the marked texts and then the title printed over it. The lettering is a LOCAL cairo draw —
//! §F5.6 says a picked frame costs "no model, no GPU", and decision 34 makes reprinting a title a redraw of
//! letters only, never a second image call. One size rule backs it: [`crate::produce_runs::reprint`], which
//! is itself a call to the Cut page's fitter ([`crate::fx_text::fit`]), so a thumbnail's title and an effect's
//! caption can never disagree about how big those letters get.

use std::path::Path;

use crate::produce_runs;
use crate::project::TitleBox;

/// Print `words` over `png_in` and write the result to `png_out`.
///
/// The base is read as PNG because that is what the drawing step stores (`output_format: png`, §F5.6 S6) and
/// the only format cairo reads here; a JPEG straight from another server would have to be converted first,
/// which is why the copy-to-`thumbnail.png` path stays the fallback when the plain picture is missing.
/// `band` is the record's own title box, in fractions of the picture, exactly as the Cut page stores a text
/// effect's box.
pub fn print_words(
    png_in: &Path,
    png_out: &Path,
    words: &[String],
    band: &TitleBox,
) -> Result<(), String> {
    let surface = read_png(png_in)?;
    let (wide, high) = (surface.width(), surface.height());
    if wide <= 0 || high <= 0 {
        return Err(format!("{} has no pixels", png_in.display()));
    }
    // The same fit ladder the export uses, measured against THIS picture rather than the 1920×1080 tier: a
    // drawn thumbnail is 1280 on its long side (§F5.6 S6), and reusing the tier's pixel box would print a
    // fifth of the picture too small.
    let box_ = crate::fx_text::Box_ {
        cx: band.cx,
        cy: band.cy,
        wf: band.wf,
        hf: band.hf,
    };
    let (x, y, w, h) = box_.to_px(f64::from(wide), f64::from(high));
    let context = cairo::Context::new(&surface).map_err(|why| format!("{why}"))?;
    for (index, line) in words.iter().enumerate() {
        let fitted = produce_runs::reprint(line, *band);
        if fitted.lines.is_empty() || fitted.size <= 0.0 {
            // Nothing to print for this entry: an empty text is not a blank stripe across the picture.
            continue;
        }
        // Each kind gets its own slice of the band, so the marked texts and the title cannot land on top of
        // one another when both are printed in one run (S7 prints texts first, then the title).
        let share = h / f64::from(words.len().max(1) as u32);
        let top = y + share * index as f64;
        print_line(&context, &fitted.lines, fitted.size, x, top, w, share);
    }
    write_png(&surface, png_out)
}

/// Draw one fitted block centred in its slice of the band.
fn print_line(
    context: &cairo::Context,
    lines: &[String],
    size: f64,
    x: f64,
    top: f64,
    width: f64,
    height: f64,
) {
    context.set_source_rgb(1.0, 1.0, 1.0);
    context.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    context.set_font_size(size);
    let leading = size * 1.2;
    // Centre the block vertically in its slice, then each line horizontally in the band.
    let block = leading * lines.len() as f64;
    let mut baseline = top + ((height - block) / 2.0).max(0.0) + size;
    for line in lines {
        let left = match context.text_extents(line) {
            Ok(extents) => x + ((width - extents.width()) / 2.0).max(0.0),
            // No metrics (a font-less headless run): start at the band's left edge rather than skip the line —
            // the words still have to land on the picture.
            Err(_) => x,
        };
        // A soft shadow keeps white letters readable over any picture without needing a plate drawn first.
        context.set_source_rgba(0.0, 0.0, 0.0, 0.45);
        context.move_to(left + size * 0.03, baseline + size * 0.03);
        let _ = context.show_text(line);
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.move_to(left, baseline);
        let _ = context.show_text(line);
        baseline += leading;
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
