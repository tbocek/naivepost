//! F3.3 (the paragraph after S4) — the PREVIEW's reading of a speed, kept apart from the render's on purpose.
//!
//! §06 §2 states the difference and gives the reason: "preview runs speed at one flat rate (a rate change is a
//! flushing seek); render follows ramps and averages overlaps". So where [`crate::cut_speed`] walks the whole span
//! list — averaged rates, healed slivers, the geometric staircase — the preview asks one question about one second
//! and gets one number back for the stretch it is in. Nothing here re-derives the render's arithmetic; nothing here
//! could be replaced by calling it.
//!
//! A stop is the other half: the picture stands still there while the clock runs (§F3.3 S4), which means two
//! different seconds have to be answered for the same moment — the second the clock shows, and the second whose
//! frame is on the panel.

use crate::cut::{EffectKind, Fx};
use crate::cut_speed;

/// §2 (`preview runs speed at one flat rate`): the multiplier the preview plays at for the second `t`.
///
/// The mean of the rates of every ⏩ Speed record whose band covers `t`, read as stored — no staircase, no healing,
/// no boundaries. That is deliberately NOT [`crate::cut_speed::rate_at`]: that one answers from
/// [`crate::cut_speed::rate_spans`], which carries the stairs, and following a staircase ten times a second means a
/// flushing seek inside every stair, which is exactly what §2 says the preview must not do. Two overlapping speeds
/// still average here, because two effects asking for ×2 and ×4 over the same seconds cannot both be played.
///
/// A stop contributes 0 to that mean and [`crate::cut_speed::applied_rate`] reads the result back to 1: under a
/// still the footage runs at its own speed ([`crate::cut_speed::applied_rate`]'s rule, not this module's).
pub fn flat_rate_at(fx: &[Fx], t: f64) -> f64 {
    let covering: Vec<f64> = bands(fx)
        .filter(|(from, to, rate)| *from <= t && t < *to && *rate > 0.0)
        .map(|(_, _, rate)| rate)
        .collect();
    if covering.is_empty() {
        return cut_speed::applied_rate(stop_mean(fx, t));
    }
    cut_speed::applied_rate(covering.iter().sum::<f64>() / covering.len() as f64)
}

/// §F3.3 S4 (`the picture stands still there while the clock runs`): the stop covering `t`, as its own seconds.
/// Earliest `t` wins when two overlap — the same tie-break every other overlapping answer on this page uses
/// ([`crate::fx_lane::sound_answer_at`], [`crate::cut_speed::camera_at`]), because one second can only hold one
/// frame. A stop is a speed record whose rate is nought (§1's column), which is how it is recognised here too.
pub fn still_band_at(fx: &[Fx], t: f64) -> Option<(f64, f64)> {
    bands(fx)
        .filter(|(from, to, rate)| *rate <= 0.0 && *from <= t && t < *to)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(from, to, _)| (from, to))
}

/// The second whose FRAME belongs on the panel at the clock second `t`: inside a stop that is the stop's first
/// second — the held picture — and everywhere else it is simply `t`. This is the only door that turns a stop into a
/// frozen frame; the clock itself keeps walking ([`advance`]).
pub fn picture_second(fx: &[Fx], clock: f64) -> f64 {
    match still_band_at(fx, clock) {
        Some((start, _)) => start,
        None => clock,
    }
}

/// Where the line is after `elapsed` seconds of wall clock, playing at the flat rate of `from`. One tick's advance,
/// so a sped stretch moves the red line further per tick and a slow one less far — which is what makes a hand-drawn
/// speed observable at all without anyone pressing anything.
///
/// It lives here rather than in [`crate::preview::play_advance`] so `preview` stays cut-agnostic: that module knows
/// folds and recorded runs, not effects. Never a backwards step: a negative multiplier would walk the line off the
/// tape, and a rate that low is a clamp's business ([`crate::cut_speed::clamp_speed`]), not the tick's.
pub fn advance(fx: &[Fx], from: f64, elapsed: f64) -> f64 {
    from + elapsed * flat_rate_at(fx, from).max(0.0)
}

/// Every speed record as `(start, end, rate)` — the band shape both doors ask about, built once so neither invents
/// its own idea of what a speed covers. Records of any other kind say nothing about the clock (§06 §1 gives them no
/// `rate`) and are left out here.
fn bands(fx: &[Fx]) -> impl Iterator<Item = (f64, f64, f64)> + '_ {
    fx.iter()
        .filter(|effect| effect.effect_kind() == Some(EffectKind::Speed))
        .filter(|effect| effect.dur > 0.0)
        .map(|effect| (effect.t, effect.t + effect.dur, effect.rate))
}

/// The mean a lone stop leaves behind: 0 when a stop covers `t` (so [`crate::cut_speed::applied_rate`] answers 1),
/// and 1 when nothing covers it at all. Split out because "no speed" and "a stop" reach the same number by
/// different roads and the difference is worth naming.
fn stop_mean(fx: &[Fx], t: f64) -> f64 {
    if still_band_at(fx, t).is_some() {
        0.0
    } else {
        1.0
    }
}
