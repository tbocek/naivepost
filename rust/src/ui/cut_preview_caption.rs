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

/// Draw one caption at `alpha`, through the ONE lettering the thumbnail's words are printed with
/// ([`crate::produce_upload_print::draw_text_box`], the prototype's `drawFxText` for both): bold sans, fitted to the
/// box, every line centred, the dark edge composited once and the white words over it. The first rewrite painted
/// each of the edge's copies at the edge's alpha, which compounds where they overlap and bands into several edges.
pub fn draw(cr: &cairo::Context, effect: &Fx, fit: (f64, f64), alpha: f64) {
    let (x, y, w, h) = box_of(effect, fit);
    crate::produce_upload_print::draw_text_box(cr, &effect.text, x, y, w, h, alpha);
}
