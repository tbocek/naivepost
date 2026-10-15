//! F4.5 S4/S2 over the real transport widgets: back, forward, the slider and the paused wheel are SEEKS —
//! each one moves the page's clock onto the cut (the snap is `snap_seek`'s) and cues the picture there,
//! rather than repainting a label beside a player that never moved.
//!
//! One scenario per binary (process-wide cwd), the app-once shape every Narrate widget test uses. The
//! players point at `/bin/true`: this container has no decoder, but the seam is still the leg's own spawn,
//! so a real child is started for every cue and none of it outlives the test.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)]
mod common;
use common::{fixture_dir, hold_last_window, narrate_button as button, release_last_window, settle, widget_in};

static RAN_BUTTONS: AtomicBool = AtomicBool::new(false);
static RAN_SLIDER: AtomicBool = AtomicBool::new(false);
static RAN_WHEEL: AtomicBool = AtomicBool::new(false);

/// Clips with a gap the cut removed between them: 15..25 is gone, so a seek aimed inside it must land on a
/// boundary and not on the second the bar was dragged to.
fn bracketing_clips() -> Vec<Seg> {
    vec![
        Seg { s: 0.0, e: 15.0, cam: 0, ..Default::default() },
        Seg { s: 25.0, e: 40.0, cam: 0, ..Default::default() },
    ]
}

/// §C.4 `cutEdge`: how far inside a clip a backwards scrub lands, so the reading is on the clip whose frame
/// shows rather than on the boundary between it and the stretch the edit removed.
const SCRUB_EDGE: f64 = 0.05;

fn state_at(session: f64) -> ui::NarrateState {
    ui::NarrateState {
        entries: vec![],
        segs: bracketing_clips(),
        takes: vec![],
        voice: String::new(),
        pitch: 0.0,
        session,
        cut_at: session,
        length: 40.0,
        clips: 2,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators: 1,
        voice_files: vec![],
        recording_covers_clip: true,
        playing: false,
        started: false,
        busy: false,
        narration_off: false,
        language: String::new(),
        covered_spans: vec![],
    }
}

fn narrate_page(app: &adw::Application, state: ui::NarrateState) -> adw::ApplicationWindow {
    release_last_window();
    // The seek seams snap against the SESSION's cut (`cut::load`), which is what a preview actually runs, so
    // the scenario files the cut its clips describe. The fixture stays untouched: this is the working session
    // beside the cwd, the same folder every flow saves into.
    let session = naivepost::startup::session_dir(&std::env::current_dir().expect("a cwd to file the cut in"));
    std::fs::create_dir_all(&session).expect("the session folder exists");
    let tree = naivepost::layout::Tree::new(&session).expect("a .naivepost folder");
    naivepost::cut::save(
        &naivepost::cut::Cut {
            segs: state.segs.clone(),
            ..Default::default()
        },
        &tree,
    )
    .expect("cut.json written");
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Narrate");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    ui::set_state(state);
    ui::refresh(&window);
    settle();
    window
}

fn scale(window: &adw::ApplicationWindow, name: &str) -> gtk::Scale {
    widget_in(window, name)
        .and_then(|w| w.downcast::<gtk::Scale>().ok())
        .unwrap_or_else(|| panic!("the Narrate page carries a scale named {name}"))
}

/// Snap in the direction asked, exactly as the page asks it: back from inside the gap lands on the previous
/// clip's end, forward on the next clip's start.
fn run_buttons(app: &adw::Application) {
    // Back: 25 - 3 = 22 sits inside the removed stretch, so the clock ends just INSIDE the previous clip (its
    // end minus §C.4's `cutEdge`), never on the dragged-to second.
    let window = narrate_page(app, state_at(25.0));
    button(&window, "narrate-back-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().session,
        15.0 - SCRUB_EDGE,
        "back from inside a gap snaps to the previous clip's end"
    );
    window.close();
    settle();

    // Forward: 5 + 3 = 8 is inside a clip, so its own answer stands.
    let window = narrate_page(app, state_at(5.0));
    button(&window, "narrate-forward-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(ui::read_state().session, 8.0, "a step inside a clip keeps its second");
    window.close();
    settle();

    // Forward from inside the gap: the next clip's start, not the dragged-to second.
    let window = narrate_page(app, state_at(18.0));
    button(&window, "narrate-forward-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().session,
        25.0,
        "forward from inside a gap snaps to the next clip's start"
    );
    RAN_BUTTONS.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// `settle()` only drains what is already due; a scrub seek is issued by a timer armed 120 ms earlier, so a
/// drag needs WAITING iterations before its effect can be read.
fn wait_settled() {
    let context = glib::MainContext::default();
    for _ in 0..2000 {
        if naivepost::ui::narrate_page_seek::pending_scrub_for_test().is_none() {
            break;
        }
        context.iteration(true);
    }
    settle();
}

fn run_slider(app: &adw::Application) {
    // Nothing has ever played: the drag still seeks, and its snapped second is what the clock takes — the bar
    // is a transport, not a number painted next to one.
    let window = narrate_page(app, state_at(5.0));
    scale(&window, "narrate-slider").set_value(8.0);
    // The drag is debounced (§1's slider: `debounced 120 ms`), so the seek comes when the bar goes quiet rather
    // than on the emission -- `sec_07_narrate_4_parameters_used_widgets` pins that wait itself.
    wait_settled();
    assert_eq!(
        ui::read_state().session,
        8.0,
        "a drag inside a clip takes its own second"
    );
    // And a drag into the stretch the edit removed answers the boundary: forward from 5.0 to 22.0 lands on the
    // next clip's start, the same snap the forward button pins above — never the dragged-to second.
    window.close();
    settle();
    let window = narrate_page(app, state_at(5.0));
    scale(&window, "narrate-slider").set_value(22.0);
    wait_settled();
    assert_eq!(
        ui::read_state().session,
        25.0,
        "a drag into a gap leaves the playhead on the boundary, not on the dragged-to second"
    );
    RAN_SLIDER.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn run_wheel(app: &adw::Application) {
    // Paused, one notch steps a frame and re-cues the still at that second. `step_paused` is the door the
    // `wheel-narrate-picture` controller's own handler calls; GTK gives no way to post a scroll event to an
    // `EventControllerScroll`, so the test fires the handler's entry point and asserts the state it changes.
    let window = narrate_page(app, state_at(5.0));
    let before = ui::read_state().session;
    let stepped = naivepost::ui::narrate_frame_wheel::step_paused(&window, 1.0);
    settle();
    let after = ui::read_state().session;
    assert!(stepped.is_some(), "a notch while paused is ours to answer");
    assert!(after > before, "the notch walked the playhead a frame forward: {before} -> {after}");
    RAN_WHEEL.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn run_round(app: &adw::Application) {
    unsafe {
        std::env::set_var("NAIVEPOST_PICTURE_PLAYER", "/bin/true");
        std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/bin/true");
    }
    run_buttons(app);
    run_slider(app);
    run_wheel(app);
}

/// One scenario, run ONCE: three `#[test]`s share this process and libgio lets only the first
/// `run_with_args` acquire the default main context — every later activation would return at once and no check
/// would run. So each of the three drives the whole round and then asserts its own flag.
fn run_scenario(flag: &std::sync::atomic::AtomicBool) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        // Our own argv: handing cargo-test's flags to libgio aborts before activate runs.
        app.run_with_args(&["naivepost"]);
    });
    assert!(
        flag.load(Ordering::SeqCst),
        "the transport scenario never completed"
    );
}

#[test]
fn f4_5_s4_transport_seeks_snap_to_the_cut_and_cue() {
    run_scenario(&RAN_BUTTONS);
}

#[test]
fn f4_5_s4_the_slider_cues_even_before_a_preview_runs() {
    run_scenario(&RAN_SLIDER);
}

#[test]
fn f4_5_s2_a_paused_wheel_step_cues_the_still() {
    run_scenario(&RAN_WHEEL);
}
