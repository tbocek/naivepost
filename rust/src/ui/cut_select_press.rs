//! F2.6 S1/S2/S3 — the selection gesture on the PAINTED band, read where it actually landed.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::cut_select::{self, Part, Surface};
use crate::timeline;
use crate::ui::{cut_folds, cut_tracks, cut_view, window as ui};

/// The band's geometry and pointer maths for one press, in one place so the widget handler and a test
/// answer with the same numbers.
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
    /// the gaps between recordings taking no width.
    pub span: timeline::Span,
}

impl Geometry {
    /// The live page's geometry: the zoom and scroll the band paints with, the row count the cut says, and
    /// the lane depth the lane paints with.
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
    /// added back), never negative.
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
        // session that has lanes is a lane, which is named ground as well.
        y_px <= self.bands.first_picture
            + self.row_count as f64 * (self.bands.row_height + self.bands.wave_height)
    }

    /// S2: which part of the band already drawn this press lands on.
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

/// What one press means, decided from the geometry and the band that was there.
#[derive(Debug, Clone, PartialEq)]
pub enum Press {
    Draws { from: f64, surface: Surface },
    Adjusts { part: Part },
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

fn row_of(geo: &Geometry, y_px: f64) -> usize {
    let pitch = geo.bands.row_height + geo.bands.wave_height;
    if pitch <= 0.0 || y_px < geo.bands.first_picture {
        return 0;
    }
    (((y_px - geo.bands.first_picture) / pitch).floor().max(0.0)) as usize
}

/// The sound scope for a surface, resolved through live state: a wave strip names the camera of the row it
/// hangs under, a lane names the separate recording stacked there.
pub fn sound_scope(geo: &Geometry, surface: Surface, cut_: &Cut, y_px: f64) -> Option<String> {
    match surface {
        Surface::WaveStrip => camera_for_row(cut_, row_of(geo, y_px)),
        // A lane press names the recording stacked at THAT y: the lanes are drawn in the order the cut
        // lists them, one row each, starting under the last camera row.
        Surface::Lane => {
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

/// Which lane row (0-based, in the cut's own order) a y below the last camera row falls in.
fn lane_row_at(geo: &Geometry, y_px: f64) -> usize {
    let row_px = crate::fx_lane::lane_height_px(1).max(1.0);
    if y_px < geo.bands.fx.0 {
        return 0;
    }
    ((y_px - geo.bands.fx.0) / row_px).floor().max(0.0) as usize
}

// The second this window's placeholder-strip drag began at, newest-slot rule like `SELECT_GESTURES` in
// `window.rs`.
thread_local! {
    static DRAG_BEGINS: std::cell::RefCell<Vec<Option<f64>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Remember where this window's drag started, in session seconds.
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

#[derive(Debug, Clone)]
struct Drag {
    from: f64,
    /// The surface chosen at begin and never re-chosen (S1: scope is what it was DRAWN on).
    surface: Surface,
    part: Option<Part>,
    start_x: f64,
}

thread_local! {
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
pub fn begin(window: &adw::ApplicationWindow, x_px: f64, y_px: f64) -> Press {
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
            // S1's scope is decided now, once.
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
            // ask for.
            if !geo.names_ground(y_px) {
                ui::clear_selection(window);
            }
            *current_drag().borrow_mut() = None;
        }
    }
    press
}

/// S1/S2 per update: draw from the remembered begin second, or move/resize the grabbed part.
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
        ui::nudge_selection(window, part, at, geo.pps);
        return;
    }
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
    cut_view::repaint_page(window);
}

/// The recording name a wave-strip or lane selection is scoped to.
pub fn recording_for_surface(
    window: &adw::ApplicationWindow,
    surface: Surface,
    y_px: f64,
) -> Option<String> {
    let cut_ = ui::newest_review_cut();
    sound_scope(&Geometry::of(window), surface, &cut_, y_px)
}

/// Attach this window's selection drag to the PAINTED band and store it for a test to fire.
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

/// The PAINTED band's selection gesture ([`attach`]), for a test that fires it the way GTK does.
pub fn band_selection_gesture_for_test(
    window: &adw::ApplicationWindow,
) -> Option<gtk::GestureDrag> {
    let _ = window;
    BAND_SELECT_GESTURES.with(|cell| cell.borrow().clone())
}
