//! §05-cut.md §1 badge **31** and the wheel's three gestures: what part of the timeline is on screen.

use adw::prelude::*;
use gtk4 as gtk;

use crate::ui::window::find_widget_by_name;
use crate::{cut_screen, timeline, ui::cut_folds};

pub const DRAG_PER_SCREENFUL_PX: f64 = 40.0;

thread_local! {
    /// No zoom was saved for this project and none has been chosen: the session stays fitted to the band.
    static FITS_THE_BAND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The project the zoom was read from, and where a new one is kept.
    static ZOOM_PROJECT: std::cell::RefCell<Option<crate::layout::Tree>> = const { std::cell::RefCell::new(None) };
}

thread_local! {
    /// The band's pixels-per-second, seeded at the open zoom: the one zoom the band paints with, and the one the
    /// toolbar's −/+ and the wheel both step.
    static PPS: std::cell::Cell<f64> = const { std::cell::Cell::new(cut_screen::ZOOM_AT_OPEN) };
    /// The left edge of the view, in timeline pixels from the start of the gutter.
    static ORIGIN: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// The session's filmed seconds: the takes' own spans merged, so dropped stretches still count (§B:
/// a fold is a view, not an edit). Read through `cut_folds`, which already holds this window's takes.
pub fn filmed_seconds() -> f64 {
    timeline::filmed_runs(&cut_folds::page_recordings())
        .iter()
        .map(|(start, end)| end - start)
        .sum()
}

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
    FITS_THE_BAND.with(|cell| cell.set(false));
    PPS.with(|cell| cell.set(pps.max(0.0)));
}

/// This window's zoom floor: the whole filmed session across the view minus the gutter (§A), which is
/// `timeline::floor_pps` over the takes' merged spans — never the constant the placeholder strip was
/// sized with, because that number knows nothing about how long this session is.
pub fn floor_pps(view: f64) -> f64 {
    timeline::floor_pps(&timeline::filmed_runs(&cut_folds::page_recordings()), view)
}

pub fn live_pps(view: f64) -> f64 {
    pps().clamp(
        floor_pps(view).min(cut_screen::ZOOM_MAX),
        cut_screen::ZOOM_MAX,
    )
}

/// How far the view starts, in pixels: 0 shows the session from its first filmed second.
pub fn origin() -> f64 {
    ORIGIN.with(|cell| cell.get())
}

/// The strip's pixels-per-second right now: the stored zoom over this strip's own view (§2). One door, so
/// the paint, the press geometry and every test read the SAME number the layout was drawn with.
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
pub fn band_clip_boxes_for_test() -> Vec<crate::cut_trim::Box_> {
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

fn clamp_origin(px: f64, total: f64, view: f64) -> f64 {
    px.clamp(0.0, (total - gutter_px() - view).max(0.0))
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
    total - gutter_px() <= view + 1e-9
}

/// Tape pixels one pixel of thumb drag moves when the drag is geared, or `None` while GTK's own drag
/// already costs at least [`DRAG_PER_SCREENFUL_PX`] per screenful (§1's gear engages at high zoom only).
pub fn drag_gear(total: f64, view: f64, bar_px: f64) -> Option<f64> {
    if bar_px <= 0.0 || view <= 0.0 {
        return None;
    }
    let geared = view / DRAG_PER_SCREENFUL_PX;
    (total / bar_px > geared).then_some(geared)
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
    let centre = view / 2.0;
    // Step from the zoom the band is drawn at. + never stops under the fit (from a stored zoom below it, the
    // first press goes straight to the fit); − never WIDENS the tape: a band already showing the whole session
    // at less than the fit stays where it is, and the press says the session is on screen.
    let current = pps().min(cut_screen::ZOOM_MAX);
    let lowest = if inward { floor } else { floor.min(current) };
    let (next, scrolled) = cut_screen::zoom_around(
        current,
        next_of(current, inward, lowest) / current,
        centre,
        origin(),
        lowest,
    );
    set_pps(next);
    keep_zoom();
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
    // `(anchor - gutter + origin) / pps` and the scroll back has to be solved in that same space.
    let current = pps().min(cut_screen::ZOOM_MAX);
    let lowest = if factor < 1.0 {
        floor.min(current)
    } else {
        floor
    };
    let (next, scrolled) =
        cut_screen::zoom_around(current, factor, anchor_x - gutter_px(), origin(), lowest);
    // `set_pps` FIRST: the clamped scroll below is measured at the zoom it leaves behind, and reading a
    // stale one here is how a zoom "keeps" a second that has already moved.
    set_pps(next);
    keep_zoom();
    // `total` was measured at the old zoom: the film's part of it scales with the zoom, the gutter does not. Clamped
    // at the old length, a zoom in near the tape's end moved the view and pushed the anchor away.
    let film = (total - gutter_px()).max(0.0);
    let total = gutter_px() + film * next / current;
    (next, set_origin(scrolled, total, view))
}

/// How far across the view a moving line may run before the view moves with it: the last quarter stays ahead of it.
pub const LINE_HOLD_FRACTION: f64 = 0.75;

pub fn reveal_playhead(playhead_px: f64, total: f64, view: f64) -> f64 {
    let held = origin();
    if playhead_px < held {
        set_origin(playhead_px - view / 2.0, total, view)
    } else if playhead_px > held + view * LINE_HOLD_FRACTION {
        set_origin(playhead_px - view * LINE_HOLD_FRACTION, total, view)
    } else {
        held
    }
}

/// [`reveal_playhead`] on this window's own view. The transport writes the line through `window.rs`'s
/// `set_playhead`, which calls this on every tick.
pub fn follow_playhead(window: &adw::ApplicationWindow, at: f64) {
    let view = view_px(surface_width(window));
    let total = total_px(filmed_seconds(), pps());
    let runs = timeline::filmed_runs(&cut_folds::page_recordings());
    let folds = crate::ui::window_preview::newest_review_cut().folds;
    reveal_playhead(line_px(&runs, &folds, pps(), at), total, view);
    repaint_page(window);
}

pub fn keep_line_through_layout(window: &adw::ApplicationWindow, layout: impl FnOnce()) {
    let at = crate::ui::window::line_position().t;
    let view = view_px(surface_width(window));
    let runs = timeline::filmed_runs(&cut_folds::page_recordings());
    let folds = crate::ui::window_preview::newest_review_cut().folds;
    let on_screen = (origin()..=origin() + view).contains(&line_px(&runs, &folds, pps(), at));
    layout();
    settle_view(window);
    if on_screen {
        follow_playhead(window, at);
    }
}

/// The band got its real width, or the takes landed: a view scrolled while the page was unmeasured may stand past
/// the tape's end, and an unchosen zoom is fitted again.
pub fn settle_view(window: &adw::ApplicationWindow) {
    open_zoom();
    let view = view_px(surface_width(window));
    if FITS_THE_BAND.with(std::cell::Cell::get) {
        PPS.with(|cell| cell.set(floor_pps(view).min(cut_screen::ZOOM_MAX)));
    }
    set_origin(origin(), total_px(filmed_seconds(), pps()), view);
    repaint_page(window);
}

static OPEN_ZOOM_HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A test whose pixels are worked out at a stated zoom keeps it through the page's first layout.
pub fn hold_open_zoom_for_test(held: bool) {
    OPEN_ZOOM_HELD.store(held, std::sync::atomic::Ordering::SeqCst);
}

/// The first look at a project: its saved zoom, or the fit.
fn open_zoom() {
    if OPEN_ZOOM_HELD.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some(tree) =
        crate::ui::window::line_root().and_then(|dir| crate::layout::Tree::new(dir).ok())
    else {
        return;
    };
    if ZOOM_PROJECT.with(|slot| slot.borrow().as_ref().map(|open| open.dir().to_path_buf()))
        == Some(tree.dir().to_path_buf())
    {
        return;
    }
    let saved = std::fs::read_to_string(tree.zoom_txt())
        .ok()
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|zoom| *zoom > 0.0);
    match saved {
        Some(zoom) => set_pps(zoom.min(cut_screen::ZOOM_MAX)),
        None => FITS_THE_BAND.with(|cell| cell.set(true)),
    }
    ZOOM_PROJECT.with(|slot| *slot.borrow_mut() = Some(tree));
}

/// A person's zoom is kept for the next time this project opens.
fn keep_zoom() {
    ZOOM_PROJECT.with(|slot| {
        if let Some(tree) = slot.borrow().as_ref() {
            let rel = tree.zoom_txt();
            let rel = rel.strip_prefix(tree.dir()).unwrap_or(&rel).to_path_buf();
            if let Err(why) = tree.write_file(&rel, format!("{:.4}", pps()).as_bytes()) {
                crate::ui::window::log_line(&format!("!!! could not save the zoom -- {why}"));
            }
        }
    });
}

pub fn line_px(runs: &[(f64, f64)], folds: &[[f64; 2]], pps: f64, at: f64) -> f64 {
    let span = timeline::Span::new(timeline::cells(runs, folds), pps, 0.0);
    span.x_of(at)
}

/// §1's three wheel gestures, attached to the widgets they belong to: transport bar and preview step
/// frames (Shift five), the tracks zoom around the cursor and pan an eighth with Shift — which gesture a
/// notch is stays `cut_screen::wheel`'s rule. `wire_wheel_gestures` attaches them; this is what one does.
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
    apply_wheel_at(window, over, shift, dy, dx, 0.0, on_step)
}

/// [`apply_wheel`] with the pointer's x on the band (page pixels), so a zoom keeps the second under the
/// cursor where it is (§1: "over the tracks it zooms around the cursor"); 0.0 means no pointer is known.
pub fn apply_wheel_at<S: Fn(i64)>(
    window: &adw::ApplicationWindow,
    over: cut_screen::WheelOver,
    shift: bool,
    dy: f64,
    dx: f64,
    pointer_x: f64,
    on_step: S,
) -> Option<String> {
    let view = wheel_gestures_view(window);
    match cut_screen::wheel(over, shift, dy, dx, view) {
        cut_screen::Wheel::StepFrames { frames } => {
            on_step(frames);
            None
        }
        cut_screen::Wheel::Zoom { factor, .. } => apply_wheel_result(
            window,
            cut_screen::Wheel::Zoom {
                factor,
                at_x: pointer_x,
            },
        ),
        action => apply_wheel_result(window, action),
    }
}

thread_local! {
    /// Where a Shift pan is gliding to, and where it last put the view, while one is under way.
    static GLIDE: std::cell::Cell<Option<(f64, f64)>> = const { std::cell::Cell::new(None) };
}

/// Move the view `px` over a few frames instead of in one jump. A notch during a glide adds to the target; anything
/// else that moves the view (a zoom, the scrollbar, the line) ends it, since the view is then no longer where the glide left it.
fn glide_pan(window: &adw::ApplicationWindow, px: f64) {
    let runs = timeline::filmed_runs(&cut_folds::page_recordings());
    let filmed: f64 = runs.iter().map(|(start, end)| end - start).sum();
    let view = wheel_gestures_view(window);
    let total = total_px(filmed, pps());
    let running = GLIDE.with(|glide| glide.get());
    let from = running.map_or_else(origin, |(target, _)| target);
    GLIDE.with(|glide| {
        glide.set(Some((
            clamp_origin(from + px, total, view),
            running.map_or_else(origin, |(_, left)| left),
        )))
    });
    if running.is_some() {
        return;
    }
    let held = window.clone();
    window.add_tick_callback(move |_, _| {
        let Some((target, left)) = GLIDE.with(|glide| glide.get()) else {
            return glib::ControlFlow::Break;
        };
        if (origin() - left).abs() > 0.01 {
            GLIDE.with(|glide| glide.set(None));
            return glib::ControlFlow::Break;
        }
        let total = total_px(
            timeline::filmed_runs(&cut_folds::page_recordings())
                .iter()
                .map(|(start, end)| end - start)
                .sum(),
            pps(),
        );
        let moved = set_origin(
            cut_screen::glide_step(left, target),
            total,
            wheel_gestures_view(&held),
        );
        repaint_page(&held);
        if (moved - target).abs() < 0.01 {
            GLIDE.with(|glide| glide.set(None));
            return glib::ControlFlow::Break;
        }
        GLIDE.with(|glide| glide.set(Some((target, moved))));
        glib::ControlFlow::Continue
    });
}

fn pointer_x_on(ctrl: &gtk::EventControllerScroll, widget: &gtk::Widget, hovered: f64) -> f64 {
    let (Some(event), Some(native)) = (ctrl.current_event(), widget.native()) else {
        return hovered;
    };
    let Some((x, y)) = event.position() else {
        return hovered;
    };
    let (sx, sy) = native.surface_transform();
    native
        .compute_point(
            widget,
            &gtk::graphene::Point::new((x - sx) as f32, (y - sy) as f32),
        )
        .map(|point| f64::from(point.x()))
        .unwrap_or(0.0)
}

/// The three wheel gestures of §1, attached to the widgets they belong to.
pub fn attach_wheel<S>(
    window: &adw::ApplicationWindow,
    targets: &[(&'static str, cut_screen::WheelOver)],
    on_step: S,
) where
    S: Fn(&adw::ApplicationWindow, i64) + 'static,
{
    let on_step = std::rc::Rc::new(on_step);
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
        // Where the pointer last stood over this ground, for a notch that arrives without a position.
        let hovered = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));
        let motion = gtk::EventControllerMotion::new();
        let seen = hovered.clone();
        motion.connect_motion(move |_, x, _| seen.set(x));
        let seen = hovered.clone();
        motion.connect_enter(move |_, x, _| seen.set(x));
        widget.add_controller(motion);
        let win = window.clone();
        let step = on_step.clone();
        controller.connect_scroll(move |ctrl, dx, dy| {
            let shift = ctrl
                .current_event_state()
                .contains(gtk::gdk::ModifierType::SHIFT_MASK);
            // A pan on the tracks moves smoothly: a touchpad's own pixels as they come, a wheel's notch as a glide.
            if let cut_screen::Wheel::Pan { px } =
                cut_screen::wheel(over, shift, dy, dx, wheel_gestures_view(&win))
            {
                let delta = if dx.abs() > dy.abs() { dx } else { dy };
                glide_or_follow(
                    &win,
                    ctrl.unit() == gtk::gdk::ScrollUnit::Surface,
                    delta,
                    px,
                );
                return glib::Propagation::Stop;
            }
            let step2 = step.clone();
            let for_step = win.clone();
            // The band is the tape itself, so its pixels are the page pixels a zoom anchors at; any other
            // ground zooms about the middle.
            let at = match ctrl.widget() {
                Some(widget) if name == "track-band" => pointer_x_on(ctrl, &widget, hovered.get()),
                _ => 0.0,
            };
            apply_wheel_at(&win, over, shift, dy, dx, at, move |frames| {
                step2(&for_step, frames)
            });
            glib::Propagation::Stop
        });
        widget.add_controller(controller);
    }
}

fn glide_or_follow(window: &adw::ApplicationWindow, pixels: bool, delta: f64, notch_px: f64) {
    if pixels {
        // The fingers are the animation: following them directly, a glide would only lag behind.
        GLIDE.with(|glide| glide.set(None));
        apply_wheel_result(window, cut_screen::Wheel::Pan { px: delta });
    } else {
        glide_pan(window, notch_px * delta.abs().max(1.0));
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
            // The red line stays where it stands while it is on screen, as the −/+ buttons keep it. Off screen the
            // zoom goes around `at_x`, the pointer on the band in page pixels; 0.0 (no pointer known) is the middle.
            let folds = crate::ui::window_preview::newest_review_cut().folds;
            let at = crate::ui::window::line_position().t;
            let line = line_px(&runs, &folds, pps(), at);
            let anchor = if (origin()..=origin() + view).contains(&line) {
                line - origin() + gutter_px()
            } else if at_x == 0.0 {
                view / 2.0 + gutter_px()
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
    crate::ui::cut_tracks::fit_band_height_for_window(window, &band_scene_for());
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
    static BAR_WIDTH: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

pub fn paint_scrollbar(bar: &gtk::Scrollbar, total: f64, view: f64) {
    bar.adjustment().configure(
        origin(),
        0.0,
        total.max(view),
        cut_screen::pan_px(view),
        view * 0.9,
        view,
    );
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
    crate::ui::cut_tracks::fit_band_height_for_window(window, &band_scene_for());
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
pub fn band_scene_for() -> crate::ui::cut_tracks::Scene {
    use crate::ui::cut_tracks;
    let recordings = cut_folds::page_recordings();
    let cut_ = crate::ui::window_preview::newest_review_cut();
    let held = crate::ui::window::selection();
    cut_tracks::scene_from(
        &cut_,
        &recordings,
        held.map(|band| (band.start, band.end)),
        crate::ui::window::watched_row(),
        crate::ui::window::preview_player().playhead.unwrap_or(0.0),
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
        // A repaint re-states the value it already holds; only a real move scrolls.
        if (value - origin()).abs() < 0.5 {
            return;
        }
        let view = view_px(surface_width(&win));
        set_origin(value, total_px(filmed_seconds(), pps()), view);
        repaint_page(&win);
    });
    // §1's gear: a press on the bar claims the drag only while GTK's own drag would cross a screenful in
    // under 40 px, and then moves the view `view / 40` per pixel from where the press found it.
    let gear = gtk::GestureDrag::new();
    gear.set_propagation_phase(gtk::PropagationPhase::Capture);
    let pressed_at: std::rc::Rc<std::cell::Cell<Option<f64>>> = Default::default();
    let (held, win, measured) = (pressed_at.clone(), window.clone(), bar.clone());
    gear.connect_drag_begin(move |gesture, _x, _y| {
        let (total, view) = extents(surface_width(&win));
        if drag_gear(total, view, bar_width(&measured)).is_some() {
            held.set(Some(origin()));
            gesture.set_state(gtk::EventSequenceState::Claimed);
        } else {
            held.set(None);
            gesture.set_state(gtk::EventSequenceState::Denied);
        }
    });
    let (held, win, measured) = (pressed_at, window.clone(), bar.clone());
    gear.connect_drag_update(move |_gesture, dx, _dy| {
        let Some(from) = held.get() else {
            return;
        };
        let (total, view) = extents(surface_width(&win));
        if let Some(per_px) = drag_gear(total, view, bar_width(&measured)) {
            set_origin(from + dx * per_px, total, view);
            repaint_page(&win);
        }
    });
    bar.add_controller(gear);
}
