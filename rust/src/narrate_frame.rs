//! §07-narrate#1-screen (**Preview**): "paused, wheel steps a frame", the seconds arithmetic behind it.
//! No GTK here, so a test can pin the step without a window.

/// Rate to step at when nothing states one; same as [`crate::cut_line::FALLBACK_FPS`] so both pages share a grid.
pub const DEFAULT_FPS: f64 = 25.0;

/// One frame in seconds; an unset rate (`0.0`) gives [`DEFAULT_FPS`].
pub fn frame_seconds(fps: f64) -> f64 {
    if fps <= 0.0 {
        return 1.0 / DEFAULT_FPS;
    }
    1.0 / fps
}

/// Step `at` by one frame along a cut `length` seconds long, clamped inside it. `None` when there is no cut.
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
