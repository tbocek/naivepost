//! F2.6 S1/S2/S3 — the selection gesture on the PAINTED band, read where it actually landed.
//!
//! F2.6's rule is `crate::cut_select`'s alone (which scope a ground gives, which part a press grabs, how
//! far a snapped end may go). What lives here is only what a widget can answer: which second a pixel is at
//! the zoom the band is drawn at, which band the y fell in, which recording a sound scope names, and
//! whether the hand is on the band that is already there. The geometry comes from the same doors the
//! painter and `cut_line_press` use (`cut_view::pps`/`origin`/`gutter_px`, `cut_tracks::bands_for`,
//! `timeline::row_count`), so a drag's seconds agree with the ink under the pointer instead of with the
//! placeholder strip's constants.
//!
//! Split out of `window.rs`, which is past its size budget.
//!
//! # What each spec clause costs here
//!
//! - **S1** draws from the second AT DRAG-BEGIN to the second at the pointer — never from 0. Under
//!   `cut_select::DRAG_SLOP_PX` the press is a click and this module does NOTHING: F2.4's line press owns
//!   a click on the same widget, and two claims on one press is how a click both places a line and draws a
//!   band. The scope is decided once at begin and kept (`keep_started_surface`), so dragging across a
//!   picture row after pressing on a wave strip does not silently change what the band is *of*.
//! - **S1's effects lane** is the one ground that refuses: a press there puts a held effect down
//!   (`ui::press_fx_lane`) and draws no band, at BEGIN, once — a drag across the lane must not put the
//!   same effect down twelve times.
//! - **S2** routes an update by `cut_select::part_at`: an end resizes and the middle moves, both through
//!   `ui::nudge_selection`, which rebuilds the 8 px snap marks from the live cut every call. A press clear
//!   of the band (`Part::Outside`) starts a new one, which is S1's job.
//! - **S3**'s marks follow because every change ends in `ui::refresh_selection_readout`, which now also
//!   queues a repaint of the band — the painter's in/out flags and blue band read `Scene::selection`,
//!   which is this window's band, so nothing here paints anything itself.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::cut_select::{self, Part, Surface};
use crate::timeline;
use crate::ui::{cut_folds, cut_tracks, cut_view, window as ui};

/// The band's geometry and pointer maths for one press, in one place so the widget handler and a test
/// answer with the same numbers. Nothing here decides a scope or a snap — that is `cut_select`.
#[derive(Debug, Clone)]
pub struct Geometry {
    /// The band's pixels-per-second (`cut_view::strip_pps` on the placeholder strip, `cut_view::pps` on
    /// the painted band — each surface is measured at the zoom it is actually drawn at).
    pub pps: f64,
    /// Scroll into the timeline, px (`cut_view::origin`).
    pub origin: f64,
    /// The §B gutter: the left strip of name plates, which is not tape.
    pub gutter: f64,
    /// The press bands the painter's own row arithmetic produces (`cut_tracks::bands_for`).
    pub bands: crate::cut_select::SurfaceBands,
    /// Rows the page draws, so `surface_at` knows where the last row ends.
    pub row_count: usize,
    /// The tape as the painter lays it out (`cut_tracks::band_span`): the filmed runs at this zoom, folds and
    /// the gaps between recordings taking no width. Seconds and pixels are converted through it.
    pub span: timeline::Span,
}

impl Geometry {
    /// The live page's geometry: the zoom and scroll the band paints with, the row count the cut says, and
    /// the lane depth the lane paints with. `Scene::of` measures the effects lane from the newest review
    /// cut, so a press must too, or the rows shift under the pointer.
    pub fn of(window: &adw::ApplicationWindow) -> Self {
        let cut_ = ui::newest_review_cut();
        let scene = cut_view::band_scene_for(window);
        Geometry {
            pps: scene.pps,
            origin: scene.origin,
            gutter: cut_view::gutter_px(),
            bands: cut_tracks::bands_for(crate::fx_lane::row_count(&cut_.fx)),
            row_count: timeline::row_count(&cut_folds::page_recordings(), &cut_),
            span: cut_tracks::band_span(&scene),
        }
    }

    /// The session second a press x is at: the painter's own inverse (`cut_tracks::span_t_at`, the scroll
    /// added back), never negative. The same door `cut_line_press::press` asks, which is the point: a line
    /// click and a band drag must call the same pixel the same second.
    pub fn second_at(&self, x_px: f64) -> f64 {
        cut_tracks::span_t_at(&self.span, x_px + self.origin)
    }

    /// A band's seconds as pixels inside the band, which is the space `part_at` measures grips in.
    pub fn px_of(&self, seconds: f64) -> f64 {
        cut_tracks::span_x_of(&self.span, seconds) - self.origin
    }

    /// Which ground a press y was drawn on.
    pub fn surface_at(&self, y_px: f64) -> Surface {
        cut_select::surface_at(y_px, &self.bands, self.row_count)
    }

    /// Whether a y is on ground the band can name (a row, the ruler, a lane or the selection band) rather
    /// than blank gutter outside the rows — S1's "the click lands on ground the band cannot name".
    pub fn names_ground(&self, y_px: f64) -> bool {
        if self.row_count == 0 || y_px < 0.0 {
            return false;
        }
        // Named ground runs from the ruler at the top of the band through the last row's bottom; the
        // selection band and the effects lane above the rows name ground too, and a y below the rows on a
        // session that has lanes is a lane, which is named ground as well. The bound is the whole PITCH
        // (picture + wave strip), not the picture half alone: with a split row a wave-strip press would
        // otherwise read as unnamed ground and a click there would clear the selection instead of leaving
        // F2.4's line click to place the line.
        y_px <= self.bands.first_picture
            + self.row_count as f64 * (self.bands.row_height + self.bands.wave_height)
    }

    /// S2: which part of the band already drawn this press lands on. `Outside` when there is no band.
    ///
    /// The reach comes from the surface the PRESS is on (§I's "selection ends on the pictures within 10 px"),
    /// not from one constant: the same pixel offset grabs an end on a picture row and misses it in the
    /// selection band, which is what `cut_select::grip_px` decides. A caller that has no surface to hand (the
    /// older two-argument call sites) gets [`GRIP_PX`](cut_select::GRIP_PX), the narrow grip, i.e. exactly the
    /// rule this method had before §6's third reach existed.
    pub fn part_of(&self, x_px: f64, band: &cut_select::Selection) -> Part {
        self.part_of_on(x_px, band, Surface::SelectionBand)
    }

    /// [`Self::part_of`] with the press's surface named, so the reach is the one that surface earns.
    pub fn part_of_on(&self, x_px: f64, band: &cut_select::Selection, surface: Surface) -> Part {
        cut_select::part_at_reach(
            x_px,
            self.px_of(band.start),
            self.px_of(band.end),
            cut_select::grip_px(surface),
        )
    }
}

/// What one press means, decided from the geometry and the band that was there. Pure: it names the work
/// without touching a slot, so the decision is testable without a window.
#[derive(Debug, Clone, PartialEq)]
pub enum Press {
    /// A new drag from `from`, scoped to `scope`. A `Surface::FxLane` scope is the one refusal: put a held
    /// effect down and draw no band.
    Draws { from: f64, surface: Surface },
    /// The hand is on the band that is already there: adjust THAT part.
    Adjusts { part: Part },
    /// Under the drag slop: a click, which F2.4's line press owns. This module does nothing at all.
    Click,
}

/// S1/S2: what the press at `(x_px, y_px)` means, given the band (if any) that was on screen when the hand
/// went down.
pub fn press_at(
    geo: &Geometry,
    x_px: f64,
    y_px: f64,
    held_band: Option<&cut_select::Selection>,
) -> Press {
    // S1's one refusal: the effects lane never takes a selection, whatever the y next says.
    let surface = geo.surface_at(y_px);
    if surface == Surface::FxLane {
        return Press::Draws {
            from: geo.second_at(x_px),
            surface,
        };
    }
    // S2 before S1: a hand that lands on the band is adjusting it, not drawing over it.
    if let Some(band) = held_band {
        let part = geo.part_of_on(x_px, band, surface);
        if part != Part::Outside {
            return Press::Adjusts { part };
        }
    }
    Press::Draws {
        from: geo.second_at(x_px),
        surface,
    }
}

/// S1: which camera the wave strip under row `row` belongs to — §B's `cam<N>` spelling, the same one
/// `timeline::kept_footage_recordings` writes and a scene's `quiet` list stores, taken from the cut's own
/// footage cameras in row order (the order the page stacks rows).
pub fn camera_for_row(cut_: &Cut, row: usize) -> Option<String> {
    let mut cams: Vec<i32> = cut_
        .segs
        .iter()
        .filter(|seg| !seg.is_insert() && seg.e > seg.s)
        .map(|seg| seg.cam)
        .collect();
    cams.sort_unstable();
    cams.dedup();
    cams.get(row).map(|cam| format!("cam{cam}"))
}

/// The row index a y falls in on this band, however the surface answer named it.
fn row_of(geo: &Geometry, y_px: f64) -> usize {
    let pitch = geo.bands.row_height + geo.bands.wave_height;
    if pitch <= 0.0 || y_px < geo.bands.first_picture {
        return 0;
    }
    (((y_px - geo.bands.first_picture) / pitch).floor().max(0.0)) as usize
}

/// The sound scope for a surface, resolved through live state: a wave strip names the camera of the row it
/// hangs under, a lane names the separate recording stacked there. `None` for ground that is neither.
pub fn sound_scope(geo: &Geometry, surface: Surface, cut_: &Cut, y_px: f64) -> Option<String> {
    match surface {
        Surface::WaveStrip => camera_for_row(cut_, row_of(geo, y_px)),
        // A lane press names the recording stacked at THAT y: the lanes are drawn in the order the cut
        // lists them, one row each, starting under the last camera row. Naming only the first would report
        // one recording's sound however many rows the session has.
        Surface::Lane => {
            // `lanes.len()` may be 0 (a session with nothing laid down still draws the lane box): saturate
            // rather than subtract, or a lane press on an empty lane crashes the page.
            let index = cut_
                .lanes
                .len()
                .saturating_sub(1)
                .min(lane_row_at(geo, y_px));
            cut_.lanes.get(index).map(|lane| lane.name.clone())
        }
        _ => None,
    }
}

/// Which lane row (0-based, in the cut's own order) a y below the last camera row falls in. The rows are
/// `fx_lane::lane_height_px`-sized and stacked from `bands.fx.0`, which is where the painter puts them.
fn lane_row_at(geo: &Geometry, y_px: f64) -> usize {
    let row_px = crate::fx_lane::lane_height_px(1).max(1.0);
    if y_px < geo.bands.fx.0 {
        return 0;
    }
    ((y_px - geo.bands.fx.0) / row_px).floor().max(0.0) as usize
}

// The second this window's placeholder-strip drag began at, newest-slot rule like `SELECT_GESTURES` in
// `window.rs`. Lives here because the only reader is `window.rs`'s own strip handler, and `window.rs` is
// past its size budget.
thread_local! {
    static DRAG_BEGINS: std::cell::RefCell<Vec<Option<f64>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Remember where this window's drag started, in session seconds. The whole Vec is swapped rather than
/// borrowed in place: a handler that reads [`drag_begin`] later would meet a live `borrow_mut` here and
/// panic with "RefCell already borrowed" inside a gesture callback.
pub(crate) fn set_drag_begin(at: f64) {
    DRAG_BEGINS.with(|slots| {
        let mut slots = slots.borrow_mut();
        if let Some(slot) = slots.last_mut() {
            *slot = Some(at);
        } else {
            slots.push(Some(at));
        }
    });
}

/// The second this window's drag began at, `None` before any press.
pub(crate) fn drag_begin() -> Option<f64> {
    DRAG_BEGINS.with(|slots| slots.borrow().last().copied().flatten())
}

/// What one drag does, per event, against the page. The three handlers below call exactly these.
#[derive(Debug, Clone)]
struct Drag {
    /// The second the hand went down on — S1's start, kept for the whole drag.
    from: f64,
    /// The surface chosen at begin and never re-chosen (S1: scope is what it was DRAWN on).
    surface: Surface,
    /// The part grabbed at begin, if this was an adjustment. `None` for a fresh draw.
    part: Option<Part>,
    /// The x at begin, for the slop test.
    start_x: f64,
}

thread_local! {
    /// The drag in flight on this window's band, newest-window rule as everywhere else here.
    static DRAGS: std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<Option<Drag>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn current_drag() -> std::rc::Rc<std::cell::RefCell<Option<Drag>>> {
    DRAGS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            return slot.clone();
        }
        let fresh = std::rc::Rc::new(std::cell::RefCell::new(None));
        slots.borrow_mut().push(fresh.clone());
        fresh
    })
}

/// The drag in flight, if any — read out of its cell by value (`Drag` is `Clone`), because borrowing the
/// cell while a handler runs would outlive the borrow the caller needs.
fn held_drag() -> Option<Drag> {
    current_drag().borrow().clone()
}

/// S1/S2 at drag-begin: decide the scope or the grabbed part, and handle the effects lane there and once.
/// Returns what it decided so a test can read the same answer the page acted on.
pub fn begin(window: &adw::ApplicationWindow, x_px: f64, y_px: f64) -> Press {
    // A press on one of the band's marks (a ✕, a fold badge, a sound switch, a lens) is that mark's, answered on
    // release by `cut_band_marks::press_at`: it starts no selection and clears none.
    let scene = crate::ui::cut_view::band_scene_for(window);
    let rows = crate::ui::cut_tracks::rows_for_press(&scene);
    if crate::ui::cut_band_marks::mark_at(&scene, rows, x_px, y_px).is_some() {
        *current_drag().borrow_mut() = None;
        return Press::Click;
    }
    let geo = Geometry::of(window);
    let held = ui::selection(window);
    let press = press_at(&geo, x_px, y_px, held.as_ref());
    match &press {
        Press::Draws { from, surface } => {
            if *surface == Surface::FxLane {
                // The one ground that refuses a selection: put a held effect down, draw no band, and leave
                // the band that was there exactly as it was.
                ui::press_fx_lane(window);
                *current_drag().borrow_mut() = None;
                return press;
            }
            *current_drag().borrow_mut() = Some(Drag {
                from: *from,
                surface: *surface,
                part: None,
                start_x: x_px,
            });
            // S1's scope is decided now, once. Nothing is drawn yet: a press that turns out to be a click
            // must not have replaced the band.
        }
        Press::Adjusts { part } => {
            *current_drag().borrow_mut() = Some(Drag {
                from: held.map(|band| band.start).unwrap_or(0.0),
                surface: ui::selection_surface(window),
                part: Some(*part),
                start_x: x_px,
            });
        }
        Press::Click => {
            // F2.6 S1: a click on ground the band cannot name (the blank gutter at its left, everything
            // outside it) selects nothing, said here rather than silently drawing a band the user did not
            // ask for. Everything else keeps F2.4's click — it places the line — and draws no selection.
            if !geo.names_ground(y_px) {
                ui::clear_selection(window);
            }
            *current_drag().borrow_mut() = None;
        }
    }
    press
}

/// S1/S2 per update: draw from the remembered begin second, or move/resize the grabbed part. Under the
/// slop nothing has happened yet — the press may still turn out to be F2.4's click.
pub fn update(window: &adw::ApplicationWindow, x_px: f64, y_px: f64) {
    let Some(drag) = held_drag() else {
        return;
    };
    if !cut_select::is_drag((x_px - drag.start_x).abs()) {
        return;
    }
    let geo = Geometry::of(window);
    let at = geo.second_at(x_px);
    if let Some(part) = drag.part {
        // S2: ends resize, middle moves; `nudge_selection` rebuilds the snap marks for THIS call, so a
        // border an edit added mid-drag is a snap target the moment it exists.
        ui::nudge_selection(window, part, at, geo.pps);
        return;
    }
    // S1: from the second the hand landed on, to the second under it now — the scope from begin, the
    // recording named off live state, and `keep_started_surface` holding the ground even when the pointer
    // wanders into another band mid-drag.
    let surface = cut_select::keep_started_surface(Some(drag.surface), geo.surface_at(y_px));
    let cut_ = ui::newest_review_cut();
    let recording = sound_scope(&geo, surface, &cut_, y_px);
    ui::draw_selection(window, surface, recording.as_deref(), drag.from, at);
}

/// End of the drag: the band the last update left is the answer, so there is nothing to commit — this
/// drops the drag so the next press decides afresh, and a press that never passed the slop has still
/// drawn nothing.
pub fn end(window: &adw::ApplicationWindow) {
    *current_drag().borrow_mut() = None;
    // The readout already followed every update; queue the band once more so the in/out flags land on the
    // second the hand stopped on even if the last update was inside a snap's reach.
    cut_view::repaint_page(window);
}

/// The recording name a wave-strip or lane selection is scoped to. `cut_select::draw` takes `Option<&str>`
/// and refuses (returns `None`) for a sound scope with no name, so the page must name one: the camera whose
/// row the strip hangs under (or, on a lane, the cut's own separate recording), named off this window's
/// live cut through the same door the painted band uses — never a stand-in literal, which would report a
/// sound the session may not have.
pub fn recording_for_surface(
    window: &adw::ApplicationWindow,
    surface: Surface,
    y_px: f64,
) -> Option<String> {
    let cut_ = ui::newest_review_cut();
    sound_scope(&Geometry::of(window), surface, &cut_, y_px)
}

/// Attach this window's selection drag to the PAINTED band and store it for a test to fire. Left button
/// only: the right button belongs to trim/move (F2.8), and a click on the same widget is F2.4's line press
/// (`cut_line_press::attach`), which this module deliberately does not consume.
pub fn attach(window: &adw::ApplicationWindow) {
    let Some(content) = window.content() else {
        return;
    };
    let Some(found) = crate::ui::window::find_widget_by_name(&content, "track-band") else {
        return;
    };
    let Ok(band) = found.downcast::<gtk::DrawingArea>() else {
        return;
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    let win = window.clone();
    // GTK hands `drag-update` the OFFSET from the press, not the pointer's place: the press point is kept so
    // each update is asked at where the hand is now, in the band's own pixels.
    let pressed: std::rc::Rc<std::cell::Cell<(f64, f64)>> = Default::default();
    let at_press = pressed.clone();
    gesture.connect_drag_begin(move |_g, x, y| {
        at_press.set((x, y));
        begin(&win, x, y);
    });
    let upd = window.clone();
    gesture.connect_drag_update(move |_g, dx, dy| {
        let (x, y) = pressed.get();
        update(&upd, x + dx, y + dy);
    });
    let done = window.clone();
    gesture.connect_drag_end(move |_g, _x, _y| {
        end(&done);
    });
    band.add_controller(gesture.clone());
    BAND_SELECT_GESTURES.with(|cell| *cell.borrow_mut() = Some(gesture));
}

thread_local! {
    /// This window's band gesture, kept here because `attach` is the only thing that writes it and a test
    /// firing it is the only thing that reads it back.
    static BAND_SELECT_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
}

/// The PAINTED band's selection gesture ([`attach`]), for a test that fires it the way GTK does. Test-only
/// reader: the page attaches the gesture and never asks for it back, so a name saying so keeps the program
/// path honest about what calls this.
pub fn band_selection_gesture_for_test(
    window: &adw::ApplicationWindow,
) -> Option<gtk::GestureDrag> {
    let _ = window;
    BAND_SELECT_GESTURES.with(|cell| cell.borrow().clone())
}
