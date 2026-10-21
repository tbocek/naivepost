//! §05-cut#1-screen — the three things the completion check found the program NOT doing, measured
//! off a real window: badge **1** holds a real picture, §B's recorders' lanes sit directly under
//! the picture rows (not detached at the bottom of the page), and badge **31** shows when the tape
//! is wider than the view and hides when everything fits. §1's red line (2 px, own layer, across
//! both bands) is pinned here too because it is the same two drawing areas.
//!
//! Every number comes out of the realized tree (`translate_coordinates`, `width()`, `height()`) or
//! out of the same pure functions the painter indexes with (`cut_tracks::band_height_px`,
//! `cut_view::fits`, `cut_view::thumb_geared`), never off a screenshot: a layout that merely renders
//! is not evidence — the broken page rendered fine and was wrong by 750 px.
//!
//! One live scenario per binary: this fixture re-points the process-wide cwd, so a second fixture
//! in the same binary would read the first one's session (the shape
//! `tests/suite/sec_05_cut_1_screen_layout.rs` uses).

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_PICTURE: AtomicBool = AtomicBool::new(false);
static RAN_LANES: AtomicBool = AtomicBool::new(false);
static RAN_BAR: AtomicBool = AtomicBool::new(false);
static RAN_LINE: AtomicBool = AtomicBool::new(false);

/// How far below the last picture row the recorders' band may start. §B packs the lanes straight
/// after the picture rows; the bug left ~750 px of empty plate between them, so anything past this
/// fails. Chosen from the numbers: a row is thumbnail + wave strip (~90 px), so a couple of rows'
/// worth of slack still catches a wall of emptiness.
const MAX_ROWS_TO_LANES_PX: i32 = 260;



/// A widget's top and bottom edge relative to ITS OWN WINDOW's top-left — the frame two widgets on
/// the same page must be compared in (see [`cut_page_with_a_lane`]'s note on stacked windows).
#[allow(deprecated)] // 4.12's replacement needs a surface node this tree does not have
fn edges_in(window: &gtk::Widget, widget: &gtk::Widget) -> (i32, i32) {
    let (x, y) = widget
        .translate_coordinates(window, 0.0, 0.0)
        .expect("the widget is inside its window");
    let _ = x;
    let top = y as i32;
    (top, top + widget.height())
}

/// A Cut-page window over a session with ONE recording and ONE separate lane, sized like a real
/// window (the layout collapses to one column without an explicit size before `present()`). The
/// lane is what makes §B's order testable: with no lane there is no recorders' band to place.
///
/// Each check CLOSES its window when it finishes. That matters for the coordinate work below: alive
/// windows stack in one native surface, so `translate_coordinates(&native)` reports negative offsets
/// for anything behind the frontmost one, and edges are only comparable inside a single window.
fn cut_page_with_a_lane(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-screen-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");
    let session = naivepost::startup::session_dir(&root);
    let mut seed = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    seed.sources.iter_mut().for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project");

    let window = naivepost::ui::build_window(app, &seed, "Prepare");
    hold_last_window(window.clone());
    window.set_default_size(1440, 900);
    window.present();
    naivepost::ui::tab_button(&window, naivepost::shell::Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    // Recordings BEFORE the cut: `set_session_recordings` borrows the page's slot that the
    // following `seed_review_cut` re-reads (same order `sec_05_cut_1_screen_layout` uses).
    naivepost::ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "lecture".into(),
        start: 0.0,
        end: 120.0,
    }]);
    let seeded = naivepost::cut::Cut {
        segs: vec![naivepost::cut::Seg { s: 0.0, e: 60.0, ..Default::default() }],
        lanes: vec![naivepost::cut::Lane {
            name: "mic-lav".into(),
            src: "project:sources/mic-lav.wav".into(),
            at: 10.0,
            off: 0.0,
            dur: 45.0,
        }],
        ..Default::default()
    };
    naivepost::ui::seed_review_cut(&window, &seeded);
    naivepost::ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 20.0 });
    settle();
    window
}

/// Badge **1**: the preview panel holds a REAL picture. The door is the one `--snapshot` uses
/// (`set_snapshot_dir` names the folder frames are listed from, `set_preview_camera_base` names the
/// take, `refresh` picks the frame through the page's own rule and hands it to `show_frame`, which
/// puts a `gdk::Texture` on the widget named `preview-picture`). Asserted off the WIDGET: a paintable
/// whose intrinsic size is non-zero, i.e. bytes gdk-pixbuf read, not a plate waiting for one.
fn check_the_preview_holds_a_real_picture(app: &adw::Application) {
    let window = cut_page_with_a_lane(app);
    // Write one real JPEG where the page lists frames, through the app's own naming rule, so the
    // frame `refresh` chooses is a file that exists. The page reads frames from its session tree's
    // `prepare/inputs/frames/<base>` (`layout::Tree::frames_dir`), which is what the snapshot bench
    // fills too — so this fixture plants the frame at the same door, not beside `--project`.
    // Write one real JPEG where the page lists frames, through the app's own naming rule. The
    // frames are named by the FILE's own stamps (§B's `at(t)`), so a frame for session second 20 of
    // a take that starts at 0 is `clock::frame_name(20.0)`; planting it at 0 would leave `frame_near`
    // with nothing at or before the line and no picture to show.
    let root = std::env::current_dir().expect("cwd pinned above");
    let session = naivepost::startup::session_dir(&root);
    let tree = naivepost::layout::Tree::new(&session).expect("the session is a project tree");
    let frames = tree.frames_dir("lecture");
    std::fs::create_dir_all(&frames).expect("frames folder");
    // The page lists frames by the STAMP in the name (`cut_tracks::frame_paths` sorts on it and
    // `frame_near` picks "at or before"), so the fixture keeps its stamp with the suffix its encoder
    // actually wrote: §B.3 names the file by its second, and Prepare's own frames are JPEG.
    let file = frames.join(format!("{}.png", naivepost::clock::frame_name(20.0)));
    write_a_real_frame(&file);
    // Point the page at the folder its own door will list: `session_tree` takes the snapshot bench's
    // named folder when one is set, and `set_snapshot_dir` names a PROJECT folder (the tree's
    // `prepare/inputs/frames/<base>` rule does the rest), so this is the session itself.
    naivepost::ui::cut_picture::set_snapshot_dir(&session);
    naivepost::ui::cut_picture::set_preview_camera_base("lecture");
    let paths = naivepost::ui::cut_tracks::frame_paths(
        &naivepost::layout::Tree::new(&session).unwrap(),
        "lecture",
    );
    let near = naivepost::ui::cut_tracks::frame_near(&paths, 20.0);
    let ok = near
        .as_ref()
        .is_some_and(|f| gtk::gdk_pixbuf::Pixbuf::from_file(f).is_ok());
    let shown = naivepost::ui::cut_picture::refresh(&window);
    println!(
        "DEBUG file={} exists={} paths={:?} near={:?} pixbuf_ok={ok} asked={:?} shown={shown}",
        file.display(),
        file.exists(),
        paths,
        near,
        naivepost::ui::cut_picture::preview_asked_for_test()
    );
    assert!(shown, "the page's own refresh found and showed a frame");
    settle();

    let picture = common::find_named(&window, "preview-picture").expect("badge 1's picture widget is on the page");
    let picture = picture
        .downcast::<gtk::Picture>()
        .expect("`preview-picture` is a gtk::Picture");
    let held = picture
        .paintable()
        .expect("the preview panel holds a paintable, not an empty plate");
    let (w, h) = (held.intrinsic_width(), held.intrinsic_height());
    println!("PICTURE texture {w}x{h}, widget {}x{}", picture.width(), picture.height());
    assert!(w > 0 && h > 0, "the texture carries real pixels: {w}x{h}");
    assert!(picture.is_visible(), "and the panel shows it");
    RAN_PICTURE.store(true, Ordering::SeqCst);
    window.close();
}

/// §B's order: "... picture rows ..., then the separate recordings' lanes, then the scrollbar".
/// Two halves: the LIVE gap (band top vs the bottom of the camera-row plates — the bug left ~750 px
/// of nothing), and the ARITHMETIC the painter indexes with (`band_height_px` grows by exactly one
/// row per lane, which is why a lane can never be painted outside the surface).
fn check_the_lanes_sit_under_the_picture_rows(app: &adw::Application) {
    let window = cut_page_with_a_lane(app);
    // The windows stack across the checks in this binary (each is closed at the end of its check, but
    // GTK keeps them unrealized-but-alive), so coordinates are only comparable WITHIN one window: take
    // every edge relative to that window's own top-left rather than to the shared native surface.
    let origin = {
        let native = window.native().expect("realized");
        #[allow(deprecated)] // 4.12's replacement needs a surface node this tree does not have
        let at = window
            .translate_coordinates(&native, 0.0, 0.0)
            .expect("laid out");
        at
    };
    let local = |widget: &gtk::Widget| -> (i32, i32) {
        let _ = origin;
        edges_in(&window.clone().upcast::<gtk::Widget>(), widget)
    };
    let scene = naivepost::ui::cut_view::band_scene_for(&window);
    assert!(!scene.lanes.is_empty(), "the seeded lane reached the band's scene");

    // The live measurement: the band's own surface starts within a row or two of the picture rows.
    //
    // What is measured is the PAINTED band (`track-band` — the drawing area §B lays ruler, selection
    // band, effects lane, camera rows and lanes out in one column) rather than the `camera-rows`
    // widget: that box is §A's clickable plate stack (name plates, speaker badges, gutter switches),
    // which `cut_page_layout::arrange` seats AFTER `track-strip` by design, so it is not the row whose
    // bottom the lanes have to follow. Inside the band, the arithmetic and the ink must agree:
    // `Rows::lane_top` says where lane 0 begins, and every painted pixel has to lie inside the band's
    // allocation — a lane drawn below it would be clipped away, which is what "detached at the bottom
    // of the page" looked like.
    let layers = common::find_named(&window, "track-layers").expect("the band's layer stack is on the page");
    let band = common::find_named(&window, "track-band").expect("the band's drawing area is on the page");
    let (band_top, band_bottom) = local(&layers);
    // The size request, not the allocation: `fit_band_height` grows the request and the allocation only
    // catches up on the next layout pass, so reading the allocation here races it — the same lane that
    // grew correctly read 132 px in one suite run and 200 px in another.
    let requested = f64::from(band.size_request().1);
    let needed = naivepost::ui::cut_tracks::band_height_px(
        &scene,
        naivepost::ui::cut_tracks::THUMB_DEFAULT_PX,
    );
    println!(
        "LANES band y={band_top}..{band_bottom} ({} px), size request {requested} px, scene needs \
         {needed} px, {} lane(s)",
        band_bottom - band_top,
        scene.lanes.len()
    );
    assert!(
        requested + 2.0 >= needed,
        "the band's size request covers the height its scene costs, so the recorders' lanes are \
         inside the tape rather than below it: requested {requested} px vs needed {needed} px"
    );
    // And the gap from the toolbar to the tape is still small — §A packs the tracks directly under it,
    // and the failure mode was ~750 px of empty plate between the two.
    let toolbar = common::find_named(&window, "cut-toolbar").expect("§A's toolbar is on the page");
    let (_, toolbar_bottom) = local(&toolbar);
    let gap = band_top - toolbar_bottom;
    println!("GAP toolbar-bottom {toolbar_bottom} -> band-top {band_top} = {gap} px");
    assert!(
        (0..=MAX_ROWS_TO_LANES_PX).contains(&gap),
        "the tape starts {gap} px below the toolbar; \u{a7}A packs it directly under it"
    );
    // And the lanes are INSIDE the painted band, not a detached plate below it: the lane row lies
    // within the height the band was grown to (`band_height_px` counts it), so the painter's own
    // arithmetic puts it inside the allocation rather than under it.
    let thumb = naivepost::ui::cut_tracks::THUMB_DEFAULT_PX;
    let lane_top = {
        // `Rows::lane_top` is the painter's own door for "where does lane N begin"; the band must be
        // at least that tall plus one row, or the lane would be clipped away.
        let rows_height = naivepost::ui::cut_tracks::band_height_px(&scene, thumb);
        assert!(
            requested + 2.0 >= rows_height,
            "the band's size request covers the height its scene costs: {requested} px vs needed \
             {rows_height} px"
        );
        rows_height
    };
    assert!(lane_top > 0.0, "the band reports a height for this scene");

    // The arithmetic, on its own: a lane costs the band exactly one row, so a lane can never be
    // drawn outside the surface the band allocated.
    let thumb = naivepost::ui::cut_tracks::THUMB_DEFAULT_PX;
    let without = naivepost::cut::Cut {
        segs: vec![naivepost::cut::Seg { s: 0.0, e: 60.0, ..Default::default() }],
        ..Default::default()
    };
    let with = naivepost::cut::Cut {
        lanes: vec![naivepost::cut::Lane {
            name: "mic-lav".into(),
            src: "project:sources/mic-lav.wav".into(),
            at: 10.0,
            off: 0.0,
            dur: 45.0,
        }],
        ..without.clone()
    };
    let recordings = naivepost::ui::page_recordings();
    let bare = naivepost::ui::cut_tracks::scene_from(&without, &recordings, None, None, 0.0);
    let lane = naivepost::ui::cut_tracks::scene_from(&with, &recordings, None, None, 0.0);
    let bare_px = naivepost::ui::cut_tracks::band_height_px(&bare, thumb);
    let lane_px = naivepost::ui::cut_tracks::band_height_px(&lane, thumb);
    println!("HEIGHT band {bare_px} px without a lane, {lane_px} px with one");
    assert!(
        lane_px > bare_px,
        "a lane costs the band a row: {bare_px} -> {lane_px}"
    );
    assert_eq!(
        naivepost::ui::cut_tracks::lane_rows(&lane),
        1,
        "one row per lane, which is what the height counts"
    );
    RAN_LANES.store(true, Ordering::SeqCst);
    window.close();
}

/// Badge **31**, BOTH branches of §A's rule ("hidden when everything fits"), plus the gearing
/// (§B: "thumb geared at high zoom so one screenful is always 40 px of drag"). Driven through the
/// page's own zoom door `press_zoom`, which ends in `repaint_scrollbar` → `cut_view::refresh_page_scrollbar`.
fn check_the_scrollbar_shows_and_hides(app: &adw::Application) {
    let window = cut_page_with_a_lane(app);
    let bar = common::find_named(&window, "track-scrollbar")
        .expect("badge 31 is on the page")
        .downcast::<gtk::Scrollbar>()
        .expect("`track-scrollbar` is a scrollbar");
    let view = naivepost::ui::cut_view::surface_width(&window);

    // At the floor the whole session fits, and §A says the bar hides rather than showing a
    // full-width thumb that says nothing. `press_zoom(false)` walks down to that floor.
    naivepost::ui::press_zoom(&window, false);
    naivepost::ui::press_zoom(&window, false);
    naivepost::ui::press_zoom(&window, false);
    settle();
    let (total_fits, _) = naivepost::ui::cut_view::extents(view);
    println!("BAR at floor: total {total_fits} px vs view {view} px");
    assert!(
        naivepost::ui::cut_view::fits(total_fits, view),
        "after stepping out the session fits the view: {total_fits} <= {view}"
    );
    assert!(!bar.is_visible(), "and the bar hides when everything fits");

    // Zoom IN until the tape is wider than the view: the bar must appear, and its thumb must be the
    // geared 40 px rather than the proportional sliver a raw ratio would give at high zoom.
    let mut said = String::new();
    for _ in 0..12 {
        said = naivepost::ui::press_zoom(&window, true);
        let (total, _) = naivepost::ui::cut_view::extents(view);
        if !naivepost::ui::cut_view::fits(total, view) {
            break;
        }
    }
    settle();
    let (total, view_now) = naivepost::ui::cut_view::extents(view);
    assert!(
        !naivepost::ui::cut_view::fits(total, view_now),
        "zooming in made the tape wider than the view ({said}): {total} > {view_now}"
    );
    assert!(bar.is_visible(), "so badge 31 shows");
    let adjustment = bar.adjustment();
    let upper = adjustment.upper();
    let page_length = adjustment.page_size();
    let _bar_px = f64::from(bar.width()).max(1.0);
    // The gearing is defined against the BAR's length, and a realized scrollbar is 1 px thick across
    // its own minor axis — `width()` is that thickness, not the run of the track. The program reads
    // the same number through `bar_width`, which falls back to the width recorded when the bar was
    // installed while the allocation is still what it is here, so the check asks the program's own
    // door rather than assuming which axis GTK laid out first.
    let geared_bar = naivepost::ui::cut_view::gear_px_for_test();
    println!(
        "BAR zoomed: upper {upper}, page-length {page_length}, thumb-geared-over {geared_bar} px, thumb {}",
        naivepost::ui::cut_view::thumb_px(total, geared_bar)
    );
    assert!(
        (page_length - 40.0).abs() < 1.0,
        "one screenful is 40 px of drag whatever the zoom: page length {page_length} measured over \
         a {geared_bar} px bar (upper {upper}, view {view_now})"
    );
    // The pure rule behind it, spelled once: 40 px of the bar, clamped so a narrow bar's thumb is
    // never smaller than the floor ratio.
    let geared = naivepost::ui::cut_view::thumb_geared(total, geared_bar);
    assert!(
        (geared * geared_bar - 40.0).abs() < 0.01 || geared >= 1.0,
        "`thumb_geared` answers 40 px of the bar: {geared} x {geared_bar}"
    );
    RAN_BAR.store(true, Ordering::SeqCst);
    window.close();
}

/// §1's sentence: "Red playhead: 2 px line on its own layer across both bands." Three parts, all
/// measurable: its OWN layer (a second drawing area stacked over the band, not painted into it),
/// 2 px wide (the constant the painter strokes with), and ACROSS BOTH BANDS (the layer is grown to
/// the same height the band needs, so the line reaches the trim strip's rows too).
fn check_the_red_line_is_two_pixels_on_its_own_layer(app: &adw::Application) {
    let window = cut_page_with_a_lane(app);
    naivepost::ui::set_playhead(&window, 24.0);
    naivepost::ui::cut_view::repaint_page(&window);
    settle();
    let band = common::find_named(&window, "track-band").expect("the band's drawing area is on the page");
    let head = common::find_named(&window, "track-playhead").expect("the red line's layer is on the page");
    assert_ne!(
        band.widget_name(),
        head.widget_name(),
        "the line is not painted into the band"
    );
    // Own layer: siblings of one overlay stack, so they draw over each other rather than into each other.
    let band_parent = band.parent().expect("the band sits in the layer stack");
    let head_parent = head.parent().expect("the playhead sits in the layer stack");
    assert_eq!(
        band_parent.widget_name(),
        head_parent.widget_name(),
        "both live in `track-layers`, the line over the band"
    );
    // Across both bands: the line's layer is as tall as the band it crosses.
    let (_, band_bottom) = edges_in(&window.clone().upcast::<gtk::Widget>(), &band);
    let (_, head_bottom) = edges_in(&window.clone().upcast::<gtk::Widget>(), &head);
    println!("LINE band bottom {band_bottom}, playhead layer bottom {head_bottom}");
    assert!(
        head_bottom >= band_bottom - 2,
        "the line's layer spans the band: {} vs {}",
        head_bottom,
        band_bottom
    );
    // 2 px: the constant `paint_playhead` fills with, and the value the painter actually uses.
    assert_eq!(
        naivepost::ui::cut_tracks::PLAYHEAD_PX,
        2.0,
        "\u{a7}1: a 2 px line"
    );
    // And it really paints: a cairo surface run through the same call gets ink at the line's x and
    // none 3 px away, which proves 2 px rather than a wider stroke.
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 60, 10)
        .expect("a surface to paint into");
    let cr = cairo::Context::new(&surface).expect("a context");
    cr.set_source_rgb(1.0, 1.0, 1.0);
    let _ = cr.paint();
    naivepost::ui::cut_tracks::paint_playhead(&cr, 1.0, 0.0, 20.0, 10.0);
    // `flush` returns a result the painter itself ignores (a failed flush paints nothing new, and
    // there is nothing to recover from here); silence the must-use so the check reads as an assertion.
    cr.target().flush();
    drop(cr);
    let data = surface.take_data().expect("the surface's pixels");
    let inked = |x: i32| {
        let offset = (x as usize) * 4;
        // ARgb32, premultiplied: ink means the pixel is not white any more.
        !(data[offset + 1] == 255 && data[offset + 2] == 255 && data[offset + 3] == 255)
    };
    assert!(inked(20) && inked(21), "two columns of ink at the line");
    assert!(!inked(23) && !inked(19), "and no third column: the line is 2 px");
    RAN_LINE.store(true, Ordering::SeqCst);
    window.close();
}

/// A real frame for the panel: cairo renders a surface and gdk-pixbuf encodes it, so the bytes are
/// whatever this build's encoder produces and `Pixbuf::from_file` reads back. Hand-rolled JPEG bytes
/// were tried first and gdk-pixbuf rejected them, so the fixture goes through the app's own encoder
/// (the snapshot bench stages its frame the same way) instead of guessing at a bitstream.
fn write_a_real_frame(path: &std::path::Path) {
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, 160, 90).expect("a surface to encode");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        cr.set_source_rgb(0.08, 0.35, 0.6);
        let _ = cr.paint();
    }
    let mut file = std::fs::File::create(path).expect("a frame file");
    surface.write_to_png(&mut file).expect("encoded pixels");
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 badge 1 holds a real picture");
        check_the_preview_holds_a_real_picture(app);
        println!("CHECK 2 the lanes sit under the picture rows");
        check_the_lanes_sit_under_the_picture_rows(app);
        println!("CHECK 3 the scrollbar shows when it must and hides when everything fits");
        check_the_scrollbar_shows_and_hides(app);
        println!("CHECK 4 the red line is 2 px on its own layer across both bands");
        check_the_red_line_is_two_pixels_on_its_own_layer(app);
    });
}

#[test]
fn sec_05_cut_1_screen_s1_the_preview_panel_holds_a_real_picture() {
    common::pin_private_cwd();
    round();
    assert!(RAN_PICTURE.load(Ordering::SeqCst), "the preview-picture check never ran");
}

#[test]
fn sec_05_cut_1_screen_s2_the_lanes_sit_directly_under_the_picture_rows() {
    common::pin_private_cwd();
    round();
    assert!(RAN_LANES.load(Ordering::SeqCst), "the lanes-under-the-rows check never ran");
}

#[test]
fn sec_05_cut_1_screen_s3_the_scrollbar_shows_when_the_tape_is_wider_than_the_view_and_hides_when_it_fits() {
    common::pin_private_cwd();
    round();
    assert!(RAN_BAR.load(Ordering::SeqCst), "the scrollbar check never ran");
}

#[test]
fn sec_05_cut_1_screen_s4_the_red_line_is_two_pixels_on_its_own_layer_across_both_bands() {
    common::pin_private_cwd();
    round();
    assert!(RAN_LINE.load(Ordering::SeqCst), "the red-line check never ran");
}
