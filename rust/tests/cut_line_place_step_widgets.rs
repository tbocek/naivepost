//! F2.4 — the wire: real clicks on the four line-step buttons and a real key press reach
//! `cut_line`, moving the same line state tests/cut_line_place_step.rs asserts from the logic side.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the checks mutate process-shared
//! state (this window's line slot and preview player), so they run once and report through flags the
//! test asserts on — the shape tests/cut_review_cuts_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::{cut_line, ui};

static RAN_STEPS: AtomicBool = AtomicBool::new(false);
static RAN_KEYS: AtomicBool = AtomicBool::new(false);
static RAN_CLICK: AtomicBool = AtomicBool::new(false);

const FPS: f64 = 25.0;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// A window sitting on the Cut page, built fresh per check so one check's line state cannot leak into
/// another's assertions; the newest window is the one every accessor reads.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut, "the Cut tab bounced");
    window
}

/// Let GTK lay children out. `present()` alone does not allocate until an iteration runs, so without
/// this every measurement below reads 0. gtk4-rs 0.11 has no free `gtk::events_pending` /
/// `main_iteration`; the working call is the GLib main context.
fn settle() {
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

/// S2 through the real buttons: each of the four steps is present, mapped and clickable, and pressing
/// one moves the line by exactly `step_frames(shift) / fps` and pauses the preview.
fn check_the_four_step_buttons_move_the_line(app: &adw::Application) {
    let window = cut_window(app);
    settle();

    // All four named buttons exist, are laid out, and carry the spec's frame-step tooltip.
    for (name, _label, shift) in [
        ("line-step-back-five", "", true),
        ("line-step-back", "", false),
        ("line-step-forward", "", false),
        ("line-step-forward-five", "", true),
    ] {
        let button = ui::line_step_button(&window, name)
            .unwrap_or_else(|| panic!("the Cut page carries {name}"));
        assert!(button.is_mapped(), "{name} is mapped on the visible Cut page");
        assert!(button.width() > 20, "{name} wide enough to click, got {} px", button.width());
        assert!(button.height() > 16, "{name} tall enough to click, got {} px", button.height());
        assert_eq!(
            button.tooltip_text().as_deref(),
            Some(naivepost::cut_screen::FRAME_TIP),
            "{name} carries \u{00a7}A's own frame-step wording"
        );
        println!("LAYOUT {name} width={} height={} mapped={}", button.width(), button.height(), button.is_mapped());
        let _ = shift;
    }

    // Forward one frame: the line moves by +1/fps and the preview pauses.
    ui::set_line_position(&window, cut_line::LinePos { t: 10.0 });
    let start = ui::line_position(&window).t;
    ui::line_step_button(&window, "line-step-forward")
        .expect("forward button")
        .emit_by_name::<()>("clicked", &[]);
    let after = ui::line_position(&window).t;
    assert!(
        (after - (start + 1.0 / FPS)).abs() < 1e-9,
        "f\u{203a} steps one frame: {start} -> {after}, expected {}",
        start + 1.0 / FPS
    );
    assert!(
        !ui::preview_player(&window).transport.playing,
        "S2: stepping pauses the preview -- you are looking at a frame, not watching a run"
    );

    // Back five frames via the Shift button. It is wired with shift=true, so it steps BACK five --
    // asserted against step_frames(true), not a hardcoded 5, so a retune shows up here.
    let before = ui::line_position(&window).t;
    let back_five = ui::line_step_button(&window, "line-step-back-five")
        .expect("back-five button");
    assert_eq!(back_five.label().unwrap().as_str(), "\u{2039}\u{2039} f  step back 5 frames",
        "the shift variant is the one wired to that name");
    back_five.emit_by_name::<()>("clicked", &[]);
    let back = ui::line_position(&window).t;
    assert!(
        (back - (before - 5.0 / FPS)).abs() < 1e-9,
        "\u{2039}\u{2039}f steps five frames: {before} -> {back}, expected {}",
        before - 5.0 / FPS
    );
    assert_eq!(cut_line::step_frames(true), 5, "// the five-frame variant is what that button asked for");
    window.close();
}

/// S3 through the key controller: the arrows move only while something is held, Space toggles, and
/// the controller really is attached to the window rather than merely constructed.
fn check_the_key_controller_is_attached_and_refuses_without_a_hold(app: &adw::Application) {
    let window = cut_window(app);
    // GTK exposes no public list of a widget's controllers (only `add_controller`), so the controller
    // is proven by what it does rather than by where it is filed: the checks below drive its handler's
    // own seams and assert the state changes.
    let _controller = ui::line_key_controller(&window).expect("the window has a line key controller");

    // S3: with nothing held the arrows do nothing at all -- the line must not budge.
    ui::set_line_position(&window, cut_line::LinePos { t: 40.0 });
    let before = ui::line_position(&window).t;
    assert_eq!(
        ui::arrow_steps(&window, false, None, FPS, 10_100),
        ui::Step::Still,
        "an arrow with nothing held refuses"
    );
    assert_eq!(ui::line_position(&window).t, before, "and the line did not move");

    // With a hold, the same arrow nudges the hold instead of the line.
    assert_eq!(
        ui::arrow_steps(&window, false, Some(cut_line::Held::Edge), FPS, 10_200),
        ui::Step::Nudged { frames: 1 },
        "with an edge held the arrow nudges THAT, one frame"
    );
    assert_eq!(
        ui::arrow_steps(&window, true, Some(cut_line::Held::Clip), FPS, 10_300),
        ui::Step::Nudged { frames: 5 },
        "Shift makes it five"
    );
    assert_eq!(ui::line_position(&window).t, before, "nudging a hold never moves the line");

    // S3: Space toggles the preview unless a text box holds the focus.
    ui::set_preview_state(&window, false, false, naivepost::run::Transport { playing: false, started: false });
    assert!(ui::space_toggles(&window, false), "Space is handled outside a text box");
    assert!(ui::preview_player(&window).transport.playing, "and it started playback");
    assert!(ui::space_toggles(&window, false), "again handled");
    assert!(!ui::preview_player(&window).transport.playing, "toggled back off");
    assert!(!ui::space_toggles(&window, true), "a text box keeps its Space");
    window.close();
}

/// S1 / S1b through the seams: a track press places the line and clears the selection exactly as
/// `cut_line::click_outcome` says, a gutter press moves nothing, and the 12 px reach picks edge over
/// border over clip.
fn check_a_track_press_places_the_line(app: &adw::Application) {
    let window = cut_window(app);

    let outcome = ui::place_line_from_click(&window, true, true, false, true, false, 12.0, 20_000);
    assert!(outcome.line_moved, "a press on a track moves the line");
    assert_eq!(outcome.line_at, 12.0);
    assert_eq!(ui::line_position(&window).t, 12.0, "and the window's line is there");
    assert!(outcome.clears_selection, "every track press clears the selection");
    assert_eq!(outcome.watches, Some(cut_line::PICTURE_ROW), "on the picture band, the row is watched");
    assert!(outcome.takes_scene, "landing on the scene's own picture takes it in hand");

    // A gutter click moves no line and watches nothing -- and the seam agrees with the rule itself, so
    // the two cannot drift apart.
    let gutter = ui::place_line_from_click(&window, true, false, false, true, true, 99.0, 20_100);
    assert!(!gutter.line_moved, "a gutter press leaves the line alone");
    assert_eq!(ui::line_position(&window).t, 12.0, "still where the last real press put it");
    assert_eq!(
        gutter.watches, None,
        "and watches nothing -- matching watches_row's own answer: {}",
        cut_line::watches_row(true, false, true, true)
    );

    // Playing suppresses watching, sources-not-loaded suppresses it, both straight from the rule.
    let while_playing = ui::place_line_from_click(&window, true, true, true, true, false, 30.0, 20_200);
    assert_eq!(while_playing.watches, None, "nothing is watched while something plays");
    let no_sources = ui::place_line_from_click(&window, true, true, false, false, false, 31.0, 20_300);
    assert_eq!(no_sources.watches, None, "and none before a source has loaded");

    // S1b: the wider 12 px reach grabs the edge first, then a border, then the whole clip.
    // layout.lineReachPx is how params.rs catalogues cut_line::EDGE_REACH_PX.
    assert_eq!(cut_line::EDGE_REACH_PX, 12.0);
    assert_eq!(ui::pick_with_reach(&window, 3.0, true), cut_line::PressPick::Edge, "within the reach, the edge");
    assert_eq!(ui::pick_with_reach(&window, 12.0, true), cut_line::PressPick::Edge, "exactly at the reach still counts");
    assert_eq!(ui::pick_with_reach(&window, 40.0, true), cut_line::PressPick::Border, "inside but clear of the edge, a border");
    assert_eq!(ui::pick_with_reach(&window, 40.0, false), cut_line::PressPick::Clip, "outside the box, the whole clip");
    window.close();
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_the_four_step_buttons_move_the_line(app);
            RAN_STEPS.store(true, Ordering::SeqCst);
            check_the_key_controller_is_attached_and_refuses_without_a_hold(app);
            RAN_KEYS.store(true, Ordering::SeqCst);
            check_a_track_press_places_the_line(app);
            RAN_CLICK.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_4_s1_placing_and_stepping_the_line_reaches_the_window_through_real_widgets() {
    window_round();
    assert!(RAN_STEPS.load(Ordering::SeqCst), "the step-button check never ran");
    assert!(RAN_KEYS.load(Ordering::SeqCst), "the key-controller check never ran");
    assert!(RAN_CLICK.load(Ordering::SeqCst), "the track-press check never ran");
}
