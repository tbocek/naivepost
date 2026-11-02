//! §07-narrate#1-screen, number **10** on the page: the take band's painted surface and its hand.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_band::{self as band, BandPaint};
use crate::narrate_screen as screen;
use crate::ui::cut_wave_rows;
use crate::ui::narrate_page_state::{NarratePage, PAGE as NARRATE_PAGE};
use crate::wave_build;

/// The lane's height, from the module that catalogues it (`narrate.bandLanePx`).
const LANE_PX: i32 = screen::BAND_LANE_PX as i32;
/// The lane's own minimum width, the number `build_band` requests before any view exists.
pub const LANE_MIN_PX: f64 = 240.0;

pub fn lane_width(zoom: f64, duration: f64, viewport: f64) -> f64 {
    band::content_px(zoom, duration, viewport).max(LANE_MIN_PX)
}

/// The ruler strip painted across the top of the lane (`narrate.bandRulerPx`).
const RULER_PX: f64 = screen::BAND_RULER_PX;
/// Wave peaks arrive at this rate from [`wave_build::ensure`]'s cache.
const PEAK_HZ: f64 = wave_build::PEAK_HZ;
/// The red bar's width in px: two, so it reads over the ruler and the lane as ONE line.
const BAR_PX: f64 = 2.0;

pub fn build_band() -> (gtk::ScrolledWindow, gtk::DrawingArea) {
    let lane = gtk::DrawingArea::new();
    lane.set_widget_name("take-band");
    lane.set_size_request(240, LANE_PX);
    lane.add_css_class("narrate-plate");
    // `Fill`, not `Start`: an area that hugs its own minimum keeps its 240 px request whatever the
    // paned offers, so the band would be a stub in a 560 px column and every px/s mapping would be
    // measured against the wrong width.
    lane.set_halign(gtk::Align::Fill);
    lane.set_hexpand(true);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_widget_name("take-band-scroller");
    // `Automatic` horizontally so §1's "own scrollbar once the wheel zooms past fit" appears exactly then.
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::External);
    scroller.set_child(Some(&lane));
    scroller.set_propagate_natural_height(false);
    scroller.set_min_content_height(LANE_PX);
    scroller.set_max_content_height(LANE_PX);
    (scroller, lane)
}

pub fn attach(window: &adw::ApplicationWindow) {
    let Some(lane) = lane_of(window) else { return };
    lane.set_draw_func(move |_area, cr, width, height| {
        paint(cr, width.max(1) as f64, height.max(1) as f64);
    });

    // ONE drag gesture, button 1: press and motion carry the live selection, release decides what it
    // became. No second button-1 gesture on this widget — two would race and GTK's order, not the
    // person's intent, would choose between selecting and setting the bar.
    let drag = gtk::GestureDrag::new();
    drag.set_button(1);
    let begin_win = window.clone();
    drag.connect_drag_begin(move |_g, x, _y| {
        note_press(x);
        repaint(&begin_win);
    });
    // GTK hands `drag-update` and `drag-end` the OFFSET from the press, so the pointer's x is the press plus it.
    let move_win = window.clone();
    drag.connect_drag_update(move |_g, dx, _y| {
        note_motion(NARRATE_PAGE.with(|NarratePage { press_x: slot, .. }| slot.get()) + dx);
        repaint(&move_win);
    });
    let end_win = window.clone();
    drag.connect_drag_end(move |_g, dx, _y| {
        finish_drag(
            &end_win,
            NARRATE_PAGE.with(|NarratePage { press_x: slot, .. }| slot.get()) + dx,
        );
    });
    lane.add_controller(drag.clone());
    NARRATE_PAGE.with(|NarratePage { drag: slot, .. }| *slot.borrow_mut() = Some(drag));

    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_name(Some("wheel-take-band"));
    let scroll_win = window.clone();
    wheel.connect_scroll(move |ctrl, _dx, dy| {
        let shift = ctrl
            .current_event_state()
            .contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if shift {
            pan(&scroll_win, dy * band::pan_by(view_width()));
        } else {
            zoom_by(&scroll_win, -dy);
        }
        repaint(&scroll_win);
        glib::Propagation::Stop
    });
    lane.add_controller(wheel);

    // ONE motion controller: §1 wants a faint red bar under the pointer, which is not a drag and so
    // needs its own source of x. A motion controller grabs no button, so it cannot race the drag above.
    let motion = gtk::EventControllerMotion::new();
    motion.set_name(Some("motion-take-band"));
    let hover_win = window.clone();
    motion.connect_motion(move |_m, x, _y| {
        note_pointer(x);
        repaint(&hover_win);
    });
    let leave_win = window.clone();
    motion.connect_leave(move |_| {
        note_pointer(-1.0);
        repaint(&leave_win);
    });
    lane.add_controller(motion);

    note_lane_size(window);
    repaint(window);
}

pub fn note_lane_size(window: &adw::ApplicationWindow) {
    let Some(lane) = lane_of(window) else { return };
    let Some(seconds) = painted_seconds() else {
        return;
    };
    let scroller = match crate::ui::window::find_widget_by_name(
        &window.content().expect("the window has content"),
        "take-band-scroller",
    ) {
        Some(scroller) => scroller,
        None => return,
    };
    let viewport = NARRATE_PAGE.with(
        |NarratePage {
             view_width: slot, ..
         }| slot.get(),
    );
    if viewport <= 0.0 {
        return;
    }
    let wanted = if zoom() <= 1.0 {
        LANE_MIN_PX as i32
    } else {
        lane_width(zoom(), seconds, viewport) as i32
    };
    if lane.width_request() != wanted {
        lane.set_size_request(wanted, LANE_PX);
    }
    let _ = scroller;
}

pub const POINTER_COLOUR: (f64, f64, f64, f64) = (0.85, 0.25, 0.25, 0.35);

fn note_pointer(x: f64) {
    let inside = (0.0..=NARRATE_PAGE.with(
        |NarratePage {
             lane_width: slot, ..
         }| slot.get(),
    ))
        .contains(&x);
    NARRATE_PAGE.with(
        |NarratePage {
             pointer_x: slot, ..
         }| slot.set(if inside { x } else { -1.0 }),
    );
}

pub fn pointer_px() -> f64 {
    NARRATE_PAGE.with(
        |NarratePage {
             pointer_x: slot, ..
         }| slot.get(),
    )
}

pub fn show_only_with_recording(window: &adw::ApplicationWindow) {
    follow_tape();
    let Some(scroller) = crate::ui::window::find_widget_by_name(
        &window.content().expect("the window has content"),
        "take-band-scroller",
    ) else {
        return;
    };
    scroller.set_visible(recording_shown());
    // And the lane's width follows the seconds it is about to draw: the first fit ran while the scroller was
    // hidden (§A.7 hides it until a voice has a recording) and a hidden widget answers 0 px wide, so the lane
    // kept its 240 px request and the wave filled less than half the plate.
    note_lane_size(window);
}

pub fn recording_shown() -> bool {
    if let Some(tree) = crate::ui::window::narrate_session_tree() {
        if let Ok(project) = crate::project::load(tree.dir()) {
            if crate::narrate_band_face::recording_exists(
                &tree,
                &project,
                &crate::ui::narrate_page::read_state().voice,
            ) {
                return true;
            }
        }
    }
    // No recording behind the slot: the reference itself is still something to select from (§Readme: the
    // waveform shows "once the voice on offer has a recording on disk (`narrate/voice_ref_base.wav`)"), and
    // captions speak nothing, so they never get a band whatever is on disk.
    crate::ui::narrate_page::read_state().voice != crate::narrate_screen::CAPTIONS
        && crate::ui::narrate_band::recording_seconds().is_some()
}

/// The seconds the band spans, asked the same way: the built reference first, then the recording itself.
pub fn painted_seconds() -> Option<f64> {
    let tree = crate::ui::window::narrate_session_tree()?;
    let project = crate::project::load(tree.dir()).ok()?;
    crate::narrate_band_face::band_seconds(
        &tree,
        &project,
        &crate::ui::narrate_page::read_state().voice,
    )
}

pub fn band_gesture_for_test() -> Option<gtk::GestureDrag> {
    NARRATE_PAGE.with(|NarratePage { drag: slot, .. }| slot.borrow().clone())
}

pub fn set_selection_for_test(_window: &adw::ApplicationWindow, span: (f64, f64)) {
    NARRATE_PAGE.with(
        |NarratePage {
             selection: slot, ..
         }| *slot.borrow_mut() = Some(span),
    );
}

pub fn painted_view_for_test() -> (f64, f64) {
    let (duration, width) = size();
    (
        duration,
        crate::narrate_band::px_per_second(zoom(), duration, width),
    )
}

pub fn view_width_for_test() -> f64 {
    NARRATE_PAGE.with(
        |NarratePage {
             view_width: slot, ..
         }| slot.get(),
    )
}

pub const RED_BAR_COLOUR: (u8, u8, u8) = (219, 51, 46);

pub fn painted_plan_for_test(width: f64, height: f64) -> band::Plan {
    // The seconds `paint` measures for THIS width — asked through the painter's own door, so a bar or a
    // take is mapped with the number the ink will use (with no recording on disk that fallback is short,
    // and px/s is then clamped at `narrate.bandMaxPps`, which is exactly what the lane draws).
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).expect("a scratch surface");
    let probe = cairo::Context::new(&surface).expect("a scratch context");
    paint(&probe, width, height.max(1.0));
    // The plan is the one `paint` itself built for this exact width and left in nothing but its own frame,
    // so ask it again with the numbers `paint` uses: the seconds it measures (0.1 with no recording on
    // disk) and px/s over THIS width — which is what keeps a bar or take inside the image at all.
    let (seconds, pps) = painted_view_for_test();
    drop(probe);
    drop(surface);
    let state = crate::ui::narrate_page::read_state();
    let red_bar = red_bar_or(state.session);
    let mut plan = band::paint_plan(&band::BandPaint {
        duration: seconds,
        width,
        zoom: zoom(),
        // A standalone surface starts at the recording's left edge; a scrolled lane subtracts `offset`.
        offset: 0.0,
        pps,
        red_bar,
        pointer_x: -1.0,
        selection: None,
        takes: state.takes.clone(),
        session: state.session,
        playing: false,
        envelope_decoded: painted_seconds().is_some(),
    });
    // The painter keeps this one in step with its own frame; a check must read the same number it drew.
    plan.red_bar_px = band::second_to_px(red_bar, offset(), pps);
    plan
}

pub fn repaint(window: &adw::ApplicationWindow) {
    if let Some(lane) = lane_of(window) {
        lane.queue_draw();
    }
}

/// The view's fit-relative zoom.
pub fn zoom() -> f64 {
    NARRATE_PAGE.with(|NarratePage { zoom: slot, .. }| slot.get())
}

/// The view's offset in px.
pub fn offset() -> f64 {
    NARRATE_PAGE.with(|NarratePage { offset: slot, .. }| slot.get())
}

/// The red ▶-start bar's second.
pub fn red_bar() -> f64 {
    NARRATE_PAGE.with(|NarratePage { red_bar: slot, .. }| slot.get())
}

pub fn red_bar_or(session: f64) -> f64 {
    match NARRATE_PAGE.with(
        |NarratePage {
             red_bar_seen: slot, ..
         }| slot.get(),
    ) {
        true => red_bar(),
        false => band_second_of(session).unwrap_or(session.max(0.0)),
    }
}

/// The band second under a session second, for the voice in hand. `None` when no clip places its tape.
fn band_second_of(session: f64) -> Option<f64> {
    let tree = crate::ui::window::narrate_session_tree()?;
    let project = crate::project::load(tree.dir()).ok()?;
    let voice = crate::ui::narrate_page::read_state().voice;
    crate::narrate_band_face::tape(&tree, &project, &voice).band_at(session)
}

pub fn follow_head(session: f64) {
    if NARRATE_PAGE.with(
        |NarratePage {
             head_seen: slot, ..
         }| slot.replace(session),
    ) == session
    {
        return;
    }
    if NARRATE_PAGE.with(
        |NarratePage {
             red_bar_seen: slot, ..
         }| slot.get(),
    ) {
        if let Some(at) = band_second_of(session) {
            NARRATE_PAGE.with(|NarratePage { red_bar: slot, .. }| slot.set(at));
        }
    }
}

pub fn red_bar_seen_for_test() -> bool {
    NARRATE_PAGE.with(
        |NarratePage {
             red_bar_seen: slot, ..
         }| slot.get(),
    )
}

pub fn live_selection() -> Option<(f64, f64)> {
    NARRATE_PAGE.with(
        |NarratePage {
             selection: slot, ..
         }| *slot.borrow(),
    )
}

/// Put the red bar at a second (the door a test and any future shortcut use).
pub fn set_red_bar(seconds: f64) {
    NARRATE_PAGE.with(|NarratePage { red_bar: slot, .. }| slot.set(seconds.max(0.0)));
    NARRATE_PAGE.with(
        |NarratePage {
             red_bar_seen: slot, ..
         }| slot.set(true),
    );
}

pub fn clear_hand() {
    NARRATE_PAGE.with(
        |NarratePage {
             selection: slot, ..
         }| *slot.borrow_mut() = None,
    );
    NARRATE_PAGE.with(
        |NarratePage {
             pointer_x: slot, ..
         }| slot.set(-1.0),
    );
    NARRATE_PAGE.with(|NarratePage { zoom: slot, .. }| slot.set(1.0));
    NARRATE_PAGE.with(|NarratePage { offset: slot, .. }| slot.set(0.0));
    NARRATE_PAGE.with(|NarratePage { red_bar: slot, .. }| slot.set(0.0));
    NARRATE_PAGE.with(
        |NarratePage {
             red_bar_seen: slot, ..
         }| slot.set(false),
    );
}

fn note_press(x: f64) {
    NARRATE_PAGE.with(|NarratePage { press_x: slot, .. }| slot.set(x));
    NARRATE_PAGE.with(
        |NarratePage {
             selection: slot, ..
         }| *slot.borrow_mut() = None,
    );
}

fn note_motion(x: f64) {
    NARRATE_PAGE.with(
        |NarratePage {
             selection: slot, ..
         }| *slot.borrow_mut() = selected(x),
    );
}

fn selected(x: f64) -> Option<(f64, f64)> {
    let press = NARRATE_PAGE.with(|NarratePage { press_x: slot, .. }| slot.get());
    // [`screen::take_press`] owns the slop and direction rule; only its CASE is
    // asked here, because the seconds come from this lane's own offset and px/s, which that rule does not
    // know about. A click (SetBar) selects nothing — it moves the red bar instead.
    match screen::take_press(press, x, 0.0) {
        screen::TakePress::SetBar => None,
        screen::TakePress::Select { .. } => {
            let (start, pps) = view();
            // The left edge clamps itself; a drag off the right end stops where the tape does.
            let end = painted_seconds().unwrap_or(f64::MAX);
            Some((
                band::x_to_second(press.min(x), start, pps).min(end),
                band::x_to_second(press.max(x), start, pps).min(end),
            ))
        }
    }
}

fn finish_drag(window: &adw::ApplicationWindow, x: f64) {
    match selected(x) {
        Some(span) => NARRATE_PAGE.with(
            |NarratePage {
                 selection: slot, ..
             }| *slot.borrow_mut() = Some(span),
        ),
        None => {
            let (start, pps) = view();
            let at = band::x_to_second(x, start, pps);
            set_red_bar(at);
            NARRATE_PAGE.with(
                |NarratePage {
                     selection: slot, ..
                 }| *slot.borrow_mut() = None,
            );
            crate::ui::narrate_page_take::band_clicked(window, at);
        }
    }
    repaint(window);
}

/// Zoom by `steps` notches (positive = bigger), clamped so the view cannot leave the file.
fn zoom_by(window: &adw::ApplicationWindow, steps: f64) {
    if zoom() <= 1.0 {
        // Leaving fit: what shows now is the width every later zoom is measured against.
        NARRATE_PAGE.with(
            |NarratePage {
                 view_width: slot, ..
             }| {
                slot.set(NARRATE_PAGE.with(
                    |NarratePage {
                         lane_width: lane, ..
                     }| lane.get(),
                ))
            },
        );
    }
    let (duration, view) = size();
    let before = band::px_per_second(zoom(), duration, view);
    let adjustment = scroller_of(window).map(|scroller| scroller.hadjustment());
    let scrolled = adjustment.as_ref().map_or(0.0, |a| a.value());
    // The second under the pointer stays under it (the view's middle when the pointer is off the band).
    let pointer = NARRATE_PAGE.with(
        |NarratePage {
             pointer_x: slot, ..
         }| slot.get(),
    );
    let anchor = if pointer >= 0.0 {
        pointer
    } else {
        scrolled + view / 2.0
    };
    let (second, on_screen) = (anchor / before.max(f64::EPSILON), anchor - scrolled);
    let zoom = band::zoom_step(zoom(), -steps);
    NARRATE_PAGE.with(|NarratePage { zoom: slot, .. }| slot.set(zoom));
    note_lane_size(window);
    let after = band::px_per_second(zoom, duration, view);
    if let Some(adjustment) = adjustment {
        glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || {
            adjustment.set_value(second * after - on_screen);
        });
    }
}

/// Pan the view by `delta` px (negative shows earlier seconds), clamped inside the recording.
fn pan(window: &adw::ApplicationWindow, delta: f64) {
    if let Some(scroller) = scroller_of(window) {
        let adjustment = scroller.hadjustment();
        adjustment.set_value(adjustment.value() + delta);
    }
}

/// The band's scroller: zoomed in, it shows one view's width of the lane and scrolls through the rest.
fn scroller_of(window: &adw::ApplicationWindow) -> Option<gtk::ScrolledWindow> {
    crate::ui::window::find_widget_by_name(&window.content()?, "take-band-scroller")?
        .downcast()
        .ok()
}

/// Where the take walk's playhead stands (seconds of the recording), or none.
pub fn set_walk_head(at: Option<f64>) {
    NARRATE_PAGE.with(
        |NarratePage {
             walk_head: slot, ..
         }| slot.set(at.unwrap_or(-1.0)),
    );
}

/// The take walk's playhead, for a test: negative while no walk plays.
pub fn walk_head() -> f64 {
    NARRATE_PAGE.with(
        |NarratePage {
             walk_head: slot, ..
         }| slot.get(),
    )
}

/// `(duration, width)` of the view as last painted.
fn size() -> (f64, f64) {
    (
        NARRATE_PAGE.with(|NarratePage { duration: slot, .. }| slot.get()),
        NARRATE_PAGE.with(
            |NarratePage {
                 view_width: slot, ..
             }| slot.get(),
        ),
    )
}

/// `(offset, pps)` the mapping runs on right now.
fn view() -> (f64, f64) {
    let (duration, width) = size();
    (offset(), band::px_per_second(zoom(), duration, width))
}

/// The lane's width as the last paint saw it, for a wheel that pans before a repaint.
fn view_width() -> f64 {
    NARRATE_PAGE.with(
        |NarratePage {
             view_width: slot, ..
         }| slot.get(),
    )
}

pub fn painted_surface_for_test(
    width: i32,
    height: i32,
) -> Result<cairo::ImageSurface, cairo::Error> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    {
        let cr = cairo::Context::new(&surface)?;
        paint(&cr, width as f64, height as f64);
    }
    Ok(surface)
}

pub fn painted_pixels_for_test(
    width: i32,
    height: i32,
) -> Result<(Vec<u8>, i32, i32, f64, f64), cairo::Error> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    let (seconds, pps) = painted_view_for_test();
    let stride = surface.stride() as usize;
    {
        let cr = cairo::Context::new(&surface)?;
        paint(&cr, width as f64, height.max(1) as f64);
    }
    let bytes = surface
        .data()
        .map_err(|_| cairo::Error::InvalidFormat)?
        .as_ref()
        .to_vec();
    // Straight to `(r, g, b)` triples, premultiplied exactly as cairo laid it down: ARgb32 on a
    // little-endian machine stores blue, green, red, alpha, so a faint bar arrives as ground + a third of
    // its own colour — which is what a check compares against [`POINTER_COLOUR`] blended the same way.
    let mut rgb = Vec::with_capacity((width * height) as usize * 3);
    for row in 0..height as usize {
        for column in 0..width as usize {
            let at = row * stride + column * 4;
            rgb.extend_from_slice(&[bytes[at + 2], bytes[at + 1], bytes[at]]);
        }
    }
    Ok((rgb, width, height, seconds, pps))
}

/// This window's take band.
fn lane_of(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    let content = window.content()?;
    crate::ui::window::find_widget_by_name(&content, "take-band")?
        .downcast()
        .ok()
}

pub fn recording_seconds() -> Option<f64> {
    let tree = crate::ui::window::narrate_session_tree()?;
    let reference = tree.voice_ref_base_wav();
    Some(crate::voice_ref::wav_seconds(&reference)?.max(0.1))
}

fn envelope(
    tree: &crate::layout::Tree,
    lane: &str,
    source: &std::path::Path,
) -> Result<Vec<u8>, String> {
    let mut said = Vec::new();
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    crate::decode_audio::ensure(tree, lane, source, &configured, &mut |line| {
        said.push(line.to_string())
    })?;
    wave_build::ensure(tree, lane, source, &mut |line| said.push(line.to_string()))?;
    let (size, mtime) = stamp(source)?;
    let wave = crate::wave::read(tree, lane, size, mtime)?
        .ok_or_else(|| format!("{}: no envelope cached", source.display()))?;
    let detail = if said.is_empty() {
        String::new()
    } else {
        format!(" ({})", said.join("; "))
    };
    wave.chans
        .into_iter()
        .next()
        .filter(|peaks| !peaks.is_empty())
        .ok_or_else(|| format!("{}: empty envelope{detail}", source.display()))
}

/// `stamp` is private to `wave_build`, so the band reads the same two numbers itself.
fn stamp(path: &std::path::Path) -> Result<(i64, i64), String> {
    let meta = std::fs::metadata(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let mtime = meta
        .modified()
        .map_err(|err| format!("{}: {err}", path.display()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("{}: {err}", path.display()))?
        .as_secs() as i64;
    Ok((meta.len() as i64, mtime))
}

pub fn paint(cr: &cairo::Context, width: f64, height: f64) {
    // `width` is the LANE: at fit it is also what shows, zoomed in it is the whole recording at the zoom, and
    // px/s is measured against what shows (`VIEW_WIDTH`), so the lane is exactly the recording's width.
    NARRATE_PAGE.with(
        |NarratePage {
             lane_width: slot, ..
         }| slot.set(width),
    );
    if zoom() <= 1.0 {
        NARRATE_PAGE.with(
            |NarratePage {
                 view_width: slot, ..
             }| slot.set(width),
        );
    }
    let state = crate::ui::narrate_page::read_state();
    let seconds = painted_seconds();
    NARRATE_PAGE
        .with(|NarratePage { duration: slot, .. }| slot.set(seconds.unwrap_or(0.0).max(0.1)));
    let (duration, view) = size();
    let pps = band::px_per_second(zoom(), duration, view);
    // Keep the lane as wide as the file it draws. GTK gives no size-allocate hook to hang this on and the
    // first fit runs while the scroller is still hidden (a hidden widget answers 0 px wide), so this is where
    // the real numbers first arrive: without it the lane kept its 240 px request and a 45 s recording fit
    // across a 560 px column stopped two fifths of the way across the plate.

    // The ground, so the plate's CSS colour is under everything even where cairo paints first.
    cr.set_source_rgb(0.11, 0.11, 0.115);
    cr.rectangle(0.0, 0.0, width, height);
    let _ = cr.fill();

    // Read off the cache: a draw func never decodes (§03-shell §1). A miss asks a worker and the lane says
    // "reading the recording…" until the peaks land and the band is drawn again.
    let peaks = cached_envelope(&state);
    let mut plan = band::paint_plan(&BandPaint {
        duration,
        width,
        zoom: zoom(),
        offset: offset(),
        pps,
        red_bar: red_bar_or(state.session),
        pointer_x: NARRATE_PAGE.with(
            |NarratePage {
                 pointer_x: slot, ..
             }| slot.get(),
        ),
        selection: live_selection(),
        takes: state.takes.clone(),
        session: state.session,
        playing: state.playing,
        // §1's "reading the recording…" is about the ENVELOPE, so it is answered by whether the envelope was
        // had — not by whether a file has seconds. The two came apart on a shot whose recording was measured
        // at 45 s but whose cache could not be read: the lane drew nothing and said nothing either.
        envelope_decoded: peaks.is_ok(),
    });
    // `paint_plan` maps the red bar with the same helper this frame uses; keep them one number.
    plan.red_bar_px = band::second_to_px(red_bar_or(state.session), offset(), pps);
    let head = NARRATE_PAGE.with(
        |NarratePage {
             walk_head: slot, ..
         }| slot.get(),
    );
    plan.playhead_px = (head >= 0.0).then(|| band::second_to_px(head, offset(), pps));

    // 1. The envelope, or the sentence that says it is not ready: §1's "reading the recording…"
    // while the envelope decodes. Drawn as vertical bars in the lane under the ruler.
    let lane_top = RULER_PX;
    let lane_h = (height - RULER_PX).max(1.0);
    if let Some(note) = &plan.note {
        cr.set_source_rgb(0.75, 0.78, 0.82);
        cr.set_font_size(10.0);
        cr.move_to(4.0, lane_top + 14.0);
        let _ = cr.show_text(note);
    } else if let Ok(peaks) = &peaks {
        draw_envelope(cr, &peaks.1, width, lane_top, lane_h);
    }

    cr.set_source_rgb(0.55, 0.58, 0.62);
    cr.set_line_width(1.0);
    cr.move_to(0.0, RULER_PX);
    cr.line_to(width, RULER_PX);
    let _ = cr.stroke();
    cr.set_font_size(8.0);
    let step = crate::ui::cut_tracks::tick_step_seconds(pps);
    for (x, label) in crate::cut_trim::ruler_ticks(duration, pps, step) {
        let x = x - offset();
        if x < 0.0 || x > width {
            continue;
        }
        cr.move_to(x + 0.5, RULER_PX);
        cr.line_to(x + 0.5, RULER_PX - 4.0);
        let _ = cr.stroke();
        cr.move_to(x + 2.0, RULER_PX - 3.0);
        let _ = cr.show_text(&label);
    }

    // 3. The takes, green, each labelled with its duration alone when wide enough.
    for (left, take_w, label) in &plan.takes {
        if *take_w <= 0.0 {
            continue;
        }
        // Translucent: the envelope is already on the lane and §F4.6's picture shows the waveform THROUGH
        // the take, so an opaque fill would erase the part of the wave the person just picked.
        cr.set_source_rgba(0.3, 0.75, 0.42, 0.45);
        cr.rectangle(*left, lane_top, *take_w, lane_h);
        let _ = cr.fill_preserve();
        cr.set_source_rgb(0.16, 0.42, 0.24);
        let _ = cr.stroke();
        if !label.is_empty() {
            // Near-white: a take is a light patch on a dark lane, and ink dark enough to read on a pale
            // ground disappears on this one — `spec/img/07-narrate.png` shows the seconds in white.
            cr.set_source_rgb(0.94, 0.95, 0.97);
            cr.set_font_size(9.0);
            let _ = cr.text_extents(label);
            cr.move_to(left + 3.0, lane_top + 11.0);
            let _ = cr.show_text(label);
        }
    }

    // 4. The blue live selection, over the takes so the hand always sees what it chose.
    if let Some((left, w)) = plan.selection_px {
        if w > 0.0 {
            cr.set_source_rgba(0.25, 0.55, 0.95, 0.45);
            cr.rectangle(left, lane_top, w, lane_h);
            let _ = cr.fill();
        }
    }

    // 5. The faint red bar under the pointer.
    if let Some(x) = plan.pointer_px {
        cr.set_source_rgba(
            POINTER_COLOUR.0,
            POINTER_COLOUR.1,
            POINTER_COLOUR.2,
            POINTER_COLOUR.3,
        );
        cr.rectangle(x, lane_top, 1.0, lane_h);
        let _ = cr.fill();
    }

    // 6. The red ▶-start bar, THROUGH the ruler too (§1's words), so it reads as one line.
    let (bar_r, bar_g, bar_b) = RED_BAR_COLOUR;
    cr.set_source_rgb(
        bar_r as f64 / 255.0,
        bar_g as f64 / 255.0,
        bar_b as f64 / 255.0,
    );
    cr.rectangle(plan.red_bar_px - BAR_PX / 2.0, 0.0, BAR_PX, height);
    let _ = cr.fill();

    // 7. The playhead while ▶ walks the cut.
    if let Some(x) = plan.playhead_px {
        cr.set_source_rgb(0.95, 0.95, 0.98);
        cr.rectangle(x, lane_top, 1.0, lane_h);
        let _ = cr.fill();
    }
}

pub fn envelope_attempt(
    state: &crate::ui::narrate_page::NarrateState,
) -> Result<(f64, Vec<u8>), String> {
    let tree = crate::ui::window::narrate_session_tree().ok_or("no session folder open")?;
    if state.voice == screen::CAPTIONS {
        return Err("the voice is captions only".to_string());
    }
    let (tape, reference) = envelope_sources(&tree, &state.voice);
    envelope_of(&tree, &tape, &reference)
}

fn envelope_sources(
    tree: &crate::layout::Tree,
    voice: &str,
) -> (crate::narrate_tape::Tape, std::path::PathBuf) {
    let tape = crate::project::load(tree.dir())
        .map(|project| crate::narrate_band_face::tape(tree, &project, voice))
        .unwrap_or_default();
    (tape, tree.voice_ref_base_wav())
}

fn envelope_of(
    tree: &crate::layout::Tree,
    tape: &crate::narrate_tape::Tape,
    reference: &std::path::Path,
) -> Result<(f64, Vec<u8>), String> {
    if tape.is_empty() {
        if !reference.is_file() {
            return Err(format!("{}: no such file", reference.display()));
        }
        let lane = format!("voice:{}", crate::narrate_data::take_base(reference));
        return envelope(tree, &lane, reference).map(|peaks| (PEAK_HZ, peaks));
    }
    // One lane per SOURCE file, not one per voice slot: `wave_build` keys its cache by (lane, size, mtime),
    // so two different files sharing a lane name would each accept whichever cache was written first.
    let mut read: std::collections::HashMap<std::path::PathBuf, Vec<u8>> =
        std::collections::HashMap::new();
    let mut out: Vec<u8> = Vec::new();
    for stretch in &tape.0 {
        if !read.contains_key(&stretch.file) {
            let lane = format!("voice:{}", stretch.base);
            read.insert(stretch.file.clone(), envelope(tree, &lane, &stretch.file)?);
        }
        let peaks = &read[&stretch.file];
        let want = ((stretch.to - stretch.from) * PEAK_HZ).round() as usize;
        let from = ((stretch.from * PEAK_HZ).round() as usize).min(peaks.len());
        let until = (from + want).min(peaks.len());
        out.extend_from_slice(&peaks[from..until]);
        out.resize(out.len() + want - (until - from), 0);
    }
    Ok((PEAK_HZ, out))
}

#[derive(Clone, PartialEq)]
pub(crate) struct EnvelopeKey {
    dir: std::path::PathBuf,
    voice: String,
    tape: crate::narrate_tape::Tape,
    stamps: Vec<(std::path::PathBuf, Option<(i64, i64)>)>,
}

/// The peaks [`envelope_of`] answers.
pub(crate) type Peaks = Result<(f64, Vec<u8>), String>;

fn cached_envelope(state: &crate::ui::narrate_page::NarrateState) -> Peaks {
    let tree = crate::ui::window::narrate_session_tree().ok_or("no session folder open")?;
    if state.voice == screen::CAPTIONS {
        return Err("the voice is captions only".to_string());
    }
    let (tape, reference) = envelope_sources(&tree, &state.voice);
    let mut files: Vec<std::path::PathBuf> =
        tape.0.iter().map(|stretch| stretch.file.clone()).collect();
    files.dedup();
    files.push(reference.clone());
    let key = EnvelopeKey {
        dir: tree.dir().to_path_buf(),
        voice: state.voice.clone(),
        tape: tape.clone(),
        stamps: files
            .iter()
            .map(|file| (file.clone(), stamp(file).ok()))
            .collect(),
    };
    let held = NARRATE_PAGE.with(|NarratePage { envelope: slot, .. }| {
        slot.borrow()
            .as_ref()
            .filter(|(read_for, _)| *read_for == key)
            .map(|(_, peaks)| peaks.clone())
    });
    match held {
        Some(Some(peaks)) => return peaks,
        Some(None) => return Err(READING.to_string()),
        None => {}
    }
    NARRATE_PAGE
        .with(|NarratePage { envelope: slot, .. }| *slot.borrow_mut() = Some((key.clone(), None)));
    crate::ui::background::run(
        "take band envelope",
        move || envelope_of(&tree, &tape, &reference),
        move |peaks: Peaks| {
            if let Err(why) = &peaks {
                crate::ui::window::log_line(&format!("take band: {why}"));
            }
            let current = NARRATE_PAGE.with(|NarratePage { envelope: slot, .. }| {
                let mut slot = slot.borrow_mut();
                match slot.as_mut() {
                    Some((read_for, answer)) if *read_for == key => {
                        *answer = Some(peaks);
                        true
                    }
                    _ => false,
                }
            });
            if current {
                if let Some(lane) = lane_of(&crate::ui::window::main_window()) {
                    lane.queue_draw();
                }
            }
        },
    );
    Err(READING.to_string())
}

/// Why a lane has no peaks yet while its worker reads them.
const READING: &str = "still reading the recording";

fn draw_envelope(cr: &cairo::Context, peaks: &[u8], width: f64, lane_top: f64, lane_h: f64) {
    let (duration, view) = size();
    let pps = band::px_per_second(zoom(), duration, view);
    let bot = cut_wave_rows::draw_ground(cr, 0.0, width, lane_top, lane_h);
    let levels: Vec<(f64, f64)> = band::peak_columns(PEAK_HZ, peaks, offset(), pps, width)
        .into_iter()
        .map(|(x, peak)| (x, cut_wave_rows::meter(f64::from(peak) / 255.0)))
        .collect();
    cut_wave_rows::draw_columns(cr, &levels, bot, lane_h - 2.0);
}

fn follow_tape() {
    let Some(tree) = crate::ui::window::narrate_session_tree() else {
        return;
    };
    let Ok(project) = crate::project::load(tree.dir()) else {
        return;
    };
    let voice = crate::ui::narrate_page::read_state().voice;
    let tape = crate::narrate_band_face::tape(&tree, &project, &voice);
    // Keyed on the tape, not the voice's id: `own` and `narrator1` are the same narrator on the same tape.
    let now = Some((tree.dir().to_path_buf(), tape));
    // The first look at a project only notes the tape: the page was seeded off disk on it when it was loaded.
    let before = NARRATE_PAGE.with(
        |NarratePage {
             tape_seen: seen, ..
         }| seen.replace(now.clone()),
    );
    if before.as_ref().map(|seen| &seen.0) != Some(&tree.dir().to_path_buf()) || before == now {
        return;
    }
    let takes = crate::ui::narrate_seed::stored_takes(&tree, &voice);
    crate::ui::narrate_page::mutate(|state| state.takes = takes);
}
