//! F3.12 — Clamp to the cut as applied (`spec/06-effects.md` §F3.12, S1..S5).

use crate::cut::{self, Cut, EffectKind, Fx};
use crate::cut_speed;
use crate::fx_record;

/// P.eng.effectMinSurvivingSeconds — a clamped band shorter than this is dropped instead of kept.
/// §10's own wording: "a clamped effect band shorter than this is dropped".
pub const MIN_SURVIVING_SECONDS: f64 = 1.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clamped {
    /// The effects that are kept, in their original order, with the trimmed ones already trimmed.
    pub kept: Vec<Fx>,
    /// How many were dropped — point effect without a footage scene under it, band under the floor,
    /// label over an insert only.
    pub dropped: usize,
    /// One note per dropped effect, in cut order, spelled by [`crate::cut_clamp_line::note`] — the
    /// kind word and the second it was at.
    pub dropped_lines: Vec<String>,
    /// The stops this pass left with no frame to hold: counted in `dropped`, and named here because
    /// §9 wants a stop refused, not silently trimmed like the rest.
    pub(crate) dropped_stops: Vec<Fx>,
}

/// F3.12 S1..S4 — hold every effect to the cut as it will actually play.
pub fn clamp_to_cut(cut: &Cut) -> Clamped {
    let mut out = Clamped::default();
    for fx in &cut.fx {
        let Some(kept) = clamp_one(fx, &cut.segs) else {
            out.dropped += 1;
            out.dropped_lines.push(crate::cut_clamp_line::describe(fx));
            if fx_record::is_stop(fx) {
                out.dropped_stops.push(fx.clone());
            }
            continue;
        };
        out.kept.push(kept);
    }
    out
}

/// §9 (`A stop at a dropped second SHOULD be refused when placed`), applied where the clamp already
/// works out which effects the cut leaves without a home: a stop whose frame cannot be held is
/// named here so its caller can say so, in the words [`crate::effect_overlap::no_frame`] gives.
pub fn refused_stops(clamped: &Clamped) -> Vec<f64> {
    clamped
        .dropped_stops
        .iter()
        .map(|stopped| stopped.t)
        .collect()
}

/// S5's line, verbatim from §F3.12: `">>> N effect(s) pointed at footage the final cut does not keep — dropped"`.
/// The separator is an em dash (U+2014), as in every other line this tree logs.
pub fn log_line(dropped: usize) -> String {
    format!(
        ">>> {dropped} effect(s) pointed at footage the final cut does not keep \u{2014} dropped"
    )
}

fn clamp_one(fx: &Fx, segs: &[cut::Seg]) -> Option<Fx> {
    let (t0, t1) = fx.spans();

    if t1 <= t0 {
        let inside = segs
            .iter()
            .any(|s| !s.is_insert() && s.s <= t0 && t0 <= s.e);
        return inside.then(|| fx.clone());
    }

    let mut stretches: Vec<(f64, f64)> = segs
        .iter()
        .filter(|seg| !seg.is_insert())
        .map(|seg| (t0.max(seg.s), t1.min(seg.e)))
        .filter(|(from, to)| to > from)
        .collect();
    stretches.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (first, last) = (
        stretches.first()?.0,
        stretches.iter().map(|s| s.1).fold(t0, f64::max),
    );
    let mut kept_seconds = 0.0;
    let mut reached = first;
    for (from, to) in stretches {
        kept_seconds += (to - from.max(reached)).max(0.0);
        reached = reached.max(to);
    }

    let was = t1 - t0; // the band before the cut took a piece off it: the fades are shrunk against this
    if kept_seconds >= was - 1e-9 {
        return Some(fx.clone());
    }
    if kept_seconds < MIN_SURVIVING_SECONDS {
        return None; // S3: too little of it survives to be the effect that was meant
    }
    let (nt0, nt1) = (first, last);

    let mut kept = fx.clone();
    kept.t = nt0;
    kept.dur = nt1 - nt0;
    match fx.effect_kind() {
        Some(
            EffectKind::Zoom
            | EffectKind::Text
            | EffectKind::Svg
            | EffectKind::Volume
            | EffectKind::Label,
        ) => {}
        Some(EffectKind::Speed) => {
            if !fx_record::is_stop(fx) {
                let (rate, dur) = cut_speed::clamp_speed(kept.rate, kept.dur);
                kept.rate = rate;
                kept.dur = dur;
            }
        }
        // Nothing this build knows how to trim: leave it where it was rather than lose it.
        None => return Some(fx.clone()),
    }
    let (trans, tout) = shrink_fades(kept.trans, kept.tout, kept.dur, was);
    kept.trans = trans;
    kept.tout = tout;
    Some(kept)
}

/// S4's "fades shrink proportionally": each fade keeps the share of the band it had before the
/// trim, then both are put back inside the length by [`cut_speed::clamp_fades`], which is the one
/// fade rule (§06-effects §7).
fn shrink_fades(trans: f64, tout: f64, dur: f64, was: f64) -> (f64, f64) {
    let (trans, tout) = if was > 0.0 && dur < was {
        let share = dur / was;
        (trans * share, tout * share)
    } else {
        (trans, tout)
    };
    cut_speed::clamp_fades(trans, tout, dur)
}
