//! F3.4 S4 — the caption's half of "same function for preview and render".
//!
//! §S4 promises the preview shows the words at the size the RENDER would give: "fitted: the largest size where ≤ 12 lines fit, floor 7 pt · bold sans, white with a dark edge · preview = render". [`crate::fx_text::fit`] is arithmetic, not font measurement, so both sides agree.
//! Nothing here lays out text: every number (box, size, lines, baselines, edge radius, alpha, step count) is asked of [`crate::fx_text`].

use crate::cut::Fx;
use crate::fx_text::{self, Box_};

/// The caption's box in the pixels of the fitted picture. A record carries `cx`/`cy`/`wf`/`hf` as fractions of the OUTPUT frame (§06#1), any may be absent (a typed caption has no box; `LOWER_THIRD` per §S2),
/// asked per missing field, since a drag that sized a box but left the centre alone keeps the centre.
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

/// `(box_px, size, lines)` for the draw, all from the shared doors: [`fx_text::fit`] over the box's own size, the same call the thumbnail makes ([`crate::produce_runs::reprint`]), so "preview = render" is one layout path.
pub fn layout(effect: &Fx, fit: (f64, f64)) -> ((f64, f64, f64, f64), f64, Vec<String>) {
    let box_px = box_of(effect, fit);
    let (size, lines) = fx_text::fit(&effect.text, box_px.2, box_px.3);
    (box_px, size, lines)
}

/// Draw one caption at `alpha`, through the ONE lettering the thumbnail's words use ([`crate::produce_upload_print::draw_text_box`]): bold sans fitted to the box, lines centred, the dark edge composited once and the white words over it.
pub fn draw(cr: &cairo::Context, effect: &Fx, fit: (f64, f64), alpha: f64) {
    let (x, y, w, h) = box_of(effect, fit);
    crate::produce_upload_print::draw_text_box(cr, &effect.text, x, y, w, h, alpha);
}
