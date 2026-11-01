//! §05-cut.md §1 "Tracks": the band's drawing, from state alone. None of it is a rule: every x comes
//! from `cut_trim::clip_boxes`, every gap from `cut_fold::dropped_gaps`, every lane row from
//! `fx_lane::lane_layout`, so it paints into a memory surface and is measured a pixel at a time (§00-principles §5).
//! The red playhead has its own surface and paint ([`paint_playhead`]) so a tick does not repaint the band.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::cut_picture_rows as picture_rows;
pub use crate::ui::cut_picture_rows::{allow_frame_decoding, first_second, THUMB_BATCH};
use crate::{clock, cut, cut_screen, cut_trim, fx_lane, layout, timeline, tools};

use crate::ui::cut_colours::{
    BAND_GROUND, EDGE_AMBER, GROUND, GUTTER_EDGE, GUTTER_GROUND, INSERT_VIOLET, JOIN_FRAME,
    JOIN_WASH, KEPT_BORDER, KEPT_EDGE, KEPT_TINT, KEPT_TINT_ALPHA, LANE_BORDER, LANE_GROUND,
    MARK_IN_GREEN, MARK_OUT_RED, OVER_EATEN, PLAYHEAD_RGB, RULER_INK, SELECTION_BLUE,
    WATCH_OUTLINE,
};
pub const PLAYHEAD_PX: f64 = 2.0;
/// §A: ruler 18 px, selection band 22, effects row 26 (`fx_lane::lane_height_px(1)`), picture row `thumbHt + 4`.
/// The ruler's 16 vs 18 is kept: `cut_trim::RULER_H` is what F2.8's press bands resolve against.
pub const SELECTION_BAND_PX: f64 = 22.0;
pub const LANE_DASH_PX: [f64; 2] = [4.0, 3.0];
pub const PICTURE_ROW_EXTRA_PX: f64 = 4.0;
/// The wave strip's share of a camera row (§27 Tracks): the lower third of the row's pitch, chosen here since no §10 row gives it.
pub const ROW_WAVE_PX: f64 = (THUMB_DEFAULT_PX as f64 + PICTURE_ROW_EXTRA_PX) / 3.0;
pub const WAVE_MIN_PX: f64 = 6.0;
pub const FILM_MIN_PX: f64 = 16.0;
const MARK_LINE_PX: f64 = 3.0;
const FLAG_PX: f64 = 10.0;
pub const CROSS_MIN_PX: f64 = 50.0;

/// One picture-row pitch at the open size: the row's height plus §A's 4 px; the live pitch is [`rows_pitch_px`].
pub const ROWS_PITCH_PX: i32 = (THUMB_DEFAULT_PX as i32) + (PICTURE_ROW_EXTRA_PX as i32);

thread_local! {
    /// §A's 🖼− / 🖼+ thumbnail height (40..160 px), kept for the session, not saved.
    static THUMB_PX: std::cell::Cell<u32> = const { std::cell::Cell::new(THUMB_DEFAULT_PX) };
    /// The sound strip's height as the grip left it; session only, like the thumbnail height.
    static WAVE_PX: std::cell::Cell<f64> = const { std::cell::Cell::new(ROW_WAVE_PX) };
}

pub fn thumb_px() -> u32 {
    THUMB_PX.with(std::cell::Cell::get)
}

pub fn set_thumb_px(px: u32) {
    THUMB_PX.with(|cell| cell.set(px.clamp(cut_screen::THUMB_MIN, cut_screen::THUMB_MAX)));
}

/// The sound strip's height as the grip left it; every painter and press reader asks this, never the opening [`ROW_WAVE_PX`].
pub fn row_wave_px() -> f64 {
    let room = thumb_px() as f64 + PICTURE_ROW_EXTRA_PX - FILM_MIN_PX;
    WAVE_PX
        .with(std::cell::Cell::get)
        .min(room)
        .max(WAVE_MIN_PX)
}

pub fn set_row_wave_px(px: f64) {
    let room = thumb_px() as f64 + PICTURE_ROW_EXTRA_PX - FILM_MIN_PX;
    WAVE_PX.with(|cell| cell.set(px.min(room).max(WAVE_MIN_PX)));
}

pub fn rows_pitch_px() -> i32 {
    thumb_px() as i32 + PICTURE_ROW_EXTRA_PX as i32
}

/// §A's tick ladder `0.2 0.5 1 2 5 10 30 60 120 300 600 s`: the FIRST step at least 70 px apart at this zoom.
pub fn tick_step_seconds(pps: f64) -> f64 {
    const LADDER: [f64; 11] = [
        0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
    ];
    LADDER
        .into_iter()
        .find(|step| step * pps >= 70.0)
        .unwrap_or(*LADDER.last().expect("the ladder is not empty"))
}

/// Everything the painter needs, resolved: built by `Scene::of` or by a test, so no rule hides in a draw callback.
#[derive(Debug, Clone)]
pub struct Scene {
    pub pps: f64,
    /// Scroll into the timeline, px; the band is drawn translated by `-origin`.
    pub origin: f64,
    pub segs: Vec<cut::Seg>,
    pub fx: Vec<cut::Fx>,
    pub recordings: Vec<timeline::Recording>,
    /// The row each of `recordings` is drawn on (`timeline::rows_for`), in order; empty means all on row 0. One
    /// camera's files filmed in sequence share a row.
    pub recording_rows: Vec<usize>,
    pub folds: Vec<[f64; 2]>,
    pub selection: Option<(f64, f64)>,
    pub watched_row: Option<i32>,
    pub row_count: usize,
    pub shift: std::collections::BTreeMap<String, f64>,
    pub lanes: Vec<cut::Lane>,
    /// §05-cut F2.5 badge 2: each lane's name plate with its channel layout (`‹name› mono`), from `crate::cut_lanes::plates`.
    pub lane_plates: Vec<String>,
    /// Thumbnails keyed by the recording's base name; an absent entry draws no pictures.
    pub thumbs: Vec<(String, Vec<std::path::PathBuf>)>,
    pub marks: crate::ui::cut_band_marks::BandMarks,
    /// The red line's second (§1's badge 30), here so the playhead layer asks the SAME scene as the band.
    pub line: f64,
}

/// Where one second of the tape is DRAWN, in the band's pixels, after the 30 px gutter, unscrolled (§B: second 0 is at x = 30).
/// A folded cell carries no width, so a folded stretch draws nothing and its neighbours close up.
pub fn band_x(scene: &Scene, t: f64) -> f64 {
    let span = band_span(scene);
    // Past the last filmed second the seconds still run linearly: extrapolate, or every late mark piles on `total_px`.
    let last = span
        .cells
        .iter()
        .filter(|cell| !cell.folded)
        .map(|cell| cell.end)
        .fold(0.0_f64, f64::max);
    span.x_of(t.min(last)) + (t - last).max(0.0) * scene.pps
}

/// The second a press at `x_px` lands on: [`band_x`] inverted, with the scroll added back, through folds and
/// unfilmed gaps (neither takes width), extrapolated past the last filmed second.
pub fn band_t_at(scene: &Scene, x_px: f64) -> f64 {
    span_t_at(&band_span(scene), x_px + scene.origin)
}

/// [`timeline::Span::t_at`] extrapolated past the end at the span's zoom; `x` is unscrolled.
pub fn span_t_at(span: &timeline::Span, x: f64) -> f64 {
    let pps = span.pps.max(f64::EPSILON);
    let last = span
        .cells
        .iter()
        .filter(|cell| !cell.folded)
        .map(|cell| cell.end)
        .reduce(f64::max);
    let Some(last) = last else {
        return ((x - span.gutter_px) / pps).max(0.0);
    };
    let total = span.total_px();
    if x >= total {
        return last + (x - total) / pps;
    }
    span.t_at(x).max(0.0)
}

pub fn span_x_of(span: &timeline::Span, t: f64) -> f64 {
    let last = span
        .cells
        .iter()
        .filter(|cell| !cell.folded)
        .map(|cell| cell.end)
        .fold(0.0_f64, f64::max);
    if span.cells.iter().all(|cell| cell.folded) {
        return span.gutter_px + t * span.pps;
    }
    span.x_of(t.min(last)) + (t - last).max(0.0) * span.pps
}

/// The band's laid-out span (§B: runs cut at folded gaps, gutter first), rebuilt per call so a fold toggle repaints through it.
pub fn band_span(scene: &Scene) -> timeline::Span {
    timeline::Span::new(
        timeline::cells(&scene.runs(), &scene.folds),
        scene.pps,
        cut_screen::GUTTER_PX,
    )
}

/// The kept clips' DRAWN `(x, width)` pairs through [`band_x`], by index into `scene.segs`; `draw_kept_bars` paints these boxes.
pub fn band_boxes(scene: &Scene) -> Vec<(f64, f64)> {
    scene
        .segs
        .iter()
        .map(|seg| {
            let shift = shift_of_camera(&scene.shift, seg.cam);
            band_span_px(
                scene,
                session_second(seg.s, shift),
                session_second(seg.e, shift),
            )
        })
        .collect()
}

/// A clip's second on the SESSION clock after its camera's shift; `s`/`e` are already session seconds.
fn session_second(local: f64, shift: f64) -> f64 {
    // `session_at(session, session, …)` would double the second.
    (local - shift).max(0.0)
}

pub fn band_span_px(scene: &Scene, from: f64, to: f64) -> (f64, f64) {
    let x0 = band_x(scene, from);
    let x1 = band_x(scene, to.max(from));
    (x0, (x1 - x0).max(0.0))
}

impl Scene {
    /// The row recording `index` is drawn on (0 when the scene was built without a colouring).
    pub(crate) fn row_of(&self, index: usize) -> usize {
        self.recording_rows.get(index).copied().unwrap_or(0)
    }

    pub(crate) fn runs(&self) -> Vec<(f64, f64)> {
        timeline::filmed_runs(&self.recordings)
    }

    pub(crate) fn tape_end(&self) -> f64 {
        self.runs().last().map(|(_, end)| *end).unwrap_or(0.0)
    }
}

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
        recording_rows: Vec::new(),
        marks: Default::default(),
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

pub fn scene_from(
    cut_: &cut::Cut,
    recordings: &[timeline::Recording],
    selection: Option<(f64, f64)>,
    watched_row: Option<usize>,
    line: f64,
) -> Scene {
    let mut scene = scene_of(
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
    );
    scene.recording_rows = timeline::rows_for(recordings, cut_);
    scene.marks = crate::ui::cut_band_marks::marks_for(
        cut_,
        recordings,
        &scene.recording_rows,
        scene.row_count,
        line,
        crate::ui::cut_hand::held_clip_or_edge().as_ref(),
    );
    scene.marks.whole = crate::ui::cut_band_marks::whole_takes();
    scene
}

/// The band's laid-out height for THIS window, from the scene the page would paint. `install` sized the surfaces
/// before any cut, so anything charging for the stack (layout, snapshot crop) asks this, not a widget not yet re-laid-out.
pub fn band_height_for_window() -> i32 {
    let mut scene = crate::ui::cut_view::band_scene_for();
    scene.thumbs = scene
        .recordings
        .iter()
        .map(|recording| {
            (
                recording.base.clone(),
                crate::ui::cut_picture::snapshot_frame_tree()
                    .map(|tree| frame_paths(&tree, &recording.base))
                    .unwrap_or_default(),
            )
        })
        .collect();
    band_height_for(&scene)
}

pub fn scrollbar() -> gtk::Scrollbar {
    let bar = gtk::Scrollbar::new(
        gtk::Orientation::Horizontal,
        Some(&gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0)),
    );
    bar.set_widget_name("track-scrollbar");
    bar
}
/// Build the band, hang both layers on it and insert it plus its scrollbar after `after`, returning the scrollbar.
/// `scene` and `thumbs` are asked at draw time, so a project opened later still gets its frames.
pub fn install(
    window: &adw::ApplicationWindow,
    host: &gtk::Box,
    after: &gtk::Widget,
    scene: impl Fn() -> Scene + Clone + 'static,
    tree: impl Fn() -> Option<layout::Tree> + Clone + 'static,
) -> gtk::Scrollbar {
    // Moved into the draw closure (`'static`). Frames are listed on a worker and held per folder version
    // (`cut_picture_cache`, §03-shell §1): a paint never lists. A folder with none draws no pictures (§1).
    let thumb_for = move |base: &str| {
        tree()
            .map(|tree| crate::ui::cut_picture_cache::frames_of(&tree, base))
            .unwrap_or_default()
    };
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
        band_height_px(&with_thumbs(), thumb_px()).ceil() as i32,
        // The MINIMUM, not the width: `build` makes the band expand; headless tests never lay out.
        BAND_WIDTH_PX,
    );
    attach(&band.surface, &band.playhead, &band.layers, with_thumbs);
    // The band grows with every row an edit adds and WIDENS with the page: the draw is clipped to its allocation, so
    // ask for the width GTK gave it or the ruler stops at 480 px. Redraw doors find the surfaces via [`band_of`].
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
    let measured = window.clone();
    band.surface.connect_resize(move |_, _, _| {
        let measured = measured.clone();
        glib::idle_add_local_once(move || crate::ui::cut_view::settle_view(&measured));
    });
    // F2.12 S6: the hold goes on wherever a band is installed. Hand over the surface just built: `band_of` would
    // answer an OLDER page's band here, as `host.insert_child_after` below has not yet put this one in the tree.
    crate::ui::cut_card_hold::wire(window, &band.surface);
    host.insert_child_after(&band.layers, Some(after));
    let bar = scrollbar();
    let stacked: gtk::Widget = band.layers.upcast();
    host.insert_child_after(&bar, Some(&stacked));
    bar
}

/// The band's MINIMUM width, so an unlaid-out page (headless) still measures a real tape. The band spans the PAGE via `hexpand` + `halign(FILL)` in [`build`].
pub const BAND_WIDTH_PX: i32 = 480;

pub fn band_height_for(scene: &Scene) -> i32 {
    band_height_px(scene, thumb_px()).ceil() as i32
}

/// The band's height with nothing cut or fitted: the same sum the painter lays out, asked of an empty scene (an empty
/// lane is one row, `fx_lane::row_count`; unknown camera count is one row), so layout and ink agree on where the tape begins.
pub fn idle_band_height() -> i32 {
    band_height_for(&idle_scene(1))
}

/// The scene the band costs with nothing cut: `count` footage rows (at least one, as the painter), no clips, effects or
/// lanes. `cut_page_layout` prices the bottom pane from it, or the second camera row is clipped off the surface.
pub fn idle_scene(count: usize) -> Scene {
    Scene {
        pps: crate::ui::cut_view::pps(),
        origin: 0.0,
        segs: Vec::new(),
        fx: Vec::new(),
        recordings: Vec::new(),
        recording_rows: Vec::new(),
        marks: Default::default(),
        folds: Vec::new(),
        selection: None,
        watched_row: None,
        row_count: count.max(1),
        shift: Default::default(),
        lanes: Vec::new(),
        lane_plates: Vec::new(),
        thumbs: Vec::new(),
        line: 0.0,
    }
}

/// Hang both layers on their drawing areas; everything painted comes from `scene` at draw time. Both return `Proceed`
/// so existing button and gesture handling keeps working. The height is settled when the picture is asked for
/// (the door [`fit_band_height_for_window`] opens), so a redraw that only queues a frame lands on the right height.
pub fn attach(
    surface: &gtk::DrawingArea,
    playhead: &gtk::DrawingArea,
    layers: &gtk::Overlay,
    scene: impl Fn() -> Scene + Clone + 'static,
) {
    let for_band = scene.clone();
    let band_area = surface.clone();
    let overlay = layers.clone();
    surface.set_draw_func(move |_, cr, width, height| {
        let held = for_band();
        fit_band_height(&overlay, &band_area, &held);
        paint_at(
            cr,
            &held,
            width.max(1) as f64,
            height.max(1) as f64,
            thumb_px(),
        );
    });
    let for_head = scene.clone();
    playhead.set_draw_func(move |_, cr, _width, height| {
        let held = for_head();
        paint_playhead(cr, playhead_x(&held), height.max(1) as f64);
    });
}

/// The band (repaints on every edit) and the playhead (every tick), two `DrawingArea`s in one overlay (§1).
pub struct Band {
    pub surface: gtk::DrawingArea,
    pub playhead: gtk::DrawingArea,
    pub layers: gtk::Overlay,
    pub gutter_badges: gtk::Box,
    pub gutter_switches: gtk::Box,
}

pub fn build(height: i32, width: i32) -> Band {
    let surface = gtk::DrawingArea::new();
    surface.set_widget_name("track-band");
    picture_rows::note_band(&surface);
    // §1: the band runs the PAGE's width; `FILL` (not `START`) makes the expansion real, or GTK keeps handing every draw 480 px.
    surface.set_size_request(width, height);
    surface.set_hexpand(true);
    surface.set_halign(gtk::Align::Fill);
    let playhead = gtk::DrawingArea::new();
    playhead.set_widget_name("track-playhead");
    playhead.set_size_request(width, height);
    playhead.set_hexpand(true);
    playhead.set_halign(gtk::Align::Fill);
    // The line overlay must not swallow the presses the band owns.
    playhead.set_can_target(false);
    let layers = gtk::Overlay::new();
    layers.set_widget_name("track-layers");
    layers.set_hexpand(true);
    layers.set_halign(gtk::Align::Fill);
    // A named holder for the per-gap fold badges (`fold-button-<i>`, §B), overlay children so they share the band's pixels.
    let badges = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    badges.set_widget_name("fold-badges");
    badges.set_can_target(false);
    badges.set_halign(gtk::Align::Start);
    badges.set_valign(gtk::Align::Start);
    // §B's gutter is a 30 px RAIL of overlay children inside `track-layers`, found by name like `fold-badges`:
    // `gutter-badges` takes the whole-cut fold badge (§1 badge 23), `gutter-switches` the lane switches and empty-row ✕ (badge 29).
    let rail = || {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.set_can_target(false);
        column.set_halign(gtk::Align::Start);
        column.set_valign(gtk::Align::Start);
        column.set_margin_start(2);
        column.set_margin_top(cut_trim::RULER_H as i32 + 2);
        column
    };
    let gutter_badges = rail();
    gutter_badges.set_widget_name("gutter-badges");
    let gutter_switches = rail();
    gutter_switches.set_widget_name("gutter-switches");
    layers.set_child(Some(&surface));
    layers.add_overlay(&playhead);
    layers.add_overlay(&badges);
    layers.add_overlay(&gutter_badges);
    layers.add_overlay(&gutter_switches);
    // Per-row sound switches in a `Fixed` of their own: the stacking rail would move every control below a switch.
    let gutter_rows = gtk::Fixed::new();
    gutter_rows.set_widget_name("gutter-row-switches");
    gutter_rows.set_halign(gtk::Align::Start);
    gutter_rows.set_valign(gtk::Align::Start);
    gutter_rows.add_css_class("cut-gutter-col");
    layers.add_overlay(&gutter_rows);
    // The rail is dark ink: controls must be flat glyphs, not Adwaita's light button plate (`spec/img/05-cut.png`, badges 23/29).
    for column in [&gutter_badges, &gutter_switches] {
        column.add_css_class("cut-gutter-col");
    }
    install_gutter_css_once();
    for holder in [
        badges.upcast_ref::<gtk::Widget>(),
        gutter_badges.upcast_ref(),
        gutter_switches.upcast_ref(),
        gutter_rows.upcast_ref(),
    ] {
        holder.set_visible(false);
    }
    Band {
        surface,
        playhead,
        layers,
        gutter_badges,
        gutter_switches,
    }
}

/// Flat rail buttons sized to the 30 px column (§B gives the gutter 30 px of TIMELINE space).
fn install_gutter_css_once() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&format!(
            ".cut-gutter-col button {{ background: none; border: none; box-shadow: none; \
             min-width: 26px; min-height: 18px; padding: 0 1px; margin: 0; }}\n\
             /* The rail is dark ink (`draw_gutter`), so its glyphs must arrive in light ink: left at the\
                theme's default colour the whole-cut fold-all badge painted dark-on-dark and read as absent\
                -- the render showed only a smudge at the top of the gutter. */\n\
             .cut-gutter-col button {{ color: #f2f2f2; font-size: 15px; }}\n\
             /* §1's gutter shows the per-lane sound switches as ROUND buttons with a ground of their own\
                (badge 29, `spec/img/05-cut.png`), unlike the fold-all badge which is a bare glyph: only\
                those get a plate. 22 px inside the {} px column leaves the round shape room to read as\
                round under the cairo renderer. */\n\
             .cut-gutter-col button.round {{ background: #6b6b6b; border-radius: 13px;\n\
                 min-width: 22px; min-height: 22px; color: #f2f2f2; }}",
            crate::cut_screen::GUTTER_PX as i32
        ));
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("a display to style"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

/// This window's gutter rail by name, searched from `content` (a closed window's tree can survive; the `fold_badges_box` rule).
fn gutter_column(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Box> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(content.upcast_ref(), name)?
        .downcast::<gtk::Box>()
        .ok()
}

/// The rail for the whole-cut fold badge: `gutter-badges` (§B), else the older `fold-all-holder` for a page without a band.
pub fn gutter_badges_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    gutter_column(window, "gutter-badges").or_else(|| crate::ui::cut_folds::fold_badges_box(window))
}

pub fn gutter_switches_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    gutter_column(window, "gutter-switches")
}

pub fn gutter_row_switches(window: &adw::ApplicationWindow) -> Option<gtk::Fixed> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(content.upcast_ref(), "gutter-row-switches")?
        .downcast::<gtk::Fixed>()
        .ok()
}

pub fn row_wave_top(row: usize) -> f64 {
    rows(&crate::ui::cut_view::band_scene_for(), thumb_px()).wave_top(row)
}

/// The band's two surfaces found again through the window by the names [`build`] gave them, so no remembered copy goes stale.
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

/// Grow THIS window's band to the scene it is about to paint; the door the page's redraw calls. [`install`] sized it
/// for the page as it opened and a `DrawingArea` clips to its allocation.
pub fn fit_band_height_for_window(window: &adw::ApplicationWindow, scene: &Scene) {
    if let Some((band, layers)) = band_of(window) {
        fit_band_height(&layers, &band, scene);
    }
}

pub fn band_height_px(scene: &Scene, thumb_px: u32) -> f64 {
    rows(scene, thumb_px).height(scene)
}

/// Grow the band's surfaces to the scene (a row per camera AND per cut lane). Compared against the SIZE REQUEST, not
/// the measured height, which only catches up after the next layout. The red line is grown too (§1: "across both bands").
pub fn fit_band_height(layers: &gtk::Overlay, band: &gtk::DrawingArea, scene: &Scene) {
    let tall = band_height_for(scene);
    if band.size_request().1 == tall {
        return;
    }
    // Height only: a width request pins the band to its minimum and undoes the span from `hexpand` in [`build`].
    band.set_size_request(-1, tall);
    layers.set_size_request(-1, tall);
    let held = layers.observe_children();
    for index in 0..held.n_items() {
        let Some(child) = held.item(index) else {
            continue;
        };
        let Ok(area) = child.downcast::<gtk::DrawingArea>() else {
            continue;
        };
        if area != *band {
            area.set_size_request(-1, tall);
        }
    }
}

impl Rows {
    fn height(&self, scene: &Scene) -> f64 {
        // A lane is a ROW of the band (§F2.12 S5), costing height as a camera does; `lane_rows` is the same count.
        self.pictures + (scene.row_count.max(1) + lane_rows(scene)) as f64 * self.row_h
    }

    pub fn lane_top(&self, scene: &Scene, lane: usize) -> f64 {
        self.pictures + self.camera_rows(scene) as f64 * self.row_h + lane as f64 * self.row_h
    }

    pub fn camera_rows(&self, scene: &Scene) -> usize {
        scene.row_count.max(1)
    }
}

pub const ROW_GRIP_PX: f64 = 3.0;

/// The thumbnail-size grip under band height `y` (§1 badge 2): the bottom edge of a picture row within [`ROW_GRIP_PX`].
/// Answers how many rows sit at or above that edge, since a drag grows every one of them.
pub fn row_grip_at(scene: &Scene, y: f64) -> Option<usize> {
    let rows = rows_for_press(scene);
    let count = rows.camera_rows(scene) + lane_rows(scene);
    (1..=count)
        .find(|above| (y - (rows.pictures + *above as f64 * rows.row_h)).abs() <= ROW_GRIP_PX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    Row(usize),
    Wave,
}

/// The grip under band height `y`, shared by press and pointer. The bottom edge goes first when both are in reach.
pub fn grip_at(scene: &Scene, y: f64) -> Option<Grip> {
    if let Some(above) = row_grip_at(scene, y) {
        return Some(Grip::Row(above));
    }
    let rows = rows_for_press(scene);
    let count = rows.camera_rows(scene) + lane_rows(scene);
    (0..count)
        .any(|row| (y - rows.wave_top(row)).abs() <= ROW_GRIP_PX)
        .then_some(Grip::Wave)
}

/// Rows the band gives the cut lanes, one each below the cameras; a spliced or overwriting card takes no row.
pub fn lane_rows(scene: &Scene) -> usize {
    scene.lanes.len()
}

/// Each cut lane as its own row below the cameras (§F2.12 S5). The PRESS surfaces (`bands_for`) deliberately
/// stop at the camera rows: F2.9/F2.10 own what a click on a row means.
fn draw_lane_rows(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for (index, lane) in scene.lanes.iter().enumerate() {
        let top = rows.lane_top(scene, index);
        // Past the gutter: a lane row filling from x = 0 paints over the rail and its sound switch reads as on the lane.
        let left = cut_screen::GUTTER_PX;
        fill(
            cr,
            LANE_GROUND,
            left,
            top,
            (tape - left).max(0.0),
            rows.row_h,
            1.0,
        );
        ink(cr, LANE_BORDER);
        cr.rectangle(left, top, (tape - left).max(0.0), 1.0);
        let _ = cr.fill();
        let (x0, width) = band_span_px(scene, lane.at, lane.at + lane.dur);
        if width > 0.0 {
            fill(
                cr,
                INSERT_VIOLET,
                x0,
                top + 2.0,
                width,
                rows.row_h - 4.0,
                0.75,
            );
        }
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(9.0);
        ink(cr, (0.9, 0.9, 0.92));
        let plate = scene
            .lane_plates
            .get(index)
            .map(String::as_str)
            .unwrap_or(lane.name.as_str());
        text_at(cr, left + 2.0, top + 10.0, plate);
    }
}

pub fn paint(cr: &cairo::Context, scene: &Scene, width: f64, height: f64) {
    paint_at(cr, scene, width, height, THUMB_DEFAULT_PX);
}

pub fn paint_at(cr: &cairo::Context, scene: &Scene, width: f64, height: f64, thumb_px: u32) {
    fill(cr, GROUND, 0.0, 0.0, width, height, 1.0);
    let rows = rows(scene, thumb_px);
    let tape = width.max(BAND_WIDTH_PX as f64) + scene.origin;
    let _ = cr.save();
    cr.translate(-scene.origin, 0.0);
    draw_ruler(cr, scene, tape);
    draw_band_ground(cr, scene, rows, tape);
    draw_kept_bars(cr, scene, rows, tape);
    draw_effects_lane(cr, scene, rows, tape);
    picture_rows::draw_picture_rows(cr, scene, rows, tape);
    crate::ui::cut_wave_rows::draw_row_waves(cr, scene, rows, tape);
    draw_kept_tint(cr, scene, rows, tape);
    draw_join_washes(cr, scene, rows, tape);
    draw_recording_edges(cr, scene, rows, tape);
    draw_watched_row(cr, scene, rows, tape);
    draw_selection(cr, scene, rows, tape);
    draw_insert_bands(cr, scene, rows, tape);
    draw_in_out_marks(cr, scene, rows, tape);
    draw_laid_pieces(cr, scene, rows, tape);
    draw_lane_rows(cr, scene, rows, tape);
    crate::ui::cut_band_marks::paint_tape(cr, scene, rows);
    let _ = cr.restore();
    draw_gutter(cr, rows, height);
    crate::ui::cut_band_marks::paint_pinned(cr, scene, rows);
    draw_name_plates(cr, scene, rows);
}

/// §B's gutter: the first 30 px, inked from the ruler's bottom edge down (`draw_ruler` prints its first label at x = 2).
/// Painted LAST (see [`paint_at`]): several painters fill from x = 0 and would paint back over the rail. Keep the call order.
fn draw_gutter(cr: &cairo::Context, rows: Rows, height: f64) {
    fill(
        cr,
        GUTTER_GROUND,
        0.0,
        rows.selection,
        cut_screen::GUTTER_PX,
        (height - rows.selection).max(0.0),
        1.0,
    );
    ink(cr, GUTTER_EDGE);
    cr.rectangle(
        cut_screen::GUTTER_PX - 1.0,
        rows.selection,
        1.0,
        (height - rows.selection).max(0.0),
    );
    let _ = cr.fill();
}

/// F2.9 S2: the pieces a sound paste laid over the kept footage, with the painter [`crate::ui::cut_trim_press::paint_laid_pieces_in`].
fn draw_laid_pieces(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let _ = cr.save();
    cr.translate(cut_screen::GUTTER_PX, 0.0);
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
    let _ = cr.restore();
}

pub const THUMB_DEFAULT_PX: u32 = cut_screen::THUMB_AT_OPEN;

#[derive(Debug, Clone, Copy)]
pub struct Rows {
    pub selection: f64,
    pub effects: f64,
    pub pictures: f64,
    pub row_h: f64,
}

impl Rows {
    /// Where one row's wave strip begins: the lower [`row_wave_px`] of its picture row. Painters and press readers all ask this.
    pub fn wave_top(&self, row: usize) -> f64 {
        self.pictures + row as f64 * self.row_h + (self.row_h - row_wave_px())
    }
}

/// The `rows` arithmetic as the surfaces a press can land on (§A's order: ruler, selection band, effects lane, then a
/// picture row with its wave strip per camera row), so a press handler need not build a whole `Scene`.
pub fn bands_for(fx_rows: usize) -> crate::cut_select::SurfaceBands {
    let held = rows_of(fx_rows, thumb_px());
    // A camera row is a picture band PLUS its wave strip (§27 Tracks), splitting the unchanged row PITCH ([`row_wave_px`]).
    // A wave-height of 0 would make `Surface::WaveStrip` unreachable and F2.9's footage/sound fork unaimable by hand.
    crate::cut_select::SurfaceBands {
        ruler: (0.0, cut_trim::RULER_H),
        selection: (cut_trim::RULER_H, held.effects),
        fx: (held.effects, held.pictures),
        first_picture: held.pictures,
        row_height: (held.row_h - row_wave_px()).max(0.0),
        wave_height: row_wave_px().min(held.row_h),
    }
}

pub fn rows_for_press(scene: &Scene) -> Rows {
    rows(scene, thumb_px())
}

fn rows(scene: &Scene, thumb_px: u32) -> Rows {
    rows_of(fx_lane::row_count(&scene.fx), thumb_px)
}

fn rows_of(fx_rows: usize, thumb_px: u32) -> Rows {
    let selection = cut_trim::RULER_H;
    let effects = crate::ui::cut_band_rows::lane_top_px(cut_trim::RULER_H, SELECTION_BAND_PX);
    let pictures = crate::ui::cut_band_rows::pictures_top_px(effects, fx_rows as f64);
    Rows {
        selection,
        effects,
        pictures,
        row_h: thumb_px as f64 + PICTURE_ROW_EXTRA_PX,
    }
}

fn draw_ruler(cr: &cairo::Context, scene: &Scene, tape: f64) {
    let step = tick_step_seconds(scene.pps);
    // The ticks count footage, not the session clock, so a night between two recordings is no jump in the numbers.
    // Each at its second's PAINTED x (after the gutter, through the folds); a tick inside a fold is dropped.
    let runs = scene.runs();
    let filmed = timeline::filmed_seconds(&runs, scene.tape_end());
    let mut last = f64::NEG_INFINITY;
    let marks: Vec<(f64, String)> = cut_trim::ruler_ticks(filmed, scene.pps, step)
        .into_iter()
        .filter_map(|(x, label)| {
            let painted = band_x(scene, timeline::clock_at(&runs, x / scene.pps));
            let fresh = painted > last + 1.0;
            last = last.max(painted);
            (fresh && painted <= tape).then_some((painted, label))
        })
        .collect();
    ink(cr, RULER_INK);
    for (x, _) in &marks {
        cr.rectangle(*x, 0.0, 1.0, cut_trim::RULER_H);
        let _ = cr.fill();
    }
    labels(cr, &marks);
}

fn labels(cr: &cairo::Context, ticks: &[(f64, String)]) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(9.0);
    ink(cr, RULER_INK);
    // Past a fold two ticks can land close together: the second keeps its line and gives up its label.
    let mut free_from = f64::NEG_INFINITY;
    for (x, label) in ticks {
        if *x >= free_from {
            text_at(cr, x + 2.0, cut_trim::RULER_H - 4.0, label);
            free_from = x + cr.text_extents(label).map_or(40.0, |seen| seen.width()) + 6.0;
        }
    }
}

fn draw_band_ground(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    for (from, to) in scene.runs() {
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        fill(
            cr,
            BAND_GROUND,
            x0,
            rows.selection,
            width,
            SELECTION_BAND_PX,
            1.0,
        );
    }
}

/// Green bar per kept scene (§1's badge 25, `go/cut_selband.go`): the scene the badges are about (held, else under the
/// line) is fuller and taller, with a light grip at each end. Boxes come from [`band_boxes`]; the ✕ is `cut_band_marks`'.
fn draw_kept_bars(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let current = scene.marks.hear.as_ref().map(|hear| hear.index);
    for (index, (x, w)) in band_boxes(scene).into_iter().enumerate() {
        if scene.segs[index].is_insert() || x >= tape || x + w < 0.0 {
            continue;
        }
        let (inset, alpha) = if current == Some(index) {
            (2.0, 0.5)
        } else {
            (4.0, 0.4)
        };
        fill(
            cr,
            KEPT_TINT,
            x,
            rows.selection + inset,
            w.max(1.0),
            SELECTION_BAND_PX - 2.0 * inset,
            alpha,
        );
        for grip in [x, x + w] {
            fill(
                cr,
                KEPT_BORDER,
                grip - 1.5,
                rows.selection,
                3.0,
                SELECTION_BAND_PX,
                1.0,
            );
        }
    }
}

/// The kept scenes over the pictures (`go/cut.go` 4154-4193): their stretch of ITS row tinted green, the wave strip fainter.
fn draw_kept_tint(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - row_wave_px()).max(0.0);
    for (index, (x, w)) in band_boxes(scene).into_iter().enumerate() {
        let seg = &scene.segs[index];
        if seg.is_insert() || x >= tape || x + w < 0.0 {
            continue;
        }
        let top = rows.pictures + seg.cam.max(0) as f64 * rows.row_h;
        fill(cr, KEPT_TINT, x, top, w, film, KEPT_TINT_ALPHA);
        fill(cr, KEPT_TINT, x, top + film, w, row_wave_px(), 0.16);
        for edge in [x, x + w] {
            fill(cr, KEPT_EDGE, edge - 1.0, top, 2.0, rows.row_h, 1.0);
        }
    }
}

fn draw_effects_lane(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    // A bar under the span floor reserves no row (§2); painted after the row's owners it stays visible.
    let mut bars = fx_lane::lane_layout(&scene.fx);
    bars.sort_by_key(|bar| !bar.reserves_a_row);
    for bar in bars {
        let (from, to) = scene.fx[bar.index].spans();
        let y = crate::ui::cut_band_rows::lane_row_top_px(rows.effects, bar.row);
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        // Same painter as the Cut page's own bars (`cut_lane_paint`), clipped to the bar's painted span.
        let view = crate::ui::cut_lane_paint::view_for(
            &scene.fx[bar.index],
            width.max(1.0),
            fx_lane::lane_height_px(1),
        );
        let _ = cr.save();
        cr.rectangle(x0, y, width.max(0.0), fx_lane::lane_height_px(1));
        cr.clip();
        cr.translate(x0, y);
        crate::ui::cut_lane_paint::paint_bar(cr, &view);
        let _ = cr.restore();
    }
    // §2's lower edge is DASHED (`spec/img/06-lane.png`), on the lane's LAST pixel row (`rows.pictures - 1`): §05's decode
    // gate samples the next row. `- 0.5` at width 1 covers exactly one pixel row; clear the pattern after or cairo hands it to every later stroke.
    ink(cr, LANE_BORDER);
    cr.set_line_width(1.0);
    cr.set_dash(&LANE_DASH_PX, 0.0);
    cr.move_to(cut_screen::GUTTER_PX, rows.pictures - 0.5);
    cr.line_to(tape, rows.pictures - 0.5);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
}

fn draw_recording_edges(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - row_wave_px()).max(1.0);
    for (index, recording) in scene.recordings.iter().enumerate() {
        let top = rows.pictures + scene.row_of(index) as f64 * rows.row_h;
        let (x0, x1) = (
            band_x(scene, recording.start) + 1.0,
            band_x(scene, recording.end) - 1.0,
        );
        if x1 < 0.0 || x0 > tape {
            continue;
        }
        edge_mark(cr, x0, 1.0, top, film, x1 - x0);
        if x1 - x0 > 3.0 {
            edge_mark(cr, x1, -1.0, top, film, x1 - x0);
        }
    }
}

fn edge_mark(cr: &cairo::Context, x: f64, dir: f64, top: f64, h: f64, room: f64) {
    let w = (room / 2.0).clamp(1.0, 9.0);
    let _ = cr.save();
    cr.rectangle(x.min(x + dir * w), top, w, h);
    cr.clip();
    cr.set_source_rgba(EDGE_AMBER.0, EDGE_AMBER.1, EDGE_AMBER.2, 0.7);
    cr.set_line_width(1.5);
    let mut dy = -w;
    while dy < h {
        cr.move_to(x, top + dy + w);
        cr.line_to(x + dir * w, top + dy);
        dy += 5.0;
    }
    let _ = cr.stroke();
    let _ = cr.restore();
    ink(cr, EDGE_AMBER);
    cr.set_line_width(2.0);
    cr.move_to(x, top);
    cr.line_to(x, top + h);
    let _ = cr.stroke();
}

/// The two washes §1 names over the picture: yellow over a recording the join pass took out entire (`retakes.tsv`
/// `whole`, F1.10; flagged, not an error), rose under a SPEED effect.
fn draw_join_washes(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - row_wave_px()).max(1.0);
    for (index, recording) in scene.recordings.iter().enumerate() {
        if !scene.marks.whole.contains(&recording.base) {
            continue;
        }
        let (x0, w) = band_span_px(scene, recording.start, recording.end);
        if x0 >= tape || x0 + w < 0.0 || w <= 2.0 {
            continue;
        }
        let top = rows.pictures + scene.row_of(index) as f64 * rows.row_h;
        fill(cr, JOIN_WASH, x0, top, w, film, 0.3);
        ink(cr, JOIN_FRAME);
        cr.set_line_width(2.0);
        cr.rectangle(x0 + 1.0, top + 1.0, w - 2.0, film - 2.0);
        let _ = cr.stroke();
    }
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
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
        fill(cr, rose, x0, rows.pictures, width, height, 0.35);
    }
}

fn draw_watched_row(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some(row) = scene.watched_row else { return };
    let y = rows.pictures + row.max(0) as f64 * rows.row_h;
    // Own rects rather than a stroked rectangle: a stroke straddles the path, so half falls outside the band for the bottom row.
    ink(cr, WATCH_OUTLINE);
    let from = band_x(scene, scene.runs().first().map(|run| run.0).unwrap_or(0.0));
    let to = band_x(scene, scene.tape_end()).min(tape);
    let mut x = from;
    while x < to {
        cr.rectangle(x, y, 12.0_f64.min(to - x), 2.0);
        cr.rectangle(x, y + rows.row_h - 2.0, 12.0_f64.min(to - x), 2.0);
        x += 20.0;
    }
    let _ = cr.fill();
}

fn draw_selection(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some((from, to)) = scene.selection else {
        return;
    };
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
        0.6,
    );
    fill(
        cr,
        SELECTION_BLUE,
        x0,
        rows.pictures,
        width.max(1.0),
        rows.row_h * scene.row_count.max(1) as f64,
        0.35,
    );
    ink(cr, SELECTION_BLUE);
    for x in [x0, band_x(scene, to)] {
        cr.rectangle(x - 1.0, rows.selection, 2.5, SELECTION_BAND_PX);
        let _ = cr.fill();
    }
}

/// The kept scenes' boxes AT THE HAND CORRECTIONS: each scene moves by its camera's shift through [`timeline::session_at`]
/// (file offset 0, as these are session seconds), then the press geometry's box arithmetic, so paint and grab agree.
pub fn clip_boxes_shifted(
    segs: &[cut::Seg],
    shift: &std::collections::BTreeMap<String, f64>,
    pps: f64,
) -> Vec<cut_trim::Box_> {
    segs.iter()
        .enumerate()
        .map(|(index, seg)| {
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

fn shift_of_camera(shift: &std::collections::BTreeMap<String, f64>, cam: i32) -> f64 {
    shift.get(&format!("cam{cam}")).copied().unwrap_or(0.0)
}

fn draw_insert_bands(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
    let drawn = band_boxes(scene);
    let boxes: Vec<cut_trim::Box_> = clip_boxes_shifted(&scene.segs, &scene.shift, scene.pps)
        .into_iter()
        .map(|b| cut_trim::Box_ {
            x: drawn[b.index].0,
            w: if scene.segs[b.index].is_overwrite_insert() {
                // An over card carries no `dur`, so `band_boxes` measured it zero-wide; its own span is what was eaten.
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
        if scene.segs[b.index].is_overwrite_insert() {
            fill(
                cr,
                OVER_EATEN,
                b.x,
                rows.pictures,
                b.w.max(2.0),
                height,
                1.0,
            );
        }
        fill(
            cr,
            INSERT_VIOLET,
            b.x,
            rows.pictures,
            b.w.max(2.0),
            height,
            0.6,
        );
        if scene.segs[b.index].s == scene.segs[b.index].e {
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

fn draw_in_out_marks(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some((from, to)) = scene.selection else {
        return;
    };
    let height = rows.pictures + rows.row_h;
    // No clamp to the tape: `band_x` runs linearly past the last filmed second.
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

fn draw_name_plates(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(9.0);
    // Past the gutter, not at x = 2: the dark rail is painted AFTER this pass and would eat the name. Each line on its own
    // light backing, one under the other (dark ink read as clipped letters on dark frames; one y overprinted a correction).
    // One plate per ROW (§1), naming the recording at the view's left edge, so a scrolled band names what is under the plate.
    let rows_drawn = (0..scene.recordings.len())
        .map(|index| scene.row_of(index) + 1)
        .max()
        .unwrap_or(0);
    for row in 0..rows_drawn {
        let on_row: Vec<&timeline::Recording> = scene
            .recordings
            .iter()
            .enumerate()
            .filter(|(index, _)| scene.row_of(*index) == row)
            .map(|(_, recording)| recording)
            .collect();
        let mut on_row = on_row;
        on_row.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(recording) = on_row
            .iter()
            .find(|recording| band_x(scene, recording.end) > scene.origin)
            .or(on_row.last())
        else {
            continue;
        };
        let top = rows.pictures + row as f64 * rows.row_h;
        for (line, word) in plates(scene, recording).iter().enumerate() {
            let y = top + 10.0 + line as f64 * PLATE_LINE_PX;
            let x = cut_screen::GUTTER_PX + 2.0;
            if let Ok(extent) = cr.text_extents(word) {
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
                cr.rectangle(x - 2.0, y - 9.0, extent.x_advance() + 4.0, PLATE_LINE_PX);
                let _ = cr.fill();
            }
            ink(cr, (0.95, 0.95, 0.95));
            text_at(cr, x, y, word);
        }
    }
}

const PLATE_LINE_PX: f64 = 11.0;

fn text_at(cr: &cairo::Context, x: f64, y: f64, text: &str) {
    cr.move_to(x, y);
    let _ = cr.show_text(text);
}

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

/// Where the red line stands on the band's surface: [`band_x`] (after the gutter, through the folds) less the scroll.
pub fn playhead_x(scene: &Scene) -> f64 {
    band_x(scene, scene.line) - scene.origin
}

pub fn paint_playhead(cr: &cairo::Context, x: f64, height: f64) {
    cr.set_source_rgb(PLAYHEAD_RGB.0, PLAYHEAD_RGB.1, PLAYHEAD_RGB.2);
    cr.rectangle(x, 0.0, PLAYHEAD_PX, height);
    let _ = cr.fill();
}

/// The frames the band may show for one recording: Prepare's extracted JPEGs on the grid, oldest first. Cached in the
/// scene so a draw never touches the disk; a missing folder answers empty (§1's prototype note).
pub fn frame_paths(tree: &layout::Tree, base: &str) -> Vec<std::path::PathBuf> {
    let dir = tree.frames_dir(base);
    crate::ui::cut_picture_cache::note_listed_here();
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                // The frame's STAMP is the rule (§B.3, and `frame_near` compares names byte-wise), not one encoder's suffix:
                // `.jpg` from Prepare, plus `.png` so the snapshot bench and fixtures can plant a decodable file.
                .filter(|path| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| name.ends_with(".jpg") || name.ends_with(".png"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    // Two files can claim the same second and the row draws ONE cell per second; the newest file wins.
    out.dedup_by(|a, b| {
        let same_second = |p: &std::path::Path| {
            p.file_name()
                .and_then(|n| n.to_str())
                .and_then(clock::read_frame)
        };
        match (same_second(a), same_second(b)) {
            (Some(x), Some(y)) if (x - y).abs() < f64::EPSILON => {
                let newer = |p: &std::path::Path| {
                    std::fs::metadata(p)
                        .and_then(|m| m.modified())
                        .map(|t| {
                            t.duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0)
                        })
                        .unwrap_or(0)
                };
                newer(a) >= newer(b)
            }
            _ => false,
        }
    });
    out
}

/// Which extracted frame is nearest a second: the last at or before it, else the first, so "nearest" means "not from the future".
pub fn frame_near(paths: &[std::path::PathBuf], at: f64) -> Option<std::path::PathBuf> {
    // Compared as instants (`clock::read_frame`, both name shapes), not file names: `…48.000.jpg` sorts after `…48.000`.
    let instant = |path: &std::path::PathBuf| {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(clock::read_frame)
    };
    paths
        .iter()
        .rfind(|path| instant(path).is_some_and(|stamp| stamp <= at + 1e-6))
        .or_else(|| paths.first())
        .cloned()
}

pub(crate) fn fill(
    cr: &cairo::Context,
    rgb: (f64, f64, f64),
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    alpha: f64,
) {
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
