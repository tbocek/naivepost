//! F3.8 Hold, move, resize, edit, remove — `spec/06-effects.md` F3.8 S1/S2/S3, the LANE side.
//!
//! The rules live in [`crate::fx_band`] and the writes behind [`crate::ui::window`]'s seams (`press_effect_bar`, `drag_held_band`). This file is the bar's gesture: it decides WHEN to ask, reads the pixels the page draws with, and forwards.
//! No threshold is restated: click-vs-pick-up, grip-vs-middle and the length floor are `fx_band`'s.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_select;
use crate::fx_band;
use crate::ui::{cut_trim_press, window as ui};

/// One drag of one bar, remembered between the toolkit's three signals. GTK gives `drag_begin` the absolute press and the others only the OFFSET, so the press position must survive for `fx_band::grab_at`.
#[derive(Debug, Clone, Copy)]
struct BarDrag {
    /// Which record this bar stands for (`fx-bar-<index>`).
    index: usize,
    /// Where inside the bar the press landed, px from its left edge.
    bar_x_px: f64,
    /// The bar's drawn width, read off the widget at the press: §A.9 states the grip (30) and ✕ (32) thresholds against it.
    width_px: f64,
    travel_px: f64,
    /// Has this gesture picked the band up yet? Until `Hold`, the seam is asked on every update (a press under the slop may still be a click).
    took_hold: bool,
    /// The band's start when this gesture picked it up: `drag-update` offsets are measured from the press, so the target is measured from there, or each tick would recount the last one's travel.
    anchor_t: f64,
}

thread_local! {
    static DRAGS: std::cell::RefCell<Option<BarDrag>> = const { std::cell::RefCell::new(None) };
    /// The window the CURRENT gesture belongs to: a closed GTK window survives and its controllers keep answering, so an old gesture must not be read against another window's cut.
    static ACTIVE: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
    /// This window's bar gestures, newest last, each tagged with the record its bar stands for.
    static GESTURES: std::cell::RefCell<Vec<(BarSlot, gtk::GestureDrag)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Is a bar drag in flight, so the whole gesture costs ONE history entry?
    static IN_FLIGHT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Has this drag already spent its single Undo step?
    static HISTORY_SPENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Attach ONE drag gesture to one effects-lane bar, from the build-once closure that creates it, so a refresh never stacks a second controller (the ✕ follows the same rule). Left button only: the right belongs to trim/move (F2.8).
pub fn wire_bar(window: &adw::ApplicationWindow, index: usize, bar: &gtk::Widget) {
    let slot = BarSlot {
        window: window.clone(),
        index,
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    let holder = bar.clone();
    let upd = slot.window.clone();
    let done = slot.window.clone();
    let tagged = slot.clone();
    gesture.connect_drag_begin(move |_g, x, _y| {
        ACTIVE.with(|cell| *cell.borrow_mut() = Some(slot.window.clone()));
        // The bar's DRAWN width is its size request (`refresh_effects_lane`, floored at 48 px); an unallocated bar reports 0 for `width()`.
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
    gesture.connect_drag_update(move |_g, dx, dy| {
        // Only the NEWEST window's gestures answer: `common::release_last_window` closes a check's window but GTK does not destroy it.
        if !ACTIVE.with(|cell| cell.borrow().as_ref().is_some_and(|live| live == &upd)) {
            return;
        }
        update(&upd, dx, dy);
    });
    gesture.connect_drag_end(move |_g, _dx, _dy| {
        if !ACTIVE.with(|cell| cell.borrow().as_ref().is_some_and(|live| live == &done)) {
            return;
        }
        end();
    });
    bar.add_controller(gesture.clone());
    GESTURES.with(|cell| cell.borrow_mut().push((tagged, gesture)));
}

/// Which record a wired gesture stands for, so a test can ask for THE gesture of one bar (every bar shares this thread-local list).
#[derive(Clone)]
struct BarSlot {
    window: adw::ApplicationWindow,
    index: usize,
}

/// S1/S2/S3 of one update: ask the seam what this press turned out to be, and once it is a hold, move the band by the pixels travelled at the lane's zoom.
fn update(window: &adw::ApplicationWindow, dx: f64, dy: f64) {
    let Some(mut drag) = DRAGS.with(|cell| *cell.borrow()) else {
        return;
    };
    drag.travel_px = (dx * dx + dy * dy).sqrt();
    if !drag.took_hold {
        // The one door that answers click-vs-pick-up: under the slop it opens the form, past it it puts the band in hand and walks the line to its start. Asked on EVERY update, so the 4 px boundary is the one the logic tests pin.
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

/// S2: carry the held band along with the pointer. Middle → the whole band slides (snapped); an end grip → that end moves. Both go through `ui::drag_held_band`.
fn move_held(window: &adw::ApplicationWindow, dx: f64, drag: BarDrag) {
    let Some(held) = crate::ui::cut_hand::held_effect() else {
        return;
    };
    let pps = ui::SELECT_SURFACE_PPS;
    if pps <= 0.0 {
        return;
    }
    let from_end = fx_band::grab_at(drag.bar_x_px, drag.width_px);
    let cut_ = crate::ui::window_preview::newest_review_cut();
    // The second under the pointer: where the band started plus the hand's travel at the lane's rate. The bar's pixels are `t * pps` with NO gutter and NO scroll (the effects lane is a box of its own, as `cut_strip_view`), unlike the painted band.
    let to = match from_end {
        // A middle grab slides: the band follows the pointer's travel from where it was at the pick-up.
        None => drag.anchor_t + dx / pps,
        // An end grab is aimed at THAT end: the record's start plus how far inside the bar the press landed, not the pointer's absolute pixel.
        Some(_) => drag.anchor_t + drag.bar_x_px / pps + dx / pps,
    };
    let session_end = cut_
        .segs
        .iter()
        .map(|seg| seg.e)
        .fold(held.t + held.dur, f64::max);
    let marks = band_snap_marks(&held);
    ui::drag_held_band(window, to, &marks, pps, session_end, from_end);
}

/// The release: the file gets its ONE write (§7's one writer; mid-drag ticks were refused), the gesture ends, and the hand keeps its hold (S2: it drops only when the line walks off the band or Esc).
fn end() {
    DRAGS.with(|cell| *cell.borrow_mut() = None);
    end_in_flight();
}

/// F3.8 S2 (`both ends offered to snap marks ... 8 px`): the landmarks a slid band lands on, from the selection drag's builder, minus the HELD effect's own two ends (a band snapping to itself cannot be dragged; `fx_band::move_whole`).
fn band_snap_marks(held: &crate::cut::Fx) -> Vec<f64> {
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let playhead = ui::preview_playhead();
    cut_select::snap_marks(&cut_, &cut_.lanes, playhead)
        .into_iter()
        .filter(|mark| (*mark - held.t).abs() > 1e-9 && (*mark - (held.t + held.dur)).abs() > 1e-9)
        .collect()
}

// --- one Undo per drag ---------------------------------------------------------------------------------------
// §7's rule (`fx_band::undo_pushed` / `cut_trim::pushes_undo`): the FIRST real move of a hold spends the history entry and later ticks ride on it. A direct call to `ui::drag_held_band` (no gesture) records every time.

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

/// Release: allow writes again and let the file have the state the hand LEFT, via the trim release's door (`end_release` = `cut_save::end_gesture()` + one `save_insert_cut`).
fn end_in_flight() {
    IN_FLIGHT.with(|cell| cell.set(false));
    HISTORY_SPENT.with(|cell| cell.set(false));
    cut_trim_press::end_release();
}

/// The gesture of THIS window's bar for record `index`, found ON the widget ([`wire_bar]), so a test can fire it the way GTK does; naming the window live keeps an earlier check's closed window from answering. Test-only.
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
