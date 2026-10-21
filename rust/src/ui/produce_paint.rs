//! §08-produce#1-screen, numbers **2** and **7**: the pictures of the Produce page, PAINTED.
//!
//! Every plate on this page (`thumbnail-picture`, each `image-picture-N`) is a bare `gtk::DrawingArea`
//! carrying the `.produce-plate` CSS class. A CSS plate only gives it a background colour: with no draw
//! func the thumbnail was a black rectangle in every render, the slot pictures were black too, and the
//! words someone had put on the thumbnail were nowhere on it. This module installs the draw funcs — the
//! same shape `narrate_picture::install_draw` uses (page's `wire` installs once, `refresh` queues again).
//!
//! Nothing here decides a rule: which slot is the base and whether it wears a crop box come from
//! [`crate::produce_screen::shows_crop_box`], and where a word box sits is [`layout_marks`], a pure
//! function over fractions so a test can pin it without a window.

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::TextMark;
use crate::produce_screen;
use crate::ui::produce_page::ProduceState;

/// One painted box: `(x, y, width, height)` in plate px plus the words to print in it.
pub type PaintedBox = (f64, f64, f64, f64, String);

/// One box per word mark, plus the title band, in plate pixels (§1 item 7: "✎ to reword", §A's marks
/// visible on the picture). Pure so a test can pin the geometry without GTK: the draw func below only
/// strokes what this returns. The title prints across the top strip the FIRST time only.
pub fn layout_marks(
    marks: &[TextMark],
    title: &str,
    title_printed: bool,
    w: f64,
    h: f64,
) -> Vec<PaintedBox> {
    let mut boxes = Vec::new();
    if title_printed && !title.is_empty() {
        // §A draws the title band across the top of the picture, inset so it never touches an edge.
        let inset = w * 0.06;
        boxes.push((inset, h * 0.06, w - 2.0 * inset, h * 0.22, title.to_string()));
    }
    for mark in marks {
        let bw = (mark.wf * w).max(1.0);
        let bh = (mark.hf * h).max(1.0);
        // The mark's centre is stored as a fraction of the plate, so the box is that centre minus half
        // its size — clamped inside the plate, because a box hanging off the edge prints nothing.
        let x = (mark.cx * w - bw / 2.0).clamp(0.0, (w - bw).max(0.0));
        let y = (mark.cy * h - bh / 2.0).clamp(0.0, (h - bh).max(0.0));
        boxes.push((x, y, bw, bh, mark.text.clone()));
    }
    boxes
}

/// Put one word box where its mark says, over the plate. The box is a child of `thumbnail-word-boxes`
/// (the holder that floats above `thumbnail-picture`), sized and placed by the SAME fractions
/// [`layout_marks`] strokes, so the dashed widget and the dashed ink cannot drift apart. Called from
/// `draw_word_chips`, which rebuilds both the boxes and the chips whole.
pub fn place_overlay_box(box_on_picture: gtk::Box, mark: &TextMark) {
    // The page's own root — what `build()` published and `wire` replaces with the window — is the only tree
    // that holds BOTH the plate and the word-box holder; `page_image_box_for_test` answers a box INSIDE it,
    // too small a subtree to see either name.
    let root = match crate::ui::produce_page::page_root() {
        Some(root) => root,
        None => return,
    };
    let holder = match find_named(&root, "thumbnail-word-boxes") {
        Some(holder) => holder,
        None => return,
    };
    let plate = match find_named(&root, "thumbnail-picture") {
        Some(plate) => plate,
        None => return,
    };
    // The overlay the box floats in is already there (the plate holds no children of its own), so this only
    // finds it again; a plate whose parent is that overlay answers it.
    overlay_over_plate(&plate, &holder);
    // The plate's own size, not a guessed 320×180: §A's box is a fraction OF THE PICTURE, so a wider
    // column draws a wider box. Until the plate has an allocation, fall back to its requested size.
    let (wide, high) = plate_px();
    let (x, y, w, h, _) = layout_marks(std::slice::from_ref(mark), "", false, wide, high)
        .pop()
        .unwrap_or((0.0, 0.0, 28.0, 28.0, String::new()));
    holder_box(&holder).append(&box_on_picture);
    box_on_picture.set_size_request(w.max(28.0) as i32, h.max(28.0) as i32);
    // GTK 4 has no "put this overlay at x,y" call: an overlay child is placed by ITS OWN alignment plus
    // margins. Centre-to-centre on both axes with a margin of (centre − half the plate) lands the box's
    // centre exactly where the mark's centre fraction says, whatever the plate's allocation turns out to be.
    box_on_picture.set_halign(gtk::Align::Center);
    box_on_picture.set_valign(gtk::Align::Center);
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    box_on_picture.set_margin_start((cx - wide / 2.0).round() as i32);
    box_on_picture.set_margin_end((wide / 2.0 - cx).round() as i32);
    box_on_picture.set_margin_top((cy - high / 2.0).round() as i32);
    box_on_picture.set_margin_bottom((high / 2.0 - cy).round() as i32);
}

/// Walk down from `node` for the widget named `name`. The page's own lookups are private to
/// `produce_page`; this one needs three different roots in one call, so it walks directly.
fn find_named(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut cursor = node.first_child();
    while let Some(current) = cursor {
        if let Some(found) = find_named(&current, name) {
            return Some(found);
        }
        cursor = current.next_sibling();
    }
    None
}

/// The holder as the box it is built as. `find_named` answers a bare `Widget`, and gtk4-rs resolves
/// `BoxExt::append` against the wrong `append` in scope unless the receiver is already a `gtk::Box`.
fn holder_box(holder: &gtk::Widget) -> gtk::Box {
    holder.clone().downcast::<gtk::Box>().expect("the word-box holder is a Box")
}

/// The overlay the word boxes float in. `build_left_column` already made it — the plate inside it holds no
/// children of its own — so this only finds it again; a plate whose parent is that overlay answers it.
fn overlay_over_plate(plate: &gtk::Widget, holder: &gtk::Widget) -> gtk::Overlay {
    if let Some(existing) = plate.parent().and_then(|p| p.downcast::<gtk::Overlay>().ok()) {
        return existing;
    }
    // No overlay to live in means the page was built without one, which is a build bug, not a case to paper
    // over: the dashed box would silently land in the column under the picture.
    let _ = holder;
    panic!("the thumbnail plate is not inside its overlay - build_left_column must wrap it")
}

/// The plate's current px size, or the size it asked for before GTK laid it out. `thumbnail-picture` is
/// the `DrawingArea` itself; the overlay that floats the word boxes over it carries its own name.
fn plate_px() -> (f64, f64) {
    let plate = crate::ui::produce_page::widget_in_root("thumbnail-picture");
    match plate {
        Some(widget) if widget.width() > 0 && widget.height() > 0 => {
            (widget.width() as f64, widget.height() as f64)
        }
        _ => (PLATE_W, PLATE_H),
    }
}

/// What `build_left_column` asks the plate for; the fallback until it has an allocation.
const PLATE_W: f64 = 320.0;
const PLATE_H: f64 = 180.0;

/// Install the painter on the thumbnail and on every slot picture. Called from the page's `wire` and
/// again from `refresh`, because `draw_slots` rebuilds the slot widgets whole and a freshly created
/// `DrawingArea` carries no draw func until this runs on it.
pub fn install(window: &adw::ApplicationWindow) {
    if let Some(area) = plate(window, "thumbnail-picture") {
        area.set_draw_func(move |_area, cr, width, height| {
            paint_thumbnail(cr, width.max(1) as f64, height.max(1) as f64);
        });
        area.queue_draw();
    }
    // Slots are numbered 0..frames.len(); anything past the row simply is not found and costs nothing.
    for index in 0..crate::produce_screen::MAX_IMAGES {
        let name = format!("image-picture-{index}");
        let Some(area) = plate(window, &name) else {
            continue;
        };
        area.set_draw_func(move |_area, cr, width, height| {
            paint_slot(cr, width.max(1) as f64, height.max(1) as f64, index);
        });
        area.queue_draw();
    }
}

fn plate(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::DrawingArea> {
    crate::ui::produce_page::widget_in(window, name)?
        .downcast::<gtk::DrawingArea>()
        .ok()
}

/// The plate's own ground: a dark grey gradient rather than the CSS flat black, so a plate reads as a
/// picture surface (with something missing on it) instead of a hole in the page.
fn ground(cr: &cairo::Context, w: f64, h: f64) {
    cr.set_source_rgb(0.16, 0.16, 0.18);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.05);
    cr.rectangle(0.0, 0.0, w, h / 2.0);
    let _ = cr.fill();
}

/// A checkerboard for a slot whose file is not on disk: §A's empty state says the picture is missing, and
/// a pattern says that louder than black, which reads as "the app drew nothing".
fn missing(cr: &cairo::Context, w: f64, h: f64) {
    let cell = (w / 8.0).max(6.0);
    let mut row = 0.0;
    while row < h {
        let mut col = 0.0;
        while col < w {
            if ((row / cell) + (col / cell)).rem_euclid(2.0) < 1.0 {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.08);
                cr.rectangle(col, row, cell.min(w - col), cell.min(h - row));
                let _ = cr.fill();
            }
            col += cell;
        }
        row += cell;
    }
}

/// Draw the frame's own picture into the plate, aspect-preserved (the plate is the video's shape, the frame
/// may not be — that difference is exactly what the crop box exists to show). gdk-pixbuf decodes it and
/// [`raster`] spells its pixels into a cairo surface; the Cut preview's own door is not used because it
/// refuses a good file whose rowstride is padded.
fn draw_photo(cr: &cairo::Context, path: &std::path::Path, w: f64, h: f64) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file(path) else {
        return false;
    };
    let wide = pixbuf.width().max(1);
    let high = pixbuf.height().max(1);
    // Fit inside the plate, centred — never stretched, or a tall frame would lie about its shape.
    let scale = (w / wide as f64).min(h / high as f64);
    let x = ((w - wide as f64 * scale) / 2.0).max(0.0);
    let y = ((h - high as f64 * scale) / 2.0).max(0.0);
    let _ = cr.save();
    cr.translate(x, y);
    if scale != 1.0 {
        cr.scale(scale, scale);
    }
    let painted = match raster(&pixbuf) {
        Some(surface) => {
            let _ = cr.set_source_surface(&surface, 0.0, 0.0);
            cr.paint().is_ok()
        }
        None => false,
    };
    let _ = cr.restore();
    painted
}

/// This pixbuf as a cairo surface, its rows copied one at a time so a source whose `rowstride` is padded
/// past `width * n_channels` still lands on cairo's unpadded stride (a JPEG's usually is). ARgb32 is
/// little-endian BGRA, so a 3-channel pixbuf gets the alpha byte the same way and an alpha-bearing one
/// copies whole.
fn raster(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> Option<cairo::ImageSurface> {
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
    let out_stride = (wide as usize) * 4;
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
            // gdk-pixbuf hands back NON-premultiplied alpha and cairo wants it premultiplied.
            let alpha = if channels == 4 { from[source + 3] } else { 255 };
            if alpha < 255 {
                to[target] = (to[target] as u16 * alpha as u16 / 255) as u8;
                to[target + 1] = (to[target + 1] as u16 * alpha as u16 / 255) as u8;
                to[target + 2] = (to[target + 2] as u16 * alpha as u16 / 255) as u8;
            }
            to[target + 3] = alpha;
        }
    }
    // Hand cairo a COPY of its own: `ImageSurface::data()` of a `create_for_data` surface came back EMPTY
    // in this repo's cairo version, so an image built that way painted nothing while still reporting a
    // good status — the exact silent failure that left every plate black.
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

/// The violet dashed box §A draws for a word mark, with its words inside.
fn draw_words(cr: &cairo::Context, boxes: &[PaintedBox]) {
    for (x, y, bw, bh, text) in boxes {
        cr.set_line_width(2.0);
        cr.set_dash(&[6.0, 4.0], 0.0);
        cr.set_source_rgb(0.54, 0.37, 0.82);
        cr.rectangle(*x, *y, *bw, *bh);
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);
        if text.is_empty() {
            continue;
        }
        // Words printed to fill the box (§1): the font shrinks with the box rather than overflowing it.
        // Cairo alone has no text engine beyond its toy API — that is all a plate needs, and it is what
        // `produce_upload_print` already uses for the upload card.
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let size = (bh * 0.55).max(8.0);
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        cr.set_font_size(size);
        match cr.text_extents(text) {
            Ok(extent) => cr.move_to(
                x + ((bw - extent.width()) / 2.0).max(2.0),
                y + (bh + extent.height()) / 2.0,
            ),
            Err(_) => cr.move_to(x + 2.0, y + bh / 2.0),
        }
        let _ = cr.show_text(text);
    }
}

/// The base slot's crop box: the video-shape rectangle the thumbnail keeps, drawn over the taller/wider
/// frame. Its centre is §10's `publish.crop` (x/y fractions of the frame), which is where a drag lands.
fn draw_crop(cr: &cairo::Context, state: &ProduceState, w: f64, h: f64) {
    if !produce_screen::shows_crop_box(state.base_shape, state.video_shape) {
        return;
    }
    // Keep the video's aspect inside the plate; that kept rectangle IS the crop.
    let (vw, vh) = (state.video_shape.0 as f64, state.video_shape.1 as f64);
    let scale = (w / vw).min(h / vh);
    let bw = vw * scale;
    let bh = vh * scale;
    // §A S8: the crop CENTRE in px is what the render wants, and §10 stores it as fractions of the
    // frame — the same pair `produce_upload_media::request_images` hands ffmpeg. Read from the record,
    // not guessed: a plate that invented a centre would show a crop the render will not make.
    let tree = crate::layout::Tree::new(session_dir()).ok();
    let crop = match tree.as_ref().map(crate::publish::load) {
        Some(Ok(record)) => (record.crop.x, record.crop.y),
        Some(Err(err)) => {
            eprintln!("produce plate: {err}");
            (0.5, 0.5)
        }
        None => (0.5, 0.5),
    };
    let centre_x = crop.0.clamp(0.0, 1.0) * (w - bw).max(0.0);
    let centre_y = crop.1.clamp(0.0, 1.0) * (h - bh).max(0.0);
    cr.set_line_width(2.0);
    cr.set_source_rgba(0.82, 0.64, 0.08, 0.2);
    cr.rectangle(centre_x, centre_y, bw, bh);
    let _ = cr.fill();
    cr.set_source_rgb(0.82, 0.64, 0.08);
    cr.rectangle(centre_x, centre_y, bw, bh);
    let _ = cr.stroke();
}

/// The thumbnail plate, painted on its own surface (the way `snapshot_lane` and `snapshot_preview_frame`
/// paint a subject the window shot cannot show): ground, photo, crop, words. Used by the `08-produce`
/// staging step to write a legible thumbnail beside the window shot.
/// The thumbnail plate: photo (or the missing marker), then the crop, then the words on top.
fn paint_thumbnail(cr: &cairo::Context, w: f64, h: f64) {
    let state = crate::ui::produce_page::read_state();
    ground(cr, w, h);
    if !draw_photo(cr, &frame_path(state.frames.first()), w, h) {
        missing(cr, w, h);
    }
    draw_crop(cr, &state, w, h);
    draw_words(cr, &layout_marks(&state.texts, &state.thumb_title, state.title_seeded, w, h));
}

/// One image slot's plate. Only the base (**2**) can differ in shape from the video, so only it draws a
/// crop box; the references are shown whole.
fn paint_slot(cr: &cairo::Context, w: f64, h: f64, index: usize) {
    let state = crate::ui::produce_page::read_state();
    ground(cr, w, h);
    let path = frame_path(state.frames.get(index));
    if !draw_photo(cr, &path, w, h) {
        missing(cr, w, h);
    }
    if index == 0 {
        draw_crop(cr, &state, w, h);
    }
}

/// A stored `project:`-relative frame path resolved against the session folder. No folder or no entry
/// answers a path that cannot exist, which the caller treats as "no picture" through the pixbuf error.
fn frame_path(entry: Option<&String>) -> std::path::PathBuf {
    match entry {
        Some(stored) => {
            let dir = session_dir();
            crate::layout::resolve(&dir, &dir, stored)
        }
        None => std::path::PathBuf::new(),
    }
}


/// The session folder this page's pictures live in: the project the app took on (Open, a Save, or a
/// snapshot's `--project`), else the launch's working copy beside cwd — which is what a test pins to.
fn session_dir() -> std::path::PathBuf {
    open_project()
        .or_else(crate::ui::save_apply::open_dir)
        .unwrap_or_else(|| crate::startup::session_dir(&std::env::current_dir().unwrap_or_default()))
}

thread_local! {
    /// The folder a snapshot asked this page to render. `build_window` resets `save_apply::open_dir` to
    /// the working copy beside the launch AFTER a snapshot stages the real one, so the plates keep their
    /// own slot for it rather than reading a door that was overwritten after them.
    static OPEN_PROJECT: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

fn open_project() -> Option<std::path::PathBuf> {
    OPEN_PROJECT.with(|slot| slot.borrow().clone())
}

/// Say which project this page renders, from the snapshot staging step. Nothing else calls it: a live
/// window answers through `save_apply::open_dir`, which `build_window` and Open both seed.
pub fn note_open_project(dir: &std::path::Path) {
    OPEN_PROJECT.with(|slot| *slot.borrow_mut() = Some(dir.to_path_buf()));
}
