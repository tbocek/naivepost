//! §05-cut#1-screen — the scrollbar. `spec/05-cut.md` §1: "scrollbar (hidden when everything fits,
//! page-wide drag anywhere)" and §A's Tracks paragraph: "a horizontal bar under both bands". The rules
//! live in `cut_screen` (whether a span needs a bar at all) and in `ui::cut_view` (this window's bar);
//! this drives both through real widgets on a realized Cut page: a shallow zoom where the session fits
//! shows no bar, deepening it past the fit puts one there, and scrolling moves what the page looks at
//! without moving anything on the timeline.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut;
use naivepost::cut_screen;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, seg};

static RAN_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_DRAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The bar the page builds for its bands (`cut_tracks::build` names it this, and
/// `cut_view::refresh_page_scrollbar` looks it up by it).
const BAR: &str = "track-scrollbar";

/// A window sitting on the Cut page whose tape is `tape` (what the session filmed) with `seeded` as the
/// cut under test. cwd is pinned to its own temp root so nothing resolves the project through `rust/`
/// (`startup::session_dir(current_dir())` is how the page finds its folder), and the previous check's
/// window is released first — a closed GTK window survives with its bar still parented to it, and a name
/// lookup from a new window then reaches the leftover. Shape of `tests/fold_rows_widgets.rs`.
fn cut_page_taped(
    app: &adw::Application,
    tape: &[naivepost::timeline::Recording],
    seeded: &cut::Cut,
) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-scroll-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    // The tape first, then the cut: the view's length comes from the tape, so the other order would size
    // the first frame of this window against the previous check's session.
    ui::set_session_recordings(tape);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// The three-scene cut every check here sits on.
fn three_scenes() -> cut::Cut {
    cut::Cut { segs: vec![seg(0.0, 30.0), seg(30.0, 60.0), seg(70.0, 90.0)], ..Default::default() }
}

/// One camera that filmed the whole session, which is what makes the zoom floor a number the checks can
/// reason about: at the floor all of it is on screen. Short enough that even the placeholder band's 450 px
/// view holds it at the opening 4 px a second (36 s → 174 px), long enough to be a session.
fn short_tape() -> Vec<naivepost::timeline::Recording> {
    vec![naivepost::timeline::Recording { base: "session.mkv".into(), start: 0.0, end: 36.0 }]
}

/// The same camera over a three-hour session: no window this page is ever given fits 10800 s at any zoom
/// down to its floor, so the bar has a reason to exist as soon as the tape outgrows the view.
fn long_tape() -> Vec<naivepost::timeline::Recording> {
    vec![naivepost::timeline::Recording { base: "marathon.mkv".into(), start: 0.0, end: 10800.0 }]
}

/// First widget in the tree with this name.
fn find_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        for child in widget.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

fn bar(window: &adw::ApplicationWindow) -> gtk::Scrollbar {
    find_named(window, BAR)
        .and_then(|widget| widget.downcast::<gtk::Scrollbar>().ok())
        .expect("the Cut page builds its bands with a horizontal bar under them")
}

/// `(total, view)` the page's own bar is being painted against right now.
fn extents(window: &adw::ApplicationWindow) -> (f64, f64) {
    let surface = ui::cut_view::surface_width(window);
    ui::cut_view::extents(surface)
}

/// S2 + §1 "hidden when everything fits": a session the view can hold shows no bar at all — `fits` is the
/// same predicate `paint_scrollbar` hides it with, so the check cannot pass while the widget disagrees
/// with the rule.
fn check_the_bar_is_hidden_while_everything_fits(app: &adw::Application) {
    let window = cut_page_taped(app, &short_tape(), &three_scenes());
    let (total, view) = extents(&window);
    assert!(
        ui::cut_view::fits(total, view),
        "a {total} px tape in a {view} px view fits: the bar stays hidden"
    );
    assert!(!cut_screen::scrollbar_shown(total, view), "the driver agrees: no bar needed");
    let bar = bar(&window);
    assert!(!bar.is_visible(), "the bar is hidden while the session fits");
    RAN_HIDDEN.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// S3: a session that no view can hold puts the bar there by itself — the toolbar's own + ladder then only
/// makes it longer, and "shown" stays the same bool the paint used.
fn check_the_bar_appears_when_the_session_stops_fitting(app: &adw::Application) {
    let window = cut_page_taped(app, &long_tape(), &three_scenes());
    let (total, view) = extents(&window);
    assert!(
        !ui::cut_view::fits(total, view),
        "{total} px of tape will not fit a {view} px view at any zoom"
    );
    assert!(
        cut_screen::scrollbar_shown(total, view),
        "{total} px of tape in a {view} px view needs a bar"
    );
    // And deepening the zoom with the toolbar's + keeps it shown: the tape only gets longer.
    let zoom_in = find_named(&window, "zoom-in-button").expect("the transport row carries a zoom + button");
    for _ in 0..3 {
        zoom_in.emit_by_name::<()>("clicked", &[]);
        settle();
        let (total, view) = extents(&window);
        assert!(cut_screen::scrollbar_shown(total, view), "a deeper zoom never hides the bar");
    }
    let bar = bar(&window);
    assert!(bar.is_visible(), "the bar showed itself when the session outgrew the view");
    // The adjustment is what a drag on the bar reads, so it must have room left to move in — upper is the
    // tape's length, page_size the part of it already on screen.
    let adj = bar.adjustment();
    assert!(adj.upper() > adj.page_size(), "{adj:?} should have room left to scroll");
    // §A's grab rule: "thumb geared at high zoom so one screenful is always 40 px of drag". The gear is a
    // number about the BAR, and `bar_width` falls back to the width the bar was last refreshed with while
    // GTK still reports 0 for an un-laid-out bar — which is exactly the state this headless window is in.
    // So the length the PAINT used is the answer, read through the same door it reads (`gear_px_for_test`), not this
    // widget's own allocation: one refresh, one width, both sides of every identity below.
    let bar_px = ui::cut_view::gear_px_for_test();
    assert!(
        (ui::cut_view::thumb_px(total, bar_px) - cut_screen::SCROLLBAR_DRAG_PX).abs() <= 1.0,
        "a {bar_px} px bar gears its thumb to {} px, not {}",
        ui::cut_view::thumb_px(total, bar_px),
        cut_screen::SCROLLBAR_DRAG_PX
    );
    // And the adjustment carries that gear as its page size: `paint_scrollbar` sets page = view × fraction,
    // so the thumb's fraction of the range is the geared fraction, whatever the session's length is. Every
    // number here comes from the ONE refresh whose adjustment we are reading.
    let fraction = adj.page_size() / adj.upper();
    assert!(
        (fraction - ui::cut_view::thumb_geared(adj.upper(), bar_px) * (view / adj.upper())).abs() < 1e-6,
        "the bar's page size is the geared slice of the view: {fraction}"
    );
    RAN_SHOWN.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// S4/S5: scrolling moves the view and nothing else. A selection made first stays where it was, and the
/// red line keeps its own time — the bar changes what you are looking at, never what the cut is.
fn check_a_drag_scrolls_and_holds_the_selection(app: &adw::Application) {
    let window = cut_page_taped(app, &long_tape(), &three_scenes());
    let band = ui::draw_selection(
        &window,
        naivepost::cut_select::Surface::PictureRow(0),
        None,
        10.0,
        40.0,
    )
    .expect("a drag on a picture row selects that row's footage");
    let before = ui::selection(&window).expect("the band is on the page");
    assert_eq!(before.start, band.start);

    let (total, view) = extents(&window);
    let from = ui::cut_view::origin();
    // What the bar's `value-changed` wire forwards to: the same door the wheel and a drag anywhere use.
    let moved = ui::cut_view::scroll_by(total, view, 120.0);
    assert!(moved > from, "a scroll asks for a positive offset: {from} -> {moved}");
    assert!(moved <= (total - view).max(0.0) + 0.01, "and never past the end of the tape: {moved} of {}", total - view);
    settle();

    let after = ui::selection(&window).expect("the selection survives the scroll");
    assert_eq!((after.start, after.end), (before.start, before.end), "a scroll must not move the selection");
    // The red line keeps its own time: `follow_playhead` moves the VIEW to it, never the other way round,
    // so a pixel scroll cannot change what second the line sits on.
    let before_line = ui::line_position(&window);
    ui::cut_view::follow_playhead(&window, 20.0);
    settle();
    assert_eq!(ui::line_position(&window), before_line, "following the line scrolls the view, it does not move the line");
    RAN_DRAG.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 hidden while everything fits");
        check_the_bar_is_hidden_while_everything_fits(app);
        println!("CHECK 2 shown once the session outgrows the view");
        check_the_bar_appears_when_the_session_stops_fitting(app);
        println!("CHECK 3 a drag scrolls the view and holds the selection");
        check_a_drag_scrolls_and_holds_the_selection(app);
    });
}

#[test]
fn sec_05_cut_1_screen_widgets_the_scrollbar_shows_only_when_it_is_needed() {
    round();
    for (flag, what) in [
        (&RAN_HIDDEN, "the hidden-while-it-fits check"),
        (&RAN_SHOWN, "the shown-when-it-does-not check"),
        (&RAN_DRAG, "the drag-scrolls check"),
    ] {
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "{what} never ran");
    }
}
