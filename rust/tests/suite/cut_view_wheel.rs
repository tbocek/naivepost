//! §05-cut#1-screen — the wheel and the zoom, through real widgets.
//!
//! `tests/suite/cut_screen_view.rs` proves what a gesture MEANS (`cut_screen::wheel` answers step / zoom / pan)
//! and where the ladder may run; this file proves the page carries controllers over the three kinds of
//! ground §1 names, that a notch over them moves the same state the logic test checks, and that the − / +
//! buttons re-scale the band instead of only logging a number.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_screen::{self, WheelOver};
use naivepost::shell::Page;
use naivepost::timeline::{self, Recording};
use naivepost::ui;
use crate::common;
use common::{fixture_dir, settle, status_text, widget_in};

static RAN_CONTROLLERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_STEP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_ZOOM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BUTTONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BAND_CONTROLLER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BAND_EMIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A tape long enough that the view has somewhere to go: 20 minutes at any zoom is more timeline than a
/// page, so the scrollbar shows and a scroll can be seen to move.
fn tape() -> Vec<Recording> {
    vec![Recording {
        base: "lecture".to_string(),
        start: 0.0,
        end: 1200.0,
    }]
}

fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!("cut-wheel-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    let seeded = cut::Cut {
        segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }],
        ..Default::default()
    };
    ui::seed_review_cut(&window, &seeded);
    ui::set_session_recordings(&tape());
    settle();
    window
}

/// §1: the wheel works over the transport bar, over the preview picture, and over the tracks. Each of
/// those three must actually carry a scroll controller by the time the page is up — a rule with no
/// controller under it is a rule nobody can trigger.
fn check_the_three_gestures_are_attached(app: &adw::Application) {
    let window = cut_window(app);
    for name in ["group-transport", "preview-panel", "track-strip"] {
        let host = widget_in(&window, name).unwrap_or_else(|| panic!("the page carries {name}"));
        let controllers = host.observe_controllers();
        let mut found = 0usize;
        for index in 0..controllers.n_items() {
            if let Some(controller) = controllers.item(index) {
                if controller.is::<gtk::EventControllerScroll>() {
                    found += 1;
                }
            }
        }
        assert!(found > 0, "{name} carries no scroll controller");
    }
    window.close();
}

/// A notch over the transport steps frames: the SAME seam the ‹‹f/f›› buttons use, so the line moved is
/// the line the readout shows — not a second copy of the arithmetic.
fn check_a_notch_over_the_transport_steps_the_line(app: &adw::Application) {
    let window = cut_window(app);
    let before = ui::line_position(&window).t;
    let said = ui::cut_view::apply_wheel(&window, WheelOver::Transport, false, 1.0, 0.0, |frames| {
        let _ = ui::step_frames_of(&window, frames);
    });
    settle();
    assert!(said.is_none(), "a step says nothing here — the transport owns that sentence");
    let after = ui::line_position(&window).t;
    assert!(
        after > before,
        "a notch forward moved the red line: {before} -> {after}"
    );
    // Shift makes it five, exactly as the button's Shift does.
    let shifted = ui::cut_view::apply_wheel(&window, WheelOver::Transport, true, 1.0, 0.0, |frames| {
        let _ = ui::step_frames_of(&window, frames);
    });
    settle();
    assert!(shifted.is_none());
    assert!(
        ui::line_position(&window).t - after > (before * 0.0 + 0.1),
        "Shift stepped further than one frame"
    );
    window.close();
}

/// Over the tracks a notch zooms around the cursor and Shift pans an eighth — both visible in the page's
/// own view state, and both said out loud on the status line.
fn check_a_notch_over_the_tracks_zooms_and_shift_pans(app: &adw::Application) {
    let window = cut_window(app);
    let runs = timeline::filmed_runs(&tape());
    // The view the page itself measures — `wheel_gestures_view`, the width §1's band spans — because that is
    // what `apply_wheel` hands `cut_screen::wheel` for BOTH halves of this rule: the zoom's floor AND Shift's
    // eighth. A 20-minute tape across it floors well below the open zoom, so a notch here has room to travel.
    let view = ui::cut_view::wheel_gestures_view(&window);
    let wheel_floor = timeline::floor_pps(&runs, view);

    let pps_before = ui::cut_view::pps();
    // A notch DOWN is one step of the ladder out, and the floor has to be far enough below for it to land:
    // `zoom_around` clamps, so a page already at its floor answers "stay put" rather than one step.
    if pps_before <= wheel_floor * cut_screen::ZOOM_STEP {
        for _ in 0..8 {
            let _ = ui::cut_view::apply_wheel(&window, WheelOver::Tracks, false, -1.0, 0.0, |_| {});
        }
        settle();
    }
    let pps_before = ui::cut_view::pps();
    assert!(
        pps_before > wheel_floor * cut_screen::ZOOM_STEP,
        "setup: the notch needs one step of room below it: floor {wheel_floor}, zoom {pps_before}"
    );

    let total = ui::cut_view::total_px(1200.0, ui::cut_view::pps());
    assert!(!ui::cut_view::fits(total, view), "20 min at the open zoom needs the bar");

    let said = ui::cut_view::apply_wheel(&window, WheelOver::Tracks, false, -1.0, 0.0, |_| {})
        .expect("a zoom has a sentence");
    settle();
    assert!(said.contains("px a second"), "{said}");
    // The wheel's controller hands its sentence to the caller and `wire_wheel_gestures` keeps it quiet —
    // deliberately: §1 says the tracks carry NO tooltips and that the status line is for what a press
    // would do, so a notch re-scales the page without writing over the row.
    assert!(
        ui::cut_view::pps() > pps_before,
        "a notch over the tracks zoomed in: {pps_before} -> {}",
        ui::cut_view::pps()
    );

    // Shift over the same spot pans instead: the zoom holds still and the scroll moves.
    let pps_now = ui::cut_view::pps();
    let origin_before = ui::cut_view::origin();
    let panned = ui::cut_view::apply_wheel(&window, WheelOver::Tracks, true, -1.0, 0.0, |_| {})
        .expect("a pan has a sentence too");
    settle();
    assert!(panned.contains("scrolled"), "{panned}");
    assert_eq!(ui::cut_view::pps(), pps_now, "a pan changed no zoom");
    assert_ne!(ui::cut_view::origin(), origin_before, "and the view slid");
    assert!(
        ui::cut_view::origin() <= total - view,
        "the scroll stayed inside the timeline: {} of {}",
        ui::cut_view::origin(),
        total - view
    );
    let _ = runs;
    window.close();
}

/// Badge **22**: − / + re-scale the page rather than only printing a number, and stop where the session
/// fits — which for a twenty-minute tape is far below the constant the placeholder strip was sized with.
fn check_the_zoom_buttons_rescale_and_stop_at_the_fit(app: &adw::Application) {
    let window = cut_window(app);
    // The surface the ladder actually measures its floor over: the strip's DECLARED width, which is what
    // `press_zoom` passes to `zoom_step`. Reading the window's realized allocation here instead would check
    // the rule against a surface no door in the program uses once §05-cut#1's paned stretches the strip.
    let view = ui::cut_view::view_px(f64::from(ui::window::STRIP_WIDTH));
    let floor = timeline::floor_pps(&timeline::filmed_runs(&tape()), view);
    let button = |name: &str| {
        widget_in(&window, name)
            .expect("the zoom group carries its two buttons")
            .downcast::<gtk::Button>()
            .expect("they are buttons")
    };

    let start = ui::cut_view::pps();
    button("zoom-in-button").emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(
        ui::cut_view::pps() > start,
        "+ raised the page's zoom: {start} -> {}",
        ui::cut_view::pps()
    );
    assert!(
        status_text(&window).contains("px a second"),
        "and the row says the new number: {}",
        status_text(&window)
    );

    // Out, all the way out: the ladder stops at 'the whole session is on screen', not at 4 px/s.
    for _ in 0..80 {
        button("zoom-out-button").emit_by_name::<()>("clicked", &[]);
    }
    settle();
    assert_eq!(
        ui::cut_view::pps(),
        floor,
        "zooming out stops where the session fits ({floor}), wherever the strip's constant sits"
    );
    assert_ne!(
        floor,
        cut_screen::ZOOM_AT_OPEN,
        "a 20-minute tape across this view floors well below the open zoom — a constant would stop early"
    );
    window.close();
}

/// Badge **24-31** on the widget that actually draws them: the page's tracks live on `track-band` (built by
/// `cut_tracks::build`, inserted by `cut_tracks::install`), not on F2.8's placeholder `track-strip`. A
/// controller over the placeholder alone would leave a notch on the ruler, the kept bars and the picture rows
/// doing nothing, so the wire is pinned here by reading the band's own controller list off the widget.
fn check_the_drawn_band_carries_the_tracks_controller(app: &adw::Application) {
    let window = cut_window(app);
    let band = widget_in(&window, "track-band").expect("the page draws its tracks on track-band");
    let held = band.observe_controllers();
    let names: Vec<String> = (0..held.n_items())
        .filter_map(|index| held.item(index))
        .filter(|controller| controller.is::<gtk::EventControllerScroll>())
        .map(|controller| {
            controller
                .downcast_ref::<gtk::EventControllerScroll>()
                .and_then(|c| c.name().map(|name| name.to_string()))
                .unwrap_or_default()
        })
        .collect();
    assert!(
        names.iter().any(|name| name == "wheel-track-band"),
        "the drawn band carries no tracks wheel controller: {names:?}"
    );
    // Exactly one: a second one would answer one notch as two steps.
    assert_eq!(
        names.iter().filter(|name| *name == "wheel-track-band").count(),
        1,
        "the band answers a notch more than once: {names:?}"
    );
    window.close();
}

/// The same gesture fired the toolkit's way, at the controller the band wears: a notch zooms around the
/// cursor, a Shift notch pans an eighth of the view, and neither may stop at the placeholder strip's floor.
fn check_a_notch_emitted_on_the_band_zooms_and_pans(app: &adw::Application) {
    let window = cut_window(app);
    let tape_runs = timeline::filmed_runs(&tape());
    let view = ui::cut_view::wheel_gestures_view(&window);
    // The floor the notch will be clamped at, read through the same door the page reads it.
    let wheel_floor = timeline::floor_pps(&tape_runs, view);
    let band = widget_in(&window, "track-band").expect("the page draws its tracks on track-band");
    let held = band.observe_controllers();
    let controller = (0..held.n_items())
        .filter_map(|index| held.item(index))
        .filter_map(|item| item.downcast::<gtk::EventControllerScroll>().ok())
        .find(|c| c.name().is_some_and(|name| name == "wheel-track-band"))
        .expect("the band's tracks controller");

    // Zoom: a notch up over the band, emitted as the scroll signal GTK would deliver it. Out of the floor's
    // way first — `zoom_around` clamps, so a page already sitting at its floor answers "stay where the whole
    // session fits" to a DOWN notch instead of moving one step.
    let pps_before = ui::cut_view::pps();
    if pps_before <= wheel_floor * cut_screen::ZOOM_STEP {
        for _ in 0..8 {
            let _ = ui::cut_view::apply_wheel(&window, WheelOver::Tracks, false, -1.0, 0.0, |_| {});
        }
        settle();
    }
    let pps_before = ui::cut_view::pps();
    // The signal's return is a `gboolean` at the GObject level (`glib::Propagation` is rust-only), so the
    // emit asks for that and reads back whether the page stopped the event — which is what "the wire
    // answered" means.
    let handled = controller.emit_by_name::<bool>("scroll", &[&0.0f64, &-1.0f64]);
    settle();
    assert!(handled, "the band's wheel controller did not claim the emitted notch");
    let pps_after = ui::cut_view::pps();
    assert!(
        pps_after > pps_before,
        "a notch emitted on the drawn band zoomed: {pps_before} -> {pps_after}"
    );
    assert!(
        (pps_after - pps_before * cut_screen::ZOOM_STEP).abs() < pps_after * 0.001,
        "the notch moved exactly one wheel step: {pps_before} -> {pps_after}"
    );

    // Pan: the shift flag comes off the event that woke the signal, and a bare emit carries no modifier
    // state headless, so the Shift branch is driven through the SAME door the controller's closure uses
    // (`apply_wheel` with shift = true) rather than pretending an emit had a keyboard state. The zoom emit
    // above is what proves the wire; this proves what the wire does with a shifted notch.
    let origin_before = ui::cut_view::origin();
    // The eighth is measured HERE, at the moment of the pan, not from the `view` read at the top of this check:
    // the band's width is only known once GTK has laid the page out, and the zoom notch plus its settle above is
    // what gets it laid out — an eighth taken before that is a different screenful than the one the page panned.
    let view = ui::cut_view::wheel_gestures_view(&window);
    let said = ui::cut_view::apply_wheel(&window, WheelOver::Tracks, true, -1.0, 0.0, |_| {})
        .expect("a pan has a sentence");
    settle();
    assert!(said.contains("scrolled"), "{said}");
    let moved = (ui::cut_view::origin() - origin_before).abs();
    let eighth = cut_screen::pan_px(view);
    assert!(
        (moved - eighth).abs() < eighth.max(1.0) * 0.5 + 1.0,
        "a Shift notch pans about an eighth of the view ({eighth:.0} px): {origin_before} -> {}",
        ui::cut_view::origin()
    );

    // And the ladder runs down to where the session fits, which is §A's wording for zooming out — the
    // placeholder strip's constant would stop far short of it on a twenty-minute tape.
    let floor = timeline::floor_pps(&tape_runs, view);
    for _ in 0..200 {
        ui::cut_view::apply_wheel(&window, WheelOver::Tracks, false, 1.0, 0.0, |_| {});
    }
    settle();
    assert_eq!(
        ui::cut_view::pps(),
        floor,
        "notches out stop where the whole session is on screen ({floor})"
    );
    window.close();
}

#[test]
fn sec_05_cut_1_screen_s4_the_drawn_band_carries_the_tracks_wheel_controller() {
    round();
    assert!(
        RAN_BAND_CONTROLLER.load(std::sync::atomic::Ordering::SeqCst),
        "the band-controller check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_s5_a_wheel_notch_on_the_band_zooms_around_the_cursor_and_a_shift_notch_pans_an_eighth() {
    round();
    assert!(
        RAN_BAND_EMIT.load(std::sync::atomic::Ordering::SeqCst),
        "the band-emit check never ran"
    );
}

fn round() {
    common::one_app_round(|app| {
        check_the_three_gestures_are_attached(app);
        RAN_CONTROLLERS.store(true, std::sync::atomic::Ordering::SeqCst);
        check_a_notch_over_the_transport_steps_the_line(app);
        RAN_STEP.store(true, std::sync::atomic::Ordering::SeqCst);
        check_a_notch_over_the_tracks_zooms_and_shift_pans(app);
        RAN_ZOOM.store(true, std::sync::atomic::Ordering::SeqCst);
        check_the_zoom_buttons_rescale_and_stop_at_the_fit(app);
        RAN_BUTTONS.store(true, std::sync::atomic::Ordering::SeqCst);
        check_the_drawn_band_carries_the_tracks_controller(app);
        RAN_BAND_CONTROLLER.store(true, std::sync::atomic::Ordering::SeqCst);
        check_a_notch_emitted_on_the_band_zooms_and_pans(app);
        RAN_BAND_EMIT.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

#[test]
fn sec_05_cut_1_screen_s4_a_wheel_over_the_transport_and_the_picture_carries_a_step_controller() {
    round();
    assert!(
        RAN_CONTROLLERS.load(std::sync::atomic::Ordering::SeqCst),
        "the controller check never ran"
    );
    assert!(RAN_STEP.load(std::sync::atomic::Ordering::SeqCst), "the step check never ran");
}

#[test]
fn sec_05_cut_1_screen_s5_a_wheel_over_the_tracks_zooms_around_the_cursor_and_shift_pans_an_eighth() {
    round();
    assert!(RAN_ZOOM.load(std::sync::atomic::Ordering::SeqCst), "the zoom/pan check never ran");
    assert!(
        RAN_BUTTONS.load(std::sync::atomic::Ordering::SeqCst),
        "the zoom-button check never ran"
    );
}
