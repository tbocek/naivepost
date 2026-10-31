//! F3.3 (the paragraph after S4) — the PREVIEW's reading of a speed, kept apart from the render's on purpose.
//!
//! §06 §2: "preview runs speed at one flat rate (a rate change is a flushing seek); render follows ramps and averages overlaps". Where [`crate::cut_speed`] walks the whole span list (averaged rates, healed slivers, the staircase),
//! the preview asks one question about one second and gets one number back; nothing here re-derives the render's arithmetic.
//!
//! A stop is the other half: the picture stands still while the clock runs (§F3.3 S4), so two seconds are answered for one moment: the one the clock shows and the one whose frame is on the panel.

use crate::cut::{EffectKind, Fx};
use crate::cut_speed;

/// §2 (`preview runs speed at one flat rate`): the multiplier the preview plays at for the second `t`.
///
/// The mean of the rates of every ⏩ Speed record whose band covers `t`, as stored: no staircase, no healing. NOT [`crate::cut_speed::rate_at`]: following its stairs ten times a second would be a flushing seek inside every stair.
/// Two overlapping speeds still average, since ×2 and ×4 over the same seconds cannot both be played.
///
/// A stop contributes 0 to that mean and [`crate::cut_speed::applied_rate`] reads the result back to 1: under a still the footage runs at its own speed.
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
/// Earliest `t` wins when two overlap, as every other overlapping answer here ([`crate::fx_lane::sound_answer_at`], `crate::cut_speed::camera_at`). A stop is a speed record whose rate is nought (§1).
pub fn still_band_at(fx: &[Fx], t: f64) -> Option<(f64, f64)> {
    bands(fx)
        .filter(|(from, to, rate)| *rate <= 0.0 && *from <= t && t < *to)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(from, to, _)| (from, to))
}

/// The second whose FRAME belongs on the panel at clock second `t`: inside a stop its first second (the held picture), elsewhere `t`; the clock keeps walking ([`advance`]).
pub fn picture_second(fx: &[Fx], clock: f64) -> f64 {
    match still_band_at(fx, clock) {
        Some((start, _)) => start,
        None => clock,
    }
}

/// Where the line is after `elapsed` seconds of wall clock, playing at the flat rate of `from`; one tick's advance.
///
/// Here rather than in [`crate::preview::play_advance`] so `preview` stays cut-agnostic. Never a backwards step: a rate that low is a clamp's business ([`crate::cut_speed::clamp_speed`]).
pub fn advance(fx: &[Fx], from: f64, elapsed: f64) -> f64 {
    from + elapsed * flat_rate_at(fx, from).max(0.0)
}

/// Every speed record as `(start, end, rate)`, built once so both doors share one idea of what a speed covers; other kinds have no `rate` (§06 §1).
fn bands(fx: &[Fx]) -> impl Iterator<Item = (f64, f64, f64)> + '_ {
    fx.iter()
        .filter(|effect| effect.effect_kind() == Some(EffectKind::Speed))
        .filter(|effect| effect.dur > 0.0)
        .map(|effect| (effect.t, effect.t + effect.dur, effect.rate))
}

/// The mean a lone stop leaves behind: 0 when a stop covers `t` (so [`crate::cut_speed::applied_rate`] answers 1), 1 when nothing covers it.
fn stop_mean(fx: &[Fx], t: f64) -> f64 {
    if still_band_at(fx, t).is_some() {
        0.0
    } else {
        1.0
    }
}
