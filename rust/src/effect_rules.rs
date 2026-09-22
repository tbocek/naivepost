//! §06-effects#7-rules — the standing rules of the effects page, `spec/06-effects.md` §7 and
//! `spec/inventory/effects.md` §G.
//!
//! Like [`crate::cut_rules`] for the Cut page, this module states the handful of §7's clauses nothing else had
//! written down and reaches through its own module for the rest — a rule restated twice is a rule that can be
//! changed in one place only. In §7's order:
//!
//! * one list, one owner — [`crate::fx_lane::MENU_ORDER`] is the single order of the kinds and
//!   [`crate::fx_lane::rows_for_effects`] packs that one list into rows;
//! * nothing reaches backwards — an effect answers at and after its `t`: [`crate::fx_zoom::camera_at`],
//!   [`crate::cut_speed::rate_at`];
//! * dur is the bar for every kind — [`crate::fx_record::uses`] with [`crate::fx_record::Field::Dur`], true of all six;
//! * one fade rule — [`crate::cut_speed::clamp_fades`], which every kind's `apply` routes its two fades through;
//! * rates average — [`crate::cut_speed::rate_spans`]; gains multiply — [`crate::cut_hear::gain_under`]; sound
//!   answers do not merge — [`crate::cut_speed_pass::merge`] holds two stretches apart when their answers differ;
//! * a stop is not a rate — [`crate::fx_record::is_stop`], [`crate::cut_speed::applied_rate`] and
//!   [`crate::render_fx::setpts`], which gives a held frame no filter at all;
//! * staircases are built whole or not at all — [`crate::cut_speed::ramps`], answering `(0.0, 0.0)` for a pair of
//!   ramps whose stairs cannot all be rendered;
//! * slivers are healed, not dropped — [`crate::cut_speed::heal`];
//! * only speed touches the clip list — [`crate::tools::clips::Clips::set_clip_speed`] is the one tool that writes a
//!   rate; an effect of any other kind lands in the effect list and leaves the clips alone;
//! * camera windows have no width, overlay boxes do — [`crate::fx_record::uses`] with `Field::Wf` and
//!   [`crate::fx_record::fractions_read_off_source_frame`]: a window's width is the frame's aspect read off its height;
//! * the box belongs to the picture, not the form — [`crate::fx_text::Box_::clamp`], applied wherever a box arrives;
//! * forms find effects by value — [`find_by_value`] here;
//! * one Undo per visit, drag or hold, and a click is not a drag — [`crate::cut_trim::pushes_undo`],
//!   [`crate::cut_trim::opens_gesture`], [`crate::cut_trim::is_click`];
//! * empty effects are never placed — [`crate::fx_text::NO_WORDS`], [`crate::fx_svg::NO_FILE`], and the volume a tool
//!   refuses at gain 1;
//! * framing is done paused — [`crate::fx_zoom::preview_press`] for the press, [`grab`] here for what it takes hold of;
//! * hit order on the picture — [`grab`] here;
//! * snapping is pixel-constant and nudging unsnapped — [`crate::fx_text::snap_to`] and [`crate::cut_select::SNAP_PX`]
//!   for the drag, [`nudge`] here for the keyboard;
//! * effects cannot leave the timeline — [`crate::tools::clips::Clips::add_effect`]'s clamp into the clip's seconds and
//!   [`crate::cut_select::move_band`], which will not answer a negative start;
//! * the lane draws what the render does — one record read twice, [`crate::fx_text`] for the fit and
//!   [`crate::render_fx::overlay_chain`] for the same fades as filters;
//! * static card files are finished pictures — [`crate::cut_cards::starts_at_zero_and_waits`],
//!   [`crate::cut_cards::is_frame_size`];
//! * a path with `?` is not a file — [`crate::cut::Seg::insert_asset`] and [`crate::cut::Seg::params`] split the query
//!   off, and [`crate::cut_cards::esc`] writes a value that cannot pass for one;
//! * suggested effects are clamped against the cut as applied — [`crate::cut_clamp::clamp_to_cut`].

use crate::cut::Fx;

/// How far from the second a form was opened at an effect still counts as the same effect. §F3.8 S4 writes it as
/// `|Δt| < 1 ms`: wide enough that a re-saved float is found, narrow enough that a nudge — one frame, or 40 ms — has
/// already moved the effect out from under the form and the page says so rather than editing its neighbour.
pub const REACH_SECONDS: f64 = 0.001;

/// §F3.8 S4 (`"that effect is no longer in the cut — nothing was changed"`): what the page says when ✎ Edit could
/// not find its effect again. Says `nothing was changed` because that is the promise: a form that had applied its
/// answer to whatever now sits at that second would have edited an effect nobody opened it on. The em dash is §7's,
/// as in every sentence this tree shows or logs; [`crate::cut_insert::GONE`] is the insert's sibling wording.
pub const GONE: &str = "that effect is no longer in the cut \u{2014} nothing was changed";

/// §F3.8 S4 (`Edit re-finds the effect by kind and start`): which record a form opened with ✎ Edit is talking about.
/// By value, never by index — the page does not hold the lane still while a dialog is open, so a trim, a clamp or
/// another effect shifts the list under an index kept across the window, and the form would write its answer onto
/// someone else's effect.
///
/// The same clause has a second half: the box the re-opened form shows comes off the effect found here, never off a
/// snapshot the form took when it opened — §F3.8 S4 (`box always from the live effect, never the form's snapshot`).
pub fn find_by_value(fx: &[Fx], kind: &str, t: f64) -> Option<usize> {
    fx.iter()
        .position(|effect| effect.kind == kind && (effect.t - t).abs() < REACH_SECONDS)
}

/// What the picture was pressed in — §7 (`framing is done paused`) and [`crate::fx_zoom::preview_press`]'s
/// precondition, asked here because the hit order below depends on it and a bool would not say which of the two
/// reasons a press took nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Nothing armed and the preview paused: the picture is there to be framed.
    Idle,
    /// A kind is armed, so the drag belongs to the new effect rather than to an existing one.
    Armed,
    /// Something is playing: a press cues the clock, it does not grab a box.
    Playing,
}

/// Which rectangle on the finished picture a press takes hold of. The index is into the cut's own effect list, so
/// the caller can go from what was grabbed straight to the record it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// An overlay box in hand — above all others because it is already being judged.
    HeldBox(usize),
    /// The camera rect of a zoom in hand.
    HeldCamera(usize),
    /// A text on the picture: the one whose words the pointer is over, topmost first.
    Text(usize),
    /// The framing camera rect, taken only when nothing else is held.
    FramingZoom(usize),
}

/// §7 (`hit order on the picture: held box → held camera → texts top-down → the framing zoom`): what a press means
/// when several rectangles cover the same pixel.
///
/// Nothing is taken while a kind is armed or the preview lives — that drag draws a new box, or it cues the clock —
/// which is why [`Press`] comes first and the rectangles are only asked about when it says [`Press::Idle`].
///
/// `texts` is the caller's already-filtered list of texts under the pointer as `(effect index, y on screen)`; the
/// winner is the smallest `y`. Top-down because the boxes overlap: a text lower in the frame lies over one higher up
/// only where they cross, so the topmost of those under the pointer is the one the hand reaches first. `framing` is
/// last because it is the picture's own framing rather than an effect being worked on — it is what a press takes
/// when it takes nothing else.
pub fn grab(
    press: Press,
    held_box: Option<usize>,
    held_camera: Option<usize>,
    texts: &[(usize, f64)],
    framing: Option<usize>,
) -> Option<Hit> {
    if press != Press::Idle {
        return None;
    }
    if let Some(index) = held_box {
        return Some(Hit::HeldBox(index));
    }
    if let Some(index) = held_camera {
        return Some(Hit::HeldCamera(index));
    }
    if let Some(&(index, _)) = texts.iter().min_by(|(_, a), (_, b)| a.total_cmp(b)) {
        return Some(Hit::Text(index));
    }
    framing.map(Hit::FramingZoom)
}

/// §F3.8 S2 (`Frame steps and ←/→ nudge a held effect unsnapped`): the second an effect lands on when the keyboard
/// moved it. What this does NOT do is the rule — no marks, no reach, no pixels: the hand asked for one frame and
/// gets one frame, even where a segment end or another effect's end sits under it.
///
/// The dragged band is the other gesture and takes the other function: [`crate::cut_select::move_band`] offers its
/// ends to the marks within `P.policy.snapToleranceSeconds` (`layout.snapPx`), because a drag that lands a splice a
/// pixel off a word is a mistake while a nudge pulled onto a mark it was deliberately walked past is one too.
pub fn nudge(t: f64, delta: f64) -> f64 {
    t + delta
}
