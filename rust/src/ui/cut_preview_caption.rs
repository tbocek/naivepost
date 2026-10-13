//! F3.4 S4 — the caption's half of "same function for preview and render".
//!
//! §S4 promises the preview shows the words at the size the RENDER would give them: "fitted: the largest size where
//! ≤ 12 lines fit, floor 7 pt · bold sans, white with a dark edge · preview = render". The fitter is
//! [`crate::fx_text::fit`], which is arithmetic rather than a font measurement precisely so that both sides can
//! agree; before this module only the thumbnail paths called it, so the preview painted its captions at a fixed
//! 13 px in one unwrapped line and the bargain had no preview side.
//!
//! Nothing here lays out text on its own. Every number — the box, the size, the lines, the baselines, the edge's
//! radius, alpha and step count — is asked of [`crate::fx_text`], so a caption cannot look different in the two
//! halves without changing the module both read.

use crate::cut::Fx;
use crate::fx_text::{self, Box_};

/// The caption's box in the pixels of the fitted picture. A record carries `cx`/`cy`/`wf`/`hf` as fractions of the
/// OUTPUT frame (§06#1), and any of them may be absent — a caption typed rather than dragged has no box stored, and
/// `LOWER_THIRD` is what §S2 says such a caption gets. Asked per missing field rather than whole, because a drag
/// that sized a box but left the centre alone keeps the centre it was given.
pub fn box_of(effect: &Fx, fit: (f64, f64)) -> (f64, f64, f64, f64) {
    let (cx, cy) = effect.centre();
    let box_ = Box_ {
        cx,
        cy,
        wf: effect.wf.unwrap_or(fx_text::LOWER_THIRD.wf),
        hf: effect.hf.unwrap_or(fx_text::LOWER_THIRD.hf),
    };
    box_.to_px(fit.0, fit.1)
}

/// `(box_px, size, lines)` — everything the draw needs, all three from the shared doors. The size and the lines come
/// straight from [`fx_text::fit`] over the box's own pixel width and height, the same call the thumbnail makes
/// ([`crate::produce_runs::reprint`]), so "preview = render" is a fact about this being the only layout path and
/// not about two paths being kept equal by hand.
pub fn layout(effect: &Fx, fit: (f64, f64)) -> ((f64, f64, f64, f64), f64, Vec<String>) {
    let box_px = box_of(effect, fit);
    let (size, lines) = fx_text::fit(&effect.text, box_px.2, box_px.3);
    (box_px, size, lines)
}

/// Draw one caption: the dark dilated edge first, then the white words over it, both at `alpha`.
///
/// Bold sans-serif (§S4), the block centred in the box by [`fx_text::baselines`] and every line centred on the box's
/// middle x. The edge is a dilation, not a drop shadow: [`fx_text::EDGE_STEPS`] offsets evenly spaced around a
/// circle of radius [`fx_text::edge_radius`], drawn in black at [`fx_text::EDGE_ALPHA`] before the letters go down.
/// Offsets are scaled by `stroke / 2` because [`fx_text::render_stroke_width`] straddles its path while these are
/// repeated fills, so the two halves' edges end up the same thickness.
pub fn draw(cr: &cairo::Context, effect: &Fx, fit: (f64, f64), alpha: f64) {
    let (box_px, size, lines) = layout(effect, fit);
    if size <= 0.0 || lines.is_empty() || alpha <= 0.0 {
        return;
    }
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(size);
    let stroke = fx_text::render_stroke_width(size);
    let radius = (stroke / 2.0).max(0.5);
    let ys = fx_text::baselines(box_px.1, box_px.3, size, lines.len());
    for (line, &baseline) in lines.iter().zip(ys.iter()) {
        let Ok(extents) = cr.text_extents(line) else { continue };
        let x = box_px.0 + box_px.2 / 2.0 - extents.width() / 2.0 - extents.x_bearing();
        // "16 directions": the angle list is derived from the constant, so §10's row is the only place to change it.
        for step in 0..fx_text::EDGE_STEPS {
            let angle = std::f64::consts::TAU * step as f64 / fx_text::EDGE_STEPS as f64;
            cr.set_source_rgba(0.0, 0.0, 0.0, fx_text::EDGE_ALPHA * alpha);
            cr.move_to(x + angle.cos() * radius, baseline + angle.sin() * radius);
            let _ = cr.show_text(line);
        }
        cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
        cr.move_to(x, baseline);
        let _ = cr.show_text(line);
    }
}
