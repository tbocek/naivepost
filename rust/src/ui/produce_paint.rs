//! §08-produce#1-screen, numbers **2** and **7**: the pictures of the Produce page, PAINTED.
//!
//! Every plate (`thumbnail-picture`, each `image-picture-N`) is a bare `gtk::DrawingArea` with the
//! `.produce-plate` class, which only gives a background colour; this module installs the draw funcs.
//! Rules come from [`crate::produce_screen::shows_crop_box`]; [`layout_marks`] is a pure function over
//! fractions so a test pins it without a window.

use adw::prelude::*;
use gtk4 as gtk;

use crate::produce_screen;
use crate::project::{TextMark, TitleBox};
use crate::ui::produce_page::ProduceState;

pub type PaintedBox = (f64, f64, f64, f64, String);

/// One box per word mark, plus the title band, in plate pixels (§1 item 7, §A's marks). Pure so a test can
/// pin the geometry without GTK. The title prints in `band`, the box the render prints it into, and only
/// while it prints.
pub fn layout_marks(
    marks: &[TextMark],
    title: &str,
    title_printed: bool,
    band: &TitleBox,
    w: f64,
    h: f64,
) -> Vec<PaintedBox> {
    let mut boxes = Vec::new();
    if title_printed && !title.is_empty() {
        let (x, y, bw, bh) = place(band.cx, band.cy, band.wf, band.hf, w, h);
        boxes.push((x, y, bw, bh, title.to_string()));
    }
    for mark in marks {
        let (x, y, bw, bh) = place(mark.cx, mark.cy, mark.wf, mark.hf, w, h);
        boxes.push((x, y, bw, bh, mark.text.clone()));
    }
    boxes
}

/// A box stored as centre and size fractions of the plate, in plate px, clamped inside the plate (a box
/// hanging off the edge prints nothing).
fn place(cx: f64, cy: f64, wf: f64, hf: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    let bw = (wf * w).max(1.0);
    let bh = (hf * h).max(1.0);
    let x = (cx * w - bw / 2.0).clamp(0.0, (w - bw).max(0.0));
    let y = (cy * h - bh / 2.0).clamp(0.0, (h - bh).max(0.0));
    (x, y, bw, bh)
}

/// Put one word box where its mark says, over the plate. A child of `thumbnail-word-boxes` (the `Fixed`
/// above `thumbnail-picture`), placed by the SAME fractions [`layout_marks`] strokes. Called from
/// `draw_word_chips`; [`follow_plate`] moves them when the plate's size changes.
pub fn place_overlay_box(box_on_picture: gtk::Box, mark: &TextMark) {
    // The page's own root is the only tree that holds BOTH the plate and the word-box holder;
    // `page_image_box_for_test` answers a box inside it, too small to see either name.
    let Some(root) = crate::ui::produce_page::page_root() else {
        return;
    };
    let (Some(holder), Some(plate)) = (
        find_named(&root, "thumbnail-word-boxes"),
        find_named(&root, "thumbnail-picture"),
    ) else {
        return;
    };
    overlay_over_plate(&plate);
    let holder = holder_fixed(&holder);
    let (x, y, w, h) = box_px(mark);
    box_on_picture.set_size_request(w, h);
    holder.put(&box_on_picture, x, y);
}

/// Where a mark's box goes on the plate as it is now, in plate px. §A's box is a fraction OF THE PICTURE, so
/// the plate's own size is used, falling back to its requested size until allocated ([`follow_plate`] corrects it).
fn box_px(mark: &TextMark) -> (f64, f64, i32, i32) {
    let (wide, high) = plate_px();
    let (px, py, pw, ph) = picture_on_plate(wide, high);
    let (x, y, w, h, _) = layout_marks(
        std::slice::from_ref(mark),
        "",
        false,
        &TitleBox::default(),
        pw,
        ph,
    )
    .pop()
    .unwrap_or((0.0, 0.0, 28.0, 28.0, String::new()));
    (px + x, py + y, w.max(28.0) as i32, h.max(28.0) as i32)
}

/// Keep the word boxes on their marks when the plate changes size: the plate has its real size only after
/// GTK lays the column out. A move during the plate's own allocation would resize inside a resize, so it
/// runs once that allocation is done and is owed until then (`ui::settle`), keeping a snapshot from
/// taking its picture between the two.
pub fn follow_plate(plate: &gtk::DrawingArea) {
    plate.connect_resize(|plate, _, _| {
        let owed = crate::ui::settle::owe("the thumbnail's word boxes on their marks", plate);
        glib::idle_add_local_once(move || {
            let _owed = owed;
            let Some(holder) = word_box_holder() else {
                return;
            };
            let marks = crate::ui::produce_page::read_state().texts;
            let mut child = holder.first_child();
            while let Some(node) = child {
                child = node.next_sibling();
                let index = node
                    .widget_name()
                    .strip_prefix("words-box-")
                    .and_then(|i| i.parse::<usize>().ok());
                if let Some(mark) = index.and_then(|i| marks.get(i)) {
                    let (x, y, w, h) = box_px(mark);
                    node.set_size_request(w, h);
                    holder.move_(&node, x, y);
                }
            }
        });
    });
}

/// Walk down from `node` for the widget named `name`; this needs three different roots in one call.
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

fn word_box_holder() -> Option<gtk::Fixed> {
    let root = crate::ui::produce_page::page_root()?;
    find_named(&root, "thumbnail-word-boxes").map(|holder| holder_fixed(&holder))
}

/// Take every word box off the plate, before `draw_word_chips` puts the current ones back.
pub fn clear_overlay_boxes() {
    if let Some(holder) = word_box_holder() {
        while let Some(child) = holder.first_child() {
            holder.remove(&child);
        }
    }
}

fn holder_fixed(holder: &gtk::Widget) -> gtk::Fixed {
    holder
        .clone()
        .downcast::<gtk::Fixed>()
        .expect("the word-box holder is a Fixed")
}

/// The overlay the word boxes float in; `build_left_column` already made it, so this only finds it again.
fn overlay_over_plate(plate: &gtk::Widget) -> gtk::Overlay {
    if let Some(existing) = plate
        .parent()
        .and_then(|p| p.downcast::<gtk::Overlay>().ok())
    {
        return existing;
    }
    // No overlay means the page was built without one, a build bug: the dashed box would land under the picture.
    panic!("the thumbnail plate is not inside its overlay - build_left_column must wrap it")
}

/// The plate's current px size, or the size it asked for before GTK laid it out.
fn plate_px() -> (f64, f64) {
    // Found from the page's root by name: `widget_in_root` answers only `gtk::Box`es, and the plate is a
    // `DrawingArea`, so going through it always fell back to 320×180.
    let plate = crate::ui::produce_page::page_root()
        .and_then(|root| find_named(&root, "thumbnail-picture"));
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

/// Install the painter on the thumbnail and on every slot picture. Called again from `refresh`, because
/// `draw_slots` rebuilds the slot widgets and a new `DrawingArea` carries no draw func.
pub fn install(window: &adw::ApplicationWindow) {
    if let Some(area) = plate(window, "thumbnail-picture") {
        area.set_draw_func(move |area, cr, width, height| {
            paint_thumbnail(area, cr, width.max(1) as f64, height.max(1) as f64);
        });
        area.queue_draw();
    }
    // Slots are numbered 0..frames.len(); anything past the row is not found and costs nothing.
    for index in 0..crate::produce_screen::MAX_IMAGES {
        let name = format!("image-picture-{index}");
        let Some(area) = plate(window, &name) else {
            continue;
        };
        area.set_draw_func(move |area, cr, width, height| {
            paint_slot(area, cr, width.max(1) as f64, height.max(1) as f64, index);
        });
        area.queue_draw();
    }
}

fn plate(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::DrawingArea> {
    crate::ui::produce_page::widget_in(window, name)?
        .downcast::<gtk::DrawingArea>()
        .ok()
}

/// The plate's own ground: a dark grey gradient, so a plate reads as a picture surface rather than a hole.
fn ground(cr: &cairo::Context, w: f64, h: f64) {
    cr.set_source_rgb(0.16, 0.16, 0.18);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.05);
    cr.rectangle(0.0, 0.0, w, h / 2.0);
    let _ = cr.fill();
}

/// A checkerboard for a slot whose file is not on disk; louder than black, which reads as "drew nothing".
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

/// What a plate's picture is when it is painted.
enum Photo {
    Painted((f64, f64, f64, f64)),
    /// Being decoded on a worker: the plate shows its ground until the picture lands.
    Coming,
    /// No file, or a file that is not a picture: the missing marker.
    Missing,
}

/// Draw the frame's own picture into the plate, aspect-preserved (the difference in shape is what the crop
/// box shows). Painted from [`decoded`]'s cache: a draw func never decodes (§03-shell §1).
fn photo_rect(
    area: &gtk::DrawingArea,
    cr: &cairo::Context,
    path: &std::path::Path,
    w: f64,
    h: f64,
) -> Photo {
    if path.as_os_str().is_empty() {
        return Photo::Missing;
    }
    let (surface, wide, high) = match decoded(area, path) {
        Decoded::Ready(surface, wide, high) => (surface, wide.max(1), high.max(1)),
        Decoded::Coming => return Photo::Coming,
        Decoded::Missing => return Photo::Missing,
    };
    let (x, y, _, _) = fit(wide as f64, high as f64, w, h);
    let scale = (w / wide as f64).min(h / high as f64);
    let _ = cr.save();
    cr.translate(x, y);
    if scale != 1.0 {
        cr.scale(scale, scale);
    }
    let _ = cr.set_source_surface(&surface, 0.0, 0.0);
    let painted = cr.paint().is_ok();
    let _ = cr.restore();
    if painted {
        Photo::Painted((x, y, wide as f64 * scale, high as f64 * scale))
    } else {
        Photo::Missing
    }
}

/// The longest side a decoded plate picture is kept at: a full-size frame would be 8 MB of pixels per slot.
const KEPT_SIDE: i32 = 1280;

/// A file as the cache knows it: size and change time, so a rewritten file is decoded again.
type Stamp = (u64, Option<std::time::SystemTime>);

/// What [`decoded`] answers.
enum Decoded {
    Ready(cairo::ImageSurface, i32, i32),
    Coming,
    Missing,
}

type Held = Option<(cairo::ImageSurface, i32, i32)>;

thread_local! {
    /// Every plate picture decoded so far, with the stamp it was decoded at; `ASKED` holds what is on a worker now.
    static PICTURES: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, (Stamp, Held)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static ASKED: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, Stamp>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The picture at `path`, if a worker has decoded this version; otherwise ask for it once and answer
/// [`Decoded::Coming`]. Reading the file's size and time is the only disk work a paint does.
fn decoded(area: &gtk::DrawingArea, path: &std::path::Path) -> Decoded {
    let Ok(meta) = std::fs::metadata(path) else {
        return Decoded::Missing;
    };
    let stamp: Stamp = (meta.len(), meta.modified().ok());
    if let Some((held_at, held)) = PICTURES.with(|pictures| pictures.borrow().get(path).cloned()) {
        if held_at == stamp {
            return match held {
                Some((surface, wide, high)) => Decoded::Ready(surface, wide, high),
                None => Decoded::Missing,
            };
        }
    }
    let asked_now = ASKED.with(|asked| {
        let mut asked = asked.borrow_mut();
        if asked.get(path) == Some(&stamp) {
            return false;
        }
        asked.insert(path.to_path_buf(), stamp);
        true
    });
    if asked_now {
        // Owed until it lands, so a snapshot of the page waits for the picture rather than shooting the ground.
        let owed = crate::ui::settle::owe("a Produce plate's picture", area);
        let file = path.to_path_buf();
        crate::ui::background::run(
            "produce-plate",
            {
                let file = file.clone();
                move || {
                    gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&file, KEPT_SIDE, KEPT_SIDE, true)
                        .ok()
                        .and_then(|pixbuf| pixels(&pixbuf))
                }
            },
            move |answer| {
                let held = answer.and_then(|(bytes, wide, high)| {
                    surface_of(&bytes, wide, high).map(|surface| (surface, wide, high))
                });
                ASKED.with(|asked| asked.borrow_mut().remove(&file));
                PICTURES.with(|pictures| pictures.borrow_mut().insert(file, (stamp, held)));
                drop(owed);
                redraw_plates();
            },
        );
    }
    Decoded::Coming
}

/// Ask every plate on the page to paint again: a picture landed.
fn redraw_plates() {
    let Some(window) = crate::ui::produce_page::held_window() else {
        return;
    };
    if let Some(area) = plate(&window, "thumbnail-picture") {
        area.queue_draw();
    }
    for index in 0..crate::produce_screen::MAX_IMAGES {
        if let Some(area) = plate(&window, &format!("image-picture-{index}")) {
            area.queue_draw();
        }
    }
}

/// For a test: whether this plate picture has been decoded and is held (a paint never decodes).
pub fn picture_held_for_test(path: &std::path::Path) -> bool {
    PICTURES.with(|pictures| {
        pictures
            .borrow()
            .get(path)
            .is_some_and(|(_, held)| held.is_some())
    })
}

/// For a test: paint slot `index`'s picture onto a surface of its own as the slot's draw func does, and
/// answer whether the picture itself was painted. The first paint answers `false`: it only asks for the decode.
pub fn paint_slot_for_test(window: &adw::ApplicationWindow, index: usize) -> bool {
    let Some(area) = plate(window, &format!("image-picture-{index}")) else {
        return false;
    };
    let Ok(surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, 320, 180) else {
        return false;
    };
    let Ok(cr) = cairo::Context::new(&surface) else {
        return false;
    };
    let state = crate::ui::produce_page::read_state();
    let path = frame_path(state.frames.get(index));
    matches!(
        photo_rect(&area, &cr, &path, 320.0, 180.0),
        Photo::Painted(_)
    )
}

/// Cairo's surface over pixels [`pixels`] spelled.
fn surface_of(bytes: &[u8], wide: i32, high: i32) -> Option<cairo::ImageSurface> {
    // Hand cairo a COPY: `ImageSurface::data()` of a `create_for_data` surface came back EMPTY in this repo's
    // cairo version, so such an image painted nothing while still reporting a good status.
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, wide, high).ok()?;
    {
        let mut target = surface.data().ok()?;
        if target.len() < bytes.len() {
            return None;
        }
        target[..bytes.len()].copy_from_slice(bytes);
    }
    surface.status().ok()?;
    Some(surface)
}

/// This pixbuf's pixels as cairo's ARgb32 bytes (little-endian BGRA), rows copied one at a time because a
/// padded `rowstride` (a JPEG's usually) must land on cairo's unpadded stride; 3-channel pixbufs get an alpha
/// byte. Runs on the decode worker; [`surface_of`] makes the surface.
fn pixels(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> Option<(Vec<u8>, i32, i32)> {
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
    Some((pixels, wide, high))
}

/// The violet dashed box §A draws for a word mark, with its words printed by the render's own rule
/// ([`crate::produce_runs::reprint`]) scaled to `picture_w` px, so the editor shows the lines the thumbnail will carry.
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
        crate::produce_upload_print::draw_text_box(cr, text, *x, *y, *bw, *bh, 1.0);
    }
}

/// The box the title prints in: the record's own when it has one, else the default, as the render reads it.
pub fn title_band() -> TitleBox {
    // A title being dragged is painted where the pointer has it; then the page's own box; then the record's.
    if let Some((crate::ui::produce_words_press::Hold::Title, dragged)) =
        crate::ui::produce_words_press::dragging()
    {
        return dragged;
    }
    if let Some(held) = crate::ui::produce_page::read_state().title_box {
        return held;
    }
    crate::layout::Tree::new(session_dir())
        .ok()
        .and_then(|tree| crate::publish::load(&tree).ok())
        .and_then(|record| record.title_box)
        .unwrap_or_default()
}

/// The marked texts as painted now: a box being dragged is painted where the pointer has it.
fn texts_now(texts: &[crate::project::TextMark]) -> Vec<crate::project::TextMark> {
    let mut out = texts.to_vec();
    if let Some((crate::ui::produce_words_press::Hold::Mark(index), dragged)) =
        crate::ui::produce_words_press::dragging()
    {
        if let Some(mark) = out.get_mut(index) {
            mark.cx = dragged.cx;
            mark.cy = dragged.cy;
            mark.wf = dragged.wf;
            mark.hf = dragged.hf;
        }
    }
    out
}

/// The base slot's crop box: the video-shape rectangle the thumbnail keeps, drawn over the frame. Its centre
/// is §10's `publish.crop` (fractions of the frame). `picture` is where the base landed on the slot; the crop
/// is a part OF THE FRAME, so it is laid over the frame, not the slot's letterbox.
fn draw_crop(cr: &cairo::Context, state: &ProduceState, picture: (f64, f64, f64, f64)) {
    if !produce_screen::shows_crop_box(state.base_shape, state.video_shape) {
        return;
    }
    let (left, top, w, h) = picture;
    // Keep the video's aspect inside the frame; that kept rectangle IS the crop.
    let (vw, vh) = (state.video_shape.0 as f64, state.video_shape.1 as f64);
    let scale = (w / vw).min(h / vh);
    let bw = vw * scale;
    let bh = vh * scale;
    // §A S8: the crop CENTRE in px is what the render wants, stored in §10 as fractions of the frame (the pair
    // `produce_upload_media::request_images` hands ffmpeg). Read from the record, not guessed.
    let tree = crate::layout::Tree::new(session_dir()).ok();
    let crop = match tree.as_ref().map(crate::publish::load) {
        Some(Ok(record)) => (record.crop.x, record.crop.y),
        Some(Err(err)) => {
            eprintln!("produce plate: {err}");
            (0.5, 0.5)
        }
        None => (0.5, 0.5),
    };
    let centre_x = left + crop.0.clamp(0.0, 1.0) * (w - bw).max(0.0);
    let centre_y = top + crop.1.clamp(0.0, 1.0) * (h - bh).max(0.0);
    cr.set_line_width(2.0);
    cr.set_source_rgba(0.82, 0.64, 0.08, 0.2);
    cr.rectangle(centre_x, centre_y, bw, bh);
    let _ = cr.fill();
    cr.set_source_rgb(0.82, 0.64, 0.08);
    cr.rectangle(centre_x, centre_y, bw, bh);
    let _ = cr.stroke();
}

/// A `wide`×`high` picture fitted inside a `w`×`h` plate, centred, never stretched: `(x, y, width, height)`.
fn fit(wide: f64, high: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    let scale = (w / wide.max(1.0)).min(h / high.max(1.0));
    let (pw, ph) = (wide * scale, high * scale);
    (((w - pw) / 2.0).max(0.0), ((h - ph) / 2.0).max(0.0), pw, ph)
}

/// The picture the thumbnail plate shows: the drawn `thumbnail-plain.png` with the marked words printed over
/// it HERE (answered `true`), so a just-marked box shows its words before any ▶; else the printed
/// `thumbnail.png` (a run stopped between the two writes leaves only it); else nothing. Never the image row's
/// base: that is what the model EDITS, not what the upload gets.
fn thumbnail_shown() -> Option<(std::path::PathBuf, bool)> {
    let tree = crate::layout::Tree::new(session_dir()).ok()?;
    let plain = tree.thumbnail_plain_png();
    if plain.is_file() {
        return Some((plain, true));
    }
    let printed = tree.thumbnail_png();
    printed.is_file().then_some((printed, false))
}

/// Where the shown thumbnail sits on a `w`×`h` plate: `(x, y, width, height)`. Word marks are fractions OF THE
/// PICTURE (§A), so paint, dashed boxes and drag measure from this one rectangle; with nothing drawn it is the
/// whole plate. Only the file's header is read, per call, so a redraw is seen at once.
pub fn picture_on_plate(w: f64, h: f64) -> (f64, f64, f64, f64) {
    thumbnail_shown()
        .and_then(|(path, _)| gtk::gdk_pixbuf::Pixbuf::file_info(&path))
        .map_or((0.0, 0.0, w, h), |(_, wide, high)| {
            fit(f64::from(wide), f64::from(high), w, h)
        })
}

/// The thumbnail plate, painted on its own surface (as `snapshot_lane` does) for the `08-produce` staging step:
/// photo (or the missing marker), then the crop, then the words on top.
fn paint_thumbnail(area: &gtk::DrawingArea, cr: &cairo::Context, w: f64, h: f64) {
    let state = crate::ui::produce_page::read_state();
    ground(cr, w, h);
    let shown = thumbnail_shown();
    let photo = match shown.as_ref() {
        Some((path, _)) => photo_rect(area, cr, path, w, h),
        None => Photo::Missing,
    };
    let rect = match photo {
        Photo::Painted(rect) => Some(rect),
        Photo::Coming => None,
        Photo::Missing => {
            missing(cr, w, h);
            None
        }
    };
    // No crop box here: the thumbnail is already the video's shape. A printed `thumbnail.png` already carries
    // its words; printing them again would double them.
    let Some((x, y, pw, ph)) = rect.filter(|_| shown.as_ref().is_some_and(|(_, live)| *live))
    else {
        return;
    };
    // Marks are fractions OF THE PICTURE (§A): laid out over the picture where it landed, printed as the render prints them.
    let _ = cr.save();
    cr.translate(x, y);
    draw_words(
        cr,
        &layout_marks(
            &texts_now(&state.texts),
            &state.thumb_title,
            state.title_seeded,
            &title_band(),
            pw,
            ph,
        ),
    );
    let _ = cr.restore();
}

/// One image slot's plate. Only the base (**2**) can differ in shape from the video, so only it draws a crop box.
fn paint_slot(area: &gtk::DrawingArea, cr: &cairo::Context, w: f64, h: f64, index: usize) {
    let state = crate::ui::produce_page::read_state();
    ground(cr, w, h);
    let path = frame_path(state.frames.get(index));
    let photo = photo_rect(area, cr, &path, w, h);
    SLOT_SHOWN.with(|shown| {
        shown.borrow_mut().insert(
            index,
            matches!(photo, Photo::Painted(_)).then(|| path.clone()),
        )
    });
    match photo {
        Photo::Painted(picture) if index == 0 => draw_crop(cr, &state, picture),
        Photo::Painted(_) | Photo::Coming => {}
        Photo::Missing => missing(cr, w, h),
    }
}

thread_local! {
    /// The picture each slot's last real draw showed, for the test that must see a slot fill in without painting it itself.
    static SLOT_SHOWN: std::cell::RefCell<std::collections::HashMap<usize, Option<std::path::PathBuf>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// For a test: the picture slot `index`'s last draw by the window showed, if any.
pub fn slot_shown_for_test(index: usize) -> Option<std::path::PathBuf> {
    SLOT_SHOWN.with(|shown| shown.borrow().get(&index).cloned().flatten())
}

/// A stored row entry as the file it shows. A `frame:<seconds>` entry is the session's extracted frame nearest
/// that second, through the request's rule ([`crate::produce_upload_media::entry_file`]); anything else is a
/// `project:`-relative or absolute path against the session folder. No folder, no entry, or a frame list still
/// being read answers a path that cannot exist, which the caller paints as "no picture".
fn frame_path(entry: Option<&String>) -> std::path::PathBuf {
    let Some(stored) = entry else {
        return std::path::PathBuf::new();
    };
    let dir = session_dir();
    if crate::produce_details::frame_line(stored).is_none() {
        return crate::layout::resolve(&dir, &dir, stored);
    }
    let Ok(tree) = crate::layout::Tree::new(&dir) else {
        return std::path::PathBuf::new();
    };
    session_frames(&tree)
        .and_then(|frames| crate::produce_upload_media::entry_file(&tree, stored, &frames))
        .unwrap_or_default()
}

/// Every extracted frame of the session's footage as `(session second, path)`, off the Cut page's frame cache,
/// listed on a worker (§03-shell §1). `None` means a listing is still coming; its landing repaints ([`repaint_plates`]).
fn session_frames(tree: &crate::layout::Tree) -> Option<Vec<(f64, std::path::PathBuf)>> {
    let project = crate::ui::window::live_project();
    let names: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.path.rsplit('/').next().unwrap_or(&source.path))
        .collect();
    let zero = crate::clock::clock(&names).zero;
    let mut frames = Vec::new();
    for stem in crate::ui::produce_press_wiring::frame_source_stems() {
        for path in crate::ui::cut_picture_cache::frames_or_coming(tree, &stem)? {
            let Some(name) = path.file_stem().and_then(|n| n.to_str()) else {
                continue;
            };
            if let Some(seconds) = crate::clock::frame_session_seconds(name, zero) {
                frames.push((seconds, path));
            }
        }
    }
    Some(frames)
}

/// A frame listing landed: the page's plates paint again.
pub fn repaint_plates() {
    if let Some(window) = crate::ui::produce_page::held_window() {
        install(&window);
    }
}

/// The session folder this page's pictures live in: the opened project, else the launch's working copy beside cwd.
fn session_dir() -> std::path::PathBuf {
    open_project()
        .or_else(crate::startup::open_dir_if_set)
        .unwrap_or_else(crate::startup::open_dir)
}

thread_local! {
    /// The folder a snapshot asked this page to render. `build_window` resets `save_apply::open_dir` AFTER a
    /// snapshot stages the real one, so the plates keep their own slot for it.
    static OPEN_PROJECT: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

fn open_project() -> Option<std::path::PathBuf> {
    OPEN_PROJECT.with(|slot| slot.borrow().clone())
}

/// Say which project this page renders, from the snapshot staging step; a live window answers through `save_apply::open_dir`.
pub fn note_open_project(dir: &std::path::Path) {
    OPEN_PROJECT.with(|slot| *slot.borrow_mut() = Some(dir.to_path_buf()));
}
