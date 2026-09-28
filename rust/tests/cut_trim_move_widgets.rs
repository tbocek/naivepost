// F2.8 — the wire: a right-drag on `track-strip` moves through `cut_trim::right_gesture` and never
// touches the red line, and a border drag lands where `cut_trim::clamp_edge` says. The logic tests
// (tests/cut_trim_gesture.rs, tests/cut_trim_move.rs) prove the rules; this proves a drag arrives.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
// state (this window's cut slot, folds and status line), so they run once and report through flags the
// test asserts on — the shape tests/cut_verb_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_trim as trim;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text};

static RAN_MOVE: AtomicBool = AtomicBool::new(false);
static RAN_CLICK: AtomicBool = AtomicBool::new(false);
static RAN_TRIM: AtomicBool = AtomicBool::new(false);
static RAN_REAL_TRIM: AtomicBool = AtomicBool::new(false);
static RAN_WAVE: AtomicBool = AtomicBool::new(false);
static RAN_SEL_SLIDE: AtomicBool = AtomicBool::new(false);
static RAN_RIGHT_TRIM: AtomicBool = AtomicBool::new(false);
static RAN_UNMOVED: AtomicBool = AtomicBool::new(false);

/// A footage scene on camera 0.
fn clip(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        cam: 0,
        ..Default::default()
    }
}

/// A window on the Cut page with a three-scene cut seeded, so a slide has neighbours to be bounded by.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);

    let mut seeded = cut::Cut::default();
    seeded.segs = vec![clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)];
    ui::seed_review_cut(&window, &seeded);
    // The press fell on the middle scene, with no band under it: the third question the flowchart asks.
    ui::set_press_scene(Some(1));
    ui::set_press_band(None);
    window
}

/// Let the main context run what the widget emissions queued.

/// S2 + S3: the real button-3 gesture moves the scene, prints exactly the rule's sentence, and leaves
/// the playhead alone.
fn check_a_right_drag_moves_and_never_touches_the_line(app: &adw::Application) {
    let window = cut_window(app);

    let strip = ui::track_strip(&window).expect("the Cut page carries track-strip");
    println!(
        "STRIP {}x{}",
        strip.width(),
        strip.height()
    );
    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    assert!(
        mover
            .upcast_ref::<gtk::EventController>()
            .widget()
            .is_some(),
        "the move gesture is attached to a widget, not floating unclaimed"
    );

    // The line before the drag. S3 says a right click never moves it, so this is the number compared
    // after every emission below.
    let line_before = ui::line_position(&window);
    println!("LINE BEFORE {line_before:?}");

    // Fire the gesture the way GTK does: begin, update 40 px along, end. The gesture reports its OWN
    // offset from the press (that is what `press_move` has always received), so the press x lives in the
    // seam below and these stay offsets. The press is placed mid-scene 1 — 90 px at this zoom, away from
    // every border's EDGE_GRAB_PX reach — so the right button slides; a press ON a border is S1's
    // question and `f2_8_s1_the_right_button_on_a_border_trims_it` drives that.
    const PRESS_X: f64 = 90.0;
    ui::set_press_x(PRESS_X);
    mover.emit_by_name::<()>("drag-begin", &[&0.0f64, &0.0f64]);
    mover.emit_by_name::<()>("drag-update", &[&40.0f64, &0.0f64]);
    settle();

    // The same call the seam made, computed here from the same inputs: this test compares against the
    // rule rather than restating it.
    let expected = trim::right_gesture(
        &trim::Press {
            scene: Some(1),
            row: 0,
            ..Default::default()
        },
        40.0,
        false,
        40.0 / ui::TRACK_STRIP_PPS,
        &[clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)],
        0.0,
        ui::TRACK_REC_END,
        ui::TRACK_STRIP_PPS,
        &[],
        &std::collections::BTreeMap::new(),
        &[],
        false,
    );
    let printed = status_text(&window);
    println!("MOVE PRINTED {printed}");
    match &expected {
        trim::Gesture::Slid { status, .. } => {
            assert_eq!(
                printed, *status,
                "the right-drag reached cut_trim::right_gesture and printed its own sentence"
            );
            assert!(
                status.contains("moved +"),
                "a slide says how far the clock moved: {status}"
            );
        }
        trim::Gesture::Click => panic!("40 px of travel must be a drag, not a click"),
    }

    // S3: the red line did not budge.
    assert_eq!(
        ui::line_position(&window),
        line_before,
        "a right drag never moves the line"
    );
    assert!(
        !trim::right_press_moves_line(),
        "the guard the page leans on says the right press never moves the line"
    );

    mover.emit_by_name::<()>("drag-end", &[&40.0f64, &0.0f64]);
    settle();
    assert_eq!(ui::line_position(&window), line_before, "and still not after release");

    window.close();
}

/// S3: an unmoved press is a click — nothing printed, nothing moved, the cut untouched.
fn check_an_unmoved_press_is_a_click(app: &adw::Application) {
    let window = cut_window(app);
    let before = status_text(&window);
    let segs_before = ui::review_cut_segs_count(&window);

    let gesture = ui::press_move(&window, 0.0, false, 0.0);
    assert_eq!(gesture, trim::Gesture::Click, "zero travel is a click");
    assert_eq!(
        status_text(&window),
        before,
        "a click prints no sentence about a change"
    );
    assert_eq!(
        ui::review_cut_segs_count(&window),
        segs_before,
        "a click changes no segments"
    );
    assert!(
        ui::open_folds(&window).is_empty(),
        "a click opens no folds"
    );

    // Below the slop is still a click even with distance asked for: the gate is the travel, not d.
    let just_short = ui::press_move(&window, trim::ROW_TRAVEL_PX - 0.5, false, 1.0);
    assert_eq!(just_short, trim::Gesture::Click);

    window.close();
}

/// S1: the trim seam writes exactly the clamped edge and prints the release sentence.
fn check_a_border_drag_lands_where_the_rule_says(app: &adw::Application) {
    let window = cut_window(app);

    // Drag the held scene's END to 45 s: the next clip starts at 40, so that is the wall the hand meets.
    let target = 45.0;
    let landed = ui::press_trim_border(&window, 1, trim::Border::End, target);
    let expected = trim::clamp_edge(
        &[clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)],
        1,
        trim::Border::End,
        target,
        0.0,
        ui::TRACK_REC_END,
    );
    println!("TRIM LANDED {landed} EXPECTED {expected}");
    assert_eq!(
        landed, expected,
        "the seam wrote clamp_edge's answer, not its own idea of the bound"
    );
    assert_eq!(
        landed, 40.0,
        "the next clip's start is an exact stop, not a slope leading to it"
    );

    let printed = status_text(&window);
    println!("TRIM PRINTED {printed}");
    assert!(
        printed.starts_with("clip 2:") || printed.starts_with("joined into one scene"),
        "the release sentence names the clip or the join: {printed}"
    );

    // A trim that ends flush against a neighbour within a frame joins them; one that does not, keeps both.
    let joined = ui::press_trim_border(&window, 1, trim::Border::Start, 9.5);
    println!("TRIM JOIN LANDED {joined}");
    let after = status_text(&window);
    assert!(
        after.contains("Undo puts the border back") || after.starts_with("clip 2:"),
        "either a join was reported or the clip simply moved: {after}"
    );

    window.close();
}

/// S1 through the real widget: a left-drag that begins within `cut_trim::EDGE_GRAB_PX` of a drawn clip
/// border takes THAT border, lands where `cut_trim::clamp_edge` says, and prints the release sentence into
/// the status line. An unmoved press on the same pixel changes no segment at all (S3).
fn check_a_border_drag_on_the_real_strip_trims_the_clip(app: &adw::Application) {
    let window = cut_window(app);
    let strip = ui::track_strip(&window).expect("the Cut page carries track-strip");
    let trimmer = ui::trim_gesture(&window).expect("the strip carries the trim gesture");
    assert!(
        trimmer
            .upcast_ref::<gtk::EventController>()
            .widget()
            .is_some(),
        "the trim gesture is attached to a widget, not floating unclaimed"
    );

    // The pixel the hand aims at: clip 2's END sits at 30 s * TRACK_STRIP_PPS, well inside the reach.
    let boxes = trim::clip_boxes(&ui::review_cut_segs(&window), ui::TRACK_STRIP_PPS);
    let end_px = boxes[1].x + boxes[1].w;
    assert!(
        trim::border_at(&boxes, end_px - 2.0) == Some((1, trim::Border::End)),
        "the press x must fall on clip 2's end border, or this proves nothing"
    );

    // Drag it to 39.5 s — half a second short of the next clip, which starts at 40 — and see where the
    // rule put it. A target past 40 would be clamped onto the neighbour's start and read as no movement
    // at all, which proves nothing about the hand.
    let target = 39.5;
    let target_px = target * ui::TRACK_STRIP_PPS;
    trimmer.emit_by_name::<()>("drag-begin", &[&end_px, &0.0f64]);
    settle();
    trimmer.emit_by_name::<()>("drag-update", &[&target_px, &0.0f64]);
    settle();
    trimmer.emit_by_name::<()>("drag-end", &[&target_px, &0.0f64]);
    settle();

    let after = ui::review_cut_segs(&window);
    let expected = trim::clamp_edge(
        &[clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)],
        1,
        trim::Border::End,
        target,
        0.0,
        ui::TRACK_REC_END,
    );
    assert_eq!(
        after[1].e, expected,
        "the dragged end landed where the rule said, not where the pointer did"
    );
    assert_eq!(after.len(), 3, "a drop half a second clear of the neighbour joins nothing");
    assert_eq!(
        status_text(&window),
        trim::trim_status(2, after[1].s, after[1].e),
        "the status line IS the release sentence, spelled by the rule"
    );
    // The other clips never moved: only the border under the hand was touched.
    assert_eq!((after[0].s, after[0].e), (0.0, 9.5));
    assert_eq!((after[2].s, after[2].e), (40.0, 70.0));

    // S1's plain trim, no join: drag the first clip's end to 5 s. The next clip starts at 10, so nothing
    // comes within a frame and the release says only where the clip now runs.
    let plain = ui::review_cut_segs(&window)[0].e * ui::TRACK_STRIP_PPS;
    let plain_to = 5.0 * ui::TRACK_STRIP_PPS;
    trimmer.emit_by_name::<()>("drag-begin", &[&plain, &0.0f64]);
    settle();
    trimmer.emit_by_name::<()>("drag-update", &[&plain_to, &0.0f64]);
    settle();
    trimmer.emit_by_name::<()>("drag-end", &[&plain_to, &0.0f64]);
    settle();
    let trimmed = ui::review_cut_segs(&window);
    assert_eq!(trimmed.len(), 3, "a trim with room around it joins nothing");
    assert_eq!(trimmed[0].e, 5.0);
    assert_eq!(
        status_text(&window),
        trim::trim_status(1, trimmed[0].s, trimmed[0].e),
        "the status line IS the release sentence, spelled by the rule"
    );

    // S3: an unmoved press on that very border is a click — the strip repaints, the cut does not.
    let before_len = ui::review_cut_segs_count(&window);
    let before_end = ui::review_cut_segs(&window)[0].e;
    let grab_px = ui::review_cut_segs(&window)[0].e * ui::TRACK_STRIP_PPS;
    trimmer.emit_by_name::<()>("drag-begin", &[&grab_px, &0.0f64]);
    settle();
    trimmer.emit_by_name::<()>("drag-end", &[&grab_px, &0.0f64]);
    settle();
    assert_eq!(
        ui::review_cut_segs(&window)[0].e, before_end,
        "a press that went nowhere trimmed nothing"
    );
    assert_eq!(ui::review_cut_segs_count(&window), before_len);
    let _ = strip;
    window.close();
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_a_right_drag_moves_and_never_touches_the_line(app);
            RAN_MOVE.store(true, Ordering::SeqCst);
            check_an_unmoved_press_is_a_click(app);
            RAN_CLICK.store(true, Ordering::SeqCst);
            check_a_right_drag_on_a_wave_strip_slides_that_recording(app);
            RAN_WAVE.store(true, Ordering::SeqCst);
            check_a_right_drag_inside_a_selection_slides_the_selected_scenes(app);
            RAN_SEL_SLIDE.store(true, Ordering::SeqCst);
            check_the_right_button_on_a_border_trims_it(app);
            RAN_RIGHT_TRIM.store(true, Ordering::SeqCst);
            check_an_unmoved_right_press_changes_nothing_through_the_widget(app);
            RAN_UNMOVED.store(true, Ordering::SeqCst);
            check_a_border_drag_lands_where_the_rule_says(app);
            RAN_TRIM.store(true, Ordering::SeqCst);
            check_a_border_drag_on_the_real_strip_trims_the_clip(app);
            RAN_REAL_TRIM.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_8_s2_a_right_drag_on_the_track_strip_moves_and_never_touches_the_line() {
    window_round();
    assert!(
        RAN_MOVE.load(Ordering::SeqCst),
        "the right-drag-moves check never ran"
    );
    assert!(
        RAN_CLICK.load(Ordering::SeqCst),
        "the unmoved-press-is-a-click check never ran"
    );
    assert!(
        RAN_TRIM.load(Ordering::SeqCst),
        "the border-trim check never ran"
    );
    assert!(
        RAN_REAL_TRIM.load(Ordering::SeqCst),
        "the real-strip border-drag check never ran"
    );
    assert!(
        RAN_WAVE.load(Ordering::SeqCst),
        "the wave-strip slide check never ran"
    );
    assert!(
        RAN_SEL_SLIDE.load(Ordering::SeqCst),
        "the selection-slide check never ran"
    );
    assert!(
        RAN_RIGHT_TRIM.load(Ordering::SeqCst),
        "the right-button border-trim check never ran"
    );
    assert!(
        RAN_UNMOVED.load(Ordering::SeqCst),
        "the unmoved-press-changes-nothing check never ran"
    );
}

// --- F2.8 S1/S2/S3: each flowchart branch reached through the REAL button-3 gesture -------------------
// The rule is proved in tests/cut_trim_move.rs; these prove a press on `track-strip` arrives at the right
// branch. Nothing here calls `press_move` directly to produce its answer: the answer comes from the state
// the gesture's own path wrote (the window's cut, its shift map and the status line), plus
// `ui::right_press_took`, which is the same door the drag callback uses.

/// The recording name a band hit resolves to. `set_press_band` supplies only the NAME; which band was hit
/// comes from the press y, so priming the name is not priming the branch.
const WAVE_REC: &str = "cam7";

/// A window with a three-scene cut seeded and NO scene seam primed, so nothing can outrank the bands.
fn bare_cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let window = cut_window(app);
    ui::set_press_scene(None);
    ui::set_press_x(0.0);
    window
}

/// F2.8 S2: a right-drag beginning at a y inside the WAVE band slides that one recording, with no scene
/// seam primed -- flowchart M2 reached through the widget, not through a seam.
fn check_a_right_drag_on_a_wave_strip_slides_that_recording(app: &adw::Application) {
    let window = bare_cut_window(app);
    ui::set_press_band(Some(WAVE_REC.to_string()));
    let bands = trim::PLACEHOLDER_STRIP_BANDS;
    // A y inside the wave band, and an x mid-scene 1 (40..120 px at this zoom) so no border steals it.
    let wave_y = bands.wave.0 + (bands.wave.1 - bands.wave.0) / 2.0;
    let press_x = 90.0;
    assert_eq!(
        trim::band_at(wave_y, &bands),
        trim::Band::Wave,
        "the y used below really is inside the wave band"
    );
    // The same door the drag callback reads, BEFORE any emission: band ground, not the recorders' band.
    let (recorders, wave, on_border) = ui::right_press_took(press_x, wave_y);
    assert_eq!(wave.as_deref(), Some(WAVE_REC), "the wave strip took that recording");
    assert_eq!(recorders, None, "and it did NOT come back as the recorders' band");
    assert!(!on_border, "mid-scene x is not a border");

    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    ui::set_press_x(press_x);
    mover.emit_by_name::<()>("drag-begin", &[&0.0f64, &wave_y]);
    // >3 px of travel, or the gesture never opens (S3's slop).
    mover.emit_by_name::<()>("drag-update", &[&24.0f64, &wave_y]);
    settle();

    let printed = status_text(&window);
    println!("WAVE SLIDE PRINTED {printed}");
    assert!(
        printed.contains("the recording moved +"),
        "a wave-strip drag says THE RECORDING moved, not the scene or the row: {printed}"
    );
    assert!(
        !printed.contains("the scene moved"),
        "it must not read as a scene slide: {printed}"
    );
    assert!(
        !printed.contains("camera row"),
        "nor as a whole-row shift: {printed}"
    );
    // And the shift landed on the named recording only -- real cut state, written by apply_gesture.
    let shifted = ui::review_cut_shift(&window);
    assert!(
        shifted.contains_key(WAVE_REC),
        "the shift map holds the recording the wave strip named: {shifted:?}"
    );
    assert!(
        shifted[WAVE_REC].abs() > f64::EPSILON,
        "and it actually moved: {:?}",
        shifted[WAVE_REC]
    );
    window.close();
}

/// F2.8 S2: a right-drag that STARTS inside a footage selection, away from any border, slides the
/// SELECTED scenes -- flowchart M3, ahead of the scene-under-the-pointer branch.
fn check_a_right_drag_inside_a_selection_slides_the_selected_scenes(app: &adw::Application) {
    use naivepost::cut_select::Surface;
    let window = bare_cut_window(app);
    ui::set_press_band(None);
    // A real band over scenes 1 and 2 (10..30 s and 40..70 s are inside it).
    let band = ui::draw_selection(&window, Surface::PictureRow(0), None, 5.0, 75.0)
        .expect("a band draws");
    assert!(band.length() > 1.0, "the band is long enough to be a selection");

    // Press at x=90 px (mid-scene 1) and at a NON-band y (the ruler), so the selection is what answers.
    let press_x = 90.0;
    let ruler_y = 4.0;
    let (_, _, on_border) = ui::right_press_took(press_x, ruler_y);
    assert!(!on_border, "away from a border, so S2 may slide the selection");

    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    ui::set_press_x(press_x);
    mover.emit_by_name::<()>("drag-begin", &[&0.0f64, &ruler_y]);
    mover.emit_by_name::<()>("drag-update", &[&20.0f64, &ruler_y]);
    settle();

    let printed = status_text(&window);
    println!("SELECTION SLIDE PRINTED {printed}");
    assert!(
        printed.contains("the selected scenes moved"),
        "a press inside a footage selection slides the SELECTED SCENES: {printed}"
    );
    // The band itself is untouched by the press -- selecting is not clearing.
    assert!(ui::selection(&window).is_some(), "the selection survived the press");
    window.close();
}

/// F2.8 S1: the RIGHT button, pressed within EDGE_GRAB_PX of a drawn clip border, TRIMS that border.
/// Asserted on the SEG moving (real cut state), not on a slide sentence.
fn check_the_right_button_on_a_border_trims_it(app: &adw::Application) {
    let window = bare_cut_window(app);
    ui::set_press_band(None);
    let segs_before = ui::review_cut_segs(&window);
    assert_eq!(segs_before.len(), 3, "three seeded scenes");
    // Scene 2 runs 40.0 .. 70.0 s => its start edge sits at 40 * TRACK_STRIP_PPS px. Grabbing THAT one
    // rather than scene 1's keeps this check unambiguous: scene 1's start (40 px) and scene 2's start
    // (160 px) are far apart, and nothing else in the fixture has an edge near 160 px.
    let start_edge_px = 40.0 * ui::TRACK_STRIP_PPS;
    let grab_x = start_edge_px + 3.0; // well inside the 6 px reach
    let bands = trim::PLACEHOLDER_STRIP_BANDS;
    let bar_y = bands.bar.0 + 1.0;

    // The wire's own first question, answered the way the callback answers it.
    let (_, _, on_border) = ui::right_press_took(grab_x, bar_y);
    assert!(on_border, "x={grab_x} is within EDGE_GRAB_PX of scene 2's start border");

    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    ui::set_press_x(grab_x);
    mover.emit_by_name::<()>("drag-begin", &[&0.0f64, &bar_y]);
    // The update carries an OFFSET from the press, so the pointer sits at grab_x + offset. Dragging 12 s
    // along puts it at (grab_x + 48 px)/PPS = 56.0 s; scene 2 ends at 70 s and the recording at
    // TRACK_REC_END, so nothing clamps it and the grabbed border lands exactly where the pointer is.
    let target_s = (grab_x + 48.0) / ui::TRACK_STRIP_PPS;
    mover.emit_by_name::<()>("drag-update", &[&48.0f64, &bar_y]);
    settle();

    let segs_after = ui::review_cut_segs(&window);
    println!(
        "BORDER TRIM scene2 before {:?}..{:?} after {:?}..{:?}",
        segs_before[2].s, segs_before[2].e, segs_after[2].s, segs_after[2].e
    );
    assert!(
        (segs_after[2].s - target_s).abs() < 0.001,
        "scene 2's START moved to where the right-button drag put it: {} vs {target_s}",
        segs_after[2].s
    );
    assert_eq!(
        segs_after[2].e, segs_before[2].e,
        "only the grabbed border moved -- its end is untouched"
    );
    assert_eq!(
        (segs_after[1].s, segs_after[1].e),
        (segs_before[1].s, segs_before[1].e),
        "and no OTHER scene moved -- this was a border trim, not a slide"
    );
    assert_eq!(
        segs_after.len(),
        segs_before.len(),
        "a trim does not add or remove scenes"
    );
    let printed = status_text(&window);
    assert!(
        !printed.contains("moved +"),
        "this was a TRIM, not a slide -- no 'moved +' sentence: {printed}"
    );
    window.close();
}

/// F2.8 S3: a right press that never travels is a click. `drag-begin` alone leaves the cut, the
/// selection and the red line exactly as they were.
fn check_an_unmoved_right_press_changes_nothing_through_the_widget(app: &adw::Application) {
    let window = bare_cut_window(app);
    ui::set_press_band(Some(WAVE_REC.to_string()));
    let segs_before = ui::review_cut_segs(&window);
    let shift_before = ui::review_cut_shift(&window);
    let line_before = ui::line_position(&window);
    let status_before = status_text(&window);

    let mover = ui::move_gesture(&window).expect("the strip carries the move gesture");
    let bands = trim::PLACEHOLDER_STRIP_BANDS;
    let wave_y = bands.wave.0 + 1.0;
    ui::set_press_x(90.0);
    // BEGIN ONLY -- no update, no end. No travel means no gesture.
    mover.emit_by_name::<()>("drag-begin", &[&0.0f64, &wave_y]);
    settle();

    assert_eq!(
        ui::review_cut_segs(&window).len(),
        segs_before.len(),
        "an unmoved press added or removed no scene"
    );
    for (i, (before, after)) in segs_before.iter().zip(ui::review_cut_segs(&window).iter()).enumerate() {
        assert_eq!(
            (before.s, before.e),
            (after.s, after.e),
            "scene {i} unchanged by an unmoved press"
        );
    }
    assert_eq!(
        ui::review_cut_shift(&window),
        shift_before,
        "no shift was pushed for a press that never moved"
    );
    assert_eq!(
        ui::line_position(&window).t,
        line_before.t,
        "\"a right click never moves the line\""
    );
    assert_eq!(
        status_text(&window),
        status_before,
        "and it prints nothing new"
    );
    window.close();
}
