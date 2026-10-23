// F3.8 Hold, move, resize, edit, remove — THE BAR'S OWN GESTURE. `tests/suite/band_hold_widgets.rs` fires the seams
// directly and proves every rule they apply; this file proves the effects-lane bar actually HAS a press: a real
// `GtkGestureDrag` attached by `refresh_effects_lane`, fired the way GTK fires it, reaching those same seams.
// Before this round `press_effect_bar` and `drag_held_band` had no caller at all, so nothing on the page could
// pick a band up.
//
// One application, one `connect_activate`, exactly one `Application::run`: `g_application_run` refuses a second
// claimant of the default main context, so all four checks live inside that single activate and each drops its
// window before the next is built. cwd is pinned to our own temp root BEFORE the run because the presses resolve
// the project through `startup::session_dir(current_dir())` and the release writes `cut.json` there. No sleeps:
// `settle()` pumps the glib context instead.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::fx_band;
use naivepost::layout::Tree;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, widget_in};

static RAN_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_MOVE: AtomicBool = AtomicBool::new(false);
static RAN_GRIP_SNAP: AtomicBool = AtomicBool::new(false);
static RAN_ONE_UNDO: AtomicBool = AtomicBool::new(false);

/// A wide zoom: 10 s at the page's opening zoom (`cut_screen::ZOOM_AT_OPEN`, 4 px/s) is 40 px, past BOTH
/// `fx_band::KILL_MIN_PX` (32) and `GRIP_MIN_PX` (30), so its ends are grips and its middle slides.
fn wide_zoom(t: f64) -> Fx {
    Fx {
        kind: "zoom".to_string(),
        t,
        dur: 10.0,
        ..Default::default()
    }
}

/// The tape: one wide zoom at 20 s on a timeline long enough to slide it in, so the clamp never answers for
/// the drag. Long enough because `fx_band::move_whole` clamps the start to `session_end - dur` and
/// `shell::cut_seconds` sums segment lengths: a 30 s cut would leave a band at 20 s with nowhere to go.
fn one_record_with_a_mark() -> Cut {
    let mut cut_ = Cut::default();
    cut_.segs.push(Seg {
        s: 0.0,
        e: 120.0,
        ..Default::default()
    });
    cut_.fx.push(wide_zoom(20.0));
    cut_
}

/// Build a window sitting on the Cut tab with the given cut seeded, dropping the previous check's window first.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    // The lane's bars — and the gesture each one carries — exist only after a refresh, which is what every real
    // page path does. Without this there is no `fx-bar-0` for a press to land on.
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// This window's bar gesture, fired the way GTK fires a `GtkGestureDrag`: `drag-begin` carries the ABSOLUTE
/// press and `drag-update`/`drag-end` carry the OFFSET from it (the idiom `cut_trim_move_widgets.rs` uses).
struct BarGesture {
    gesture: gtk::GestureDrag,
    /// Where the bar sits in the page's pixel space: the press must be inside it, because the gesture reads the
    /// bar's drawn width off the widget and the hand's x within the bar decides grip-versus-middle.
    bar_x: f64,
    width: f64,
}

fn bar_gesture(window: &adw::ApplicationWindow) -> BarGesture {
    let bar = widget_in(window, "fx-bar-0").expect("the lane drew bar 0");
    let width = f64::from(bar.size_request().0).max(1.0);
    // The gesture measures the press against the bar's OWN drawn width (the widget's size request, floored at
    // 48 px), so the test asks the widget rather than assuming a pps: `grab_at` must see the same number.
    // Found on the WIDGET, not out of a global list: an earlier check's closed window still has its bars in the
    // tree, and its controller must not answer this check's drag.
    let gesture = ui::bar_gesture_for_test(window, 0).expect("the lane wired a gesture for bar 0");
    // Any absolute x inside the bar works: the seam measures the press against the bar's own width, and the
    // page's scroll is whatever the placeholder strip happens to use.
    BarGesture {
        gesture,
        bar_x: width / 2.0,
        width,
    }
}

/// Press in the bar's MIDDLE and travel `dx` px, then release.
fn drag_middle(bar: &BarGesture, dx: f64) {
    // All three signals in one go, then one settle: see `drag_end_of` for why a settle between them loses it.
    bar.gesture
        .emit_by_name::<()>("drag-begin", &[&bar.bar_x, &0.0f64]);
    bar.gesture
        .emit_by_name::<()>("drag-update", &[&dx, &0.0f64]);
    bar.gesture.emit_by_name::<()>("drag-end", &[&dx, &0.0f64]);
    settle();
}

/// Press on ONE END (within `cut_select::GRIP_PX` of it) and travel `dx` px, then release.
fn drag_end_of(bar: &BarGesture, leading: bool, dx: f64) {
    // The press x is taken from the bar's OWN drawn width so `fx_band::grab_at` sees an end. One pixel inside
    // the edge rather than exactly on it: a begin at 0.0 would be indistinguishable from "no press recorded".
    let x = if leading { 1.0 } else { bar.width - 1.0 };
    // `settle()` between the three signals is what BREAKS a hand-off press: GTK's gesture state machine has
    // no event sequence behind an emitted signal, so letting the main context run between begin and update
    // loses the hold. Fire all three in one go, then settle once — the way `snapshot.rs` drives its gestures.
    bar.gesture.emit_by_name::<()>("drag-begin", &[&x, &0.0f64]);
    bar.gesture
        .emit_by_name::<()>("drag-update", &[&dx, &0.0f64]);
    bar.gesture.emit_by_name::<()>("drag-end", &[&dx, &0.0f64]);
    settle();
}

/// The session folder the release writes into: the test cwd is pinned to a temp root, and the working copy is
/// `<root>/session.naivepost`.
fn session_tree() -> Tree {
    let root = std::env::current_dir().expect("cwd is pinned");
    Tree::new(naivepost::startup::session_dir(&root))
        .expect("the working copy is a .naivepost folder")
}

// --- the round ----------------------------------------------------------------------------------------

fn gesture_round(app: &adw::Application) {
    // --- (1) S1: a press under 4 px of travel is a CLICK: it opens the form and moves nothing -------
    let window = cut_page(app, &one_record_with_a_mark());
    let bar = bar_gesture(&window);
    bar.gesture
        .emit_by_name::<()>("drag-begin", &[&bar.bar_x, &0.0f64]);
    settle();
    bar.gesture
        .emit_by_name::<()>("drag-update", &[&2.0, &0.0f64]);
    settle();
    bar.gesture.emit_by_name::<()>("drag-end", &[&2.0, &0.0f64]);
    settle();
    assert!(
        ui::zoom_form_open().is_some(),
        "a 2 px press on the bar opened the ZOOM form \u{2014} the click reached `open_effect_form`"
    );
    assert!(
        ui::held_effect().is_none(),
        "a click never picks the band up"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx[0].t,
        20.0,
        "a click wrote nothing to the record"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx[0].dur,
        10.0,
        "nor changed its length"
    );
    RAN_CLICK.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) S1/S2/S3: a real drag picks the band up, slides it, and says so -----------------------
    let window = cut_page(app, &one_record_with_a_mark());
    let pps = ui::SELECT_SURFACE_PPS;
    let bar = bar_gesture(&window);
    drag_middle(&bar, 8.0);
    let moved = &ui::review_cut_of(&window).fx[0];
    assert_eq!(
        moved.t,
        20.0 + 8.0 / pps,
        "the band followed the pointer: 8 px at {pps} px/s is 2 s"
    );
    assert_eq!(moved.dur, 10.0, "a slide never changes the length");
    assert_eq!(
        status_text(&window),
        fx_band::moved("Zoom"),
        "the ending the spec promises printed FROM THE PAGE"
    );
    assert!(
        ui::held_effect().is_some(),
        "S2: the hold outlives the release \u{2014} it drops when the line walks off or Esc says so"
    );
    // The line rode with the band (otherwise the refresh would have read the old line against the new
    // position and dropped the hold as "walked off the band").
    assert_eq!(
        ui::line_position(&window).t,
        moved.t,
        "the line sits on the band"
    );
    // §7's one writer: the RELEASE is where `cut.json` gets the answer, so the disk agrees with the page.
    let on_disk = naivepost::cut::load(&session_tree()).expect("cut.json was written");
    assert_eq!(
        on_disk.fx[0].t, moved.t,
        "the release wrote the moved second to cut.json"
    );
    RAN_MOVE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) S1/S2: an END grip resizes, and both ends get snapped within 8 px ---------------------
    let window = cut_page(app, &one_record_with_a_mark());
    // A hand left full by the previous check would be put DOWN by this press instead of filling (§05#7: one
    // thing held at a time), so the page starts this check with an empty hand and lets its own press fill it.
    ui::set_held_effect(None);
    settle();
    let bar = bar_gesture(&window);
    // Drag the LEADING end 5 px right: the second under the hand is the bar's own left edge (20 s) plus the
    // press x inside the bar (1 px = 0.25 s) plus the travel (1.25 s), so 21.5 s — but the segment border at
    // 24 s is only 2.5 s (10 px) away and the playhead the pick-up just placed sits exactly on the band's old
    // start, INSIDE the 8 px reach, so the snapped answer is that landmark rather than the raw pixel. What
    // must hold either way is S2's rule: an end grip changes the LENGTH and leaves the untouched stop alone.
    drag_end_of(&bar, true, 5.0);
    let resized = &ui::review_cut_of(&window).fx[0];
    assert_eq!(
        status_text(&window),
        fx_band::resized("Zoom"),
        "an end grip reports 'resized', not 'moved'"
    );
    assert!(
        resized.dur < 10.0,
        "the grabbed end shortened the band from its 10 s start, got {}",
        resized.dur
    );
    assert!(
        resized.t >= 20.0 && resized.t + resized.dur <= 30.0,
        "the resize stayed inside the band it started from: t={} dur={}",
        resized.t,
        resized.dur
    );
    assert!(
        resized.dur >= naivepost::fx_lane::MIN_BAND_SECONDS,
        "length stopped at the floor, got {}",
        resized.dur
    );
    assert!(
        resized.dur >= naivepost::fx_lane::MIN_BAND_SECONDS,
        "length stopped at the floor, got {}",
        resized.dur
    );
    // Snap: a mark INSIDE the reach pulls the slid band onto it. Two segments give a timeline long enough to
    // slide in AND a landmark exactly inside the reach: the border at 24 s is 2 s from where the hand lands
    // (8 px at 4 px/s), which is exactly `cut_select::SNAP_PX`, so the band lands ON the segment end.
    let mut snapped_cut = Cut::default();
    snapped_cut.segs.push(Seg {
        s: 0.0,
        e: 24.0,
        ..Default::default()
    });
    snapped_cut.segs.push(Seg {
        s: 24.0,
        e: 120.0,
        ..Default::default()
    });
    snapped_cut.fx.push(wide_zoom(20.0));
    let window = cut_page(app, &snapped_cut);
    let bar = bar_gesture(&window);
    drag_middle(&bar, 8.0);
    let landed = &ui::review_cut_of(&window).fx[0];
    assert_eq!(
        landed.t,
        24.0,
        "both ends are offered to snap marks within 8 px \u{2014} the band landed ON the segment end"
    );
    RAN_GRIP_SNAP.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (4) S3: many ticks of ONE drag cost ONE Undo step -----------------------------------------
    let window = cut_page(app, &one_record_with_a_mark());
    let bar = bar_gesture(&window);
    bar.gesture
        .emit_by_name::<()>("drag-begin", &[&bar.bar_x, &0.0f64]);
    settle();
    for tick in 1..=8 {
        bar.gesture
            .emit_by_name::<()>("drag-update", &[&(tick as f64 * 4.0), &0.0f64]);
        settle();
    }
    bar.gesture
        .emit_by_name::<()>("drag-end", &[&32.0, &0.0f64]);
    settle();
    let dragged = ui::review_cut_of(&window).fx[0].t;
    assert!(
        dragged > 20.0,
        "the eight-tick drag moved the band, got {dragged}"
    );
    // ONE ↶ takes the WHOLE gesture back: if any mid-drag tick had spent a history entry of its own, undoing
    // once would land on some intermediate second instead of the one the hand started from.
    ui::press_undo(&window);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx[0].t,
        20.0,
        "one Undo undid all eight ticks \u{2014} the drag owned a single step"
    );
    RAN_ONE_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn f3_8_s1_s2_s3_the_lane_bar_carries_a_real_press() {
    let root = std::env::temp_dir().join(format!("np-f38wire-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(gesture_round);
        app.run_with_args::<&str>(&[]);
    });

    assert!(
        RAN_CLICK.load(Ordering::SeqCst),
        "S1 click-through-gesture never ran"
    );
    assert!(
        RAN_MOVE.load(Ordering::SeqCst),
        "S2 move-through-gesture never ran"
    );
    assert!(
        RAN_GRIP_SNAP.load(Ordering::SeqCst),
        "S2 grip-resize-and-snap never ran"
    );
    assert!(
        RAN_ONE_UNDO.load(Ordering::SeqCst),
        "S3 one-undo-per-drag never ran"
    );
}
