//! §05-cut#7-rules bullet 8 — `the tracks are a window drawn under a translate`.
//!
//! The placeholder strip (`track-strip`) is the page's narrow view of a session that is usually far wider than
//! it. A window onto something wider only works if both halves agree about where the window sits: the painter
//! must shift its tape left by the scroll, and a press must add the same scroll back to recover the second the
//! pointer is over. Before this module neither side did either — the strip painted from x=0 whatever the scroll
//! said, and `press_strip` divided the pixel by pps alone — so click and paint agreed only because both ignored
//! the same number. Scrolling moved nothing, and a scrolled click would have landed on the wrong clip.
//!
//! Three numbers, one source: [`scroll_px`] is the single scroll the strip knows, taken from
//! [`crate::ui::cut_view::origin`], which is what the PAINTED band already translates by. One scroll therefore
//! drives both surfaces instead of two that can drift.
//!
//! The gutter is deliberately outside all of this: like [`crate::ui::cut_tracks`], which puts the row name
//! plates before its translate, this strip's fixed furniture (the ground, the empty hint) stays put while the
//! tape slides under it.


thread_local! {
    /// The strip's own scroll in px. Zero until the placeholder surface scrolls with the view; see
    /// [`scroll_px`] for why the number exists before anything moves it.
    static SCROLL: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// The strip's own pixels-per-second: the placeholder `select-surface` draws at the fixed open-at zoom
/// (`crate::ui::window::SELECT_SURFACE_PPS`, 4 px/s) with no gutter and no scroll of its own, and every surface
/// on it (ruler / picture row / wave strip / lane / effects lane) plus each widget's box width is sized against
/// that constant. Reading a live pps there would repaint the ruler, the fold badges and the row plates at a
/// scale their own height rules do not assume — a §05-cut#1-screen change, not this rule's business. So
/// [`seconds_at`] divides by the constant below, matching that surface exactly, while the PAINTED band keeps its
/// own conversion in [`crate::ui::cut_line_press::press`] (view pps + gutter + origin).
///
/// The `track-strip` widget is a THIRD surface and keeps its own zoom ([`crate::ui::window::strip_pps`], which
/// fits the session to the widget's width): its trim/move gestures convert in TRAVELLED pixels, where a scroll
/// offset cancels out of `press_x + x` rather than being added to it. Those conversions stay as they were.
pub fn pps() -> f64 {
    crate::ui::window::SELECT_SURFACE_PPS
}

/// How far into the timeline the strip's left edge sits, in px — the translate everything else is measured
/// against. Zero for now: the placeholder `select-surface` is a fixed-width box drawn at a fixed zoom
/// (§05-cut#1-screen), so its window has nothing to slide under yet. The painter and every press on it still go
/// through here, which is what makes wiring the scroll up a one-line change instead of a hunt for the places
/// that assumed zero — and when it does move, it moves by [`crate::ui::cut_view::origin`], the number the
/// painted band already translates by, so one wheel turn drives both windows.
pub fn scroll_px() -> f64 {
    SCROLL.with(|cell| cell.get())
}

/// The strip's scroll, forced to a known px for a check. §7's translate is what the check measures, and while
/// [`scroll_px`] answers zero (the placeholder surface has no scroll yet) a check still needs to drive the two
/// conversions against a non-zero offset — that is what this sets. The program never calls it; its scroll will
/// come from the wheel, the scrollbar and ▶.
pub fn set_scroll_for_test(px: f64) -> f64 {
    SCROLL.with(|cell| {
        cell.set(px);
        cell.get()
    })
}

/// Where a tape-pixel `x` (a value produced by `seconds * pps`, as every painter here computes) lands inside
/// the widget: the translate, applied. Exactly the inverse of [`seconds_at`] — `tape_x_for_test(seconds_at(px)
/// * pps) == px` is the property that keeps a click on the clip it drew, and it is what the §7 check measures.
/// The PROGRAM slides whole blocks instead of one x at a time: `paint_track_strip` wraps each block in cairo's own
/// `save`/`translate(-scroll_px())`/`restore`, so applying this offset there too would shift those marks twice.
pub fn tape_x_for_test(x: f64) -> f64 {
    x - scroll_px()
}

/// The second the tape at widget-x `x_px` stands for: the window's own offset goes back ON before dividing,
/// because a pixel near the left edge is not near the head of the session once the view has been scrolled.
/// This is the ONE conversion a press on this surface uses.
pub fn seconds_at(x_px: f64) -> f64 {
    ((x_px + scroll_px()) / pps()).max(0.0)
}
