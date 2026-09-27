//! F3.8 Hold, move, resize, edit, remove — `spec/06-effects.md` F3.8 and `spec/inventory/effects.md` §A.9.
//!
//! The rules of working with an effect that is ALREADY on the lane, as opposed to laying a new one down: what a
//! press on a band means, how far it may go, when the hand is still holding it, how ✎ Edit finds its way back to
//! the record it was opened on, and when a live form has said enough to be worth an Undo.
//!
//! Most of these numbers already belong to another module and are REACHED THROUGH IT rather than restated here —
//! a rule written twice is a rule that can be changed in one place only. What this module adds is the pick-up /
//! hold-release / 2-px-undo thresholds §A.9 gives the lane and nobody else had written down, plus the sentences.
//! Borrowed, not copied: the click slop [`cut_select::DRAG_SLOP_PX`], the snap reach
//! [`cut_select::SNAP_PX`] (`layout.snapPx`) applied by [`cut_select::snap`], the grip reach
//! [`cut_select::GRIP_PX`], the length floor [`fx_lane::MIN_BAND_SECONDS`] (`P.eng.effectMinSeconds`) applied
//! by [`fx_lane::drag_end`], the edit reach [`effect_rules::REACH_SECONDS`] and the miss sentence
//! [`effect_rules::GONE`] applied by [`effect_rules::find_by_value`], and the one-Undo-per-gesture rule
//! [`cut_trim::pushes_undo`].

use crate::cut::Fx;
use crate::cut_select;
use crate::cut_trim;
use crate::effect_rules;
use crate::fx_lane;

// --- S1: what a press on a band means ------------------------------------------------------------------------------

/// F3.8 S1 (`ends are grips when the band is ≥ 30 px`): §A.9's "end grips only when ≥ 30 px wide (6 px
/// reach)". The width is the bar's own drawn width; the 6 px REACH inside it is [`cut_select::GRIP_PX`], which
/// §10 lists among the "grab reaches" — reached through, never re-declared here. The 30 px itself has no `P.`
/// row in §10 (`params.rs` notes this width "stays unowned"), so it takes a bare prefix:
/// `effects.gripMinPx`.
pub const GRIP_MIN_PX: f64 = 30.0;

/// F3.8 S3 (`The ✕ in the band's middle (≥ 32 px)`): §A.9's "✕ in the middle only when ≥ 32 px". Two pixels
/// wider than the grip threshold on purpose: a kill affordance must not sit where the hand was reaching for an
/// edge, so the bar has to be past the grip zone before a ✕ appears in what is left of its middle. Also unowned
/// by §10 → bare prefix `effects.killMinPx`.
pub const KILL_MIN_PX: f64 = 32.0;

/// F3.8 S3 (`undo pushed on the first 2 px of travel`): the travel at which a drag stops being a hover and
/// becomes an edit worth unwinding. Under it nothing has moved; over it the gesture owns one Undo entry, taken at
/// the crossing so a 200 px drag does not cost 100 entries. Bare prefix `effects.undoPushPx`.
pub const UNDO_PUSH_PX: f64 = 2.0;

/// F3.8 S2 (`Hold drops when the line walks > 1/24 s off the band`): §A.9 verbatim. 1/24 s is one frame at
/// 24 fps, and the slack exists because the clock keeps running while something is in hand: a held band whose
/// edge crosses the line by a single frame must not vanish from under the pointer mid-drag, or the hand is left
/// dragging nothing. Past a frame outside, the hold is genuinely behind the viewer and drops. §10 gives this no
/// row → bare prefix `machine.holdReleaseSeconds`.
pub const HOLD_RELEASE_SECONDS: f64 = 1.0 / 24.0;

/// F3.8 S5 (`every keystroke lands after a debounce`): §A.9 names the behaviour but no number. 300 ms chosen
/// as long enough that a burst of typing produces ONE write rather than one per character, and short enough that
/// the form still feels live — the pause is felt as the app keeping up, not as lag. §10 has no row for it → bare
/// prefix `effects.formDebounceMs`.
pub const DEBOUNCE_MS: f64 = 300.0;

/// F3.8 S5 (`"Kept as you type — ↶ Undo takes the whole edit back."`): the note every live form carries. The
/// volume and label forms already print this sentence as their footer literal; it belongs HERE, in the module that
/// owns the live-form rule, so the two cannot drift apart from the promise the Undo behaviour makes.
pub const KEPT_AS_YOU_TYPE: &str =
    "Kept as you type \u{2014} \u{21b6} Undo takes the whole edit back.";

/// F3.8 S1/S3: what a press on the lane turned out to be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Press {
    /// Under [`cut_select::DRAG_SLOP_PX`] of travel: a click, and a click on a band opens its form (§A.9).
    ClickForm,
    /// A real drag on a band: it is picked up, starting at `t`. `from_end` says where the hand grabbed it —
    /// `None` for the middle (the band slides), `Some(start)` / `Some(end)` for a grip (that end resizes).
    Hold { t: f64, from_end: Option<bool> },
    /// The ✕ was pressed on a bar wide enough to carry one.
    Kill,
    /// Nothing under the pointer: the empty lane drops whatever was in hand (§A.9 "empty lane → drop").
    Dropped,
}

/// F3.8 S1 (`under 4 px of travel is a click`): the band's own slop, which is §A.9's "drag under 4 px is a
/// click" and nothing else. `cut_trim::is_click` deliberately does NOT answer this — its threshold is
/// `ROW_TRAVEL_PX` (3 px), because F2.8's clip drag opens its gesture one pixel earlier than the selection's.
/// Reusing it here would make a 3 px jitter on a band a MOVE when the spec says that press is a click. So the
/// reach is read through [`cut_select::is_drag`] (`moved_px > DRAG_SLOP_PX`, i.e. over 4 px) rather than being
/// written again as a literal: the number stays in the one module that owns the slop.
fn is_band_drag(travel_px: f64) -> bool {
    cut_select::is_drag(travel_px)
}

/// F3.8 S1/S3 (`Press: ✕ → kill; band → hold; empty lane → drop` + `Drag under 4 px is a click`) + S2
/// (`where did the drag start?`): what a press answers, in the order the flowchart asks.
///
/// The ✕ is checked FIRST because a kill is deliberate and final within the gesture: someone who aimed at the
/// cross did not mean to pick the band up, even if the pointer travelled far afterwards. Then whether there was
/// a band at all — pressing empty lane is how you put something down without Esc. Only then does travel decide,
/// by [`is_band_drag`] — 4 px, §A.9's own number, not F2.8's 3 px.
/// between a click (open the form) and a drag (pick it up), through [`cut_trim::is_click`], which is the same
/// between a click (open the form) and a drag (pick it up). `grab` is carried into the
/// `Hold` so the caller knows whether to slide or resize without asking the geometry again.
pub fn press_band(t: f64, width_px: f64, travel_px: f64, over_kill: bool, lane_hit: bool, grab: Option<bool>) -> Press {
    if over_kill && kill_open(width_px) {
        return Press::Kill;
    }
    if !lane_hit {
        return Press::Dropped;
    }
    if !is_band_drag(travel_px) {
        return Press::ClickForm;
    }
    Press::Hold { t, from_end: grab }
}

/// F3.8 S2 (`where did the drag start? the middle / an end`): which part of a bar `x` (px from its left edge)
/// is a grip of, given the bar's width. `Some(true)` = the left end, `Some(false)` = the right end, `None` = the
/// middle. The reach inside the bar is [`cut_select::GRIP_PX`] (6 px, one of §10's "grab reaches") and the bar
/// must be wide enough to have grips at all ([`grips_open`]) — below [`GRIP_MIN_PX`] every pixel of the bar is
/// a handle for moving, never for resizing, because a 12 px bar whose last 6 px were a grip gets resized every
/// time someone grabs it to move it.
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

/// F3.8 S1 (`ends are grips when the band is ≥ 30 px`): does this bar offer its ends at all? Below the
/// threshold the whole bar is a handle for moving and nothing is a handle for resizing — a 12 px bar whose last
/// 6 px were a grip would be resized by accident every time it was grabbed.
pub fn grips_open(width_px: f64) -> bool {
    width_px >= GRIP_MIN_PX
}

/// F3.8 S3 (`✕ in the middle only when ≥ 32 px`): does this bar have room for a ✕ that is not inside either
/// grip zone? Kept as its own predicate beside [`grips_open`] so the 30 / 32 distinction is a tested fact rather
/// than a coincidence of two numbers happening to differ.
pub fn kill_open(width_px: f64) -> bool {
    width_px >= KILL_MIN_PX
}

// --- S2: moving, resizing, nudging -------------------------------------------------------------------------------

/// F3.8 S2 (`both ends snap within 8 px`): slide the whole band so it starts at `to`, snapped against `marks`
/// and clamped inside `[0, session_end]`.
///
/// [`cut_select::snap_span`] is the span form of the rule ("the nearer end wins, and both ends get their chance
/// within the reach"), which is what §A.9 asks for when a whole band slides; [`cut_select::snap`] would only ever
/// pull the start. The exclusion is the other half: `marks` arrives with **the moved effect's own two ends
/// filtered out** by the caller, because §A.9 says "both ends offered when sliding; the held effect's own ends
/// excluded" — a band that snaps to itself cannot be dragged at all once it lines up with its own start.
/// Snapping stays pixel-constant through SNAP_PX (`layout.snapPx`), so the reach in seconds follows the zoom
/// level instead of being a fixed slice of film.
pub fn move_whole(fx: &Fx, to: f64, marks: &[f64], pps: f64, session_end: f64) -> (f64, f64) {
    let snapped = cut_select::snap_span(to, fx.dur, marks, pps);
    // Clamped so the whole band stays inside the timeline: a start below 0 is refused by the clamp rather than
    // answered negatively, and a start that would run the tail past the end is pulled back to fit exactly.
    let latest = (session_end - fx.dur).max(0.0);
    let start = snapped.clamp(0.0, latest);
    (start, fx.dur)
}

/// F3.8 S2 (`an end ... that end moves · length ≥ 0.1 s`): delegate to [`fx_lane::drag_end`], which holds
/// `MIN_BAND_SECONDS` (**P.eng.effectMinSeconds**) and keeps the untouched end where it was. One floor rule for
/// every kind and every entry point, which is why nothing here repeats the arithmetic.
pub fn resize_end(fx: &Fx, end: bool, to: f64) -> (f64, f64) {
    fx_lane::drag_end(fx.t, fx.dur, end, to)
}

/// F3.8 S2 (`Frame steps and ←/→ nudge a held effect unsnapped`): the keyboard's move. Routed through
/// [`effect_rules::nudge`], whose doc states the rule as what it does NOT do — no marks, no reach, no pixels —
/// so a nudge walked past a segment end stays exactly one frame off it. Length is untouched: the keyboard moves
/// a band, it does not stretch one.
pub fn nudged(fx: &Fx, delta: f64) -> (f64, f64) {
    (effect_rules::nudge(fx.t, delta), fx.dur)
}

/// F3.8 S2 (`Hold drops when the line walks > 1/24 s off the band`): is the hand still holding? False anywhere
/// inside the band, and false within [`HOLD_RELEASE_SECONDS`] of either edge; true only beyond that slack.
pub fn hold_drops(t: f64, dur: f64, line: f64) -> bool {
    line < t - HOLD_RELEASE_SECONDS || line > t + dur + HOLD_RELEASE_SECONDS
}

/// F3.8 S3 (`undo pushed on the first 2 px of travel`): does THIS update take the gesture's one Undo entry?
/// Delegated to [`cut_trim::pushes_undo`] for the once-only half; the 2 px threshold is the "did it move"
/// argument, since §A.9 fixes the distance rather than leaving it at any non-zero travel.
pub fn undo_pushed(travel_px: f64, already_pushed: bool) -> bool {
    cut_trim::pushes_undo(travel_px >= UNDO_PUSH_PX, already_pushed)
}

// --- S4: ✎ Edit finding its way back -----------------------------------------------------------------------------

/// F3.8 S4 (`Edit re-finds the effect by kind and start (|Δt| < 1 ms)`): which record the reopened form is
/// talking about. Straight through [`effect_rules::find_by_value`] with [`effect_rules::REACH_SECONDS`]: by
/// value, never by index, because the page does not hold the lane still while a dialog is open.
pub fn refind(fx: &[Fx], kind: &str, t: f64) -> Option<usize> {
    effect_rules::find_by_value(fx, kind, t)
}

/// F3.8 S4 (`box always from the live effect, never the form's snapshot`): the four fractions a form must show.
/// The `form_snapshot` argument exists ONLY to make the rule visible in a test and at the call site — whatever it
/// holds, the answer comes off `live`. A form that redrew from its own earlier copy would happily overwrite a
/// box someone moved since it opened, which is the exact mistake this clause forbids.
pub fn live_box(live: &Fx, _form_snapshot: Option<(f64, f64, f64, f64)>) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    (live.cx, live.cy, live.wf, live.hf)
}

/// F3.8 S4 (`"that effect is no longer in the cut — nothing was changed"`): the miss sentence, reached through
/// [`effect_rules::GONE`] rather than rewritten, so the insert's sibling wording and this one stay separate facts.
pub const GONE: &str = effect_rules::GONE;

// --- S5: forms are live -----------------------------------------------------------------------------------------

/// F3.8 S5 (`Forms are live: first answer pushes Undo`): one form visit, one Undo entry, until a refusal
/// resets it. Held by the form drawer, not by each field, because "the whole edit back" means the visit — every
/// keystroke after the first rides on the entry the first answer took.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveForm {
    /// Has this visit already taken its Undo entry?
    pub undo_pushed: bool,
}

impl LiveForm {
    /// F3.8 S5: is THIS the answer that pushes Undo? True the first time, false for every change after it.
    pub fn first_answer(&mut self) -> bool {
        if self.undo_pushed {
            return false;
        }
        self.undo_pushed = true;
        true
    }

    /// F3.8 S5 (§A.9 "refusals reset that"): a refused answer gives the Undo entry back, so the next accepted
    /// one is again the first. A refusal wrote nothing, so it must not have spent the entry either — otherwise a
    /// person refused once could never unwind what they type afterwards.
    pub fn refusal_resets(&mut self) {
        self.undo_pushed = false;
    }
}

/// F3.8 S5 (`every keystroke lands after a debounce`): has the pause since the last keystroke run out?
/// `pending_ms` is the time since the most recent keystroke, so the write happens when it reaches
/// [`DEBOUNCE_MS`] and not before.
pub fn keystroke_lands(pending_ms: f64, debounce_ms: f64) -> bool {
    pending_ms >= debounce_ms
}

// --- the sentences ----------------------------------------------------------------------------------------------

/// F3.8 S1 (`status "<label> picked up"`): §A.9 verbatim. No em dash and no Undo clause — picking something up
/// has changed nothing yet, so saying more would promise an edit that has not happened.
pub fn picked_up(label: &str) -> String {
    format!("{label} picked up")
}

/// F3.8 S3 (`"removed <label> — ↶ Undo takes it back"`): §A.9's kill sentence, whole. The clause is load-bearing
/// rather than decorative: removal goes through `record_edit`, so ↶ really does restore the record.
pub fn removed(label: &str) -> String {
    format!("removed {label} \u{2014} \u{21b6} Undo takes it back")
}

/// F3.8 S3 (`"<label> moved — ↶ Undo takes it back"`): a band slid to a new second. Same tail as the kill,
/// because the same single Undo covers it.
pub fn moved(label: &str) -> String {
    format!("{label} moved \u{2014} \u{21b6} Undo takes it back")
}

/// F3.8 S3 (`"<label> resized — …"`): a band's end dragged to a new length.
pub fn resized(label: &str) -> String {
    format!("{label} resized \u{2014} \u{21b6} Undo takes it back")
}

/// F3.8 S3 (`Esc drops everything and disarms ("cancelled" for a bare disarm)`): what the page says when Escape
/// gave something up. One word, because the action is a reflex and the sentence only has to confirm that the key
/// was heard.
pub const CANCELLED: &str = "cancelled";
