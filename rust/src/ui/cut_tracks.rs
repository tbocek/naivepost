//! §05-cut.md §1 "Tracks" — the band's real drawing, from state alone.
//!
//! §1 lists what the band wears: a ruler with its tick labels, a green bar per kept scene, fold badges in
//! dropped gaps, the blue selection with its ends, the effects lane, picture rows, amber edges where a
//! recording starts and ends, the yellow wash over a take the join pass removed whole, a rose wash under a
//! speed effect, violet insert bands, in/out flags, a dashed outline around the watched row, and pinned
//! name plates. NONE of it is a rule: every x comes from `cut_trim::clip_boxes`, every gap from
//! `cut_fold::dropped_gaps`, every lane row from `fx_lane::lane_layout`, every plate from the cut's own
//! `shift`/`lanes`. This module only turns those numbers into ink, which is why it can be painted into a
//! memory surface and measured a pixel at a time (§00-principles §5).
//!
//! The red playhead is NOT here. §1 gives it "its own layer across both bands", so it has its own surface
//! and its own paint ([`paint_playhead`]) — a line drawn inside the band would be repainted by every
//! selection change and would drag the whole band's work with each tick.

use adw::prelude::*;
use gtk4 as gtk;

use crate::{
    clock, cut, cut_fold, cut_screen, cut_trim, fx_lane, layout, timeline, tools,
};

/// The band's tints come from [`cut_colours`], catalogued by `params::cut()` under `cut.colour.*`: §6 lists
/// "colours" among the values a tuning pass may move, so no drawing helper holds one of its own any more.
use crate::ui::cut_colours::{
    EDGE_AMBER, GROUND, INSERT_VIOLET, JOIN_WASH, KEPT_BORDER, KEPT_TINT, LANE_BORDER, LANE_GROUND,
    MARK_IN_GREEN, MARK_OUT_RED, OVER_EATEN, PLAYHEAD_RGB, RULER_INK, SELECTION_BLUE, WATCH_OUTLINE,
};
/// §1's playhead width: "2 px line on its own layer". Its colour is [`PLAYHEAD_RGB`].
pub const PLAYHEAD_PX: f64 = 2.0;
/// §A: the ruler is 18 px, the selection band 22 px, an effects row 26 (`fx_lane::lane_height_px(1)`), a
/// picture row `thumbHt + 4`. The ruler's 16 vs 18 is the placeholder's number kept: `cut_trim::RULER_H`
/// is what F2.8's press bands resolve against, and moving it would move a press.
pub const SELECTION_BAND_PX: f64 = 22.0;
pub const PICTURE_ROW_EXTRA_PX: f64 = 4.0;
/// The wave strip's share of a camera row (§27 Tracks: "each row's first audio track as a strip below
/// it"). No §10 row gives this height, so the painter chooses it, exactly as `cut_trim_press::{LANE_H,
/// WAVE_H}` does for the placeholder strip's own rows: the lower third of the row's pitch, which leaves the
/// upper two thirds to the thumbnail and keeps every press y that meant "row N" meaning row N.
pub const ROW_WAVE_PX: f64 = (THUMB_DEFAULT_PX as f64 + PICTURE_ROW_EXTRA_PX) / 3.0;
/// A flag's triangle: 3 px of line (§1) with a pennant this wide and tall.
const MARK_LINE_PX: f64 = 3.0;
const FLAG_PX: f64 = 10.0;

/// §A's tick ladder: `0.2 0.5 1 2 5 10 30 60 120 300 600 s`, the FIRST one whose spacing at this zoom is at
/// least 70 px apart — a label every 30 px would be a grey bar, and every 700 px a ruler with no marks.
/// Kept here rather than in `cut_screen` because nothing but the painter asks for it.
pub fn tick_step_seconds(pps: f64) -> f64 {
    const LADDER: [f64; 11] = [0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0];
    LADDER
        .into_iter()
        .find(|step| step * pps >= 70.0)
        .unwrap_or(*LADDER.last().expect("the ladder is not empty"))
}

/// Everything the painter needs, already resolved. Built by [`Scene::of`] from live state and by a test
/// from whatever it wants on screen — the painter takes this and nothing else, so no rule can hide in a
/// draw callback.
#[derive(Debug, Clone)]
pub struct Scene {
    /// The page's pixels-per-second (`cut_view::pps`).
    pub pps: f64,
    /// Scroll into the timeline, px. The band is drawn translated by `-origin`, so a test at origin 0 sees
    /// the same picture the page does at its own scroll.
    pub origin: f64,
    /// The kept clips and inserts, from the window's own cut.
    pub segs: Vec<cut::Seg>,
    /// The effects, same source.
    pub fx: Vec<cut::Fx>,
    /// The takes, so filmed runs / amber edges / name plates all agree with the fold half of the page.
    pub recordings: Vec<timeline::Recording>,
    /// The fold list: a view, not an edit (§B).
    pub folds: Vec<[f64; 2]>,
    /// The selection band, if one is drawn.
    pub selection: Option<(f64, f64)>,
    /// The row the preview is watching, `None` when the cut itself is what the preview shows.
    pub watched_row: Option<i32>,
    /// Rows the page draws, so the gutter can mark an empty one.
    pub row_count: usize,
    /// The hand corrections (`cut.json`'s `shift`), which the name plates read out.
    pub shift: std::collections::BTreeMap<String, f64>,
    /// The cut lanes, whose `at` the plate spells "from m:ss".
    pub lanes: Vec<cut::Lane>,
    /// §05-cut F2.5 badge 2: each lane's name plate WITH its channel layout (`‹name› mono`), in
    /// `lanes`' order. Built by `crate::cut_lanes::plates`, which is where the rule lives; the
    /// painter only draws what is here, so a check can read the same text the band shows.
    pub lane_plates: Vec<String>,
    /// Thumbnails the band can show, keyed by the recording's base name. Decoded elsewhere (the page
    /// decodes once per row height and hands the paths in); an absent entry draws no pictures.
    pub thumbs: Vec<(String, Vec<std::path::PathBuf>)>,
    /// The red line's second (§1's badge 30). It lives here rather than in a second thread-local because the
    /// playhead's own layer must ask the SAME scene the band asks, or the two layers disagree about where
    /// the line is whenever the page scrolls or zooms.
    pub line: f64,
}

/// Where one second of the tape is DRAWN, in the band's own pixels: `x_of(t) - x_of(0)`.
///
/// This is the painter's single seconds→x door. It deliberately starts at the tape's own first pixel rather
/// than at the span's gutter and ignores scroll, exactly as every `draw_*` helper below already did when the
/// answer was plain `t * pps` — §A measures the gutter INSIDE the timeline (`cut_view::view_px` subtracts it
/// from a window width), so the painter's surface starts where the tape does and the caller owns the 30 px.
/// What it adds over that multiplication is the FOLD: a folded cell carries no width (§B), so a dropped
/// stretch that has been folded away draws nothing and its neighbours close up around the seam.
pub fn band_x(scene: &Scene, t: f64) -> f64 {
    let span = band_span(scene);
    // Past the last filmed second the seconds still run linearly: a selection or an in/out mark may reach
    // beyond the tape, and §B's `x_of` answers every one of them with `total_px`, which would pile all those
    // late marks on a single column. Extrapolate instead so a mark past the end stays where its own second is.
    let last = span
        .cells
        .iter()
        .filter(|cell| !cell.folded)
        .map(|cell| cell.end)
        .fold(0.0_f64, f64::max);
    span.x_of(t.min(last)) + (t - last).max(0.0) * scene.pps - span.x_of(0.0)
}

/// The band's laid-out span: the filmed runs cut at this scene's folds, at its zoom, after the gutter
/// (§B: "cells = runs cut at folded gaps", "spans carry pixel origins (gutter first; a folded cell has zero
/// width)"). Built from the scene each time it is asked, so a fold toggle repaints through it without any
/// second copy of the geometry to keep in step.
pub fn band_span(scene: &Scene) -> timeline::Span {
    timeline::Span::new(
        timeline::cells(&scene.runs(), &scene.folds),
        scene.pps,
        cut_screen::GUTTER_PX,
    )
}

/// The kept clips' DRAWN `(x, width)` pairs, index by index into `scene.segs`, through [`band_x`] — so the
/// number a test reads is the number the paint used, and a fold shows up as a bar that moved left rather
/// than only as a sentence in the status line. Not a second geometry: `draw_kept_bars` paints these boxes.
pub fn band_boxes(scene: &Scene) -> Vec<(f64, f64)> {
    scene
        .segs
        .iter()
        .map(|seg| {
            let shift = shift_of_camera(&scene.shift, seg.cam);
            band_span_px(scene, session_second(seg.s, shift), session_second(seg.e, shift))
        })
        .collect()
}

/// A clip's own second on the SESSION clock after its camera's hand correction — the same `session_at` the
/// boxes below use, with the file offset at 0 because a scene's `s`/`e` are already session seconds.
fn session_second(local: f64, shift: f64) -> f64 {
    // `session_at(session, session, …)` would double the second: this is the correction applied to a time
    // that is already on the session clock, i.e. `t - shift`.
    (local - shift).max(0.0)
}

/// Where a mark that spans `from..to` begins and how wide it draws, honouring a fold between the two.
pub fn band_span_px(scene: &Scene, from: f64, to: f64) -> (f64, f64) {
    let x0 = band_x(scene, from);
    let x1 = band_x(scene, to.max(from));
    (x0, (x1 - x0).max(0.0))
}

impl Scene {
    /// The painted seconds: everything filmed, so a dropped stretch still takes its place in the band.
    pub(crate) fn runs(&self) -> Vec<(f64, f64)> {
        timeline::filmed_runs(&self.recordings)
    }

    /// The band's tape length in seconds, which bounds the ruler.
    pub(crate) fn tape_end(&self) -> f64 {
        self.runs().last().map(|(_, end)| *end).unwrap_or(0.0)
    }
}

/// The page's own scene: every number the band paints, read from state the caller already holds.
#[allow(clippy::too_many_arguments)] // one field of Scene per argument; a struct would only rename them
pub fn scene_of(
    segs: &[cut::Seg],
    fx: &[cut::Fx],
    folds: &[[f64; 2]],
    recordings: &[timeline::Recording],
    selection: Option<(f64, f64)>,
    watched_row: Option<i32>,
    row_count: usize,
    shift: &std::collections::BTreeMap<String, f64>,
    lanes: &[cut::Lane],
    pps: f64,
    origin: f64,
    line: f64,
) -> Scene {
    Scene {
        pps,
        origin,
        segs: segs.to_vec(),
        fx: fx.to_vec(),
        recordings: recordings.to_vec(),
        folds: folds.to_vec(),
        selection,
        watched_row,
        row_count,
        shift: shift.clone(),
        lanes: lanes.to_vec(),
        lane_plates: crate::cut_lanes::plates(lanes),
        thumbs: Vec::new(),
        line,
    }
}

/// The page's own scene, gathered here rather than in `window.rs`, which is past its size budget: every
/// number comes from state the window already keeps, so the band cannot disagree with the page.
pub fn scene_from(
    cut_: &cut::Cut,
    recordings: &[timeline::Recording],
    selection: Option<(f64, f64)>,
    watched_row: Option<usize>,
    line: f64,
) -> Scene {
    scene_of(
        &cut_.segs,
        &cut_.fx,
        &cut_.folds,
        recordings,
        selection,
        watched_row.map(|row| row as i32),
        timeline::row_count(recordings, cut_),
        &cut_.shift,
        &cut_.lanes,
        crate::ui::cut_view::pps(),
        crate::ui::cut_view::origin(),
        line,
    )
}

/// Badge **31**: the bar itself, named what `repaint_scrollbar` already looks it up by.
pub fn scrollbar() -> gtk::Scrollbar {
    let bar = gtk::Scrollbar::new(
        gtk::Orientation::Horizontal,
        Some(&gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0)),
    );
    bar.set_widget_name("track-scrollbar");
    bar
}
/// Build the band, hang both layers on it and insert it plus its scrollbar after `after`, returning the
/// scrollbar so the caller's own `previous` cursor keeps walking. Everything the paint needs comes from
/// `scene`, called at draw time; `thumbs` is likewise asked at draw time, so a project opened after this
/// call still gets its frames. This lives here because `window.rs` is past its size budget.
pub fn install(
    window: &adw::ApplicationWindow,
    host: &gtk::Box,
    after: &gtk::Widget,
    scene: impl Fn() -> Scene + Clone + 'static,
    tree: Option<layout::Tree>,
) -> gtk::Scrollbar {
    // Moved into the draw closure, which must be `'static`: the frames are Prepare's output, listed per
    // recording when the page asks for its scene — a folder with none draws no pictures, which is §1's
    // answer rather than black.
    let thumb_for = move |base: &str| {
        tree.as_ref()
            .map(|tree| frame_paths(tree, base))
            .unwrap_or_default()
    };
    // The frames are Prepare's output, listed per recording when the page asks for its scene.
    let with_thumbs = move || {
        let mut held = scene();
        held.thumbs = held
            .recordings
            .iter()
            .map(|recording| (recording.base.clone(), thumb_for(&recording.base)))
            .collect();
        held
    };
    let band = build(
        band_height_px(&with_thumbs(), THUMB_DEFAULT_PX).ceil() as i32,
        // The MINIMUM, not the width: `build` makes the band expand, so this is only what it never shrinks
        // under (a window that has never been laid out, which is every headless test).
        BAND_WIDTH_PX,
    );
    attach(&band.surface, &band.playhead, &band.layers, with_thumbs, THUMB_DEFAULT_PX);
    // §1's band grows with every row an edit adds; the page's own redraw doors find these surfaces again
    // through [`band_of`], so nothing has to be remembered here.
    // §1's tape also WIDENS with the page: the surface's draw is clipped to its allocation, so the first frame
    // after the window is laid out has to be asked for at the width GTK actually gave it, or the ruler stops
    // at 480 px of an otherwise full-width plate until the next edit repaints it.
    let widen = band.layers.clone();
    let redraw = band.surface.clone();
    widen.connect_map(move |_| {
        redraw.queue_draw();
    });
    let resize = band.layers.clone();
    let repaint = band.surface.clone();
    resize.connect_realize(move |_| {
        repaint.queue_draw();
    });
    // F2.12 S6: the hold lives on the band, so it goes on wherever a band is installed — not on the caller,
    // which is `window.rs`, already past its size budget. The band is found by name through [`band_of`], and
    // at THIS point in `install` the freshly built one is not yet in the window's tree (`host.insert_child_after`
    // below is what puts it there), so `band_of` answers an OLDER page's band or nothing at all — which is how
    // S6's wire test came to find no gesture on a second window in the same process. The surface `build` just
    // made is handed over directly: it is the only band this call can be talking about.
    crate::ui::cut_card_hold::wire(window, &band.surface);
    host.insert_child_after(&band.layers, Some(after));
    let bar = scrollbar();
    let stacked: gtk::Widget = band.layers.upcast();
    host.insert_child_after(&bar, Some(&stacked));
    bar
}

/// The band's MINIMUM width: the placeholder strip's, so a page that has never been laid out (every headless
/// test, a window built but not presented) still measures a real tape rather than nothing. §1's band is not
/// this wide — it spans the PAGE (the paned §A describes, and `spec/img/05-cut.png`), which is what the
/// `hexpand` + `halign(FILL)` in [`build`] do. This is only the floor it never shrinks under.
pub const BAND_WIDTH_PX: i32 = 480;

/// The height the band's own scene needs, in px. `install` hands this to `build` once, before the page has
/// any cut, and every later row comes from [`fit_band_height`].
pub fn band_height_for(scene: &Scene) -> i32 {
    band_height_px(scene, THUMB_DEFAULT_PX).ceil() as i32
}

/// Hang both layers on their drawing areas; everything they paint comes from `scene`, called at draw time,
/// so neither callback holds state of its own. Both return `Proceed`: the band draws and does not consume
/// the event, so the page's existing button and gesture handling keeps working as it did on the strip.
///
/// §1's band is not a fixed height — it costs a row per camera AND a row per cut lane — so the overlay the
/// two surfaces live in has to be reachable when the picture is asked for: that is where the size gets
/// settled, at the moment the page asks for this picture (the same door [`fit_band_height_for_window`] opens
/// for the stack a page is showing), so a redraw that only queues a frame still lands on the right height.
pub fn attach(
    surface: &gtk::DrawingArea,
    playhead: &gtk::DrawingArea,
    layers: &gtk::Overlay,
    scene: impl Fn() -> Scene + Clone + 'static,
    thumb_px: u32,
) {
    let for_band = scene.clone();
    // Cloned in: a draw callback is `'static` and so cannot borrow the caller's widgets.
    let band_area = surface.clone();
    let overlay = layers.clone();
    surface.set_draw_func(move |_, cr, width, height| {
        let held = for_band();
        fit_band_height(&overlay, &band_area, &held);
        paint_at(cr, &held, width.max(1) as f64, height.max(1) as f64, thumb_px);
    });
    let for_head = scene.clone();
    playhead.set_draw_func(move |_, cr, _width, height| {
        let held = for_head();
        paint_playhead(cr, held.pps, held.origin, held.line, height.max(1) as f64);
    });
}

/// The named surfaces of the band, stacked: §1 puts the tracks under the toolbar and the red line over both,
/// so they are two `DrawingArea`s inside one overlay — the band, which repaints on every edit, and the
/// playhead, which repaints on every tick. Built here because `window.rs` is past its size budget.
pub struct Band {
    pub surface: gtk::DrawingArea,
    pub playhead: gtk::DrawingArea,
    pub layers: gtk::Overlay,
}

/// Build the two surfaces with the names §1's inventory uses. Height is the band's own arithmetic; the
/// overlay matches it so the red line really crosses both bands.
pub fn build(height: i32, width: i32) -> Band {
    let surface = gtk::DrawingArea::new();
    surface.set_widget_name("track-band");
    // §1's ruler, bars, wave strips and plates run the PAGE's width, not a 480 px stub: the band expands into
    // whatever the paned gives it and fills it. `FILL` rather than `START` is what makes the expansion real — a
    // `START`-aligned area keeps its minimum request, so GTK would never widen it and every draw below would
    // keep being handed 480 px.
    surface.set_size_request(width, height);
    surface.set_hexpand(true);
    surface.set_halign(gtk::Align::Fill);
    // The band a test fires the hold at is found again through the window (`band_of`), so nothing is remembered here.
    let playhead = gtk::DrawingArea::new();
    playhead.set_widget_name("track-playhead");
    playhead.set_size_request(width, height);
    playhead.set_hexpand(true);
    playhead.set_halign(gtk::Align::Fill);
    // The overlay draws a line over what the band drew; it must not swallow the presses the band owns.
    playhead.set_can_target(false);
    let layers = gtk::Overlay::new();
    layers.set_widget_name("track-layers");
    layers.set_hexpand(true);
    layers.set_halign(gtk::Align::Fill);
    // A NAMED holder for the per-gap fold badges (`fold-button-<i>`), which §B puts inside the selection
    // band, over the bars their x was computed from — so they are overlay children of this same stack and
    // share its pixels with the paint. The box is empty until `ui::cut_folds::refresh_fold_badges` fills it.
    let badges = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    badges.set_widget_name("fold-badges");
    // The badge row must never eat a press meant for the band beneath it: only the buttons inside target.
    badges.set_can_target(false);
    badges.set_halign(gtk::Align::Start);
    badges.set_valign(gtk::Align::Start);
    layers.set_child(Some(&surface));
    layers.add_overlay(&playhead);
    layers.add_overlay(&badges);
    Band {
        surface,
        playhead,
        layers,
    }
}

/// The band this page shows: the two surfaces §1's stack is made of, found again through the window that
/// installed them, so a caller holding only the window can grow them. Looked up by the same widget names
/// [`build`] gave them, which is also how every other Cut-page refresh finds its widgets — no remembered copy
/// to go stale.
pub fn band_of(window: &adw::ApplicationWindow) -> Option<(gtk::DrawingArea, gtk::Overlay)> {
    let content = window.content()?;
    let layers = crate::ui::window::find_widget_by_name(content.upcast_ref(), "track-layers")?
        .downcast::<gtk::Overlay>()
        .ok()?;
    let band = crate::ui::window::find_widget_by_name(layers.upcast_ref(), "track-band")?
        .downcast::<gtk::DrawingArea>()
        .ok()?;
    Some((band, layers))
}

/// Grow THIS window's band to the scene it is about to paint — the door the page's redraw calls
/// (`cut_view::repaint_page`, `refresh_page_scrollbar`, `window::refresh_effects_lane`).
///
/// §05-cut#1-screen: §1 costs the band a row per camera AND a row per cut lane, but [`install`] could only
/// size it for the page as it opened, and a `DrawingArea` clips its own draw to its allocation — so a row an
/// edit added was painted below what the page had made room for.
pub fn fit_band_height_for_window(window: &adw::ApplicationWindow, scene: &Scene) {
    if let Some((band, layers)) = band_of(window) {
        fit_band_height(&layers, &band, scene);
    }
}

/// How tall the band must be for the scene it is about to paint.
pub fn band_height_px(scene: &Scene, thumb_px: u32) -> f64 {
    rows(scene, thumb_px).height(scene)
}

/// Grow the band's surfaces to the scene they are about to paint: §1's band costs a row per camera AND per
/// cut lane, `install` sized it once before the page had any cut, and a `DrawingArea` clips its draw callback
/// to its own allocation — so an added row is otherwise painted outside what the page allocated, however
/// correct its ink was. The check is against the surface's SIZE REQUEST, which is what the next layout will
/// honour: the measured height only catches up after that layout, so comparing it makes a grow that landed
/// look like one that did not. The red line above the band is grown to the same height too — §1 puts it "across
/// both bands", and a short one would stop above the row an edit added.
pub fn fit_band_height(layers: &gtk::Overlay, band: &gtk::DrawingArea, scene: &Scene) {
    let tall = band_height_for(scene);
    if band.size_request().1 == tall {
        return;
    }
    // Height only: the width is the layout's decision, made by the `hexpand` in [`build`]. Writing a width
    // request here (as this did) pins the band back to its minimum and undoes the span §1 asks for. A size
    // request of -1 means "no opinion", which is exactly what a width GTK is already choosing means.
    band.set_size_request(-1, tall);
    layers.set_size_request(-1, tall);
    // Every other drawing area in the stack — the red line's layer — follows the band's height.
    let held = layers.observe_children();
    for index in 0..held.n_items() {
        let Some(child) = held.item(index) else { continue };
        let Ok(area) = child.downcast::<gtk::DrawingArea>() else { continue };
        if area != *band {
            area.set_size_request(-1, tall);
        }
    }
}

impl Rows {
    /// Ruler + selection band + effects lane + one picture row per recorded row — the same four sums the
    /// painter lays out, so a row added by an edit grows the surface instead of being cut off.
    fn height(&self, scene: &Scene) -> f64 {
        // Camera rows, then one row per cut lane: a lane is a ROW of the band (§F2.12 S5 "a row of its own"),
        // so it costs the band height exactly as a camera does. `lane_rows` is the same count, public for tests.
        self.pictures + (scene.row_count.max(1) + lane_rows(scene)) as f64 * self.row_h
    }

    /// Where a cut lane's own row begins: below every camera row, in lane order. `camera_rows` is how many
    /// rows the cameras take, so a lane never lands on top of a picture row however many there are.
    pub fn lane_top(&self, scene: &Scene, lane: usize) -> f64 {
        self.pictures + self.camera_rows(scene) as f64 * self.row_h + lane as f64 * self.row_h
    }

    /// Rows the cameras take — the same `max(1)` the height uses, in one place so the two cannot drift.
    pub fn camera_rows(&self, scene: &Scene) -> usize {
        scene.row_count.max(1)
    }
}

/// Rows the band gives the cut lanes — one each, below the cameras. Public so a test reads the SAME number the
/// painter laid out rather than counting `scene.lanes` itself and drifting from it. A spliced or overwriting
/// card takes no row of its own (they sit among the footage), so this is also how many rows an insert costs.
pub fn lane_rows(scene: &Scene) -> usize {
    scene.lanes.len()
}

/// Each cut lane as its own row below the cameras (§F2.12 S5: LANE "adds a row", and until now nothing drew
/// one, leaving the status line as the only proof a lane existed). The row gets a plate-coloured ground, the
/// file's name, and a violet span where the lane actually runs, so `at`/`dur` are visible rather than implied.
/// The PRESS surfaces (`bands_for`) deliberately stop at the camera rows: F2.9/F2.10 own what a click on a row
/// means, and a lane row nobody can press is still a row someone can see.
fn draw_lane_rows(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for (index, lane) in scene.lanes.iter().enumerate() {
        let top = rows.lane_top(scene, index);
        fill(cr, LANE_GROUND, 0.0, top, tape, rows.row_h, 1.0);
        ink(cr, LANE_BORDER);
        cr.rectangle(0.0, top, tape, 1.0);
        let _ = cr.fill();
        let (x0, width) = band_span_px(scene, lane.at, lane.at + lane.dur);
        if width > 0.0 {
            fill(cr, INSERT_VIOLET, x0, top + 2.0, width, rows.row_h - 4.0, 0.75);
        }
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(9.0);
        ink(cr, (0.15, 0.15, 0.2));
        // Badge 2's plate: the name WITH its channel layout, from `Scene::lane_plates`.
        let plate = scene
            .lane_plates
            .get(index)
            .map(String::as_str)
            .unwrap_or(lane.name.as_str());
        text_at(cr, 2.0, top + 10.0, plate);
    }
}

/// Paint the whole band into `cr`, `width` x `height` px at the open-at thumbnail height. Pure: it reads
/// `scene` and draws; nothing in here decides a number the model did not already have.
pub fn paint(cr: &cairo::Context, scene: &Scene, width: f64, height: f64) {
    paint_at(cr, scene, width, height, THUMB_DEFAULT_PX);
}

/// The same with the page's real thumbnail height, which the form column's ladder sets.
pub fn paint_at(cr: &cairo::Context, scene: &Scene, width: f64, height: f64, thumb_px: u32) {
    fill(cr, GROUND, 0.0, 0.0, width, height, 1.0);
    let rows = rows(scene, thumb_px);
    // The tape's painted width: the surface's own, so the ruler's ground, the wave strips and the lane rows
    // reach the page's right edge instead of stopping at a constant, and a clip that runs past it is cut there.
    let tape = width.max(BAND_WIDTH_PX as f64);
    draw_ruler(cr, scene, tape);
    draw_dropped_gaps(cr, scene, rows, tape);
    draw_kept_bars(cr, scene, rows, tape);
    draw_effects_lane(cr, scene, rows, tape);
    draw_picture_rows(cr, scene, rows, tape);
    draw_row_wave_strips(cr, scene, rows, tape);
    draw_recording_edges(cr, scene, rows, tape);
    draw_join_washes(cr, scene, rows, tape);
    draw_watched_row(cr, scene, rows, tape);
    draw_selection(cr, scene, rows, tape);
    draw_insert_bands(cr, scene, rows, tape);
    draw_in_out_marks(cr, scene, rows, tape);
    draw_laid_pieces(cr, scene, rows, tape);
    draw_lane_rows(cr, scene, rows, tape);
    draw_name_plates(cr, scene, rows);
}

/// F2.9 S2: the pieces a sound paste laid over the kept footage, on the PAINTED band as well as on the
/// window's placeholder strip. Same painter ([`crate::ui::cut_trim_press::paint_laid_pieces_in`]), this
/// caller's row geometry, drawn last but for the name plates so the plate sits on the footage it was laid
/// over. Without this the lay is invisible on the surface the Cut page actually shows.
fn draw_laid_pieces(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    crate::ui::cut_trim_press::paint_laid_pieces_in(
        cr,
        tape,
        &cut::Cut {
            segs: scene.segs.clone(),
            ..Default::default()
        },
        scene.pps,
        rows.pictures,
        rows.row_h,
    );
}

/// The placeholder's open-at thumbnail height, used wherever a caller has not said otherwise.
pub const THUMB_DEFAULT_PX: u32 = cut_screen::THUMB_AT_OPEN;

/// The band's layout: the row tops every painter shares. Kept in one place because the effects lane's height
/// feeds the picture rows' start, and a second arithmetic would be a second answer.
#[derive(Debug, Clone, Copy)]
pub struct Rows {
    pub selection: f64,
    pub effects: f64,
    pub pictures: f64,
    pub row_h: f64,
}

impl Rows {
    /// Where one row's wave strip begins: the lower [`ROW_WAVE_PX`] of its picture row. Every painter and
    /// every press reader asks THIS rather than doing its own arithmetic, or the ink and the reach disagree.
    pub fn wave_top(&self, row: usize) -> f64 {
        self.pictures + row as f64 * self.row_h + (self.row_h - ROW_WAVE_PX)
    }
}

/// The same `rows` arithmetic the painter lays out, as the surfaces a press can land on (§A's order:
/// ruler, selection band, effects lane, then one picture row with its wave strip per camera row). Built
/// from the fx row count and the row count rather than a whole `Scene`, so a press handler can ask it
/// without building what it is only going to measure — and cannot disagree with what is drawn.
pub fn bands_for(fx_rows: usize) -> crate::cut_select::SurfaceBands {
    let held = rows_of(fx_rows, THUMB_DEFAULT_PX);
    // One camera row is a picture band PLUS its own wave strip, exactly as §27 Tracks lays it out and as
    // `SurfaceBands` was designed: the row's PITCH (`held.row_h`) is unchanged, split into the picture half
    // above and the wave strip below ([`ROW_WAVE_PX`]), so `surface_at` bounds the rows by the same bottom
    // it always did and every press y that meant a row still means that row. A wave-height of 0 here would
    // make `Surface::WaveStrip` unreachable on the painted band, and a sound scope could then only be aimed
    // at the placeholder strip — which is what left F2.9's footage/sound fork unaimable by hand.
    crate::cut_select::SurfaceBands {
        ruler: (0.0, cut_trim::RULER_H),
        selection: (cut_trim::RULER_H, held.effects),
        fx: (held.effects, held.pictures),
        first_picture: held.pictures,
        row_height: (held.row_h - ROW_WAVE_PX).max(0.0),
        wave_height: ROW_WAVE_PX.min(held.row_h),
    }
}

fn rows(scene: &Scene, thumb_px: u32) -> Rows {
    rows_of(fx_lane::row_count(&scene.fx), thumb_px)
}

fn rows_of(fx_rows: usize, thumb_px: u32) -> Rows {
    let selection = cut_trim::RULER_H;
    let effects = selection + SELECTION_BAND_PX;
    let pictures = effects + fx_lane::lane_height_px(fx_rows);
    Rows {
        selection,
        effects,
        pictures,
        row_h: thumb_px as f64 + PICTURE_ROW_EXTRA_PX,
    }
}

/// The ruler and its labels, at §A's ladder, only what the tape shows: the ticks run to the surface's own
/// width, so the ruler reaches the page's right edge and stops there rather than being inked off-surface.
fn draw_ruler(cr: &cairo::Context, scene: &Scene, tape: f64) {
    let step = tick_step_seconds(scene.pps);
    let marks: Vec<(f64, String)> = cut_trim::ruler_ticks(scene.tape_end(), scene.pps, step)
        .into_iter()
        .filter(|(x, _)| *x >= 0.0 && *x <= tape)
        .collect();
    ink(cr, RULER_INK);
    for (x, _) in &marks {
        cr.rectangle(*x, 0.0, 1.0, cut_trim::RULER_H);
        let _ = cr.fill();
    }
    labels(cr, &marks);
}

/// The tick labels, at the size the ruler's height allows.
fn labels(cr: &cairo::Context, ticks: &[(f64, String)]) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(9.0);
    ink(cr, RULER_INK);
    for (x, label) in ticks {
        text_at(cr, x + 2.0, cut_trim::RULER_H - 4.0, label);
    }
}

/// The dropped stretches inside filmed material, then their fold badges (§A puts the −/+ badges in the
/// selection band, where the gap's badge is drawn after the gap itself).
fn draw_dropped_gaps(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for (from, to) in cut_fold::dropped_gaps(&scene.runs(), &scene.segs) {
        // A folded gap has no pixels of its own (§B), so nothing is painted for it and no mid-gap ink either
        // — the seam is what its + badge sits on. Asked BEFORE any fill so a fold draws nothing at all.
        let (x0, width) = band_span_px(scene, from, to);
        if width <= 0.0 || x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(
            cr,
            (0.88, 0.88, 0.88),
            x0,
            rows.selection,
            width,
            SELECTION_BAND_PX,
            1.0,
        );
        // The badge: `cut_fold::badges` decides WHICH gap wears one and where its x lands on a laid-out
        // span; here the mid-gap mark is that same middle, drawn as ink so a test can see it.
        let x = band_x(scene, (from + to) / 2.0);
        ink(cr, (0.35, 0.35, 0.4));
        cr.rectangle(x - 1.0, rows.selection + 4.0, 3.0, SELECTION_BAND_PX - 8.0);
        let _ = cr.fill();
    }
}

/// Green bar per kept scene (§1's badge 25), the placeholder's colour and borders so F2.8's grips stay
/// where its tests press them. The boxes come from [`band_boxes`], i.e. through the folded span, so a fold
/// elsewhere in the run slides every later bar left instead of leaving the row untouched.
fn draw_kept_bars(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let drawn = band_boxes(scene);
    // Kept scenes are CLIPPED to the tape, not skipped when they overhang it: a kept scene's bar has to reach
    // the page's right edge (that is what a scrolled band looks like), and a box drawn past the surface is cut
    // by cairo anyway — but a bar wholly off the left edge would leave its right border inked at x < 0, so
    // the visible test runs before the box is handed to the border rule.
    let boxes: Vec<cut_trim::Box_> = clip_boxes_shifted(&scene.segs, &scene.shift, scene.pps)
        .into_iter()
        .map(|b| cut_trim::Box_ {
            x: drawn[b.index].0,
            w: drawn[b.index].1,
            index: b.index,
            insert: b.insert,
        })
        .filter(|b| b.x < tape && b.x + b.w > 0.0)
        .collect();
    for b in &boxes {
        if b.insert {
            continue;
        }
        fill(cr, KEPT_TINT, b.x, rows.selection, b.w.max(1.0), SELECTION_BAND_PX, 1.0);
    }
    ink(cr, KEPT_BORDER);
    for (x, _, _) in cut_trim::border_positions(&boxes) {
        cr.rectangle(x - 0.5, rows.selection, 1.5, SELECTION_BAND_PX);
        let _ = cr.fill();
    }
}


/// The effects lane: one row per overlapping group, each bar in its kind's colour.
fn draw_effects_lane(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for bar in fx_lane::lane_layout(&scene.fx) {
        let (from, to) = scene.fx[bar.index].spans();
        let y = rows.effects + bar.row as f64 * fx_lane::lane_height_px(1);
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(
            cr,
            bar.colour,
            x0,
            y,
            width.max(0.0),
            fx_lane::lane_height_px(1),
            0.6,
        );
    }
}

/// The picture rows: the filmed ground of each recorded row, then one thumbnail per row rather than one per
/// frame. §1 says the band has a picture at every zoom, and the prototype's gap between frames is its own
/// frame spacing, not something the band should reproduce as black; one representative frame per row is what
/// a shot can be compared against. Row 0's frames are the ones shown, which is the row the preview watches.
fn draw_picture_rows(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for run in scene.runs() {
        let (x0, width) = band_span_px(scene, run.0, run.1);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(
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
    // §1: "the band has a picture at every zoom". One frame is shown, the one nearest the second at the
    // view's left edge — `frame_near` picks it by the file name's own timestamp, and the mark sits where
    // that second lands. Frames are not decoded here: a draw must never block on a decode (§7), and the
    // window of names a pass asks about is ONE §D batch wide ([`batch_of`], `thumbBatch 6`), the loader's
    // bound rather than the row's whole list.
    let at = (scene.origin / scene.pps).max(0.0);
    let step = frame_step_seconds(paths.len());
    let first = if step > 0.0 {
        (at / step).floor() as usize
    } else {
        0
    };
    let Some(near) = frame_near(batch_of(paths, first), at) else {
        return;
    };
    fill(
        cr,
        (0.65, 0.68, 0.72),
        path_x(scene, &near),
        rows.pictures,
        2.0,
        rows.row_h,
        1.0,
    );
}

/// The seconds between two entries of a row's frame list: Prepare extracts one frame every 250 ms, which is
/// what a `.jpg` name's stamp is spaced by and the only thing a frame's place in the list can be measured
/// against. §10 lists this in its frame-extraction sentence (`intervalSeconds`, 0.25 at the default) without a
/// `P.` id of its own for the reader's copy, so the band states it once here and says so.
const FRAME_STEP_SECONDS: f64 = 0.25;

/// One entry of a row's frame list stands for this many seconds of tape; a list of one or none stands for the
/// whole row, which no window of the list can express, so it answers 0 and the caller takes the first batch.
fn frame_step_seconds(count: usize) -> f64 {
    if count < 2 {
        return 0.0;
    }
    FRAME_STEP_SECONDS
}

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

/// §27 Tracks: "each row's first audio track as a strip below it". One plate per recorded row, in the
/// lower third of that row ([`Rows::wave_top`]), inked from `cache/waves/<base>.wave` through the same
/// painter the placeholder strip uses (`cut_trim_press::paint_row_wave`) so the two surfaces cannot show
/// different answers for one recording. This is also what makes the wave strip a PRESS target: the band
/// reserves the space in [`bands_for`] and this inks it, so a y down there reads `Surface::WaveStrip` over
/// ink rather than over an empty grey row.
fn draw_row_wave_strips(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let w = tape;
    for (index, recording) in scene.recordings.iter().enumerate() {
        crate::ui::cut_trim_press::paint_row_wave(
            cr,
            w,
            recording,
            scene.pps,
            rows.wave_top(index),
            rows.pictures + (index + 1) as f64 * rows.row_h,
        );
    }
}

/// A thumbnail's x, on the folded span: frames are named by their second, so the file name is the position.
/// An unparsable name draws at the row's start rather than nowhere.
fn path_x(scene: &Scene, path: &std::path::Path) -> f64 {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.get(11..19))
        .and_then(|hhmmss| {
            let h: f64 = hhmmss.get(0..2)?.parse().ok()?;
            let m: f64 = hhmmss.get(3..5)?.parse().ok()?;
            let s: f64 = hhmmss.get(6..9)?.parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        })
        .map(|seconds| band_x(scene, seconds))
        .unwrap_or(0.0)
}

/// Amber striped edges where a recording starts and ends (§1).
fn draw_recording_edges(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let height = rows.row_h * scene.row_count.max(1) as f64;
    for run in scene.runs() {
        // Both edges through the folded span: a fold inside a run moves its far edge with everything else.
        for x in [band_x(scene, run.0), (band_x(scene, run.1) - 3.0).max(0.0)] {
            if x < 0.0 || x > tape {
                continue;
            }
            for stripe in 0..3i32 {
                fill(cr, EDGE_AMBER, x + stripe as f64, rows.pictures, 1.0, height, 1.0);
            }
        }
    }
}

/// The two washes §1 names over the picture. They are WASHES, so alpha rather than a solid block — but each
/// is asked for only once, and by its own rule: the yellow marks stretches the join pass removed whole
/// (`cut_play_leg::dimmed`, the dropped gaps), and the rose lies under a SPEED effect and nothing else.
/// Asking the dropped-gap list for the yellow twice painted it a second time in the same place, and a second
/// 0.5 alpha over footage ground is what made one hole read as a bright band across the row.
fn draw_join_washes(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
    for (from, to) in crate::cut_play_leg::dimmed(
        &cut::Cut { segs: scene.segs.clone(), ..Default::default() },
        &scene.runs(),
    ) {
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(
            cr,
            JOIN_WASH,
            x0,
            rows.pictures,
            width.max(1.0),
            height,
            0.5,
        );
    }
    let rose = fx_lane::bar_colour(crate::cut::EffectKind::Speed, false);
    for effect in &scene.fx {
        if effect.effect_kind() != Some(crate::cut::EffectKind::Speed) {
            continue;
        }
        let (from, to) = effect.spans();
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(
            cr,
            rose,
            x0,
            rows.pictures,
            width,
            height,
            0.35,
        );
    }
}

/// The dashed white outline around the row a click set the preview to watch, gone when ▶ hands it back.
fn draw_watched_row(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some(row) = scene.watched_row else { return };
    let y = rows.pictures + row.max(0) as f64 * rows.row_h;
    // Drawn as its own rects rather than a stroked rectangle: a stroke straddles the path, so half the line
    // falls OUTSIDE the band when the outline is the row at the bottom, which is the common one-row case.
    ink(cr, WATCH_OUTLINE);
    // The dashes run the TAPE's width, cut at its right end: the row a click set the preview to watch is
    // outlined wherever there is tape to outline, not across empty ground past the last filmed second.
    let span = (scene.tape_end() * scene.pps).min(tape).max(0.0) as i64;
    for step in (0..span).step_by(10) {
        if step % 20 < 12 {
            cr.rectangle(step as f64, y, 10.0, 2.0);
            cr.rectangle(step as f64, y + rows.row_h - 2.0, 10.0, 2.0);
        }
    }
    let _ = cr.fill();
}

/// The selection: blue with its ends (§1's badge 27). The ✕ is a button in the page, not ink here.
fn draw_selection(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some((from, to)) = scene.selection else { return };
    let (x0, width) = band_span_px(scene, from, to);
    if x0 >= tape || x0 + width < 0.0 {
        return;
    }
    fill(
        cr,
        SELECTION_BLUE,
        x0,
        rows.selection,
        width.max(1.0),
        SELECTION_BAND_PX,
        0.45,
    );
    ink(cr, SELECTION_BLUE);
    for x in [x0, band_x(scene, to)] {
        cr.rectangle(x - 1.0, rows.selection, 2.5, SELECTION_BAND_PX);
        let _ = cr.fill();
    }
}

/// The kept scenes' boxes AT THE HAND CORRECTIONS: a recording that was slid by hand draws its clips
/// slid too, which is the whole point of §B's `shift` — a coordinate nobody applies is not a correction.
/// Each scene moves by its own camera's shift through [`timeline::session_at`] (§B's `sessionAt`, with the
/// file offset at 0 because these are session seconds already), then goes through the same box arithmetic
/// the press geometry uses, so paint and grab never disagree.
pub fn clip_boxes_shifted(
    segs: &[cut::Seg],
    shift: &std::collections::BTreeMap<String, f64>,
    pps: f64,
) -> Vec<cut_trim::Box_> {
    segs.iter()
        .enumerate()
        .map(|(index, seg)| {
            // A scene belongs to the recording of its own camera row; no entry means nothing was slid.
            let moved = timeline::session_at(seg.s, 0.0, 0.0, shift_of_camera(shift, seg.cam))
                .max(0.0)
                * pps;
            cut_trim::Box_ {
                x: moved,
                w: ((seg.e - seg.s).max(0.0)) * pps,
                index,
                insert: seg.is_insert(),
            }
        })
        .collect()
}

/// The shift of the recording a scene sits on: rows are named `cam<N>` exactly as
/// [`timeline::kept_footage_recordings`] names them, so a camera number finds its source without a second map.
fn shift_of_camera(shift: &std::collections::BTreeMap<String, f64>, cam: i32) -> f64 {
    shift.get(&format!("cam{cam}")).copied().unwrap_or(0.0)
}

/// Every insert a violet band across every row, hatched when spliced, with a plated mark naming the file.
fn draw_insert_bands(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
    // Inserts ride the same folded geometry as the footage bars, so a card after a fold moves with them.
    let drawn = band_boxes(scene);
    let boxes: Vec<cut_trim::Box_> = clip_boxes_shifted(&scene.segs, &scene.shift, scene.pps)
        .into_iter()
        .map(|b| cut_trim::Box_ {
            x: drawn[b.index].0,
            w: if scene.segs[b.index].is_overwrite_insert() {
                // An over card runs for the footage it replaced, so it carries no `dur` and `band_boxes`
                // measured a zero-wide bar for it. Its own span is what was eaten — that is the box to paint.
                ((scene.segs[b.index].e - scene.segs[b.index].s) * scene.pps).max(2.0)
            } else {
                drawn[b.index].1
            },
            index: b.index,
            insert: b.insert || !scene.segs[b.index].ins.is_empty(),
        })
        .collect();
    for b in boxes.iter().filter(|b| b.insert) {
        if b.x >= tape || b.x + b.w < 0.0 {
            continue;
        }
        // An OVERWRITE card replaced these seconds outright, so the footage under it is GONE (§F2.12 S5
        // "replaces exactly those seconds", the same as － Remove). Paint that first — the dropped-gap grey,
        // full row height — or the violet band would sit on footage that is no longer in the cut and the two
        // modes would be indistinguishable on the surface the page actually shows.
        if scene.segs[b.index].is_overwrite_insert() {
            fill(cr, OVER_EATEN, b.x, rows.pictures, b.w.max(2.0), height, 1.0);
        }
        fill(cr, INSERT_VIOLET, b.x, rows.pictures, b.w.max(2.0), height, 0.6);
        if scene.segs[b.index].s == scene.segs[b.index].e {
            // Spliced: four slants say so without needing a colour §1 did not list.
            ink(cr, (0.35, 0.2, 0.5));
            for step in 0..4 {
                cr.rectangle(b.x + step as f64 * 4.0, rows.pictures, 1.5, height);
                let _ = cr.fill();
            }
        }
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(8.0);
        ink(cr, (0.2, 0.1, 0.3));
        let name = scene.segs[b.index]
            .ins
            .rsplit('/')
            .next()
            .unwrap_or("card")
            .to_string();
        text_at(cr, b.x + 2.0, rows.pictures + 9.0, &name);
    }
}

/// The in/out marks: 3 px lines with flag triangles across the whole band, green in, red out. The marks
/// ARE the selection's ends, so the flags and the readout cannot disagree (`cut_select::marks`).
fn draw_in_out_marks(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some((from, to)) = scene.selection else { return };
    let height = rows.pictures + rows.row_h;
    // No clamp to the tape: `band_x` runs linearly past the last filmed second, so a selection that reaches
    // beyond the recording still shows both of its marks at their own seconds.
    for (x, colour) in [
        (band_x(scene, from), MARK_IN_GREEN),
        (band_x(scene, to), MARK_OUT_RED),
    ] {
        if x < 0.0 || x > tape {
            continue;
        }
        fill(cr, colour, x, 0.0, MARK_LINE_PX, height, 1.0);
        cr.set_source_rgb(colour.0, colour.1, colour.2);
        cr.move_to(x, 0.0);
        cr.line_to(x + FLAG_PX, 4.0);
        cr.line_to(x, 8.0);
        cr.close_path();
        let _ = cr.fill();
    }
}

/// Pinned name plates: the recording's base name, `from m:ss` for a cut lane windowing a file, and
/// `+1.23 s` when the source carries a hand correction.
fn draw_name_plates(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(9.0);
    ink(cr, (0.15, 0.15, 0.2));
    for (index, recording) in scene.recordings.iter().enumerate() {
        let y = rows.pictures + index as f64 * rows.row_h + 10.0;
        for word in plates(scene, recording) {
            text_at(cr, 2.0, y, &word);
        }
    }
}

/// Ink a string at a point; cairo returns a result nobody can act on from a paint pass.
fn text_at(cr: &cairo::Context, x: f64, y: f64, text: &str) {
    cr.move_to(x, y);
    let _ = cr.show_text(text);
}

/// What a row's plate says: the base name, `from m:ss` when a cut lane windows the file, and `+1.23 s` when
/// the source carries a hand correction (§1's badge 28). Two lines, so a narrow row still reads.
pub fn plates(scene: &Scene, recording: &timeline::Recording) -> Vec<String> {
    let mut out = vec![recording.base.clone()];
    if let Some(lane) = scene.lanes.iter().find(|lane| lane.name == recording.base) {
        out.push(format!("from {}", tools::mm_ss(lane.at)));
    }
    if let Some(shift) = scene.shift.get(&recording.base) {
        out.push(format!("{:+.2} s", shift));
    }
    out
}

/// The red line, on its OWN surface: 2 px across the whole height, at the second the preview says.
pub fn paint_playhead(cr: &cairo::Context, pps: f64, origin: f64, t: f64, height: f64) {
    let x = t * pps - origin;
    cr.set_source_rgb(PLAYHEAD_RGB.0, PLAYHEAD_RGB.1, PLAYHEAD_RGB.2);
    cr.rectangle(x, 0.0, PLAYHEAD_PX, height);
    let _ = cr.fill();
}

/// The frames the band may show for one recording: Prepare's extracted JPEGs on the grid, oldest first.
/// Listed once per install (§1's Pictures) and cached in the scene, so a draw never touches the disk; a
/// missing folder answers empty, which draws no pictures rather than inventing black (§1's prototype note).
pub fn frame_paths(tree: &layout::Tree, base: &str) -> Vec<std::path::PathBuf> {
    let dir = tree.frames_dir(base);
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".jpg"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Which extracted frame is nearest a second: the last one at or before it, falling back to the first
/// when the view starts ahead of every frame. `clock::frame_name` stamps the files, so comparing the
/// formatted stamp against a floor is what makes "nearest" mean "not from the future".
pub fn frame_near(paths: &[std::path::PathBuf], at: f64) -> Option<std::path::PathBuf> {
    let want = clock::frame_name(at.floor());
    paths
        .iter()
        // The names sort as strings and carry the stamp, so "at or before" is a byte compare against the
        // wanted stamp — which is what keeps a shown frame from being one that has not happened yet.
        .rfind(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.as_bytes() <= want.as_bytes())
        })
        .or_else(|| paths.first())
        .cloned()
}

fn fill(cr: &cairo::Context, rgb: (f64, f64, f64), x: f64, y: f64, w: f64, h: f64, alpha: f64) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    cr.set_source_rgba(rgb.0, rgb.1, rgb.2, alpha);
    cr.rectangle(x, y, w, h);
    let _ = cr.fill();
}

fn ink(cr: &cairo::Context, rgb: (f64, f64, f64)) {
    cr.set_source_rgb(rgb.0, rgb.1, rgb.2);
}
