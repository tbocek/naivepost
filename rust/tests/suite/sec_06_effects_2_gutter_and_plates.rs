// §06-effects#2, 06-lane's gutter and plates. §05-cut §1 puts "per-lane and per-row sound switches" in the
// gutter, and the lane picture shows the tape row's round 🔊 at its lower left. A camera row's own sound is the
// scene's `mute` property (F2.5 S2), so the row's switch moves `mute` on every kept scene shown from that row; a
// lane's switch moves the scene's `quiet` list and stays its own. And a row's name plate reads on its own light
// backing, one line under the other: dark ink straight on the pictures read as clipped letters.
//
// One `#[test]` drives the widget half: the checks share this window's cut slot and status line, so they run
// in sequence inside one application round (tests/suite/cam_hearing_widgets.rs has the same shape).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Cut, Seg};
use naivepost::cut_hear as hear;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;
use naivepost::ui::cut_tracks::{self as tracks, Scene};

use crate::common;
use common::{fixture_dir, settle, status_text};

static RAN_SWITCH: AtomicBool = AtomicBool::new(false);

/// Kept footage shown from camera row `row`.
fn film(s: f64, e: f64, row: i32) -> Seg {
    Seg {
        s,
        e,
        cam: row,
        ..Default::default()
    }
}

/// The row's switch silences its own sound in every kept scene shown from it, and brings it back; scenes from
/// another row and inserts keep their `mute`, and both directions say how many scenes moved.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s22_the_row_switch_speaks_for_the_rows_own_sound() {
    let mut cut = Cut::default();
    cut.segs = vec![
        film(0.0, 10.0, 0),
        film(20.0, 30.0, 0),
        film(40.0, 50.0, 1),
        Seg {
            s: 60.0,
            e: 70.0,
            ins: "card.svg".into(),
            ..Default::default()
        },
    ];
    assert!(hear::row_is_heard_anywhere(&cut, 0), "nothing is muted yet");
    assert_eq!(
        hear::toggle_row_all(&mut cut, 0, "session-tape"),
        "session-tape sound off for the whole cut \u{2014} 2 scene(s) changed"
    );
    assert!(
        cut.segs[0].mute && cut.segs[1].mute,
        "both scenes from row 0 are muted"
    );
    assert!(
        !cut.segs[2].mute && !cut.segs[3].mute,
        "another row's scene and the insert keep their own sound: {:?}",
        cut.segs
    );
    assert!(!hear::row_is_heard_anywhere(&cut, 0));
    // Half-silenced by hand still reads as heard, and the next press finishes the job.
    cut.segs[1].mute = false;
    assert!(hear::row_is_heard_anywhere(&cut, 0));
    assert_eq!(
        hear::toggle_row_all(&mut cut, 0, "session-tape"),
        "session-tape sound off for the whole cut \u{2014} 1 scene(s) changed"
    );
    assert_eq!(
        hear::toggle_row_all(&mut cut, 0, "session-tape"),
        "session-tape sound is on for the whole cut \u{2014} every scene plays it (2 changed)"
    );
    assert!(!cut.segs[0].mute && !cut.segs[1].mute);
    assert_eq!(
        hear::toggle_row_all(&mut cut, 5, "row 6"),
        "row 6 is in no scene yet \u{2014} cut something first"
    );
}

/// A Cut-page window over the fixture, with one camera row of two kept scenes and one lane.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    common::size_to_minimum(&window);
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    common::wait_for_stable_layout(&window);
    let mut seeded = Cut::default();
    seeded.segs = vec![film(0.0, 10.0, 0), film(20.0, 30.0, 0)];
    seeded.lanes = vec![cut::Lane {
        name: "mic".to_string(),
        src: "mic".to_string(),
        at: 0.0,
        off: 0.0,
        dur: 30.0,
    }];
    ui::seed_review_cut(&window, &seeded);
    ui::refresh_camera_rows(&window);
    settle();
    window
}

/// The row's sound switch (§1 badge 29): drawn in the gutter at row 1's sound strip, ON at first, and a real
/// press on the band there mutes both scenes, leaves the lane's `quiet` alone and says so on the status line.
fn check_the_row_switch_is_pressed_in_the_gutter(app: &adw::Application) {
    let window = cut_window(app);
    let switch = |window: &adw::ApplicationWindow| {
        ui::cut_band_marks::placed_on(window)
            .into_iter()
            .find(|placed| matches!(placed.mark, ui::cut_band_marks::Mark::RowSound(0, _)))
            .expect("row 1's sound switch is drawn on the band")
    };
    let at = switch(&window);
    assert_eq!(
        at.mark,
        ui::cut_band_marks::Mark::RowSound(0, true),
        "the row's sound plays at first, so the switch stands ON"
    );
    let strip = ui::cut_tracks::row_wave_top(&window, 0);
    assert!(
        at.x < naivepost::cut_screen::GUTTER_PX,
        "the switch sits in the gutter, x = {}",
        at.x
    );
    assert!(
        at.y >= strip && at.y <= strip + ui::cut_tracks::ROW_WAVE_PX,
        "the switch sits on row 1's sound strip ({strip}..{}): {at:?}",
        strip + ui::cut_tracks::ROW_WAVE_PX
    );

    // A real press on the band where the switch is drawn, through the band's own click gesture.
    let gesture = ui::window::band_line_gesture_for_test(&window).expect("the band takes clicks");
    gesture.emit_by_name::<()>("released", &[&1i32, &at.x, &at.y]);
    settle();
    let segs = ui::review_cut_segs(&window);
    assert!(
        segs.iter().all(|seg| seg.mute),
        "every scene from the row is muted: {segs:?}"
    );
    assert!(
        segs.iter().all(|seg| seg.quiet.is_empty()),
        "the lane's hearing is not the row's: {segs:?}"
    );
    assert!(
        status_text(&window).contains("sound off for the whole cut"),
        "{}",
        status_text(&window)
    );
    assert_eq!(
        switch(&window).mark,
        ui::cut_band_marks::Mark::RowSound(0, false),
        "and the switch reads OFF"
    );
    RAN_SWITCH.store(true, Ordering::SeqCst);
    window.close();
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s22_the_row_switch_is_pressed_on_the_band() {
    let root = std::env::temp_dir().join(format!("np-row-switch-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    common::one_app_round(|app| {
        check_the_row_switch_is_pressed_in_the_gutter(app);
    });
    assert!(
        RAN_SWITCH.load(Ordering::SeqCst),
        "the row switch check never ran"
    );
}

/// The plate of a row with a cut lane windowing its file and a hand correction has three lines (`take-a`,
/// `from 01:15`, `+1.23 s`), each on a dark backing of its own: down the column just right of the gutter, where
/// every line starts, the two lines a one-line plate does not have sit one under the other. With the three lines
/// inked at one y and no backing, that column was the row's ground all the way down.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s23_a_name_plate_reads_on_its_own_backing() {
    common::pin_private_cwd();
    let scene = Scene {
        pps: 2.0,
        origin: 0.0,
        segs: vec![film(0.0, 20.0, 0)],
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
        shift: [("take-a".to_string(), 1.234)].into_iter().collect(),
        lanes: vec![cut::Lane {
            name: "take-a".into(),
            src: "project:take-a.mp4".into(),
            at: 75.0,
            off: 0.0,
            dur: 10.0,
        }],
        lane_plates: Vec::new(),
        thumbs: Vec::new(),
        line: 0.0,
    };
    assert_eq!(tracks::plates(&scene, &scene.recordings[0]).len(), 3);
    let (wide, high) = (
        300,
        tracks::band_height_px(&scene, tracks::THUMB_DEFAULT_PX).ceil() as i32,
    );
    let paint = |scene: &Scene| {
        let mut surface =
            cairo::ImageSurface::create(cairo::Format::ARgb32, wide, high).expect("a surface");
        {
            let cr = cairo::Context::new(&surface).expect("a context");
            tracks::paint_at(
                &cr,
                scene,
                wide as f64,
                high as f64,
                tracks::THUMB_DEFAULT_PX,
            );
        }
        let stride = surface.stride() as usize;
        let data = surface.data().expect("the pixels").to_vec();
        (data, stride)
    };
    // The same row with a one-line plate (no lane windowing it, no correction): what the second and third lines
    // add, each on its own backing, is the difference between the two down the column where every line begins.
    let mut one_line = scene.clone();
    one_line.lanes[0].name = "elsewhere".into();
    one_line.shift.clear();
    assert_eq!(tracks::plates(&one_line, &one_line.recordings[0]).len(), 1);
    let ((three, stride), (one, _)) = (paint(&scene), paint(&one_line));
    let pixel = |data: &[u8], x: i32, y: i32| {
        let at = y as usize * stride + x as usize * 4;
        (data[at], data[at + 1], data[at + 2])
    };
    let column = naivepost::cut_screen::GUTTER_PX as i32 + 1;
    // The picture row only: the lane's own row below it is named differently in the two scenes.
    let pictures = naivepost::ui::cut_band_rows::pictures_top_px(
        naivepost::ui::cut_band_rows::lane_top_px(
            naivepost::cut_trim::RULER_H,
            tracks::SELECTION_BAND_PX,
        ),
        0.0,
    ) as i32;
    let row_end =
        pictures + (tracks::THUMB_DEFAULT_PX as f64 + tracks::PICTURE_ROW_EXTRA_PX) as i32;
    let differs: Vec<i32> = (pictures..row_end)
        .filter(|y| pixel(&three, column, *y) != pixel(&one, column, *y))
        .collect();
    // The longest run of rows that differ, and where it starts.
    let (mut run, mut start, mut now, mut from) = (0, 0, 0, 0);
    for (i, y) in differs.iter().enumerate() {
        if i > 0 && *y == differs[i - 1] + 1 {
            now += 1;
        } else {
            (now, from) = (1, *y);
        }
        if now > run {
            (run, start) = (now, from);
        }
    }
    assert!(
        run >= 20,
        "the second and third plate lines, on their backing, one under the other, add a run of about 22 px; \
         the longest is {run} px"
    );
    // Just under them the column is the row's own ink again: the run is the two lines' backing.
    assert_eq!(
        pixel(&three, column, start + run + 2),
        pixel(&one, column, start + run + 2),
        "under the plate the row is as it was"
    );
}
