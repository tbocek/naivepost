//! §06-effects#8-details-confirmed-against-the-code-verification-pass — the details §8 lists against the prototype,
//! held where each rule lives.
//!
//! §8 is a verification pass rather than a flow: it says which of the page's small behaviours were checked against
//! the old code and are therefore not to be "improved" away. Almost all of them already have an owner, and this
//! module exists to point at them rather than to restate them:
//!
//! * the drawing's two refusals and its once-only preview line — [`crate::fx_svg::NO_LINE`],
//!   [`crate::fx_svg::NO_FILE`] and [`crate::fx_svg::Rasters::failure_line`], which answers `Some` the first time a
//!   file fails and `None` after;
//! * a rate for a whole segment being a speed effect over it, outside any decorations ceiling —
//!   [`crate::cut_suggest::is_speed_effect`] and [`crate::cut_suggest::exempt_from_decorations_cap`];
//! * the cards: their stamp, holes, generators, seeds, bake and length without an end — [`crate::cut_cards`], whose
//!   §8 block holds the three notes a render logs;
//! * a still needing one frame, and a CSS animation with no `@keyframes` being one — [`crate::cut_insert`];
//! * the 1× read head's floor and the rejoin's dip — [`crate::fx_lane::DEBT_TAIL_MIN_SECONDS`],
//!   [`crate::fx_lane::draws_debt_tail`] and [`crate::fx_lane::rejoin_dip`].
//!
//! What is here is what nothing else had written down: the `>>> ` prefix §8 quotes its two preview-failure lines
//! with, the stop frame's own failure line, the sentence that neither failure stops the effect, and the read head
//! asked as one question instead of three.

use crate::fx_lane;

/// §8 (`failure logged once`; §D's log lines): the prefix every line the page logs carries. This tree writes it
/// into the line rather than having the log pane add it — [`crate::cut_clamp::log_line`] and
/// [`crate::cut_suggest`] do the same — so a caller hands over the body once and cannot forget or double it.
pub fn log(body: &str) -> String {
    format!(">>> {body}")
}

/// §8 (`unrenderable still ">>> the stop frame at m:ss cannot be shown in the preview: <err>"`): the line for a
/// still the preview could not decode. The second is spelled by [`crate::tools::mm_ss`], the same clock every other
/// sentence about this effect quotes, and names the moment rather than the file because a stop has no file — it is
/// [`crate::cut_speed`]'s rate-0 effect, whose frame is taken from the footage under it.
///
/// The line says the preview could not show it; it does not undo anything. A [`crate::cut_speed::STOP_SECONDS`]
/// effect that cannot be drawn in the preview is still on the lane, and the render may well manage what the
/// preview's one-shot decode could not.
pub fn still_failure(t: f64, reason: &str) -> String {
    log(&format!(
        "the stop frame at {} cannot be shown in the preview: {reason}",
        crate::tools::mm_ss(t)
    ))
}

/// §8 (`neither stops the effect`): a drawing or a still the preview cannot raster is said once and stays placed.
/// Refusing the edit would punish the person for the preview's limits — the render runs a different pipeline, with
/// ffmpeg rather than one texture, and often succeeds where this said it could not.
pub fn preview_failure_stops_the_effect() -> bool {
    false
}

/// §8 (`The 1× sound read head opens only where debt ≥ 0.05 s; a card, held frame or clip on no recording closes
/// it`): the one question the lane asks, which is [`crate::fx_lane::draws_debt_tail`]'s three conditions — only the
/// `scene` answer runs to the scene's end, the debt has to be worth a line, and the scene has still to be running
/// on — and two more that say there is nothing to read back.
///
/// A card or a held frame films nothing and a clip can sit on ground no recording covers ([`crate::cut_hear::hush`]
/// answers an insert with no hearing list at all), so in both cases the sound is exactly as long as the picture and
/// there is no debt to open for, however much the arithmetic says there is.
pub fn read_head_open(snd: &str, debt: f64, scene_runs_on: bool, filmed: bool, insert: bool) -> bool {
    fx_lane::draws_debt_tail(snd, debt, scene_runs_on) && filmed && !insert
}

/// §8 (`the dip half on each side of the join`): what one stream loses where delayed sound splices back into the
/// picture. The whole of [`crate::fx_lane::rejoin_dip`] is the crossfade, so it is shared — half goes to the
/// outgoing audio and half to the incoming, which is why a caller must not fade both by the whole and hear a dip
/// twice as deep as the one that stops the click.
pub fn dip_each_side() -> f64 {
    fx_lane::SOUND_DIP_SECONDS / 2.0
}
