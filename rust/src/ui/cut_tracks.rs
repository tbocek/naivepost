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

use crate::ui::cut_picture_rows as picture_rows;
/// The band's picture rows live in their own file (this one is at its size budget); `paint_at` calls them and the
/// doors below stay re-exported so every existing caller (`params.rs`, the §05/§06 checks) keeps its path.
pub use crate::ui::cut_picture_rows::{allow_frame_decoding, first_second, THUMB_BATCH};
use crate::{clock, cut, cut_screen, cut_trim, fx_lane, layout, timeline, tools};

/// The band's tints come from [`cut_colours`], catalogued by `params::cut()` under `cut.colour.*`: §6 lists
/// "colours" among the values a tuning pass may move, so no drawing helper holds one of its own any more.
use crate::ui::cut_colours::{
    BAND_GROUND, EDGE_AMBER, GROUND, GUTTER_EDGE, GUTTER_GROUND, INSERT_VIOLET, JOIN_FRAME,
    JOIN_WASH, KEPT_BORDER, KEPT_EDGE, KEPT_TINT, KEPT_TINT_ALPHA, LANE_BORDER, LANE_GROUND,
    MARK_IN_GREEN, MARK_OUT_RED, OVER_EATEN, PLAYHEAD_RGB, RULER_INK, SELECTION_BLUE,
    WATCH_OUTLINE,
};
/// §1's playhead width: "2 px line on its own layer". Its colour is [`PLAYHEAD_RGB`].
pub const PLAYHEAD_PX: f64 = 2.0;
/// §A: the ruler is 18 px, the selection band 22 px, an effects row 26 (`fx_lane::lane_height_px(1)`), a
/// picture row `thumbHt + 4`. The ruler's 16 vs 18 is the placeholder's number kept: `cut_trim::RULER_H`
/// is what F2.8's press bands resolve against, and moving it would move a press.
pub const SELECTION_BAND_PX: f64 = 22.0;
/// The dash pattern of the effects lane's lower edge (§06-effects#2: the seam that says "the lane ends here").
/// 4 px of ink, 3 px of gap: legible at the lane's own scale without reading as a dotted hairline.
pub const LANE_DASH_PX: [f64; 2] = [4.0, 3.0];
pub const PICTURE_ROW_EXTRA_PX: f64 = 4.0;
/// The wave strip's share of a camera row (§27 Tracks: "each row's first audio track as a strip below
/// it"). No §10 row gives this height, so the painter chooses it, exactly as `cut_trim_press::{LANE_H,
/// WAVE_H}` does for the placeholder strip's own rows: the lower third of the row's pitch, which leaves the
/// upper two thirds to the thumbnail and keeps every press y that meant "row N" meaning row N.
pub const ROW_WAVE_PX: f64 = (THUMB_DEFAULT_PX as f64 + PICTURE_ROW_EXTRA_PX) / 3.0;
/// A flag's triangle: 3 px of line (§1) with a pennant this wide and tall.
const MARK_LINE_PX: f64 = 3.0;
const FLAG_PX: f64 = 10.0;
/// §A's selection-band rule: the ✕ on a kept bar or on the selection appears "only when ≥50 px wide".
/// Below that the glyph would be most of the mark and there is nothing to press.
pub const CROSS_MIN_PX: f64 = 50.0;

/// One picture-row pitch at the open size: the row's own height plus the 4 px §A puts between rows. The live
/// pitch is [`rows_pitch_px`], which follows the 🖼− / 🖼+ ladder.
pub const ROWS_PITCH_PX: i32 = (THUMB_DEFAULT_PX as i32) + (PICTURE_ROW_EXTRA_PX as i32);

thread_local! {
    /// The page's thumbnail height, §A's 🖼− / 🖼+ (40..160 px): the height of every picture row's film on the
    /// band. Kept for the session, not saved (the spec stores it nowhere). The first rewrite's ladder wrote a
    /// number nothing read, so the buttons changed their own label and no picture.
    static THUMB_PX: std::cell::Cell<u32> = const { std::cell::Cell::new(THUMB_DEFAULT_PX) };
}

/// The thumbnail height the band is painted, fitted and pressed at right now.
pub fn thumb_px() -> u32 {
    THUMB_PX.with(std::cell::Cell::get)
}

/// Set the thumbnail height, kept inside §A's 40..160.
pub fn set_thumb_px(px: u32) {
    THUMB_PX.with(|cell| cell.set(px.clamp(cut_screen::THUMB_MIN, cut_screen::THUMB_MAX)));
}

/// The live picture-row pitch: the gutter rail places its switches with it, so a switch lands on the row it
/// stands for at any thumbnail size.
pub fn rows_pitch_px() -> i32 {
    thumb_px() as i32 + PICTURE_ROW_EXTRA_PX as i32
}

/// §A's tick ladder: `0.2 0.5 1 2 5 10 30 60 120 300 600 s`, the FIRST one whose spacing at this zoom is at
/// least 70 px apart — a label every 30 px would be a grey bar, and every 700 px a ruler with no marks.
/// Kept here rather than in `cut_screen` because nothing but the painter asks for it.
pub fn tick_step_seconds(pps: f64) -> f64 {
    const LADDER: [f64; 11] = [
        0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
    ];
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
    /// The row each of `recordings` is drawn on (`timeline::rows_for`, the colouring every other part of the
    /// page uses), in `recordings`' order. Empty when the builder had no cut to colour with: every recording is
    /// then on row 0. One camera's files filmed one after another share a row; they are not one row each.
    pub recording_rows: Vec<usize>,
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
    /// The marks a hand can press on the band, and what each says (`cut_band_marks`).
    pub marks: crate::ui::cut_band_marks::BandMarks,
    /// The red line's second (§1's badge 30). It lives here rather than in a second thread-local because the
    /// playhead's own layer must ask the SAME scene the band asks, or the two layers disagree about where
    /// the line is whenever the page scrolls or zooms.
    pub line: f64,
}

/// Where one second of the tape is DRAWN, in the band's own pixels: `x_of(t)`, after the 30 px gutter, unscrolled.
///
/// This is the painter's single seconds→x door, and the press side's arithmetic (`gutter + t·pps − origin`) is
/// its inverse with no fold. §B: "the gutter is the first 30 px of timeline space", so second 0 stands at x = 30;
/// the first port subtracted the gutter here, which drew the tape's first 30 px under the gutter and put every
/// mark 30 px left of where a press found it. What it adds over the multiplication is the FOLD: a folded cell
/// carries no width (§B), so a folded stretch draws nothing and its neighbours close up around the seam.
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
    span.x_of(t.min(last)) + (t - last).max(0.0) * scene.pps
}

/// The second a press at `x_px` in the band's own pixels lands on: [`band_x`] inverted, the scroll added back
/// (the tape is drawn translated by `-origin`), through the folds and the unfilmed gaps between recordings
/// (neither takes any width), and past the last filmed second extrapolated as [`band_x`] extrapolates. Every
/// press on the band asks this one door. The first rewrite measured presses as `(x - gutter - origin) / pps`:
/// the scroll with its sign reversed, and no gap or fold, so once the band was scrolled (or a session had two
/// recordings with time between them) a click put the line seconds away from where the person clicked.
pub fn band_t_at(scene: &Scene, x_px: f64) -> f64 {
    span_t_at(&band_span(scene), x_px + scene.origin)
}

/// [`timeline::Span::t_at`] past the end extrapolated at the span's zoom, the inverse of [`band_x`]'s tail;
/// a span with nothing filmed reads linearly after the gutter. `x` is unscrolled.
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

/// [`band_x`] on a span already built: the inverse of [`span_t_at`], unscrolled.
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
            band_span_px(
                scene,
                session_second(seg.s, shift),
                session_second(seg.e, shift),
            )
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
    /// The row recording `index` is drawn on (0 when the scene was built without a colouring).
    pub(crate) fn row_of(&self, index: usize) -> usize {
        self.recording_rows.get(index).copied().unwrap_or(0)
    }

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

/// The page's own scene, gathered here rather than in `window.rs`, which is past its size budget: every
/// number comes from state the window already keeps, so the band cannot disagree with the page.
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
        crate::ui::window::held_segment().as_ref(),
    );
    scene.marks.whole = crate::ui::cut_band_marks::whole_takes();
    scene
}

/// The band's laid-out height for THIS window: the same sum the painter lays its rows out by, asked of the scene
/// the page would paint — ruler + selection band + one lane row per stacked effect row + a picture row per camera
/// and per cut lane. `cut_tracks::install` sized the surfaces once, before the page had any cut; §1's band grows
/// with every row an edit adds, so anything that has to charge for the stack (the layout above it, and the snapshot
/// crop that cuts the track area out of the live page) asks THIS rather than measuring a widget that may not have
/// been re-laid-out yet.
pub fn band_height_for_window(window: &adw::ApplicationWindow) -> i32 {
    let mut scene = crate::ui::cut_view::band_scene_for(window);
    // The frames come off the door the installed band uses (`window_page_box` hands `install` this same tree), so
    // the height is asked of the scene the page will actually paint — lane rows and film rows included.
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
    tree: impl Fn() -> Option<layout::Tree> + Clone + 'static,
) -> gtk::Scrollbar {
    // Moved into the draw closure, which must be `'static`: the frames are Prepare's output, listed per
    // recording when the page asks for its scene — a folder with none draws no pictures, which is §1's
    // answer rather than black.
    // Listed on a worker and held per folder version (`cut_picture_cache`, §03-shell §1): a paint never lists.
    let thumb_for = move |base: &str| {
        tree()
            .map(|tree| crate::ui::cut_picture_cache::frames_of(&tree, base))
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
        band_height_px(&with_thumbs(), thumb_px()).ceil() as i32,
        // The MINIMUM, not the width: `build` makes the band expand, so this is only what it never shrinks
        // under (a window that has never been laid out, which is every headless test).
        BAND_WIDTH_PX,
    );
    attach(&band.surface, &band.playhead, &band.layers, with_thumbs);
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
    band_height_px(scene, thumb_px()).ceil() as i32
}

/// The band's height when NOTHING has been cut yet and nothing has been fitted to a scene: what [`build`]
/// starts the surfaces at, and the number §1's "opened with the upper part 380 px tall" has to leave room for
/// below the divider. It is the same sum the painter lays out, asked of an empty scene — an empty lane is
/// still one row deep (`fx_lane::row_count`), and §1's band costs a row per camera (an unknown camera count
/// is one row, `Scene::row_count.max(1)`) plus a row per cut lane (none before a cut) — so the layout and the
/// ink cannot disagree about where the tape begins. Before this was computed the same way in
/// `cut_page_layout`, the two drifted and the tape was starved of the rows it asks for at draw time.
pub fn idle_band_height() -> i32 {
    band_height_for(&idle_scene(1))
}

/// The scene the band costs when nothing has been cut: `count` rows of footage (a page that has not been told
/// what was filmed still draws one row, the same `row_count.max(1)` the painter uses), no clips, no effects,
/// no lanes. `cut_page_layout` prices the bottom pane from it, and a caller that knows how many rows this
/// session filmed asks for that many — a two-camera session costs two picture rows, each carrying its wave
/// strip, and the pane must make room for both or the second row is clipped off the surface.
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
) {
    let for_band = scene.clone();
    // Cloned in: a draw callback is `'static` and so cannot borrow the caller's widgets.
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
            // Read at every draw: the 🖼 ladder changes it while the band stands.
            thumb_px(),
        );
    });
    let for_head = scene.clone();
    playhead.set_draw_func(move |_, cr, _width, height| {
        let held = for_head();
        paint_playhead(cr, playhead_x(&held), height.max(1) as f64);
    });
}

/// The named surfaces of the band, stacked: §1 puts the tracks under the toolbar and the red line over both,
/// so they are two `DrawingArea`s inside one overlay — the band, which repaints on every edit, and the
/// playhead, which repaints on every tick. Built here because `window.rs` is past its size budget.
pub struct Band {
    pub surface: gtk::DrawingArea,
    pub playhead: gtk::DrawingArea,
    pub layers: gtk::Overlay,
    /// §B's gutter rail, IN the band: the whole-cut `fold-all-button` goes here (`ui::wire_fold_all`).
    pub gutter_badges: gtk::Box,
    /// The same rail one row lower per lane/row control: `gutter-switch-<lane>` and `row-cross-<n>`.
    pub gutter_switches: gtk::Box,
}

/// Build the two surfaces with the names §1's inventory uses. Height is the band's own arithmetic; the
/// overlay matches it so the red line really crosses both bands.
pub fn build(height: i32, width: i32) -> Band {
    let surface = gtk::DrawingArea::new();
    surface.set_widget_name("track-band");
    // Repainted when the picture row's worker lands a batch of decoded thumbnails.
    picture_rows::note_band(&surface);
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
    // §B's gutter: the 30 px column at the band's left edge is a RAIL the page's controls sit on, not a
    // band of its own, so its two holders are overlay children of this same stack — painted over the
    // inked column `draw_gutter` lays down, and inside `track-layers` so every Cut-page refresh finds them
    // by name the way it finds `fold-badges`. `gutter-badges` takes the whole-cut fold badge (§1's badge 23);
    // `gutter-switches` takes one sound switch per lane and the empty-row ✕ (§1's badge 29), each pushed
    // down to its own row by `cut_cameras::refresh_camera_rows`.
    let rail = || {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.set_can_target(false);
        column.set_halign(gtk::Align::Start);
        column.set_valign(gtk::Align::Start);
        column.set_margin_start(2);
        // Below the ruler: §B puts the gutter's controls in the selection band and the rows under it.
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
    // §1's per-row sound switches, each at its own row's sound strip. A `Fixed` of their own, because the
    // rail above stacks its children, so a switch placed in it moves every control below it.
    let gutter_rows = gtk::Fixed::new();
    gutter_rows.set_widget_name("gutter-row-switches");
    gutter_rows.set_halign(gtk::Align::Start);
    gutter_rows.set_valign(gtk::Align::Start);
    gutter_rows.add_css_class("cut-gutter-col");
    layers.add_overlay(&gutter_rows);
    // §B's rail is dark ink, so its controls must not arrive wearing Adwaita's light button plate: over the
    // gutter each one is a flat glyph on the rail, which is how `spec/img/05-cut.png` draws badges 23/29.
    for column in [&gutter_badges, &gutter_switches] {
        column.add_css_class("cut-gutter-col");
    }
    install_gutter_css_once();
    // The band draws its own marks and answers presses on them (`cut_band_marks`), as §1 and the prototype
    // have it. These holders keep the named controls the page's doors and refreshes find, unseen: shown, they
    // were grey button plates over the bars and the gutter.
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

/// The gutter rail's own CSS, installed once per process like `install_toolbar_css_once`: buttons on the
/// rail are flat (no frame, no background) and sized to the 30 px column, because §B gives the gutter 30 px
/// of TIMELINE space — a default button is wider than the rail it lives on.
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

/// This window's gutter rail, by name, scoped to its own content — the `fold_badges_box` rule: a closed
/// window's tree survives and a name search from the window can land in it, so search from `content`.
fn gutter_column(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Box> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(content.upcast_ref(), name)?
        .downcast::<gtk::Box>()
        .ok()
}

/// The rail that holds this window's whole-cut fold badge: the band's `gutter-badges` when the page drew
/// a band (§B's seat), else the older `fold-all-holder` so a page without a band still gets its badge.
pub fn gutter_badges_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    gutter_column(window, "gutter-badges").or_else(|| crate::ui::cut_folds::fold_badges_box(window))
}

/// The rail this window's lane switches and row crosses live on, or None when the page has no band.
pub fn gutter_switches_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    gutter_column(window, "gutter-switches")
}

/// The layer this window's per-row sound switches sit in, or None when the page has no band.
pub fn gutter_row_switches(window: &adw::ApplicationWindow) -> Option<gtk::Fixed> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(content.upcast_ref(), "gutter-row-switches")?
        .downcast::<gtk::Fixed>()
        .ok()
}

/// Where camera row `row`'s sound strip begins on this window's band, from the same rows the painter lays out,
/// so a switch placed there sits on the strip it stands for.
pub fn row_wave_top(window: &adw::ApplicationWindow, row: usize) -> f64 {
    rows(&crate::ui::cut_view::band_scene_for(window), thumb_px()).wave_top(row)
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

/// How near the bottom edge of a picture row a press takes the thumbnail-size grip (§1 badge 2), either side.
pub const ROW_GRIP_PX: f64 = 3.0;

/// The thumbnail-size grip under band height `y` (§1 badge 2): the bottom edge of a picture row, camera or cut
/// lane, within [`ROW_GRIP_PX`]. Answers how many rows sit above that edge, itself included, since a drag there
/// grows every one of them and the edge must stay under the hand.
pub fn row_grip_at(scene: &Scene, y: f64) -> Option<usize> {
    let rows = rows_for_press(scene);
    let count = rows.camera_rows(scene) + lane_rows(scene);
    (1..=count)
        .find(|above| (y - (rows.pictures + *above as f64 * rows.row_h)).abs() <= ROW_GRIP_PX)
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
        // Past the gutter: §1's gutter is a rail down the whole track stack, so a lane row that fills from x = 0
        // paints over it and its sound switch reads as sitting on the lane.
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
        // Badge 2's plate: the name WITH its channel layout, from `Scene::lane_plates`.
        let plate = scene
            .lane_plates
            .get(index)
            .map(String::as_str)
            .unwrap_or(lane.name.as_str());
        text_at(cr, left + 2.0, top + 10.0, plate);
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
    // The tape is drawn scrolled: translated by `-origin`, the scroll the scrollbar, a zoom and a restored line
    // set, and the same one a press (`cut_line_press::press`) and the playhead subtract. The band used to paint
    // from 0:00 whatever the scroll, so a scrolled page moved its red line and its clicks while the tape stood
    // still, and the picture rows (which skip what lies left of the scroll) drew nothing at all.
    // The tape's painted width: the surface's own, so the ruler's ground, the wave strips and the lane rows
    // reach the page's right edge instead of stopping at a constant; in the tape's own (unscrolled) pixels its
    // right edge is the scroll plus that width.
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
    // Pinned: the gutter, its marks and the row plates stay at the band's left edge however far the tape is
    // scrolled.
    draw_gutter(cr, rows, height);
    crate::ui::cut_band_marks::paint_pinned(cr, scene, rows);
    draw_name_plates(cr, scene, rows);
}

/// §B's gutter: the first 30 px of the band's own space, inked from the ruler's bottom edge down so the rail
/// under the switches reads as a rail. The ruler keeps its own ground on purpose: `draw_ruler` prints its
/// first label at x = 2, and inking over it would erase the tape's first tick label.
///
/// It is painted LAST (see [`paint_at`]) because several painters fill from x = 0 across the whole tape —
/// the row wave strips reach the surface's full width by design — and any of them drawn after this would
/// paint back over the rail. That is also why `draw_picture_rows` clips its ground to the rail's edge: the
/// row wash is painted before this one, so it needs no clip, but a reader who moves the call must keep the
/// order or the gutter silently disappears into the row colour.
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

/// F2.9 S2: the pieces a sound paste laid over the kept footage, on the PAINTED band as well as on the
/// window's placeholder strip. Same painter ([`crate::ui::cut_trim_press::paint_laid_pieces_in`]), this
/// caller's row geometry, drawn last but for the name plates so the plate sits on the footage it was laid
/// over. Without this the lay is invisible on the surface the Cut page actually shows.
fn draw_laid_pieces(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    // That painter lays seconds out at `t · pps`; the tape starts after the gutter.
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

/// The placeholder's open-at thumbnail height, used wherever a caller has not said otherwise.
pub const THUMB_DEFAULT_PX: u32 = cut_screen::THUMB_AT_OPEN;

/// The band's layout: the row tops every painter shares. Kept in one place because the effects lane's height
/// feeds the picture rows' start, and a second arithmetic would be a second answer.
#[derive(Debug, Clone, Copy)]
pub struct Rows {
    /// The ruler's own height at the top of the band (`cut_trim::RULER_H`). Public so the layout that seats
    /// the pane above can charge the band for the ruler it paints, as it charges it for the lane and the rows.
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
    let held = rows_of(fx_rows, thumb_px());
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

/// The rows a press on the live band is measured against: the band is painted at [`thumb_px`].
pub(crate) fn rows_for_press(scene: &Scene) -> Rows {
    rows(scene, thumb_px())
}

fn rows(scene: &Scene, thumb_px: u32) -> Rows {
    rows_of(fx_lane::row_count(&scene.fx), thumb_px)
}

fn rows_of(fx_rows: usize, thumb_px: u32) -> Rows {
    let selection = cut_trim::RULER_H;
    // §06-effects#2: the lane begins under the selection band and its rows stack top to bottom; those two
    // placements are `cut_band_rows`' sums, so the painter and anything reading the band agree on them.
    let effects = crate::ui::cut_band_rows::lane_top_px(cut_trim::RULER_H, SELECTION_BAND_PX);
    let pictures = crate::ui::cut_band_rows::pictures_top_px(effects, fx_rows as f64);
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
    // Each tick at its second's PAINTED x (after the gutter, through the folds), so the ruler reads the tape
    // under it; a tick inside a fold lands on the seam and is dropped rather than stacked there.
    let mut last = f64::NEG_INFINITY;
    let marks: Vec<(f64, String)> = cut_trim::ruler_ticks(scene.tape_end(), scene.pps, step)
        .into_iter()
        .filter_map(|(x, label)| {
            let painted = band_x(scene, x / scene.pps);
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

/// The tick labels, at the size the ruler's height allows.
fn labels(cr: &cairo::Context, ticks: &[(f64, String)]) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(9.0);
    ink(cr, RULER_INK);
    for (x, label) in ticks {
        text_at(cr, x + 2.0, cut_trim::RULER_H - 4.0, label);
    }
}

/// The selection band's ground across each filmed run (the prototype's `bandGround`): the bars sit on it, the
/// dropped gaps are what is left of it, and unfilmed time, which is never laid out, has none.
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

/// Green bar per kept scene (§1's badge 25), the prototype's way (`go/cut_selband.go`): translucent, the scene the
/// badges are about (held, else under the line) a little fuller and taller than the rest, and a light grip at
/// each end where a press takes it. The boxes come from [`band_boxes`], i.e. through the folded span. The ✕ is
/// `cut_band_marks`', which also answers the press on it.
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

/// The kept scenes over the pictures (`go/cut.go` 4154-4193): each scene's stretch of ITS row tinted green, its
/// wave strip more faintly, and a green line down either side, so what the cut keeps reads off the pictures as
/// well as off the bars.
fn draw_kept_tint(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - ROW_WAVE_PX).max(0.0);
    for (index, (x, w)) in band_boxes(scene).into_iter().enumerate() {
        let seg = &scene.segs[index];
        if seg.is_insert() || x >= tape || x + w < 0.0 {
            continue;
        }
        let top = rows.pictures + seg.cam.max(0) as f64 * rows.row_h;
        fill(cr, KEPT_TINT, x, top, w, film, KEPT_TINT_ALPHA);
        fill(cr, KEPT_TINT, x, top + film, w, ROW_WAVE_PX, 0.16);
        for edge in [x, x + w] {
            fill(cr, KEPT_EDGE, edge - 1.0, top, 2.0, rows.row_h, 1.0);
        }
    }
}

/// The effects lane: one row per overlapping group, each bar in its kind's colour.
fn draw_effects_lane(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    // A bar under the span floor reserves no row (§2), so it shares one with a bar that does; painted after
    // the row's owners it stays visible instead of vanishing under the bar that covers its seconds.
    let mut bars = fx_lane::lane_layout(&scene.fx);
    bars.sort_by_key(|bar| !bar.reserves_a_row);
    for bar in bars {
        let (from, to) = scene.fx[bar.index].spans();
        let y = crate::ui::cut_band_rows::lane_row_top_px(rows.effects, bar.row);
        let (x0, width) = band_span_px(scene, from, to);
        if x0 >= tape || x0 + width < 0.0 {
            continue;
        }
        // The lane's ink is the SAME painter the Cut page's own bars wear (`cut_lane_paint`), not a second wash
        // of this file's own: §A.8 gives every kind something to print on its bar, and a band that only tints
        // cannot show it. Clipped to the bar's painted span so a bar running off the surface paints nothing past
        // the tape.
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
    // §2's lower edge: "the lane … sits between the selection band above and the picture rows below", and
    // `spec/img/06-lane.png` draws that seam DASHED — a solid rule would read as yet another lane; the dashes say
    // "the lane ends here". It sits on the lane's LAST pixel row (`rows.pictures - 1`), not on `rows.pictures`:
    // that row belongs to the picture cells, and §05's decode gate samples it, so an inked line there reads as
    // decoded frame pixels. A stroke is a box of the line width centred on its coordinates, so `- 0.5` at width 1
    // covers exactly one pixel row; the pattern is cleared after or cairo hands it to every later stroke.
    ink(cr, LANE_BORDER);
    cr.set_line_width(1.0);
    cr.set_dash(&LANE_DASH_PX, 0.0);
    cr.move_to(cut_screen::GUTTER_PX, rows.pictures - 0.5);
    cr.line_to(tape, rows.pictures - 0.5);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
}

/// Amber striped edges where a recording starts and ends (§1), on its own row's pictures: a solid line on the
/// edge and diagonals 5 px apart leaning into the recording (`go/cut.go` `srcEdgeMark`). The far end only where the
/// recording is drawn wider than its two marks.
fn draw_recording_edges(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - ROW_WAVE_PX).max(1.0);
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

/// One striped edge at `x`, leaning `dir` (+1 into a start, -1 into an end), at most 9 px or half the room.
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

/// The two washes §1 names over the picture. The yellow lies over a recording the join pass took out entire
/// (`retakes.tsv`'s `whole` column, F1.10): "flagged for a look, not an error", so a wash and a frame round its
/// pictures (`go/cut.go` `wholeTakeMark`). The first port washed every dropped stretch yellow, which put a bright
/// block over each hole in the cut. The rose lies under a SPEED effect and nothing else.
fn draw_join_washes(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let film = (rows.row_h - ROW_WAVE_PX).max(1.0);
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

/// The dashed white outline around the row a click set the preview to watch, gone when ▶ hands it back.
fn draw_watched_row(cr: &cairo::Context, scene: &Scene, rows: Rows, tape: f64) {
    let Some(row) = scene.watched_row else { return };
    let y = rows.pictures + row.max(0) as f64 * rows.row_h;
    // Drawn as its own rects rather than a stroked rectangle: a stroke straddles the path, so half the line
    // falls OUTSIDE the band when the outline is the row at the bottom, which is the common one-row case.
    ink(cr, WATCH_OUTLINE);
    // The dashes run the TAPE's width, cut at its right end: the row a click set the preview to watch is
    // outlined wherever there is tape to outline, not across empty ground past the last filmed second.
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

/// The selection: blue in the band with its ends, and a blue wash over the pictures it spans (§1's badge 27,
/// `spec/img/05-cut.png`). Its ✕ is `cut_band_marks`'.
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
    let Some((from, to)) = scene.selection else {
        return;
    };
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
    // Past the gutter, not at x = 2: §1 puts the plate ON the row ("pinned name plate per row"), while the gutter's
    // first 30 px carry the fold-all badge and the sound switches. Inked at 2 the name ran under the rail and read as
    // clipped letters — the dark rail is painted AFTER this pass, so it ate whatever landed inside its width.
    // Each line on a light backing of its own: dark ink straight on the thumbnails read as clipped letters
    // wherever a dark frame sat under the first ones ("ession-tape" over a dark-blue frame). And one line
    // under the other: both had been inked at the same y, so a plate with a correction printed over itself.
    // One plate per ROW (§1's "pinned name plate per row"), naming the recording the row shows at the view's left
    // edge (the first on the row still in view at the scroll), so a scrolled band names what is under the plate.
    // One plate per recording at `index × row height` put a camera's second file a row lower than the tape it is
    // on, and the rest of a ten-file session off the band, so only the first file was ever named.
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

/// One name-plate line's height: the 9 px font and the backing's margin.
const PLATE_LINE_PX: f64 = 11.0;

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

/// Where the red line stands on the band's surface: its second through the same span the tape is painted by
/// ([`band_x`]: after the gutter, through the folds), less the scroll. The line used to be `t · pps − origin`,
/// which put it left of the picture it names by the gutter and by every fold before it.
pub fn playhead_x(scene: &Scene) -> f64 {
    band_x(scene, scene.line) - scene.origin
}

/// The red line, on its OWN surface: 2 px across the whole height, at `x` ([`playhead_x`]).
pub fn paint_playhead(cr: &cairo::Context, x: f64, height: f64) {
    cr.set_source_rgb(PLAYHEAD_RGB.0, PLAYHEAD_RGB.1, PLAYHEAD_RGB.2);
    cr.rectangle(x, 0.0, PLAYHEAD_PX, height);
    let _ = cr.fill();
}

/// The frames the band may show for one recording: Prepare's extracted JPEGs on the grid, oldest first.
/// Listed once per install (§1's Pictures) and cached in the scene, so a draw never touches the disk; a
/// missing folder answers empty, which draws no pictures rather than inventing black (§1's prototype note).
pub fn frame_paths(tree: &layout::Tree, base: &str) -> Vec<std::path::PathBuf> {
    let dir = tree.frames_dir(base);
    crate::ui::cut_picture_cache::note_listed_here();
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                // The frame's STAMP is the rule (§B.3: `‹YYYY-MM-DD_HH-MM-SS.mmm›.jpg`, and
                // `frame_near` compares names byte-wise), not one encoder's suffix. Prepare writes
                // JPEG, so `.jpg` is what a real session holds; accepting PNG as well costs nothing
                // and lets anything that renders a frame — the snapshot bench, a test fixture — plant
                // a decodable file under its proper stamp instead of mislabelling PNG bytes `.jpg`.
                .filter(|path| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| name.ends_with(".jpg") || name.ends_with(".png"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    // Two files can claim the same second — a `.jpg` and a `.png` of one stamp, or a stand-in left beside a newer
    // staged frame. The row draws ONE cell per second it shows, so a duplicate would paint over its own neighbour.
    // Newest file wins: that is the frame whatever wrote it last intended, and Prepare's own extractor rewrites in
    // place rather than adding a second copy.
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

/// Which extracted frame is nearest a second: the last one at or before it, falling back to the first
/// when the view starts ahead of every frame. `clock::frame_name` stamps the files, so comparing the
/// formatted stamp against a floor is what makes "nearest" mean "not from the future".
pub fn frame_near(paths: &[std::path::PathBuf], at: f64) -> Option<std::path::PathBuf> {
    // The last frame whose instant is at or before `at`, so a shown frame is never one that has not happened yet.
    // Compared as instants (`clock::read_frame`, both name shapes), not as file names: a name carries `.jpg`
    // after its stamp, so `…48.000.jpg` sorted after a wanted `…48.000` and was passed over, and the wanted
    // second was floored, so every lookup landed a frame early.
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
