//! F2.8 on the PAINTED band: the left button drags a clip border it lands within 6 px of (on the picture row or
//! the green bar), the right button drags a border it lands on or else moves what it pressed (S2's order). The
//! first rewrite wired these two gestures only to `track-strip`, a placeholder strip packed under the band, so on
//! the band a person sees no border could be dragged and no clip moved: a real drag there drew a selection or
//! held nothing. The rules are the strip's own (`cut_trim_press`), measured here at the band's zoom and scroll.
//!
//! A left press on a border CLAIMS the sequence at once, so the selection drag and the line click on the same
//! band stand down (a border drag must not also draw a selection). A right press claims only once it travels,
//! so an unmoved right click still reaches the card hold and F2.8 S3's "this one".

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_select::{self, Surface};
use crate::cut_trim;
use crate::ui::cut_select_press::Geometry;
use crate::ui::{cut_trim_press, cut_view};

/// One border drag in hand: which clip, which end, and the press it started from.
#[derive(Debug, Clone, Copy)]
struct BorderDrag {
    index: usize,
    border: cut_trim::Border,
    press_x: f64,
    scrubbed_ms: Option<u64>,
    pushed: bool,
}

thread_local! {
    static LEFT: std::cell::RefCell<Option<BorderDrag>> = const { std::cell::RefCell::new(None) };
    static RIGHT: std::cell::RefCell<Option<RightPress>> = const { std::cell::RefCell::new(None) };
}

/// A right press: where it landed, the border it landed on (if any), and whether it has travelled yet.
#[derive(Debug, Clone)]
struct RightPress {
    x: f64,
    y: f64,
    border: Option<(usize, cut_trim::Border)>,
    travelled: bool,
    first_trim: bool,
}

/// The kept scenes as the band draws them (`cut_tracks::band_boxes`: the gutter, the folds, the gaps between
/// recordings and the hand corrections counted), unscrolled.
fn boxes(window: &adw::ApplicationWindow) -> Vec<cut_trim::Box_> {
    let scene = cut_view::band_scene_for(window);
    crate::ui::cut_tracks::band_boxes(&scene)
        .into_iter()
        .zip(scene.segs.iter())
        .enumerate()
        .map(|(index, ((x, w), seg))| cut_trim::Box_ {
            x,
            w,
            index,
            insert: seg.is_insert(),
        })
        .collect()
}

/// The border a press at band pixel `x` grabs, if it is within reach and not an insert's (inserts are files,
/// never trimmed: rule 8).
fn border_under(
    window: &adw::ApplicationWindow,
    geo: &Geometry,
    x: f64,
) -> Option<(usize, cut_trim::Border)> {
    let boxes = boxes(window);
    let (index, border) = cut_trim::border_at(&boxes, x + geo.origin)?;
    let insert = boxes
        .iter()
        .find(|b| b.index == index)
        .is_some_and(|b| b.insert);
    (!insert).then_some((index, border))
}

/// Move the dragged border to band pixel `x`, one Undo step for the whole hold (S1), the picture scrubbing.
fn trim_to(window: &adw::ApplicationWindow, drag: &mut BorderDrag, x: f64, last: bool) {
    let geo = Geometry::of(window);
    let to = geo.second_at(x);
    if last {
        crate::ui::window_trim_strip::press_trim_border(window, drag.index, drag.border, to);
    } else {
        let now = crate::ui::window_trim_strip::now_millis();
        if !cut_trim::scrubs(drag.scrubbed_ms, now) {
            return;
        }
        let first = !drag.pushed;
        drag.scrubbed_ms = Some(now);
        drag.pushed = true;
        cut_trim_press::press_border_move(window, drag.index, drag.border, to, first);
    }
    let landed = cut_trim_press::edge_second(drag.index, drag.border);
    cut_trim_press::scrub_picture(window, landed);
    cut_view::repaint_page(window);
}

/// Attach the two gestures to `track-band`.
pub(crate) fn wire(window: &adw::ApplicationWindow) {
    let Some(band) = window
        .content()
        .and_then(|content| crate::ui::window::find_widget_by_name(&content, "track-band"))
    else {
        return;
    };

    let left = gtk::GestureDrag::new();
    left.set_button(1);
    let win = window.clone();
    left.connect_drag_begin(move |gesture, x, y| {
        let geo = Geometry::of(&win);
        let on_trim_ground = matches!(
            geo.surface_at(y),
            Surface::PictureRow(_) | Surface::SelectionBand
        );
        let taken = on_trim_ground
            .then(|| border_under(&win, &geo, x))
            .flatten();
        LEFT.with(|cell| {
            *cell.borrow_mut() = taken.map(|(index, border)| BorderDrag {
                index,
                border,
                press_x: x,
                scrubbed_ms: None,
                pushed: false,
            })
        });
        if taken.is_some() {
            // The border is the press's: the selection drag and the line click on this band stand down.
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    let win = window.clone();
    left.connect_drag_update(move |_g, dx, _dy| {
        let held = LEFT.with(|cell| *cell.borrow());
        if let Some(mut drag) = held {
            let x = drag.press_x + dx;
            trim_to(&win, &mut drag, x, false);
            LEFT.with(|cell| *cell.borrow_mut() = Some(drag));
        }
    });
    let win = window.clone();
    left.connect_drag_end(move |_g, dx, _dy| {
        if let Some(mut drag) = LEFT.with(|cell| cell.borrow_mut().take()) {
            let x = drag.press_x + dx;
            trim_to(&win, &mut drag, x, true);
        }
    });
    band.add_controller(left);

    let right = gtk::GestureDrag::new();
    right.set_button(3);
    let win = window.clone();
    right.connect_drag_begin(move |_g, x, y| {
        let geo = Geometry::of(&win);
        RIGHT.with(|cell| {
            *cell.borrow_mut() = Some(RightPress {
                x,
                y,
                border: border_under(&win, &geo, x),
                travelled: false,
                first_trim: true,
            })
        });
    });
    let win = window.clone();
    right.connect_drag_update(move |gesture, dx, _dy| {
        let Some(mut press) = RIGHT.with(|cell| cell.borrow().clone()) else {
            return;
        };
        if !press.travelled {
            if !cut_select::is_drag(dx.abs()) {
                return;
            }
            press.travelled = true;
            gesture.set_state(gtk::EventSequenceState::Claimed);
            // The hold opens only now: an unmoved right click (S3) opens nothing, so it leaves no gesture
            // standing that would refuse every later write of `cut.json`.
            cut_trim_press::begin_move();
        }
        let geo = Geometry::of(&win);
        if let Some((index, border)) = press.border {
            // S1: "either button drags that border".
            let to = geo.second_at(press.x + dx);
            cut_trim_press::press_border_move(&win, index, border, to, press.first_trim);
            press.first_trim = false;
        } else {
            // S2's order: the recorders' band, the wave strip, a selection the press falls inside, the scene.
            let surface = geo.surface_at(press.y);
            let named = crate::ui::cut_select_press::recording_for_surface(&win, surface, press.y);
            let (recorders, wave) = match surface {
                Surface::Lane => (named, None),
                Surface::WaveStrip => (None, named),
                _ => (None, None),
            };
            let press_second = geo.second_at(press.x);
            let at = press.x + geo.origin;
            let scene = boxes(&win)
                .iter()
                .find_map(|b| (at >= b.x && at <= b.x + b.w && !b.insert).then_some(b.index));
            cut_trim_press::press_move_scaled(
                &win,
                (recorders, wave, false, scene, press_second),
                dx,
                false,
                dx / geo.pps,
                geo.pps,
            );
        }
        cut_view::repaint_page(&win);
        RIGHT.with(|cell| *cell.borrow_mut() = Some(press));
    });
    let win = window.clone();
    right.connect_drag_end(move |_g, _dx, _dy| {
        let press = RIGHT.with(|cell| cell.borrow_mut().take());
        if press.is_some_and(|press| press.travelled) {
            cut_trim_press::end_move(&win);
            cut_trim_press::end_release();
            cut_view::repaint_page(&win);
        }
    });
    band.add_controller(right);
}
