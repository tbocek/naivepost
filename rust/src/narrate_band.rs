//! §07-narrate#1-screen, the take band's arithmetic (§1's number **10**, inventory §A.7).

use crate::narrate_screen::{self as screen, BAND_MAX_PPS, TAKE_LABEL_MIN_PX};

/// Zoom 1 is "the whole recording fits": fit px/s × this factor.
pub const ZOOM_NOTCH: f64 = 1.25;

/// Shift+wheel pans by w/8 of the visible width (§A.7: "Shift+wheel pans by w/8").
pub const PAN_DIVISOR: f64 = 8.0;

/// Pixels per second at `zoom`, capped at [`BAND_MAX_PPS`] ("max 200 px/s").
pub fn px_per_second(zoom: f64, duration: f64, width: f64) -> f64 {
    (fit_pps(duration, width) * zoom.max(0.0)).min(BAND_MAX_PPS)
}

/// The band's zoom after one wheel notch of `dy` (`scroll.get_dy()`: negative scrolls up = zoom in).
pub fn zoom_step(zoom: f64, wheel_dy: f64) -> f64 {
    (zoom / ZOOM_NOTCH.powf(wheel_dy)).max(1.0)
}

/// The px/s that puts the whole recording across `width`.
pub fn fit_pps(duration: f64, width: f64) -> f64 {
    if duration <= 0.0 || width <= 0.0 {
        return 1.0;
    }
    width / duration
}

/// Does the view need its own scrollbar?
pub fn needs_scrollbar(zoom: f64, duration: f64, width: f64) -> bool {
    content_px(zoom, duration, width) > width + f64::EPSILON
}

/// How far one Shift+wheel notch moves the view, in px (§A.7's w/8).
pub fn pan_by(width: f64) -> f64 {
    (width / PAN_DIVISOR).max(1.0)
}

/// The widest the drawn content can be: `duration × px_per_second`.
pub fn content_px(zoom: f64, duration: f64, width: f64) -> f64 {
    duration * px_per_second(zoom, duration, width)
}

/// Keep the view inside the recording: the left edge never goes below 0 and never past the point
/// where less than a screenful of file remains (unless the file is shorter than the band).
pub fn clamp_view(offset: f64, zoom: f64, duration: f64, width: f64) -> f64 {
    let span = content_px(zoom, duration, width);
    if span <= width {
        return 0.0;
    }
    offset.clamp(0.0, span - width)
}

/// A pixel in the band's own coordinates → the second of the recording it sits on.
pub fn x_to_second(x: f64, offset: f64, pps: f64) -> f64 {
    ((x + offset) / pps.max(f64::EPSILON)).max(0.0)
}

/// A second of the recording → the px it is drawn at, never left of the lane's start.
pub fn second_to_px(t: f64, offset: f64, pps: f64) -> f64 {
    (t * pps - offset).max(0.0)
}

/// Is a take wide enough to carry its duration?
pub fn take_shows_label(width_px: f64) -> bool {
    width_px > TAKE_LABEL_MIN_PX
}

/// Where a take is drawn: its left px and its width px inside the visible lane, both clipped to
/// what the band can actually show (a take partly scrolled off still paints the part on screen).
pub fn take_box(take: (f64, f64), offset: f64, pps: f64, width: f64) -> (f64, f64) {
    let left = take.0 * pps - offset;
    let right = take.1 * pps - offset;
    let l = left.max(0.0);
    let r = right.min(width);
    (l, (r - l).max(0.0))
}

/// The label a take carries: its duration ALONE (§1's `"4.2s"`), spelled by the same helper the
/// status line uses so the two can never disagree about rounding.
pub fn label_in(take: (f64, f64)) -> String {
    screen::take_label(take.1 - take.0)
}

/// The faint red bar under the pointer: the px it is drawn at, or `None` when the pointer is off the
/// band (negative x, or past its right edge).
pub fn pointer_bar(x: f64, width: f64) -> Option<f64> {
    (x >= 0.0 && x <= width).then_some(x)
}

/// The playhead while ▶ walks the cut: its px, or `None` while nothing plays (§1: "playhead while ▶
/// walks") — a parked preview draws no playhead over the takes.
pub fn playhead_px(session: f64, offset: f64, pps: f64, playing: bool) -> Option<f64> {
    playing
        .then_some(session * pps - offset)
        .filter(|px| *px >= 0.0)
}

/// What the band says while its envelope is not decoded yet (§1: "reading the recording…" while the
/// envelope decodes).
pub fn envelope_note(decoded: bool) -> Option<&'static str> {
    (!decoded).then_some("reading the recording\u{2026}")
}

/// The wave as the lane draws it: one `(x, peak)` per pixel column in `width` px, the loudest peak the column
/// covers.
pub fn peak_columns(hz: f64, peaks: &[u8], offset: f64, pps: f64, width: f64) -> Vec<(f64, u8)> {
    if peaks.is_empty() || hz <= 0.0 || pps <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut x = 0.0;
    while x < width {
        let first = (x_to_second(x, offset, pps) * hz).floor().max(0.0) as usize;
        let last = (x_to_second(x + 1.0, offset, pps) * hz).ceil().max(0.0) as usize;
        if first >= peaks.len() {
            break;
        }
        let covered = &peaks[first..last.max(first + 1).min(peaks.len())];
        out.push((x, covered.iter().copied().max().unwrap_or(0)));
        x += 1.0;
    }
    out
}

/// The band's paint plan: everything the draw func needs, decided from numbers alone, so a test can
/// assert the picture without a cairo surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The note shown instead of a waveform while the envelope decodes.
    pub note: Option<String>,
    /// The red ▶-start bar, drawn through the ruler too (§1).
    pub red_bar_px: f64,
    /// The faint red bar under the pointer.
    pub pointer_px: Option<f64>,
    /// The blue live selection, as (left, width) px.
    pub selection_px: Option<(f64, f64)>,
    /// Each take: (left, width, label-or-empty).
    pub takes: Vec<(f64, f64, String)>,
    /// The playhead while ▶ walks.
    pub playhead_px: Option<f64>,
    /// Whether the lane needs its own horizontal scrollbar.
    pub scrollable: bool,
}

/// What one frame of the band is asked to show: the view (`zoom`, `offset`, `pps` derived from them),
/// the hand (`pointer_x`, `selection` in seconds, `red_bar`), and what is on it (`takes`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BandPaint {
    /// Seconds of recording the lane covers — the denominator of fit.
    pub duration: f64,
    /// The lane's width in px, as allocated.
    pub width: f64,
    /// Fit-relative zoom, 1.0 = the whole recording fits.
    pub zoom: f64,
    /// Px scrolled from the recording's start.
    pub offset: f64,
    /// Pixels per second for this view (`px_per_second(zoom, duration, width)`).
    pub pps: f64,
    /// Where the red ▶-start bar sits, in seconds.
    pub red_bar: f64,
    /// Pointer x in lane px; negative means off the band.
    pub pointer_x: f64,
    /// The drag in progress, in seconds.
    pub selection: Option<(f64, f64)>,
    /// The takes, already cleaned (sorted, ≥ P.eng.takeMinSeconds, merged).
    pub takes: Vec<(f64, f64)>,
    /// Session second ▶ has reached.
    pub session: f64,
    /// Is ▶ walking the cut right now?
    pub playing: bool,
    /// Has the waveform cache been decoded for this recording?
    pub envelope_decoded: bool,
}

/// Decide the whole picture from state.
pub fn paint_plan(state: &BandPaint) -> Plan {
    Plan {
        note: envelope_note(state.envelope_decoded).map(str::to_string),
        red_bar_px: second_to_px(state.red_bar, state.offset, state.pps),
        pointer_px: pointer_bar(state.pointer_x, state.width),
        selection_px: state
            .selection
            .map(|(from, to)| take_box((from, to), state.offset, state.pps, state.width)),
        takes: state
            .takes
            .iter()
            .map(|take| {
                let (left, w) = take_box(*take, state.offset, state.pps, state.width);
                let label = if take_shows_label(w) {
                    label_in(*take)
                } else {
                    String::new()
                };
                (left, w, label)
            })
            .collect(),
        playhead_px: playhead_px(state.session, state.offset, state.pps, state.playing),
        scrollable: needs_scrollbar(state.zoom, state.duration, state.width),
    }
}
