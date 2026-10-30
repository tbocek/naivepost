//! §05-cut.md §1 badge **31** — the scrollbar and the zoom it belongs to.
//!
//! The bar's whole job is a proportion: how much of the session is on screen, and how far a thumb must
//! travel to cross a screenful. So these tests read numbers out of `cut_view` rather than pixels: the
//! gear (§1: "thumb geared at high zoom so one screenful is always 40 px of drag"), the clamp that keeps
//! a scroll inside the timeline, when the bar hides itself because everything already fits, and where
//! the − / + ladder stops — which is what the session fits into, not the constant the placeholder strip
//! was sized with.

use crate::common;
use adw::prelude::*;
use naivepost::timeline::{self, Recording};
use naivepost::ui::cut_view;
use std::sync::atomic::Ordering;

/// The one app round the widget case needs: GTK will not build a window off the main thread, and tests run
/// on their own, so exactly one `adw::Application` emission exists for the whole binary.
static WHEEL_ROUND: std::sync::Once = std::sync::Once::new();

/// A session as the page sees it: one take from 0 to `seconds`.
fn tape(seconds: f64) -> Vec<Recording> {
    vec![Recording {
        base: "lecture".to_string(),
        start: 0.0,
        end: seconds,
    }]
}

#[test]
fn sec_05_cut_1_screen_s9_the_scrollbar_thumb_is_geared_at_forty_px_per_screenful() {
    common::pin_private_cwd();
    // §1: "thumb geared at high zoom so one screenful is always 40 px of drag". The thumb is the view's share
    // of the tape, as on any bar; GTK's own drag moves `total / bar` tape px per px, and once that would cross
    // a screenful in under 40 px the drag is geared to `view / 40` per px instead.
    let view = 1200.0 - cut_view::gutter_px();
    let bar = 1200.0;
    for (seconds, pps) in [
        (600.0, naivepost::cut_screen::ZOOM_MAX),
        (7200.0, naivepost::cut_screen::ZOOM_MAX),
        (7200.0, 12.0),
    ] {
        let total = cut_view::total_px(seconds, pps);
        let per_px = cut_view::drag_gear(total, view, bar).unwrap_or_else(|| {
            panic!("{seconds} s at {pps} px/s is high zoom: the drag is geared")
        });
        assert!(
            (view / per_px - cut_view::DRAG_PER_SCREENFUL_PX).abs() < 1e-9,
            "{seconds} s at {pps} px/s: one screenful costs {} px of drag, not 40",
            view / per_px
        );
    }
    // A short session at the open zoom: GTK's own drag already costs more than 40 px a screenful, so the
    // bar is left to GTK.
    let total = cut_view::total_px(600.0, cut_screen_open());
    assert_eq!(
        cut_view::drag_gear(total, view, bar),
        None,
        "{total} px of tape on a {bar} px bar needs no gear"
    );
    // The gear only bites once there IS more off screen: at the floor a whole session fits, so the bar
    // hides itself (§A) and its thumb fills the bar — which is the same rule read from the other side.
    let view_only = 1200.0;
    let floor_pps = timeline::floor_pps(&timeline::filmed_runs(&tape(600.0)), view_only);
    // The floor is defined on the VIEW (`cut_screen::fit_pps(filmed, view_px)`), and `total_px` adds the
    // pinned gutter back on — so at the floor the whole timeline is exactly one surface wide: filmed px
    // plus the gutter that always sits in front of it.
    let floor_total = cut_view::total_px(600.0, floor_pps);
    assert!(
        (floor_total - (view_only + cut_view::gutter_px())).abs() < 1e-9,
        "at {floor_pps} px/s a ten-minute session is one surface of film plus the gutter: {floor_total}"
    );
    // And at the floor the film exactly fills the view, so the session fits and the bar hides (§A: "hidden
    // when everything fits"). The first rewrite weighed the gutter on one side only and left the bar up here.
    assert!(
        cut_view::fits(floor_total, view_only),
        "at the floor the whole session fits: {floor_total} px of tape (gutter included) over {view_only}"
    );
}

#[test]
fn sec_05_cut_1_screen_s9_the_scroll_hides_itself_when_the_whole_session_fits() {
    common::pin_private_cwd();
    // §A: "hidden when everything fits" — a full-width thumb claims there is more to the right.
    let view = 1200.0 - cut_view::gutter_px();
    let short = cut_view::total_px(60.0, 4.0);
    assert!(
        short <= view,
        "a 60 s session at the open zoom does fit: {short} <= {view}"
    );
    assert!(cut_view::fits(short, view), "so the bar hides");

    // The same session at the deepest zoom no longer fits, and the bar comes back.
    let deep = cut_view::total_px(60.0, naivepost::cut_screen::ZOOM_MAX);
    assert!(
        !cut_view::fits(deep, view),
        "240 px/s over a minute is {deep} px of timeline"
    );
}

#[test]
fn sec_05_cut_1_screen_s9_a_scroll_stays_inside_the_timeline_at_both_ends() {
    common::pin_private_cwd();
    let view = 1200.0 - cut_view::gutter_px();
    let total = cut_view::total_px(600.0, 40.0);
    // Past the right end: the last second's pixel is as far as the hand gets. `total` counts the pinned gutter
    // in front of the film and `view` does not, so the film alone is what scrolls past the view.
    assert_eq!(
        cut_view::set_origin(total * 3.0, total, view),
        total - cut_view::gutter_px() - view
    );
    // Left of the gutter: it never had room to go there.
    assert_eq!(cut_view::set_origin(-500.0, total, view), 0.0);
    // A pan is an eighth of the view, wherever it starts.
    assert_eq!(cut_view::pan_eighth(view), view / 8.0);
    assert_eq!(
        cut_view::scroll_by(total, view, cut_view::pan_eighth(view)),
        view / 8.0
    );
    // An empty timeline has nowhere to scroll: the origin stays put at zero.
    assert_eq!(cut_view::set_origin(50.0, 100.0, view), 0.0);
}

#[test]
fn sec_05_cut_1_screen_s9_zooming_out_stops_where_the_session_fits_not_at_a_constant() {
    common::pin_private_cwd();
    // Badge 22's tooltip: "it stops where the whole session is on screen". That number depends on how
    // long THIS session is, which is why the page asks `timeline::floor_pps` over the filmed runs rather
    // than handing the ladder `TRACK_STRIP_PPS` (4.0 whatever the session).
    let view = 1200.0 - cut_view::gutter_px();
    let runs = timeline::filmed_runs(&tape(600.0));
    let floor = timeline::floor_pps(&runs, view);
    assert_ne!(
        floor,
        naivepost::cut_screen::ZOOM_AT_OPEN,
        "a ten-minute session across this view floors well below the open zoom — a constant would stop too early"
    );
    cut_view::set_pps(naivepost::cut_screen::ZOOM_MAX);
    let mut pps = cut_view::pps();
    for _ in 0..200 {
        let (next, _) = cut_view::zoom_step(false, &runs, cut_view::total_px(600.0, pps), view);
        pps = next;
    }
    assert_eq!(
        pps, floor,
        "the ladder stops at 'all of it fits' and no further"
    );
    // One step back in is possible from there: the floor is a stop, not a lock.
    let (one_in, _) = cut_view::zoom_step(true, &runs, cut_view::total_px(600.0, pps), view);
    assert!(
        one_in > floor,
        "+ off the floor still moves: {one_in} > {floor}"
    );
}

#[test]
fn sec_05_cut_1_screen_s9_zooming_around_the_cursor_keeps_that_second_under_it() {
    common::pin_private_cwd();
    // §A: the wheel zooms "around the cursor". The second under the pointer must stay put, or every
    // zoom is a hunt for the moment you were looking at.
    let view = 1200.0 - cut_view::gutter_px();
    let runs = timeline::filmed_runs(&tape(600.0));
    cut_view::set_pps(4.0);
    cut_view::set_origin(0.0, cut_view::total_px(600.0, 4.0), view);
    let anchor = 400.0 + cut_view::gutter_px();
    // The zoom's promise is about the PIXEL under the pointer: the second it holds stays under it, so a
    // notch in at 400 px from the start scrolls the tape left by what the zoom grew under the pointer. (The
    // first rewrite asserted the opposite: that a cursor inside the view scrolled nothing, which is the
    // reversed scroll sign that jumped every wheel zoom back to 0:00.)
    let total = cut_view::total_px(600.0, cut_view::pps());
    let second_under = |pps: f64| (anchor - cut_view::gutter_px() + cut_view::origin()) / pps;
    let before = second_under(cut_view::pps());
    let (pps_after, _) = cut_view::zoom_around(anchor, 1.25, &runs, total, view);
    assert!(
        pps_after > 4.0,
        "a notch in raised the zoom: {} -> {pps_after}",
        cut_view::pps()
    );
    assert!(
        (second_under(pps_after) - before).abs() < 1e-6,
        "and the second under the cursor stayed there: {before} -> {}",
        second_under(pps_after)
    );
    // Same gesture one step deeper, from a scrolled view: the pixel and its second both hold.
    cut_view::set_pps(40.0);
    cut_view::set_origin(2000.0, cut_view::total_px(600.0, 40.0), view);
    let deep_total = cut_view::total_px(600.0, cut_view::pps());
    let before = second_under(cut_view::pps());
    let (deeper, _) = cut_view::zoom_around(anchor, 1.25, &runs, deep_total, view);
    let after = second_under(deeper);
    assert!(
        (before - after).abs() < 1e-6,
        "the second under the cursor moved: {before} -> {after}"
    );
}

#[test]
fn sec_05_cut_1_screen_s9_the_view_only_slips_to_follow_a_line_that_left_it() {
    common::pin_private_cwd();
    // The view stands still up to three quarters of its width, then holds the line there.
    let view = 1000.0;
    let total = 8000.0;
    cut_view::set_origin(0.0, total, view);
    assert_eq!(
        cut_view::reveal_playhead(750.0, total, view),
        0.0,
        "a line up to three quarters does not move the view"
    );
    assert_eq!(
        cut_view::reveal_playhead(760.0, total, view),
        10.0,
        "past three quarters the view moves just enough to hold the line there"
    );
    assert_eq!(
        cut_view::reveal_playhead(765.0, total, view),
        15.0,
        "and keeps holding it"
    );
    assert_eq!(
        cut_view::reveal_playhead(400.0, total, view),
        15.0,
        "a line moving back inside the view does not move it"
    );
    let furthest = cut_view::set_origin(total, total, view);
    assert_eq!(
        cut_view::reveal_playhead(total - 10.0, total, view),
        furthest,
        "at the tape's end the line runs on into the last quarter"
    );
    cut_view::set_origin(0.0, total, view);
    let slipped = cut_view::reveal_playhead(5000.0, total, view);
    assert!(
        slipped > 0.0,
        "a line far past the right edge pulls the view along"
    );
    assert!(
        slipped <= 5000.0 && slipped + view >= 5000.0,
        "and it lands so the line is inside again: {slipped}"
    );
}

/// A wheel zoom never pushes a line that was on screen off it, and moves the view no more than that takes.
#[test]
fn sec_05_cut_1_screen_s9_a_wheel_zoom_pulls_the_line_back_on_screen() {
    common::pin_private_cwd();
    let (view, total) = (1000.0, 8000.0);
    cut_view::set_origin(2000.0, total, view);
    assert_eq!(cut_view::pull_line_on_screen(2500.0, total, view), 2000.0, "still inside: the view stays");
    assert_eq!(cut_view::pull_line_on_screen(1500.0, total, view), 1500.0, "off the left: to the left edge");
    assert_eq!(cut_view::pull_line_on_screen(3000.0, total, view), 2000.0, "off the right: to the right edge");
}

/// §1's three wheel gestures through the live door `apply_wheel` — the same function `attach_wheel`'s
/// controllers call. Zoom over the tracks, pan with Shift, and a step over the transport that moves no view
/// state and says nothing here. It needs a real window for the sentence and the repaint, so it rides the one
/// activate round this binary owns (the shape `tests/suite/cut_view_wheel.rs` uses).
#[test]
fn sec_05_cut_1_screen_s9_a_wheel_over_the_tracks_zooms_or_pans_and_says_so() {
    common::pin_private_cwd();
    static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    WHEEL_ROUND.call_once(|| {
        let app = adw::Application::builder()
            .application_id(naivepost::ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            wheel_over_tracks_and_transport(app);
            RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        let _ = app.run_with_args::<String>(&[]);
    });
    assert!(RAN.load(Ordering::SeqCst), "the wheel round ran");
}

fn wheel_over_tracks_and_transport(app: &adw::Application) {
    use naivepost::cut_screen::WheelOver;
    use naivepost::shell::Page;
    let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
    let window = naivepost::ui::build_window(app, &model, "Prepare");
    window.present();
    naivepost::ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    naivepost::ui::set_session_recordings(&tape(600.0));
    common::settle();
    cut_view::set_pps(8.0);
    let view = cut_view::view_px(cut_view::surface_width(&window));
    cut_view::set_origin(0.0, cut_view::total_px(600.0, 8.0), view);

    // A notch over the tracks zooms: the page's zoom moved and the answer names the new number.
    let said = cut_view::apply_wheel(&window, WheelOver::Tracks, false, -1.0, 0.0, |_| {})
        .expect("a notch over the tracks is a zoom, and a zoom has something to say");
    assert!(said.contains("px a second"), "{said}");
    assert!(
        cut_view::pps() > 8.0,
        "a notch away zoomed in: {}",
        cut_view::pps()
    );

    // Shift over the same spot pans an eighth instead, and moves the scroll.
    let before = cut_view::origin();
    cut_view::apply_wheel(&window, WheelOver::Tracks, true, 1.0, 0.0, |_| {})
        .expect("a pan has a sentence too");
    assert_ne!(cut_view::origin(), before, "Shift+wheel panned the view");

    // Over the transport the SAME gesture steps frames through the caller's seam: no view state moves and
    // there is no sentence, because the transport owns what a step did.
    let stepped = std::cell::Cell::new(0i64);
    let pps_now = cut_view::pps();
    let origin_now = cut_view::origin();
    assert_eq!(
        cut_view::apply_wheel(&window, WheelOver::Transport, false, -1.0, 0.0, |frames| {
            stepped.set(frames);
        }),
        None,
        "a notch over the transport is nobody's zoom"
    );
    assert_ne!(
        stepped.get(),
        0,
        "and the caller's transport seam got the step"
    );
    assert_eq!(cut_view::pps(), pps_now, "it changed no zoom");
    assert_eq!(cut_view::origin(), origin_now, "and no scroll");
    window.close();
}

fn cut_screen_open() -> f64 {
    naivepost::cut_screen::ZOOM_AT_OPEN
}

/// The view follows the line where it is drawn: the time between two recordings and a folded stretch take no width,
/// so a session with gaps must not send the view past the line.
#[test]
fn the_view_follows_the_line_through_gaps_and_folds() {
    let runs = [(0.0, 100.0), (300.0, 400.0)];
    assert_eq!(cut_view::line_px(&runs, &[], 2.0, 50.0), 100.0);
    assert_eq!(cut_view::line_px(&runs, &[], 2.0, 350.0), 300.0, "the gap takes no width");
    assert_eq!(cut_view::line_px(&runs, &[[10.0, 60.0]], 2.0, 350.0), 200.0, "nor does a fold");
}
