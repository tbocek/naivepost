//! §06-effects#2-the-lane-and-the-preview — the preview's real frame, painted on cairo.
//!
//! Before this module the panel was a row of CSS plates: a dim box, a rect box, an overlay stack and a
//! label, side by side in a horizontal Box. That can state each rule separately (and the widget tests read
//! them that way) but it cannot be the thing §2 describes — a frame, with the area outside the camera rect
//! dimmed, the rect drawn over the picture, and overlays sitting where they will be seen. So the panel grows
//! ONE painted surface that draws exactly that, while the plate widgets stay in the tree as the state a test
//! reads. Nothing is moved out from under `preview-camera-rect`'s opacity assertions
//! (`zoom_drag_widgets.rs` F3.1 S1): while ⊕ Zoom is armed the armed flag reaches this painter too, and both
//! surfaces put the camera layer down together.
//!
//! The painter composes no rule of its own. The dim's alpha, the stroke's width, the held outline's colour
//! and the dash choices all come from [`fx_lane`]; the camera's position comes from [`fx_zoom`]. One entry
//! per §2 clause, in the order §2 lists them.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self, Cut};
use crate::fx_lane::{self, PausedScene};
use crate::fx_svg;
use crate::fx_text;
use crate::fx_zoom::{self, Camera};
use crate::preview;

/// The painted surface's widget name. A test that wants to know the panel is drawn rather than plates-only
/// finds this; the rules themselves are asserted against `fx_lane`'s view-model, which is what the painter
/// reads.
pub const FRAME_NAME: &str = "preview-frame";

/// The one CSS rule this module's state needs, pushed into the page's provider by `install_lane_css_once`.
/// `.zoom-camera-down` carries no colour on purpose: the arm's look is decided by this painter (which draws
/// no dim while the camera layer is down) and by `preview-camera-rect`'s opacity. The class is the STATE a
/// widget test reads off the panel, added and removed off `zoom_armed()`, never a second paint.
pub fn css() -> String {
    ".zoom-camera-down { }\n".to_string()
}

/// This window's painted surface, resolved through its own content so a closed window's surviving tree
/// cannot answer for a live one — the same scoping `preview_panel` uses.
pub fn frame_of(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, FRAME_NAME)?
        .downcast::<gtk::DrawingArea>()
        .ok()
}

/// The panel's painted surface, built once by the page and handed a draw func that reads this window's live
/// state. It is laid OVER the plate widgets rather than replacing them: the plates stay in the tree as the
/// state a test reads (`preview-camera-rect`'s opacity still answers F3.1 S1), while the surface is what
/// actually looks like a frame.
pub fn build() -> gtk::DrawingArea {
    let frame = gtk::DrawingArea::new();
    frame.set_widget_name(FRAME_NAME);
    // The dim, the stroked camera rect and every overlay are drawn in THIS surface's pixels, so it has to
    // cover the whole picture. A fixed 240x135 request plus `set_clip_overlay(true)` clipped the surface --
    // and with it the white camera rect -- to a small square in the middle of a wider panel, which is why
    // badge 1 read as an empty plate once §A's paned gave the preview its half of the page.
    frame.set_hexpand(true);
    frame.set_vexpand(true);
    // No size request at all: the floor belongs to `preview-panel` (§A's "min height 160" family), and a
    // second one here fought the panel for the measure/clip marks `page_box` sets on this overlay.
    frame
}

/// Ask the surface to paint itself again. The refresh calls this once it has pushed every value into the
/// plate widgets, so the painted frame and the plate states cannot disagree about which refresh they show.
pub fn repaint(window: &adw::ApplicationWindow) {
    if let Some(frame) = frame_of(window) {
        frame.queue_draw();
    }
}

/// The draw func the page installs on the surface: it takes the window's own answers through [`observe`] and
/// hands them to [`paint`], so nothing here composes a rule and nothing here holds state. The page's drag
/// module calls [`note_box`] while a box is under the hand; every other refresh just repaints.
pub fn install_draw(window: &adw::ApplicationWindow) {
    let held = window.clone();
    if let Some(frame) = frame_of(&held) {
        frame.set_draw_func(move |_, cr, width, height| {
            let fit = fit_size_of(&held, width as f64, height as f64);
            let cut = crate::ui::review_cut_of(&held);
            let frame = describe(&held, fit, &held_frame(&cut, drawn_box()));
            let seen = observe(&frame, live_of(&held));
            paint(cr, &frame, &seen);
        });
    }
}

thread_local! {
    /// The box currently under the hand, in panel px — written by [`note_box`] from the drag that draws it and
    /// read at paint time. One slot per thread, the window's own convention for a value a gesture holds.
    static DRAWN_BOX: std::cell::Cell<Option<(f64, f64, f64, f64)>> =
        const { std::cell::Cell::new(None) };
}

/// The box being drawn right now, or `None`. Read by [`install_draw`]'s draw func.
pub fn drawn_box() -> Option<(f64, f64, f64, f64)> {
    DRAWN_BOX.with(|slot| slot.get())
}

thread_local! {
    /// Whether the §F3.4 S5 snap lines are being shown: set while a hand drags a placed caption's box, so the frame
    /// draws the six lines `fx_text::snap_lines_px` names (see [`paint_paused`]). A thread-local like `DRAWN_BOX`,
    /// for the same reason: the draw func gets only the widget, and this is per-window state the gesture sets.
    static EDIT_SNAP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Say that a caption box is being dragged: the frame then draws the snap targets under its outline. No repaint
/// here — the caller (`wire_zoom_drag`'s update leg) repaints once for both this and the box itself.
pub fn note_edit(active: bool) {
    EDIT_SNAP.with(|slot| slot.set(active));
}

/// Are the snap lines on? The door a check reads rather than the thread-local.
pub fn edit_snap_on() -> bool {
    EDIT_SNAP.with(|slot| slot.get())
}

/// This window's preview surface is a fixed 240 x 135 ([`crate::ui::window::PREVIEW_PANEL_W`] x
/// [`crate::ui::window::PREVIEW_PANEL_H`]), so a box the drag drew in its pixels is already in the panel's own
/// px: this is the door the drag calls with them. The corners arrive in whatever order the hand moved them, so
/// they are normalised here — a negative width paints an outline nothing can see.
pub fn note_box(box_: Option<(f64, f64, f64, f64)>, window: &adw::ApplicationWindow) {
    DRAWN_BOX.with(|slot| {
        slot.set(box_.map(|(x, y, w, h)| {
            (
                x.min(x + w),
                y.min(y + h),
                w.abs().max(1.0),
                h.abs().max(1.0),
            )
        }))
    });
    repaint(window);
}

/// The record in hand, resolved to this cut's index, and the box under the hand if one is being drawn. The
/// page hands these to [`describe`]; a held record that is not on this cut holds no overlay.
pub fn held_frame(cut: &Cut, box_px: Option<(f64, f64, f64, f64)>) -> Held {
    let index = crate::ui::held_effect().and_then(|held| {
        cut.fx
            .iter()
            .position(|effect| effect.kind == held.kind && effect.t == held.t)
    });
    // §2's "a box being drawn dashed": a box is only under the hand while an arm that DRAWS one is up — ⊕ Zoom,
    // ▨ SVG or ❝ Text, each of which drags its rectangle on this panel. Once the drag ended the record is placed,
    // and continuing to dash its box would claim the hand is still there (§2: an unfinished one is dashed).
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

/// The footage's box on this panel. The panel pins itself to
/// [`crate::ui::window::PREVIEW_PANEL_W`] x [`crate::ui::window::PREVIEW_PANEL_H`] and no
/// ffprobe runs headless, so the fit is taken against the panel's own declared shape — the same answer
/// `preview_picture_px` gives the drag, and for the same reason: reading an allocation that differs between an
/// isolated test and a loaded suite makes a fraction land on the clamp depending on scheduler luck.
pub fn fit_size_of(window: &adw::ApplicationWindow, width: f64, height: f64) -> (f64, f64) {
    let (w, h) = crate::ui::window::preview_picture_px(window);
    // §9 (`both MUST use the first footage clip's frame`): the shape the fractions are
    // multiplied by is the plate's agreed shape — the first footage recording's, decided in
    // `cut_picture::refresh` through `effect_overlap::source_frame` — not whatever camera a
    // watch happens to be showing. This is the one place preview fractions become pixels.
    let (sw, sh) = crate::ui::cut_picture::plate_shape(window);
    let (w, h) = if sw > 0 && sh > 0 {
        (sw as f64, sh as f64)
    } else {
        (w, h)
    };
    fit_size(w, h, width.max(w), height.max(h))
}

/// The frame the page paints, handed to [`install_draw`]'s draw func at paint time. The three numbers the
/// panel's surface needs that only the page holds — the effect in hand's index (a lane of copies is told apart
/// by position, so identity is `kind` + `t`), whether a box is under the hand, and where that box sits — come
/// in as [`Held`], resolved through [`held_frame`] at paint time. `None` for the index simply means no overlay
/// is in hand.
#[derive(Debug, Default, Clone)]
pub struct Held {
    /// Index into `cut.fx` of the record in hand.
    pub index: Option<usize>,
    /// ⊕ Zoom armed and a box is being dragged: its px box on the panel.
    pub box_px: Option<(f64, f64, f64, f64)>,
}

/// The page's answers for one frame, read at paint time so the surface can never show a stale cut.
pub fn describe(window: &adw::ApplicationWindow, fit: (f64, f64), held: &Held) -> Frame {
    let cut = crate::ui::review_cut_of(window);
    let line = crate::ui::line_position(window).t;
    describe_at(
        &cut,
        line,
        crate::ui::preview_player(window).transport.playing,
        crate::ui::zoom_armed(),
        fit,
        held,
    )
}

/// [`describe`] with the four page answers handed in instead of read from a window — the same struct, one fewer
/// door into window state. Split out so `snapshot_preview_frame` can paint §2's frame from a staged cut with
/// nothing mapped, which is what its `-frame.png` sidecar needs; the window keeps answering all four itself.
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

/// The live clock while playing: the player's own playhead plus the elapsed part of the current tick,
/// through [`preview::live_clock`] — the smoothed clock §2 names, which is what keeps the mask from shimmering
/// off the picture between ticks. A player that has never been played answers `None`, and [`camera_clock`]
/// falls back to the line.
///
/// The rate is the FLAT rate of the second the line left ([`crate::cut_speed_play::flat_rate_at`]), not 1: §2's
/// one rate per stretch is the same number the tick advanced by, so reading the camera at rate 1 while the line
/// runs at ×2 would slide the mask off the picture exactly when a hand-drawn speed is on screen.
pub fn live_of(window: &adw::ApplicationWindow) -> Option<f64> {
    let player = crate::ui::preview_player(window);
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

/// What one painted frame needs, gathered at the refresh that asks for the repaint. The page holds every
/// piece of it already (the cut, the line, the effect in hand, the transport), so this is a hand-over, not a
/// second state: [`observe`] builds it from the same answers `refresh_effects_lane` already took.
#[derive(Debug, Clone)]
pub struct Frame {
    /// This window's cut, effects included.
    pub cut: Cut,
    /// The session second the red line stands on.
    pub t: f64,
    /// Index into `cut.fx` of the effect in hand, if any.
    pub held: Option<usize>,
    /// A box still under the pointer (⊕ Zoom armed and dragging).
    pub drawing_box: bool,
    /// The arm's own box in panel px, `Some` only while a box is being drawn.
    pub box_px: Option<(f64, f64, f64, f64)>,
    /// `▶` — the playing half draws the mask and the titles only.
    pub playing: bool,
    /// The camera layer is down (⊕ Zoom armed): the whole source shows, so no rect and no dim.
    pub camera_down: bool,
    /// The footage's pixel box once fitted onto the panel ([`fit_size`]) — every px measurement in this module
    /// is taken against it, so the picture's own size and the panel's are never conflated.
    pub fit: (f64, f64),
}

/// Which seconds the camera should be read at. While playing that is the smoothed live clock (§2: "camera
/// layer on the smoothed live clock") because the playhead only moves once a tick and the mask would drift
/// off the picture between ticks; otherwise the line.
pub fn camera_clock(playing: bool, live: Option<f64>, line: f64) -> f64 {
    match (playing, live) {
        (true, Some(at)) if fx_lane::camera_clock_is_smoothed() => at,
        _ => line,
    }
}

/// The frame as the painter needs it: `held` answers the paused half ([`fx_lane::paused_scene`]), `titles`
/// the playing half ([`fx_lane::playing_scene`]), `camera` the framing at [`camera_clock`]. The camera is read
/// on the smoothed clock, so a stop's still — which borrows its framing from the scene and never invents one
/// ([`fx_lane::still_come_from_scene_camera`]) — is framed by the same number as the mask it sits under.
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

/// The rectangle the camera keeps, in the px of the box it is fitted into — the rect §2 dims everything
/// outside of.
///
/// A camera is a fraction OF THE FOOTAGE ([`fx_zoom::Camera`]: `hf` the height fraction, `cx`/`cy` the centre),
/// and the footage is fitted onto the panel before anything is drawn on it, so the conversion follows the same
/// shape §B's caption box uses ([`crate::fx_text::Box_::to_px`] — centre fraction × the box's own size, the
/// width and height half off each side). `fit` is that fitted size ([`fit_size`]), which is why the footage's
/// pixel size has to be carried in at all rather than assuming the panel already is the picture. A rect that
/// would fall outside is clamped to it rather than painted off the edge.
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

/// The footage's pixel box once fitted into the panel: contained, so nothing is cropped and the letterbox
/// bars are what the camera is measured against. Unknown footage (no ffprobe ran — the headless case) answers
/// the panel itself rather than dividing by zero.
pub fn fit_size(source_w: f64, source_h: f64, panel_w: f64, panel_h: f64) -> (f64, f64) {
    if source_w <= 0.0 || source_h <= 0.0 {
        return (panel_w, panel_h);
    }
    let scale = (panel_w / source_w).min(panel_h / source_h);
    (source_w * scale, source_h * scale)
}

/// A stop's still is the one record that breaks that rule: it is rendered from the scene's OWN camera (§2), so
/// it has no box of its own and the camera supplies one. [`paint_playing`] asks where that still sits.
pub fn stop_px(fx: &[crate::cut::Fx], at: f64, fit: (f64, f64)) -> Option<(f64, f64, f64, f64)> {
    fx.iter()
        .any(|effect| {
            fx_lane::still_come_from_scene_camera(effect)
                && effect.spans().0 <= at
                && at < effect.spans().1
        })
        .then(|| camera_rect_px(&fx_zoom::camera_at(fx, at), fit))
    // Framed exactly as the frame it stands in: the camera already there, fitted on the footage.
}

/// Which kinds contribute the playing frame's black mask: a zoom's, and only a zoom's
/// ([`fx_lane::mask_only_while_playing`]), in [`fx_lane::MENU_ORDER`]. TEST-ONLY: the painter takes the mask
/// from [`playing_mask_px`], the same rule in px; this answers the KIND list so it can be checked against
/// every kind instead of a list written out again here.
pub fn mask_kinds_for_test() -> Vec<cut::EffectKind> {
    fx_lane::MENU_ORDER
        .into_iter()
        .filter(|kind| fx_lane::mask_only_while_playing(*kind))
        .collect()
}

/// The black the playing frame lays over what the finished frame hides. While a camera is gliding the hidden
/// area is everything OUTSIDE the rect it is travelling to, so the mask is the dim's shape in solid black.
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

/// Paint one frame of the preview. Called by the panel's own draw func, which hands over the cairo
/// context and the allocation; everything about what to draw arrives in `frame` plus `seen`.
///
/// Paused (§2's order): dim outside the camera rect, the rect stroked white, every overlay at its real
/// alpha, the held one full with the dashed violet outline, a box under the hand dashed. Playing: only the
/// black mask over what the finished frame hides, plus the titles.
pub fn paint(cr: &cairo::Context, frame: &Frame, seen: &Observed) {
    if frame.camera_down {
        // ⊕ Zoom armed: the camera layer is DOWN so a box is drawn against the whole source (§2/F3.1 S1).
        // Nothing above the picture is painted either — a dim over the frame you are framing would hide it.
        return;
    }
    if frame.playing {
        paint_playing(cr, frame, seen);
        paint_titles(cr, frame, &seen.titles);
        return;
    }
    paint_paused(cr, frame, seen);
}

/// The paused half: the dim, the stroked rect, then the overlays.
pub fn paint_paused(cr: &cairo::Context, frame: &Frame, seen: &Observed) {
    let fit = frame.fit;
    let (x, y, w, h) = camera_rect_px(&seen.camera, fit);
    // A stop standing at the line's second holds its frame while the clock runs, and the paused half used to show
    // none of that: the same borrowed box and the same inked still as the playing half, under the dim so the
    // camera's own framing still reads around it (§2 puts the still on the footage's transform, not above the crop).
    if let Some(still) = stop_px(&frame.cut.fx, frame.t, fit) {
        paint_still(cr, still, crate::ui::cut_picture::frame_shown().as_deref());
    }
    // §F3.4 S5's snap is invisible unless the lines it snaps TO are shown: while a placed caption's box is being
    // dragged, the frame draws the six lines `fx_text::snap_lines_px` names — asked through that door, never
    // redrawn from a list this file remembers. Under the overlays, so the box's own outline stays on top of them.
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
    // "outside the camera rect dimmed black 0.45": four bands, never a wash over the whole panel.
    cr.set_source_rgba(0.0, 0.0, 0.0, seen.held.dim_alpha);
    for band in playing_mask_px(&seen.camera, fit) {
        cr.rectangle(band.0, band.1, band.2, band.3);
    }
    let _ = cr.fill();
    // "rect stroked white with a plate": the plate itself is the `preview-plate` label, whose wording,
    // minimum width and no-room-before-layout rules stay in `fx_lane::plate_for`/`room_px` and are pushed in
    // by the refresh; the surface only draws the rect the plate sits on.
    cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    cr.set_line_width(seen.held.rect_stroke_px);
    cr.rectangle(x, y, w, h);
    let _ = cr.stroke();
    for (index, _kind, alpha) in &seen.held.overlays {
        paint_overlay(cr, frame, *index, *alpha, false);
    }
    // The record in hand: full alpha, dashed violet outline (§2), drawn last so the outline is not covered.
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

/// A stop's still, INKED: the frame already on the panel, cropped to `box_` and laid down opaque.
///
/// §2 renders a stop's still "from the scene's own camera, fitted on the footage's transform", and the preview
/// must not go and extract a second frame to do it — the picture the person is looking at IS the held frame, so
/// this paints that file into the box [`stop_px`] borrowed from the camera. No file, no decode, or a zero box:
/// nothing is painted and nothing is said here (§8 keeps that in [`crate::ui::cut_stop_frame`], and a stop the
/// preview cannot show stays on the lane regardless).
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
    let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file(path) else {
        return;
    };
    let Some(surface) = raster_surface(&pixbuf) else {
        return;
    };
    // Same scaling trick the svg overlay uses: translate/scale so the surface lands exactly on the borrowed box.
    let _ = cr.save();
    cr.translate(x, y);
    cr.scale(w / surface.width() as f64, h / surface.height() as f64);
    let _ = cr.set_source_surface(&surface, 0.0, 0.0);
    let _ = cr.paint();
    let _ = cr.restore();
}

/// The playing half: the mask over what the finished frame hides, nothing else of the paused furniture.
/// A camera at the settled framing hides nothing, so there is no mask to lay down.
///
/// PUBLIC because the `06-preview` screen shows a stop's still and a caption, and §2 describes BOTH on the
/// playing half ("Playing: only the black mask over what the finished frame hides, plus titles; … A stop's still").
/// The staging paints that half on its own surface rather than hoping a headless window's own draw func runs
/// before the pixels are taken — `snapshot_preview` holds the cut, the fitted box and the frame file already.
pub fn paint_playing(cr: &cairo::Context, frame: &Frame, seen: &Observed) {
    if let Some(still) = stop_px(&frame.cut.fx, frame.t, frame.fit) {
        // A stop's still is drawn even when nothing is hidden: §2 renders it "from the scene's own camera,
        // fitted on the footage's transform", and [`stop_px`] is the only door to that borrowed box. Ink first,
        // then the outline on top of it — stroked solid, because a dash here would mean "unfinished" and a
        // placed stop is not.
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

/// The preview's own pixels: this pixbuf as a cairo surface, alpha premultiplied on the way in. The seam exists
/// for one check (a test wants to prove a raster file is the kind of thing the preview can actually paint), and
/// the rule behind it — gdk-pixbuf's RGBA into cairo's premultiplied ARgb32 — lives once in
/// [`raster_surface`], which this only forwards to.
pub fn raster_surface_for_test(path: &std::path::Path) -> Option<cairo::ImageSurface> {
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file(path).ok()?;
    raster_surface(&pixbuf)
}

/// The drawing's box in the fitted picture's px — the same four fractions §06#1 stores for an overlay, read the way
/// [`crate::ui::cut_preview_caption::box_of`] reads a caption's so a dragged box cannot mean two different rectangles
/// depending on which kind of thing was dragged. A record with no stored box takes §F3.5's default, `MIDDLE`.
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

/// This pixbuf as a cairo surface, alpha premultiplied on the way in (see the caller). `None` if either step
/// refuses — a png that will not decode is a missing raster, and the caller treats it as one.
fn raster_surface(pixbuf: &gtk::gdk_pixbuf::Pixbuf) -> Option<cairo::ImageSurface> {
    let (w, h) = (pixbuf.width(), pixbuf.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let channels = pixbuf.n_channels().max(3) as usize;
    let stride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let src = &bytes[..];
    let has_alpha = pixbuf.has_alpha();
    // The buffer is filled BEFORE cairo is handed it: `create_for_data` borrows the vec for the surface's life, so
    // there is no `modify_data` to write through afterwards, and writing after the hand-over would be writing
    // through a borrow the surface already owns.
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
            // Premultiply, then pack B,G,R,A little-endian: what cairo's ARgb32 means on this machine.
            let word = (b * a / 255) | ((g * a / 255) << 8) | ((r * a / 255) << 16) | (a << 24);
            let at = y * out_stride + x * 4;
            pixels[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
    cairo::ImageSurface::create_for_data(pixels, cairo::Format::ARgb32, w, h, out_stride as i32)
        .ok()
}

/// One overlay: the caption's words or a drawing's file, wherever the record says, at `alpha`.
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
    // F3.4 S4 (`same function for preview and render`): a caption is laid out by `cut_preview_caption`, which
    // asks `fx_text::fit` — the thumbnail's own fitter — for its size and lines. The generic path below stays for
    // a drawing, whose words are its file rather than its `text`.
    if effect.effect_kind() == Some(cut::EffectKind::Text) {
        crate::ui::cut_preview_caption::draw(cr, effect, frame.fit, alpha);
        if outline {
            let box_ = crate::ui::cut_preview_caption::box_of(effect, frame.fit);
            draw_box_outline(cr, box_, true, fx_lane::HELD_OUTLINE, alpha);
        }
        return;
    }
    // F3.5 S5 (`preview raster 512 px, cached per file`): a drawing is its FILE, so the preview paints the raster
    // ffmpeg made for it — into `fx_svg::fit_inside`, the one place preview and render agree that "the drawing keeps
    // its own shape inside it" (see [`crate::fx_svg::fit_inside`]'s own note). The path comes from the program's own
    // cache, never a guess: with no raster (§A.5's failure path) the INK is missing but the placement is not, so the
    // box is still outlined and the person can see where the drawing will sit.
    if effect.effect_kind() == Some(cut::EffectKind::Svg) {
        let box_ = svg_box_px(effect, frame.fit);
        let raster = crate::ui::cut_svg::raster_of(&effect.src)
            .and_then(|file| gtk::gdk_pixbuf::Pixbuf::from_file(file).ok());
        // The drawing's own shape: the png's width over its height, which is the svg's shape because ffmpeg was
        // asked to keep it (`-keep_ar 1`). No raster means no known shape, and `fit_inside` then answers the box.
        let aspect = raster
            .as_ref()
            .map(|pixbuf| pixbuf.width() as f64 / (pixbuf.height().max(1)) as f64)
            .unwrap_or(0.0);
        let (dx, dy, dw, dh) = fx_svg::fit_inside(box_.2, box_.3, aspect);
        let drawn = (box_.0 + dx, box_.1 + dy, dw, dh);
        if let Some(pixbuf) = raster.filter(|_| dw > 0.0 && dh > 0.0) {
            // gdk-pixbuf gives RGBA bytes; cairo's ARgb32 is native-endian BGRA with PREMULTIPLIED alpha, so the
            // copy converts both per pixel — that is what keeps a transparent card transparent over the video,
            // which is exactly what §S5's "transparent" asks for. `set_source_pixbuf` would be one line, but gtk4-rs
            // 0.11 does not expose it, so the pixels are carried by hand through [`raster_surface`].
            // An unreadable png costs the ink, not the placement: the outline below still says where it goes.
            if let Some(surface) = raster_surface(&pixbuf) {
                // `translate`/`scale` return nothing (they only mark the context on error); `save`,
                // `set_source_surface` and `paint_with_alpha` return a Result, which is ignored the way this
                // painter already ignores `show_text` — a cairo error here means the surface is already unusable,
                // and a half-drawn card is no worse than none for a preview.
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
    // A caption is placed by its own stored fractions (§1's table); with none stored it sits at the bottom,
    // which is where §06's screenshots put a caption that was typed rather than dragged.
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
    // A wordless record (a drawing's words are its file, not its `text`) still gets its outline, so the
    // centring below only happens when there is something to centre.
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

/// The titles the playing frame keeps (§2: "plus titles"), at their real alpha — the same kinds
/// [`fx_lane::drawn_while_playing`] answers, drawn by the same hand as a paused overlay so a caption cannot
/// look different in the two halves.
fn paint_titles(cr: &cairo::Context, frame: &Frame, titles: &[(usize, cut::EffectKind, f64)]) {
    for (index, _kind, alpha) in titles {
        paint_overlay(cr, frame, *index, *alpha, false);
    }
}

/// A dashed (or solid) outline in `colour`, used by the held overlay and by a box under the hand. The dash
/// is one call and it is RESET afterwards: cairo's dash list is per-context, and leaving it set would dash
/// the next thing painted, which in a refresh loop is the following overlay rather than nothing.
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
