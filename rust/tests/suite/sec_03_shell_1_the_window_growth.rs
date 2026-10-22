//! §03-shell.md §1, the half of the window sentence that points UPWARD: "Above the minimum the layout grows
//! with the window and no part has a fixed pixel size: the video preview scales to the room it is given
//! (keeping its aspect), text boxes, lists and the log take the extra height and get their own scrollbar when
//! their content is longer, the timeline's lanes share the height that is left, and splits keep their
//! proportions."
//!
//! `tests/suite/window_shrinks.rs` measures the floor; nothing measured the growing direction, which is how
//! the Cut page could pin its preview to a 240×135 plate and pass every round. So this file allocates the
//! SAME window twice — once at §1's minimum, once well above it — and reads where the picture, the lane under
//! it and the tape actually landed each time. Every number comes from a drawn box (`compute_bounds`,
//! `width()`/`height()` after a real `size_allocate`), never from a `measure` ask, and nothing waits on a
//! guessed amount of time: the layout is forced by the allocation itself.

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};

use naivepost::shell;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_GROWTH: AtomicBool = AtomicBool::new(false);

/// The bigger box the same window is allocated: comfortably past §1's minimum on BOTH axes, so a part that
/// only reacts to width (or only to height) cannot hide behind the other axis staying put.
const BIG_W: i32 = 1900;
const BIG_H: i32 = 1500;

/// A Cut-page window whose session HAS footage, since an empty Cut page draws none of the parts this
/// measures. Built at §1's minimum, laid out, and handed back with its divider for the position checks.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("naivepost-sec03grow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");
    std::env::set_current_dir(&root).expect("cwd pinned to the scratch root");
    let session = naivepost::startup::session_dir(&root);
    let mut seed = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    seed.sources.iter_mut().for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");

    let window = ui::build_window(app, &seed, "Cut");
    window.set_default_size(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    hold_last_window(window.clone());
    lay_out(&window, shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    // …and the Cut page's own divider opened at §A's 380 px: headless its ask lands on GTK's clamp (655 of
    // 660) and the pane below it — toolbar, tracks, strip — is left UNMAPPED, so there would be nothing here
    // to measure growing.
    common::rest_cut_page_split(&window);
    window
}

/// Force one real layout pass at `w`×`h`: GTK gives every widget its box here, so what the checks below read
/// are allocations rather than asks. No waiting: the allocation IS the event.
fn lay_out(window: &adw::ApplicationWindow, w: i32, h: i32) {
    window.present();
    let widget = window.upcast_ref::<gtk::Widget>();
    if !widget.is_mapped() {
        widget.map();
    }
    widget.size_allocate(&gtk::Allocation::new(0, 0, w, h), -1);
    settle();
    // The shell's resting split, asked and then re-allocated: headless a paned moves its handle without
    // re-allocating its halves, so the band keeps its old box until someone allocates again. WITHOUT this the
    // minimum-size measurement reads GTK's own default split (measured 488 px of band against §1's 53 px) and
    // nothing at the minimum is measurable. It has to be THIS window's size that gets allocated again — the
    // shared helper allocates §1's minimum, which would shrink a bigger box back down.
    naivepost::ui::window_divider::rest_split(
        &ui::find_source_widget(window, "main-divider")
            .expect("§1: page and bottom bar are two halves of a divider")
            .downcast::<gtk::Paned>()
            .expect("the divider is a gtk::Paned"),
    );
    widget.size_allocate(&gtk::Allocation::new(0, 0, w, h), -1);
    settle();
}

/// The shell's page/bottom-bar divider.
fn divider(window: &adw::ApplicationWindow) -> gtk::Paned {
    ui::find_source_widget(window, "main-divider")
        .expect("§1: page and bottom bar are two halves of a divider")
        .downcast()
        .expect("the divider is a gtk::Paned")
}

/// Where a named widget was drawn, panicking with the name in the message: a missing box is itself a failure
/// of "on screen at once", not something to skip.
fn drawn(window: &adw::ApplicationWindow, named: &str) -> (f64, f64, f64, f64) {
    ui::window_shot::bounds_in_window_for_test(window.upcast_ref(), named)
        .unwrap_or_else(|| panic!("{named} has no drawn box"))
}

/// §1: "the video preview scales to the room it is given (keeping its aspect)" and "the timeline's lanes
/// share the height that is left". Measured by giving one live Cut window two allocations and comparing where
/// its picture landed in each: at the minimum it already stands at §A's floor, and past the minimum it must be
/// taller again — a panel pinned to a pixel size would report the same height twice.
fn check_the_preview_grows_with_the_room(app: &adw::Application) {
    let window = cut_window(app);
    let (_, _, _, small_h) = drawn(&window, "preview-panel");
    let handle_small = divider(&window).position();
    assert!(
        small_h >= ui::cut_page_layout::PREVIEW_FLOOR_H,
        "at the minimum the preview already stands at \u{a7}A's floor ({} px), not its {} px thumbnail shape: {small_h}",
        ui::cut_page_layout::PREVIEW_FLOOR_H,
        ui::window::PREVIEW_PANEL_H
    );

    lay_out(&window, BIG_W, BIG_H);
    let (_, _, big_w, big_h) = drawn(&window, "preview-panel");
    println!("GROWTH preview {BIG_W}x{BIG_H} window: panel {big_w}x{big_h} (was {small_h} tall at the minimum)");
    assert!(
        big_h > small_h,
        "\u{a7}1: above the minimum the preview takes the room it is given, but its drawn height did not grow: {small_h} -> {big_h}"
    );
    assert!(
        big_w > shell::WINDOW_MIN_W as f64 / 2.0,
        "and it keeps its share of the width too: {big_w} px"
    );
    // The lane under the frame and the tape under the pane both still have pixels: the leftover height is
    // shared, not spent on the picture alone. The window's own box is read off the widget rather than the
    // allocation asked for: under xvfb the headless WM reports a few px more than §1's number (the same
    // reason the band check below measures `window.height()`), and a control near the bottom edge would
    // otherwise fail a comparison against a size the window was never given.
    let (win_w, win_h) = (
        window.width() as f64,
        window.height() as f64,
    );
    for name in ["preview-panel", "cut-preview-stack", "effects-lane", "track-strip", "cut-bottom"] {
        let (x, y, w, h) = drawn(&window, name);
        assert!(
            w > 0.0 && h > 0.0 && x >= -0.5 && y >= -0.5 && x < win_w && y < win_h,
            "\u{a7}1: {name} lost its pixels when the window grew: ({x:.0},{y:.0},{w:.0},{h:.0}) in a \
             {win_w:.0}x{win_h:.0} px window",
        );
    }
    // Splits keep their proportions: the shell's own divider hands the extra height to the PAGE half, so
    // its position grows with the window rather than sitting where the minimum left it.
    let handle_big = divider(&window).position();
    println!("GROWTH divider position {handle_small} -> {handle_big} for {BIG_H} px of window");
    assert!(
        handle_big > handle_small,
        "\u{a7}1: the page half should grow with the window, but the divider stayed at {handle_small}: {handle_big}"
    );
    RAN_GROWTH.store(true, Ordering::SeqCst);
    window.close();
}

/// §1's collapsed band keeps its own height however tall the window gets, and it still ends at the window's
/// bottom edge: the log's scroller is where a long log goes, so nothing empty opens under the run bar.
fn check_the_bottom_band_keeps_its_own_height(app: &adw::Application) {
    let window = cut_window(app);
    let (_, _, _, small_band) = drawn(&window, "bottom-bar");
    lay_out(&window, BIG_W, BIG_H);
    let (ex, ey, _, eh) = drawn(&window, "log-expander");
    let (_, _, _, band_h) = drawn(&window, "bottom-bar");
    println!("BAND at {BIG_W}x{BIG_H}: expander y {ey} h {eh} x {ex}; bottom-bar {band_h} (was {small_band})");
    // The window's own bottom edge, which under xvfb sits a little below the box it was allocated (the
    // headless WM adds decoration height), so "no empty space" is measured against the real edge.
    let win_h = window.height() as f64;
    assert!(
        (ey + eh - win_h).abs() <= 2.0,
        "\u{a7}1: with the log collapsed there is no empty space under the band: it ends at {:.0}+{:.0} in a {win_h:.0} px window", ey + eh, eh
    );
    // The band did not grow with the window: what it gained over the minimum is GTK's own spacing around the
    // two rows \u00a71 counts, printed above rather than hidden.
    assert!(
        band_h - small_band <= 60.0,
        "\u{a7}1: the collapsed band keeps its own height instead of eating the page's extra room: {small_band} -> {band_h}"
    );
    window.close();
}

#[test]
fn sec_03_shell_1_the_window_s1h_above_the_minimum_the_preview_grows_with_the_room() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        check_the_preview_grows_with_the_room(app);
        check_the_bottom_band_keeps_its_own_height(app);
    });
    assert!(RAN_GROWTH.load(Ordering::SeqCst), "the growth round never ran");
}
