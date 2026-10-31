//! F3.3 (the arithmetic after S4) — the render's pass over the segments, cut where the clock changes.

use crate::cut::{Cut, Fx, Seg};
use crate::cut_speed::{self, Span};
use crate::tools::cutpass::MIN_CLIP_SECONDS;

/// §F3.3 / §08 S3: the segments the render plans, split at every rate boundary and stamped with its rate.
pub fn sped_segs(segs: &[Seg], fx: &[Fx]) -> Vec<Seg> {
    let spans = cut_speed::rate_spans(fx);
    if spans.is_empty() {
        return segs.to_vec();
    }
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len());
    for seg in segs {
        if !seg.ins.is_empty() || seg.e <= seg.s {
            out.push(seg.clone());
            continue;
        }
        for piece in pieces(seg, &spans) {
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

fn on_screen(piece: &Seg) -> f64 {
    (piece.e - piece.s) / cut_speed::applied_rate(piece.rate)
}
