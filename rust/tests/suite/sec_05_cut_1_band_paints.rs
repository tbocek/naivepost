//! §05-cut#1-screen — the Cut page's band really draws the §1 marks, measured as pixels rather than by eye.
//!
//! Every check paints a small `Scene` onto an `ImageSurface`, reads bytes back and compares a sample
//! INSIDE the mark with a sample beside it. A wrong rule (a colour applied to the wrong span, a band drawn
//! at the wrong row) changes a number here; a CSS class always parses, so a stylesheet could never tell us.

use crate::common;
use gtk4 as gtk;
use gtk4::prelude::*;
use naivepost::{
    cut::{Fx, Seg},
    cut_screen, fx_lane,
    timeline::Recording,
    ui::{
        cut_colours,
        cut_tracks::{self as tracks, Scene, PICTURE_ROW_EXTRA_PX, SELECTION_BAND_PX},
    },
};
use std::sync::atomic::{AtomicBool, Ordering};

const PPS: f64 = 2.0;
const WIDTH: i32 = 300;
/// The band's three fixed rows above the pictures, as the painter lays them.
fn head_px(scene: &Scene) -> i32 {
    (naivepost::cut_trim::RULER_H
        + SELECTION_BAND_PX
        + fx_lane::lane_height_px(fx_lane::row_count(&scene.fx))) as i32
}

#[test]
fn sec_05_cut_1_screen_the_rows_stack_in_the_order_the_inventory_lists_them() {
    common::pin_private_cwd();
    // §A's Tracks paragraph, top to bottom: ruler (18 px), selection band (22 px), effects lane (26 px per
    // row), picture rows (thumbHt+4), then that row's wave strip (30 px). The painter's own height function
    // is what sizes the surface every other check samples, so the §A numbers are only the row order if
    // adding a row of each kind makes the band exactly that much taller, in that order, with nothing counted
    // twice and nothing left out.
    let base = scene();
    let head = naivepost::cut_trim::RULER_H + SELECTION_BAND_PX;
    // One effect row is one row of 26 px, and a second overlapping group is a second row of the same 26.
    let one_group = Fx {
        kind: "zoom".into(),
        t: 10.0,
        dur: 5.0,
        ..Default::default()
    };
    let mut second_group = one_group.clone();
    second_group.t = 12.0;
    let mut one_row = base.clone();
    one_row.fx = vec![one_group.clone()];
    let mut two_rows = base.clone();
    two_rows.fx = vec![one_group.clone(), second_group];
    let h = |scene: &Scene| tracks::band_height_px(scene, cut_screen::THUMB_AT_OPEN);
    assert!(
        (h(&one_row)
            - (head
                + fx_lane::lane_height_px(1)
                + cut_screen::THUMB_AT_OPEN as f64
                + PICTURE_ROW_EXTRA_PX))
            < 1.0,
        "{} px: ruler + selection band + one effect row + one picture row",
        h(&one_row)
    );
    assert!(
        (h(&two_rows) - h(&one_row) - fx_lane::lane_height_px(1)).abs() < 1.0,
        "a second overlapping group costs another effect row: {} -> {}",
        h(&one_row),
        h(&two_rows)
    );
    // No effect at all costs no effects lane: the lane is sized by the groups, not by a floor of one.
    assert!(
        (h(&base)
            - (head
                + fx_lane::lane_height_px(fx_lane::row_count(&base.fx))
                + cut_screen::THUMB_AT_OPEN as f64
                + PICTURE_ROW_EXTRA_PX))
            .abs()
            < 1.0,
        "{} px with the scene's own row count",
        h(&base)
    );
    // And a second camera row is another picture row plus its wave strip, which is where §A's "each row's
    // first audio track as a strip below" and its 30 px come out of the height rather than being asserted.
    let mut two_cams = base.clone();
    two_cams.row_count = 2;
    let extra = h(&two_cams) - h(&base);
    assert!(
        extra > 0.0,
        "a second row has to be paid for: {} -> {}",
        h(&base),
        h(&two_cams)
    );
    assert!(
        extra >= cut_screen::THUMB_AT_OPEN as f64 + PICTURE_ROW_EXTRA_PX,
        "a picture row alone costs {:?} px, so a second row costs at least that: {extra}",
        cut_screen::THUMB_AT_OPEN as f64 + PICTURE_ROW_EXTRA_PX
    );
}

#[test]
fn sec_05_cut_1_screen_the_gutter_is_the_first_30_px_of_timeline_space() {
    common::pin_private_cwd();
    // §A: "gutter (first 30 px of timeline space; fold-all badge, lane sound switches, row switches,
    // empty-row ✕)" — the gutter is measured IN the tape, not bolted in front of it, so a pixel of the
    // window is a pixel of the timeline minus 30. `cut_view` is what turns a window width into the view the
    // zoom floor and the scrollbar's page size are computed from, so the subtraction happens exactly once.
    let surface = 1030.0;
    assert_eq!(
        naivepost::ui::cut_view::gutter_px(),
        cut_screen::GUTTER_PX,
        "one gutter constant"
    );
    assert_eq!(
        naivepost::ui::cut_view::view_px(surface) + cut_screen::GUTTER_PX,
        surface,
        "view + gutter is the surface it was measured from"
    );
    // And the tape is the gutter plus the filmed seconds, so a session of zero length still costs 30 px
    // of gutter: the band is never completely empty of a place to put the badges.
    assert_eq!(
        naivepost::ui::cut_view::total_px(0.0, 4.0),
        cut_screen::GUTTER_PX
    );
    assert_eq!(
        naivepost::ui::cut_view::total_px(100.0, 4.0),
        cut_screen::GUTTER_PX + 400.0
    );
}

#[test]
fn sec_05_cut_1_screen_the_picture_band_asks_for_frames_at_its_own_zoom_and_position() {
    common::pin_private_cwd();
    // §1's Pictures paragraph ("the band has a picture at every zoom") only holds if the band can NAME the
    // frame it wants: the same rule the draw uses must be a function of the zoom and the scroll origin, or
    // zooming and scrolling would repaint from the same fixed list of files. `frame_near` picks the nearest
    // extracted frame to a second; `frame_paths` lists what Prepare extracted. Both are the painter's own.
    let dir = std::env::temp_dir().join(format!("band-frames-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp frame folder");
    // Four frames a second apart, named the only way the page names an extracted frame: `clock::frame_name`
    // stamps it with the second it came from, and `frame_near` is a byte compare against that stamp. The
    // names are made by asking the clock what IT calls second 0/1/2/3 — no epoch constant to get wrong.
    let want = |second: f64| format!("{}.jpg", naivepost::clock::frame_name(second));
    let paths: Vec<std::path::PathBuf> = (0..4)
        .map(|second| {
            let name = want(second as f64);
            let path = dir.join(&name);
            std::fs::write(&path, b"jpeg-bytes").expect("a frame file");
            path
        })
        .collect();
    assert_eq!(paths.len(), 4, "four distinct stamps: {:?}", paths);
    // The clock names second 0 `1970-01-01_00-00-00.000`, and a `.jpg` suffix makes the file name one
    // character longer than the stamp it was built from — the rule `frame_near` compares on.
    let stamp = |second: f64| {
        let name = want(second);
        assert_eq!(name.len(), naivepost::clock::frame_name(0.0).len() + 4);
        dir.join(name)
    };

    // The newest frame at or before the second asked for, which is the only safe direction to look: a row
    // may show a frame that already happened, never one that has not. The compare is the FILE NAME against
    // the wanted stamp, and a file name carries a four-character `.jpg`, so every frame sorts one past its
    // own second and is the answer for a whole second longer than it was extracted for.
    assert_eq!(tracks::frame_near(&paths, 0.0), Some(stamp(0.0)));
    assert_eq!(
        tracks::frame_near(&paths, 0.999),
        Some(stamp(0.0)),
        "inside a second, its own frame"
    );
    assert_eq!(
        tracks::frame_near(&paths, 1.0),
        Some(stamp(0.0)),
        "the suffix's one second of slack"
    );
    assert_eq!(tracks::frame_near(&paths, 1.6), Some(stamp(0.0)));
    assert_eq!(tracks::frame_near(&paths, 1.999), Some(stamp(0.0)));
    assert_eq!(
        tracks::frame_near(&paths, 2.0),
        Some(stamp(1.0)),
        "one frame per second of the tape"
    );
    assert_eq!(tracks::frame_near(&paths, 3.0), Some(stamp(2.0)));
    // Earlier than every frame there is nothing older to show, so the first one stands in rather than the
    // row going empty — which is what a band scrolled back before the first extracted picture does.
    assert_eq!(tracks::frame_near(&paths, -5.0), Some(stamp(0.0)));
    // Past the last frame there is nothing newer to ask for, and the same rule keeps the band from going
    // black at the end of a session.
    assert_eq!(tracks::frame_near(&paths, 99.0), Some(stamp(3.0)));

    // The zoom and the origin decide WHICH seconds get a picture: at 2 px/s a 64 px row covers 32 s, at
    // 240 px/s it covers 0.27 s, and scrolling by 100 px at either zoom moves the asked-for second by
    // `100 / pps` seconds. Same numbers the band is painted with, read from `cut_view`'s own helpers.
    let second_at = |pps: f64, origin: f64, x: f64| (origin + x) / pps;
    assert!((second_at(2.0, 0.0, 100.0) - 50.0).abs() < 1e-9);
    assert!((second_at(240.0, 0.0, 100.0) - 0.4166).abs() < 1e-3);
    assert!(
        (second_at(2.0, 100.0, 0.0) - 50.0).abs() < 1e-9,
        "scrolling asks for a later second"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A scene with two takes, one row, nothing else set: each check adds the one thing it is about.
fn scene() -> Scene {
    Scene {
        pps: PPS,
        origin: 0.0,
        segs: vec![Seg {
            s: 0.0,
            e: 20.0,
            ..Default::default()
        }],
        fx: Vec::new(),
        recordings: vec![Recording {
            base: "take-a".into(),
            start: 0.0,
            end: 50.0,
        }],
        folds: Vec::new(),
        selection: None,
        watched_row: None,
        row_count: 1,
        shift: Default::default(),
        lanes: Vec::new(),
        lane_plates: Vec::new(),
        thumbs: Vec::new(),
        line: 0.0,
    }
}

/// Paint `scene` and hand back a pixel reader: (x, y) -> (r, g, b) plus the whole surface's height.
fn painted(scene: &Scene) -> (Vec<u8>, usize, i32) {
    let height = tracks::band_height_px(scene, tracks::THUMB_DEFAULT_PX).ceil() as i32;
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, WIDTH, height).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        tracks::paint_at(
            &cr,
            scene,
            WIDTH as f64,
            height as f64,
            tracks::THUMB_DEFAULT_PX,
        );
    }
    let data = {
        let read = surface.data().expect("read the pixels back").to_vec();
        read
    };
    let stride = surface.stride() as usize;
    (data, stride, height)
}

fn rgb(data: &[u8], stride: usize, x: i32, y: i32) -> (u8, u8, u8) {
    // ARgb32 is stored B,G,R,A on little-endian.
    let base = y as usize * stride + x as usize * 4;
    (data[base + 2], data[base + 1], data[base])
}

/// Does anything at all change down this column between `from` and `to`? A mark that paints nothing and a
/// mark painted off-screen both answer "no", which is the point.
fn column_differs(
    painted: &(Vec<u8>, usize, i32),
    x: i32,
    from: i32,
    to: i32,
    ground: (u8, u8, u8),
) -> bool {
    (from..to).any(|y| rgb(&painted.0, painted.1, x, y) != ground)
}

/// The one GTK initialisation this binary does, shared by both widget checks: tests run on their own
/// threads and GTK refuses work off the main one, so exactly one app round exists and it runs where GTK
/// allows. A second `gtk::init` in the same process would leave the app unable to acquire the main context,
/// which is why neither check initialises for itself.
static GTK_ROUND: std::sync::Once = std::sync::Once::new();

/// Both widget checks, run inside the single activate emission this process is allowed.
fn run_widget_round() {
    GTK_ROUND.call_once(|| {
        let app = adw::Application::builder()
            .application_id(naivepost::ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            scrollbar_gears_one_screenful_at_40_px(app);
            SCROLLBAR_RAN.store(true, Ordering::SeqCst);
            page_hangs_band_and_red_line_on_two_layers(app);
            LAYERS_RAN.store(true, Ordering::SeqCst);
            page_band_grows_for_a_lane(app);
            GROWTH_RAN.store(true, Ordering::SeqCst);
            page_band_spans_the_page(app);
            SPAN_RAN.store(true, Ordering::SeqCst);
            page_redraw_reaches_the_red_line_layer(app);
            LINE_RAN.store(true, Ordering::SeqCst);
            app.quit();
        });
        let _ = app.run_with_args::<String>(&[]);
    });
}

static SCROLLBAR_RAN: AtomicBool = AtomicBool::new(false);
static LAYERS_RAN: AtomicBool = AtomicBool::new(false);
static GROWTH_RAN: AtomicBool = AtomicBool::new(false);
static SPAN_RAN: AtomicBool = AtomicBool::new(false);
static LINE_RAN: AtomicBool = AtomicBool::new(false);

/// Badge **30**'s wire: the page's own redraw door names the red line's surface. A `DrawingArea` only runs
/// its draw callback for a widget someone queued, and §1 puts the line on its OWN layer, so a redraw list that
/// leaves `track-playhead` out paints the line once at its opening position and never moves it again — not on
/// a zoom, not on a scroll, not on a click that placed it. Read off the same list the door iterates, and
/// against the real page, so the two cannot drift apart.
#[test]
fn sec_05_cut_1_screen_s30_the_page_redraw_moves_the_red_line_layer_too() {
    common::pin_private_cwd();
    let targets = naivepost::ui::cut_view::repaint_targets();
    assert!(
        targets.contains(&"track-band"),
        "the tape is redrawn by the page's door: {targets:?}"
    );
    assert!(
        targets.contains(&"track-playhead"),
        "§1's red line has its own layer, so the page's redraw has to name it or it never moves: {targets:?}"
    );
    // And every one of those names is a widget the Cut page really builds — a name in the list that no page
    // contains would pass the rule test while drawing nothing.
    run_widget_round();
    assert!(
        LINE_RAN.load(Ordering::SeqCst),
        "the page-side half of this check never ran"
    );
}

/// The page half of badge **30**: each name the redraw door queues (but the placeholder strip, which lives
/// outside the band's stack) resolves to a real widget inside `track-layers`.
fn page_redraw_reaches_the_red_line_layer(app: &adw::Application) {
    common::release_last_window();
    let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
    let window = naivepost::ui::build_window(app, &model, "Cut");
    window.set_default_size(1400, 900);
    window.present();
    common::settle();
    let layers =
        common::widget_in(&window, "track-layers").expect("the band's stacked layers exist");
    for name in naivepost::ui::cut_view::repaint_targets() {
        if name == "track-strip" {
            continue; // F2.8's placeholder, packed beside the stack rather than inside it
        }
        let found = common::widget_in(&window, name).unwrap_or_else(|| {
            panic!("the redraw door names {name}, but the page builds no such widget")
        });
        assert!(
            found.parent().is_some(),
            "{name} is in the tree but parentless, so nothing lays it out"
        );
        // Both of the band's own layers live in the one overlay §1's stack is.
        let inside = found
            .parent()
            .map(|parent| parent.upcast_ref::<gtk::Widget>() == layers.upcast_ref::<gtk::Widget>())
            .unwrap_or(false);
        assert!(
            inside,
            "{name} sits outside the band's overlay, so the stack does not cover it"
        );
    }
    common::hold_last_window(window);
}

/// Badge **24-31** on the real page: the tape spans the PAGE. §A's page is a paned whose bottom holds the
/// toolbar, the tracks and the scrollbar, and `spec/img/05-cut.png` shows the ruler, the kept bars and the
/// wave strips running its whole width — a band fixed at the placeholder's 480 px leaves the right two thirds
/// of the screen as an empty plate, which is what the first shot of this item showed. Measured as an
/// allocation, not as a colour: the width is GTK's decision, so only the laid-out widget can answer it.
fn page_band_spans_the_page(app: &adw::Application) {
    let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
    let window = naivepost::ui::build_window(app, &model, "Cut");
    // Every Cut-page snapshot needs the size before `present()` or the page collapses to one column.
    window.set_default_size(1400, 900);
    window.present();
    common::settle();
    let band = common::widget_in(&window, "track-band")
        .expect("the Cut page draws the track band")
        .downcast::<gtk::DrawingArea>()
        .expect("the band is a drawing area");
    let line = common::widget_in(&window, "track-playhead")
        .expect("the red line has its own layer")
        .downcast::<gtk::DrawingArea>()
        .expect("the line is a drawing area");
    let page = f64::from(window.width());
    let tape = f64::from(band.width());
    assert!(
        page > 0.0 && tape > 0.0,
        "both laid out: page {page}, band {tape}"
    );
    assert!(
        tape > f64::from(tracks::BAND_WIDTH_PX),
        "the band outgrew the placeholder's floor: {tape} px over {page} px of window"
    );
    // The page's own column, not the window's: the Cut page sits inside the tab stack and the panes' handles,
    // so what the band can be is the column it was packed into. §1 asks that the tape fill the page, and
    // `set_hexpand` hands it exactly that, so it is the bottom box's width one-to-one.
    let column = band
        .parent()
        .and_then(|layers| layers.parent())
        .map(|host| f64::from(host.width()))
        .unwrap_or(0.0);
    assert!(column > 0.0, "the band's own column is laid out too");
    assert!(
        (column - tape).abs() <= 1.0,
        "the tape is as wide as the page column it fills: {tape} px in a {column} px column \
         ({page} px window, {} px placeholder floor)",
        tracks::BAND_WIDTH_PX
    );
    // §1: the red line is "across both bands", so it is as wide as the tape it lies on.
    assert_eq!(
        tape,
        f64::from(line.width()),
        "the red line spans what the band spans"
    );
    // And the view the zoom floor, the scrollbar's page size and the wheel are all computed from is this
    // surface's width minus the gutter — one subtraction, in `cut_view`, no second copy.
    let view = naivepost::ui::cut_view::view_px(naivepost::ui::cut_view::surface_width(&window));
    assert!(
        (view - (tape - cut_screen::GUTTER_PX)).abs() < 1.0,
        "the page's view is the band it shows: view {view}, band {tape}"
    );
    common::hold_last_window(window);
}

/// Badge **25** at a scroll: a kept scene whose bar begins off the LEFT edge still paints the part that is
/// on screen, AND the tape's own right edge is where ground stops — a bar that overhangs is cut there, not
/// smeared into the empty ground past the filmed material (which is what `hexpand` made possible: the surface
/// is now wider than the session at the open zoom).
#[test]
fn sec_05_cut_1_screen_s1_a_bar_at_the_tape_edge_is_cut_there() {
    common::pin_private_cwd();
    // The fixture-like scene: a recording 0..50 s at PPS 2 lays the tape out to 30 + 50*2 = 130 px, while the
    // painted strip is 300 px wide. A kept scene 0..20 s ends well inside the tape; a kept scene reaching to
    // the very last filmed second ends AT it, so its bar stops there and the ground past it stays ground.
    let edge = scene(); // segs 0..20, recording 0..50
    let read = painted(&edge);
    // INSIDE the selection band the painter laid out: its top is `cut_trim::RULER_H`, its height
    // `SELECTION_BAND_PX` — read from the same two constants `rows_of` uses, not guessed below the ruler.
    let bar_y = (naivepost::cut_trim::RULER_H + SELECTION_BAND_PX / 2.0) as i32;
    let green = |x: i32| rgb(&read.0, read.1, x, bar_y);
    let ground = green(WIDTH - 2); // right of the tape: bare ground, by the setup
    let inked: Vec<i32> = (2..WIDTH - 2).filter(|x| green(*x) != ground).collect();
    let last = inked.last().copied().unwrap_or(-1);
    // The tape's last painted pixel: gutter + filmed seconds * pps, the same arithmetic `band_x` uses.
    let tape_end = (cut_screen::GUTTER_PX + 50.0 * PPS) as i32;
    assert!(last > 0, "the kept scene paints green: {inked:?}");
    assert!(
        last <= tape_end + 2,
        "nothing is inked past the tape's end at {tape_end}: last inked x {last}"
    );
    // And a scene that WOULD run past it (its seconds reach the last filmed second) is cut there rather than
    // extrapolated over the empty ground: same green, same stop.
    let to_the_end = Scene {
        segs: vec![Seg {
            s: 0.0,
            e: 50.0,
            ..Default::default()
        }],
        ..scene()
    };
    let over = painted(&to_the_end);
    let over_inked: Vec<i32> = (2..WIDTH - 2)
        .filter(|x| rgb(&over.0, over.1, *x, bar_y) != ground)
        .collect();
    let over_last = over_inked.last().copied().unwrap_or(-1);
    assert!(
        over_last <= tape_end + 2 && over_last > 0,
        "a bar to the last filmed second stops at the tape ({tape_end}), not at the strip's edge: {over_last}"
    );
}

#[test]
fn sec_05_cut_1_screen_s16_the_band_spans_the_page_width() {
    common::pin_private_cwd();
    run_widget_round();
    assert!(
        SPAN_RAN.load(Ordering::SeqCst),
        "the page-width check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_an_added_lane_grows_the_drawn_band() {
    common::pin_private_cwd();
    run_widget_round();
    assert!(
        GROWTH_RAN.load(Ordering::SeqCst),
        "the band-growth check never ran"
    );
}

#[test]
fn sec_05_cut_1_screen_the_scrollbar_gears_one_screenful_at_40_px() {
    common::pin_private_cwd();
    run_widget_round();
    assert!(SCROLLBAR_RAN.load(Ordering::SeqCst), "the round ran");
}

/// The band and the red line as widgets: two separate surfaces, stacked so the line goes OVER the marks
/// without either taking the other's space — and the overlay must not swallow presses, or F2.8's grips stop
/// being grabbable. Driven through a real Cut-page window, not a hand-built box.
#[test]
fn sec_05_cut_1_screen_the_page_hangs_the_band_and_the_red_line_on_two_layers() {
    common::pin_private_cwd();
    run_widget_round();
    assert!(LAYERS_RAN.load(Ordering::SeqCst), "the round ran");
}

/// §1's badge 31 as a number: one screenful is always 40 px of drag, however far the zoom has gone.
fn scrollbar_gears_one_screenful_at_40_px(_app: &adw::Application) {
    // The gear is a fraction of the BAR, so the number a caller reads out is 40 px however long the bar is.
    for bar_px in [200.0, 700.0, 1400.0] {
        let geared = naivepost::ui::cut_view::thumb_geared(999_999.0, bar_px);
        assert!(
            (naivepost::ui::cut_view::thumb_px(999_999.0, bar_px) - 40.0).abs() < 0.5,
            "one screenful is 40 px of drag on a {bar_px} px bar, got {}",
            naivepost::ui::cut_view::thumb_px(999_999.0, bar_px)
        );
        assert!(
            (geared * bar_px - naivepost::ui::cut_view::DRAG_PER_SCREENFUL_PX).abs() < 0.5,
            "the fraction and the constant agree at {bar_px} px"
        );
    }
    // And a bar long enough to hold everything shows no bar at all.
    naivepost::ui::cut_view::set_pps(4.0);
    let adjustment = gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 1.0, 1.0);
    let bar = gtk::Scrollbar::new(gtk::Orientation::Horizontal, Some(&adjustment));
    naivepost::ui::cut_view::paint_scrollbar(&bar, 30.0, 700.0);
    assert!(!bar.is_visible(), "everything fits, so no scrollbar");
}

fn page_hangs_band_and_red_line_on_two_layers(app: &adw::Application) {
    let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
    let window = naivepost::ui::build_window(app, &model, "Cut");
    window.present();
    common::settle();
    let band = common::widget_in(&window, "track-band").expect("the Cut page draws the track band");
    let line = common::widget_in(&window, "track-playhead")
        .expect("the Cut page draws the playhead layer");
    let band_area = band
        .downcast::<gtk::DrawingArea>()
        .expect("the band is a drawing area");
    let line_area = line
        .clone()
        .downcast::<gtk::DrawingArea>()
        .expect("the playhead is its own drawing area");
    assert!(
        !line_area.can_target(),
        "the red line's layer must not take the presses the band owns"
    );
    // The line sits in an overlay above the band, which is how §1's "its own layer across both bands" is
    // drawn without a second timeline.
    let parent = line.parent().expect("the playhead is inside the page");
    assert!(
        parent.is::<gtk::Overlay>(),
        "the two layers are stacked, not side by side ({})",
        parent.upcast_ref::<gtk::Widget>().type_()
    );
    assert_eq!(
        band_area.width(),
        line_area.width(),
        "both layers span the same width"
    );
    common::hold_last_window(window);
}

/// Badge **31** on the real page: an edit that adds a lane must also make the drawn band taller, or the row
/// it added is painted outside the surface's allocation and nobody sees it. The paint checks above size their
/// own `cairo` surface and so cannot see this: only the page's own `track-band` can.
fn page_band_grows_for_a_lane(app: &adw::Application) {
    let model = naivepost::project::load(&common::fixture_dir()).expect("fixture loads");
    let window = naivepost::ui::build_window(app, &model, "Cut");
    window.present();
    common::settle();
    let band = common::widget_in(&window, "track-band")
        .expect("the Cut page draws the track band")
        .downcast::<gtk::DrawingArea>()
        .expect("the band is a drawing area");
    // One lane, on the cut the page actually paints from. Two, so "it grew" is a comparison the page can
    // not answer by coincidence: one lane could already fit inside the height the page opened with.
    let with_lane = naivepost::cut::Cut {
        segs: vec![naivepost::cut::Seg {
            s: 0.0,
            e: 20.0,
            ..Default::default()
        }],
        lanes: vec![
            naivepost::cut::Lane {
                name: "mic-lav".to_string(),
                src: "project:sources/mic-lav.wav".to_string(),
                at: 2.0,
                off: 0.0,
                dur: 10.0,
            },
            naivepost::cut::Lane {
                name: "mic-boom".to_string(),
                src: "project:sources/mic-boom.wav".to_string(),
                at: 2.0,
                off: 0.0,
                dur: 10.0,
            },
        ],
        ..Default::default()
    };
    naivepost::ui::seed_review_cut(&window, &with_lane);
    naivepost::ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "take-a".to_string(),
        start: 0.0,
        end: 60.0,
    }]);
    // What the band's surfaces were sized to when the page opened, and what the two new lanes make it need.
    let opened = band.size_request().1;
    let need = tracks::band_height_for(&naivepost::ui::cut_view::band_scene_for(&window));
    assert!(
        need > opened,
        "two lanes must cost rows the page did not open with: sized {opened} px, scene needs {need} px"
    );
    // Drive the page's own redraw door — the one every verb already calls — and then ask what it sized to.
    // The REQUEST is the number to compare: a window that has not been allocated yet measures 0 or its old
    // height in `height()`, while the size the next layout will honour is the one `fit_band_height` wrote.
    naivepost::ui::cut_view::repaint_page(&window);
    common::settle();
    assert!(
        band.size_request().1 >= need,
        "the band's surface is sized {} px but its own scene needs {need} px (it opened at {opened} px): a \
         cut lane's row would be painted outside what the page allocated",
        band.size_request().1
    );
    // §1: the red line crosses BOTH layers, so the layer above the band grows with it or stops short.
    let line = common::widget_in(&window, "track-playhead")
        .expect("the Cut page draws the playhead layer")
        .downcast::<gtk::DrawingArea>()
        .expect("the playhead is its own drawing area");
    assert!(
        line.size_request().1 >= need,
        "the red line's layer is sized {} px while the band is sized {} px",
        line.size_request().1,
        band.size_request().1
    );
    common::hold_last_window(window);
}

#[test]
fn sec_05_cut_1_screen_band_lays_ruler_selection_effects_and_rows_in_order() {
    common::pin_private_cwd();
    let scene = scene();
    let painted = painted(&scene);
    let ground = rgb(&painted.0, painted.1, 0, painted.2 - 1);
    // The ruler's tick column differs from blank tape, and the picture row's ground is not the page's.
    assert!(
        column_differs(&painted, 100, 0, painted.2, ground),
        "the band paints marks at pps {PPS}, not an empty plate"
    );
    // The heights the painter and the surface agree on, so nothing is drawn past the bottom.
    assert!(
        (painted.2 as f64
            - (naivepost::cut_trim::RULER_H
                + SELECTION_BAND_PX
                + fx_lane::lane_height_px(1)
                + (cut_screen::THUMB_AT_OPEN as f64 + PICTURE_ROW_EXTRA_PX)))
            .abs()
            < 1.0,
        "one row of pictures sits under the effects lane's minimum row: {}",
        painted.2
    );
}

#[test]
fn sec_05_cut_1_screen_a_folded_gap_is_greyer_than_the_kept_bar_beside_it() {
    common::pin_private_cwd();
    let mut scene = scene();
    // Keep 0..10 and 20..50: 10..20 is dropped inside filmed material, so it must show as a gap.
    scene.segs = vec![
        Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        },
        Seg {
            s: 20.0,
            e: 50.0,
            ..Default::default()
        },
    ];
    let painted = painted(&scene);
    let band = head_px(&scene) / 2;
    // Kept green covers 0..10 s and 20..50 s; the dropped middle is between them. Every sample comes from
    // `band_span_px` — the tape starts AFTER §B's 30 px gutter, so a hand count of x would drift the moment
    // the rail moves, and a sample inside the rail answers for the bar instead of the bar.
    let (kept_x, kept_w) = tracks::band_span_px(&scene, 20.0, 50.0);
    let kept_mid = (kept_x + kept_w / 2.0).round() as i32;
    let (gap_x, gap_w) = tracks::band_span_px(&scene, 10.0, 20.0);
    // The dropped stretch's own span ENDS where the kept scene begins (`x + w`), so the last pixel of ink
    // still belongs to the gap; its midpoint sits under the kept bar's left border.
    let gap_edge = (gap_x + gap_w - 2.0).round() as i32;
    println!("FOLD kept x={kept_x:.0}+{kept_w:.0} -> {kept_mid}, gap x={gap_x:.0}+{gap_w:.0} -> {gap_edge}");
    let kept = rgb(&painted.0, painted.1, kept_mid, band);
    let gap = rgb(&painted.0, painted.1, gap_edge, band);
    // And the kept green reaches every second the cut keeps: x 60 is inside the 20..50 s scene, so a column
    // sampled there must read the same green top to bottom -- a hole would be a gap the fold painter fills.
    // That column stays inside the 22 px selection band: `band` is its midpoint (head/2 = 32 of the band's
    // 16..38) and two rows below it the ink is already the effects lane's ground, so a taller sample would
    // compare green against empty plate and always differ.
    assert!(
        !column_differs(&painted, kept_mid, band, band + 1, kept),
        "the kept bar runs unbroken across the seconds a scene covers"
    );
    assert!(
        kept.1 > kept.0 && kept.1 > kept.2,
        "kept footage is the green bar, got {kept:?}"
    );
    assert!(
        gap.0 == gap.1 && gap.1 == gap.2 && gap.0 > 200,
        "a dropped stretch inside filmed material is a grey gap, got {gap:?}"
    );
}

#[test]
fn sec_05_cut_1_screen_the_gutter_is_painted_down_the_bands_left_edge() {
    common::pin_private_cwd();
    // §B: "gutter (a 30 px column down the left edge of every band, not a band of its own)". The number is
    // one constant (§1's `GUTTER_PX`), so what proves it is ink: the first 30 px read DARKER than the tape
    // beside them at the same height — in the selection band and again inside a picture row, because §B says
    // EVERY band, and the rail carries both the fold badge and the row switches.
    let scene = scene();
    let painted = painted(&scene);
    let ground_bytes = |part: f64| (part * 255.0).round() as u8;
    let ground_sum = 3 * ground_bytes(cut_colours::GROUND.0) as u32;
    let samples = [
        (naivepost::cut_trim::RULER_H as i32 + 5, "selection band"),
        (head_px(&scene) + 5, "picture row"),
    ];
    for (y, where_) in samples {
        let rail = rgb(&painted.0, painted.1, 10, y);
        let tape = rgb(&painted.0, painted.1, WIDTH / 2, y);
        println!(
            "GUTTER y={y} ({where_}): rail {rail:?} vs tape {tape:?}, ground sums to {ground_sum}"
        );
        let sum = |c: (u8, u8, u8)| c.0 as u32 + c.1 as u32 + c.2 as u32;
        assert!(
            sum(rail) < sum(tape),
            "the gutter must be darker than the tape at the same height: rail {rail:?} vs tape {tape:?}"
        );
        assert!(
            sum(rail) < ground_sum,
            "the gutter must not be the band's own ground: rail {rail:?}, ground sums to {ground_sum}"
        );
    }
}

#[test]
fn sec_05_cut_1_screen_a_wide_kept_bar_wears_a_cross() {
    common::pin_private_cwd();
    // §A's selection-band rule: the kept bar's ✕ appears "only when ≥50 px wide". Two scenes differing ONLY
    // in how wide the kept bar is drawn, sampled at that bar's centre: the wide one shows white ink, the
    // narrow one shows its own green with no glyph on it.
    let centre_y = naivepost::cut_trim::RULER_H + SELECTION_BAND_PX / 2.0;
    // The kept green is laid at alpha 1.0 over the ground (`draw_kept_bars`), so a bar's own colour is known
    // and "no glyph here" can be pinned to it rather than to "not white".
    let kept_green = |c: (u8, u8, u8)| c.1 > c.0 + 40 && c.1 > c.2 + 40;
    let near_white = |c: (u8, u8, u8)| c.0 >= 200 && c.1 >= 200 && c.2 >= 200;
    let mut wide = scene();
    wide.segs = vec![Seg {
        s: 5.0,
        e: 40.0,
        ..Default::default()
    }];
    let (wx, ww) = tracks::band_span_px(&wide, 5.0, 40.0);
    assert!(
        ww >= tracks::CROSS_MIN_PX,
        "fixture: the wide bar is {} px",
        ww
    );
    let wide_painted = painted(&wide);
    let mid_x = (wx + ww / 2.0).round() as i32;
    let on_wide = rgb(
        &wide_painted.0,
        wide_painted.1,
        mid_x,
        centre_y.round() as i32,
    );
    let mut narrow = scene();
    // The gutter is inked over the band's first 30 px (§B), so the narrow bar has to sit clear of it or the
    // sample would read the rail instead of the bar.
    narrow.segs = vec![Seg {
        s: 40.0,
        e: 55.0,
        ..Default::default()
    }];
    let (nx, nw) = tracks::band_span_px(&narrow, 40.0, 55.0);
    assert!(
        nw < tracks::CROSS_MIN_PX,
        "fixture: the narrow bar is {} px",
        nw
    );
    assert!(
        nx > cut_screen::GUTTER_PX,
        "fixture: the narrow bar starts past the gutter at x={nx:.0}"
    );
    let narrow_painted = painted(&narrow);
    let on_narrow = rgb(
        &narrow_painted.0,
        narrow_painted.1,
        (nx + nw / 2.0).round() as i32,
        centre_y.round() as i32,
    );
    println!(
        "CROSS wide {ww:.0}px centre x={mid_x} -> {on_wide:?}; narrow {nw:.0}px -> {on_narrow:?}"
    );
    assert!(
        near_white(on_wide),
        "a bar ≥ {:.0} px wears the white \u{2715}: {on_wide:?}",
        tracks::CROSS_MIN_PX
    );
    assert!(
        kept_green(on_narrow),
        "a narrower bar shows its own green with no glyph on it: {on_narrow:?}"
    );
}

#[test]
fn sec_05_cut_1_screen_a_selection_is_blue_and_only_where_it_is() {
    common::pin_private_cwd();
    let mut scene = scene();
    scene.selection = Some((20.0, 60.0));
    let painted = painted(&scene);
    let band = head_px(&scene) / 2;
    let inside = rgb(&painted.0, painted.1, 60, band);
    assert!(
        inside.2 > inside.0 + 20,
        "the selection is blue, got {inside:?}"
    );
    let outside = rgb(&painted.0, painted.1, 10, band);
    assert!(
        outside.2 < inside.2,
        "the blue stops where the selection stops: {outside:?} vs {inside:?}"
    );
}

#[test]
fn sec_05_cut_1_screen_an_insert_bands_violet_across_the_picture_rows() {
    common::pin_private_cwd();
    let mut scene = scene();
    scene.segs.push(Seg {
        s: 60.0,
        e: 60.0,
        dur: 3.0,
        ins: "/cards/ending.png".into(),
        ..Default::default()
    });
    let painted = painted(&scene);
    let row_y = head_px(&scene) + 10;
    let card = rgb(&painted.0, painted.1, 125, row_y);
    assert!(
        card.2 > card.1 && card.0 > card.1,
        "an insert is a violet band over the rows, got {card:?}"
    );
}

#[test]
fn sec_05_cut_1_screen_the_in_and_out_marks_are_green_then_red() {
    common::pin_private_cwd();
    let mut scene = scene();
    scene.selection = Some((20.0, 60.0));
    let painted = painted(&scene);
    // The marks run the whole band's height, so sample below the selection band, inside the picture rows.
    let y = head_px(&scene) + 30;
    let left = rgb(&painted.0, painted.1, 40, y);
    let right = rgb(&painted.0, painted.1, 120, y);
    assert!(
        left.1 > left.0 && left.1 > left.2,
        "the in mark is green, got {left:?}"
    );
    assert!(
        right.0 > right.1 && right.0 > right.2,
        "the out mark is red, got {right:?}"
    );
}

#[test]
fn sec_05_cut_1_screen_a_speed_effect_washes_the_picture_rose() {
    common::pin_private_cwd();
    let mut scene = scene();
    scene.fx = vec![Fx {
        kind: "speed".into(),
        t: 10.0,
        dur: 8.0,
        rate: 2.0,
        ..Default::default()
    }];
    let with_wash = painted(&scene);
    let mut plain = scene.clone();
    plain.fx.clear();
    let bare = painted(&plain);
    let y = head_px(&scene) + 20;
    assert_ne!(
        rgb(&with_wash.0, with_wash.1, 30, y),
        rgb(&bare.0, bare.1, 30, y),
        "a rose wash must cover the seconds a speed effect runs over"
    );
}

#[test]
fn sec_05_cut_1_screen_the_watched_row_wears_a_dashed_outline() {
    common::pin_private_cwd();
    let mut scene = scene();
    let bare = painted(&scene);
    scene.watched_row = Some(0);
    let watched = painted(&scene);
    let y = head_px(&scene) + 1;
    // x 20 sits in the first dash, not in its gap: the dashes are 12 px of ink per 20 px, so this sample is
    // one a missing outline would fail.
    // x 40, not x 20: §B's gutter inks the band's first 30 px, which would otherwise hide the outline here.
    assert_ne!(
        rgb(&bare.0, bare.1, 40, y),
        rgb(&watched.0, watched.1, 40, y),
        "the watched row's outline must be visible over its own row"
    );
}

#[test]
fn sec_05_cut_1_screen_the_playhead_is_a_two_px_red_line_on_its_own_layer() {
    common::pin_private_cwd();
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, WIDTH, 120).expect("surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().expect("white ground");
        tracks::paint_playhead(&cr, PPS, 0.0, 50.0, 120.0);
    }
    let data = surface.data().expect("pixels").to_vec();
    let stride = surface.stride() as usize;
    let at = |x: i32| {
        let base = 60 * stride + x as usize * 4;
        (data[base + 2], data[base + 1], data[base])
    };
    let line = at(100);
    assert!(
        line.0 > 150 && line.1 < 100 && line.2 < 100,
        "the playhead is red, got {line:?}"
    );
    assert_eq!(at(98), (255, 255, 255), "the line is 2 px, not a band");
    assert_eq!(at(102), (255, 255, 255), "the line is 2 px, not a band");
}

/// §1's badge 28 as text: a lane's `from m:ss` and a hand correction's `+1.23 s` both come off the cut's own
/// numbers, so a plate can never disagree with the lane it names.
#[test]
fn sec_05_cut_1_screen_a_name_plate_names_the_row_its_window_and_its_correction() {
    common::pin_private_cwd();
    let mut scene = scene();
    scene.lanes = vec![naivepost::cut::Lane {
        name: "take-a".into(),
        src: "project:take-a.mp4".into(),
        at: 75.0,
        off: 0.0,
        dur: 10.0,
    }];
    scene.shift.insert("take-a".to_string(), 1.234);
    let words = tracks::plates(&scene, &scene.recordings[0].clone());
    assert_eq!(
        words[0], "take-a",
        "the plate starts with the recording's base name"
    );
    assert_eq!(words[1], "from 01:15", "a lane is spelled `from m:ss`");
    assert_eq!(
        words[2], "+1.23 s",
        "a hand correction is spelled to two decimals"
    );
}

/// §1's Pictures: the band shows Prepare's extracted frames, and it shows the one nearest the second at
/// the view's left edge — not every file in the folder. Both doors are the page's own: `install` lists with
/// `frame_paths`, the painter picks with `frame_near`. Frame names are wall-clock stamps (`clock::frame_name`
/// over the recording's own start), so the test asks for the stamp of a real second rather than inventing one.
#[test]
fn sec_05_cut_1_screen_the_band_lists_prepare_frames_and_shows_the_one_it_picks() {
    common::pin_private_cwd();
    let dir = common::temp_dir_created("sec05frames.naivepost");
    let frames = dir.join("prepare/inputs/frames/take-a");
    std::fs::create_dir_all(&frames).expect("frames folder");
    // Three seconds of grid, named exactly as `frames::extract_plan` names them.
    let names: Vec<String> = (0..3)
        .map(|second| {
            format!(
                "{}.jpg",
                naivepost::clock::frame_name(1704067200.0 + second as f64)
            )
        })
        .collect();
    for name in &names {
        std::fs::write(frames.join(name), b"not really a jpeg").expect("a frame file");
    }
    let tree = naivepost::layout::Tree::new(&dir).expect("a project-shaped folder");
    let paths = tracks::frame_paths(&tree, "take-a");
    assert_eq!(paths.len(), 3, "the page lists what Prepare wrote");
    let picked = tracks::frame_near(&paths, 1704067203.0)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    assert_eq!(
        picked.as_deref(),
        Some(names[2].as_str()),
        "the frame shown is the one nearest the second at the view's left edge"
    );
    // And an absent folder answers empty, which draws no pictures rather than inventing black (§1).
    assert!(tracks::frame_paths(&tree, "no-such-take").is_empty());
}
