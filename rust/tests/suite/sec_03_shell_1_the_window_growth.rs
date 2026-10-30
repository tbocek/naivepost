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
//! `width()`/`height()` once GTK's frames show the layout at rest), never from a `measure` ask, and nothing
//! waits on a guessed amount of time.

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};

use naivepost::shell;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, wait_until};

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
    seed.sources
        .iter_mut()
        .for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");

    let window = ui::build_window(app, &seed, "Cut");
    window.set_default_size(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    hold_last_window(window.clone());
    lay_out(&window, shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    common::wait_for_stable_layout(&window);
    window
}

/// Grow the window to `w`×`h` and wait until it is that big, so what the checks below read are the boxes GTK
/// gave a window of that size, rather than a size forced on it for one frame.
fn lay_out(window: &adw::ApplicationWindow, w: i32, h: i32) {
    window.present();
    // The window grows for real: a size request moves the X window, and the shell lays itself out at the
    // new size with nothing asked of it by the test.
    window.set_size_request(w, h);
    common::wait_for_stable_layout(window);
    let surface = window.surface().expect("a presented window has a surface");
    assert_eq!(
        (surface.width(), surface.height()),
        (w, h),
        "the window took its {w}x{h} size"
    );
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
    let (win_w, win_h) = (window.width() as f64, window.height() as f64);
    for name in [
        "preview-panel",
        "cut-preview-stack",
        "effects-lane",
        "track-strip",
        "cut-bottom",
    ] {
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

/// §1's margin under the bottom bar: the band ends this far above the window's bottom edge.
const MARGIN: f64 = naivepost::ui::margins::BAR_EDGE_PX as f64;

/// §1's collapsed band keeps its own height however tall the window gets, and it still ends §1's 12 px margin
/// above the window's bottom edge: the log's scroller is where a long log goes, so nothing empty opens under the run bar.
fn check_the_bottom_band_keeps_its_own_height(app: &adw::Application) {
    let window = cut_window(app);
    let (_, _, _, small_band) = drawn(&window, "bottom-bar");
    lay_out(&window, BIG_W, BIG_H);
    // The divider settles on the new height through the same allocations: wait for the band to get there.
    wait_until(
        "the collapsed band reached the window's bottom margin",
        || {
            let (_, ey, _, eh) = drawn(&window, "log-expander");
            (ey + eh + MARGIN - window.height() as f64).abs() <= 2.0
        },
    );
    let (ex, ey, _, eh) = drawn(&window, "log-expander");
    let (_, _, _, band_h) = drawn(&window, "bottom-bar");
    println!("BAND at {BIG_W}x{BIG_H}: expander y {ey} h {eh} x {ex}; bottom-bar {band_h} (was {small_band})");
    // The window's own bottom edge, which under xvfb sits a little below the box it was allocated (the
    // headless WM adds decoration height), so "no empty space" is measured against the real edge.
    let win_h = window.height() as f64;
    assert!(
        (ey + eh + MARGIN - win_h).abs() <= 2.0,
        "\u{a7}1: with the log collapsed there is no empty space under the band but its 12 px margin: it ends at {:.0}+{:.0} in a {win_h:.0} px window", ey + eh, eh
    );
    // The band did not grow with the window: what it gained over the minimum is GTK's own spacing around the
    // two rows \u00a71 counts, printed above rather than hidden.
    assert!(
        band_h - small_band <= 60.0,
        "\u{a7}1: the collapsed band keeps its own height instead of eating the page's extra room: {small_band} -> {band_h}"
    );
    window.close();
}

/// §1's one rule on every page: 12 px from the window's edges and from the divider under the page, one line for a
/// divider, one height for a button. And nothing empty under Cut's tape: the bottom half is its three rows.
fn check_every_page_keeps_the_same_margins(app: &adw::Application) {
    let (mut seed, session) = {
        release_last_window();
        let root = std::env::temp_dir().join(format!("naivepost-sec03edge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch root");
        std::env::set_current_dir(&root).expect("cwd pinned to the scratch root");
        let session = naivepost::startup::session_dir(&root);
        let seed = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
        std::fs::create_dir_all(&session).expect("session folder");
        (seed, session)
    };
    seed.sources.iter_mut().for_each(|source| source.footage = true);
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");
    fn walk(widget: &gtk::Widget, seen: &mut Vec<gtk::Widget>) {
        seen.push(widget.clone());
        let kids = widget.observe_children();
        for index in 0..kids.n_items() {
            if let Some(child) = kids.item(index).and_then(|c| c.downcast::<gtk::Widget>().ok()) {
                walk(&child, seen);
            }
        }
    }
    for (page, top) in [
        ("Prepare", "prepare-split"),
        ("Cut", "cut-page-split"),
        ("Narrate", "narrate-left-split"),
        ("Produce", "produce-split"),
    ] {
        let window = ui::build_window(app, &seed, page);
        hold_last_window(window.clone());
        window.present();
        common::wait_for_stable_layout(&window);
        let (x, _, _, h) = drawn(&window, top);
        let (_, y, _, _) = drawn(&window, top);
        assert_eq!(x, 12.0, "{page}: the page keeps 12 px from the window's left edge");
        let main = divider(&window);
        let (_, main_y, _, _) = drawn(&window, "main-divider");
        let foot = main_y + f64::from(main.position()) - (y + h);
        assert!((foot - 12.0).abs() <= 1.0, "{page}: 12 px above the divider under the page, not {foot}");
        let mut seen = Vec::new();
        walk(window.content().expect("content").upcast_ref(), &mut seen);
        for widget in seen.iter().filter(|w| w.is_mapped()) {
            let kind = widget.type_().name();
            if kind == "GtkPanedHandle" {
                assert_eq!(widget.width().min(widget.height()), 1, "{page}: a divider is one line");
            }
            if kind == "GtkButton" && widget.height() > 0 && widget.ancestor(adw::HeaderBar::static_type()).is_none() && widget.widget_name() != "preview-picture-button" {
                assert_eq!(widget.height(), 34, "{page}: {} is one height", widget.widget_name());
            }
        }
        let plate = match page {
            "Cut" => Some("preview-picture-button"),
            "Narrate" => Some("narrate-video"),
            _ => None,
        };
        if let Some(name) = plate {
            let widget = common::find_named(&window, name).expect("the video preview");
            assert!(widget.has_css_class("video-plate"), "{page}: {name} stands on the one video plate");
        }
        // An open log gives way; the run bar and the log header stay inside the window, on every page.
        let log: gtk::Expander = ui::find_source_widget(&window, "log-expander")
            .expect("the log expander")
            .downcast()
            .expect("an expander");
        log.set_expanded(true);
        common::wait_for_stable_layout(&window);
        let (_, bar_y, _, bar_h) = drawn(&window, "bottom-bar");
        assert!(
            bar_y + bar_h <= f64::from(window.height()),
            "{page}: the bottom bar ends at {} in a {} px window",
            bar_y + bar_h,
            window.height()
        );
        log.set_expanded(false);
        common::wait_for_stable_layout(&window);
        if page == "Cut" {
            // 12 px between the line and the toolbar under it, as on each side of every divider.
            let cut_split: gtk::Paned = ui::find_source_widget(&window, "cut-page-split")
                .expect("the top/bottom divider")
                .downcast()
                .expect("a paned");
            let (_, split_y, _, _) = drawn(&window, "cut-page-split");
            let (_, toolbar_y, _, _) = drawn(&window, "cut-toolbar");
            let under = toolbar_y - (split_y + f64::from(cut_split.position()));
            assert!((under - 13.0).abs() <= 1.0, "the toolbar sits 12 px under the line: {under}");
            // The openings are not minimums: the handle goes left of the preview's 660 px opening.
            let top: gtk::Paned = ui::find_source_widget(&window, "cut-top")
                .expect("the left/right divider")
                .downcast()
                .expect("a paned");
            top.set_position(300);
            common::wait_for_stable_layout(&window);
            assert!(top.position() <= 300, "the preview gets smaller: {}", top.position());
            let (_, tape_y, _, tape_h) = drawn(&window, "track-layers");
            let (_, bottom_y, _, bottom_h) = drawn(&window, "cut-bottom");
            assert!(
                bottom_y + bottom_h - (tape_y + tape_h) <= 40.0,
                "under the tape only the scrollbar's room: {} px",
                bottom_y + bottom_h - (tape_y + tape_h)
            );
        }
        window.close();
        common::settle();
    }
}

/// The grip above the sound strip through the page: the pointer offers it at the line and not beside it, a
/// drag's height lands on the band, the row's sound switch follows, and the status line says the height and the
/// end of its travel.
fn check_the_sound_strip_grip_sizes_the_strip(app: &adw::Application) {
    use naivepost::ui::{cut_tracks, cut_view, window};
    release_last_window();
    let root = std::env::temp_dir().join(format!("naivepost-sec03wave-{}", std::process::id()));
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
    window.present();
    common::wait_for_stable_layout(&window);

    let scene = cut_view::band_scene_for(&window);
    let rows = cut_tracks::rows_for_press(&scene);
    let x = 600.0;
    let line = rows.wave_top(0);
    assert_eq!(ui::band_cursor_at(&window, x, line), Some("row-resize"), "the line offers the grip");
    assert_ne!(ui::band_cursor_at(&window, x, line - 10.0), Some("row-resize"), "the pictures do not");

    let status = || ui::find_status(window.upcast_ref()).expect("the status line").text().to_string();
    let was = cut_tracks::row_wave_px();
    let now = window::press_wave(&window, was + 10.0);
    assert_eq!(now, was + 10.0, "the drag's height is the strip's");
    assert_eq!(cut_tracks::row_wave_top(&window, 0), line - 10.0, "and the line moved with the hand");
    assert_eq!(status(), format!("sound strip {} px", now.round()));
    window::press_wave(&window, 10_000.0);
    assert!(status().contains("at its largest"), "{}", status());
    window::press_wave(&window, 0.0);
    assert!(status().contains("at its smallest"), "{}", status());
    window::press_wave(&window, was);
    window.close();
}

/// §1 "the toolbar, the tracks, the scrollbar": the Cut page's bottom half stays inside the page, whichever way
/// the page's height is taken from it. Dragging the page's divider down, or making the open log taller, takes
/// it from the picture. The first rewrite let the handle go past the half's minimum, so the half kept its height
/// and slid out of the page: the tape's lower rows and the scrollbar were cut off (08-dex, 2026-10-30). Read off
/// drawn boxes, so it holds on any headless display.
fn check_the_cut_bottom_half_keeps_its_floor(app: &adw::Application) {
    release_last_window();
    let root = std::env::temp_dir().join(format!("naivepost-sec03cut-{}", std::process::id()));
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
    window.present();
    window.set_size_request(BIG_W, BIG_H);
    common::wait_for_stable_layout(&window);
    let split: gtk::Paned = ui::find_source_widget(&window, "cut-page-split")
        .expect("the Cut page's divider")
        .downcast()
        .expect("a paned");
    let holds = |when: &str| {
        let (_, y, _, height) = drawn(&window, "cut-bottom");
        let (_, split_y, _, split_h) = drawn(&window, "cut-page-split");
        assert!(
            y + height <= split_y + split_h + 1.0,
            "{when}: the bottom half ends at {} but the page ends at {}: the tape's lower rows and the scrollbar are cut off",
            y + height,
            split_y + split_h
        );
    };
    split.set_position(split.height());
    common::wait_for_stable_layout(&window);
    holds("with the divider dragged all the way down");

    let log: gtk::Expander = ui::find_source_widget(&window, "log-expander")
        .expect("the log expander")
        .downcast()
        .expect("an expander");
    log.set_expanded(true);
    common::wait_for_stable_layout(&window);
    let main = divider(&window);
    main.set_position(main.position() - 300);
    common::wait_for_stable_layout(&window);
    holds("with the open log dragged taller");
    window.close();
}

#[test]
fn sec_03_shell_1_the_window_s1h_above_the_minimum_the_preview_grows_with_the_room() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        check_the_preview_grows_with_the_room(app);
        check_the_bottom_band_keeps_its_own_height(app);
        check_the_open_log_takes_the_extra_height(app);
        check_the_cut_bottom_half_keeps_its_floor(app);
        check_the_sound_strip_grip_sizes_the_strip(app);
        check_every_page_keeps_the_same_margins(app);
    });
    assert!(
        RAN_GROWTH.load(Ordering::SeqCst),
        "the growth round never ran"
    );
}


/// §1: "text boxes, lists and the log take the extra height" and "splits keep their proportions". With the log
/// OPEN, a window that grows keeps the band's share of the height, so the log grows with it; a handle dragged up
/// gives the log that room too; and either way the log's text box reaches the band's bottom margin, with no
/// empty space under it. The first rewrite snapped the open band back to 220 px of log at every resize and left
/// a dragged band blank under the log. Read off drawn boxes and the window's own height only, so it holds on any
/// headless display (the surface's size includes client-side shadows on some).
fn check_the_open_log_takes_the_extra_height(app: &adw::Application) {
    release_last_window();
    let root = std::env::temp_dir().join(format!("naivepost-sec03log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");
    std::env::set_current_dir(&root).expect("cwd pinned to the scratch root");
    let session = naivepost::startup::session_dir(&root);
    let seed = naivepost::project::load(&fixture_dir()).expect("the snapshot fixture loads");
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");
    let window = ui::build_window(app, &seed, "Prepare");
    window.set_default_size(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    hold_last_window(window.clone());
    window.present();
    window.set_size_request(shell::WINDOW_MIN_W, shell::WINDOW_MIN_H);
    common::wait_for_stable_layout(&window);
    let log: gtk::Expander = ui::find_source_widget(&window, "log-expander")
        .expect("the log expander")
        .downcast()
        .expect("an expander");
    log.set_expanded(true);
    common::wait_for_stable_layout(&window);
    let fills = |when: &str| {
        let (_, ey, _, eh) = drawn(&window, "log-expander");
        let win_h = window.height() as f64;
        assert!(
            (ey + eh + MARGIN - win_h).abs() <= 2.0,
            "\u{a7}1, {when}: the open log reaches the band's bottom margin: it ends at {:.0} in a {win_h:.0} px window",
            ey + eh
        );
        drawn(&window, "log-scroller").3
    };
    let at_min = fills("at the minimum");

    window.set_size_request(BIG_W, BIG_H);
    common::wait_for_stable_layout(&window);
    let grown = fills("in a taller window");
    assert!(
        grown > at_min + 100.0,
        "\u{a7}1: the open log takes some of the extra height: {at_min} -> {grown}"
    );

    let d = divider(&window);
    d.set_position(d.position() - 300);
    common::wait_for_stable_layout(&window);
    let dragged = fills("with the handle dragged up");
    assert!(
        dragged >= grown + 250.0,
        "\u{a7}1: a handle dragged up 300 px gives the log that room: {grown} -> {dragged}"
    );
    window.close();
}
