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

use gtk4::prelude::*;
use gtk4 as gtk;

use crate::{
    clock, cut, cut_fold, cut_screen, cut_trim, fx_lane, layout, timeline, tools,
};

/// Band ground: the same near-white the placeholder strip already paints, so the ruler's grey ink and the
/// kept green read identically on either surface.
pub const GROUND: (f64, f64, f64) = (0.95, 0.95, 0.95);
/// The ruler's ink — ticks and their labels.
const RULER_INK: (f64, f64, f64) = (0.45, 0.45, 0.45);
/// The kept scene's green: the placeholder's, kept so a shot does not change colour mid-migration.
const KEPT_GREEN: (f64, f64, f64) = (0.2, 0.8, 0.3);
const KEPT_BORDER: (f64, f64, f64) = (0.11, 0.42, 0.16);
/// The selection's blue (§1's "the selection, with its ✕").
const SELECT_BLUE: (f64, f64, f64) = (0.25, 0.5, 0.9);
/// An insert's violet, and the amber of a recording's striped edge.
const INSERT_VIOLET: (f64, f64, f64) = (0.55, 0.35, 0.75);
const RECORDING_EDGE: (f64, f64, f64) = (0.85, 0.6, 0.12);
/// A take the join pass removed whole: "flagged for a look, not an error" (F1.10), so a yellow wash and a
/// frame rather than red.
const JOIN_WASH: (f64, f64, f64) = (0.98, 0.9, 0.35);
/// In green, out red (§1's own words for the flags).
const MARK_IN: (f64, f64, f64) = (0.15, 0.7, 0.25);
const MARK_OUT: (f64, f64, f64) = (0.85, 0.2, 0.2);
/// §1's playhead: "2 px line on its own layer", RGB(0.9, 0.15, 0.15) per inventory/cut.md §A.
pub const PLAYHEAD_PX: f64 = 2.0;
pub const PLAYHEAD_RGB: (f64, f64, f64) = (0.9, 0.15, 0.15);
/// §A: the ruler is 18 px, the selection band 22 px, an effects row 26 (`cut_screen::EFFECT_ROW_PX`), a
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
/// The dashed white outline around the row a click set the preview to watch.
const WATCH_OUTLINE: (f64, f64, f64) = (1.0, 1.0, 1.0);

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
    /// Thumbnails the band can show, keyed by the recording's base name. Decoded elsewhere (the page
    /// decodes once per row height and hands the paths in); an absent entry draws no pictures.
    pub thumbs: Vec<(String, Vec<std::path::PathBuf>)>,
    /// The red line's second (§1's badge 30). It lives here rather than in a second thread-local because the
    /// playhead's own layer must ask the SAME scene the band asks, or the two layers disagree about where
    /// the line is whenever the page scrolls or zooms.
    pub line: f64,
}

impl Scene {
    /// The painted seconds: everything filmed, so a dropped stretch still takes its place in the band.
    fn runs(&self) -> Vec<(f64, f64)> {
        timeline::filmed_runs(&self.recordings)
    }

    /// The band's tape length in seconds, which bounds the ruler.
    fn tape_end(&self) -> f64 {
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
        BAND_WIDTH_PX,
    );
    attach(&band.surface, &band.playhead, with_thumbs, THUMB_DEFAULT_PX);
    host.insert_child_after(&band.layers, Some(after));
    let bar = scrollbar();
    let stacked: gtk::Widget = band.layers.upcast();
    host.insert_child_after(&bar, Some(&stacked));
    bar
}

/// The band's width: the placeholder strip's, so the two agree about where a pixel of the timeline is.
pub const BAND_WIDTH_PX: i32 = 480;

/// Hang both layers on their drawing areas; everything they paint comes from `scene`, called at draw time,
/// so neither callback holds state of its own. Both return `Proceed`: the band draws and does not consume
/// the event, so the page's existing button and gesture handling keeps working as it did on the strip.
pub fn attach(
    surface: &gtk::DrawingArea,
    playhead: &gtk::DrawingArea,
    scene: impl Fn() -> Scene + Clone + 'static,
    thumb_px: u32,
) {
    let for_band = scene.clone();
    surface.set_draw_func(move |_, cr, width, height| {
        paint_at(cr, &for_band(), width.max(1) as f64, height.max(1) as f64, thumb_px);
    });
    playhead.set_draw_func(move |_, cr, _width, height| {
        let held = scene();
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
    surface.set_size_request(width, height);
    surface.set_halign(gtk::Align::Start);
    let playhead = gtk::DrawingArea::new();
    playhead.set_widget_name("track-playhead");
    playhead.set_size_request(width, height);
    playhead.set_halign(gtk::Align::Start);
    // The overlay draws a line over what the band drew; it must not swallow the presses the band owns.
    playhead.set_can_target(false);
    let layers = gtk::Overlay::new();
    layers.set_widget_name("track-layers");
    layers.set_child(Some(&surface));
    layers.add_overlay(&playhead);
    Band {
        surface,
        playhead,
        layers,
    }
}

/// The band's height from the scene, in px.
pub fn band_height_px(scene: &Scene, thumb_px: u32) -> f64 {
    rows(scene, thumb_px).height(scene)
}

impl Rows {
    /// Ruler + selection band + effects lane + one picture row per recorded row — the same four sums the
    /// painter lays out, so a row added by an edit grows the surface instead of being cut off.
    fn height(&self, scene: &Scene) -> f64 {
        self.pictures + scene.row_count.max(1) as f64 * self.row_h
    }
}

/// Paint the whole band into `cr`, `width` x `height` px at the open-at thumbnail height. Pure: it reads
/// `scene` and draws; nothing in here decides a number the model did not already have.
pub fn paint(cr: &cairo::Context, scene: &Scene, width: f64, height: f64) {
    paint_at(cr, scene, width, height, THUMB_DEFAULT_PX);
}

/// The same with the page's real thumbnail height, which the form column's ladder sets.
pub fn paint_at(cr: &cairo::Context, scene: &Scene, width: f64, height: f64, thumb_px: u32) {
    let _ = width;
    fill(cr, GROUND, 0.0, 0.0, width, height, 1.0);
    let rows = rows(scene, thumb_px);
    draw_ruler(cr, scene);
    draw_dropped_gaps(cr, scene, rows);
    draw_kept_bars(cr, scene, rows);
    draw_effects_lane(cr, scene, rows);
    draw_picture_rows(cr, scene, rows);
    draw_row_wave_strips(cr, scene, rows);
    draw_recording_edges(cr, scene, rows);
    draw_join_washes(cr, scene, rows);
    draw_watched_row(cr, scene, rows);
    draw_selection(cr, scene, rows);
    draw_insert_bands(cr, scene, rows);
    draw_in_out_marks(cr, scene, rows);
    draw_laid_pieces(cr, scene, rows);
    draw_name_plates(cr, scene, rows);
}

/// F2.9 S2: the pieces a sound paste laid over the kept footage, on the PAINTED band as well as on the
/// window's placeholder strip. Same painter ([`crate::ui::cut_trim_press::paint_laid_pieces_in`]), this
/// caller's row geometry, drawn last but for the name plates so the plate sits on the footage it was laid
/// over. Without this the lay is invisible on the surface the Cut page actually shows.
fn draw_laid_pieces(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    crate::ui::cut_trim_press::paint_laid_pieces_in(
        cr,
        BAND_WIDTH_PX as f64,
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

/// The ruler and its labels, at §A's ladder.
fn draw_ruler(cr: &cairo::Context, scene: &Scene) {
    let step = tick_step_seconds(scene.pps);
    let ticks = cut_trim::ruler_ticks(scene.tape_end(), scene.pps, step);
    ink(cr, RULER_INK);
    for (x, _) in &ticks {
        cr.rectangle(*x, 0.0, 1.0, cut_trim::RULER_H);
        let _ = cr.fill();
    }
    labels(cr, &ticks);
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
fn draw_dropped_gaps(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    for (from, to) in cut_fold::dropped_gaps(&scene.runs(), &scene.segs) {
        fill(
            cr,
            (0.88, 0.88, 0.88),
            from * scene.pps,
            rows.selection,
            (to - from) * scene.pps,
            SELECTION_BAND_PX,
            1.0,
        );
        // The badge: `cut_fold::badges` decides WHICH gap wears one and where its x lands on a laid-out
        // span; here the mid-gap mark is that same middle, drawn as ink so a test can see it.
        let x = (from + to) / 2.0 * scene.pps;
        ink(cr, (0.35, 0.35, 0.4));
        cr.rectangle(x - 1.0, rows.selection + 4.0, 3.0, SELECTION_BAND_PX - 8.0);
        let _ = cr.fill();
    }
}

/// Green bar per kept scene (§1's badge 25), the placeholder's colour and borders so F2.8's grips stay
/// where its tests press them.
fn draw_kept_bars(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let boxes = clip_boxes_shifted(&scene.segs, &scene.shift, scene.pps);
    for b in &boxes {
        if b.insert {
            continue;
        }
        fill(cr, KEPT_GREEN, b.x, rows.selection, b.w.max(1.0), SELECTION_BAND_PX, 1.0);
    }
    ink(cr, KEPT_BORDER);
    for (x, _, _) in cut_trim::border_positions(&boxes) {
        cr.rectangle(x - 0.5, rows.selection, 1.5, SELECTION_BAND_PX);
        let _ = cr.fill();
    }
}

/// The effects lane: one row per overlapping group, each bar in its kind's colour.
fn draw_effects_lane(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    for bar in fx_lane::lane_layout(&scene.fx) {
        let (from, to) = scene.fx[bar.index].spans();
        let y = rows.effects + bar.row as f64 * cut_screen::EFFECT_ROW_PX;
        fill(
            cr,
            bar.colour,
            from * scene.pps,
            y,
            (to - from).max(0.0) * scene.pps,
            cut_screen::EFFECT_ROW_PX,
            0.6,
        );
    }
}

/// The picture rows: the filmed ground of each recorded row, then one thumbnail per row rather than one per
/// frame. §1 says the band has a picture at every zoom, and the prototype's gap between frames is its own
/// frame spacing, not something the band should reproduce as black; one representative frame per row is what
/// a shot can be compared against. Row 0's frames are the ones shown, which is the row the preview watches.
fn draw_picture_rows(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    for run in scene.runs() {
        fill(
            cr,
            (0.8, 0.8, 0.82),
            run.0 * scene.pps,
            rows.pictures,
            (run.1 - run.0) * scene.pps,
            rows.row_h * scene.row_count.max(1) as f64,
            1.0,
        );
    }
    let Some((_, paths)) = scene.thumbs.first() else {
        return;
    };
    // §1: "the band has a picture at every zoom". One frame is shown, the one nearest the second at the
    // view's left edge — `frame_near` picks it by the file name's own timestamp, and the mark sits where
    // that second lands. Frames are not decoded here: a draw must never block on a decode (§7).
    let at = (scene.origin / scene.pps).max(0.0);
    let Some(near) = frame_near(paths, at) else {
        return;
    };
    fill(
        cr,
        (0.65, 0.68, 0.72),
        path_x(&near, scene.pps),
        rows.pictures,
        2.0,
        rows.row_h,
        1.0,
    );
}

/// §27 Tracks: "each row's first audio track as a strip below it". One plate per recorded row, in the
/// lower third of that row ([`Rows::wave_top`]), inked from `cache/waves/<base>.wave` through the same
/// painter the placeholder strip uses (`cut_trim_press::paint_row_wave`) so the two surfaces cannot show
/// different answers for one recording. This is also what makes the wave strip a PRESS target: the band
/// reserves the space in [`bands_for`] and this inks it, so a y down there reads `Surface::WaveStrip` over
/// ink rather than over an empty grey row.
fn draw_row_wave_strips(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let w = BAND_WIDTH_PX as f64;
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

/// A thumbnail's x: frames are named by their second, so the file name is the position. An unparsable
/// name draws at the row's start rather than nowhere.
fn path_x(path: &std::path::Path, pps: f64) -> f64 {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.get(11..19))
        .and_then(|hhmmss| {
            let h: f64 = hhmmss.get(0..2)?.parse().ok()?;
            let m: f64 = hhmmss.get(3..5)?.parse().ok()?;
            let s: f64 = hhmmss.get(6..9)?.parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        })
        .map(|seconds| seconds * pps)
        .unwrap_or(0.0)
}

/// Amber striped edges where a recording starts and ends (§1).
fn draw_recording_edges(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let height = rows.row_h * scene.row_count.max(1) as f64;
    for run in scene.runs() {
        for x in [run.0 * scene.pps, (run.1 * scene.pps - 3.0).max(0.0)] {
            for stripe in 0..3i32 {
                fill(cr, RECORDING_EDGE, x + stripe as f64, rows.pictures, 1.0, height, 1.0);
            }
        }
    }
}

/// The two washes §1 names over the picture: the yellow of a take the join pass removed whole, and the rose
/// under any speed effect. Both are washes, so alpha, not a solid block.
fn draw_join_washes(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
    let cut_ = cut::Cut {
        segs: scene.segs.clone(),
        ..Default::default()
    };
    for (from, to) in crate::cut_play_leg::dimmed(&cut_, &scene.runs()) {
        fill(
            cr,
            JOIN_WASH,
            from * scene.pps,
            rows.pictures,
            ((to - from) * scene.pps).max(1.0),
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
        fill(
            cr,
            rose,
            from * scene.pps,
            rows.pictures,
            (to - from).max(0.0) * scene.pps,
            height,
            0.35,
        );
    }
}

/// The dashed white outline around the row a click set the preview to watch, gone when ▶ hands it back.
fn draw_watched_row(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let Some(row) = scene.watched_row else { return };
    let y = rows.pictures + row.max(0) as f64 * rows.row_h;
    // Drawn as its own rects rather than a stroked rectangle: a stroke straddles the path, so half the line
    // falls OUTSIDE the band when the outline is the row at the bottom, which is the common one-row case.
    ink(cr, WATCH_OUTLINE);
    for step in (0..scene.tape_end() as i64 * scene.pps as i64).step_by(10) {
        if step % 20 < 12 {
            cr.rectangle(step as f64, y, 10.0, 2.0);
            cr.rectangle(step as f64, y + rows.row_h - 2.0, 10.0, 2.0);
        }
    }
    let _ = cr.fill();
}

/// The selection: blue with its ends (§1's badge 27). The ✕ is a button in the page, not ink here.
fn draw_selection(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let Some((from, to)) = scene.selection else { return };
    fill(
        cr,
        SELECT_BLUE,
        from * scene.pps,
        rows.selection,
        (to - from).max(1.0) * scene.pps,
        SELECTION_BAND_PX,
        0.45,
    );
    ink(cr, SELECT_BLUE);
    for x in [from * scene.pps, to * scene.pps] {
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
fn draw_insert_bands(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let height = (rows.row_h * scene.row_count.max(1) as f64).max(rows.row_h);
    let boxes = clip_boxes_shifted(&scene.segs, &scene.shift, scene.pps);
    for b in boxes.iter().filter(|b| b.insert) {
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
fn draw_in_out_marks(cr: &cairo::Context, scene: &Scene, rows: Rows) {
    let Some((from, to)) = scene.selection else { return };
    let height = rows.pictures + rows.row_h;
    for (x, colour) in [(from * scene.pps, MARK_IN), (to * scene.pps, MARK_OUT)] {
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
