//! F3.12 — Clamp to the cut as applied (`spec/06-effects.md` §F3.12, S1..S5).
//!
//! One pass over the effect list, run **after** edge snapping, dead-air removal, mark removal and coalescing have all
//! moved the timeline. Every one of those steps changes the seconds an effect was chosen for, so a rule enforced
//! earlier is enforced against a cut that no longer exists — §06-effects §7's last clause: *suggested effects are
//! clamped against the cut as applied*. The pass reads no model and no file, so it can run at any point.
//!
//! # What each kind gets
//!
//! - **A point effect** (`dur == 0`, a label being the usual one) has no width to overlap with: either it sits inside a
//!   footage scene or it is dropped.
//! - **A band** (zoom, text, svg, volume, speed) is trimmed to the *footage* scene it shares the most time with. An
//!   insert is never home — it brings its own picture, and an effect placed over a card belongs to no footage. Left
//!   under [`MIN_SURVIVING_SECONDS`] the band is dropped rather than kept as a fragment: most of the moment it
//!   decorated is no longer in the video.
//! - **A speed** is re-clamped after trimming so its rate stays playable over what is left; **a stop is not**, because
//!   clamping would hand the frozen frame a rate and quietly turn a still into slow motion.
//! - **A label is trimmed like the rest.** The prototype drops it outright, and §F3.12 says so in brackets: a label
//!   marks a moment for the narration brief, and that moment may well still be in the cut. Trimming keeps the mark;
//!   dropping it would lose information the next flow reads.
//! - **Any other kind** — one this build does not know how to trim — is left exactly as it stands rather than dropped.
//!   An unknown `kind` in an old file must not cost the user an effect (§1's reason for keeping `kind` a `String`).
//!
//! Fades shrink with the band, proportionally and in the same order they were given in, then take part in the usual
//! rule that both together fit inside the length.

use crate::cut::{self, Cut, EffectKind, Fx};
use crate::cut_speed;
use crate::fx_record;

/// P.eng.effectMinSurvivingSeconds — a clamped band shorter than this is dropped instead of kept. §10's own wording:
/// "a clamped effect band shorter than this is dropped". It is a floor on the *survivor*, not on what was trimmed
/// away, so a 3 s caption with 2.5 s of it still in the cut stays.
pub const MIN_SURVIVING_SECONDS: f64 = 1.0;

/// What one pass did, so the caller can log S5's line and a test can name what went.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clamped {
    /// The effects that are kept, in their original order, with the trimmed ones already trimmed.
    pub kept: Vec<Fx>,
    /// How many were dropped — point effect without a footage scene under it, band under the floor, label over an
    /// insert only. S5 logs the count and nothing else; which effect went is not said (§12's decision 19).
    pub dropped: usize,
}

/// F3.12 S1..S4 — hold every effect to the cut as it will actually play. `cut.segs` are the scenes *as applied*: this
/// is the whole reason the pass exists, so passing a list from before snapping or coalescing answers about a cut that
/// is not the one being rendered.
pub fn clamp_to_cut(cut: &Cut) -> Clamped {
    let mut out = Clamped::default();
    for fx in &cut.fx {
        let Some(kept) = clamp_one(fx, &cut.segs) else {
            out.dropped += 1;
            continue;
        };
        out.kept.push(kept);
    }
    out
}

/// S5's line, verbatim from §F3.12: `">>> N effect(s) pointed at footage the final cut does not keep — dropped"`.
/// The separator is an em dash (U+2014), as in every other line this tree logs.
pub fn log_line(dropped: usize) -> String {
    format!(">>> {dropped} effect(s) pointed at footage the final cut does not keep \u{2014} dropped")
}

/// One effect against the scenes: kept as proposed, kept trimmed, or `None` for gone.
fn clamp_one(fx: &Fx, segs: &[cut::Seg]) -> Option<Fx> {
    let (t0, t1) = fx.spans();

    // S1: a point effect has no width, so being inside a footage scene is the whole question. `<=` both sides: a stamp
    // on the exact second a scene starts is on it, not before it.
    if t1 <= t0 {
        let inside = segs
            .iter()
            .any(|s| !s.is_insert() && s.s <= t0 && t0 <= s.e);
        return inside.then(|| fx.clone());
    }

    // S2: the footage scene this effect shares the most time with. Zero overlap is not "the most" — a band that only
    // touches inserts, or sits in a hole the cut does not keep, has no home at all.
    let mut best: Option<&cut::Seg> = None;
    let mut overlap = 0.0;
    for seg in segs {
        if seg.is_insert() {
            continue;
        }
        let o = t1.min(seg.e) - t0.max(seg.s);
        if o > overlap {
            overlap = o;
            best = Some(seg);
        }
    }
    let Some(seg) = best else { return None };

    // Already inside: exactly as proposed, fades and all. The comparison is on the span, so an effect whose band ends
    // on a scene border counts as inside rather than as needing a trim to itself.
    if t0 >= seg.s && t1 <= seg.e {
        return Some(fx.clone());
    }

    let was = t1 - t0; // the band before the cut took a piece off it — the fades are shrunk against this
    let (nt0, nt1) = (t0.max(seg.s), t1.min(seg.e));
    if nt1 - nt0 < MIN_SURVIVING_SECONDS {
        return None; // S3: too little of it survives to be the effect that was meant
    }

    let mut kept = fx.clone();
    kept.t = nt0;
    kept.dur = nt1 - nt0;
    match fx.effect_kind() {
        // The band is the band; its fades shrink with it below.
        Some(EffectKind::Zoom | EffectKind::Text | EffectKind::Svg | EffectKind::Volume | EffectKind::Label) => {}
        // S4: a rate has to stay playable over what the cut left. A stop gets none of this — see the module header.
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

/// S4's "fades shrink proportionally": each fade keeps the share of the band it had before the trim, then both are put
/// back inside the length by [`cut_speed::clamp_fades`], which is the one fade rule (§06-effects §7). `was` is the
/// length before the trim; a band that grew or did not move leaves its fades alone.
fn shrink_fades(trans: f64, tout: f64, dur: f64, was: f64) -> (f64, f64) {
    let (trans, tout) = if was > 0.0 && dur < was {
        let share = dur / was;
        (trans * share, tout * share)
    } else {
        (trans, tout)
    };
    cut_speed::clamp_fades(trans, tout, dur)
}
