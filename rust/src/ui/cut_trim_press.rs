//! F2.8 S1/S2/S3 — the Cut page's trim-and-move PRESS half, split out of `window.rs` (over its budget).
//!
//! `cut_trim` holds the RULES (where a border may go, what a right-press takes, what the sentence says).
//! This module holds the three things only the page can supply:
//!
//! * the strip's real vertical bands and the NAME a press in each of them resolves to — the recorders'
//!   band answers the LANE whose painted span holds x, the wave strip answers the recording whose filmed
//!   run holds the press second, so "that recording slides" and "the recording under the pointer slides"
//!   are geometry, not the y-accident of a shared placeholder span;
//! * the page side of a gesture — what gets persisted, how often history is pushed, which folds the drag
//!   opened and must refold, and the live re-seek that makes a dragged border scrub the picture;
//! * S3's unmoved press: a right click is a click and says `this one`.
//!
//! DECISION (spec states the band ORDER in §27 Tracks and pictures it in `spec/img/05-trim.png` but gives
//! no pixel heights for the strip's own lane/wave rows): the kept bar keeps exactly the span it had
//! (`RULER_H .. RULER_H + BAR_H`) so no press y changes band, and the recorders' band and the wave strip
//! are ADDED below it, each its own span, each present only when the cut actually has lanes / filmed
//! recordings. With neither, the strip answers the old placeholder layout.

use crate::cut::Cut;
use crate::cut_trim::{self, Band, StripBands};
use crate::timeline::Recording;
use crate::ui::window;
use adw::prelude::*;

/// F2.8 S2: the recorders' band's height — one lane row, short because it carries a plate, not pictures.
pub const LANE_H: f64 = 18.0;
/// F2.8 S1/S2: the wave strip's height. The row's own sound strip (§27 Tracks: "each row's first audio
/// track as a strip below it"), drawn under the kept bar for the same reason the spec pictures it there.
pub const WAVE_H: f64 = 18.0;

/// The strip's TOTAL height when every row it can draw is drawn: ruler, kept bar, the recorders' band and
/// the wave strip. `window.rs` sizes the drawing area with this, because a band a press can resolve must
/// also have room to be painted in — sizing the strip by the tallest answer [`bands`] can give is what
/// keeps a short cut and a cut with lanes in one layout rather than two, which is also what keeps an
/// existing press y in the band it meant.
pub const STRIP_H: f64 = cut_trim::RULER_H + cut_trim::BAR_H + LANE_H + WAVE_H;

/// This strip's bands RIGHT NOW: what the painter draws and what a press resolves against, so the two can
/// never disagree. The kept bar keeps its old span so every press y that meant "bar" still means "bar";
/// the two new rows sit under it, and each is only there when there is something to press in it. No lanes
/// and no filmed recordings means the strip draws one row only, and answering the placeholder layout is the
/// honest answer for a strip that drew one row.
pub fn bands(cut_: &Cut, filmed: &[Recording]) -> StripBands {
    let bar_top = cut_trim::RULER_H;
    let bar_bottom = bar_top + cut_trim::BAR_H;
    if cut_.lanes.is_empty() && filmed.is_empty() {
        return cut_trim::PLACEHOLDER_STRIP_BANDS;
    }
    // A missing row collapses to zero height at the bottom of the row above it, so `band_at` can never
    // answer a band the painter did not draw.
    let lane_top = bar_bottom;
    let lane_bottom = if cut_.lanes.is_empty() { lane_top } else { lane_top + LANE_H };
    let wave_top = lane_bottom;
    let wave_bottom = if filmed.is_empty() { wave_top } else { wave_top + WAVE_H };
    StripBands {
        recorders: (lane_top, lane_bottom),
        // `band_at` tests wave before bar, so a collapsed wave span can never steal the bar's row.
        wave: (wave_top, wave_bottom),
        bar: (bar_top, bar_bottom),
    }
}

/// F2.8 S1: how the wave strip's peaks are cached, when a cache exists. Not a new dial: `wave::write`'s own
/// rate, restated here because the painter needs *some* window to ask the cache with, and `params.rs` holds
/// no numbers to catalogue it under (F2.11 owns the real waveform rows).
const WAVE_HZ: f64 = 20.0;

/// F2.8 S2 ("a footage selection the press falls inside"): does the press's second lie inside the band's
/// seconds? The band's LENGTH is not the question — a band that exists but sits nowhere near the pointer
/// takes nothing. `Selection` lives in `window.rs`'s layer, so the rule is asked of the span itself.
pub fn press_falls_inside(band: Option<&crate::cut_select::Selection>, at_seconds: f64) -> bool {
    band.is_some_and(|band| band.start <= at_seconds && at_seconds <= band.end)
}

/// The lane painted under x on the recorders' band: its span is `at .. at + dur` at this strip's zoom,
/// which is exactly what [`paint_bands`] draws, so the plate the eye saw is the plate the hand picked.
fn lane_at_x(cut_: &Cut, x: f64, pps: f64) -> Option<String> {
    let at = x / pps;
    cut_
        .lanes
        .iter()
        .find(|lane| lane.at <= at && at < lane.at + lane.dur)
        .map(|lane| lane.name.clone())
}

/// The recording whose filmed run holds the press second — the wave strip's answer, named by
/// `Recording::base` like every other place a recording is named.
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
/// A name can only come from a plate that was PAINTED. Where the strip drew no lane row or no wave row
/// there is nothing to read a name off, so a caller that already knows which recording its press means
/// keeps its answer through `seam` instead of losing the branch — and only for the row that is absent: a
/// cut that does have lanes gets its name from the lane under x, never from the seam.
pub fn press_targets_at(
    cut_: &Cut,
    x: f64,
    y: f64,
    boxes: &[cut_trim::Box_],
    pps: f64,
    seam: Option<&str>,
) -> (Option<String>, Option<String>, bool) {
    let on_border = cut_trim::border_at(boxes, x).is_some();
    // The wave strip's recordings come from the page, so the door asks them itself rather than trusting a
    // caller that has no way to know what was painted.
    let filmed = filmed_rows(cut_);
    // A name can only come from a plate that was PAINTED. Where a row is absent — collapsed to zero height,
    // or the whole layout back to the placeholder because this strip drew one row — there is nothing to read
    // a name off, so a caller that already knows which recording its press means keeps its answer through
    // `seam`. A row that WAS drawn answers only from the plate under x, never from the seam.
    let drawn = painted_bands(cut_, &filmed);
    // Which row the press fell on is decided by the SAME numbers the painter used, through
    // [`painted_bands`]: a row with nothing to plate for it has collapsed to zero height there, so it can
    // never be answered. Where a row was not painted there is nothing to read a name off, and the caller's
    // seam stands in for THAT row only — a cut that does have lanes gets its name from the lane under x.
    let no_lane_row = cut_.lanes.is_empty();
    match band_at(y, &drawn) {
        Band::Recorders => (
            lane_at_x(cut_, x, pps).or_else(|| {
                no_lane_row.then(|| seam.map(str::to_string)).flatten()
            }),
            None,
            on_border,
        ),
        Band::Wave => (
            None,
            wave_answer(cut_, &filmed, x, pps, seam),
            on_border,
        ),
        Band::Ruler | Band::Bar => (None, None, on_border),
    }
}

/// `cut_trim::band_at` over THIS strip's bands. Kept local so the page never passes the placeholder const
/// by accident; the rule itself stays in `cut_trim` and stays tested there.
fn band_at(y: f64, bands: &StripBands) -> Band {
    cut_trim::band_at(y, bands)
}

/// The strip's bands as a PRESS must read them. Identical to [`bands`] except in the one case where the two
/// questions genuinely differ: `bands` sizes the drawing area, so it always reserves the two rows; a press
/// may only resolve a row that was PAINTED, and the painter paints the recorders' band per lane and the wave
/// strip per recording the page was handed. With neither, this strip drew ONE row, and answering the
/// placeholder layout is what makes such a press land on the bar (flowchart M3/M4/M5) instead of on a band
/// whose plate is not there.
fn painted_bands(cut_: &Cut, filmed: &[Recording]) -> StripBands {
    if cut_.lanes.is_empty() || filmed.is_empty() {
        return cut_trim::PLACEHOLDER_STRIP_BANDS;
    }
    bands(cut_, filmed)
}

/// Is the wave strip's recording read from a plate at all? Only when the recorders' band sits above it,
/// i.e. only when the cut has lanes: without them [`painted_bands`] answers the placeholder layout, whose
/// wave span shares the bar's row — one painted row, so no plate of its own to name, and the caller's seam
/// stands in. This is asked separately from [`painted_bands`] because the two are different questions: the
/// placeholder makes a wave y resolve to WAVE (so the branch fires), while the naming rule must still know
/// there was no plate behind it.
fn wave_plate_exists(cut_: &Cut, filmed: &[Recording]) -> bool {
    !filmed.is_empty() && !cut_.lanes.is_empty()
}

/// The wave strip's answer for this press: the RECORDING whose plate lies under x when such a plate was
/// painted, and otherwise the caller's seam — which is what keeps flowchart M2 reachable on a strip that drew
/// one row, where there is nothing to read a name off.
fn wave_answer(cut_: &Cut, filmed: &[Recording], x: f64, pps: f64, seam: Option<&str>) -> Option<String> {
    if wave_plate_exists(cut_, filmed) {
        return recording_at_x(filmed, x, pps);
    }
    seam.map(str::to_string)
}

/// The recordings the wave strip is painted over: ONLY what the page was handed (`page_recordings`). A cut's
/// own kept footage is not a substitute — that is what the PICTURE ROWS are made of (F2.11), and the wave
/// strip under them belongs to those recordings; a page nobody handed a session list drew no wave plates, so
/// there is nothing to read a name off and the caller's seam stands in.
pub fn filmed_rows(_cut_: &Cut) -> Vec<Recording> {
    crate::ui::cut_folds::page_recordings()
}

/// S2's questions in the order the flowchart asks them, bundled because the page answers them all at the
/// press and the move rule then needs every one of them.
pub type MoveScope = (
    Option<String>, // the recorders' band's lane, if the press landed on one
    Option<String>, // the wave strip's recording, if it landed on one
    bool,           // whether it landed on a drawn clip border (S1 claims a border first)
    Option<usize>,  // the scene under it, if none of the above did
    f64,            // where the press fell, in seconds: S2's "a selection the press falls inside"
);

/// F2.8 S2/S3: the move rule, with the scope bundle standing in for the flowchart's five questions so the
/// signature stays readable: `(recorders' band, wave strip, on a border, scene, press second)`. Both doors
/// land here from `window.rs`: the widget's position-driven `press_move_at` and the older seam-only
/// `press_move`, which passes whatever seams were primed.
pub fn press_move_with_override(
    window: &adw::ApplicationWindow,
    scope: MoveScope,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
) -> cut_trim::Gesture {
    let (recorders_band, wave_strip, on_border, hand_scene, press_second) = scope;
    // The scene seam only answers when this press took neither a band, nor a border, nor a selection the
    // press fell inside: the flowchart asks those questions BEFORE it asks about a scene. A band the press
    // falls outside, and a border, both still answer `None` here — one answer, so the two branches cannot
    // drift apart. A door that already worked out which scene the press landed on hands it in the scope
    // (`hand_scene`); the seam answers for the doors that have nothing to hand.
    let scene = if recorders_band.is_some()
        || wave_strip.is_some()
        || on_border
        || press_falls_inside(window::selection(window).as_ref(), press_second)
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
    )
}

/// F2.8 S2: the move rule with every input supplied, including which scene (if any) the press fell on.
fn press_move_core(
    window: &adw::ApplicationWindow,
    scope: MoveScope,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
) -> cut_trim::Gesture {
    let (recorders_band, wave_strip, on_border, scene, press_second) = scope;
    let cut_ = window::newest_review_cut();
    let band = window::selection(window);
    let band_name = recorders_band
        .clone()
        // F2.8 S2: the shift lands on whichever recording the press took, and a wave-strip press names its
        // recording in `wave_strip`, not in `recorders_band` — leaving this at the band alone wrote the
        // correction to no source at all for every M2 press.
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
        window::TRACK_REC_END,
        crate::ui::cut_view::strip_pps(),
        &cut_.folds,
        &cut_.shift,
        &sources,
        false,
    );
    // What the answer costs (persistence, the one Undo step, the folds) is [`apply_gesture`]; the sentence
    // the rule earned goes on this window's own status line.
    apply_gesture(window, &gesture);
    if let cut_trim::Gesture::Slid { status, .. } = &gesture {
        if let Some(status_line) = window::find_status(window.upcast_ref()) {
            status_line.set_text(status);
        }
    }
    gesture
}

/// S3 from the widget: the release of a right press that never moved. `spec/inventory/cut.md`'s summary of
/// this surface gives the wording ("Unmoved right press = \"this one\" (status only)"); a click writes
/// nothing at all, so this is the whole of what the page does with it.
pub fn press_unmoved(window: &adw::ApplicationWindow) {
    if let Some(status_line) = window::find_status(window.upcast_ref()) {
        status_line.set_text(CLICK_STATUS);
    }
}

/// The plate colours the spec's picture uses: a dim lane plate under the ruler's kept greens, and the
/// recordings' own blue wave strip. The GEOMETRY is the point — a press must land where a plate is — so
/// the waveform is best-effort and a missing cache draws a flat baseline rather than nothing.
pub fn paint_bands(cr: &cairo::Context, w: f64, cut_: &Cut, filmed: &[Recording], pps: f64) {
    let bands = bands(cut_, filmed);
    let (lane_top, lane_bottom) = bands.recorders;
    if lane_bottom > lane_top {
        for lane in &cut_.lanes {
            let x = lane.at * pps;
            let wide = (lane.dur * pps).max(2.0);
            if x > w {
                continue;
            }
            cr.set_source_rgba(0.35, 0.45, 0.6, 0.45);
            cr.rectangle(x, lane_top + 2.0, wide.min(w - x), lane_bottom - lane_top - 4.0);
            let _ = cr.fill();
            cr.set_source_rgb(0.95, 0.95, 0.95);
            cr.set_font_size(9.0);
            cr.move_to(x + 3.0, lane_bottom - 5.0);
            let _ = cr.show_text(&lane.name);
        }
    }
    let (wave_top, wave_bottom) = bands.wave;
    for rec in filmed {
        paint_row_wave(cr, w, rec, pps, wave_top, wave_bottom);
    }
}

/// One row's wave strip, painted at whatever geometry the caller has: the placeholder strip calls this from
/// [`paint_bands`] with its own span, and the PAINTED band's picture rows call it with the lower third of
/// each camera row (`cut_tracks::ROW_WAVE_PX`). Split out of [`paint_bands`] so both surfaces ink the same
/// plate from the same cache instead of drawing two different answers to "what does this row sound like".
///
/// The peaks are decoration: a cache that cannot be read draws a flat baseline, so the row still reads as a
/// row and a press there means what its band says.
pub fn paint_row_wave(
    cr: &cairo::Context,
    w: f64,
    rec: &Recording,
    pps: f64,
    top: f64,
    bottom: f64,
) {
    if bottom <= top {
        return;
    }
    let x = rec.start * pps;
    let wide = ((rec.end - rec.start) * pps).max(2.0).min((w - x).max(0.0));
    if x > w || wide <= 0.0 {
        return;
    }
    cr.set_source_rgba(0.2, 0.4, 0.8, 0.30);
    cr.rectangle(x, top, wide, bottom - top);
    let _ = cr.fill();
    // Ask the real cache (`cache/waves/<base>.wave`) and draw a peak column every `1/WAVE_HZ` s that fits
    // in this strip; no cache, flat baseline.
    let peaks = cached_peaks(&rec.base);
    cr.set_source_rgb(0.15, 0.32, 0.66);
    let mid = top + (bottom - top) / 2.0;
    match &peaks {
        Some(block) => {
            let step = 1.0 / WAVE_HZ * pps;
            let first = (rec.start * WAVE_HZ) as usize;
            let mut column = 0usize;
            let mut px = x;
            while px < x + wide {
                let peak = block
                    .get(first + column)
                    .map(|peak| (*peak as f64 / 255.0) * (bottom - top) / 2.0)
                    .unwrap_or(0.0);
                cr.rectangle(px, mid - peak, step.max(1.0), peak * 2.0);
                let _ = cr.fill();
                column += 1;
                px += step;
            }
        }
        None => {
            cr.rectangle(x, mid, wide, 1.0);
            let _ = cr.fill();
        }
    }
}

/// F2.9 S2: the pieces a sound paste LAID over the kept footage, painted on top of everything else in the
/// bar. A segment whose `lane` is non-empty is exactly what [`crate::cut_copy::lay_pieces`] writes — `ins`
/// names the file, `lane` the recording it stands in for, `ss` the file second it opens on — and it is
/// neither kept footage nor an insert, so the kept green and the insert violet would both be a lie about it.
///
/// Colour is the painter's own decision: §A's inventory gives the band a kept green and an insert violet and
/// says nothing about a laid-over sound, so this takes the same blue as every wave plate in this strip
/// ([`paint_row_wave`]) — one colour for "this is audio", whatever row carries it — drawn as a thin plate at
/// the bar's bottom so the picture under it stays visible: the spec's point is that the video did NOT change.
/// The lane name goes on when the box is wide enough to hold it; a narrow piece reads as a plate alone rather
/// than as a plate with a word cut off it.
pub fn paint_laid_pieces(cr: &cairo::Context, w: f64, cut_: &Cut, pps: f64) {
    paint_laid_pieces_in(cr, w, cut_, pps, cut_trim::RULER_H, 8.0);
}

/// [`paint_laid_pieces`] with the strip's own geometry handed in: `top` is where the caller's kept bar
/// begins and `plate_from_top` how tall that bar is, so the plate sits at the bottom of whichever bar carries
/// it — the window's placeholder strip (RULER_H / BAR_H) and the PAINTED band (its picture-row top and
/// height) then show the same pieces, and a paste that only inked one surface would read as a failed paste on
/// the other.
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

/// This recording's cached mono envelope, if `cache/waves` holds one that applies. Best-effort by design:
/// a cache that cannot be read is a missing decoration, never a reason the strip draws nothing.
fn cached_peaks(base: &str) -> Option<Vec<u8>> {
    let root = std::env::current_dir().ok()?;
    let tree = crate::layout::Tree::new(crate::startup::session_dir(&root)).ok()?;
    let meta = std::fs::metadata(tree.wave(base)).ok()?;
    let wave = crate::wave::read(&tree, base, meta.len() as i64, 0).ok().flatten()?;
    wave.chans.into_iter().next()
}

// --- what the page holds between the press and the release ---------------------------------------------

thread_local! {
    /// Has this gesture's hold already spent its Undo step? One act of the hand is one step
    /// (`cut_trim::pushes_undo`), and a drag fires an update per mouse-move event. Reset at `drag-begin`.
    static MOVE_PUSHED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The folds THIS drag opened, as the spans they were, so the release can refold exactly those and
    /// leave alone a fold the user had already opened.
    static DRAG_FOLDS: std::cell::RefCell<Vec<[f64; 2]>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The folds the gesture in flight asked to open, newest write last — what `ui::open_folds` reads.
    static DRAG_OPEN: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The move gesture's `drag-begin`: start the hold clean, so the first real move is the one that pushes.
pub fn begin_move() {
    MOVE_PUSHED.with(|cell| cell.set(false));
    DRAG_FOLDS.with(|cell| cell.borrow_mut().clear());
    DRAG_OPEN.with(|cell| cell.borrow_mut().clear());
}

/// Has this hold already spent its Undo step?
fn already_pushed() -> bool {
    MOVE_PUSHED.with(|cell| cell.get())
}

/// The folds the drag in flight opened, newest write last.
pub fn open_folds() -> Vec<usize> {
    DRAG_OPEN.with(|cell| cell.borrow().clone())
}

/// The folds the rule asked to open, as the cut's own fold spans. Indexing into the cut rather than
/// reading the surface is what makes the answer the same whether the cut has two folds or none.
fn fold_spans(cut_: &Cut, open: &[usize]) -> Vec<[f64; 2]> {
    open.iter().filter_map(|index| cut_.folds.get(*index).copied()).collect()
}

/// F2.8 S2: apply a right gesture to this window's cut. Called from `window.rs`' move path, which owns
/// the press position and the status line; this owns what the answer COSTS:
///
/// * `Gesture::Click` (S3) writes nothing at all and prints `this one` — the wording
///   `spec/inventory/cut.md` gives for an unmoved right press ("Unmoved right press = \"this one\" (status
///   only)"), and a click that wrote a history step would make ↶ eat a press nobody made.
/// * a slide writes the shift map, the row pin and the trimmed seconds to cut.json through the same door
///   every editing press uses, so a correction survives a reopen;
/// * the first real move of a hold goes through `record_edit` (it pushes the one Undo step); the updates
///   after it write state without pushing, which is what `pushes_undo` means on this path.
/// * the folds the rule asked to open are opened now and refolded on release ([`end_move`]).
pub fn apply_gesture(window: &adw::ApplicationWindow, gesture: &cut_trim::Gesture) -> bool {
    let cut_trim::Gesture::Slid { shift, row, open, .. } = gesture else {
        return false;
    };
    let mut cut_ = crate::ui::window::newest_review_cut();
    cut_.shift = shift.clone();
    if let Some(row) = row {
        // §3 keeps a hand row correction in `rows`, per source; `nrows` stays what it always was, a floor
        // under the row count. The pinned recording is whatever the gesture moved, which the caller's cut
        // already knows: the shift map's own keys are the sources in play.
        for source in shift.keys() {
            cut_.rows.insert(source.clone(), *row as i32);
        }
        cut_.nrows = (*row as i32) + 1;
    }
    // (c) the folds — S2's "folds near the grab open for the drag, refold after", and refolding is what
    // makes them belong to THIS gesture rather than to the user. The cut's own fold list is the measure:
    // an index that no longer names a fold (a second update, or nothing folded where the drag travelled)
    // is no gesture at all, so it neither opens nor closes anything. The drag therefore never persists the
    // act of opening: the fold the person saved is written back whole on release, so a page that reloads
    // mid-gesture, or a gesture that goes nowhere, cannot be left holding a fold nobody chose.
    let opened = fold_spans(&cut_, open);
    if opened.is_empty() {
        DRAG_FOLDS.with(|cell| cell.borrow_mut().clear());
        DRAG_OPEN.with(|cell| cell.borrow_mut().clear());
    } else {
        // Open them for the drag through F2.11's own door (a fold is a view: saved, and it spends no
        // Undo step), remembered by span so the release can put back exactly these.
        DRAG_FOLDS.with(|cell| *cell.borrow_mut() = opened.clone());
        DRAG_OPEN.with(|cell| *cell.borrow_mut() = open.clone());
        let mut cut_open = cut_.clone();
        for span in opened.iter() {
            cut_open.folds.retain(|fold| !(fold[0] < span[1] && fold[1] > span[0]));
        }
        crate::ui::window::seed_review_cut(window, &cut_open);
        crate::ui::cut_folds::refresh_fold_badges(window);
    }
    // One Undo per gesture: the first real move pushes, the rest of the hold updates in place.
    if already_pushed() {
        crate::ui::window::seed_review_cut(window, &cut_);
    } else {
        crate::ui::window::record_edit(window, &cut_);
        MOVE_PUSHED.with(|cell| cell.set(true));
    }
    crate::ui::window::save_insert_cut(&cut_);
    true
}


/// S3: an unmoved right press is a click. Nothing is written, nothing is undone; the page says `this one`.
/// `spec/inventory/cut.md`'s summary of this surface is the source of the wording.
pub const CLICK_STATUS: &str = "this one";

/// F2.8 S2 ("refold after"): the drag released. Put back exactly the folds this drag opened, newest span
/// first so a list that shifted under the unfolds lands where it was, and leave every other fold alone —
/// a fold the user had already opened is not this drag's to close.
pub fn end_move(window: &adw::ApplicationWindow) {
    let opened = DRAG_FOLDS.with(|cell: &std::cell::RefCell<Vec<[f64; 2]>>| cell.borrow_mut().split_off(0));
    if opened.is_empty() {
        DRAG_OPEN.with(|cell| cell.borrow_mut().clear());
        MOVE_PUSHED.with(|cell| cell.set(false));
        return;
    }
    let mut cut_ = crate::ui::window::newest_review_cut();
    for span in opened.iter().rev() {
        if !crate::cut_fold::is_folded(&cut_.folds, (span[0], span[1])) {
            cut_.folds.push(*span);
        }
    }
    // `record_edit`, not a bare seed: opening the drag's folds and refolding them is ONE act of the hand,
    // so the refold belongs to the step the move already opened rather than spending one of its own.
    crate::ui::window::record_edit(window, &cut_);
    crate::ui::cut_folds::save_folds(&cut_);
    crate::ui::cut_folds::refresh_fold_badges(window);
    DRAG_OPEN.with(|cell| cell.borrow_mut().clear());
    MOVE_PUSHED.with(|cell| cell.set(false));
}

/// F2.8 S1 ("picture scrubs live · the picture lands on the edge"): where the border the hand just landed
/// on actually is in the cut, so the page asks the PLAYER for the second the picture must show rather than
/// trusting the pixel the pointer reached. [`crate::cut_trim::clamp_edge`] is the only thing that decides
/// a border's place, so reading the segment back after the write is what keeps a scrub past the recording's
/// end, or past a neighbour, from showing a picture the cut does not contain.
pub fn edge_second(index: usize, border: cut_trim::Border) -> f64 {
    let cut_ = crate::ui::window::newest_review_cut();
    match cut_.segs.get(index) {
        Some(seg) => match border {
            cut_trim::Border::Start => seg.s,
            cut_trim::Border::End => seg.e,
        },
        // Nothing there to scrub: the segment went away (a release that joined two scenes) or the index is
        // stale. 0.0 is never asked for, because the caller only scrubs a border it just wrote.
        None => 0.0,
    }
}

/// F2.8 S1 ("picture scrubs live (throttled)"): re-seek the RUNNING media leg to the second the dragged border just landed on, through the same player path every other press
/// uses — `cut_record_play`'s own file list into `cut_play_back::cue_clips`, which is the leg the
/// `NAIVEPOST_PLAYER` seam records. Answers whether anything was re-seeked: with no preview running there
/// is no picture to scrub, and starting one on a trim would be the page inventing playback nobody asked
/// for. The release calls this too, unconditionally, so the LAST edge always reaches the picture even when
/// the [`crate::cut_trim::SCRUB_MS`] throttle swallowed the updates before it.
pub fn scrub_picture(window: &adw::ApplicationWindow, at_secs: f64) -> bool {
    if !crate::ui::window::preview_player(window).transport.playing {
        return false;
    }
    if crate::cut_play_back::playing_pid().is_none() {
        return false;
    }
    let files = crate::ui::cut_record_play::files_for(window, &crate::ui::cut_record_play::runs_now());
    if files.is_empty() {
        return false;
    }
    crate::cut_play_back::cue_clips(&files, at_secs, &|line| crate::ui::window::log_line(line)).is_ok()
}
