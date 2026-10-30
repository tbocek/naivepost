//! F2.4 S1/S1b — one press on the painted band, read where it actually landed.
//!
//! The rule is `crate::cut_line`'s; this module only answers the questions a widget can answer: which
//! second a pixel is at the zoom the band is drawn at, whether the pointer was in the gutter, which band
//! the y fell in, and whether anything is playing right now. Every number comes from the same doors the
//! painter uses (`cut_view::pps`, `cut_view::origin`, `cut_tracks::bands_for`,
//! `cut_tracks::clip_boxes_shifted`), so the reach, the row and the seconds agree with what is on screen
//! rather than with the placeholder strip's constants.
//!
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self, Cut};
use crate::cut_delete;
use crate::cut_line::{self, Hit};
use crate::cut_play_back;
use crate::cut_select::{self, Surface};
use crate::play_back;
use crate::ui::{self, cut_picture, cut_tracks, cut_view};

/// One press on the PLACEHOLDER strip (`select-surface`), which is not the painted band: its box holds one
/// picture row at `SELECT_SURFACE_PPS` and has no gutter of its own (F2.6 drew it that way and F2.8's
/// geometry lives in `cut_trim::PLACEHOLDER_STRIP_BANDS`). The seconds come from its own pps and the y comes
/// from the press itself, read through the same `surface_at` the painter's bands use, so a click on the
/// strip's ruler is a ruler click and not a picture-row click; everything else -- scene under the click,
/// whether anything plays, whether sources are loaded -- is read from live state by [`press`], never passed
/// as a constant.
pub fn press_strip(
    window: &adw::ApplicationWindow,
    x_px: f64,
    y_px: f64,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    // §7's translate: the strip is a WINDOW, so the second under the pointer is the widget pixel plus however
    // far the window has been scrolled — `cut_strip_view::seconds_at` is the one conversion this surface uses,
    // and it is the inverse of what the painter offsets by (see that module). Dividing by pps alone would put a
    // scrolled click on whichever clip happened to be painted there instead of the one it names.
    let at = crate::ui::cut_strip_view::seconds_at(x_px);
    // §7: `a selection is of what it was DRAWN on`. This strip's own table answers the surfaces it actually
    // draws — ruler at the top, one picture row with its wave strip, and the effects lane along the bottom — so
    // a y in this widget's 48 px box resolves against THAT split, read as drawn. `cut_tracks::bands_for`
    // measures the PAINTED band (thumbnail-tall rows, effects lane above them); asking it about a press in a
    // 48 px placeholder box sends every click below the ruler past the last row and answers "no track", which is
    // how this strip lost the ability to watch a row at all. Same rule, two surfaces, each read by its own
    // drawing: the painted band's press goes through `bands_for` in [`press`], this one through the table its
    // surface was sized for ([`crate::ui::cut_select_press::Geometry::of`] takes that same table for a drag here).
    let surface = cut_select::surface_at(y_px, &cut_select::PLACEHOLDER_BANDS, STRIP_PICTURE_ROWS);
    let on_picture = matches!(surface, Surface::PictureRow(_));
    let row = match surface {
        Surface::PictureRow(row) => row,
        _ => cut_line::PICTURE_ROW,
    };
    press_in(window, at, on_picture, row, false, now_ms)
}

/// §7 bullet 2 (`a selection is of what it was DRAWN on`) through the PAINTED band: which ground a press y
/// resolves to, read off the geometry the painter uses for this window's cut — the lane depth from its effects,
/// the row count from the session's recordings. A drag and a click cannot disagree about a y because both ask
/// it of this one table.
pub fn surface_at_band(window: &adw::ApplicationWindow, y_px: f64) -> Surface {
    let cut_ = ui::review_cut_of(window);
    let bands = cut_tracks::bands_for(crate::fx_lane::row_count(&cut_.fx));
    cut_select::surface_at(
        y_px,
        &bands,
        crate::timeline::row_count(&ui::page_recordings(), &cut_),
    )
}

/// One press on the band at `(x_px, y_px)` in the band's own pixels, at `now_ms` for the write throttle.
/// Returns what the rule decided, so a test reads the same outcome the page acted on.
pub fn press(
    window: &adw::ApplicationWindow,
    x_px: f64,
    y_px: f64,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    let cut_ = ui::review_cut_of(window);
    // The band as it is painted: a press converts through the painter's own inverse (`band_t_at`), so the
    // scroll, the folds and the gaps between recordings are counted the way the ink was laid down.
    let scene = cut_view::band_scene_for(window);
    let at = cut_tracks::band_t_at(&scene, x_px);
    // §B's gutter is the band's left strip -- the 30 px that carry the row name plates, not tape. A press
    // there sizes rows and must move no line and watch nothing (S1's parenthetical).
    let gutter = x_px < cut_view::gutter_px();

    // Which band the y fell in, off the arithmetic the painter lays out (see [`surface_at_band`]). The lane's
    // row count comes from the SAME source as its painted height: `Scene::of` measures the lane from the newest
    // review cut, so a press must too, or the rows shift under the pointer.
    let surface = surface_at_band(window, y_px);
    let on_picture = matches!(surface, Surface::PictureRow(_));
    let row = match surface {
        Surface::PictureRow(row) => row,
        _ => cut_line::PICTURE_ROW,
    };
    let outcome = press_in(window, at, on_picture, row, gutter, now_ms);

    // S1b: the wider 12 px reach, measured off the boxes the band DRAWS — shifted clips at the live zoom,
    // translated by the scroll — and only where S1b says it applies: on the picture band, clear of the green
    // (a press already on a scene's own picture is S1's click) and clear of the effects lane and the gutter,
    // which never reached the tape. `press_tries_reach` states that condition.
    if cut_line::press_tries_reach(on_picture, gutter, outcome.takes_scene) {
        pick_on_band(&cut_, &scene, x_px);
    }

    // S4's cue-and-scroll follows the position rather than being a rule of its own: the frame under the
    // line is repainted. The line lands under the hand, so the view does not move.
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
    let cut_ = ui::review_cut_of(window);
    let recordings = ui::page_recordings();

    // "the scene under the click" means its PICTURE: a kept scene covering that second, on a picture row.
    // DECISION where §1 and §11.7 differ (see `cut_line`'s module note): a row's camera is the recording
    // drawn on it, so a scene counts as under the click when it covers the second AND belongs to that row
    // (`Seg::cam` against the row's recording), which is what makes S1b's "a clip whose green sits on
    // another row" a case that needs the wider reach rather than an ordinary click.
    let on_scene_picture = on_picture && scene_on_row(&cut_, &recordings, row, at);

    let hit = Hit {
        at,
        row,
        on_picture,
        on_scene_picture,
        playing: something_playing(window),
        sources: !recordings.is_empty(),
        gutter,
    };
    crate::ui::window::place_line_press(window, hit, now_ms)
}

/// Is a kept scene's picture under this press? The scene must cover the second and sit on the clicked row:
/// rows are named `cam<N>` exactly as [`crate::timeline::kept_footage_recordings`] names them, so the row
/// number finds its camera without a second map.
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

/// Is anything playing? The transport's own flag covers a preview the app drives; the two media legs cover
/// a player process that is running whether or not the transport was told (F2.1/F2.2 started them).
fn something_playing(window: &adw::ApplicationWindow) -> bool {
    ui::preview_player(window).transport.playing
        || play_back::running()
        || cut_play_back::playing_pid().is_some()
}

/// S1b on the drawn geometry: nearest edge within [`cut_line::EDGE_REACH_PX`] → the edge; else inside a
/// box → its border; else the whole clip. Boxes are the ones the band paints ([`cut_tracks::band_boxes`]:
/// hand corrections, folds and the gutter applied) moved into on-screen pixels by subtracting the scroll
/// origin. The first rewrite measured tape pixels without the gutter against band pixels with it, so every
/// edge sat 30 px off.
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

/// Attach the line-placing gesture to the painted band. Both layers of the overlay are given one: the
/// playhead layer sets `set_can_target(false)`, but GTK still routes an event to whichever area claims
/// it, and putting the gesture on both means a press lands wherever the hand put it.
pub fn attach(window: &adw::ApplicationWindow, band: &gtk::Widget) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(1);
    let win = window.clone();
    // Where the press landed: a release that comes after the hand moved past the drag slop ended a DRAG (a
    // selection, a trim, a move), not a click. GTK does not cancel a mouse click for moving, so without this
    // the release placed the line at the drag's end and cleared the selection the drag had just drawn: no
    // real drag on the band ever kept its selection (the signal-emitting tests never sent that release).
    // GTK always sends the press before its release; a release with no press stored (only a test emitting
    // the signal alone can make one) is measured as the click it stands for.
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
        press(&win, x, y, crate::ui::window::line_now_ms());
    });
    band.add_controller(gesture.clone());
    crate::ui::window::store_band_line_gesture(gesture);
}

/// How many picture rows the placeholder strip draws: one. Its box is 48 px tall and holds a ruler, that row
/// with its wave strip, and the effects lane along the bottom (`cut_select::PLACEHOLDER_BANDS` is that split).
pub const STRIP_PICTURE_ROWS: usize = 1;
