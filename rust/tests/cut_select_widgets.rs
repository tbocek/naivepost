//! F2.6 — the wire: a real drag on `select-surface` selects what it was drawn on, and the real ✕
//! clears it, both landing in the same state tests/cut_select_band.rs asserts from the logic side.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's selection slot and readout), so they run once and report through flags the test
//! asserts on — the shape tests/cut_hear_mix_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut_hear;
use naivepost::cut_select::{self, Scope, Surface};
use naivepost::shell::Page;
use naivepost::ui;

static RAN_DRAG: AtomicBool = AtomicBool::new(false);
static RAN_SOUND: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// A window sitting on the Cut page, built fresh per check so one check's band cannot leak into
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
    assert_eq!(ui::state(&window).page, Page::Cut, "the Cut page is showing");
    settle();
    window
}

/// Let GTK finish laying out and mapping what was just shown. gtk4-rs 0.11 exposes no free
/// `events_pending`, so pump the GLib main context directly.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

/// The page's pixels-per-second for the placeholder surface — read from the wiring's own constant, so
/// the seconds asserted here are the seconds the handler computed, not ones back-solved by hand.
const PPS: f64 = ui::SELECT_SURFACE_PPS;

/// F2.6 S1 + S2 + S3: a real drag draws a band scoped to its ground, the readout follows it live, and
/// the real cross clears it.
fn check_a_real_drag_selects_and_the_cross_clears(app: &adw::Application) {
    let window = cut_window(app);

    // Nothing selected at first: the cross is insensitive, so it cannot pretend there is something to
    // clear, and the readout shows the unclicked face rather than a zero-length span.
    let cross = ui::clear_selection_button(&window).expect("the Cut page carries clear-selection");
    assert!(cross.is_mapped(), "the cross is drawn on the visible Cut page");
    assert!(!cross.is_sensitive(), "with no band there is nothing to clear");
    assert!(ui::selection(&window).is_none(), "a fresh window holds no band");
    assert_eq!(
        ui::selection_readout(&window)
            .expect("the Cut page carries selection-readout")
            .text()
            .as_str(),
        cut_select::READOUT_NONE,
        "the empty readout is the module's own unclicked text"
    );

    // The drag surface is really there and really carries a gesture.
    let area = ui::select_surface(&window).expect("the Cut page carries select-surface");
    println!(
        "LAYOUT select-surface width={} height={} mapped={}",
        area.width(),
        area.height(),
        area.is_mapped()
    );
    assert!(area.is_mapped(), "the drag surface is mapped");
    assert!(area.width() > 40, "surface wide enough to drag on, got {} px", area.width());
    assert!(area.height() >= 48, "surface tall enough, got {} px", area.height());
    let gesture = ui::selection_gesture(&window).expect("the surface carries a drag gesture");

    // Fire the drag the way the toolkit does. The handler converts x to session seconds at PPS, so a
    // drag out to 160 px means 40 s -- and the assertion below compares against the rule itself, not
    // against a number typed twice.
    gesture.emit_by_name::<()>("drag-begin", &[&0.0f64, &0.0f64]);
    gesture.emit_by_name::<()>("drag-update", &[&160.0f64, &0.0f64]);
    gesture.emit_by_name::<()>("drag-end", &[&160.0f64, &0.0f64]);
    settle();

    let expected = cut_select::draw(Surface::Ruler, None, 0.0, 160.0 / PPS)
        .expect("the rule itself accepts this surface and span");
    let band = ui::selection(&window).expect("the drag drew a band");
    assert_eq!(
        band, expected,
        "the widget path returns exactly what cut_select::draw returns for the same surface and seconds"
    );
    assert!(
        matches!(band.scope, Scope::Footage { row } if row == cut_select::ANY_ROW),
        "a drag on ruler ground selects the whole timeline's footage, not one camera's"
    );

    // S3: the readout followed the band, in the page's own clock form.
    let readout = ui::selection_readout(&window).expect("readout present").text().to_string();
    println!("READOUT {readout}");
    assert!(
        readout.starts_with(cut_select::READOUT_PREFIX),
        "the readout is labelled, got {readout}"
    );
    assert!(
        readout.contains(&cut_hear::scene_clock(band.start))
            && readout.contains(&cut_hear::scene_clock(band.end)),
        "the readout carries the band's own from/to, got {readout}"
    );
    assert_ne!(readout, cut_select::READOUT_NONE, "a band replaces the empty text");

    // And the cross woke up because there is now something to clear.
    assert!(cross.is_sensitive(), "holding a band makes the cross live");

    // S2: press the real cross.
    cross.emit_by_name::<()>("clicked", &[]);
    settle();
    assert!(ui::selection(&window).is_none(), "the cross cleared the band");
    assert!(
        !cross.is_sensitive(),
        "and the cross went insensitive again -- the sensitivity tracks the state"
    );
    let after = ui::selection_readout(&window).expect("readout present").text().to_string();
    assert_eq!(after, cut_select::READOUT_NONE, "the readout fell back to the empty text");

    window.close();
}

/// F2.6 S4: a SOUND-scoped band greys Add/Split/Remove and re-aims Copy and Insert at sound --
/// through the page's seam, checking the same `Verbs` fields the logic tests check.
fn check_a_sound_selection_reaches_the_verb_state(app: &adw::Application) {
    let window = cut_window(app);
    let recording = "2026-09-16 17-26-20";

    // Sixty seconds of a lane's sound -- long past P.policy.minSceneSeconds, so the grey-out that
    // follows is scope alone and not a length floor quietly doing the work.
    let band = ui::draw_selection(
        &window,
        Surface::Lane,
        Some(recording),
        10.0,
        70.0,
    )
    .expect("a drag on a lane selects that recording's sound");
    assert_eq!(
        band.scope,
        Scope::Sound { recording: recording.to_string() },
        "the band is scoped to the one recording it was drawn on"
    );
    // P.policy.minSceneSeconds = 1.0 is well under this band, so S4's grey-out is about scope.
    assert!(band.length() >= cut_select::MIN_SCENE_SECONDS);

    let verbs = ui::selection_verbs(&window);
    assert!(!verbs.add, "a sound selection greys Add");
    assert!(!verbs.split, "a sound selection greys Split");
    assert!(!verbs.remove, "a sound selection greys Remove");
    assert!(verbs.copy, "Copy still works -- it takes the sound");
    assert_eq!(
        verbs.insert,
        cut_select::InsertAim::Sound,
        "and Insert aims at sound, not between the footage"
    );

    // Clear it and the verbs come back: the state is live, not latched.
    assert!(ui::clear_selection(&window), "there was something to clear");
    let after = ui::selection_verbs(&window);
    assert!(!after.add, "with nothing selected Add stays off -- it needs a band");
    assert!(!after.copy, "and Copy has nothing to take");
    assert_eq!(
        after.insert,
        cut_select::InsertAim::Footage,
        "Insert falls back to footage with no sound selection"
    );

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
            check_a_real_drag_selects_and_the_cross_clears(app);
            RAN_DRAG.store(true, Ordering::SeqCst);
            check_a_sound_selection_reaches_the_verb_state(app);
            RAN_SOUND.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_6_s1_and_s2_a_drag_selects_and_the_cross_clears_through_real_widgets() {
    window_round();
    assert!(RAN_DRAG.load(Ordering::SeqCst), "the drag-and-cross check never ran");
    assert!(RAN_SOUND.load(Ordering::SeqCst), "the sound-scope verb check never ran");
}
