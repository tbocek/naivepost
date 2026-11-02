//! §06-effects#2-the-lane-and-the-preview: the preview's real frame painted on cairo: area outside the camera rect dimmed, the rect over the picture, overlays where they will be seen. The CSS plate widgets stay in the tree as the state tests read (`preview-camera-rect`'s opacity, `zoom_drag_widgets.rs` F3.1 S1); while ⊕ Zoom is armed both put the camera layer down together.
//! No rule here: dim alpha, stroke width, outline colour and dashes come from [`fx_lane`], the camera position from [`fx_zoom`], one entry per §2 clause in order.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self, Cut};
use crate::fx_lane::{self, PausedScene};
use crate::fx_svg;
use crate::fx_text;
use crate::fx_zoom::{self, Camera};
use crate::preview;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};

/// The painted surface's widget name; tests find it to know the panel is drawn, and assert rules against `fx_lane`'s view-model.
pub const FRAME_NAME: &str = "preview-frame";

/// The one CSS rule this module needs, pushed by `install_lane_css_once`. `.zoom-camera-down` has no colour: it is the STATE a test reads off `zoom_armed()`, never a second paint.
pub fn css() -> String {
    ".zoom-camera-down { }\n".to_string()
}

/// This window's painted surface, resolved through its own content so a closed window's tree cannot answer (as `preview_panel`).
pub fn frame_of(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, FRAME_NAME)?
        .downcast::<gtk::DrawingArea>()
        .ok()
}

/// The panel's painted surface, laid OVER the plate widgets, with a draw func reading this window's live state.
pub fn build() -> gtk::DrawingArea {
    let frame = gtk::DrawingArea::new();
    frame.set_widget_name(FRAME_NAME);
    // The surface must cover the whole picture: a fixed 240x135 request plus `set_clip_overlay(true)` clipped the camera rect to a small square once §A's paned gave the preview half the page.
    frame.set_hexpand(true);
    frame.set_vexpand(true);
    // No size request: the floor belongs to `preview-panel` (§A "min height 160"); a second one fought the measure/clip marks `page_box` sets.
    frame
}

/// Ask the surface to repaint, after the refresh pushed every value into the plate widgets, so they cannot disagree.
pub fn repaint(window: &adw::ApplicationWindow) {
    if let Some(frame) = frame_of(window) {
        frame.queue_draw();
    }
}

/// The draw func: takes the window's answers through [`observe`] and hands them to [`paint`]; [`note_box`] is called by the drag while a box is under the hand.
pub fn install_draw(window: &adw::ApplicationWindow) {
    let held = window.clone();
    if let Some(frame) = frame_of(&held) {
        frame.set_draw_func(move |_, cr, width, height| {
            let panel = (width as f64, height as f64);
            let fit = fit_size_of(panel.0, panel.1);
            let cut = crate::ui::window_preview::newest_review_cut();
            // The box under the hand arrives in panel px; the frame draws in the picture's, which starts after the letterbox bars.
            let (ox, oy) = picture_offset(panel, fit);
            let drawn = drawn_box().map(|(x, y, w, h)| (x - ox, y - oy, w, h));
            let frame = describe(fit, &held_frame(&cut, drawn));
            let seen = observe(&frame, live_of());
            paint_in_panel(cr, &frame, &seen, panel);
        });
    }
}

pub fn drawn_box() -> Option<(f64, f64, f64, f64)> {
    CUT_PAGE.with(
        |CutPage {
             drawn_box: slot, ..
         }| slot.get(),
    )
}

/// A caption box is being dragged: the frame draws the snap targets. No repaint here (the caller repaints once).
pub fn note_edit(active: bool) {
    CUT_PAGE.with(
        |CutPage {
             edit_snap: slot, ..
         }| slot.set(active),
    );
}

pub fn edit_snap_on() -> bool {
    CUT_PAGE.with(
        |CutPage {
             edit_snap: slot, ..
         }| slot.get(),
    )
}

/// The door the drag calls with a box in panel px (the surface is a fixed 240 x 135, [`crate::ui::window::PREVIEW_PANEL_W`] x [`crate::ui::window::PREVIEW_PANEL_H`]). Corners are normalised since a negative width paints an invisible outline.
pub fn note_box(box_: Option<(f64, f64, f64, f64)>, window: &adw::ApplicationWindow) {
    CUT_PAGE.with(
        |CutPage {
             drawn_box: slot, ..
         }| {
            slot.set(box_.map(|(x, y, w, h)| {
                (
                    x.min(x + w),
                    y.min(y + h),
                    w.abs().max(1.0),
                    h.abs().max(1.0),
                )
            }))
        },
    );
    repaint(window);
}

/// The record in hand resolved to this cut's index, and the box under the hand; a held record not on this cut holds no overlay.
pub fn held_frame(cut: &Cut, box_px: Option<(f64, f64, f64, f64)>) -> Held {
    let index = crate::ui::cut_hand::held_effect().and_then(|held| {
        cut.fx
            .iter()
            .position(|effect| effect.kind == held.kind && effect.t == held.t)
    });
    // §2's "a box being drawn dashed": only while an arm that DRAWS one is up (⊕ Zoom, ▨ SVG, ❝ Text); after the drag the record is placed.
    let box_px = if crate::ui::zoom_armed()
        || crate::ui::cut_svg::svg_armed()
        || crate::ui::cut_text_hand::text_armed()
    {
        box_px
    } else {
        None
    };
    Held { index, box_px }
}

/// The footage's box on this panel: no ffprobe runs headless, so the fit is against the panel's declared shape ([`crate::ui::window::PREVIEW_PANEL_W`] x [`crate::ui::window::PREVIEW_PANEL_H`]), as `preview_picture_px` does; an allocation differing between a test and a loaded suite made fractions hit the clamp.
pub fn fit_size_of(width: f64, height: f64) -> (f64, f64) {
    let (w, h) = crate::ui::window::preview_picture_px();
    // §9 (`both MUST use the first footage clip's frame`): fractions multiply by the plate's agreed shape (`cut_picture::refresh` via `effect_overlap::source_frame`), not whatever camera is watched. The one place preview fractions become pixels.
    let (sw, sh) = crate::ui::cut_picture::plate_shape();
    let (w, h) = if sw > 0 && sh > 0 {
        (sw as f64, sh as f64)
    } else {
        (w, h)
    };
    fit_size(w, h, width.max(w), height.max(h))
}

/// The frame the page paints, handed to [`install_draw`]'s draw func: the held effect's index (identity is `kind` + `t`), whether a box is under the hand, and where, via [`Held`] and [`held_frame`]. `None` index means no overlay in hand.
#[derive(Debug, Default, Clone)]
pub struct Held {
    pub index: Option<usize>,
    pub box_px: Option<(f64, f64, f64, f64)>,
}

pub fn describe(fit: (f64, f64), held: &Held) -> Frame {
    let cut = crate::ui::window_preview::newest_review_cut();
    let line = crate::ui::line_position().t;
    describe_at(
        &cut,
        line,
        crate::ui::preview_player().transport.playing,
        crate::ui::zoom_armed(),
        fit,
        held,
    )
}

/// [`describe`] with the four page answers handed in, so `snapshot_preview_frame` can paint §2's frame from a staged cut with nothing mapped.
pub fn describe_at(
    cut: &Cut,
    line: f64,
    playing: bool,
    camera_down: bool,
    fit: (f64, f64),
    held: &Held,
) -> Frame {
    Frame {
        t: line,
        held: held.index,
        drawing_box: held.box_px.is_some(),
        box_px: held.box_px,
        playing,
        camera_down,
        fit,
        cut: cut.clone(),
    }
}

/// The live clock while playing: the player's playhead plus the elapsed tick through [`preview::live_clock`] (§2's smoothed clock, so the mask does not shimmer). `None` for a never-played player ([`camera_clock`] falls back to the line). The rate is the FLAT rate of the second the line left ([`crate::cut_speed_play::flat_rate_at`]), else the mask slides off the picture under a hand-drawn speed.
pub fn live_of() -> Option<f64> {
    let player = crate::ui::preview_player();
    if !player.transport.playing {
        return None;
    }
    player.playhead.map(|at| {
        preview::live_clock(
            at,
            0,
            crate::cut_speed_play::flat_rate_at(&crate::ui::window::newest_review_cut().fx, at),
        )
    })
}

/// What one painted frame needs, gathered at the refresh from answers the page already holds; a hand-over, not a second state ([`observe`]).
#[derive(Debug, Clone)]
pub struct Frame {
    pub cut: Cut,
    pub t: f64,
    pub held: Option<usize>,
    pub drawing_box: bool,
    pub box_px: Option<(f64, f64, f64, f64)>,
    pub playing: bool,
    pub camera_down: bool,
    /// The footage's pixel box fitted onto the panel ([`fit_size`]); every px measurement is taken against it.
    pub fit: (f64, f64),
}

/// Which seconds the camera is read at: while playing the smoothed live clock (§2), else the line.
pub fn camera_clock(playing: bool, live: Option<f64>, line: f64) -> f64 {
    match (playing, live) {
        (true, Some(at)) if fx_lane::camera_clock_is_smoothed() => at,
        _ => line,
    }
}

/// The frame as the painter needs it: `held` the paused half ([`fx_lane::paused_scene`]), `titles` the playing half ([`fx_lane::playing_scene`]), `camera` at [`camera_clock`]. A stop's still borrows the scene camera ([`fx_record::is_stop`]) so it is framed by the same number as the mask.
pub struct Observed {
    pub held: PausedScene,
    pub titles: Vec<(usize, cut::EffectKind, f64)>,
    pub camera: Camera,
}

pub fn observe(frame: &Frame, live: Option<f64>) -> Observed {
    let at = camera_clock(frame.playing, live, frame.t);
    Observed {
        held: fx_lane::paused_scene(&frame.cut.fx, frame.t, frame.held, frame.drawing_box),
        titles: fx_lane::playing_scene(&frame.cut.fx, frame.t),
        camera: fx_zoom::camera_at(&frame.cut.fx, at),
    }
}

/// The rectangle the camera keeps, in px of the fitted box; §2 dims everything outside. A camera is a fraction OF THE FOOTAGE ([`fx_zoom::Camera`]: `hf`, `cx`/`cy`), converted like [`crate::fx_text::Box_::to_px`], so `fit` ([`fit_size`]) is carried in. A rect falling outside is clamped.
pub fn camera_rect_px(camera: &Camera, fit: (f64, f64)) -> (f64, f64, f64, f64) {
    let share = camera.hf.min(1.0);
    let w = share * fit.0;
    let h = share * fit.1;
    (
        (camera.cx * fit.0 - w / 2.0).max(0.0),
        (camera.cy * fit.1 - h / 2.0).max(0.0),
        w.max(1.0),
        h.max(1.0),
    )
}

/// The footage's box fitted (contained, so nothing is cropped); unknown footage (headless, no ffprobe) answers the panel itself.
pub fn fit_size(source_w: f64, source_h: f64, panel_w: f64, panel_h: f64) -> (f64, f64) {
    if source_w <= 0.0 || source_h <= 0.0 {
        return (panel_w, panel_h);
    }
    let scale = (panel_w / source_w).min(panel_h / source_h);
    (source_w * scale, source_h * scale)
}

/// A stop's still is rendered from the scene's OWN camera (§2), so it has no box and the camera supplies one ([`paint_playing`]).
pub fn stop_px(fx: &[crate::cut::Fx], at: f64, fit: (f64, f64)) -> Option<(f64, f64, f64, f64)> {
    fx.iter()
        .any(|effect| {
            crate::fx_record::is_stop(effect) && effect.spans().0 <= at && at < effect.spans().1
        })
        .then(|| camera_rect_px(&fx_zoom::camera_at(fx, at), fit))
}

/// The kinds contributing the playing mask: only a zoom's ([`fx_lane::mask_only_while_playing`]), in [`fx_lane::MENU_ORDER`]. Test-only; the painter uses [`playing_mask_px`].
pub fn mask_kinds_for_test() -> Vec<cut::EffectKind> {
    fx_lane::MENU_ORDER
        .into_iter()
        .filter(|kind| fx_lane::mask_only_while_playing(*kind))
        .collect()
}

/// The black the playing frame lays over what the finished frame hides: outside the rect a gliding camera travels to.
pub fn playing_mask_px(camera: &Camera, fit: (f64, f64)) -> Vec<(f64, f64, f64, f64)> {
    let (width, height) = fit;
    let (x, y, w, h) = camera_rect_px(camera, fit);
    let mut bands = Vec::new();
    if x > 0.0 {
        bands.push((0.0, 0.0, x, height));
    }
    if x + w < width {
        bands.push((x + w, 0.0, width - (x + w), height));
    }
    if y > 0.0 {
        bands.push((x, 0.0, w, y));
    }
    if y + h < height {
        bands.push((x, y + h, w, height - (y + h)));
    }
    bands
}

/// Where the fitted picture starts in a panel of `panel` px: contained and centred, so the letterbox bars shift the drawing.
pub fn picture_offset(panel: (f64, f64), fit: (f64, f64)) -> (f64, f64) {
    (
        ((panel.0 - fit.0) / 2.0).max(0.0),
        ((panel.1 - fit.1) / 2.0).max(0.0),
    )
}

/// [`paint`] onto a panel of `panel` px: measured in the FITTED picture's px, so the drawing is shifted onto the centred picture first (else the rect's right edge stood as a white bar).
pub fn paint_in_panel(cr: &cairo::Context, frame: &Frame, seen: &Observed, panel: (f64, f64)) {
    let (ox, oy) = picture_offset(panel, frame.fit);
    let _ = cr.save();
    cr.translate(ox, oy);
    paint(cr, frame, seen);
    let _ = cr.restore();
}

pub fn paint(cr: &cairo::Context, frame: &Frame, seen: &Observed) {
    if frame.camera_down {
        // ⊕ Zoom armed: the camera layer is DOWN so a box is drawn against the whole source (§2/F3.1 S1); no dim over what is being framed.
        return;
    }
    if frame.playing {
        paint_playing(cr, frame, seen);
        paint_titles(cr, frame, &seen.titles);
        return;
    }
    paint_paused(cr, frame, seen);
}

pub fn paint_paused(cr: &cairo::Context, frame: &Frame, seen: &Observed) {
    let fit = frame.fit;
    let (x, y, w, h) = camera_rect_px(&seen.camera, fit);
    if let Some(still) = stop_px(&frame.cut.fx, frame.t, fit) {
        paint_still(cr, still, crate::ui::cut_picture::frame_shown().as_deref());
    }
    // §F3.4 S5's snap lines are shown while a caption box is dragged, asked via `fx_text::snap_lines_px`, under the overlays.
    if edit_snap_on() {
        let violet = fx_lane::HELD_OUTLINE;
        cr.set_source_rgba(violet[0], violet[1], violet[2], 0.45);
        cr.set_line_width(1.0);
        cr.set_dash(&[3.0, 3.0], 0.0);
        for (x, y, w, h) in crate::fx_text::snap_lines_px(fit) {
            cr.rectangle(x, y, w, h);
        }
        let _ = cr.fill();
        cr.set_dash(&[], 0.0);
    }
    cr.set_source_rgba(0.0, 0.0, 0.0, seen.held.dim_alpha);
    for band in playing_mask_px(&seen.camera, fit) {
        cr.rectangle(band.0, band.1, band.2, band.3);
    }
    let _ = cr.fill();
    // "rect stroked white with a plate": the plate wording and room rules stay in `fx_lane::plate_for`/`room_px`; the surface draws the rect.
    cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    cr.set_line_width(seen.held.rect_stroke_px);
    cr.rectangle(x, y, w, h);
    let _ = cr.stroke();
    for (index, _kind, alpha) in &seen.held.overlays {
        paint_overlay(cr, frame, *index, *alpha, false);
    }
    if let Some(index) = frame.held {
        paint_overlay(
            cr,
            frame,
            index,
            if seen.held.held_full { 1.0 } else { 0.0 },
            seen.held.held_outline_dashed,
        );
    }
    if let Some((x, y, w, h)) = frame.box_px {
        draw_box_outline(cr, (x, y, w, h), seen.held.box_dashed, [1.0, 1.0, 1.0], 1.0);
    }
}

/// A stop's still, INKED: the frame already on the panel cropped to `box_` ([`stop_px`], §2) and laid opaque; no second extraction. No file, no decode or a zero box paints nothing (§8: [`crate::ui::cut_stop_frame`]).
pub fn paint_still(
    cr: &cairo::Context,
    box_: (f64, f64, f64, f64),
    frame_path: Option<&std::path::Path>,
) {
    let (x, y, w, h) = box_;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let Some(path) = frame_path else { return };
    let Some(surface) = still_surface(path) else {
        return;
    };
    let _ = cr.save();
    cr.translate(x, y);
    cr.scale(w / surface.width() as f64, h / surface.height() as f64);
    let _ = cr.set_source_surface(&surface, 0.0, 0.0);
    let _ = cr.paint();
    let _ = cr.restore();
}

/// The playing half: the mask over what the finished frame hides (none at the settled framing). PUBLIC so `snapshot_preview` can paint a stop's still and caption (§2 playing half) on its own surface.
pub fn paint_playing(cr: &cairo::Context, frame: &Frame, seen: &Observed) {
    if let Some(still) = stop_px(&frame.cut.fx, frame.t, frame.fit) {
        // A stop's still is drawn even when nothing is hidden (§2), via [`stop_px`]; ink first, then a solid outline (a dash would mean "unfinished").
        paint_still(cr, still, crate::ui::cut_picture::frame_shown().as_deref());
        draw_box_outline(cr, still, false, [1.0, 1.0, 1.0], 1.0);
    }
    if seen.camera.hf >= fx_zoom::SETTLED.hf {
        return;
    }
    cr.set_source_rgba(0.0, 0.0, 0.0, 1.0);
    for band in playing_mask_px(&seen.camera, frame.fit) {
        cr.rectangle(band.0, band.1, band.2, band.3);
    }
    let _ = cr.fill();
}

/// This pixbuf as a cairo surface (alpha premultiplied); the seam exists for one check that a raster is paintable. The rule lives in [`raster_surface`].
pub fn raster_surface_for_test(path: &std::path::Path) -> Option<cairo::ImageSurface> {
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file(path).ok()?;
    raster_surface(&pixbuf)
}

/// The drawing's box in the fitted picture's px, read like [`crate::ui::cut_preview_caption::box_of`]; no stored box takes §F3.5's `MIDDLE`.
fn svg_box_px(effect: &cut::Fx, fit: (f64, f64)) -> (f64, f64, f64, f64) {
    let (cx, cy) = effect.centre();
    fx_text::Box_ {
        cx,
        cy,
        wf: effect.wf.unwrap_or(fx_svg::MIDDLE.wf),
        hf: effect.hf.unwrap_or(fx_svg::MIDDLE.hf),
    }
    .to_px(fit.0, fit.1)
}

/// The still's surface if decoded for this file version; otherwise a worker is asked and the window repaints when it lands. A snapshot decodes on the spot.
fn still_surface(path: &std::path::Path) -> Option<cairo::ImageSurface> {
    if crate::ui::cut_picture_rows::frame_decoding_allowed() {
        let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file(path).ok()?;
        return raster_surface(&pixbuf);
    }
    let stamp = std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok());
    let held = CUT_PAGE.with(|CutPage { still, .. }| {
        still
            .borrow()
            .as_ref()
            .filter(|(file, at, _)| file == path && *at == stamp)
            .map(|(_, _, surface)| surface.clone())
    });
    if let Some(surface) = held {
        return surface;
    }
    let key = (path.to_path_buf(), stamp);
    if CUT_PAGE.with(
        |CutPage {
             still_asked: asked, ..
         }| asked.borrow().as_ref() == Some(&key),
    ) {
        return None;
    }
    CUT_PAGE.with(
        |CutPage {
             still_asked: asked, ..
         }| *asked.borrow_mut() = Some(key.clone()),
    );
    let file = path.to_path_buf();
    crate::ui::background::run(
        "still",
        move || {
            gtk::gdk_pixbuf::Pixbuf::from_file(&file)
                .ok()
                .and_then(|pixbuf| raster_pixels(&pixbuf))
        },
        move |pixels| {
            let surface = pixels.and_then(|(bytes, w, h, stride)| {
                cairo::ImageSurface::create_for_data(bytes, cairo::Format::ARgb32, w, h, stride)
                    .ok()
            });
            CUT_PAGE.with(|CutPage { still, .. }| {
                *still.borrow_mut() = Some((key.0.clone(), key.1, surface))
            });
            CUT_PAGE.with(
                |CutPage {
                     still_asked: asked, ..
                 }| {
                    let mut asked = asked.borrow_mut();
                    if asked.as_ref() == Some(&key) {
                        *asked = None;
                    }
                },
            );
            if let Some(window) = crate::ui::window::built_window() {
                gtk::prelude::WidgetExt::queue_draw(&window);
            }
        },
    );
    None
}

/// This pixbuf as a cairo surface (premultiplied); `None` if a step refuses (a png that will not decode is a missing raster).
fn raster_surface(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> Option<cairo::ImageSurface> {
    let (pixels, w, h, stride) = raster_pixels(pixbuf)?;
    cairo::ImageSurface::create_for_data(pixels, cairo::Format::ARgb32, w, h, stride).ok()
}

/// The pixels behind [`raster_surface`]: cairo's premultiplied ARgb32 bytes, width, height and stride, so a worker can make them.
fn raster_pixels(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> Option<(Vec<u8>, i32, i32, i32)> {
    let (w, h) = (pixbuf.width(), pixbuf.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let channels = pixbuf.n_channels().max(3) as usize;
    let stride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let src = &bytes[..];
    let has_alpha = pixbuf.has_alpha();
    // The buffer is filled BEFORE cairo borrows it; there is no `modify_data` afterwards.
    let out_stride = (w * 4) as usize;
    let mut pixels = vec![0u8; out_stride * h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let from = y * stride + x * channels;
            if from + 2 >= src.len() {
                break;
            }
            let (r, g, b) = (src[from] as u32, src[from + 1] as u32, src[from + 2] as u32);
            let a = if has_alpha { src[from + 3] as u32 } else { 255 };
            // Premultiply, then pack B,G,R,A little-endian (cairo ARgb32 here).
            let word = (b * a / 255) | ((g * a / 255) << 8) | ((r * a / 255) << 16) | (a << 24);
            let at = y * out_stride + x * 4;
            pixels[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    Some((pixels, w, h, out_stride as i32))
}

fn paint_overlay(cr: &cairo::Context, frame: &Frame, index: usize, alpha: f64, outline: bool) {
    if alpha <= 0.0 {
        return;
    }
    let Some(effect) = frame.cut.fx.get(index) else {
        return;
    };
    if !fx_lane::drawn_paused(effect.effect_kind().expect("filtered by paused_scene")) {
        return;
    }
    // F3.4 S4: a caption is laid out by `cut_preview_caption` via `fx_text::fit` (the thumbnail's fitter); the generic path below is for a drawing, whose words are its file.
    if effect.effect_kind() == Some(cut::EffectKind::Text) {
        crate::ui::cut_preview_caption::draw(cr, effect, frame.fit, alpha);
        if outline {
            let box_ = crate::ui::cut_preview_caption::box_of(effect, frame.fit);
            draw_box_outline(cr, box_, true, fx_lane::HELD_OUTLINE, alpha);
        }
        return;
    }
    // F3.5 S5: a drawing is its FILE, so paint the ffmpeg raster into `fx_svg::fit_inside`. With no raster (§A.5 failure path) the ink is missing but the box is still outlined.
    if effect.effect_kind() == Some(cut::EffectKind::Svg) {
        let box_ = svg_box_px(effect, frame.fit);
        let raster = crate::ui::cut_svg::raster_of(&effect.src)
            .and_then(|file| gtk::gdk_pixbuf::Pixbuf::from_file(file).ok());
        // The png's width over height is the svg's shape (`-keep_ar 1`); no raster, no known shape, and `fit_inside` answers the box.
        let aspect = raster
            .as_ref()
            .map(|pixbuf| pixbuf.width() as f64 / (pixbuf.height().max(1)) as f64)
            .unwrap_or(0.0);
        let (dx, dy, dw, dh) = fx_svg::fit_inside(box_.2, box_.3, aspect);
        let drawn = (box_.0 + dx, box_.1 + dy, dw, dh);
        if let Some(pixbuf) = raster.filter(|_| dw > 0.0 && dh > 0.0) {
            // gdk-pixbuf gives RGBA; cairo ARgb32 is native BGRA with PREMULTIPLIED alpha, converted per pixel so a transparent card stays transparent (§S5). gtk4-rs 0.11 lacks `set_source_pixbuf`, so via [`raster_surface`]. An unreadable png costs the ink, not the placement.
            if let Some(surface) = raster_surface(&pixbuf) {
                // cairo errors are ignored here as `show_text` is: an unusable surface makes a half-drawn card no worse than none.
                let _ = cr.save();
                cr.translate(drawn.0, drawn.1);
                cr.scale(dw / surface.width() as f64, dh / surface.height() as f64);
                let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                let _ = cr.paint_with_alpha(alpha);
                let _ = cr.restore();
            }
        }
        if outline {
            draw_box_outline(cr, drawn, true, fx_lane::HELD_OUTLINE, alpha);
        }
        return;
    }
    let (width, height) = frame.fit;
    // A caption is placed by its stored fractions (§1); none stored sits at the bottom (§06 screenshots).
    let cx = effect.cx.unwrap_or(0.5);
    let cy = effect.cy.unwrap_or(0.85);
    let box_ = (
        cx * width - width * 0.35,
        cy * height - 12.0,
        width * 0.7,
        24.0,
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(13.0);
    let words = &effect.text;
    // A wordless record still gets its outline.
    if let Ok(extents) = cr.text_extents(words) {
        if !words.is_empty() {
            cr.move_to(
                box_.0 + box_.2 / 2.0 - extents.width() / 2.0 - extents.x_bearing(),
                box_.1 + 16.0,
            );
            let _ = cr.show_text(words);
        }
    }
    if outline {
        draw_box_outline(cr, box_, true, fx_lane::HELD_OUTLINE, alpha);
    }
}

/// The titles the playing frame keeps (§2 "plus titles") at real alpha ([`fx_lane::drawn_while_playing`]), drawn as a paused overlay is.
fn paint_titles(cr: &cairo::Context, frame: &Frame, titles: &[(usize, cut::EffectKind, f64)]) {
    for (index, _kind, alpha) in titles {
        paint_overlay(cr, frame, *index, *alpha, false);
    }
}

/// A dashed (or solid) outline in `colour` for the held overlay and the box under the hand. The dash is RESET afterwards (cairo's dash list is per-context).
fn draw_box_outline(
    cr: &cairo::Context,
    (x, y, w, h): (f64, f64, f64, f64),
    dashed: bool,
    colour: [f64; 3],
    alpha: f64,
) {
    cr.set_source_rgba(colour[0], colour[1], colour[2], alpha);
    cr.set_line_width(fx_lane::RECT_STROKE_PX);
    if dashed {
        cr.set_dash(&[4.0, 3.0], 0.0);
    }
    cr.rectangle(x, y, w, h);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
}
