//! §05-cut#1-screen "Tracks": the marks a hand presses on the band — the ✕ on a green bar (badge 25) and on the
//! selection (27), the −/+ fold badges in the dropped gaps (26) and the fold-all badge (23), each row's sound switch
//! (29) and the lens on the scene under the line — are drawn ON the band and answered there (`ui::cut_band_marks`),
//! the way the prototype draws and hit-tests them. The first port made them GTK buttons: grey plates over the bars
//! and a row of crosses and lenses under the tape, while the ✕ inked on a bar took no press at all.
//!
//! Every press here goes through the band's own click gesture, at the place the band draws the mark.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::cut_line::LinePos;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;
use naivepost::ui::cut_band_marks::{Mark, Placed};
use naivepost::ui::cut_tracks::{self as tracks, Scene};

use crate::common;
use common::{fixture_dir, settle, status_text};

static RAN: AtomicBool = AtomicBool::new(false);

/// Kept footage shown from camera row `row`.
fn film(s: f64, e: f64, row: i32) -> Seg {
    Seg {
        s,
        e,
        cam: row,
        ..Default::default()
    }
}

/// A Cut page over two cameras that both filmed 0..120 s, two kept scenes on row 1 with a dropped gap between
/// them, and the red line inside the first scene.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    common::size_to_minimum(&window);
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    common::wait_for_stable_layout(&window);
    ui::set_session_recordings(&[
        Recording {
            base: "take-a".into(),
            start: 0.0,
            end: 120.0,
        },
        Recording {
            base: "take-b".into(),
            start: 0.0,
            end: 120.0,
        },
    ]);
    let mut cut = Cut::default();
    cut.segs = vec![film(10.0, 40.0, 0), film(60.0, 100.0, 0)];
    ui::seed_review_cut(&window, &cut);
    ui::set_held_clip(None);
    ui::set_line_position(&window, LinePos { t: 20.0 });
    ui::cut_view::repaint_page(&window);
    settle();
    window
}

/// The mark the band draws that `pick` accepts.
fn mark(window: &adw::ApplicationWindow, pick: impl Fn(&Mark) -> bool) -> Placed {
    ui::cut_band_marks::placed_on(window)
        .into_iter()
        .find(|placed| pick(&placed.mark))
        .unwrap_or_else(|| {
            panic!(
                "no such mark on the band: {:?}",
                ui::cut_band_marks::placed_on(window)
            )
        })
}

/// A real click on the band where `placed` is drawn.
fn press(window: &adw::ApplicationWindow, placed: &Placed) {
    let gesture = ui::window::band_line_gesture_for_test(window).expect("the band takes clicks");
    gesture.emit_by_name::<()>("released", &[&1i32, &placed.x, &placed.y]);
    settle();
}

fn check_the_marks_stand_where_section_1_puts_them(window: &adw::ApplicationWindow) {
    let band_y = naivepost::cut_trim::RULER_H + tracks::SELECTION_BAND_PX / 2.0;
    let scene = ui::cut_view::band_scene_for(window);
    // Badge 25: a ✕ in the middle of each green bar, in the selection band.
    for index in 0..2 {
        let cross = mark(window, |m| *m == Mark::SceneCross(index));
        let (x, w) = tracks::band_span_px(&scene, scene.segs[index].s, scene.segs[index].e);
        assert!(
            (cross.x - (x + w / 2.0 - scene.origin)).abs() < 0.5 && (cross.y - band_y).abs() < 0.5,
            "scene {index}'s \u{2715} sits mid-bar in the selection band: {cross:?}"
        );
    }
    // Badge 26: the dropped 40..60 s between them wears a − (it is open).
    let fold = mark(
        window,
        |m| matches!(m, Mark::Fold(gap, false) if gap.0 == 40.0),
    );
    assert!((fold.y - band_y).abs() < 0.5, "in the selection band");
    // Badge 23 at the gutter's top, and badge 29 per row, both pinned in the gutter.
    for placed in [
        mark(window, |m| matches!(m, Mark::FoldAll(false))),
        mark(window, |m| matches!(m, Mark::RowSound(0, true))),
        mark(window, |m| matches!(m, Mark::RowSound(1, _))),
    ] {
        assert!(
            placed.pinned && placed.x < naivepost::cut_screen::GUTTER_PX,
            "a gutter mark sits in the gutter: {placed:?}"
        );
    }
    // The scene under the line was filmed by both cameras, so it wears a lens per row, lit on the row it is shown
    // from; and its own sound's speaker on its wave strip.
    mark(window, |m| *m == Mark::Lens(0, true));
    mark(window, |m| *m == Mark::Lens(1, false));
    mark(window, |m| *m == Mark::SceneSound(true));
}

fn check_a_press_on_a_mark_does_its_job_and_leaves_the_line(window: &adw::ApplicationWindow) {
    // The fold badge folds its gap; the line stays where it was.
    let fold = mark(
        window,
        |m| matches!(m, Mark::Fold(gap, false) if gap.0 == 40.0),
    );
    press(window, &fold);
    assert!(
        ui::review_cut_of(window)
            .folds
            .iter()
            .any(|[s, e]| *s <= 40.5 && *e >= 59.5),
        "the − folded the 40..60 s gap: {:?}",
        ui::review_cut_of(window).folds
    );
    assert!(
        status_text(window).starts_with("folded"),
        "and says so: {}",
        status_text(window)
    );
    assert_eq!(
        ui::line_position(window).t,
        20.0,
        "a press on a mark places no line"
    );
    // The folded gap now wears a +, which opens it again.
    let unfold = mark(
        window,
        |m| matches!(m, Mark::Fold(gap, true) if gap.0 == 40.0),
    );
    press(window, &unfold);
    assert!(
        ui::review_cut_of(window).folds.is_empty(),
        "the + opened it"
    );

    // The lens on row 2 shows the scene from camera 2.
    let lens = mark(window, |m| *m == Mark::Lens(1, false));
    press(window, &lens);
    assert_eq!(
        ui::review_cut_segs(window)[0].cam,
        1,
        "the scene is shown from row 2 now"
    );
    assert!(
        status_text(window).contains("is shown from"),
        "{}",
        status_text(window)
    );

    // The selection's ✕ clears the band and nothing else.
    ui::draw_selection(
        window,
        naivepost::cut_select::Surface::PictureRow(0),
        None,
        64.0,
        90.0,
    );
    ui::cut_view::repaint_page(window);
    let cross = mark(window, |m| *m == Mark::SelectionCross);
    press(window, &cross);
    assert!(
        ui::selection(window).is_none(),
        "the \u{2715} cleared the selection"
    );
    assert_eq!(ui::review_cut_segs(window).len(), 2, "and removed no scene");

    // The second scene's ✕ drops that scene.
    let cross = mark(window, |m| *m == Mark::SceneCross(1));
    press(window, &cross);
    assert_eq!(
        ui::review_cut_segs(window).len(),
        1,
        "the \u{2715} took scene 2 out of the cut"
    );
}

#[test]
fn sec_05_cut_1_screen_the_band_draws_its_marks_and_answers_a_press_on_them() {
    common::pin_private_cwd();
    common::one_app_round(|app| {
        let window = cut_window(app);
        check_the_marks_stand_where_section_1_puts_them(&window);
        check_a_press_on_a_mark_does_its_job_and_leaves_the_line(&window);
        window.close();
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the band-marks round never ran");
}

/// The paint: a bar's ✕ is a dark plate under white arms, and the plate is darker than the green beside it.
#[test]
fn sec_05_cut_1_screen_a_bar_cross_is_plated_dark_on_the_green() {
    common::pin_private_cwd();
    let scene = Scene {
        pps: 4.0,
        origin: 0.0,
        segs: vec![film(5.0, 40.0, 0)],
        fx: Vec::new(),
        recordings: vec![Recording {
            base: "take-a".into(),
            start: 0.0,
            end: 50.0,
        }],
        recording_rows: Vec::new(),
        marks: Default::default(),
        folds: Vec::new(),
        selection: None,
        watched_row: None,
        row_count: 1,
        shift: Default::default(),
        lanes: Vec::new(),
        lane_plates: Vec::new(),
        thumbs: Vec::new(),
        line: 0.0,
    };
    let (w, h) = (
        300,
        tracks::band_height_px(&scene, tracks::THUMB_DEFAULT_PX).ceil() as i32,
    );
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        tracks::paint_at(&cr, &scene, w as f64, h as f64, tracks::THUMB_DEFAULT_PX);
    }
    let stride = surface.stride() as usize;
    let data = surface.data().expect("pixels").to_vec();
    let at = |x: f64, y: f64| {
        let base = y as usize * stride + x as usize * 4;
        (
            data[base + 2] as u32,
            data[base + 1] as u32,
            data[base] as u32,
        )
    };
    let (x, bw) = tracks::band_span_px(&scene, 5.0, 40.0);
    let (cx, cy) = (
        (x + bw / 2.0).round(),
        naivepost::cut_trim::RULER_H + tracks::SELECTION_BAND_PX / 2.0,
    );
    let sum = |c: (u32, u32, u32)| c.0 + c.1 + c.2;
    let centre = at(cx, cy);
    let plate = at(cx + 5.0, cy);
    let bar = at(cx + 12.0, cy);
    assert!(
        centre.0 > 180 && centre.1 > 180 && centre.2 > 180,
        "white arms cross at the centre: {centre:?}"
    );
    assert!(
        sum(plate) < sum(bar),
        "the plate under them is darker than the bar beside it: {plate:?} vs {bar:?}"
    );
    assert!(
        bar.1 > bar.0 && bar.1 > bar.2,
        "and the bar is green: {bar:?}"
    );
}
