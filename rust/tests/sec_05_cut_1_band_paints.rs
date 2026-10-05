//! §05-cut#1-screen — the Cut page's band really draws the §1 marks, measured as pixels rather than by eye.
//!
//! Every check paints a small `Scene` onto an `ImageSurface`, reads bytes back and compares a sample
//! INSIDE the mark with a sample beside it. A wrong rule (a colour applied to the wrong span, a band drawn
//! at the wrong row) changes a number here; a CSS class always parses, so a stylesheet could never tell us.

use gtk4::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};
#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use naivepost::{
    cut::{Fx, Seg},
    cut_screen, fx_lane, timeline::Recording,
    ui::cut_tracks::{self as tracks, Scene, PICTURE_ROW_EXTRA_PX, SELECTION_BAND_PX},
};

const PPS: f64 = 2.0;
const WIDTH: i32 = 300;
/// The band's three fixed rows above the pictures, as the painter lays them.
fn head_px(scene: &Scene) -> i32 {
    (naivepost::cut_trim::RULER_H
        + SELECTION_BAND_PX
        + fx_lane::lane_height_px(fx_lane::row_count(&scene.fx))) as i32
}

/// A scene with two takes, one row, nothing else set: each check adds the one thing it is about.
fn scene() -> Scene {
    Scene {
        pps: PPS,
        origin: 0.0,
        segs: vec![Seg { s: 0.0, e: 20.0, ..Default::default() }],
        fx: Vec::new(),
        recordings: vec![Recording { base: "take-a".into(), start: 0.0, end: 50.0 }],
        folds: Vec::new(),
        selection: None,
        watched_row: None,
        row_count: 1,
        shift: Default::default(),
        lanes: Vec::new(),
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
        tracks::paint_at(&cr, scene, WIDTH as f64, height as f64, tracks::THUMB_DEFAULT_PX);
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
            app.quit();
        });
        let _ = app.run_with_args::<String>(&[]);
    });
}

static SCROLLBAR_RAN: AtomicBool = AtomicBool::new(false);
static LAYERS_RAN: AtomicBool = AtomicBool::new(false);

#[test]
fn sec_05_cut_1_screen_the_scrollbar_gears_one_screenful_at_40_px() {
    run_widget_round();
    assert!(SCROLLBAR_RAN.load(Ordering::SeqCst), "the round ran");
}

/// The band and the red line as widgets: two separate surfaces, stacked so the line goes OVER the marks
/// without either taking the other's space — and the overlay must not swallow presses, or F2.8's grips stop
/// being grabbable. Driven through a real Cut-page window, not a hand-built box.
#[test]
fn sec_05_cut_1_screen_the_page_hangs_the_band_and_the_red_line_on_two_layers() {
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
    let line =
        common::widget_in(&window, "track-playhead").expect("the Cut page draws the playhead layer");
    let band_area = band.downcast::<gtk::DrawingArea>().expect("the band is a drawing area");
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
    assert_eq!(band_area.width(), line_area.width(), "both layers span the same width");
    common::hold_last_window(window);
}

#[test]
fn sec_05_cut_1_screen_band_lays_ruler_selection_effects_and_rows_in_order() {
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
    let mut scene = scene();
    // Keep 0..10 and 20..50: 10..20 is dropped inside filmed material, so it must show as a gap.
    scene.segs = vec![
        Seg { s: 0.0, e: 10.0, ..Default::default() },
        Seg { s: 20.0, e: 50.0, ..Default::default() },
    ];
    let painted = painted(&scene);
    let band = head_px(&scene) / 2;
    // Kept green covers 0..10 s (x 0..20) and 20..50 s (x 40..100); the dropped middle is x 20..40.
    let kept = rgb(&painted.0, painted.1, 10, band);
    let gap = rgb(&painted.0, painted.1, 28, band);
    // And the kept green reaches every second the cut keeps: x 60 is inside the 20..50 s scene, so a column
    // sampled there must read the same green top to bottom -- a hole would be a gap the fold painter fills.
    assert!(
        !column_differs(&painted, 60, band, band + 2, kept),
        "the kept bar runs unbroken across the seconds a scene covers"
    );
    assert!(kept.1 > kept.0 && kept.1 > kept.2, "kept footage is the green bar, got {kept:?}");
    assert!(
        gap.0 == gap.1 && gap.1 == gap.2 && gap.0 > 200,
        "a dropped stretch inside filmed material is a grey gap, got {gap:?}"
    );
}

#[test]
fn sec_05_cut_1_screen_a_selection_is_blue_and_only_where_it_is() {
    let mut scene = scene();
    scene.selection = Some((20.0, 60.0));
    let painted = painted(&scene);
    let band = head_px(&scene) / 2;
    let inside = rgb(&painted.0, painted.1, 60, band);
    assert!(inside.2 > inside.0 + 20, "the selection is blue, got {inside:?}");
    let outside = rgb(&painted.0, painted.1, 10, band);
    assert!(outside.2 < inside.2, "the blue stops where the selection stops: {outside:?} vs {inside:?}");
}

#[test]
fn sec_05_cut_1_screen_an_insert_bands_violet_across_the_picture_rows() {
    let mut scene = scene();
    scene.segs.push(Seg { s: 60.0, e: 60.0, dur: 3.0, ins: "/cards/ending.png".into(), ..Default::default() });
    let painted = painted(&scene);
    let row_y = head_px(&scene) + 10;
    let card = rgb(&painted.0, painted.1, 125, row_y);
    assert!(card.2 > card.1 && card.0 > card.1, "an insert is a violet band over the rows, got {card:?}");
}

#[test]
fn sec_05_cut_1_screen_the_in_and_out_marks_are_green_then_red() {
    let mut scene = scene();
    scene.selection = Some((20.0, 60.0));
    let painted = painted(&scene);
    // The marks run the whole band's height, so sample below the selection band, inside the picture rows.
    let y = head_px(&scene) + 30;
    let left = rgb(&painted.0, painted.1, 40, y);
    let right = rgb(&painted.0, painted.1, 120, y);
    assert!(left.1 > left.0 && left.1 > left.2, "the in mark is green, got {left:?}");
    assert!(right.0 > right.1 && right.0 > right.2, "the out mark is red, got {right:?}");
}

#[test]
fn sec_05_cut_1_screen_a_speed_effect_washes_the_picture_rose() {
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
    let mut scene = scene();
    let bare = painted(&scene);
    scene.watched_row = Some(0);
    let watched = painted(&scene);
    let y = head_px(&scene) + 1;
    // x 20 sits in the first dash, not in its gap: the dashes are 12 px of ink per 20 px, so this sample is
    // one a missing outline would fail.
    assert_ne!(
        rgb(&bare.0, bare.1, 20, y),
        rgb(&watched.0, watched.1, 20, y),
        "the watched row's outline must be visible over its own row"
    );
}

#[test]
fn sec_05_cut_1_screen_the_playhead_is_a_two_px_red_line_on_its_own_layer() {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, WIDTH, 120).expect("surface");
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
    assert!(line.0 > 150 && line.1 < 100 && line.2 < 100, "the playhead is red, got {line:?}");
    assert_eq!(at(98), (255, 255, 255), "the line is 2 px, not a band");
    assert_eq!(at(102), (255, 255, 255), "the line is 2 px, not a band");
}

/// §1's badge 28 as text: a lane's `from m:ss` and a hand correction's `+1.23 s` both come off the cut's own
/// numbers, so a plate can never disagree with the lane it names.
#[test]
fn sec_05_cut_1_screen_a_name_plate_names_the_row_its_window_and_its_correction() {
    let mut scene = scene();
    scene.lanes = vec![naivepost::cut::Lane {
        name: "take-a".into(),
        src: "project:take-a.mp4".into(),
        at: 75.0,
        off: 0.0,
        dur: 10.0,
    }];
    scene
        .shift
        .insert("take-a".to_string(), 1.234);
    let words = tracks::plates(&scene, &scene.recordings[0].clone());
    assert_eq!(words[0], "take-a", "the plate starts with the recording's base name");
    assert_eq!(words[1], "from 01:15", "a lane is spelled `from m:ss`");
    assert_eq!(words[2], "+1.23 s", "a hand correction is spelled to two decimals");
}

/// §1's Pictures: the band shows Prepare's extracted frames, and it shows the one nearest the second at
/// the view's left edge — not every file in the folder. Both doors are the page's own: `install` lists with
/// `frame_paths`, the painter picks with `frame_near`. Frame names are wall-clock stamps (`clock::frame_name`
/// over the recording's own start), so the test asks for the stamp of a real second rather than inventing one.
#[test]
fn sec_05_cut_1_screen_the_band_lists_prepare_frames_and_shows_the_one_it_picks() {
    let dir = common::temp_dir_created("sec05frames.naivepost");
    let frames = dir.join("prepare/inputs/frames/take-a");
    std::fs::create_dir_all(&frames).expect("frames folder");
    // Three seconds of grid, named exactly as `frames::extract_plan` names them.
    let names: Vec<String> = (0..3)
        .map(|second| format!("{}.jpg", naivepost::clock::frame_name(1704067200.0 + second as f64)))
        .collect();
    for name in &names {
        std::fs::write(frames.join(name), b"not really a jpeg").expect("a frame file");
    }
    let tree = naivepost::layout::Tree::new(&dir).expect("a project-shaped folder");
    let paths = tracks::frame_paths(&tree, "take-a");
    assert_eq!(paths.len(), 3, "the page lists what Prepare wrote");
    let picked = tracks::frame_near(&paths, 1704067203.0)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    assert_eq!(picked.as_deref(), Some(names[2].as_str()), "the frame shown is the one nearest the second at the view's left edge");
    // And an absent folder answers empty, which draws no pictures rather than inventing black (§1).
    assert!(tracks::frame_paths(&tree, "no-such-take").is_empty());
}
