//! F3.8 Hold, move, resize, edit, remove (`spec/06-effects.md` F3.8, `spec/inventory/effects.md` §A.9): working with an effect already on the lane: what a press means, how far it may go, when the hand still holds it, how ✎ Edit finds its record, and when a live form is worth an Undo.
//! Most numbers are reached through their owners, not restated: [`cut_select::DRAG_SLOP_PX`], [`cut_select::SNAP_PX`] (`layout.snapPx`) via [`cut_select::snap`], [`cut_select::GRIP_PX`], [`fx_lane::MIN_BAND_SECONDS`] (`P.eng.effectMinSeconds`) via [`fx_lane::drag_end`], [`effect_rules::REACH_SECONDS`], [`effect_rules::GONE`] via [`effect_rules::find_by_value`], [`cut_trim::pushes_undo`]. This adds the pick-up / hold-release / 2-px-undo thresholds and the sentences.

use crate::cut::Fx;
use crate::cut_select;
use crate::cut_trim;
use crate::effect_rules;
use crate::fx_lane;

// --- S1: what a press on a band means ------------------------------------------------------------------------------

/// F3.8 S1 (`ends are grips when the band is ≥ 30 px`): the bar's drawn width; the 6 px reach inside is [`cut_select::GRIP_PX`]. Unowned by §10: `effects.gripMinPx`.
pub const GRIP_MIN_PX: f64 = 30.0;

/// F3.8 S3 (`The ✕ in the band's middle (≥ 32 px)`): two px wider than the grip threshold so a ✕ never sits where the hand reaches for an edge. Unowned by §10: `effects.killMinPx`.
pub const KILL_MIN_PX: f64 = 32.0;

/// F3.8 S3 (`undo pushed on the first 2 px of travel`): where a drag becomes an edit; the gesture takes one Undo at the crossing. `effects.undoPushPx`.
pub const UNDO_PUSH_PX: f64 = 2.0;

/// F3.8 S2 (`Hold drops when the line walks > 1/24 s off the band`): one frame at 24 fps of slack, since the clock keeps running while something is in hand. Unowned by §10: `machine.holdReleaseSeconds`.
pub const HOLD_RELEASE_SECONDS: f64 = 1.0 / 24.0;

/// F3.8 S5 (`every keystroke lands after a debounce`): §A.9 gives no number; 300 ms makes a burst one write yet stays live. `effects.formDebounceMs`.
pub const DEBOUNCE_MS: f64 = 300.0;

/// F3.8 S5: the note every live form carries (`"Kept as you type — ↶ Undo takes the whole edit back."`); owned here so the forms' footers cannot drift from the Undo behaviour.
pub const KEPT_AS_YOU_TYPE: &str =
    "Kept as you type \u{2014} \u{21b6} Undo takes the whole edit back.";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Press {
    ClickForm,
    /// A real drag on a band: picked up at `t`. `from_end` is `None` for the middle (slides), `Some(start)`/`Some(end)` for a grip (that end resizes).
    Hold {
        t: f64,
        from_end: Option<bool>,
    },
    Kill,
    Dropped,
}

/// F3.8 S1/S3 (`Press: ✕ → kill; band → hold; empty lane → drop` + `Drag under 4 px is a click`) + S2: what a press answers, in the flowchart's order. The ✕ is checked FIRST (a kill is deliberate); then whether there was a band at all; only then travel decides click vs drag via [`cut_select::is_drag`] (4 px, not F2.8's 3). `grab` is carried into the `Hold`.
pub fn press_band(
    t: f64,
    width_px: f64,
    travel_px: f64,
    over_kill: bool,
    lane_hit: bool,
    grab: Option<bool>,
) -> Press {
    if over_kill && kill_open(width_px) {
        return Press::Kill;
    }
    if !lane_hit {
        return Press::Dropped;
    }
    if !cut_select::is_drag(travel_px) {
        return Press::ClickForm;
    }
    Press::Hold { t, from_end: grab }
}

/// F3.8 S2 (`where did the drag start? the middle / an end`): which part of a bar `x` (px from its left edge) grips: `Some(true)` left end, `Some(false)` right end, `None` middle. Reach is [`cut_select::GRIP_PX`]; below [`GRIP_MIN_PX`] (see [`grips_open`]) the whole bar only moves, else a narrow bar gets resized when grabbed.
pub fn grab_at(x_px: f64, width_px: f64) -> Option<bool> {
    if !grips_open(width_px) {
        return None;
    }
    if x_px <= cut_select::GRIP_PX {
        Some(true)
    } else if x_px >= width_px - cut_select::GRIP_PX {
        Some(false)
    } else {
        None
    }
}

/// F3.8 S1: does this bar offer its ends at all? Below the threshold the whole bar only moves.
pub fn grips_open(width_px: f64) -> bool {
    width_px >= GRIP_MIN_PX
}

/// F3.8 S3: does this bar have room for a ✕ outside both grip zones? A separate predicate from [`grips_open`] so 30 vs 32 is a tested fact.
pub fn kill_open(width_px: f64) -> bool {
    width_px >= KILL_MIN_PX
}

/// F3.8 S2 (`both ends snap within 8 px`): slide the band to start at `to`, snapped against `marks` and clamped inside `[0, session_end]`. [`cut_select::snap_span`] gives the nearer end its chance (plain `snap` would only pull the start). The caller filters **the moved effect's own ends** out of `marks` (§A.9), else a band snaps to itself. Reach is pixel-constant (SNAP_PX), so it follows the zoom.
pub fn move_whole(fx: &Fx, to: f64, marks: &[f64], pps: f64, session_end: f64) -> (f64, f64) {
    let snapped = cut_select::snap_span(to, fx.dur, marks, pps);
    // A start below 0 is clamped; one that would run the tail past the end is pulled back to fit.
    let latest = (session_end - fx.dur).max(0.0);
    let start = snapped.clamp(0.0, latest);
    (start, fx.dur)
}

/// F3.8 S2 (`an end ... that end moves · length ≥ 0.1 s`): delegates to [`fx_lane::drag_end`], which holds `MIN_BAND_SECONDS` (**P.eng.effectMinSeconds**) and keeps the other end.
pub fn resize_end(fx: &Fx, end: bool, to: f64) -> (f64, f64) {
    fx_lane::drag_end(fx.t, fx.dur, end, to)
}

/// F3.8 S2 (`Frame steps and ←/→ nudge a held effect unsnapped`): via [`effect_rules::nudge`] (no marks, no reach, no pixels); length untouched.
pub fn nudged(fx: &Fx, delta: f64) -> (f64, f64) {
    (effect_rules::nudge(fx.t, delta), fx.dur)
}

/// F3.8 S2 (`Hold drops when the line walks > 1/24 s off the band`): does this window's hold still stand at `line_t`? True inside the band and within [`HOLD_RELEASE_SECONDS`] of either edge, since the clock keeps running mid-drag. The pick-up path cannot release itself: §S1 puts the line ON the band's start.
pub fn hold_survives(line_t: f64, band_t: f64, band_dur: f64) -> bool {
    !hold_drops(band_t, band_dur, line_t)
}

/// F3.8 S2: is the hand still holding? False inside the band and within [`HOLD_RELEASE_SECONDS`] of an edge; true only beyond.
pub fn hold_drops(t: f64, dur: f64, line: f64) -> bool {
    line < t - HOLD_RELEASE_SECONDS || line > t + dur + HOLD_RELEASE_SECONDS
}

/// F3.8 S3 (`undo pushed on the first 2 px of travel`): does this update take the gesture's one Undo? Once-only via [`cut_trim::pushes_undo`]; the 2 px is the "did it move" argument.
pub fn undo_pushed(travel_px: f64, already_pushed: bool) -> bool {
    cut_trim::pushes_undo(travel_px >= UNDO_PUSH_PX, already_pushed)
}

/// F3.8 S4 (`box always from the live effect, never the form's snapshot`): the four fractions a form shows. `form_snapshot` exists only to make the rule visible; the answer comes off `live`.
pub fn live_box(
    live: &Fx,
    _form_snapshot: Option<(f64, f64, f64, f64)>,
) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    (live.cx, live.cy, live.wf, live.hf)
}

/// F3.8 S4: the miss sentence, via [`effect_rules::GONE`].
pub const GONE: &str = effect_rules::GONE;

/// F3.8 S5 (`Forms are live: first answer pushes Undo`): one Undo entry per form visit, held by the form drawer, until a refusal resets it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveForm {
    pub undo_pushed: bool,
}

impl LiveForm {
    pub fn first_answer(&mut self) -> bool {
        if self.undo_pushed {
            return false;
        }
        self.undo_pushed = true;
        true
    }

    /// F3.8 S5 (§A.9 "refusals reset that"): a refused answer gives the Undo entry back, since it wrote nothing.
    pub fn refusal_resets(&mut self) {
        self.undo_pushed = false;
    }
}

/// F3.8 S5 (`every keystroke lands after a debounce`): has `pending_ms` since the last keystroke reached [`DEBOUNCE_MS`]?
pub fn keystroke_lands(pending_ms: f64, debounce_ms: f64) -> bool {
    pending_ms >= debounce_ms
}

/// F3.8 S1 (`status "<label> picked up"`): §A.9 verbatim; no Undo clause since nothing has changed yet.
pub fn picked_up(label: &str) -> String {
    format!("{label} picked up")
}

/// F3.8 S3 (`"removed <label> — ↶ Undo takes it back"`): §A.9's kill sentence; removal goes through `record_edit`, so ↶ restores the record.
pub fn removed(label: &str) -> String {
    format!("removed {label} \u{2014} \u{21b6} Undo takes it back")
}

/// F3.8 S3 (`"<label> moved — ↶ Undo takes it back"`): same tail as the kill.
pub fn moved(label: &str) -> String {
    format!("{label} moved \u{2014} \u{21b6} Undo takes it back")
}

pub fn resized(label: &str) -> String {
    format!("{label} resized \u{2014} \u{21b6} Undo takes it back")
}

/// F3.8 S3 (`Esc drops everything and disarms ("cancelled" for a bare disarm)`): what the page says when Escape gave something up; one word.
pub const CANCELLED: &str = "cancelled";
