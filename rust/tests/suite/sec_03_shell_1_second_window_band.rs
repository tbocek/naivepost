//! §03-shell#1 — the collapsed band keeps its own height in EVERY window, not just the first one a
//! process opens. Headless, GTK gives a fresh `gtk::Paned` its own default split (measured: 707 px of band
//! against §1's ~53 px in an 843 px divider) and never re-notifies the paned of its size, so the shell has to
//! ask its resting split for each window it builds. When it asks too early — before the new window is
//! allocated — that window keeps the 707 px split for good and the Cut page's toolbar lands off screen.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::{self, Page};
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, settle};

static RAN_SECOND_WINDOW: AtomicBool = AtomicBool::new(false);

/// §1's collapsed band, measured off the drawn window: what the divider's bottom half actually got.
fn band_height(window: &adw::ApplicationWindow) -> f64 {
    ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), "bottom-bar")
        .map(|(_, _, _, h)| h)
        .unwrap_or_else(|| panic!("§1: the shell draws a bottom-band"))
}

/// A window on the Cut page whose session HAS footage, so the page draws the controls this measures.
fn cut_page_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Cut");
    lay_out(&window);
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "the Cut page is showing"
    );
    window
}

/// Force one real layout pass at §1's minimum: `present()` alone leaves an Xvfb window UNRESIZED (measured:
/// it keeps GTK's 200×120 default and then reports its content's own minimum height), so every box below
/// would be measured against a window that was never sized. The allocation IS the event — no waiting.
fn lay_out(window: &adw::ApplicationWindow) {
    window.present();
    hold_last_window(window.clone());
    let widget = window.upcast_ref::<gtk::Widget>();
    if !widget.is_mapped() {
        widget.map();
    }
    widget.size_allocate(
        &gtk::Allocation::new(0, 0, shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        -1,
    );
    settle();
    // …and once more AFTER the resting ask has run (`settle` drains it): headless a paned does not re-allocate
    // its halves when a handler moves its handle — measured: `position()` answers the new number while both
    // children keep their old boxes — so the drawn geometry only catches up on the next allocation.
    widget.size_allocate(
        &gtk::Allocation::new(0, 0, shell::WINDOW_MIN_W, shell::WINDOW_MIN_H),
        -1,
    );
    settle();
}

/// The second window built while the first is still alive gets §1's band too, and its toolbar is on screen.
fn check_the_second_window_keeps_the_band(app: &adw::Application) {
    // First window: Prepare, laid out, left open — the shape a test round leaves behind.
    let first = {
        let model = naivepost::project::load(&fixture_dir()).expect("the fixture loads");
        let window = ui::build_window(app, &model, "Prepare");
        lay_out(&window);
        window
    };
    let first_band = band_height(&first);
    println!("BAND first window: {first_band:.0} px");

    // The FIRST window stays open and held: this is the case being tested — a second shell built while an
    // earlier one is still alive. `release_last_window` would close it and the round would measure one
    // window twice instead of two windows at once.
    let second = cut_page_window(app);
    let second_band = band_height(&second);
    println!(
        "BAND second window: {second_band:.0} px (window {} px tall)",
        second.height()
    );
    // §1 counts the run-bar row plus the log header for a closed log — tens of pixels, printed above.
    // Asserting a ceiling rather than an exact number: the rows' heights follow the theme's font metrics.
    assert!(
        second_band < 120.0,
        "§1: every window gets the collapsed band, not GTK's own default split: {second_band:.0} px \
         drawn in a {} px window (the first window shows {first_band:.0} px)",
        second.height()
    );
    // What the band stealing the page's room actually costs: the Cut toolbar falls below the window edge.
    for name in ["play-cut-button", "preview-volume"] {
        assert!(
            ui::window_shot::is_on_screen_for_test(second.upcast_ref(), name),
            "§1: {name} is painted in the second window's Cut page: {:?}",
            ui::window_shot::bounds_in_window_for_test(second.upcast_ref(), name)
        );
    }
    RAN_SECOND_WINDOW.store(true, Ordering::SeqCst);
    second.close();
    first.close();
    // Hand the shell back to the next round with nothing standing on LAST_WINDOW.
    common::release_last_window();
}

#[test]
fn sec_03_shell_1_s1h_a_second_window_gets_the_collapsed_band_too() {
    common::pin_private_cwd();
    common::one_app_round(check_the_second_window_keeps_the_band);
    assert!(
        RAN_SECOND_WINDOW.load(Ordering::SeqCst),
        "the second-window round never ran"
    );
}
