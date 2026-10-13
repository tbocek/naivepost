//! F3.3 (the arithmetic after S4) — the render's pass over the segments, cut where the clock changes.
//!
//! §08 S3 promises "one clip per segment, AFTER THE SPEED SPLITS", and [`crate::cut_speed`] owns those splits —
//! but nothing used to make them: [`crate::produce_render::plan_clips`] read `Seg::rate` off a list nobody had
//! stamped, so every clip came out at ×1 however many ⏩ Speed records sat on the lane. This module is the step
//! that was missing: one call that turns the cut's segments into the list of clips the planner was always meant
//! to receive.
//!
//! It asks exactly one question of [`crate::cut_speed::rate_spans`], and that single call is where all three of
//! §F3.3's arithmetic rules run in the program: overlapping rates AVERAGE per span, spans rendering under
//! `P.eng.minClipSeconds` HEAL into the longer neighbour, and each ramp becomes its geometric staircase with
//! every stair ≥ `P.eng.rampStepSeconds` on screen (`crate::cut_speed::stairs`). Nothing here re-derives any of
//! it — this file only cuts seconds at the boundaries those spans draw and stamps the rate they answered.

use crate::cut::{Cut, Fx, Seg};
use crate::cut_speed::{self, Span};
use crate::tools::cutpass::MIN_CLIP_SECONDS;

/// §F3.3 / §08 S3: the segments the render plans, split at every rate boundary and stamped with its rate.
///
/// Only FOOTAGE is cut. A spliced or overwriting card keeps its own clock — its length IS its timing, and
/// re-timing one would move every other card — which is the same reason [`crate::render_fx::split_pictures`]
/// leaves one alone. A piece carries the RAW rate of the span covering it, including `0.0`: a stop is not a rate,
/// and [`crate::produce_render::plan_clips`] reads a rate of nought as "the footage runs on at 1× and the still
/// rides over it" (§4), so stamping 1 here would erase the only thing that says a frame is held.
///
/// A cut with no speed effect comes back unchanged, piece for piece, in the same order.
pub fn sped_segs(segs: &[Seg], fx: &[Fx]) -> Vec<Seg> {
    let spans = cut_speed::rate_spans(fx);
    if spans.is_empty() {
        return segs.to_vec();
    }
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len());
    for seg in segs {
        // A card is not re-timed, and it is not cut either: its seconds are its own.
        if !seg.ins.is_empty() || seg.e <= seg.s {
            out.push(seg.clone());
            continue;
        }
        for piece in pieces(seg, &spans) {
            // `heal` already gave away every sliver it could inside the SPANS, but a span can still be thinner than
            // the floor where it meets a segment border — half a span inside this scene and half in the next. Such a
            // piece is not encoded (§10's `P.eng.minClipSeconds` is "the shortest clip the render makes"), and the
            // seconds it holds are already counted by the neighbour that took them, so dropping it here loses
            // nothing. The drop lives in this one function so exactly one place owns it.
            if on_screen(&piece) < MIN_CLIP_SECONDS - 1e-9 {
                continue;
            }
            out.push(piece);
        }
    }
    out
}

/// The same door asked through a whole cut: how `produce_exec::plan` composes it, in one call.
pub fn sped_cut(cut_: &Cut) -> Vec<Seg> {
    sped_segs(&cut_.segs, &cut_.fx)
}

/// One segment as the pieces the spans cut it into. A second no span covers is left at ×1 by simply being
/// unstamped — the planner's own reading of `rate <= 0.0` aside, an unstamped piece is what §4 calls plain footage.
fn pieces(seg: &Seg, spans: &[Span]) -> Vec<Seg> {
    // The boundaries inside this segment, plus its own ends, in order.
    let mut marks: Vec<f64> = vec![seg.s, seg.e];
    for span in spans {
        for edge in [span.from, span.to] {
            if edge > seg.s && edge < seg.e {
                marks.push(edge);
            }
        }
    }
    marks.sort_by(f64::total_cmp);
    marks.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    let mut out = Vec::with_capacity(marks.len().saturating_sub(1));
    for pair in marks.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if to - from < 1e-9 {
            continue;
        }
        let mut piece = seg.clone();
        piece.s = from;
        piece.e = to;
        piece.rate = rate_over(spans, from, to);
        out.push(piece);
    }
    out
}

/// The rate one piece runs at: the span covering its middle. Every boundary in the list came FROM the spans, so one
/// middle answers for the whole piece — the same argument §4 makes about a clip's middle carrying the rate.
fn rate_over(spans: &[Span], from: f64, to: f64) -> f64 {
    let mid = (from + to) / 2.0;
    spans
        .iter()
        .find(|span| mid >= span.from && mid < span.to)
        .map(|span| span.rate)
        .unwrap_or(1.0)
}

/// Seconds the finished video spends on a piece: its session seconds divided by the rate that PLAYS, which is what
/// `P.eng.minClipSeconds` is measured against ([`crate::cut_speed::applied_rate`] reads a stop back to 1 because
/// the footage under a still runs at its own speed).
fn on_screen(piece: &Seg) -> f64 {
    (piece.e - piece.s) / cut_speed::applied_rate(piece.rate)
}
