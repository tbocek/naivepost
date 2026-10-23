//! §05-cut.md §1 badge **31** and the wheel's three gestures: what part of the timeline is on screen.
//!
//! The zoom and the scroll are VIEW state, not cut state — folding, trimming or adding a scene never
//! changes them, and they change without an edit to undo. So they live here rather than in `cut::Cut`,
//! and every number that decides where the band is drawn comes from this module instead of the frozen
//! [`crate::ui::window::TRACK_STRIP_PPS`] the placeholder strip was built with.
//!
//! Like the rest of `ui`'s split-out files this one exists because `window.rs` is past its size budget;
//! it holds no rule of its own either (§00-principles §5): the ladder, the step and the floor all come
//! from `cut_screen`/`timeline`, and this file only keeps the two numbers a view has (how tight, how far
//! along) and turns a wheel event into a move.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::window::find_widget_by_name;
use crate::{cut_screen, timeline, ui::cut_folds};

/// One screenful of drag on the scrollbar (§1: "thumb geared at high zoom so one screenful is always
/// 40 px of drag"). It is a DRAG distance, not the thumb's length: dragging the thumb by its own length
/// moves the view by exactly one screenful, so `drag = thumb * total / view`, and solving for the thumb
/// that makes that 40 gives `40 * view / total`. That is why the thumb shrinks as the session grows:
/// the same 40 px of hand travel always crosses the same screenful, at any zoom.
pub const DRAG_PER_SCREENFUL_PX: f64 = 40.0;

/// GTK refuses a page smaller than 1 % of the bar (`gtk_adjustment_new` clamps `page_size` below
/// `upper / 100`), which is the floor the thumb gets here rather than an invented minimum.
const THUMB_FLOOR_RATIO: f64 = 0.01;

thread_local! {
    /// The band's pixels-per-second, seeded at the open zoom. `window.rs`'s `PREVIEW_PPS` stays the
    /// placeholder strip's own number so F2.8's press geometry does not move; the real band reads this.
    static PPS: std::cell::Cell<f64> = const { std::cell::Cell::new(cut_screen::ZOOM_AT_OPEN) };
    /// The left edge of the view, in timeline pixels from the start of the gutter.
    static ORIGIN: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// The session's filmed seconds: the takes' own spans merged, so dropped stretches still count (§B:
/// a fold is a view, not an edit). Read through `cut_folds`, which already holds this window's takes.
fn filmed_seconds() -> f64 {
    timeline::filmed_runs(&cut_folds::page_recordings())
        .iter()
        .map(|(start, end)| end - start)
        .sum()
}

/// The same number, for a reader outside this module that has to do the tape's arithmetic the page's way —
/// §A's zoom floor and the scrollbar's fit rule both divide the view by it, so a check that guessed the
/// session's length would be testing its own guess. Named for that second reader; the program asks the
/// private one.
pub fn filmed_seconds_for_test() -> f64 {
    filmed_seconds()
}

/// The page's own surface width: §1's real band, asked of GTK for its measured allocation rather than trusted
/// to a constant — the band spans the page column (`set_hexpand`), so it is wider than the 480 px placeholder
/// packed beside it, and it is the surface the tape, the ruler and the red line are actually drawn on. This is
/// what the scrollbar's fit rule and §B's reveal are measured over; the zoom ladder's floor is NOT — that one
/// reads [`zoom_surface_width`], which stays on the strip the ladder has always been sized against.
pub fn surface_width(window: &adw::ApplicationWindow) -> f64 {
    let content = match window.content() {
        Some(content) => content,
        None => return STRIP_FALLBACK_PX,
    };
    ["track-band", "track-playhead"]
        .iter()
        .find_map(|name| find_widget_by_name(&content, name))
        .map(|widget| f64::from(widget.width()))
        .filter(|px| *px > 0.0)
        .unwrap_or_else(|| zoom_surface_width(window))
}

/// The width the page's ZOOM arithmetic is measured over: always F2.8's placeholder strip, never §1's band.
/// Read OFF THE WINDOW rather than through [`surface_width`], which falls back to this function when its own
/// search finds nothing — calling each other would turn an un-laid-out window (every headless test) into
/// infinite recursion.
///
/// The strip and not the band because that 480 px is what the ladder has been sized against since it existed:
/// `strip_pps`, the door the placeholder's paint and press geometry read, clamps at `view_px(window::STRIP_WIDTH)`
/// whatever GTK has laid out, so a floor worked out over the full-page band would be a number nothing on screen
/// agrees with — − would stop there while the strip still showed the tape from its first second.
pub fn zoom_surface_width(window: &adw::ApplicationWindow) -> f64 {
    let content = match window.content() {
        Some(content) => content,
        None => return STRIP_FALLBACK_PX,
    };
    ["track-strip"]
        .iter()
        .find_map(|name| find_widget_by_name(&content, name))
        .map(|widget| f64::from(widget.width()))
        .filter(|px| *px > 0.0)
        .unwrap_or(STRIP_FALLBACK_PX)
}

/// What to measure when the page has no allocated width yet (built but never presented, as in a test).
const STRIP_FALLBACK_PX: f64 = crate::ui::window::STRIP_WIDTH as f64;

/// This window's view at a surface width: `(total, view)` off the live zoom and the session's length,
/// which is what every caller of [`paint_scrollbar`] needs and never recomputes by hand.
pub fn extents(surface_width: f64) -> (f64, f64) {
    let total = total_px(filmed_seconds(), pps());
    (total, view_px(surface_width))
}

/// Repaint THIS window's scrollbar from its live zoom, scroll position and session length — the door
/// `window.rs` calls after any zoom, scroll or cut change, so no caller has to carry numbers around.
/// It records the bar's OWN allocation when GTK has given it one (the gear is a number about the bar's
/// pixels, not about the band beside it) and falls back to the surface width only while the bar is still
/// un-laid-out, which is every headless repaint and a test's freshly built window.
pub fn refresh_scrollbar(bar: &gtk::Scrollbar, surface_width: f64) {
    let measured = f64::from(bar.width());
    let gear_from = if measured > 0.0 {
        measured
    } else {
        view_px(surface_width).max(0.0)
    };
    BAR_WIDTH.with(|cell| cell.set(gear_from));
    let (total, view) = extents(surface_width);
    paint_scrollbar(bar, total, view);
}

/// The gutter's width, asked of the one table that draws it (`cut_screen`, whose fold badges already
/// offset by it) so the band and the badges cannot disagree about where the timeline starts.
pub fn gutter_px() -> f64 {
    cut_screen::GUTTER_PX
}

pub fn pps() -> f64 {
    PPS.with(|cell| cell.get())
}

pub fn set_pps(pps: f64) {
    PPS.with(|cell| cell.set(pps.max(0.0)));
}

/// This window's zoom floor: the whole filmed session across the view minus the gutter (§A), which is
/// `timeline::floor_pps` over the takes' merged spans — never the constant the placeholder strip was
/// sized with, because that number knows nothing about how long this session is.
pub fn floor_pps(view: f64) -> f64 {
    timeline::floor_pps(&timeline::filmed_runs(&cut_folds::page_recordings()), view)
}

/// The pixels-per-second the page's geometry is drawn at RIGHT NOW: the stored zoom (§2's "zoom 4 px/s at
/// open"), kept inside §2's own bounds — no shallower than the filmed-length fit of THIS session in THIS
/// view (`floor_pps`, so − stops where all of it is on screen) and no deeper than [`cut_screen::ZOOM_MAX`]
/// (240 px/s). Every strip, press and ruler reads this instead of a constant, which is what makes the
/// stored zoom drive the layout rather than sit beside it. `view` comes from the caller that already knows
/// its own width; [`live_pps_in`] takes it, because a headless window has no allocation to ask.
pub fn live_pps(view: f64) -> f64 {
    pps().clamp(floor_pps(view), cut_screen::ZOOM_MAX)
}

/// How far the view starts, in pixels: 0 shows the session from its first filmed second.
pub fn origin() -> f64 {
    ORIGIN.with(|cell| cell.get())
}

/// The strip's pixels-per-second right now: the stored zoom over this strip's own view (§2). One door, so
/// the paint, the press geometry and every test read the SAME number the layout was drawn with.
///
/// The surface is the strip's DECLARED width (`window::STRIP_WIDTH`, its size request), NOT whatever width
/// GTK happened to allocate it. That is deliberate: `press_zoom` floors the ladder at the same declared
/// width via [`zoom_surface_width`]'s caller passing `view_px(STRIP_WIDTH)`, so §A's "− stops where the
/// whole session is on screen" means "on screen in the strip as built" — a number that does not change if
/// the Cut page's paned later stretches the strip across the column. Reading the realized allocation here
/// would make − stop at one fit while the strip's own paint and press geometry ran at another.
pub fn strip_pps() -> f64 {
    live_pps(view_px(f64::from(crate::ui::window::STRIP_WIDTH)))
}

/// The boxes `track-strip` paints and grabs at the zoom it is drawn at — the same list its own press
/// geometry uses, so a check reads what the widget used rather than recomputing it elsewhere.
pub fn strip_clip_boxes_for_test() -> Vec<crate::cut_trim::Box_> {
    crate::cut_trim::clip_boxes(&crate::ui::window::newest_review_cut().segs, strip_pps())
}

/// The kept scenes as the REAL band draws them, hand corrections applied (§B's `shift`) — what `cut_tracks`
/// paints its green bars from, so a check reads exactly that geometry. Test-only reader, hence the name.
pub fn band_clip_boxes_for_test(window: &adw::ApplicationWindow) -> Vec<crate::cut_trim::Box_> {
    let _ = window;
    let cut_ = crate::ui::window::newest_review_cut();
    crate::ui::cut_tracks::clip_boxes_shifted(&cut_.segs, &cut_.shift, pps())
}

/// How far the timeline reaches: the gutter plus every filmed second at the current zoom. Dropped
/// stretches still take space — a fold is a view, not an edit — so this is the tape's length, which is
/// what [`timeline::filmed_runs`] measures.
pub fn total_px(filmed_seconds: f64, pps: f64) -> f64 {
    gutter_px() + filmed_seconds.max(0.0) * pps
}

/// What the band can actually show: its own width minus the gutter, which is pinned at the left.
pub fn view_px(surface_width: f64) -> f64 {
    (surface_width - gutter_px()).max(0.0)
}

/// Clamp a scroll position into what exists: never left of the gutter, never past the last pixel.
fn clamp_origin(px: f64, total: f64, view: f64) -> f64 {
    px.clamp(0.0, (total - view).max(0.0))
}

/// Set the scroll position, clamped to the timeline's own ends.
pub fn set_origin(px: f64, total: f64, view: f64) -> f64 {
    let clamped = clamp_origin(px, total, view);
    ORIGIN.with(|cell| cell.set(clamped));
    clamped
}

/// Does the whole session fit? When it does the scrollbar hides itself (§A: "hidden when everything
/// fits") — a bar with a full-width thumb says nothing except that there is room to the right.
pub fn fits(total: f64, view: f64) -> bool {
    total <= view
}

/// The thumb's fraction of the bar (§B: "thumb geared at high zoom"). See [`DRAG_PER_SCREENFUL_PX`]
/// for the arithmetic: the bar is `bar_px` long and crossing one screenful must cost 40 px of it, so the
/// thumb is `40 / bar_px` of the bar. GTK wants that fraction as the adjustment's `page_size`, whose own
/// length is `fraction * upper` — hence [`paint_scrollbar`] multiplies it by the VIEW, not by `total`.
pub fn thumb_geared(_total: f64, bar_px: f64) -> f64 {
    if bar_px <= 0.0 {
        return 1.0;
    }
    (DRAG_PER_SCREENFUL_PX / bar_px).clamp(THUMB_FLOOR_RATIO, 1.0)
}

/// The thumb's on-screen length in pixels — 40 px, which is the whole point of the gear.
pub fn thumb_px(_total: f64, bar_px: f64) -> f64 {
    thumb_geared(0.0, bar_px) * bar_px
}

/// Pan by an eighth of the view (§A's Shift+wheel and a trackpad swipe). Delegates so the fraction is
/// spelled once, in `cut_screen::pan_px`. Nothing converts the thumb to a drag distance: dragging the
/// thumb by its own length moves one screenful, so [`thumb_px`] IS the 40 px §1 asks for.
pub fn pan_eighth(view: f64) -> f64 {
    cut_screen::pan_px(view)
}

/// Scroll by a pixel delta, returning the new clamped origin.
pub fn scroll_by(total: f64, view: f64, px: f64) -> f64 {
    set_origin(origin() + px, total, view)
}

/// One step of the − / + ladder over the real view: `inward` is `+`. The floor is where the WHOLE
/// session is on screen — [`floor_pps`] over the filmed runs — not the constant the placeholder strip
/// was sized with, which would stop a two-hour session zoomed in far too tight.
pub fn zoom_step(inward: bool, runs: &[(f64, f64)], total: f64, view: f64) -> (f64, f64) {
    let floor = timeline::floor_pps(runs, view);
    // Zoom about the MIDDLE of the view (§A: the buttons "zoom about the view centre"), so pressing −
    // does not jump to the start of the session. The anchor has to be read at the OLD zoom, which is why
    // the step is computed before `set_pps` and handed over as a factor.
    let centre = origin() + view / 2.0;
    let (next, scrolled) = cut_screen::zoom_around(
        pps(),
        next_of(pps(), inward, floor) / pps(),
        centre,
        origin(),
        floor,
    );
    set_pps(next);
    (next, set_origin(scrolled, total, view))
}

/// One step of the ladder, kept apart so [`zoom_step`] reads as one idea.
fn next_of(pps: f64, inward: bool, floor: f64) -> f64 {
    if inward {
        cut_screen::zoom_up(pps, floor)
    } else {
        cut_screen::zoom_down(pps, floor)
    }
}

/// Zoom keeping the second under the cursor where it is (§A's wheel "does the same, around the
/// cursor"), with the same floor as the buttons. Returns `(new_pps, new_origin)`.
/// Zoom keeping the second under the cursor where it is (§A's wheel "does the same, around the
/// cursor"), with the same floor as the buttons. Returns `(new_pps, new_origin)`.
pub fn zoom_around(
    anchor_x: f64,
    factor: f64,
    runs: &[(f64, f64)],
    total: f64,
    view: f64,
) -> (f64, f64) {
    let floor = timeline::floor_pps(runs, view);
    // The anchor arrives in PAGE pixels (where the pointer actually was). `origin()` scrolls the
    // TIMELINE, whose first pixel sits after the pinned gutter, so the second under the pointer is
    // `(anchor - gutter - origin) / pps` and the scroll back has to be solved in that same space.
    let time = (anchor_x - gutter_px() - origin()) / pps();
    let next = (pps() * factor).clamp(floor, cut_screen::ZOOM_MAX);
    let scrolled = (anchor_x - gutter_px() - time * next).max(0.0);
    // `set_pps` FIRST: the clamped scroll below is measured at the zoom it leaves behind, and reading a
    // stale one here is how a zoom "keeps" a second that has already moved.
    set_pps(next);
    (next, set_origin(scrolled, total, view))
}

/// Recentre the line when it leaves the view (§B's `revealPlayhead`) and leave it alone otherwise — a
/// preview that slid the view on every tick could never be read. `playhead_px` is a pixel INSIDE the
/// timeline (the gutter already subtracted), which is what [`total_px`] measures from its start.
pub fn reveal_playhead(playhead_px: f64, total: f64, view: f64) -> f64 {
    let held = origin();
    if playhead_px < held || playhead_px > held + view {
        set_origin(playhead_px - view / 2.0, total, view)
    } else {
        held
    }
}

/// §B's `revealPlayhead`, on this window's own view: recentre only when the line has LEFT the view. The
/// transport writes the line through `window.rs`'s `set_playhead`, which calls this, so a preview that ran
/// for an hour pulls the view along once per crossing instead of sliding it on every tick.
pub fn follow_playhead(window: &adw::ApplicationWindow, at: f64) {
    let view = view_px(surface_width(window));
    let total = total_px(filmed_seconds(), pps());
    reveal_playhead(at * pps(), total, view);
    repaint_page(window);
}

/// §1's three wheel gestures, attached to the widgets they belong to: transport bar and preview step
/// frames (Shift five), the tracks zoom around the cursor and pan an eighth with Shift — which gesture a
/// notch is stays `cut_screen::wheel`'s rule. `wire_wheel_gestures` attaches them; this is what one does.
/// `on_step` is the caller's transport seam: the red line belongs to `window.rs`, so a step goes through
/// it and this answers `None` rather than reaching for a player it does not own.
pub fn apply_wheel<S: Fn(i64)>(
    window: &adw::ApplicationWindow,
    over: cut_screen::WheelOver,
    shift: bool,
    dy: f64,
    dx: f64,
    on_step: S,
) -> Option<String> {
    // A Shift notch pans an eighth of the view the tape is drawn on, so `cut_screen::wheel` gets the page's own
    // width; the zoom branch below re-reads it over the strip, which is where the ladder's floor lives.
    let view = wheel_gestures_view(window);
    match cut_screen::wheel(over, shift, dy, dx, view) {
        cut_screen::Wheel::StepFrames { frames } => {
            on_step(frames);
            None
        }
        action => apply_wheel_result(window, action),
    }
}

/// The three wheel gestures of §1, attached to the widgets they belong to.
///
/// One `EventControllerScroll` per widget, each told what kind of ground it sits on: over the transport
/// bar and the preview picture a notch steps frames, over the tracks it zooms around the cursor, and
/// Shift+wheel (or a trackpad's sideways swipe) pans an eighth of the view. Which gesture a notch is,
/// and what Shift does to it, is `cut_screen::wheel`'s rule alone — this only reads the modifier off the
/// event and hands the answer to `apply_wheel`, which is also what a test drives.
///
/// `on_step` is the caller's transport seam (§A: stepping frames moves the red line, and the line belongs
/// to `window.rs`), so this module never reaches for a player it does not own.
pub fn attach_wheel<S>(
    window: &adw::ApplicationWindow,
    targets: &[(&'static str, cut_screen::WheelOver)],
    on_step: S,
) where
    S: Fn(&adw::ApplicationWindow, i64) + 'static,
{
    let on_step = std::rc::Rc::new(on_step);
    // The caller's list plus the band §1's tracks are actually drawn on. `track-band` is not in the caller's
    // list because that list lives in `window.rs`, past its size budget, and because a widget built later than
    // this call (the band arrives from `cut_tracks::install`) belongs to whoever builds it: naming it here
    // keeps one rule — every ground the page shows gets exactly one controller of its kind.
    let mut grounds: Vec<(&'static str, cut_screen::WheelOver)> = targets.to_vec();
    // `track-band` is §1's single tracks stack; it is named here as well as in the caller's list because a
    // widget built later than this call (the band arrives from `cut_tracks::install`) belongs to whoever
    // builds it. The duplicate is harmless — the loop below skips any widget that already carries one.
    grounds.push(("track-band", cut_screen::WheelOver::Tracks));
    for (name, over) in grounds {
        let (name, over) = (name, over);
        let Some(widget) = find_widget_by_name(window.upcast_ref(), name) else {
            continue;
        };
        // One controller per widget: a second scroll controller on ground that already has one would answer
        // the same notch twice, zooming two steps where the person rolled one.
        if carries_scroll_controller(&widget) {
            continue;
        }
        let controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        controller.set_name(Some(&format!("wheel-{name}")));
        let win = window.clone();
        let step = on_step.clone();
        controller.connect_scroll(move |ctrl, dx, dy| {
            // The modifier comes off the event that woke the signal, not off a remembered key state:
            // a trackpad swipe carries Shift without anyone holding a key the app watched. One door for
            // both a live notch and a test's: `apply_wheel`, which is also what this closure is.
            let shift = ctrl
                .current_event_state()
                .contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let step2 = step.clone();
            let for_step = win.clone();
            apply_wheel(&win, over, shift, dy, dx, move |frames| {
                step2(&for_step, frames)
            });
            glib::Propagation::Stop
        });
        widget.add_controller(controller);
    }
}

/// Whether this widget already answers a scroll wheel. Read off the widget rather than remembered, so a
/// re-wire (a test's second `wire_wheel_gestures`, a page rebuilt on a tab switch) cannot stack controllers.
fn carries_scroll_controller(widget: &gtk::Widget) -> bool {
    let held = widget.observe_controllers();
    (0..held.n_items())
        .filter_map(|index| held.item(index))
        .any(|controller| controller.is::<gtk::EventControllerScroll>())
}

/// Carry out what `cut_screen::wheel` decided, then redraw. Returns the sentence for the status line,
/// or `None` for a step — which belongs to the transport, and says nothing here.
/// The width ONE wheel gesture is carried out over: §1's band when GTK has laid it out, else the placeholder
/// strip. Both doors below (`apply_wheel`, and this) read it, so the eighth of the view a Shift notch pans and
/// the floor that notch's zoom clamps at come from the same screenful — measured once per gesture rather than
/// once per branch, which is what kept them disagreeing while each asked for itself.
pub fn wheel_gestures_view(window: &adw::ApplicationWindow) -> f64 {
    view_px(surface_width(window))
}

fn apply_wheel_result(
    window: &adw::ApplicationWindow,
    action: cut_screen::Wheel,
) -> Option<String> {
    let runs = timeline::filmed_runs(&cut_folds::page_recordings());
    let filmed: f64 = runs.iter().map(|(start, end)| end - start).sum();
    // The surface the caller measured — see [`wheel_gestures_view`], which both wheel doors go through so a
    // notch's zoom clamp and its eighth cannot disagree about how wide the page is.
    let view = wheel_gestures_view(window);
    let total = total_px(filmed, pps());
    let said = match action {
        cut_screen::Wheel::StepFrames { .. } => None,
        cut_screen::Wheel::Zoom { factor, at_x } => {
            let anchor = if at_x == 0.0 {
                origin() + view / 2.0 + gutter_px()
            } else {
                at_x
            };
            let (next, _) = zoom_around(anchor, factor, &runs, total, view);
            Some(format!("zoom {:.1} px a second", next))
        }
        cut_screen::Wheel::Pan { px } => {
            let moved = scroll_by(total, view, px);
            Some(format!("scrolled to {:.0} px", moved))
        }
    };
    repaint_page(window);
    said
}

/// The surfaces a page redraw has to queue, in one place so the list and the page cannot drift apart.
/// `track-band` is the tape; `track-playhead` is §1's red line on ITS OWN layer, which is exactly why it
/// needs naming here — a draw callback only runs for a widget someone asked to be redrawn, so leaving the
/// line's surface out of this list meant the 2 px line was painted once at its opening position and then
/// never moved again, through a zoom, a scroll or a click that placed it. `track-strip` is F2.8's
/// placeholder strip, still packed under the band and still repainted by the same door.
pub fn repaint_targets() -> [&'static str; 3] {
    ["track-band", "track-playhead", "track-strip"]
}

/// Redraw what the view changed: the band measures itself at the new zoom, the bar follows it.
pub fn repaint_page(window: &adw::ApplicationWindow) {
    let Some(content) = window.content() else {
        return;
    };
    // §1's band is a row per camera AND a row per cut lane, and it was sized before the page had a cut, so
    // every redraw asks first whether the scene it is about to paint still fits the surfaces it has.
    crate::ui::cut_tracks::fit_band_height_for_window(window, &band_scene_for(window));
    for name in repaint_targets() {
        if let Some(band) = find_widget_by_name(&content, name) {
            band.queue_draw();
        }
    }
    if let Some(bar) = find_widget_by_name(&content, "track-scrollbar") {
        if let Ok(bar) = bar.clone().downcast::<gtk::Scrollbar>() {
            paint_scrollbar(
                &bar,
                total_px(filmed_seconds(), pps()),
                view_px(surface_width(window)),
            );
        }
    }
}

thread_local! {
    /// How wide the bar last refreshed was told to be, so [`paint_scrollbar`] can be handed only the view
    /// and still size its thumb in pixels rather than in a fraction nobody checked. GTK gives a scrollbar
    /// no natural width of its own, so before the page is laid out (a headless repaint, a test's fresh bar)
    /// `bar.width()` is 0 and this is the only length there is. Written by [`refresh_scrollbar`], the one
    /// door every repaint of badge **31** goes through.
    static BAR_WIDTH: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Repaint the bar from the view: upper is the whole timeline, the page is the geared thumb (exactly
/// [`DRAG_PER_SCREENFUL_PX`] px of bar, floored at what GTK will draw), the step is an eighth of the
/// screenful, and the bar disappears when everything already fits.
pub fn paint_scrollbar(bar: &gtk::Scrollbar, total: f64, view: f64) {
    let thumb_fraction = thumb_geared(total, bar_width(bar));
    let adjustment = gtk::Adjustment::new(
        origin(),
        0.0,
        total.max(view),
        pan_eighth(view),
        view,
        view * thumb_fraction,
    );
    bar.set_adjustment(Some(&adjustment));
    bar.set_visible(!fits(total, view));
}

/// The bar's own length: what it really is once GTK has laid it out, or the width it was last refreshed
/// with (`refresh_scrollbar` records it) while its allocation is still 0.
fn bar_width(bar: &gtk::Scrollbar) -> f64 {
    let measured = f64::from(bar.width());
    if measured > 0.0 {
        measured
    } else {
        BAR_WIDTH.with(|cell| cell.get())
    }
}

/// The width the thumb gear of the LAST repaint used — `bar_width`'s answer read without a widget in hand,
/// so a check can compare its own numbers against the same length the painter did (a headless bar reports
/// 0 for its allocation, and another window's refresh may have been the last one to write it).
pub fn gear_px_for_test() -> f64 {
    BAR_WIDTH.with(|cell| cell.get())
}

/// The scrollbar's `value-changed` wire (§A: the bar scrolls the band). A `Scrollbar` is not a `Range` in
/// GTK 4 — they share only `Scrollable` — so this takes the adjustment, which is where a bar's value lives.
pub fn on_scroll_changed<F: Fn(f64) + 'static>(
    adjustment: &gtk::Adjustment,
    on_scroll: F,
) -> glib::SignalHandlerId {
    adjustment.connect_value_changed(move |a| on_scroll(a.value()))
}

/// Repaint THIS window's scrollbar and band from its live zoom, scroll and session length, and hide the
/// bar when everything fits. Looked up in THIS window's tree, never by a search that could land on another
/// tab's copy (the `fold-all-button` lesson). The door `window.rs` calls after any zoom, scroll or cut.
pub fn refresh_page_scrollbar(window: &adw::ApplicationWindow) {
    let Some(content) = window.content() else {
        return;
    };
    // Same door as [`repaint_page`]: a zoom or a scroll can change what the scene costs, and the bar beside it
    // is repainted from those same numbers.
    crate::ui::cut_tracks::fit_band_height_for_window(window, &band_scene_for(window));
    if let Some(bar) = find_widget_by_name(&content, "track-scrollbar")
        .and_then(|bar| bar.downcast::<gtk::Scrollbar>().ok())
    {
        refresh_scrollbar(&bar, surface_width(window));
    }
    for name in ["track-band", "track-strip"] {
        if let Some(band) = find_widget_by_name(&content, name) {
            band.queue_draw();
        }
    }
}

/// The bar's length as the page can give it: its own allocation once it has one, the surface it scrolls
/// otherwise. Recorded through `refresh_scrollbar`, which is the door that stores it (`BAR_WIDTH`).
fn naivepost_length_of(bar: &gtk::Widget, window: &adw::ApplicationWindow) {
    let Some(scrollbar) = bar.downcast_ref::<gtk::Scrollbar>() else {
        return;
    };
    refresh_scrollbar(scrollbar, surface_width(window));
}

/// This window's band scene, from state the page already keeps: the newest review cut, this page's takes,
/// the newest window's selection and red line — the same newest-slot doors every other Cut-page reader
/// uses, so the band cannot paint an older window's marks. Lives here because `window.rs` is at its budget.
pub fn band_scene_for(window: &adw::ApplicationWindow) -> crate::ui::cut_tracks::Scene {
    use crate::ui::cut_tracks;
    let recordings = cut_folds::page_recordings();
    let cut_ = crate::ui::window::review_cut_of(window);
    let held = crate::ui::window::selection(window);
    cut_tracks::scene_from(
        &cut_,
        &recordings,
        held.map(|band| (band.start, band.end)),
        crate::ui::window::watched_row(),
        crate::ui::window::preview_player(window)
            .playhead
            .unwrap_or(0.0),
    )
}

/// Attach THIS window's `track-scrollbar` to the view: moving the thumb sets the origin and redraws what
/// moved. Built here rather than in `window.rs`, which is past its size budget; the widget itself is made
/// by `cut_tracks::scrollbar`.
pub fn attach_scrollbar(window: &adw::ApplicationWindow) {
    let Some(content) = window.content() else {
        return;
    };
    let Some(widget) = find_widget_by_name(&content, "track-scrollbar") else {
        return;
    };
    // A bar §1 says is hidden has no allocation, and an unmapped scrollbar answers 0 px on both axes. The
    // gearing is defined against the BAR's length (§B: "thumb geared at high zoom so one screenful is always
    // 40 px of drag"), so the length has to be recorded while the bar is mapped and page-wide — which is true
    // the moment the page is realized, whatever the tape's width turns out to be. Without this the bar is
    // never mapped at the open zoom (the session fits), stays 0×0, and every thumb it later paints is geared
    // against nothing.
    let bar = widget.clone();
    let win = window.clone();
    widget.connect_realize(move |_| {
        naivepost_length_of(&bar, &win);
    });
    let Ok(bar) = widget.downcast::<gtk::Scrollbar>() else {
        return;
    };
    let adjustment = bar.adjustment();
    let win = window.clone();
    on_scroll_changed(&adjustment, move |value| {
        let view = view_px(surface_width(&win));
        set_origin(value, total_px(filmed_seconds(), pps()), view);
        repaint_page(&win);
    });
}
