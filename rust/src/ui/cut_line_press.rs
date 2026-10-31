//! F2.4 S1/S1b — one press on the painted band, read where it actually landed.
//!
//! The rule is `crate::cut_line`'s; this module answers what a widget can: which second a pixel is at, whether the pointer was in the gutter, which band the y fell in, and whether anything plays.
//! Every number comes from the painter's own doors (`cut_view::pps`, `cut_view::origin`, `cut_tracks::bands_for`, `cut_tracks::clip_boxes_shifted`), so reach, row and seconds agree with what is on screen.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self, Cut};
use crate::cut_delete;
use crate::cut_line::{self, Hit};
use crate::cut_play_back;
use crate::cut_select::{self, Surface};
use crate::play_back;
use crate::ui::{self, cut_picture, cut_tracks, cut_view};

/// One press on the PLACEHOLDER strip (`select-surface`), which is not the painted band: one picture row at `SELECT_SURFACE_PPS` and no gutter (geometry in `cut_trim::PLACEHOLDER_STRIP_BANDS`).
/// The y is read through `surface_at`, so a click on the strip's ruler is a ruler click; scene, playing state and loaded sources are read from live state by [`press`].
pub fn press_strip(
    window: &adw::ApplicationWindow,
    x_px: f64,
    y_px: f64,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    // §7's translate: the strip is a WINDOW, so the second under the pointer is the pixel plus the scroll (`cut_strip_view::seconds_at`, the inverse of the painter's offset);
    // dividing by pps alone would put a scrolled click on the wrong clip.
    let at = crate::ui::cut_strip_view::seconds_at(x_px);
    // §7: `a selection is of what it was DRAWN on`. This strip draws a ruler, one picture row with its wave strip, and the effects lane along the bottom, so a y in its 48 px box resolves against THAT split.
    // `cut_tracks::bands_for` measures the PAINTED band and would send every click below the ruler past the last row ("no track"). The painted band's press goes through `bands_for` in [`press`]; this one through
    // the table its surface was sized for ([`crate::ui::cut_select_press::Geometry::of`] takes the same table for a drag here).
    let surface = cut_select::surface_at(y_px, &cut_select::PLACEHOLDER_BANDS, STRIP_PICTURE_ROWS);
    let on_picture = matches!(surface, Surface::PictureRow(_));
    let row = match surface {
        Surface::PictureRow(row) => row,
        _ => cut_line::PICTURE_ROW,
    };
    press_in(window, at, on_picture, row, false, now_ms)
}

/// §7 bullet 2 (`a selection is of what it was DRAWN on`) through the PAINTED band: which ground a press y resolves to, off the painter's geometry (lane depth from the effects, row count from the recordings).
/// A drag and a click both ask this one table.
pub fn surface_at_band(y_px: f64) -> Surface {
    let cut_ = ui::window_preview::newest_review_cut();
    let bands = cut_tracks::bands_for(crate::fx_lane::row_count(&cut_.fx));
    cut_select::surface_at(
        y_px,
        &bands,
        crate::timeline::row_count(&ui::page_recordings(), &cut_),
    )
}

/// One press on the band at `(x_px, y_px)` in the band's own pixels, at `now_ms` for the write throttle; returns the rule's outcome so a test reads what the page acted on.
pub fn press(
    window: &adw::ApplicationWindow,
    x_px: f64,
    y_px: f64,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    let cut_ = ui::window_preview::newest_review_cut();
    // The band as painted: a press converts through the painter's inverse (`band_t_at`), counting scroll, folds and gaps as the ink was laid down.
    let scene = cut_view::band_scene_for();
    let at = cut_tracks::band_t_at(&scene, x_px);
    // §B's gutter is the band's left 30 px (row name plates, not tape): a press there sizes rows and moves no line and watches nothing (S1).
    let gutter = x_px < cut_view::gutter_px();

    // Which band the y fell in (see [`surface_at_band`]). The lane's row count comes from the same source as its painted height (`Scene::of`, the newest review cut), or rows shift under the pointer.
    let surface = surface_at_band(y_px);
    let on_picture = matches!(surface, Surface::PictureRow(_));
    let row = match surface {
        Surface::PictureRow(row) => row,
        _ => cut_line::PICTURE_ROW,
    };
    let outcome = press_in(window, at, on_picture, row, gutter, now_ms);

    // S1b: the wider 12 px reach, measured off the boxes the band DRAWS (shifted clips at the live zoom, translated by the scroll), only where [`cut_line::press_tries_reach`] says.
    if cut_line::press_tries_reach(on_picture, gutter, outcome.takes_scene) {
        pick_on_band(&cut_, &scene, x_px);
    }

    // S4's cue-and-scroll follows the position; the line lands under the hand, so the view does not move.
    if outcome.line_moved {
        cut_picture::note_line_moved(window, outcome.line_at);
        cut_view::repaint_page(window);
    }
    // F2.10 S3: a click that watches a row says what the cut shows there, as the row's own plate did.
    if let Some(row) = outcome.watches {
        crate::ui::cut_cameras::press_watch_row(window, row as i32);
    }
    outcome
}

/// The part both surfaces share: decide from where the pointer landed and apply the rule.
fn press_in(
    window: &adw::ApplicationWindow,
    at: f64,
    on_picture: bool,
    row: usize,
    gutter: bool,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    let cut_ = ui::window_preview::newest_review_cut();
    let recordings = ui::page_recordings();

    // "The scene under the click" means its PICTURE: a kept scene covering that second on a picture row. DECISION where §1 and §11.7 differ (see `cut_line`'s module note):
    // a scene counts when it covers the second AND belongs to that row (`Seg::cam` against the row's recording), so S1b's "clip whose green sits on another row" needs the wider reach.
    let on_scene_picture = on_picture && scene_on_row(&cut_, &recordings, row, at);

    let hit = Hit {
        at,
        row,
        on_picture,
        on_scene_picture,
        playing: something_playing(),
        sources: !recordings.is_empty(),
        gutter,
    };
    crate::ui::window::place_line_press(window, hit, now_ms)
}

/// Is a kept scene's picture under this press? It must cover the second and sit on the clicked row (rows are named `cam<N>` as [`crate::timeline::kept_footage_recordings`] names them).
fn scene_on_row(
    cut_: &Cut,
    recordings: &[crate::timeline::Recording],
    row: usize,
    at: f64,
) -> bool {
    let Some(cam) = recordings
        .get(row)
        .and_then(|recording| recording.base.strip_prefix("cam"))
        .and_then(|tail| tail.parse::<i32>().ok())
    else {
        // No recording drawn on that row: there is no picture there to take in hand.
        return false;
    };
    cut_delete::scene_under(&cut_.segs, at).is_some_and(|seg| seg.cam == cam)
}

/// Is anything playing? The transport's own flag covers a preview the app drives; the two media legs cover a player process the transport was not told about (F2.1/F2.2).
fn something_playing() -> bool {
    ui::preview_player().transport.playing
        || play_back::running()
        || cut_play_back::playing_pid().is_some()
}

/// S1b on the drawn geometry: nearest edge within [`cut_line::EDGE_REACH_PX`] → the edge; else inside a box → its border; else the whole clip. Boxes are the ones the band paints
/// ([`cut_tracks::band_boxes`]), moved into on-screen pixels by subtracting the scroll origin (tape pixels without the gutter against band pixels with it put every edge 30 px off).
fn pick_on_band(cut_: &cut::Cut, scene: &cut_tracks::Scene, x_px: f64) {
    let boxes = cut_tracks::band_boxes(scene);
    let mut nearest: Option<(f64, usize)> = None;
    for (index, (x, w)) in boxes.iter().enumerate() {
        let left = x - scene.origin;
        let right = left + w;
        let edge_px = (x_px - left).abs().min((x_px - right).abs());
        let better = nearest.map(|(best, _)| edge_px < best).unwrap_or(true);
        if better {
            nearest = Some((edge_px, index));
        }
    }
    let Some((edge_px, index)) = nearest else {
        return;
    };
    let (x, w) = boxes[index];
    let left = x - scene.origin;
    let inside = x_px >= left && x_px <= left + w;
    let pick = cut_line::first_press_pick(edge_px, inside);
    ui::apply_pick(pick, cut_.segs.get(index).cloned());
}

/// Attach the line-placing gesture to the painted band. Both overlay layers get one: the playhead layer sets `set_can_target(false)`, but GTK still routes an event to whichever area claims it.
pub fn attach(window: &adw::ApplicationWindow, band: &gtk::Widget) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(1);
    let win = window.clone();
    // Where the press landed: a release after the hand moved past the drag slop ended a DRAG (selection, trim, move), not a click; without this the release placed the line and cleared the selection just drawn.
    // A release with no stored press (only a test emitting the signal alone) is measured as the click it stands for.
    let pressed_at: std::rc::Rc<std::cell::Cell<Option<(f64, f64)>>> = Default::default();
    let at_press = pressed_at.clone();
    gesture.connect_pressed(move |_g, _n_press, x, y| at_press.set(Some((x, y))));
    gesture.connect_released(move |_g, _n_press, x, y| {
        if let Some((px, py)) = pressed_at.take() {
            if crate::cut_select::is_drag((x - px).abs())
                || crate::cut_select::is_drag((y - py).abs())
            {
                return;
            }
        }
        // A mark on the band (a ✕, a fold badge, a sound switch, a lens) takes the press; the line stays.
        if crate::ui::cut_band_marks::press_at(&win, x, y) {
            return;
        }
        press(&win, x, y, ui::window_line_step::now_ms());
    });
    band.add_controller(gesture.clone());
    crate::ui::window::store_band_line_gesture(gesture);
}

/// How many picture rows the placeholder strip draws: one (its 48 px box holds a ruler, that row with its wave strip, and the effects lane; `cut_select::PLACEHOLDER_BANDS`).
pub const STRIP_PICTURE_ROWS: usize = 1;
