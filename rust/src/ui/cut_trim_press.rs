//! F2.8 S1/S2/S3 — the Cut page's trim-and-move PRESS half. `cut_trim` holds the RULES; this holds what only
//! the page can supply: the strip's real bands and the name a press in each resolves to, what gets persisted
//! and pushed to history, the folds a drag opens and refolds, and the live re-seek that scrubs the picture.

use crate::cut::Cut;
use crate::cut_trim::{self, Band, StripBands};
use crate::timeline::Recording;
use crate::ui::cut_page_state::{CutPage, PAGE as CUT_PAGE};
use crate::ui::window;
use adw::prelude::*;

/// F2.8 S2: the recorders' band's height: one lane row, short because it carries a plate, not pictures.
pub const LANE_H: f64 = 18.0;
/// F2.8 S1/S2: the wave strip's height (§27 Tracks: "each row's first audio track as a strip below it").
pub const WAVE_H: f64 = 18.0;

/// The strip's TOTAL height when every row is drawn. `window.rs` sizes the drawing area with this, so a
/// band a press can resolve always has room to be painted in.
pub const STRIP_H: f64 = cut_trim::RULER_H + cut_trim::BAR_H + LANE_H + WAVE_H;

/// This strip's bands RIGHT NOW: what the painter draws and what a press resolves against. The kept bar
/// keeps its old span; the two new rows sit under it, each only when there is something to press in it.
pub fn bands(cut_: &Cut, filmed: &[Recording]) -> StripBands {
    let bar_top = cut_trim::RULER_H;
    let bar_bottom = bar_top + cut_trim::BAR_H;
    if cut_.lanes.is_empty() && filmed.is_empty() {
        return cut_trim::PLACEHOLDER_STRIP_BANDS;
    }
    // A missing row collapses to zero height, so `band_at` can never answer a band the painter did not draw.
    let lane_top = bar_bottom;
    let lane_bottom = if cut_.lanes.is_empty() {
        lane_top
    } else {
        lane_top + LANE_H
    };
    let wave_top = lane_bottom;
    let wave_bottom = if filmed.is_empty() {
        wave_top
    } else {
        wave_top + WAVE_H
    };
    StripBands {
        recorders: (lane_top, lane_bottom),
        // `band_at` tests wave before bar, so a collapsed wave span can never steal the bar's row.
        wave: (wave_top, wave_bottom),
        bar: (bar_top, bar_bottom),
    }
}

/// F2.8 S2: does the press's second lie inside the band's seconds? A band that sits nowhere near the pointer
/// takes nothing.
pub fn press_falls_inside(band: Option<&crate::cut_select::Selection>, at_seconds: f64) -> bool {
    band.is_some_and(|band| band.start <= at_seconds && at_seconds <= band.end)
}

/// The lane painted under x on the recorders' band: its span is `at .. at + dur`, as [`paint_bands`] draws it.
fn lane_at_x(cut_: &Cut, x: f64, pps: f64) -> Option<String> {
    let at = x / pps;
    cut_.lanes
        .iter()
        .find(|lane| lane.at <= at && at < lane.at + lane.dur)
        .map(|lane| lane.name.clone())
}

/// The recording whose filmed run holds the press second, the wave strip's answer.
fn recording_at_x(filmed: &[Recording], x: f64, pps: f64) -> Option<String> {
    let at = x / pps;
    filmed
        .iter()
        .find(|rec| rec.start <= at && at < rec.end)
        .map(|rec| rec.base.clone())
}

/// The single door from a press position to the flowchart's three inputs, read off the painted geometry:
/// `(recorders_band, wave_strip, on_border)`. `window.rs::right_press_took` forwards here.
///
/// A name can only come from a PAINTED plate; where a row was not drawn, `seam` keeps the caller's answer
/// for that row only.
pub fn press_targets_at(
    cut_: &Cut,
    x: f64,
    y: f64,
    boxes: &[cut_trim::Box_],
    pps: f64,
    seam: Option<&str>,
) -> (Option<String>, Option<String>, bool) {
    let on_border = cut_trim::border_at(boxes, x).is_some();
    let filmed = filmed_rows(cut_);
    // Where a row is absent (collapsed, or the placeholder layout) there is nothing to read a name off, so
    // `seam` stands in for THAT row only. A row that WAS drawn answers only from the plate under x.
    let drawn = painted_bands(cut_, &filmed);
    let no_lane_row = cut_.lanes.is_empty();
    match cut_trim::band_at(y, &drawn) {
        Band::Recorders => (
            lane_at_x(cut_, x, pps)
                .or_else(|| no_lane_row.then(|| seam.map(str::to_string)).flatten()),
            None,
            on_border,
        ),
        Band::Wave => (None, wave_answer(cut_, &filmed, x, pps, seam), on_border),
        Band::Ruler | Band::Bar => (None, None, on_border),
    }
}

/// The strip's bands as a PRESS must read them: like [`bands`] except that a press may only resolve a row that
/// was PAINTED (recorders' band per lane, wave strip per recording). With neither, the strip drew ONE row and
/// the placeholder layout makes the press land on the bar (flowchart M3/M4/M5).
fn painted_bands(cut_: &Cut, filmed: &[Recording]) -> StripBands {
    if cut_.lanes.is_empty() || filmed.is_empty() {
        return cut_trim::PLACEHOLDER_STRIP_BANDS;
    }
    bands(cut_, filmed)
}

/// Is the wave strip's recording read from a plate at all? Only when the cut has lanes; otherwise the
/// placeholder's wave span shares the bar's row, so there is no plate to name and the seam stands in.
fn wave_plate_exists(cut_: &Cut, filmed: &[Recording]) -> bool {
    !filmed.is_empty() && !cut_.lanes.is_empty()
}

/// The wave strip's answer for this press: the RECORDING whose plate lies under x if one was painted,
/// else the caller's seam (keeps flowchart M2 reachable on a one-row strip).
fn wave_answer(
    cut_: &Cut,
    filmed: &[Recording],
    x: f64,
    pps: f64,
    seam: Option<&str>,
) -> Option<String> {
    if wave_plate_exists(cut_, filmed) {
        return recording_at_x(filmed, x, pps);
    }
    seam.map(str::to_string)
}

/// The recordings the wave strip is painted over: ONLY what the page was handed (`page_recordings`), not the
/// cut's kept footage; a page nobody handed a session list drew no wave plates.
pub fn filmed_rows(_cut_: &Cut) -> Vec<Recording> {
    crate::ui::cut_folds::page_recordings()
}

/// S2's questions in the order the flowchart asks them.
pub type MoveScope = (
    Option<String>, // the recorders' band's lane, if the press landed on one
    Option<String>, // the wave strip's recording, if it landed on one
    bool,           // whether it landed on a drawn clip border (S1 claims a border first)
    Option<usize>,  // the scene under it, if none of the above did
    f64,            // where the press fell, in seconds: S2's "a selection the press falls inside"
);

/// F2.8 S2/S3: the move rule; the scope bundle is `(recorders' band, wave strip, on a border, scene, press
/// second)`. Both doors land here: `press_move_at` and the seam-only `press_move`.
pub fn press_move_with_override(
    window: &adw::ApplicationWindow,
    scope: MoveScope,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
) -> cut_trim::Gesture {
    press_move_scaled(
        window,
        scope,
        travel_px,
        row_change,
        d_seconds,
        crate::ui::cut_view::strip_pps(),
    )
}

/// [`press_move_with_override`] at the caller's zoom: S2's 8 px snap is measured at the painted band's pps.
pub(crate) fn press_move_scaled(
    window: &adw::ApplicationWindow,
    scope: MoveScope,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
    pps: f64,
) -> cut_trim::Gesture {
    let (recorders_band, wave_strip, on_border, hand_scene, press_second) = scope;
    // The scene seam only answers when the press took neither a band, a border, nor a selection it fell inside
    // (the flowchart asks those BEFORE a scene). `hand_scene` carries a scene a door already worked out.
    let scene = if recorders_band.is_some()
        || wave_strip.is_some()
        || on_border
        || press_falls_inside(window::selection().as_ref(), press_second)
    {
        None
    } else {
        hand_scene.or_else(window::press_scene)
    };
    press_move_core(
        window,
        (recorders_band, wave_strip, on_border, scene, press_second),
        travel_px,
        row_change,
        d_seconds,
        pps,
    )
}

fn press_move_core(
    window: &adw::ApplicationWindow,
    scope: MoveScope,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
    pps: f64,
) -> cut_trim::Gesture {
    let (recorders_band, wave_strip, on_border, scene, press_second) = scope;
    let cut_ = CUT_PAGE
        .with(
            |CutPage {
                 move_open: cell, ..
             }| cell.borrow().clone(),
        )
        .unwrap_or_else(window::newest_review_cut);
    let band = window::selection();
    let band_name = recorders_band
        .clone()
        // The shift lands on whichever recording the press took; a wave-strip press names it in `wave_strip`,
        // not `recorders_band`.
        .or_else(|| wave_strip.clone());
    let selected: Vec<usize> = match (&band, &cut_) {
        (Some(band), cut_) => cut_
            .segs
            .iter()
            .enumerate()
            .filter(|(_, seg)| seg.ins.is_empty() && seg.e > band.start && seg.s < band.end)
            .map(|(index, _)| index)
            .collect(),
        (None, _) => Vec::new(),
    };
    let sources: Vec<String> = cut_
        .shift
        .keys()
        .cloned()
        .chain(band_name.clone())
        .collect::<std::collections::BTreeSet<String>>()
        .into_iter()
        .collect();
    let press = cut_trim::Press {
        recorders_band: recorders_band.as_deref(),
        wave_strip: wave_strip.as_deref(),
        selection: band.as_ref().map(|b| (b.start, b.end)),
        selected,
        inside_selection: press_falls_inside(band.as_ref(), press_second),
        on_border,
        scene,
        row: window::press_row(window),
    };
    let gesture = cut_trim::right_gesture(
        &press,
        travel_px,
        row_change,
        d_seconds,
        &cut_.segs,
        0.0,
        crate::ui::window_trim_strip::session_wall(),
        pps,
        &cut_.folds,
        &cut_.shift,
        &sources,
        false,
    );
    // What the answer costs (persistence, the one Undo step, the folds) is [`apply_gesture`].
    apply_gesture(window, &gesture);
    if let cut_trim::Gesture::Slid { status, .. } = &gesture {
        if let Some(status_line) = window::find_status(window.upcast_ref()) {
            status_line.set_text(status);
        }
    }
    gesture
}

/// S3 from the widget: the release of a right press that never moved. A click writes nothing
/// (`spec/inventory/cut.md`: "Unmoved right press = \"this one\" (status only)").
pub fn press_unmoved(window: &adw::ApplicationWindow) {
    if let Some(status_line) = window::find_status(window.upcast_ref()) {
        status_line.set_text(CLICK_STATUS);
    }
}

/// [`paint_laid_pieces`] with the caller's bar geometry (`top`, and `plate_from_top` as the bar's height), so
/// the placeholder strip and the PAINTED band show the same pieces.
pub fn paint_laid_pieces_in(
    cr: &cairo::Context,
    w: f64,
    cut_: &Cut,
    pps: f64,
    top: f64,
    plate_from_top: f64,
) {
    for seg in cut_.segs.iter().filter(|seg| !seg.lane.is_empty()) {
        let x = seg.s.max(0.0) * pps;
        if x > w {
            continue;
        }
        let wide = ((seg.e - seg.s).max(0.0) * pps).min((w - x).max(0.0));
        if wide <= 0.0 {
            continue;
        }
        cr.set_source_rgba(0.15, 0.32, 0.66, 0.85);
        cr.rectangle(x, top + plate_from_top - 7.0, wide, 6.0);
        let _ = cr.fill();
        if wide < 34.0 {
            continue;
        }
        cr.set_source_rgb(0.98, 0.98, 1.0);
        cr.set_font_size(8.0);
        cr.move_to(x + 2.0, top + plate_from_top - 9.0);
        let _ = cr.show_text(&seg.lane);
    }
}

/// The move gesture's `drag-begin`: start the hold clean, so the first real move is the one that pushes.
pub fn begin_move() {
    CUT_PAGE.with(
        |CutPage {
             move_pushed: cell, ..
         }| cell.set(false),
    );
    // Every update measures its travel from the cut as the press found it, or eight one-second moves would slide eight seconds.
    CUT_PAGE.with(
        |CutPage {
             move_open: cell, ..
         }| *cell.borrow_mut() = Some(window::newest_review_cut()),
    );
    // §7: the file waits for the release; every write until `end_move` is refused at the writer.
    crate::cut_save::begin_gesture();
    CUT_PAGE.with(
        |CutPage {
             drag_folds: cell, ..
         }| cell.borrow_mut().clear(),
    );
    CUT_PAGE.with(
        |CutPage {
             drag_open: cell, ..
         }| cell.borrow_mut().clear(),
    );
}

/// Has this hold already spent its Undo step?
fn already_pushed() -> bool {
    CUT_PAGE.with(
        |CutPage {
             move_pushed: cell, ..
         }| cell.get(),
    )
}

/// The folds the drag in flight opened, newest write last.
pub fn open_folds() -> Vec<usize> {
    CUT_PAGE.with(
        |CutPage {
             drag_open: cell, ..
         }| cell.borrow().clone(),
    )
}

/// The folds the rule asked to open, as the cut's own fold spans.
fn fold_spans(cut_: &Cut, open: &[usize]) -> Vec<[f64; 2]> {
    open.iter()
        .filter_map(|index| cut_.folds.get(*index).copied())
        .collect()
}

/// F2.8 S2: apply a right gesture to this window's cut. This owns what the answer COSTS:
///
/// * `Gesture::Click` (S3) writes nothing and prints `this one`; a history step would make ↶ eat a press nobody made.
/// * a slide writes the shift map, row pin and trimmed seconds to cut.json through the usual editing door;
/// * the first real move of a hold goes through `record_edit` (the one Undo step); later updates write state
///   without pushing.
/// * the folds the rule asked to open are opened now and refolded on release ([`end_move`]).
pub fn apply_gesture(window: &adw::ApplicationWindow, gesture: &cut_trim::Gesture) -> bool {
    let cut_trim::Gesture::Slid {
        shift,
        row,
        open,
        dragged,
        segs,
        ..
    } = gesture
    else {
        return false;
    };
    let mut cut_ = crate::ui::window::newest_review_cut();
    cut_.shift = shift.clone();
    cut_.segs = segs.clone();
    if let Some(row) = row {
        if let Some(base) = dragged {
            cut_.rows = crate::cut_cam::pin_for_drag(base, *row, &cut_.rows);
            cut_.nrows = (*row as i32 + 1).max(cut_.nrows);
        }
    }
    let opened = fold_spans(&cut_, open);
    if opened.is_empty() {
        CUT_PAGE.with(
            |CutPage {
                 drag_folds: cell, ..
             }| cell.borrow_mut().clear(),
        );
        CUT_PAGE.with(
            |CutPage {
                 drag_open: cell, ..
             }| cell.borrow_mut().clear(),
        );
    } else {
        // Open them through F2.11's own door (a fold is a view: saved, no Undo step), remembered by span for the release.
        CUT_PAGE.with(
            |CutPage {
                 drag_folds: cell, ..
             }| *cell.borrow_mut() = opened.clone(),
        );
        CUT_PAGE.with(
            |CutPage {
                 drag_open: cell, ..
             }| *cell.borrow_mut() = open.clone(),
        );
        let mut cut_open = cut_.clone();
        for span in opened.iter() {
            cut_open
                .folds
                .retain(|fold| !(fold[0] < span[1] && fold[1] > span[0]));
        }
        crate::ui::window::seed_review_cut(&cut_open);
        crate::ui::cut_folds::refresh_fold_badges(window);
        cut_ = cut_open;
    }
    // §7's one Undo per drag: the FIRST real move spends the history step, and the FILE waits for the gesture's end.
    if already_pushed() {
        crate::ui::window_history_zoom::amend_edit(window, &cut_);
    } else {
        crate::ui::window::record_edit(window, &cut_);
        CUT_PAGE.with(
            |CutPage {
                 move_pushed: cell, ..
             }| cell.set(true),
        );
    }
    // NOT a file write: this runs on every mouse-move; §7's one writer stays at the gesture's end.
    true
}

/// S3: an unmoved right press is a click: nothing written or undone; the page says `this one`.
pub const CLICK_STATUS: &str = "this one";

/// §7 bullet 7 / F2.8 S1: the trim's release. The drag only updates state; THIS door writes `cut.json`, so
/// the file holds the edge the hand left.
pub fn end_release() {
    // The gesture ends HERE, so the write below is the first one the hold is allowed.
    crate::cut_save::end_gesture();
    window::save_insert_cut(&window::newest_review_cut());
    land_trim(&window::main_window());
}

/// S1 on release: what a trim settles once the hand lets go. A neighbour landed against within a frame joins,
/// and effects over what the trim took away are clamped. Mid-drag a join would lose the far side of a border.
pub fn land_trim(window: &adw::ApplicationWindow) {
    let Some((index, border)) = CUT_PAGE.with(|page| page.trimmed.take()) else {
        return;
    };
    let mut cut_ = window::newest_review_cut();
    if let Some(held) = cut_trim::merge_at_border(&mut cut_.segs, index, border) {
        crate::ui::window_history_zoom::amend_edit(window, &cut_);
        if let (Some(seg), Some(status)) = (
            cut_.segs.get(held),
            window::find_status(window.upcast_ref()),
        ) {
            status.set_text(&cut_trim::joined_status(seg.s, seg.e));
        }
        crate::ui::cut_folds::refresh_fold_badges(window);
    }
    window::clamp_effects_to_cut(window);
}

/// F2.8 S2 ("refold after"): the drag released. Put back exactly the folds this drag opened, newest span first
/// so a shifted list lands where it was; a fold the user had already opened is not this drag's to close.
pub fn end_move(window: &adw::ApplicationWindow) {
    CUT_PAGE.with(
        |CutPage {
             move_open: cell, ..
         }| cell.borrow_mut().take(),
    );
    let opened = CUT_PAGE.with(|page| page.drag_folds.borrow_mut().split_off(0));
    if opened.is_empty() {
        CUT_PAGE.with(
            |CutPage {
                 drag_open: cell, ..
             }| cell.borrow_mut().clear(),
        );
        CUT_PAGE.with(
            |CutPage {
                 move_pushed: cell, ..
             }| cell.set(false),
        );
        // The hold still moved the edge, so the release still writes it, fold or no fold.
        crate::cut_save::end_gesture();
        crate::ui::window::save_insert_cut(&crate::ui::window::newest_review_cut());
        return;
    }
    let mut cut_ = crate::ui::window::newest_review_cut();
    for span in opened.iter().rev() {
        if !crate::cut_fold::is_folded(&cut_.folds, (span[0], span[1])) {
            cut_.folds.push(*span);
        }
    }
    crate::ui::cut_folds::sync_page_folds(&mut cut_);
    // One act of the hand: the refold belongs to the step the move already opened.
    crate::ui::window::record_edit(window, &cut_);
    crate::cut_save::end_gesture();
    crate::ui::cut_folds::save_folds(&cut_);
    crate::ui::cut_folds::refresh_fold_badges(window);
    CUT_PAGE.with(
        |CutPage {
             drag_open: cell, ..
         }| cell.borrow_mut().clear(),
    );
    CUT_PAGE.with(
        |CutPage {
             move_pushed: cell, ..
         }| cell.set(false),
    );
}

/// F2.8 S1: where the border the hand just landed on actually is in the cut, so the page asks the PLAYER for
/// the second the picture must show rather than trusting the pointer's pixel. Reading the segment back after
/// the write keeps a scrub past the recording's end or a neighbour from showing a picture the cut lacks.
pub fn edge_second(index: usize, border: cut_trim::Border) -> f64 {
    let cut_ = crate::ui::window::newest_review_cut();
    match cut_.segs.get(index) {
        Some(seg) => match border {
            cut_trim::Border::Start => seg.s,
            cut_trim::Border::End => seg.e,
        },
        // The segment went away (a release that joined two scenes) or the index is stale; 0.0 is never asked for.
        None => 0.0,
    }
}

/// F2.8 S1: re-seek the RUNNING media leg to the second the dragged border just landed on, through
/// `cut_play_back::cue_clips` (the leg the `NAIVEPOST_PLAYER` seam records). Answers whether anything was
/// re-seeked: with no preview running there is no picture to scrub. The release calls this too, so the LAST
/// edge always reaches the picture despite the [`crate::cut_trim::SCRUB_MS`] throttle.
pub fn scrub_picture(at_secs: f64) -> bool {
    if !crate::ui::window::preview_player().transport.playing {
        return false;
    }
    if crate::cut_play_back::playing_pid().is_none() {
        return false;
    }
    let files = crate::ui::cut_record_play::files_for(&crate::ui::cut_record_play::runs_now());
    if files.is_empty() {
        return false;
    }
    crate::cut_play_back::cue_clips(&files, at_secs, &|line| crate::ui::window::log_line(line))
        .is_ok()
}

/// F2.8 S1 with the drag in hand remembered: `first` says whether THIS move is the hold's first, the only one
/// that spends an Undo step (§7; [`cut_trim::pushes_undo`], `cut_rules::picking_up_edits`). Mid-drag updates
/// write and persist the cut but leave history alone, so one ↶ takes back a whole drag.
pub fn press_border_move(
    window: &adw::ApplicationWindow,
    index: usize,
    border: cut_trim::Border,
    to: f64,
    first: bool,
) -> f64 {
    let mut cut_ = crate::ui::window::newest_review_cut();
    let (rec_start, rec_end) = cut_
        .segs
        .get(index)
        .map_or((0.0, window::TRACK_REC_END), |seg| {
            crate::ui::window_trim_strip::clip_wall(seg.s, seg.e)
        });
    let clamped = cut_trim::clamp_edge(&cut_.segs, index, border, to, rec_start, rec_end);
    match border {
        cut_trim::Border::Start => {
            if let Some(seg) = cut_.segs.get_mut(index) {
                seg.s = clamped;
            }
        }
        cut_trim::Border::End => {
            if let Some(seg) = cut_.segs.get_mut(index) {
                seg.e = clamped;
            }
        }
    }
    CUT_PAGE.with(|CutPage { trimmed: held, .. }| held.set(Some((index, border))));
    let status = cut_
        .segs
        .get(index)
        .map(|seg| cut_trim::trim_status(index + 1, seg.s, seg.e))
        .unwrap_or_default();
    if crate::cut_rules::picking_up_edits(first) {
        window::record_edit(window, &cut_);
    } else {
        // The drag's own Undo step is amended.
        crate::ui::window_history_zoom::amend_edit(window, &cut_);
    }
    // S1: the picture follows the edge, at the second the cut now holds (a clamp may stop short of the pointer).
    scrub_picture(edge_second(index, border));
    if !status.is_empty() {
        if let Some(status_line) = window::find_status(window.upcast_ref()) {
            status_line.set_text(&status);
        }
    }
    clamped
}
