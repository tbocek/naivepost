//! F3.8 Hold, move, resize, edit, remove — `spec/06-effects.md` F3.8 S1/S2/S3, the LANE side.
//!
//! Every rule of working with a band already lives in [`crate::fx_band`] and every write already lives behind
//! [`crate::ui::window`]'s seams (`press_effect_bar`, `drag_held_band`). What was missing is the bar's own
//! gesture: without it no press on a lane band ever reached those seams, so the rules could only be fired by a
//! test. This file is that gesture and nothing else — it decides WHEN to ask, reads the pixels the page draws
//! with, and forwards. No threshold is restated here: click-vs-pick-up, grip-vs-middle and the length floor are
//! answered by `fx_band` through the seams, exactly as they are when a test calls them directly.
//!
//! Split out of `window.rs`, which is past its size budget.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_select;
use crate::fx_band;
use crate::shell;
use crate::ui::{cut_trim_press, window as ui};

/// One drag of one bar, remembered between the toolkit's three signals. GTK gives `drag_begin` the absolute
/// press and `drag_update`/`drag_end` only the OFFSET from it, so the press position has to survive the begin
/// for `fx_band::grab_at` (which end was grabbed) to be asked against the same pixel all the way through.
#[derive(Debug, Clone, Copy)]
struct BarDrag {
    /// Which record this bar stands for (`fx-bar-<index>`).
    index: usize,
    /// Where inside the bar the press landed, px from its left edge.
    bar_x_px: f64,
    /// The bar's drawn width, read off the widget at the press: §A.9 states the grip (30) and ✕ (32)
    /// thresholds against THAT number, not against the record's seconds.
    width_px: f64,
    travel_px: f64,
    /// Has this gesture picked the band up yet? A press under the slop is still possibly a click, so the seam
    /// is asked on every update until it answers `Hold`.
    took_hold: bool,
    /// The band's start when this gesture picked it up: a `drag-update` offset is measured from the press, so
    /// the target must be measured from where the band was THEN, not from wherever the last tick left it —
    /// otherwise tick two counts tick one's travel again and the band runs away from the pointer.
    anchor_t: f64,
}

thread_local! {
    static DRAGS: std::cell::RefCell<Option<BarDrag>> = const { std::cell::RefCell::new(None) };
    /// The window the CURRENT gesture belongs to, newest slot: a closed GTK window survives and its controllers
    /// keep answering, so a test firing an old gesture must not have its press read against another window's cut.
    static ACTIVE: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
    /// This window's bar gestures, newest last, each tagged with the record its bar stands for.
    static GESTURES: std::cell::RefCell<Vec<(BarSlot, gtk::GestureDrag)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Is a bar drag in flight, so the whole gesture must cost ONE history entry rather than one per tick?
    static IN_FLIGHT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Has this drag already spent its single Undo step?
    static HISTORY_SPENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Attach ONE drag gesture to one effects-lane bar. Called once per bar widget, from the same build-once
/// closure that creates it, so a refresh never stacks a second controller on the same box (the ✕ follows the
/// identical rule). Left button only: the right button belongs to trim/move (F2.8).
pub fn wire_bar(window: &adw::ApplicationWindow, index: usize, bar: &gtk::Widget) {
    let slot = BarSlot {
        window: window.clone(),
        index,
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    let holder = bar.clone();
    gesture.connect_drag_begin(move |_g, x, _y| {
        // The bar's DRAWN width is its size request: `refresh_effects_lane` sets it from the record's own
        // length floored at 48 px, and an unallocated bar reports 0 for `width()`.
        let width_px = f64::from(holder.size_request().0).max(1.0);
        DRAGS.with(|cell| {
            *cell.borrow_mut() = Some(BarDrag {
                index: slot.index,
                bar_x_px: x,
                width_px,
                travel_px: 0.0,
                took_hold: false,
                anchor_t: 0.0,
            })
        });
        HISTORY_SPENT.with(|cell| cell.set(false));
        begin_in_flight();
    });
    let upd = slot.window.clone();
    gesture.connect_drag_update(move |_g, dx, dy| {
        // Only the NEWEST window's gestures answer: `common::release_last_window` closes a check's window but
        // GTK does not destroy it, so every earlier window's bars still hold live controllers.
        if !ACTIVE.with(|cell| cell.borrow().as_ref().is_some_and(|live| live == &upd)) {
            return;
        }
        update(&upd, dx, dy);
    });
    let done = slot.window.clone();
    gesture.connect_drag_end(move |_g, _dx, _dy| {
        if !ACTIVE.with(|cell| cell.borrow().as_ref().is_some_and(|live| live == &done)) {
            return;
        }
        end(&done);
    });
    bar.add_controller(gesture.clone());
    GESTURES.with(|cell| cell.borrow_mut().push((slot, gesture)));
}

/// Which record a wired gesture stands for, kept beside it so a test can ask for THE gesture of one bar rather
/// than whichever was attached last (every bar shares this thread-local list).
#[derive(Clone)]
struct BarSlot {
    window: adw::ApplicationWindow,
    index: usize,
}

/// S1/S2/S3 of one update: ask the seam what this press turned out to be, and once it is a hold, move the band
/// by the pixels the hand travelled at the zoom the lane is drawn at.
fn update(window: &adw::ApplicationWindow, dx: f64, dy: f64) {
    let Some(mut drag) = DRAGS.with(|cell| *cell.borrow()) else {
        return;
    };
    drag.travel_px = (dx * dx + dy * dy).sqrt();
    if !drag.took_hold {
        // The one door that answers click-vs-pick-up: under the slop it opens the form, past it it puts the
        // band in hand, walks the line to its start and prints "<label> picked up". Asking it on EVERY update
        // is what makes the 4 px boundary the same boundary the logic tests pin.
        let answer = ui::press_effect_bar(
            window,
            drag.index,
            drag.bar_x_px,
            drag.width_px,
            drag.travel_px,
            false,
        );
        let fx_band::Press::Hold { t, .. } = answer else {
            DRAGS.with(|cell| *cell.borrow_mut() = Some(drag));
            return;
        };
        drag.took_hold = true;
        // The pick-up tells us where the band started; every later tick measures its travel from there.
        drag.anchor_t = t;
    }
    DRAGS.with(|cell| *cell.borrow_mut() = Some(drag));
    move_held(window, dx, drag);
}

/// S2: carry the held band along with the pointer. Middle → the whole band slides (snapped); an end grip →
/// that end moves. Both go through `ui::drag_held_band`, which owns the refind-by-kind-and-start, the line
/// riding with the band, and the "<label> moved"/"<label> resized" sentences.
fn move_held(window: &adw::ApplicationWindow, dx: f64, drag: BarDrag) {
    let Some(held) = ui::held_effect() else {
        return;
    };
    let pps = ui::SELECT_SURFACE_PPS;
    if pps <= 0.0 {
        return;
    }
    let from_end = fx_band::grab_at(drag.bar_x_px, drag.width_px);
    let cut_ = ui::review_cut_of(window);
    // The second under the pointer: where the band started, plus how far the hand travelled at the rate the
    // lane is drawn at. The bar's own pixels are `t * pps` with NO gutter and NO scroll — the effects lane is
    // a box of its own whose children start at its left edge (`cut_strip_view` states the same for the strip),
    // unlike the painted band, whose press must subtract both.
    let to = match from_end {
        // A middle grab slides: the band follows the pointer's travel from where it was at the pick-up.
        None => drag.anchor_t + dx / pps,
        // An end grab is aimed at THAT end, so the second under the hand is the bar's own left edge (the
        // record's start) plus how far inside the bar the press landed — not the pointer's absolute pixel.
        Some(_) => drag.anchor_t + drag.bar_x_px / pps + dx / pps,
    };
    let session_end = shell::cut_seconds(&cut_).max(held.t + held.dur);
    let marks = band_snap_marks(window, &held);
    ui::drag_held_band(window, to, &marks, pps, session_end, from_end);
}

/// The release: the file gets its ONE write (§7's one writer — every mid-drag tick was refused at the writer),
/// the gesture ends, and the hand keeps whatever it was holding (S2: the hold drops only when the line walks
/// off the band or Esc says so, never because the pointer was released).
fn end(window: &adw::ApplicationWindow) {
    DRAGS.with(|cell| *cell.borrow_mut() = None);
    end_in_flight(window);
}

/// F3.8 S2 (`both ends offered to snap marks ... 8 px`): the landmarks a slid band lands on. Same builder the
/// selection drag uses — segment ends, lane edges, every other effect's ends, the red line — with the HELD
/// effect's own two ends removed, because a band that snaps to itself cannot be dragged at all once it lines
/// up with its own start (`fx_band::move_whole`'s reason for asking the caller to filter).
fn band_snap_marks(window: &adw::ApplicationWindow, held: &crate::cut::Fx) -> Vec<f64> {
    let cut_ = ui::review_cut_of(window);
    let playhead = ui::preview_playhead(window);
    cut_select::snap_marks(&cut_, &cut_.lanes, playhead)
        .into_iter()
        .filter(|mark| (*mark - held.t).abs() > 1e-9 && (*mark - (held.t + held.dur)).abs() > 1e-9)
        .collect()
}

// --- one Undo per drag ---------------------------------------------------------------------------------------
//
// §7's rule, stated once in `fx_band::undo_pushed` / `cut_trim::pushes_undo`: the FIRST real move of a hold
// spends the history entry and every later tick rides on it. A mouse-move loop would otherwise spend one
// entry per pixel. These two doors are what `ui::drag_held_band` asks; a direct call to that seam (no gesture
// in flight) keeps recording every time, which is what its own tests pin.

/// Is a bar drag in flight? Read by `ui::drag_held_band`.
pub fn band_drag_in_flight() -> bool {
    IN_FLIGHT.with(|cell| cell.get())
}

/// Does THIS tick take the drag's single Undo entry? True the first time, false after.
pub fn take_history_step() -> bool {
    HISTORY_SPENT.with(|cell| {
        if cell.get() {
            return false;
        }
        cell.set(true);
        true
    })
}

/// Begin of a gesture: `cut.json` waits for the release, and the history starts unspent.
fn begin_in_flight() {
    crate::cut_save::begin_gesture();
    IN_FLIGHT.with(|cell| cell.set(true));
    HISTORY_SPENT.with(|cell| cell.set(false));
}

/// Release: allow writes again and let the file have the state the hand LEFT, through the same door the trim
/// release uses (`end_release` = `cut_save::end_gesture()` + one `save_insert_cut`).
fn end_in_flight(window: &adw::ApplicationWindow) {
    IN_FLIGHT.with(|cell| cell.set(false));
    HISTORY_SPENT.with(|cell| cell.set(false));
    let _ = window;
    cut_trim_press::end_release();
}

/// The gesture of THIS window's bar for record `index`, found ON the widget ([`wire_bar]), so a test can fire
/// it the way GTK does. Naming this window live on the way through is what keeps an earlier check's closed
/// window (still in the tree, controllers still attached) from answering the next check's drag.
/// Test-only reader: the page attaches gestures and never asks for one back.
pub fn bar_gesture_for_test(
    window: &adw::ApplicationWindow,
    index: usize,
) -> Option<gtk::GestureDrag> {
    ACTIVE.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    GESTURES.with(|cell| {
        cell.borrow()
            .iter()
            .rev()
            .find(|(slot, _)| slot.index == index && slot.window == *window)
            .map(|(_, gesture)| gesture.clone())
    })
}
