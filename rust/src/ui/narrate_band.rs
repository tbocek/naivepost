//! §07-narrate#1-screen, number **10** on the page: the take band's painted surface and its hand.
//!
//! The page (`crate::ui::narrate_page`) draws the widget and calls three things here: [`build_band`]
//! (the lane inside its own horizontal scroller), [`attach`] (the ONE drag gesture and the ONE wheel
//! controller), and [`repaint`] from `refresh`. Where a position means something — click-vs-drag,
//! the zoom step, what a take's label says — is [`crate::narrate_band`]'s rule; this module only reads the
//! event positions, writes slots, and puts ink where the plan says. Colours are §A.7's documented
//! four: dark ground (the CSS plate), blue envelope, green takes, red cue bar.

use adw::prelude::*;
use gtk4 as gtk;

use crate::narrate_band::{self as band, BandPaint};
use crate::narrate_screen as screen;
use crate::wave_build;

/// The lane's height, from the module that catalogues it (`narrate.bandLanePx`).
const LANE_PX: i32 = screen::BAND_LANE_PX as i32;
/// The lane's own minimum width, the number `build_band` requests before any view exists.
pub const LANE_MIN_PX: f64 = 240.0;

/// The width the lane must ASK for to show what the view holds: `Automatic` on the scroller only
/// produces a scrollbar once the child is wider than the viewport, so past fit the lane has to say how
/// wide its content is. Floored at `build_band`'s 240 px minimum, which is what fit itself leaves it.
pub fn lane_width(zoom: f64, duration: f64, viewport: f64) -> f64 {
    band::content_px(zoom, duration, viewport).max(LANE_MIN_PX)
}

/// The ruler strip painted across the top of the lane (`narrate.bandRulerPx`).
const RULER_PX: f64 = screen::BAND_RULER_PX;
/// Wave peaks arrive at this rate from [`wave_build::ensure`]'s cache.
const PEAK_HZ: f64 = wave_build::PEAK_HZ;
/// The red bar's width in px: two, so it reads over the ruler and the lane as ONE line.
const BAR_PX: f64 = 2.0;

/// The lane and the scroller it lives in, named as the page names it. Returned so the page can append
/// it where §A.1 puts it; the scroller exists because §1 promises the band its "own scrollbar once the
/// wheel zooms past fit" — `Automatic` means it appears exactly then and is absent at fit.
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

/// Install the painter and both controllers on the band. Called once, from the page's `wire`, after
/// `set_content` so the widget exists.
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
        note_motion(PRESS_X.with(|slot| slot.get()) + dx);
        repaint(&move_win);
    });
    let end_win = window.clone();
    drag.connect_drag_end(move |_g, dx, _y| {
        finish_drag(&end_win, PRESS_X.with(|slot| slot.get()) + dx);
    });
    lane.add_controller(drag.clone());
    DRAG.with(|slot| *slot.borrow_mut() = Some(drag));

    // ONE vertical wheel controller: plain notches zoom, Shift pans an eighth of the view, the way the wheel
    // turned. GTK hands a mouse notch over as dy = ±1, so one notch is one zoom step (the prototype's
    // `1.25^-dy`); the first rewrite divided by 50 as if dy were pixels, and a notch zoomed by 0.4 %, which
    // reads as a wheel that does nothing.
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_name(Some("wheel-take-band"));
    let scroll_win = window.clone();
    wheel.connect_scroll(move |ctrl, _dx, dy| {
        let shift = ctrl
            .current_event_state()
            .contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if shift {
            pan(&scroll_win, dy * band::pan_by(view_width(&scroll_win)));
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

    fit_lane_to_view(window);
    repaint(window);
}

/// Hand `lane_width`'s answer to the widget. Called wherever the view changes — attach, zoom, pan, and
/// after the page publishes the seconds the band is drawn across.
fn fit_lane_to_view(window: &adw::ApplicationWindow) {
    note_lane_size(window);
}

/// Give the lane the width its own view now implies. Split from [`paint`]'s copy of the same arithmetic
/// because a paint that changes its widget's request redraws forever: this one runs from the page's
/// refresh, where a relayout is the point.
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
    // The viewport is the width the lane is fitted TO, and it is the PAINTED one: `paint` records the width
    // GTK actually drew across, which is what every px/s mapping in this file already uses. Reading
    // `scroller.width()` instead would fit against a number no repaint agrees with (a scroller that has not
    // been allocated yet answers 0 px), and then §A.7's "own scrollbar once past fit" check — which compares
    // the lane's request with `lane_width(zoom, seconds, view_width)` — would disagree with the widget.
    let viewport = VIEW_WIDTH.with(|slot| slot.get());
    if viewport <= 0.0 {
        return;
    }
    // At fit the lane asks for its floor and fills what the column gives; zoomed in it asks for the whole
    // recording at the zoom, and the scroller shows one view of it with a scrollbar. The scroller itself never
    // asks for that width: the first rewrite set it as the scroller's minimum, so a zoom widened the window.
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

/// The pointer's bar as [`paint`] lays it down (§1's "faint red bar under the pointer"): the same red as
/// the cue bar at roughly a third of its strength, so a pixel check can name the ink instead of guessing
/// at a shade. Public for that reason alone — `paint` is its only other reader.
pub const POINTER_COLOUR: (f64, f64, f64, f64) = (0.85, 0.25, 0.25, 0.35);

/// The pointer's second x in lane px, or negative when it is off the band: §1's faint red bar follows the hand
/// across the ruler and the lane, and nothing may draw it outside the widget it belongs to.
fn note_pointer(x: f64) {
    let inside = (0.0..=LANE_WIDTH.with(|slot| slot.get())).contains(&x);
    POINTER_X.with(|slot| slot.set(if inside { x } else { -1.0 }));
}

/// The pointer's x as the painter currently sees it: negative means no hand over the band, which is what
/// a widget test reads back after firing the motion controller's `leave`.
pub fn pointer_px() -> f64 {
    POINTER_X.with(|slot| slot.get())
}

/// Show the band only for a voice that HAS a recording (§A.7: "visible only for a narrator slot with a
/// recording") — captions-only, or a reference that is not on disk, leaves nothing to waveforms. The
/// SCROLLER goes hidden rather than the lane, so the sensitivity rules that grey `take-band` still see it.
///
/// Asked of the PROJECT ([`crate::narrate_band_face::recording_exists`]), not of `voice_ref_base.wav`: §F4.6
/// S4 deletes that file on every take, pitch or voice change, so keying visibility to it hid the band on the
/// very press that made a take — the shot printed "take band seeded = true" over a page with no lane in it.
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

/// Does this window's voice have a recording to draw? `false` when no project is open at all (nothing is
/// tagged anywhere) and otherwise §A.7's question, answered by [`crate::narrate_band_face`].
/// Does this window's voice have a recording to draw? §A.7's question, asked of the PROJECT (a source
/// tagged with that narrator slot whose file is on disk) and, failing that, of the built reference: §F4.6 S4
/// removes `voice_ref_base.wav` on every take, pitch or voice change, so an answer built ONLY from the
/// project would hide the band for a voice that has a reference but no tagged row — which is what the page
/// does in a scratch session with no project open at all.
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

/// The band's gesture, kept for the test that fires it the way GTK does — the same reason
/// `cut_select_press::band_selection_gesture_for_test` exists on the Cut page.
pub fn band_gesture_for_test(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    DRAG.with(|slot| slot.borrow().clone())
}

/// Stand a span in the band's selection slot, the way a drag's release leaves one, for the test that checks
/// what ＋ WRITES rather than how the span was chosen (the pixel mapping has its own test).
pub fn set_selection_for_test(_window: &adw::ApplicationWindow, span: (f64, f64)) {
    SELECTION.with(|slot| *slot.borrow_mut() = Some(span));
}

thread_local! {
    /// The drag this window put on the band, so a test can reach the one the page installed.
    static DRAG: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
    /// Fit-relative zoom of the view. 1.0 = the whole recording across the lane.
    static ZOOM: std::cell::Cell<f64> = const { std::cell::Cell::new(1.0) };
    /// Px scrolled from the recording's start; never outside the file ([`band::clamp_view`]).
    static OFFSET: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// The red ▶-start bar, in seconds of the recording.
    static RED_BAR: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// Has anyone put the red bar down yet? Until then it rides the cut's playhead.
    static RED_BAR_SEEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The live selection in seconds: written by motion, emptied by a click.
    static SELECTION: std::cell::RefCell<Option<(f64, f64)>> =
        const { std::cell::RefCell::new(None) };
    /// The drag's press second, so release can ask `band::drag_or_click` with both ends.
    static PRESS_X: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// The pointer's x in lane px, or negative when it is off the band.
    static POINTER_X: std::cell::Cell<f64> = const { std::cell::Cell::new(-1.0) };
    /// The visible width of the band (the scroller's viewport), which px/s is measured against: the lane's own
    /// painted width at fit, and kept from then while zoomed in, when the lane is wider than what shows.
    static VIEW_WIDTH: std::cell::Cell<f64> = const { std::cell::Cell::new(240.0) };
    /// The lane's own painted width: the whole recording at the zoom, scrolled through by the scroller.
    static LANE_WIDTH: std::cell::Cell<f64> = const { std::cell::Cell::new(240.0) };
    /// The take walk's playhead, in seconds of the recording; negative while no walk plays. The red bar stays
    /// where it was put: it is where ▶ starts, not where the sound is.
    static WALK_HEAD: std::cell::Cell<f64> = const { std::cell::Cell::new(-1.0) };
    /// The recording's seconds, as last handed to the painter.
    static DURATION: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// The band's view as its painter last set it: `(seconds, px/s)`. A test reads these so its own
/// mapping is the painter's — including the case where no recording exists yet and the lane falls back
/// to a short duration (which clamps px/s at `narrate.bandMaxPps`).
pub fn painted_view_for_test() -> (f64, f64) {
    let (duration, width) = size();
    (
        duration,
        crate::narrate_band::px_per_second(zoom(), duration, width),
    )
}

/// The lane's last allocated width, as its painter saw it — the viewport `lane_width` is measured
/// against, and what a wheel test compares the lane's own request to.
pub fn view_width_for_test() -> f64 {
    VIEW_WIDTH.with(|slot| slot.get())
}

/// The seconds the painter drew across, or 0.1 when no recording exists yet (the same floor `paint`
/// hands the mapping), so a check can name the duration behind a width without reading ink.
pub fn painted_seconds_for_test() -> f64 {
    DURATION.with(|slot| slot.get().max(0.1))
}

/// The red bar's colour as [`paint`] lays it down, so a pixel check names the ink it expects rather than
/// accepting any bright dot near the right place.
pub const RED_BAR_COLOUR: (u8, u8, u8) = (219, 51, 46);

/// Where [`paint`] would put the red ▶-start bar in a lane `width` px wide and `height` px tall — the
/// whole view it computes (`duration`, `pps`, `red_bar_px`), so a test that paints its own image reads
/// the same numbers the ink used instead of rebuilding the mapping and hoping it agrees.
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

/// Ask the lane to paint again. The page calls this from `refresh` after anything the picture reads
/// changed — takes, voice, session, narration greys.
pub fn repaint(window: &adw::ApplicationWindow) {
    if let Some(lane) = lane_of(window) {
        lane.queue_draw();
    }
}

/// The view's fit-relative zoom.
pub fn zoom() -> f64 {
    ZOOM.with(|slot| slot.get())
}

/// The view's offset in px.
pub fn offset() -> f64 {
    OFFSET.with(|slot| slot.get())
}

/// The red ▶-start bar's second.
pub fn red_bar() -> f64 {
    RED_BAR.with(|slot| slot.get())
}

/// Where the red bar belongs when nothing has been clicked yet: the cut's playhead, so the band's ▶
/// starts where the picture is. [`set_red_bar`] overwrites it for good; this is the fallback the page
/// asks for, which keeps the page from poking a private slot.
pub fn red_bar_or(session: f64) -> f64 {
    match RED_BAR_SEEN.with(|slot| slot.get()) {
        true => red_bar(),
        false => session.max(0.0),
    }
}

/// Has anyone clicked the band since it was cleared? `false` means the bar still sits at the playhead,
/// which is what a page owes before the first click (§1) — and what a paint check reads that way.
pub fn red_bar_seen_for_test() -> bool {
    RED_BAR_SEEN.with(|slot| slot.get())
}

/// The live selection in seconds, or `None` when the band holds none. This is what the page's ＋ and −
/// buttons are asked for, so the drag and the buttons cannot disagree about what is selected.
pub fn live_selection() -> Option<(f64, f64)> {
    SELECTION.with(|slot| *slot.borrow())
}

/// Put the red bar at a second (the door a test and any future shortcut use).
pub fn set_red_bar(seconds: f64) {
    RED_BAR.with(|slot| slot.set(seconds.max(0.0)));
    RED_BAR_SEEN.with(|slot| slot.set(true));
}

/// Empty the band's hand: no selection, pointer off. Called when the voice changes, since the takes
/// of one recording mean nothing over another's.
pub fn clear_hand() {
    SELECTION.with(|slot| *slot.borrow_mut() = None);
    POINTER_X.with(|slot| slot.set(-1.0));
    ZOOM.with(|slot| slot.set(1.0));
    OFFSET.with(|slot| slot.set(0.0));
    RED_BAR.with(|slot| slot.set(0.0));
    RED_BAR_SEEN.with(|slot| slot.set(false));
}

/// One drag step's press: remember where the hand landed and hold that second as the bar's would-be
/// place, so a click with no motion still means "here".
fn note_press(x: f64) {
    PRESS_X.with(|slot| slot.set(x));
    SELECTION.with(|slot| *slot.borrow_mut() = None);
}

/// Motion during a drag: the live selection runs from the press to here, whichever way the pointer
/// went. The band is a selection tool first, so the selection appears as the hand moves.
fn note_motion(x: f64) {
    SELECTION.with(|slot| *slot.borrow_mut() = selected(x));
}

/// The ONE door that turns a press plus the pointer's current x into SECONDS: [`band::selection_is_a_drag`]
/// decides click-vs-drag (the same slop `take_press` uses), and the two pixel ends go through
/// [`band::x_to_second`] with the lane's own offset and px/s. Motion and release both read this, so the
/// live band and the take ＋ commits cannot disagree about which seconds were chosen.
fn selected(x: f64) -> Option<(f64, f64)> {
    let press = PRESS_X.with(|slot| slot.get());
    // [`band::drag_or_click`] owns the slop and direction rule (`screen::take_press`); only its CASE is
    // asked here, because the seconds come from this lane's own offset and px/s, which that rule does not
    // know about. A click (SetBar) selects nothing — it moves the red bar instead.
    match band::drag_or_click(press, x, 0.0) {
        screen::TakePress::SetBar => None,
        screen::TakePress::Select { .. } => {
            let (start, pps) = view();
            Some((
                band::x_to_second(press.min(x), start, pps),
                band::x_to_second(press.max(x), start, pps),
            ))
        }
    }
}

/// Release: a press that barely travelled is a click and MOVES THE RED BAR (§1: "click sets the red
/// bar"); a drag keeps its selection for ＋ and − to act on.
fn finish_drag(window: &adw::ApplicationWindow, x: f64) {
    match selected(x) {
        Some(span) => SELECTION.with(|slot| *slot.borrow_mut() = Some(span)),
        None => {
            let (start, pps) = view();
            let at = band::x_to_second(x, start, pps);
            set_red_bar(at);
            SELECTION.with(|slot| *slot.borrow_mut() = None);
        }
    }
    repaint(window);
}

/// Zoom by `steps` notches (positive = bigger), clamped so the view cannot leave the file.
fn zoom_by(window: &adw::ApplicationWindow, steps: f64) {
    if zoom() <= 1.0 {
        // Leaving fit: what shows now is the width every later zoom is measured against.
        VIEW_WIDTH.with(|slot| slot.set(LANE_WIDTH.with(|lane| lane.get())));
    }
    let (duration, view) = size();
    let before = band::px_per_second(zoom(), duration, view);
    let adjustment = scroller_of(window).map(|scroller| scroller.hadjustment());
    let scrolled = adjustment.as_ref().map_or(0.0, |a| a.value());
    // The second under the pointer stays under it (the view's middle when the pointer is off the band).
    let pointer = POINTER_X.with(|slot| slot.get());
    let anchor = if pointer >= 0.0 {
        pointer
    } else {
        scrolled + view / 2.0
    };
    let (second, on_screen) = (anchor / before.max(f64::EPSILON), anchor - scrolled);
    let zoom = band::zoom_step(zoom(), -steps);
    ZOOM.with(|slot| slot.set(zoom));
    fit_lane_to_view(window);
    let after = band::px_per_second(zoom, duration, view);
    if let Some(adjustment) = adjustment {
        // After the relayout that gives the lane its new width, or the adjustment's range is still the old one.
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
    WALK_HEAD.with(|slot| slot.set(at.unwrap_or(-1.0)));
}

/// The take walk's playhead, for a test: negative while no walk plays.
pub fn walk_head() -> f64 {
    WALK_HEAD.with(|slot| slot.get())
}

/// `(duration, width)` of the view as last painted.
fn size() -> (f64, f64) {
    (
        DURATION.with(|slot| slot.get()),
        VIEW_WIDTH.with(|slot| slot.get()),
    )
}

/// `(offset, pps)` the mapping runs on right now.
fn view() -> (f64, f64) {
    let (duration, width) = size();
    (offset(), band::px_per_second(zoom(), duration, width))
}

/// The lane's width as the last paint saw it, for a wheel that pans before a repaint.
fn view_width(window: &adw::ApplicationWindow) -> f64 {
    let _ = window;
    VIEW_WIDTH.with(|slot| slot.get())
}

/// The band's own painted picture on an image surface, at the size a test asks for. A draw func gets
/// cairo's target from GTK and cannot be handed one, so this runs THE SAME `paint` over a surface a
/// test can read pixels from — which is how a check proves the lane was inked rather than left a bare
/// CSS plate (`cut_preview_frame::raster_surface_for_test` is the Cut page's version of the idea).
/// The band's own painted picture on an image surface, at the size a test asks for. A drawing area's
/// cairo's target from GTK and cannot be handed one, so this runs THE SAME `paint` over a surface the
/// test can read pixels from — which is how a check proves the lane was inked rather than merely named
/// CSS plate (`cut_preview_frame::raster_surface_for_test` is the Cut page's version of the same door).
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

/// Paint the lane into an RGB buffer of the caller's choosing and return `(bytes, width, height)` plus
/// the `(seconds, px/s)` that frame used. This is `painted_surface_for_test`'s work with the pixel copy
/// kept out of the test: a pointer-bar check needs both the ink and the mapping that placed it, and one
/// call gives both instead of letting the test rebuild the second half and hope it agrees.
/// Paint the lane into an RGB buffer of the caller's choosing and return `(bytes, width, height)` plus
/// the `(seconds, px/s)` that frame used. This is `painted_surface_for_test`'s work with the pixel copy
/// kept out of the test: a pointer-bar check needs both the ink and the mapping that placed it, and one
/// call gives both instead of letting the test rebuild the second half and hope it agrees.
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

/// The seconds of the reference recording the band is drawn across: the file the voice is cloned
/// from, measured by its own header. `None` when the voice is captions-only or has no file yet — the
/// band then says so rather than drawing an empty ruler.
pub fn recording_seconds() -> Option<f64> {
    let tree = crate::ui::window::narrate_session_tree()?;
    let reference = crate::narrate_data::base_reference(&tree);
    let (channels, rate) = crate::voice_ref::wav_header(&reference)?;
    let bytes = std::fs::metadata(&reference).ok()?.len();
    // 16-bit mono PCM: the header's channels and rate say how many samples the file holds.
    let bytes_per_second = (channels.max(1) as u64) * (rate.max(1) as u64) * 2;
    Some(((bytes.saturating_sub(44)) as f64 / bytes_per_second as f64).max(0.1))
}

/// The peaks to draw, straight off the wave cache: measured by the same code Prepare's envelope step
/// uses, so the band and the trim strips cannot disagree about where the loud parts are. Returns the
/// cache's own peaks and rate; `Err` says why there are none.
///
/// The 16 kHz decode is asked for first, through the door Prepare itself uses ([`crate::decode_audio::ensure`]):
/// `wave_build::ensure` folds a decode that already exists but never makes one, so on a project whose voice
/// lane was never transcribed the band had nothing to draw and printed "reading the recording…" forever.
/// ffmpeg decides where ffprobe/ffmpeg live, exactly as everywhere else in the app (`""` = this machine has
/// no settings home, which the decode reports as its own refusal).
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
    // §00's "failure is specific and local": these are the lines the decode and the wave build printed on
    // their way to an answer. They used to be dropped here, which left a band that said only "reading the
    // recording…" forever with nothing anywhere saying why — so they now ride out with the refusal.
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

/// Paint one frame. Ink only: every position comes from [`band::paint_plan`]. The window is what a
/// future per-window slot would read; today every slot here is per-thread like the page's other ones.
pub fn paint(cr: &cairo::Context, width: f64, height: f64) {
    // `width` is the LANE: at fit it is also what shows, zoomed in it is the whole recording at the zoom, and
    // px/s is measured against what shows (`VIEW_WIDTH`), so the lane is exactly the recording's width.
    LANE_WIDTH.with(|slot| slot.set(width));
    if zoom() <= 1.0 {
        VIEW_WIDTH.with(|slot| slot.set(width));
    }
    let state = crate::ui::narrate_page::read_state();
    let seconds = painted_seconds();
    DURATION.with(|slot| slot.set(seconds.unwrap_or(0.0).max(0.1)));
    let (duration, view) = size();
    let pps = band::px_per_second(zoom(), duration, view);
    // Keep the lane as wide as the file it draws. GTK gives no size-allocate hook to hang this on and the
    // first fit runs while the scroller is still hidden (a hidden widget answers 0 px wide), so this is where
    // the real numbers first arrive: without it the lane kept its 240 px request and a 45 s recording fit
    // across a 560 px column stopped two fifths of the way across the plate.
    // The lane's width comes from `note_lane_size`, which the page's refresh calls: a draw func that changes
    // its own widget's size request queues another draw, and the next one changes it again.

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
        pointer_x: POINTER_X.with(|slot| slot.get()),
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
    // The take walk's own playhead, where the sound is (§1 "playhead while ▶ walks").
    let head = WALK_HEAD.with(|slot| slot.get());
    if head >= 0.0 {
        plan.playhead_px = Some(band::second_to_px(head, offset(), pps));
    }

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

    // 2. The ruler, on the Cut page's tick ladder (the prototype's `tickStep`): the first step at least 70 px
    // apart at this zoom, every tick labelled. The first rewrite ticked every 30 s whatever the zoom, so a
    // recording under half a minute had a bare ruler, and zooming in never added a mark.
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

/// The envelope for the voice in hand: `(peaks, their rate)`. Captions-only answers `None`, and so does a
/// voice with nothing to measure — which is what makes the band print the "reading…" sentence instead.
///
/// Asked of the RECORDING first (§A.7: the lane is "the recording this voice is cloned from"): the built
/// reference is §F4.6 S3's output and S4 deletes it on every take, pitch or voice change, so measuring only
/// that file left the lane blank right after the ＋ press that made a take. With no recording to read (a
/// voices-folder id whose file went away), the reference still answers.
/// The door a test asks the envelope question through: the same walk [`paint`] runs, with the refusal kept
/// instead of folded away. `*_for_test` because the program never calls it — `paint` walks `envelope_attempt`
/// itself and logs the reason where the user can read it.
pub fn envelope_report_for_test(
    state: &crate::ui::narrate_page::NarrateState,
) -> Result<(f64, Vec<u8>), String> {
    envelope_attempt(state)
}

/// The envelope walk itself, as `Ok(peaks)` or the `String` saying what stopped it: the reason has to travel
/// out of here for §00's "failure is specific and local", because every caller used to fold it to `None` and
/// a lane that drew nothing gave the person no way to find out why.
fn envelope_attempt(
    state: &crate::ui::narrate_page::NarrateState,
) -> Result<(f64, Vec<u8>), String> {
    let tree = crate::ui::window::narrate_session_tree().ok_or("no session folder open")?;
    if state.voice == screen::CAPTIONS {
        return Err("the voice is captions only".to_string());
    }
    let (tape, reference) = envelope_sources(&tree, &state.voice);
    envelope_of(&tree, &tape, &reference)
}

/// What the lane is drawn from: the voice's tape ([`crate::narrate_band_face::tape`]), and the built reference
/// for a voice with no recording at all.
fn envelope_sources(
    tree: &crate::layout::Tree,
    voice: &str,
) -> (crate::narrate_tape::Tape, std::path::PathBuf) {
    let tape = crate::project::load(tree.dir())
        .map(|project| crate::narrate_band_face::tape(tree, &project, voice))
        .unwrap_or_default();
    (tape, crate::narrate_data::base_reference(tree))
}

/// The tape's peaks, end to end: each recording's envelope read once (decoded and measured when it has to be, so
/// this runs on a worker, [`cached_envelope`]), and each stretch's seconds of it laid where the stretch sits on
/// the band. A stretch the envelope runs short of is padded with silence, so every later stretch stays at its own
/// second. With no tape, the built reference whole.
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

/// What a cached envelope was read for: the session, the voice, the tape's stretches, and each file's size and
/// mtime, so a new take, a new voice, an edited cut or a re-recorded file is read again and nothing else is.
#[derive(Clone, PartialEq)]
struct EnvelopeKey {
    dir: std::path::PathBuf,
    voice: String,
    tape: crate::narrate_tape::Tape,
    stamps: Vec<(std::path::PathBuf, Option<(i64, i64)>)>,
}

/// The peaks [`envelope_of`] answers.
type Peaks = Result<(f64, Vec<u8>), String>;

thread_local! {
    /// The band's envelope as last read, keyed on what it was read from; `None` beside a key while a worker
    /// is reading it.
    static ENVELOPE: std::cell::RefCell<Option<(EnvelopeKey, Option<Peaks>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The envelope for this paint, off the cache. The first paint for a session, voice and set of files asks a
/// worker to decode and measure (the first rewrite did it inside the draw func: seconds to minutes of a frozen
/// window on a long recording, and again on every paint while it kept failing). Until the answer lands this
/// says the lane is still being read; a failure is logged once, when it lands, with its reason (§00's "failure
/// is specific and local").
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
    let held = ENVELOPE.with(|slot| {
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
    ENVELOPE.with(|slot| *slot.borrow_mut() = Some((key.clone(), None)));
    crate::ui::background::run(
        "take band envelope",
        move || envelope_of(&tree, &tape, &reference),
        move |peaks: Peaks| {
            if let Err(why) = &peaks {
                crate::ui::window::log_line(&format!("take band: {why}"));
            }
            let current = ENVELOPE.with(|slot| {
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

/// Vertical peak bars across the visible window, blue over the ground.
fn draw_envelope(cr: &cairo::Context, peaks: &[u8], width: f64, lane_top: f64, lane_h: f64) {
    let (duration, view) = size();
    let pps = band::px_per_second(zoom(), duration, view);
    let visible = band::visible_peaks(PEAK_HZ, peaks, offset(), pps, width);
    cr.set_source_rgb(0.35, 0.6, 0.9);
    let step = 1.0_f64.max(2.0_f64.min(pps / PEAK_HZ.max(1.0)));
    for (at, peak) in &visible {
        let x = band::second_to_px(*at, offset(), pps);
        if x > width {
            break;
        }
        let h = (lane_h - 4.0) * (*peak as f64 / 255.0);
        cr.rectangle(x, lane_top + (lane_h - h) / 2.0, step, h.max(1.0));
    }
    let _ = cr.fill();
}

thread_local! {
    /// The project and tape the page's takes were last read for.
    static TAPE_SEEN: std::cell::RefCell<Option<(std::path::PathBuf, crate::narrate_tape::Tape)>> =
        const { std::cell::RefCell::new(None) };
}

/// Keep the page's takes on the tape the band shows: when the tape changes (another voice, the session's
/// recordings measured, the cut edited on the Cut page) the takes are read again off `narrate/takes.json`, where
/// every pick already is, in the new tape's seconds. A take list carried over from another tape would put its
/// green on seconds that are now someone else's.
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
    let before = TAPE_SEEN.with(|seen| seen.replace(now.clone()));
    if before.as_ref().map(|seen| &seen.0) != Some(&tree.dir().to_path_buf()) || before == now {
        return;
    }
    let takes = crate::ui::narrate_seed::stored_takes(&tree, &voice);
    crate::ui::narrate_page::mutate(|state| state.takes = takes);
}
