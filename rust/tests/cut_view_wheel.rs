//! §05-cut#1-screen — the wheel and the zoom, through real widgets.
//!
//! `tests/cut_screen_view.rs` proves what a gesture MEANS (`cut_screen::wheel` answers step / zoom / pan)
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
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text, widget_in};

static RAN_CONTROLLERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_STEP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_ZOOM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_BUTTONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    let view = ui::cut_view::view_px(ui::cut_view::surface_width(&window));
    let total = ui::cut_view::total_px(1200.0, ui::cut_view::pps());
    assert!(!ui::cut_view::fits(total, view), "20 min at the open zoom needs the bar");

    let pps_before = ui::cut_view::pps();
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
    let view = ui::cut_view::view_px(ui::cut_view::surface_width(&window));
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

fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_three_gestures_are_attached(app);
            RAN_CONTROLLERS.store(true, std::sync::atomic::Ordering::SeqCst);
            check_a_notch_over_the_transport_steps_the_line(app);
            RAN_STEP.store(true, std::sync::atomic::Ordering::SeqCst);
            check_a_notch_over_the_tracks_zooms_and_shift_pans(app);
            RAN_ZOOM.store(true, std::sync::atomic::Ordering::SeqCst);
            check_the_zoom_buttons_rescale_and_stop_at_the_fit(app);
            RAN_BUTTONS.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
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
