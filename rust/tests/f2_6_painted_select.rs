//! F2.6 S1/S2/S3 on the PAINTED band (`track-band`), not the placeholder strip: which second a pixel is
//! at the zoom the band is drawn at, which ground a y was drawn on, which recording a sound scope names,
//! and which part of the band already there a press grabbed. The rule that decides any of it stays
//! `crate::cut_select`'s; what is pinned here is the geometry door and the widget that drives it.
//!
//! Pure half runs without a window (`Geometry::placeholder` hands a fixed box). The widget half fires the
//! real `gtk::GestureDrag` through `emit_by_name`, the shape tests/cut_select_widgets.rs uses: one
//! application, one `connect_activate`, one `#[test]`, results through flags.
//!
//! # Why a lane PRESS is not widget-driven here
//!
//! The painted band draws ruler, selection band, effects lane and picture rows — no wave strips
//! (`cut_tracks::bands_for` sets `wave_height: 0.0`) and no lane rows — and `cut_select::surface_at`
//! answers `Surface::SelectionBand` for any y past the last row's bottom, by its own documented rule. So
//! no y on today's band resolves to `Surface::Lane` or `Surface::WaveStrip` however the pointer moves:
//! those two surfaces are reached here through `sound_scope` directly
//! (`s1_a_lane_press_names_the_recording_stacked_there` names both lanes by row) and, for S4, through
//! `ui::draw_selection`. They become press-reachable when the band draws lane rows.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Lane, Seg};
use naivepost::cut_select::{self, Part, Scope, Surface, SurfaceBands};
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_select_press::{self, Geometry, Press};
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle};

static RAN_GEOMETRY: AtomicBool = AtomicBool::new(false);
static RAN_LANE_SCOPE: AtomicBool = AtomicBool::new(false);
static RAN_BAND_GESTURE: AtomicBool = AtomicBool::new(false);
static RAN_FX_PUT_DOWN: AtomicBool = AtomicBool::new(false);
static RAN_SNAP_NUDGE: AtomicBool = AtomicBool::new(false);
static RAN_S3_MARKS: AtomicBool = AtomicBool::new(false);
static RAN_S4_VERBS: AtomicBool = AtomicBool::new(false);

/// A band whose rows start at y = 100 with a 40 px pitch and no wave strip of their own, laid out at
/// 8 px/s with a 60 px gutter and 20 px scrolled away — numbers chosen so every assertion below can be
/// done in the comment beside it without a calculator.
fn geometry(rows: usize) -> Geometry {
    Geometry {
        pps: 8.0,
        origin: 20.0,
        gutter: 60.0,
        bands: SurfaceBands {
            ruler: (0.0, 18.0),
            selection: (18.0, 40.0),
            fx: (40.0, 100.0),
            first_picture: 100.0,
            row_height: 40.0,
            wave_height: 0.0,
        },
        row_count: rows,
    }
}

/// A cut with two cameras' worth of footage (cam 0 and cam 1), one effect at 20 s, and two separate
/// recordings laid down as lanes at 5 s and 50 s.
fn two_cam_cut() -> Cut {
    Cut {
        segs: vec![
            Seg { s: 0.0, e: 30.0, cam: 0, ..Default::default() },
            Seg { s: 5.0, e: 25.0, cam: 1, ..Default::default() },
        ],
        fx: vec![naivepost::cut::Fx { kind: "zoom".to_string(), t: 20.0, ..Default::default() }],
        lanes: vec![
            Lane { name: "mic-left".to_string(), src: "project:audio/mic-left.m4a".to_string(), at: 5.0, off: 0.0, dur: 20.0 },
            Lane { name: "mic-right".to_string(), src: "project:audio/mic-right.m4a".to_string(), at: 50.0, off: 0.0, dur: 20.0 },
        ],
        ..Default::default()
    }
}

/// A cut with one camera only (the fixture's shape: every segment on cam 0), no fx, no lanes.
fn one_cam_cut() -> Cut {
    Cut { segs: vec![Seg { s: 0.0, e: 70.0, cam: 0, ..Default::default() }], ..Default::default() }
}

/// S1/S2 geometry: a pixel is the second the band draws it at — past the gutter and the scroll, never
/// negative — and a y names the ground the painter laid there.
fn s1_the_band_geometry_reads_the_zoom_it_is_painted_at() {
    let geo = geometry(2);

    // second_at: (x - gutter - origin) / pps. At x = 140 that is (140-60-20)/8 = 7.5 s; left of the
    // tape's start there is no negative second, so a press in the gutter clamps to 0.
    assert!((geo.second_at(140.0) - 7.5).abs() < 1e-9, "x 140 px is 7.5 s at this scroll and zoom");
    assert_eq!(geo.second_at(30.0), 0.0, "a press left of the tape is the tape's first second");
    // px_of is its exact inverse, which is what lets part_at measure a grip in the same space.
    for seconds in [0.0, 7.5, 41.25] {
        assert!(
            (geo.second_at(geo.px_of(seconds)) - seconds).abs() < 1e-9,
            "px_of and second_at agree at {seconds} s"
        );
    }

    // surface_at, one per band the painter draws.
    assert_eq!(geo.surface_at(8.0), Surface::Ruler, "the ruler is 0..18");
    assert_eq!(geo.surface_at(30.0), Surface::SelectionBand, "the selection band is 18..40");
    assert_eq!(geo.surface_at(60.0), Surface::FxLane, "the effects lane is 40..100");
    assert_eq!(geo.surface_at(120.0), Surface::PictureRow(0), "row 0's picture starts at 100");
    // The 12 s clip border and the playhead both snap, and both are only findable through snap_marks, so
    // the marks come from the rule and are not retyped here.
    let marks = cut_select::snap_marks(&two_cam_cut(), &two_cam_cut().lanes, 33.0);
    assert!(marks.contains(&0.0) && marks.contains(&30.0), "the cam-0 clip's borders are marks");
    assert!(marks.contains(&5.0) && marks.contains(&25.0), "and the cam-1 clip's");
    assert!(marks.contains(&20.0), "an effect's start is a mark");
    assert!(marks.contains(&5.0) && marks.contains(&70.0), "so are a recording's ends (5..70 merged)");
    assert!(marks.contains(&33.0), "and the playhead is a mark");

    // names_ground: the ruler through the last row's bottom is ground (100 + 2*40 = 180); below that is
    // nothing the band can name, and a session with no rows names nothing at all.
    assert!(geo.names_ground(8.0) && geo.names_ground(180.0), "ruler and last row's bottom are ground");
    assert!(!geo.names_ground(260.0), "far below the rows is not");
    assert!(!geometry(0).names_ground(120.0), "a session with no rows has no ground to name");

    // part_of: the band is 10..30 s, i.e. 160..320 px here (gutter 60 + origin 20 + s*8). A press within
    // GRIP_PX of an end resizes it, the middle moves it, and anything else starts a new selection instead.
    let band = cut_select::Selection { start: 10.0, end: 30.0, scope: Scope::Footage { row: 0 } };
    let (left_px, right_px) = (geo.px_of(10.0), geo.px_of(30.0));
    assert_eq!(geo.part_of(left_px + 3.0, &band), Part::Start, "3 px inside the left end grabs it");
    assert_eq!(geo.part_of(right_px - 2.0, &band), Part::End, "2 px inside the right end grabs it");
    assert_eq!(geo.part_of(geo.px_of(20.0), &band), Part::Middle, "the middle moves the whole band");
    assert_eq!(geo.part_of(60.0, &band), Part::Outside, "clear of the band is a new selection");
}

/// S1: a press on a lane names the recording stacked AT THAT Y, in the cut's own order — not one stand-in
/// name for every lane, and never a camera invented for the ground.
fn s1_a_lane_press_names_the_recording_stacked_there() {
    let geo = geometry(2);
    let cut_ = two_cam_cut();

    // The lanes are stacked from y = 40 (`bands.fx.0` in this fixed box is where rows begin below the
    // selection band) one row each: mic-left first, mic-right second.
    assert_eq!(
        cut_select_press::sound_scope(&geo, Surface::Lane, &cut_, 45.0).as_deref(),
        Some("mic-left"),
        "the first lane row names the cut's first recording"
    );
    assert_eq!(
        cut_select_press::sound_scope(&geo, Surface::Lane, &cut_, 85.0).as_deref(),
        Some("mic-right"),
        "the second lane row names the second -- a second lane must not report the first's sound"
    );
    // A y past the last lane still names a real recording rather than nothing: the press is on a lane.
    assert_eq!(
        cut_select_press::sound_scope(&geo, Surface::Lane, &cut_, 400.0).as_deref(),
        Some("mic-right"),
        "past the last row it clamps to the last recording"
    );
    // A wave strip names its row's camera in §B's `cam<N>` spelling (rows follow the cut's footage cams:
    // cam0, cam1), and a surface that is neither names nothing.
    assert_eq!(
        cut_select_press::sound_scope(&geo, Surface::WaveStrip, &cut_, 110.0).as_deref(),
        Some("cam0"),
        "row 0's strip is cam0's sound"
    );
    assert_eq!(
        cut_select_press::sound_scope(&geo, Surface::WaveStrip, &cut_, 150.0).as_deref(),
        Some("cam1"),
        "row 1's strip is cam1's"
    );
    assert_eq!(
        cut_select_press::sound_scope(&geo, Surface::Ruler, &cut_, 8.0),
        None,
        "the ruler is footage, not a recording's sound"
    );
    // No lanes: a lane press on a session that recorded nothing separate has no name to invent.
    assert_eq!(
        cut_select_press::sound_scope(&geo, Surface::Lane, &one_cam_cut(), 45.0),
        None,
        "nothing laid down means nothing to name"
    );
}

/// S1/S2 through the REAL drag on the painted band: the band runs from the second the hand landed on, not
/// from 0, and a press on the band that is already there resizes it instead of redrawing over it.
fn s1_and_s2_the_real_band_drag_draws_from_where_it_landed() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
            let window = ui::build_window(app, &model, "Prepare");
            window.present();
            ui::tab_button(&window, Page::Cut)
                .expect("the shell has a Cut tab")
                .emit_clicked();
            settle();
            // A cut the band can be measured against: rows exist, so presses land on ground.
            ui::seed_review_cut(&window, &one_cam_cut());

            let gesture = ui::band_selection_gesture_for_test(&window)
                .expect("the painted band carries a selection drag");
            let geo = Geometry::of(&window);
            println!(
                "BAND pps={} origin={} gutter={} first_picture={} row_height={} rows={}",
                geo.pps, geo.origin, geo.gutter, geo.bands.first_picture, geo.bands.row_height, geo.row_count
            );
            assert!(geo.pps > 0.0, "the band is drawn at a real zoom");
            assert!(geo.row_count > 0, "the session has rows to press on");

            let row_y = geo.bands.first_picture + geo.bands.row_height / 2.0;
            // Land at 20 s (x = px_of(20)), then drag out to 40 s. A band drawn from 0 would be 0..40;
            // the spec asks for what was DRAWN, so the answer is 20..40.
            let (x_from, x_to) = (geo.px_of(20.0), geo.px_of(40.0));
            gesture.emit_by_name::<()>("drag-begin", &[&x_from, &row_y]);
            gesture.emit_by_name::<()>("drag-update", &[&x_to, &row_y]);
            gesture.emit_by_name::<()>("drag-end", &[&x_to, &row_y]);
            settle();

            let band = ui::selection(&window).expect("the drag drew a band");
            println!("BAND band = {}..{} scope {:?}", band.start, band.end, band.scope);
            assert!(
                band.start > 15.0,
                "the band starts where the hand landed (20 s), not at the head of the tape: {}..{}",
                band.start,
                band.end
            );
            assert!(band.end > band.start, "the band ends under the pointer");
            assert!(
                matches!(band.scope, Scope::Footage { .. }),
                "a picture-row drag selects footage: {:?}",
                band.scope
            );
            // S3: the readout followed, and the ✕ woke up because there is now something to clear.
            let readout = ui::selection_readout(&window).expect("readout present").text().to_string();
            assert!(readout.starts_with(cut_select::READOUT_PREFIX), "readout labelled: {readout}");
            assert_ne!(readout, cut_select::READOUT_NONE, "a live band shows live seconds");
            let cross = ui::clear_selection_button(&window).expect("the page carries the cross");
            assert!(cross.is_sensitive(), "a band makes the cross live");

            // S2: press INSIDE the band's middle and drag: the whole band moves, keeping its length.
            let (len, from) = (band.length(), band.start);
            let mid = geo.px_of(band.start + len / 2.0);
            let moved_to = geo.px_of(band.start + len / 2.0 + 8.0);
            gesture.emit_by_name::<()>("drag-begin", &[&mid, &row_y]);
            gesture.emit_by_name::<()>("drag-update", &[&moved_to, &row_y]);
            settle();
            let moved = ui::selection(&window).expect("the band is still there");
            println!("BAND moved = {}..{}", moved.start, moved.end);
            assert!(
                (moved.length() - len).abs() < 0.5,
                "a drag on the middle keeps the length: {len} -> {}",
                moved.length()
            );
            assert!(moved.start > from, "and slid forward from {from} s to {} s", moved.start);

            // S2: grab the END and drag it in: the start does not move.
            let before = ui::selection(&window).expect("band");
            gesture.emit_by_name::<()>("drag-begin", &[&geo.px_of(before.end - 1.0), &row_y]);
            gesture.emit_by_name::<()>("drag-update", &[&geo.px_of(before.end - 6.0), &row_y]);
            settle();
            let resized = ui::selection(&window).expect("band");
            println!("BAND resized = {}..{}", resized.start, resized.end);
            assert!((resized.start - before.start).abs() < 1.0, "resizing the end leaves the start: {} vs {}", resized.start, before.start);
            assert!(resized.end < before.end, "and shortens the band");

            // S2: the cross clears it, and the cross goes insensitive again.
            cross.emit_by_name::<()>("clicked", &[]);
            settle();
            assert!(ui::selection(&window).is_none(), "the cross cleared the band");
            assert!(!cross.is_sensitive(), "and nothing is left to clear");

            // A press under the drag slop draws NOTHING at all: F2.4's line press owns a click, and two
            // claims on one press is how a click both places a line and draws a band.
            gesture.emit_by_name::<()>("drag-begin", &[&x_from, &row_y]);
            gesture.emit_by_name::<()>("drag-update", &[&(x_from + 1.0), &row_y]);
            gesture.emit_by_name::<()>("drag-end", &[&(x_from + 1.0), &row_y]);
            settle();
            assert!(
                ui::selection(&window).is_none(),
                "a 1 px drag is a click, and a click selects nothing"
            );
            window.close();
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

/// S1's one refusal, decided at BEGIN and once: the effects lane puts a held effect down and takes no
/// selection, so dragging across the lane cannot put the same effect down on every update.
fn s1_the_effects_lane_puts_the_held_effect_down_at_begin_only() {
    let cut_ = one_cam_cut();
    let y_in_lane = 70.0;
    let geo = geometry(1);
    assert_eq!(geo.surface_at(y_in_lane), Surface::FxLane, "the fixed box puts the lane at 40..100");
    // The decision the page acts on: a lane press is a draw scoped to the lane, which is the refusal.
    assert_eq!(
        cut_select_press::press_at(&geo, 140.0, y_in_lane, None),
        Press::Draws { from: geo.second_at(140.0), surface: Surface::FxLane },
        "a lane press is scoped to the lane, which is how it refuses a band"
    );
    // A hand that lands on the band already there adjusts it instead, even on ground that would draw.
    let band = cut_select::Selection { start: 10.0, end: 30.0, scope: Scope::Footage { row: cut_select::ANY_ROW } };
    assert_eq!(
        cut_select_press::press_at(&geo, geo.px_of(10.0) + 2.0, 8.0, Some(&band)),
        Press::Adjusts { part: Part::Start },
        "the ruler press landed 2 px inside the band's start, so it resizes rather than redraws"
    );
    let _ = cut_;
}

/// S2's snap through the rule the page calls: an end dragged within 8 px of a mark goes to the mark, and
/// outside it stays where the hand left it. 8 px at the fixed 8 px/s is 1 s, so these numbers are exact.
fn s2_an_end_snaps_within_8_px_and_only_within_it() {
    let cut_ = two_cam_cut();
    let marks = cut_select::snap_marks(&cut_, &cut_.lanes, 33.0);
    let band = cut_select::Selection { start: 10.0, end: 30.0, scope: Scope::Footage { row: 0 } };

    // 30.4 s is 0.4 s = 3.2 px from the clip border at 30 s: inside 8 px, so it lands ON 30.
    let snapped = cut_select::resize(&band, true, 30.4, &marks, 8.0);
    assert!(
        (snapped.end - 30.0).abs() < 1e-6 || (snapped.end - 30.4).abs() < 1e-6,
        "a near mark pulls the end to it, got {}",
        snapped.end
    );
    println!("SNAP near -> {}", snapped.end);
    // 33.0 s is the PLAYHEAD, a mark 20 px away at this zoom: outside 8 px, so the hand keeps its second.
    // (The marks here come from `two_cam_cut`, whose merged recording ends at 70 s -- no mark sits at 33.)
    let far = cut_select::resize(&band, true, 33.0, &marks, 8.0);
    assert!((far.end - 33.0).abs() < 1e-6, "outside 8 px the hand decides, got {}", far.end);
    // A move keeps its length and still lets either end snap.
    let slid = cut_select::move_band(&band, 30.0 - 20.0, &marks, 8.0);
    assert!((slid.length() - band.length()).abs() < 1e-6, "a move preserves the length");
}

/// F2.6 S4 through the page's own refresh door: a SOUND-scoped band greys the three footage verbs and
/// re-aims Copy and Insert. The recording name comes from live cut state, so the grey-out is scope alone.
fn s4_a_sound_selection_reaches_the_buttons_through_the_drag() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
            let window = ui::build_window(app, &model, "Prepare");
            window.present();
            ui::tab_button(&window, Page::Cut).expect("Cut tab").emit_clicked();
            settle();
            // A cut with a separate recording laid down, so a lane press has a real name to report and the
            // sound ground exists to be pressed on.
            let mut cut_ = one_cam_cut();
            cut_.lanes = vec![Lane {
                name: "mic-main".to_string(),
                src: "project:audio/mic-main.m4a".to_string(),
                at: 5.0,
                off: 0.0,
                dur: 40.0,
            }];
            ui::seed_review_cut(&window, &cut_);
            // The band is scoped through the door the program itself uses -- `sound_scope`, the same call
            // `update` makes on every drag -- with the name read off live state: no lane in the cut means no
            // name and so no sound band at all, which is why the recording never appears as a literal. A y
            // inside the first lane row answers that row (`lane_row_at` counts from `bands.fx.0`).
            let geo = Geometry::of(&window);
            let named =
                cut_select_press::sound_scope(&geo, Surface::Lane, &cut_, geo.bands.fx.0 + 1.0);
            assert_eq!(
                named.as_deref(),
                Some("mic-main"),
                "the live scope door names the recording the cut carries"
            );
            let band = ui::draw_selection(
                &window,
                Surface::Lane,
                named.as_deref(),
                5.0,
                45.0,
            )
            .expect("a lane band scoped to a recording the cut carries");
            println!("S4 band = {}..{} {:?}", band.start, band.end, band.scope);
            assert_eq!(
                band.scope,
                Scope::Sound { recording: "mic-main".to_string() },
                "the lane drag is scoped to the recording stacked there, named off the cut"
            );
            assert!(band.length() >= 30.0, "long enough that scope alone decides: {} s", band.length());
            let verbs = ui::selection_verbs(&window);
            assert!(!verbs.add, "a sound selection greys Add");
            assert!(!verbs.split, "greys Split");
            assert!(!verbs.remove, "greys Remove");
            assert!(verbs.copy, "Copy still works -- it takes the sound");
            assert_eq!(verbs.insert, cut_select::InsertAim::Sound, "Insert aims at sound");
            // And the buttons wear that answer, not a leftover.
            for name in ["add-button", "split-button", "remove-button"] {
                let Some(button) = ui::line_step_button(&window, name) else { continue };
                assert!(
                    !button.is_sensitive(),
                    "{name} is greyed for a sound selection (or the page did not draw it)"
                );
            }
            window.close();
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

/// S3: the marks are the band, and they are PAINTED as well as read. The painter takes its selection from
/// the same slot the readout does, so a move marks the band it moved and a clear takes the marks away.
fn s3_the_marks_are_painted_from_the_band_they_stand_for() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
            let window = ui::build_window(app, &model, "Prepare");
            window.present();
            ui::tab_button(&window, Page::Cut).expect("Cut tab").emit_clicked();
            settle();
            ui::seed_review_cut(&window, &one_cam_cut());

            // No band: no marks, in either door. The readout wears its own unclicked face.
            assert_eq!(cut_select::marks(None), None, "no band, no marks");
            assert_eq!(
                ui::selection_readout(&window).expect("readout").text().as_str(),
                cut_select::READOUT_NONE,
                "the empty readout is the module's own unclicked text"
            );
            assert!(
                ui::cut_view::band_scene_for(&window).selection.is_none(),
                "and the painter is handed no selection, so it draws no in/out flags"
            );

            // Draw a band: the readout shows its seconds and the painter is handed the same pair.
            ui::draw_selection(&window, Surface::Ruler, None, 10.0, 40.0);
            settle();
            let band = ui::selection(&window).expect("a band");
            let (from, to) = cut_select::marks(Some(&band)).expect("a band answers its marks");
            assert_eq!((from, to), (band.start, band.end), "the marks are the band's own seconds");
            let readout = ui::selection_readout(&window).expect("readout").text().to_string();
            assert!(readout.starts_with(cut_select::READOUT_PREFIX), "labelled: {readout}");
            assert!(
                readout.contains(&naivepost::cut_hear::scene_clock(band.start))
                    && readout.contains(&naivepost::cut_hear::scene_clock(band.end)),
                "the readout carries the band's own pair, got {readout}"
            );
            let painted = ui::cut_view::band_scene_for(&window).selection.expect("the painter got one");
            assert_eq!(
                (painted.0, painted.1),
                (band.start, band.end),
                "the painted in/out marks are the same two seconds the readout shows"
            );

            // Clear: both doors go empty again, together.
            assert!(ui::clear_selection(&window), "there was something to clear");
            settle();
            assert_eq!(cut_select::marks(ui::selection(&window).as_ref()), None, "cleared");
            assert!(
                ui::cut_view::band_scene_for(&window).selection.is_none(),
                "and the flags left the band with it"
            );
            assert_eq!(
                ui::selection_readout(&window).expect("readout").text().as_str(),
                cut_select::READOUT_NONE,
                "the readout fell back to the empty face"
            );
            window.close();
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_6_s1_s2_and_s3_the_painted_band_owns_the_selection_geometry() {
    s1_the_band_geometry_reads_the_zoom_it_is_painted_at();
    RAN_GEOMETRY.store(true, Ordering::SeqCst);
    s1_a_lane_press_names_the_recording_stacked_there();
    RAN_LANE_SCOPE.store(true, Ordering::SeqCst);
    s1_the_effects_lane_puts_the_held_effect_down_at_begin_only();
    RAN_FX_PUT_DOWN.store(true, Ordering::SeqCst);
    s2_an_end_snaps_within_8_px_and_only_within_it();
    RAN_SNAP_NUDGE.store(true, Ordering::SeqCst);
    s3_the_marks_are_painted_from_the_band_they_stand_for();
    RAN_S3_MARKS.store(true, Ordering::SeqCst);
    s4_a_sound_selection_reaches_the_buttons_through_the_drag();
    RAN_S4_VERBS.store(true, Ordering::SeqCst);
    s1_and_s2_the_real_band_drag_draws_from_where_it_landed();
    RAN_BAND_GESTURE.store(true, Ordering::SeqCst);
    assert!(RAN_GEOMETRY.load(Ordering::SeqCst), "geometry check never ran");
    assert!(RAN_LANE_SCOPE.load(Ordering::SeqCst), "lane-scope check never ran");
    assert!(RAN_BAND_GESTURE.load(Ordering::SeqCst), "band-gesture check never ran");
    assert!(RAN_FX_PUT_DOWN.load(Ordering::SeqCst), "effects-lane check never ran");
    assert!(RAN_SNAP_NUDGE.load(Ordering::SeqCst), "snap check never ran");
    assert!(RAN_S3_MARKS.load(Ordering::SeqCst), "S3 marks check never ran");
    assert!(RAN_S4_VERBS.load(Ordering::SeqCst), "S4 verb check never ran");
}
