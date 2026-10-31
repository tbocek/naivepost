//! §05-cut#7-rules bullet 8 — `the tracks are a window drawn under a translate`.

thread_local! {
    /// The strip's own scroll in px.
    static SCROLL: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// The strip's own pixels-per-second: the placeholder `select-surface` draws at the fixed open-at zoom
/// (`crate::ui::window::SELECT_SURFACE_PPS`, 4 px/s) with no gutter and no scroll of its own, and every surface
/// on it (ruler / picture row / wave strip / lane / effects lane) plus each widget's box width is sized against
/// that constant.
pub fn pps() -> f64 {
    crate::ui::window::SELECT_SURFACE_PPS
}

/// How far into the timeline the strip's left edge sits, in px — the translate everything else is measured
/// against.
pub fn scroll_px() -> f64 {
    SCROLL.with(|cell| cell.get())
}

/// The strip's scroll, forced to a known px for a check.
pub fn set_scroll_for_test(px: f64) -> f64 {
    SCROLL.with(|cell| {
        cell.set(px);
        cell.get()
    })
}

/// Where a tape-pixel `x` (a value produced by `seconds * pps`, as every painter here computes) lands inside
/// the widget: the translate, applied.
pub fn tape_x_for_test(x: f64) -> f64 {
    x - scroll_px()
}

/// The second the tape at widget-x `x_px` stands for: the window's own offset goes back ON before dividing,
/// because a pixel near the left edge is not near the head of the session once the view has been scrolled.
pub fn seconds_at(x_px: f64) -> f64 {
    ((x_px + scroll_px()) / pps()).max(0.0)
}
