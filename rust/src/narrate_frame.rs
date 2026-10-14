//! §07-narrate#1-screen (**Preview**): "paused, wheel steps a frame" — the seconds arithmetic behind
//! that sentence. The Cut page steps its red line through [`crate::cut_line::step_line`] at the footage
//! file's own probed rate; this page has no line and no footage of its own to probe (the picture is the
//! cut's), so it needs only the interval and an end-of-cut rule of its own.
//!
//! No GTK here: the wheel handler in [`crate::ui::narrate_frame_wheel`] reads state, calls these, and
//! writes back — which is what lets a test pin the step without a window.

/// The rate to step at when nothing states one. Decided rather than read from the spec: §1 promises "a
/// frame" but names no rate, and this page cannot ask the source (its frame is composited from the cut).
/// Same number as [`crate::cut_line::FALLBACK_FPS`], so a paused Narrate step and a Cut frame step land
/// on the same grid.
pub const DEFAULT_FPS: f64 = 25.0;

/// One frame's length in seconds. A rate nobody filled in (`0.0`, an unset field) answers with
/// [`DEFAULT_FPS`] rather than dividing by zero or stepping nowhere.
pub fn frame_seconds(fps: f64) -> f64 {
    if fps <= 0.0 {
        return 1.0 / DEFAULT_FPS;
    }
    1.0 / fps
}

/// Step `at` by one frame along a cut `length` seconds long, clamped inside it. `None` when there is no
/// cut to step along: a page with no timeline has no clock, and a notch there must do nothing rather than
/// invent a second. Clamping means a step at either end answers the second it was given, so the caller
/// can tell "already at the end" from "no cut" only through the `Option`.
pub fn step(at: f64, length: f64, frame: f64, forward: bool) -> Option<f64> {
    if length <= 0.0 {
        return None;
    }
    let moved = if forward {
        at + frame
    } else {
        (at - frame).max(0.0)
    };
    Some(moved.min(length))
}
