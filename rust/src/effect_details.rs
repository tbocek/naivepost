//! §06-effects#8-details-confirmed-against-the-code-verification-pass — the details §8 lists against the prototype,
//! held where each rule lives.
//!
//! §8 is a verification pass: its checked behaviours are not to be "improved" away. Most have an owner, which this module points at:
//!
//! * the drawing's two refusals and its once-only preview line — [`crate::fx_svg::NO_LINE`],
//!   [`crate::fx_svg::NO_FILE`] and [`crate::fx_svg::Rasters::failure_line`], which answers `Some` the first time a
//!   file fails and `None` after;
//! * a rate for a whole segment being a speed effect over it, outside any decorations ceiling —
//!   [`crate::cut_suggest::is_speed_effect`];
//! * the cards: their stamp, holes, generators, seeds, bake and length without an end — [`crate::cut_cards`], whose
//!   §8 block holds the three notes a render logs;
//! * a still needing one frame, and a CSS animation with no `@keyframes` being one — [`crate::cut_insert`];
//! * the 1× read head's floor and the rejoin's dip — [`crate::fx_lane::DEBT_TAIL_MIN_SECONDS`],
//!   [`crate::fx_lane::draws_debt_tail`] and [`crate::fx_lane::rejoin_dip`].
//!
//! What is here is what nothing else wrote down: the `>>> ` prefix of §8's two preview-failure lines, the stop frame's failure line, the neither-stops-the-effect rule, and the read head as one question.

use crate::fx_lane;

/// §8 (`failure logged once`; §D's log lines): the prefix every line the page logs carries, written into the line (as [`crate::cut_clamp::log_line`] and [`crate::cut_suggest`]) so a caller hands over the body once and cannot forget or double it.
pub const PREFIX: &str = ">>> ";

pub fn log(body: &str) -> String {
    format!("{PREFIX}{body}")
}

/// §8 (`unrenderable still ">>> the stop frame at m:ss cannot be shown in the preview: <err>"`): the line for a still the preview could not decode. It names the moment ([`crate::tools::mm_ss`]) not the file, since a stop ([`crate::cut_speed`]'s rate-0 effect) has no file.
///
/// It undoes nothing: a [`crate::cut_speed::STOP_SECONDS`] effect the preview cannot draw is still on the lane, and the render may manage what the one-shot decode could not.
pub fn still_failure(t: f64, reason: &str) -> String {
    log(&format!(
        "the stop frame at {} cannot be shown in the preview: {reason}",
        crate::tools::mm_ss(t)
    ))
}

/// §8 (`neither stops the effect`): a drawing or still the preview cannot raster is said once and stays placed; the render runs a different pipeline and often succeeds.
pub fn preview_failure_stops_the_effect() -> bool {
    false
}

/// §8 (`The 1× sound read head opens only where debt ≥ 0.05 s; a card, held frame or clip on no recording closes it`): the one question the lane asks: [`crate::fx_lane::draws_debt_tail`]'s three conditions plus two that say there is nothing to read back.
///
/// A card or held frame films nothing and a clip can sit on ground no recording covers ([`crate::cut_hear::hush`]), so the sound is as long as the picture and there is no debt to open for.
pub fn read_head_open(
    snd: &str,
    debt: f64,
    scene_runs_on: bool,
    filmed: bool,
    insert: bool,
) -> bool {
    fx_lane::draws_debt_tail(snd, debt, scene_runs_on) && filmed && !insert
}

/// §8 (`the dip half on each side of the join`): what one stream loses where delayed sound splices back in. [`crate::fx_lane::rejoin_dip`] is the whole crossfade: half to the outgoing audio and half to the incoming, so a caller must not fade both by the whole.
pub fn dip_each_side() -> f64 {
    fx_lane::SOUND_DIP_SECONDS / 2.0
}
