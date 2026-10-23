//! F2.8 (Trim and move) — the page's half of the flow: what a press READS off the painted strip, what a
//! gesture WRITES, and whether the media leg was actually re-seeked.
//!
//! `tests/suite/cut_trim_move.rs` and `tests/suite/cut_trim_gesture.rs` pin the rules in `cut_trim`;
//! `tests/suite/cut_trim_move_widgets.rs` pins that a drag arrives at the right branch. What lives here is what only
//! the page can get wrong: the lane / recording a press names coming from the PLATE under the pointer rather
//! than from a primed seam; a band that exists but sits nowhere near the press taking nothing; a slide and a
//! trim actually landing in cut.json; one Undo per gesture on the persisting path; the folds a drag opened
//! coming back on release; and the running player re-seeked at most every `cut_trim::SCRUB_MS` and finally AT
//! the released edge.
//!
//! ONE application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared state
//! (this window's cut slot, the session recording list, folds, the cwd the session is resolved from and the
//! shared `NAIVEPOST_PLAYER` seam), so they run once and report through flags the test asserts on — the shape
//! `tests/suite/cut_trim_move_widgets.rs` uses.
//!
//! THE PIXEL MATH, stated up front because every press below is placed in it. The strip draws at
//! `ui::cut_view::strip_pps()` = `ZOOM_AT_OPEN (4 px/s).clamp(floor_pps(450 px view), ZOOM_MAX)`, and
//! `floor_pps` is `450 / (the merged tape's seconds)` — so the tape this file seeds (70 s + 50 s = 120 s
//! merged) floors at 3.75 and leaves the zoom at EXACTLY 4 px/s. A shorter tape would clamp the zoom UP and
//! move every border out from under the numbers below. At 4 px/s the three scenes' borders sit at 0/38/40/
//! 120/160/280 px, which is why both lanes live at 96..116 s and 120..140 s: past the last scene, so a press
//! on a plate is never within [`cut_trim::EDGE_GRAB_PX`] of a border and cannot be read as a trim grab.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Cut, Lane, Seg};
use naivepost::cut_select::Surface;
use naivepost::cut_trim::{self, Border};
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;
use naivepost::ui::cut_trim_press;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_LANE_PLATE: AtomicBool = AtomicBool::new(false);
static RAN_WAVE_PLATE: AtomicBool = AtomicBool::new(false);
static RAN_NO_BAND: AtomicBool = AtomicBool::new(false);
static RAN_INSIDE_BAND: AtomicBool = AtomicBool::new(false);
static RAN_PERSIST: AtomicBool = AtomicBool::new(false);
static RAN_ONE_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_FOLDS: AtomicBool = AtomicBool::new(false);
static RAN_CLICK: AtomicBool = AtomicBool::new(false);

/// The strip's pixels-per-second, pinned to §2's open zoom by the tape lengths documented above. Every pixel
/// in this file is `seconds * PPS`.
const PPS: f64 = naivepost::cut_screen::ZOOM_AT_OPEN;
/// Where a RIGHT-press lands when it should belong to no band and grab no border: the ruler. It is neither
/// the recorders' band nor the wave strip, and — unlike the kept bar — it is not part of the placeholder
/// layout either, where the bar and the wave share a row.
const NO_BAND_Y: f64 = cut_trim::RULER_H / 2.0;

fn px(secs: f64) -> f64 {
    secs * PPS
}

fn footage(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        cam: 0,
        ..Default::default()
    }
}

fn lane(name: &str, at: f64, dur: f64) -> Lane {
    Lane {
        name: name.into(),
        src: format!("project:sources/{name}.wav"),
        at,
        dur,
        ..Default::default()
    }
}

fn recording(base: &str, start: f64, end: f64) -> Recording {
    Recording {
        base: base.into(),
        start,
        end,
    }
}

/// The session's tape: the lecture take over the cut's own footage plus a second take past it. Together they
/// merge to 120 s, which is what holds the strip at 4 px/s (see the module note).
fn tape() -> Vec<Recording> {
    vec![
        recording("lecture", 0.0, 70.0),
        recording("cam9", 90.0, 140.0),
    ]
}

/// Three scenes plus two separate lanes. Both plates (96..116 s, 120..140 s) lie past the last scene, so a
/// press inside one is far from any clip border.
fn lanes_cut() -> Cut {
    Cut {
        segs: vec![footage(0.0, 9.5), footage(10.0, 30.0), footage(40.0, 70.0)],
        lanes: vec![lane("mic-main", 96.0, 20.0), lane("mic-boom", 120.0, 20.0)],
        ..Default::default()
    }
}

/// A project folder of our own with the `session.naivepost` layout the save doors write into: the doors save
/// through `std::env::current_dir()`, so a persistence check pins the cwd here and reads the same file back.
fn stage(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("naivepost-f28live-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let session = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(session.join("cut")).expect("a session cut folder");
    std::fs::create_dir_all(session.join("prepare")).expect("a session prepare folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    root
}

/// What the doors wrote, read back through the same loader the app uses at open.
fn saved_cut() -> Cut {
    let tree = naivepost::layout::Tree::new(naivepost::startup::session_dir(
        &std::env::current_dir().expect("a cwd"),
    ))
    .expect("the staged folder is a project");
    cut::load(&tree).expect("cut.json parses")
}

/// A window on the Cut page over a staged project: `seeded` as its live cut, `recordings` as the tape the page
/// lays out. Nothing is primed — every scope answer below must come from where the pointer landed. Returns the
/// zoom the strip really drew at so a check converts seconds with THAT number.
fn cut_window(
    app: &adw::Application,
    tag: &str,
    seeded: &Cut,
    recordings: &[Recording],
) -> (adw::ApplicationWindow, f64) {
    stage(tag);
    release_last_window();
    let mut model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    model.sources = vec![naivepost::project::Source {
        path: "project:sources/lecture.mkv".into(),
        footage: true,
        narrator: 0,
        sepvoice: false,
        tracks: vec![],
    }];
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "the Cut tab is live with footage"
    );
    ui::set_session_recordings(recordings);
    ui::seed_review_cut(&window, seeded);
    ui::set_press_scene(None);
    ui::set_press_band(None);
    settle();
    let pps = naivepost::ui::cut_view::strip_pps();
    assert!(
        (pps - PPS).abs() < 1e-9,
        "this file's pixels are `{PPS}` px/s; the strip drew at {pps}, so the tape no longer merges to 120 s"
    );
    (window, pps)
}

/// The recorders' band's own middle y, read from the module's bands so no height is guessed here.
fn recorder_y(cut_: &Cut, recordings: &[Recording]) -> f64 {
    let bands = cut_trim_press::bands(cut_, recordings);
    bands.recorders.0 + (bands.recorders.1 - bands.recorders.0) / 2.0
}

fn wave_y(cut_: &Cut, recordings: &[Recording]) -> f64 {
    let bands = cut_trim_press::bands(cut_, recordings);
    bands.wave.0 + (bands.wave.1 - bands.wave.0) / 2.0
}

/// S2: a press on the recorders' band names the LANE whose PLATE lies under it — not a primed seam, and not
/// whichever lane happens to be first in the list. Two plates means the answer has to be positional.
fn f2_8_s2_a_press_on_the_recorders_band_names_that_lane(app: &adw::Application) {
    let cut_ = lanes_cut();
    let tape = tape();
    let (window, pps) = cut_window(app, "laneplate", &cut_, &tape);
    let y = recorder_y(&cut_, &tape);
    println!("OBSERVED: zoom={pps} band y={y} | mic-main plate 96..116 s (x {}), mic-boom 120..140 s (x {})", px(106.0), px(130.0));

    let (first, wave, border) = ui::right_press_took(px(106.0), y);
    println!(
        "PRESS x={} y={y} → band={first:?} wave={wave:?} border={border}",
        px(106.0)
    );
    assert_eq!(
        first.as_deref(),
        Some("mic-main"),
        "the plate under x names its own lane"
    );
    assert_eq!(wave, None, "a lane press is not the wave strip's answer");
    assert!(!border, "mid-plate x is not a border");

    let (second, _, _) = ui::right_press_took(px(130.0), y);
    println!("PRESS x={} y={y} → band={second:?}", px(130.0));
    assert_eq!(
        second.as_deref(),
        Some("mic-boom"),
        "a different plate answers a different lane"
    );

    // 118 s falls between the two plates (mic-main ends at 116, mic-boom starts at 120): band ground with no
    // plate under it, so it names nothing.
    let (gap, gap_wave, _) = ui::right_press_took(px(118.0), y);
    println!(
        "PRESS x={} y={y} → band={gap:?} wave={gap_wave:?}",
        px(118.0)
    );
    assert_eq!(gap, None, "a bare strip of band is not a lane");
    assert_eq!(gap_wave, None, "and it names no recording either");
    drop(window);
}

/// S2: a press on the wave strip names the RECORDING filmed under it. The wave row is only its own row when a
/// lane row exists above it (`cut_trim_press::wave_plate_exists`: with no lanes the strip draws one row and
/// there is no plate of its own to name), hence the lanes in this cut.
fn f2_8_s2_a_press_on_the_wave_strip_names_the_recording_under_the_pointer(app: &adw::Application) {
    let cut_ = lanes_cut();
    let tape = tape();
    let (window, pps) = cut_window(app, "waveplate", &cut_, &tape);
    let y = wave_y(&cut_, &tape);
    println!(
        "OBSERVED: zoom={pps} wave y={y} | lecture 0..70 s (x {}), cam9 90..140 s (x {})",
        px(20.0),
        px(106.0)
    );

    let (band, wave, _) = ui::right_press_took(px(20.0), y);
    println!("PRESS x={} y={y} → band={band:?} wave={wave:?}", px(20.0));
    assert_eq!(
        wave.as_deref(),
        Some("lecture"),
        "the wave plate under x names its recording"
    );
    assert_eq!(band, None, "and the recorders' band stays out of it");

    let (_, second, _) = ui::right_press_took(px(106.0), y);
    println!("PRESS x={} y={y} → wave={second:?}", px(106.0));
    assert_eq!(
        second.as_deref(),
        Some("cam9"),
        "a second recording answers its own name"
    );
    drop(window);
}

/// S2: the two "no band" answers the flowchart has to give — ruler ground takes nothing, while a press ON a
/// drawn border is a trim target (which is how S1 outranks S2's slide).
fn f2_8_s2_ruler_ground_and_a_border_take_no_band(app: &adw::Application) {
    let cut_ = lanes_cut();
    let tape = tape();
    let (window, pps) = cut_window(app, "noband", &cut_, &tape);
    let boxes = naivepost::ui::cut_view::strip_clip_boxes_for_test();
    println!("OBSERVED: zoom={pps} ruler y={NO_BAND_Y} boxes={boxes:?}");

    // Ruler ground over mic-main's plate: no band answers, and no border out there.
    let (band, wave, border) = ui::right_press_took(px(106.0), NO_BAND_Y);
    println!(
        "RULER PRESS x={} → ({band:?}, {wave:?}, {border})",
        px(106.0)
    );
    assert_eq!((band, wave), (None, None), "ruler ground takes no band");
    assert!(!border, "and no border there either");

    // Scene 0's END border, at its drawn pixel: `on_border`, which is what makes a border press a trim.
    let first = boxes.first().expect("the strip drew a box");
    let end_px = first.x + first.w;
    let (b_band, b_wave, b_border) = ui::right_press_took(end_px, NO_BAND_Y);
    println!("BORDER PRESS x={end_px} → ({b_band:?}, {b_wave:?}, {b_border})");
    assert!(b_border, "a press on a drawn border is a border press");
    assert_eq!(
        (b_band, b_wave),
        (None, None),
        "and it names no band: the border owns the press"
    );
    drop(window);
}

/// S2 ("a footage selection the press falls inside"): only a band the press falls INSIDE slides its scenes.
/// The old page answered on the band's LENGTH, so any band at all — however far from the pointer — swallowed
/// the press. Same window, same band, two x values: that is the whole regression.
fn f2_8_s2_only_a_selection_the_press_falls_inside_slides_its_scenes(app: &adw::Application) {
    let cut_ = lanes_cut();
    let tape = tape();
    let (window, _pps) = cut_window(app, "inside", &cut_, &tape);
    let band =
        ui::draw_selection(&window, Surface::PictureRow(0), None, 5.0, 50.0).expect("a band draws");
    assert!(
        band.length() > 1.0,
        "the band exists and is long — length is not the question"
    );

    // 20 s lies inside 5..50; 60 s does not. Same y (ruler ground, so no band answers), same travel.
    let inside = ui::press_move_at(&window, px(20.0), NO_BAND_Y, 12.0, 3.0);
    let said_inside = status_text(&window);
    let _outside = ui::press_move_at(&window, px(60.0), NO_BAND_Y, 12.0, 3.0);
    let said_outside = status_text(&window);
    println!("INSIDE PRESS PRINTED: {said_inside}");
    println!("OUTSIDE PRESS PRINTED: {said_outside}");

    assert!(
        said_inside.contains("the selected scenes moved"),
        "a press inside the band slides the selected scenes: {said_inside}"
    );
    assert!(
        !said_outside.contains("selected scenes"),
        "a band the press falls outside takes nothing, so it must not read as a selection slide: {said_outside}"
    );
    assert!(
        matches!(inside, cut_trim::Gesture::Slid { .. }),
        "the inside press travelled 12 px; it is not a click"
    );
    drop(window);
}

/// S1/S2 persistence: a slide's shift correction and a trim's trimmed seconds are IN cut.json, so a reopen
/// reads them back instead of asking for the correction again.
fn f2_8_s2_a_trim_and_a_shift_correction_survive_reopen(app: &adw::Application) {
    let cut_ = lanes_cut();
    let tape = tape();
    let (window, _pps) = cut_window(app, "persist", &cut_, &tape);
    ui::set_press_scene(Some(1));
    // The wave strip's own recording answers this press, so the shift has a named source to land on — the
    // same thing a real page hands it once the rows are drawn (F2.10/F2.11).
    ui::set_press_band(Some("lecture".to_string()));
    let gesture = ui::press_move_at(&window, px(20.0), wave_y(&cut_, &tape), 20.0, 5.0);
    assert!(
        matches!(gesture, cut_trim::Gesture::Slid { .. }),
        "the slide answered, so there is a correction to persist: {gesture:?}"
    );
    let landed = ui::press_trim_border(&window, 0, Border::Start, 1.25);
    // S1's other half, measured where it is decided: the second the picture must show is the second the CUT
    // holds after the write, not the pixel the pointer happened to reach.
    let edge = cut_trim_press::edge_second(0, Border::Start);
    println!("EDGE FOR THE PICTURE: {edge} (trim landed {landed})");
    assert!(
        (edge - landed).abs() < 1e-9,
        "the edge the picture must show is the edge the cut holds, not the pixel the pointer reached: {edge} vs {landed}"
    );
    settle();

    let on_disk = saved_cut();
    println!(
        "CUT.JSON AFTER GESTURES: shift={:?} seg0={}..{} (landed {landed})",
        on_disk.shift, on_disk.segs[0].s, on_disk.segs[0].e
    );
    assert!(
        on_disk.shift.values().any(|secs| secs.abs() > f64::EPSILON),
        "the shift correction is IN THE FILE, not only in the window: {:?}",
        on_disk.shift
    );
    assert!(
        (on_disk.segs[0].s - landed).abs() < 1e-9,
        "and the trimmed seconds are in the file too: file {} vs landed {landed}",
        on_disk.segs[0].s
    );
    assert!(
        (on_disk.segs[0].s - 1.25).abs() < 1e-9,
        "at the edge the hand asked for: {}",
        on_disk.segs[0].s
    );
    drop(window);
}

/// S2: many `drag-update`s, ONE Undo step. The old move path pushed per update, so one drag needed many ↶.
fn f2_8_s2_one_undo_per_gesture_on_the_move_path(app: &adw::Application) {
    let cut_ = lanes_cut();
    let tape = tape();
    let (window, _pps) = cut_window(app, "undo", &cut_, &tape);
    ui::set_press_scene(Some(1));
    // A source for the shift to land on, exactly as a drawn wave strip would name it (F2.10/F2.11).
    ui::set_press_band(Some("lecture".to_string()));
    let press_y = wave_y(&cut_, &tape);
    ui::set_press_x(px(20.0));
    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    mover.emit_by_name::<()>("drag-begin", &[&px(20.0), &press_y]);
    let mut seen = Vec::new();
    for offset in [12.0, 24.0] {
        mover.emit_by_name::<()>("drag-update", &[&offset, &press_y]);
        settle();
        let shift = ui::review_cut_shift(&window);
        println!("SHIFT AFTER UPDATE +{offset}px: {shift:?}");
        seen.push(shift.values().sum::<f64>());
    }
    mover.emit_by_name::<()>("drag-end", &[&36.0, &press_y]);
    settle();

    let after = ui::review_cut_shift(&window).values().sum::<f64>();
    // A third update of the same hold must add no further correction on top of the second's: each update
    // reports where the pointer got to FROM THE PRESS, and one gesture is one act of the hand.
    assert!(
        seen[1] > seen[0],
        "the hold moved as the pointer travelled: {seen:?}"
    );
    assert!(
        after.abs() > f64::EPSILON,
        "and the drag did move something: {after}"
    );

    let said = ui::press_undo(&window);
    let back = ui::review_cut_shift(&window).values().sum::<f64>();
    println!("UNDO SAID: {said} → shift {back}");
    assert!(
        back.abs() < 1e-9,
        "one undo undoes the whole drag (a step per update would need three): shift {back}"
    );
    drop(window);
}

/// S2: a fold the drag travels through opens for the drag and is refolded on release; a fold it never reaches
/// is left exactly as it was. The drag takes the SELECTION here, because `cut_trim` measures a recording/scene
/// grab at the scene's own start rather than at the pointer — the selection is the slide whose grab is where
/// the hand actually pressed, which is what "near the grab" means.
fn f2_8_s2_folds_near_the_grab_open_for_the_drag_and_refold_after(app: &adw::Application) {
    let mut cut_ = lanes_cut();
    // A fold the drag (grab 20 s → 44 s) travels through, and one it does not (90..100 s).
    cut_.folds = vec![[30.0, 36.0], [90.0, 100.0]];
    let tape = tape();
    let (window, _pps) = cut_window(app, "folds", &cut_, &tape);
    let band = ui::draw_selection(&window, Surface::PictureRow(0), None, 20.0, 50.0)
        .expect("a band draws, so its start is the grab");
    assert_eq!((band.start, band.end), (20.0, 50.0));
    let folds_at_open = ui::review_cut_of(&window).folds.clone();
    println!("FOLDS BEFORE: {folds_at_open:?}");

    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    mover.emit_by_name::<()>("drag-begin", &[&px(20.0), &NO_BAND_Y]);
    let travel_px = px(24.0);
    mover.emit_by_name::<()>("drag-update", &[&travel_px, &NO_BAND_Y]);
    settle();

    let open_now = ui::open_folds(&window);
    let during = ui::review_cut_of(&window).folds.clone();
    println!("DURING DRAG: open={open_now:?} folds={during:?}");
    assert!(
        open_now.contains(&0),
        "the fold the drag travelled through is the one it asked to open"
    );
    assert!(
        !open_now.contains(&1),
        "and not the one it never reached: {open_now:?}"
    );

    mover.emit_by_name::<()>("drag-end", &[&travel_px, &NO_BAND_Y]);
    settle();
    let after = ui::review_cut_of(&window).folds.clone();
    println!(
        "AFTER RELEASE: open={:?} folds={after:?}",
        ui::open_folds(&window)
    );
    assert_eq!(
        after, folds_at_open,
        "release refolds exactly what the drag opened"
    );
    assert!(
        ui::open_folds(&window).is_empty(),
        "and the drag forgets the hold: nothing stays opened for it"
    );
    assert_eq!(
        saved_cut().folds,
        folds_at_open,
        "and the file keeps the fold list the person saved, not the drag's temporary opening"
    );
    drop(window);
}

/// S3: an unmoved right press is a click. The status says `this one`, and nothing at all changed.
fn f2_8_s3_an_unmoved_right_press_is_a_click_and_says_this_one(app: &adw::Application) {
    let cut_ = lanes_cut();
    let tape = tape();
    let (window, _pps) = cut_window(app, "click", &cut_, &tape);
    let before_segs = ui::review_cut_segs_count(&window);
    let before_shift = ui::review_cut_shift(&window);
    let before_line = ui::line_position(&window).t;

    // The strip really carries the gesture the page wired; the click below is asked through `press_move_at`,
    // the same door the release uses, so nothing here can move.
    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    let _ = mover;
    // S3 lives at the press, and the press's position is what `press_move_at` reads — so the click is asked
    // through the same door the drag's release uses, with no travel: 0 px of offset.
    ui::set_press_x(px(20.0));
    let gesture = ui::press_move_at(&window, px(20.0), NO_BAND_Y, 0.0, 0.0);
    settle();

    let said = status_text(&window);
    println!(
        "CLICK STATUS: {said:?} (want {:?})",
        cut_trim_press::CLICK_STATUS
    );
    assert_eq!(
        said.trim(),
        cut_trim_press::CLICK_STATUS,
        "an unmoved right press says exactly `{}`",
        cut_trim_press::CLICK_STATUS
    );
    assert_eq!(
        ui::review_cut_segs_count(&window),
        before_segs,
        "and a click cuts nothing"
    );
    assert_eq!(
        ui::review_cut_shift(&window),
        before_shift,
        "nor shifts anything"
    );
    assert_eq!(
        ui::line_position(&window).t,
        before_line,
        "nor moves the line"
    );
    assert!(
        matches!(gesture, cut_trim::Gesture::Click),
        "and the rule answers a click: {gesture:?}"
    );
    drop(window);
}

/// S1 ("picture scrubs live (throttled)"): the running picture is re-seeked at most once per
/// `cut_trim::SCRUB_MS`, so a drag that fires an update per mouse-move event does not spend one accurate seek
/// — and one pipeline flush — per pixel. The throttle is the whole of S1's "live" rule that lives in `src` as
/// a pure decision, so it is checked here as one; what the seek is pointed AT is `edge_second`, asserted in
/// [`f2_8_s2_a_trim_and_a_shift_correction_survive_reopen`] against the seconds the cut actually holds.
#[test]
fn f2_8_s1_a_live_scrub_is_offered_at_most_one_seek_per_scrub_window() {
    let first = cut_trim::scrubs(None, 50);
    let too_soon = cut_trim::scrubs(Some(0), 50);
    let due = cut_trim::scrubs(Some(0), cut_trim::SCRUB_MS);
    println!(
        "SCRUB THROTTLE: never-scrubbed={first} at 50 ms={too_soon} at {} ms={due}",
        cut_trim::SCRUB_MS
    );
    assert!(first, "a drag that has not scrubbed yet always does");
    assert!(
        !too_soon,
        "and {} ms after a seek the picture waits",
        cut_trim::SCRUB_MS
    );
    assert!(
        due,
        "one SCRUB_MS window later it goes again — exactly one seek per window"
    );
}

// The live-player leg of S1 — a real `-s` reaching a running mpv — is NOT covered by this file:
// `cut_record_play::files_for` builds its file list from the session's own sources and drops a run whose file
// it cannot resolve, which no test here can stage; `tests/suite/f2_1_recording_play_widgets.rs` covers ▶ itself.

#[test]
fn f2_8_s1_s2_s3_the_page_reads_and_writes_what_a_press_actually_landed_on() {
    let app = adw::Application::builder()
        .application_id("org.naivepost.F2_8Live")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        f2_8_s2_a_press_on_the_recorders_band_names_that_lane(app);
        RAN_LANE_PLATE.store(true, Ordering::SeqCst);
        f2_8_s2_a_press_on_the_wave_strip_names_the_recording_under_the_pointer(app);
        RAN_WAVE_PLATE.store(true, Ordering::SeqCst);
        f2_8_s2_ruler_ground_and_a_border_take_no_band(app);
        RAN_NO_BAND.store(true, Ordering::SeqCst);
        f2_8_s2_only_a_selection_the_press_falls_inside_slides_its_scenes(app);
        RAN_INSIDE_BAND.store(true, Ordering::SeqCst);
        f2_8_s2_a_trim_and_a_shift_correction_survive_reopen(app);
        RAN_PERSIST.store(true, Ordering::SeqCst);
        f2_8_s2_one_undo_per_gesture_on_the_move_path(app);
        RAN_ONE_UNDO.store(true, Ordering::SeqCst);
        f2_8_s2_folds_near_the_grab_open_for_the_drag_and_refold_after(app);
        RAN_FOLDS.store(true, Ordering::SeqCst);
        f2_8_s3_an_unmoved_right_press_is_a_click_and_says_this_one(app);
        RAN_CLICK.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    let ran: [(&str, &AtomicBool); 8] = [
        ("lane plate", &RAN_LANE_PLATE),
        ("wave plate", &RAN_WAVE_PLATE),
        ("ruler ground / border", &RAN_NO_BAND),
        ("inside-band selection", &RAN_INSIDE_BAND),
        ("persistence", &RAN_PERSIST),
        ("one undo per gesture", &RAN_ONE_UNDO),
        ("folds for the drag", &RAN_FOLDS),
        ("unmoved press is a click", &RAN_CLICK),
    ];
    for (what, flag) in ran {
        assert!(flag.load(Ordering::SeqCst), "the {what} check never ran");
    }
}
