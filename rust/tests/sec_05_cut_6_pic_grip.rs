//! §05-cut#6-parameters-used — the fourth pixel reach, `spec/05-cut.md` §6's "pixel reaches (6, 8, 10, 12 px)".
//!
//! Three of §6's four reaches had rows in `params::cut()` (`layout.edgeGrabPx` 6, `layout.snapPx` 8,
//! `layout.lineReachPx` 12) and no rule read a 10 px one: the tree grabbed a selection end at 6 px on every
//! surface. The prototype distinguishes them — `gui/cut_selband.go` names `selGripPx = 6` and
//! `selGripPicsPx = 10` ("on the pictures the reach is wider"), which is `spec/inventory/cut.md` §I's press
//! order line "selection ends on the pictures within 10 px". So this round implements the reach rather than
//! merely cataloguing a number nothing reads: `cut_select::PIC_GRIP_PX` + `cut_select::grip_px(surface)` are
//! what `Geometry::part_of_on` asks for every press, and `layout.picGripPx` points at the constant.
//!
//! Shape: one application, one `connect_activate`, one `#[test]` per check (the shape
//! `tests/f2_6_painted_select.rs` uses), because the selection slot is process-shared state. The widget check
//! fires the painted band's own `gtk::GestureDrag` through `emit_by_name`, so what is proven is that the PAGE
//! uses the wide grip, not only that a pure function can compute it.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::cut_select::{self, Part, Scope, Surface};
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_select_press::Geometry;
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle};

static RAN_ROWS: AtomicBool = AtomicBool::new(false);
static RAN_REACH: AtomicBool = AtomicBool::new(false);
static RAN_WIRE: AtomicBool = AtomicBool::new(false);

/// One row by its §6 id.
fn row(id: &str) -> naivepost::params::Param {
    naivepost::params::cut()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| panic!("{id} is one of §6's pixel reaches and so must be catalogued"))
}

/// A cut with one row of footage to press on, as long as the F2.6 band test's.
fn one_cam_cut() -> Cut {
    Cut { segs: vec![Seg { s: 0.0, e: 70.0, cam: 0, ..Default::default() }], ..Default::default() }
}

/// §6's four pixel reaches, each with the value §6 writes and the module whose constant holds it. This is the
/// row the completion check found missing: 10 px existed nowhere.
fn check_the_four_pixel_reaches_are_all_catalogued() {
    let expected: [(&str, f64, &str); 4] = [
        ("layout.edgeGrabPx", 6.0, "cut_trim::EDGE_GRAB_PX"),
        ("layout.snapPx", 8.0, "cut_select::SNAP_PX"),
        ("layout.picGripPx", 10.0, "cut_select::PIC_GRIP_PX"),
        ("layout.lineReachPx", 12.0, "cut_line::EDGE_REACH_PX"),
    ];
    for (id, value, home) in expected {
        let listed = row(id);
        let spelled: f64 = listed
            .spelled
            .parse()
            .unwrap_or_else(|_| panic!("{id} spells a number, got {:?}", listed.spelled));
        assert_eq!(spelled, value, "{id} carries §6's number");
        assert_eq!(listed.from, home, "{id} names the constant its rule reads");
    }
    // And the constants themselves, so a row cannot disagree with the code it points at.
    assert_eq!(naivepost::cut_select::PIC_GRIP_PX, 10.0);
    assert_eq!(naivepost::cut_select::GRIP_PX, 6.0);
    RAN_ROWS.store(true, Ordering::SeqCst);
}

/// The rule: 10 px on a picture row where the band's own strip stays at 6, plus the thin-band case holding at
/// the wider reach too.
fn check_a_picture_grab_reaches_10_px_where_the_band_reaches_6() {
    // A band drawn from 100 px to 400 px.
    let (from_px, to_px) = (100.0, 400.0);
    // 8 px inside the left end: a hand on the PICTURES finds it, a hand in the selection band does not.
    assert_eq!(
        cut_select::part_at_reach(108.0, from_px, to_px, cut_select::PIC_GRIP_PX),
        Part::Start,
        "8 px off an end grabs it on a picture row (§I's 10 px)"
    );
    assert_eq!(
        cut_select::part_at_reach(108.0, from_px, to_px, cut_select::GRIP_PX),
        Part::Middle,
        "and misses it at the band's 6 px, where the middle takes the press instead"
    );
    // Both agree within the narrower reach, and neither reaches past the wider one. Presses taken from INSIDE
    // the band: a point outside it is `Outside` at any reach (there is no end there to grab), which is what
    // makes the selection-band counter-case below a fresh draw rather than a resize.
    assert_eq!(cut_select::part_at_reach(105.0, from_px, to_px, cut_select::PIC_GRIP_PX), Part::Start);
    assert_eq!(cut_select::part_at_reach(105.0, from_px, to_px, cut_select::GRIP_PX), Part::Start);
    assert_eq!(cut_select::part_at_reach(112.0, from_px, to_px, cut_select::PIC_GRIP_PX), Part::Middle);
    assert_eq!(cut_select::part_at_reach(112.0, from_px, to_px, cut_select::GRIP_PX), Part::Middle);
    // The right end mirrors it, measured from inside: 6 px off it is within BOTH reaches (the narrow one is
    // "within 6", inclusive), so the split on this side sits at 8 px too.
    assert_eq!(cut_select::part_at_reach(394.0, from_px, to_px, cut_select::PIC_GRIP_PX), Part::End);
    assert_eq!(cut_select::part_at_reach(394.0, from_px, to_px, cut_select::GRIP_PX), Part::End);
    assert_eq!(
        cut_select::part_at_reach(392.0, from_px, to_px, cut_select::PIC_GRIP_PX),
        Part::End,
        "8 px off the right end is an end grab for the picture hand"
    );
    assert_eq!(
        cut_select::part_at_reach(392.0, from_px, to_px, cut_select::GRIP_PX),
        Part::Middle,
        "and still the middle for the band hand"
    );
    // And clear of the band by more than either reach, both say "draw something new". (A press just PAST an
    // end is an end grab — that is what `px >= from_px` / `px <= to_px` allow, and why a hand can catch an end
    // it overshot; the counter-case below uses a point well outside instead.)
    assert_eq!(
        cut_select::part_at_reach(to_px + 20.0, from_px, to_px, cut_select::PIC_GRIP_PX),
        Part::Outside,
        "20 px past the drawn end is beyond even the wide grip"
    );
    assert_eq!(
        cut_select::part_at_reach(to_px + 20.0, from_px, to_px, cut_select::GRIP_PX),
        Part::Outside
    );

    // Which surface earns which reach — the mapping the page asks, not a number typed at the call site.
    assert_eq!(cut_select::grip_px(Surface::PictureRow(0)), 10.0, "a picture row reaches 10 px");
    assert_eq!(cut_select::grip_px(Surface::PictureRow(3)), 10.0, "whichever row it is");
    assert_eq!(cut_select::grip_px(Surface::SelectionBand), 6.0, "the selection band stays at 6");
    // DECISION the spec leaves open: the wave strip under a picture row keeps the NARROW grip. §I widens the
    // reach for "the pictures" alone, and that strip is where F2.6's own band is drawn and aimed at, so the
    // end there is already a visible edge rather than something to find through thumbnails.
    assert_eq!(cut_select::grip_px(Surface::WaveStrip), 6.0, "the wave strip is aimed at, not through");
    assert_eq!(cut_select::grip_px(Surface::Ruler), 6.0);
    assert_eq!(cut_select::grip_px(Surface::Lane), 6.0);
    assert_eq!(cut_select::grip_px(Surface::FxLane), 6.0);

    // The thin-band rule survives the wider reach: a band narrower than two grips answers an END, because a
    // press can only mean one thing and resizing is what the hand was aiming at.
    assert_eq!(
        cut_select::part_at_reach(105.0, 100.0, 108.0, cut_select::PIC_GRIP_PX),
        Part::Start,
        "inside a 8 px-wide band at the wide grip: an end, not the middle"
    );
    // And `part_at` (no reach given) is still the narrow rule, which is what every older caller relies on.
    assert_eq!(cut_select::part_at(108.0, from_px, to_px), Part::Middle, "part_at kept its 6 px meaning");
    RAN_REACH.store(true, Ordering::SeqCst);
}

/// The wire: the painted band's own drag grabs a picture-row end 8 px off, and the same offset in the
/// selection band does not resize anything.
fn widget_check(app: &adw::Application) {
        let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
        let window = ui::build_window(app, &model, "Prepare");
        window.present();
        ui::tab_button(&window, Page::Cut)
            .expect("the shell has a Cut tab")
            .emit_clicked();
        settle();
        ui::seed_review_cut(&window, &one_cam_cut());
        let gesture = ui::band_selection_gesture_for_test(&window)
            .expect("the painted band carries a selection drag");
        let geo = Geometry::of(&window);
        println!(
            "GRIP pps={} origin={} gutter={} first_picture={} row_height={} wave={} rows={}",
            geo.pps, geo.origin, geo.gutter, geo.bands.first_picture, geo.bands.row_height,
            geo.bands.wave_height, geo.row_count
        );
        assert!(geo.pps > 0.0 && geo.row_count > 0, "the band is drawn with real rows to press on");
        let row_y = geo.bands.first_picture + geo.bands.row_height / 2.0;
        // Lay a band down first: draw from 10 s out to 40 s.
        gesture.emit_by_name::<()>("drag-begin", &[&geo.px_of(10.0), &row_y]);
        gesture.emit_by_name::<()>("drag-update", &[&geo.px_of(40.0), &row_y]);
        gesture.emit_by_name::<()>("drag-end", &[&geo.px_of(40.0), &row_y]);
        settle();
        let laid = ui::selection(&window).expect("the drag drew a band");
        println!("GRIP laid = {}..{}", laid.start, laid.end);

        // Press 8 px INSIDE the band's drawn end, ON THE PICTURE ROW, and move. At the old 6 px grip this
        // press was the band's middle and would have MOVED the whole band; at §I's 10 px it is the end and
        // resizes it, leaving the start where it was. That difference is the whole assertion.
        let end_px = geo.px_of(laid.end);
        let before_start = laid.start;
        gesture.emit_by_name::<()>("drag-begin", &[&(end_px - 8.0), &row_y]);
        gesture.emit_by_name::<()>("drag-update", &[&(end_px - 8.0 - 6.0 * geo.pps), &row_y]);
        settle();
        let resized = ui::selection(&window).expect("the band is still there after the end grab");
        println!("GRIP resized = {}..{} (was {}..{})", resized.start, resized.end, laid.start, laid.end);
        assert!(
            (resized.start - before_start).abs() < 0.5,
            "an end grab 8 px in resizes the band rather than sliding it: start {before_start} -> {}",
            resized.start
        );
        assert!(resized.end < laid.end, "and the end came inward: {} -> {}", laid.end, resized.end);
        assert!(
            matches!(resized.scope, Scope::Footage { .. }),
            "the band stayed scoped to the footage it was drawn on: {:?}",
            resized.scope
        );

        // The counter-case that separates the two reaches: the SAME 8 px offset on the SELECTION BAND's y.
        // There the narrow grip applies, so the press is outside an end and the page draws a NEW band from
        // where the hand landed instead of resizing the one on screen.
        ui::clear_selection(&window);
        gesture.emit_by_name::<()>("drag-begin", &[&geo.px_of(10.0), &row_y]);
        gesture.emit_by_name::<()>("drag-update", &[&geo.px_of(40.0), &row_y]);
        gesture.emit_by_name::<()>("drag-end", &[&geo.px_of(40.0), &row_y]);
        settle();
        let again = ui::selection(&window).expect("a second band laid down");
        let sel_y = (geo.bands.selection.0 + geo.bands.selection.1) / 2.0;
        let right_edge = geo.px_of(again.end);
        gesture.emit_by_name::<()>("drag-begin", &[&(right_edge - 8.0), &sel_y]);
        gesture.emit_by_name::<()>("drag-update", &[&(right_edge - 8.0 - 3.0 * geo.pps), &sel_y]);
        settle();
        let redrawn = ui::selection(&window).expect("the press left a band on screen");
        println!(
            "GRIP selection-band press: band {}..{} -> {}..{} (press x {})",
            again.start, again.end, redrawn.start, redrawn.end, right_edge - 8.0
        );
        // A resize would have LEFT the start at 10 s and pulled the end in; this press started a NEW band at
        // the second under the pointer instead, which is only what the narrow grip allows.
        assert!(
            redrawn.start > again.start + 1.0 && redrawn.end > again.end,
            "at the band's 6 px grip an 8 px-in press is NOT an end grab: the page drew a new band \n             {redrawn:?} instead of resizing {again:?}"
        );
        RAN_WIRE.store(true, Ordering::SeqCst);
        window.close();
}

fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let failures = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let log = failures.clone();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let log = log.clone();
        app.connect_activate(move |app| {
            for (name, check) in [
                ("rows", check_the_four_pixel_reaches_are_all_catalogued as fn()),
                ("reach", check_a_picture_grab_reaches_10_px_where_the_band_reaches_6),
            ] {
                if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
                    let why = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "a non-string panic".to_string());
                    log.lock().unwrap().push(format!("{name}: {why}"));
                }
            }
            if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                widget_check(app)
            })) {
                let why = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "a non-string panic".to_string());
                log.lock().unwrap().push(format!("wire: {why}"));
            }
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    let failed = failures.lock().unwrap().clone();
    assert!(failed.is_empty(), "pixel-reach checks failed -> {}", failed.join(" // "));
    for (name, ran) in [
        ("rows", RAN_ROWS.load(Ordering::SeqCst)),
        ("reach", RAN_REACH.load(Ordering::SeqCst)),
        ("wire", RAN_WIRE.load(Ordering::SeqCst)),
    ] {
        assert!(ran, "the {name} check never ran");
    }
}

#[test]
fn sec_05_cut_6_parameters_used_s6_the_four_pixel_reaches_are_all_catalogued() {
    common::pin_private_cwd();
    round();
}
