//! §06-effects#8 bullet 1 — the preview's other failure line: a **stop** whose held frame the
//! preview cannot show. Split out of `cut_picture.rs`, which is near its size budget, so the
//! panel keeps one call and this module owns the rule.
//!
//! §8 says both preview failures are only said ("neither stops the effect"): a stop that the
//! preview cannot draw stays on the lane, because the render takes its still from the footage
//! through ffmpeg rather than from one extracted texture, and often succeeds where the preview
//! could not.

use adw::prelude::Cast;

use crate::cut::Fx;
use crate::effect_details;
use crate::fx_record;
use crate::ui;

/// The reason §8's sentence names when the frame search came up empty. Spelled here rather than
/// at the caller so the log line has one wording for "no frame to show".
const NO_FRAME: &str = "no extracted frame for that recording at that second";

/// A stop covers `t`? Half-open like every other range in the app, and a stop is a speed whose
/// rate is 0 ([`fx_record::is_stop`]).
fn stop_covers(fx: &[Fx], t: f64) -> bool {
    fx.iter()
        .filter(|effect| fx_record::is_stop(effect))
        .any(|effect| {
            let (from, to) = effect.spans();
            from <= t && t < to
        })
}

/// Say §8's line when a stop stands over the line and the preview found no frame to hold there.
/// `found` is the panel's own answer from its frame search, so this decides nothing about the
/// picture — it only speaks. Returns the line it logged, so a test can compare it with
/// [`effect_details::still_failure`] verbatim; `None` when there was nothing to say (no stop
/// covering `t`, or a frame was found).
pub fn say_missing_still(
    window: &adw::ApplicationWindow,
    fx: &[Fx],
    t: f64,
    found: bool,
) -> Option<String> {
    if found || !stop_covers(fx, t) {
        return None;
    }
    // §8's `neither stops the effect`, read where it bites: this door only speaks, so
    // a preview that cannot decode one frame never refuses or removes the stop.
    if effect_details::preview_failure_stops_the_effect() {
        return None;
    }
    let said = effect_details::still_failure(t, NO_FRAME);
    crate::ui::window::log_line(&said);
    if let Some(status) = ui::find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    Some(said)
}
