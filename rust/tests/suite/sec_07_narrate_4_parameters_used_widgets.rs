//! §07-narrate#4-parameters-used over the real scrub bar: `narrate.seekDebounceMs` (120 ms) is not a number in
//! a table, it is how long the page waits after the last move of a drag before it seeks. A drag therefore
//! produces ONE seek — the second the finger ended on — while a transport button still seeks at once.
//!
//! One scenario per binary (process-wide cwd), the app-once shape every Narrate widget test uses. The players
//! point at `/bin/true`: this container has no decoder, but the seam is still the leg's own spawn.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Seg;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{
    fixture_dir, hold_last_window, narrate_button as button, release_last_window, settle, widget_in,
};

static RAN_DEBOUNCE: AtomicBool = AtomicBool::new(false);
static RAN_BUTTON_STILL_NOW: AtomicBool = AtomicBool::new(false);

/// Clips with a gap the cut removed between them: 15..25 is gone, so a seek aimed inside it must land on a
/// boundary rather than on the second the bar was dragged to.
fn bracketing_clips() -> Vec<Seg> {
    vec![
        Seg {
            s: 0.0,
            e: 15.0,
            cam: 0,
            ..Default::default()
        },
        Seg {
            s: 25.0,
            e: 40.0,
            cam: 0,
            ..Default::default()
        },
    ]
}

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

/// The session's `cut.json` is what a preview snaps against, so the scenario files the cut its clips describe
/// (the same folder and the same reason as `f4_5_transport_widgets`).
fn narrate_page(app: &adw::Application, state: ui::NarrateState) -> adw::ApplicationWindow {
    release_last_window();
    common::pin_private_cwd();
    let session = naivepost::startup::session_dir(
        &std::env::current_dir().expect("a cwd to file the cut in"),
    );
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
    // Every Cut-page snapshot and widget check needs a size before present() or the page collapses to one column.
    window.set_default_size(1334, 900);
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

/// Dispatch the main context until `done` holds. `settle()` only drains what is already due, so a timer armed
/// a moment ago (the scrub bar's 120 ms debounce) needs a WAITING iteration to be reached.
fn wait_until(done: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    for _ in 0..2000 {
        if done() {
            return;
        }
        context.iteration(true);
    }
    assert!(
        done(),
        "the awaited state never arrived -- a timer did not fire"
    );
}

/// The drag: two moves with NO dispatch between them, which is what a finger crossing the bar actually is — a
/// burst of `value-changed` emissions. Neither may be sought at once; the last one is what survives.
fn run_debounce(app: &adw::Application) {
    let window = narrate_page(app, state_at(5.0));
    // One pending drag per process, so nothing from an earlier check may still be owed here.
    assert_eq!(ui::narrate_page_seek::pending_scrub_for_test(), None);
    let slider = scale(&window, "narrate-slider");
    slider.set_value(8.0);
    slider.set_value(22.0);
    // Nothing dispatched yet: the first move must NOT have been issued, and the pending target is the LAST one.
    assert_eq!(
        ui::read_state().session,
        5.0,
        "a drag does not seek per pixel -- the clock stays where it was until the bar goes quiet"
    );
    assert_eq!(
        ui::narrate_page_seek::pending_scrub_for_test(),
        Some(22.0),
        "the drag that is owed is the second the bar ended on, not the first it passed"
    );
    // Now let the bar be quiet: past SEEK_DEBOUNCE_MS the single owed seek is issued, and it lands on the cut
    // exactly as the forward button's does (22.0 sits in the removed stretch, so the next clip's start).
    wait_until(|| ui::narrate_page_seek::pending_scrub_for_test().is_none());
    settle();
    assert_eq!(
        ui::read_state().session,
        25.0,
        "the one seek the drag owes is the snapped second of the LAST value"
    );
    RAN_DEBOUNCE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The other side of the rule: the debounce belongs to the drag alone. A transport press still seeks at once.
fn run_button_still_now(app: &adw::Application) {
    let window = narrate_page(app, state_at(18.0));
    button(&window, "narrate-forward-3s").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::read_state().session,
        25.0,
        "one deliberate press is answered immediately -- nothing is owed"
    );
    assert_eq!(
        ui::narrate_page_seek::pending_scrub_for_test(),
        None,
        "a button press leaves no drag pending"
    );
    RAN_BUTTON_STILL_NOW.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn run_round(app: &adw::Application) {
    unsafe {
        std::env::set_var("NAIVEPOST_PICTURE_PLAYER", "/bin/true");
        std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/bin/true");
    }
    run_debounce(app);
    run_button_still_now(app);
}

/// One scenario, run ONCE: libgio lets only the first `run_with_args` acquire the default main context, so each
/// test drives the whole round and then asserts its own flag.
fn run_scenario(flag: &std::sync::atomic::AtomicBool) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args(&["naivepost"]);
    });
    assert!(
        flag.load(Ordering::SeqCst),
        "the scrub-debounce scenario never completed"
    );
}

#[test]
fn sec_07_narrate_4_parameters_used_s_seek_debounce_debounces_a_drag() {
    run_scenario(&RAN_DEBOUNCE);
}

#[test]
fn sec_07_narrate_4_parameters_used_s_a_transport_press_still_seeks_at_once() {
    run_scenario(&RAN_BUTTON_STILL_NOW);
}
