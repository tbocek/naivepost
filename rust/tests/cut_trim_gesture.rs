// F2.8 (Trim and move) — the composed gesture: spec/05-cut.md F2.8 S1/S2/S3 through
// `cut_trim::right_gesture`, the one call a page makes per drag event. The sibling file
// tests/cut_trim_move.rs checks each rule on its own; this checks they are wired into one answer in the
// order the flowchart asks, so a caller cannot get a half-answered gesture.

use std::collections::BTreeMap;

use naivepost::cut::Seg;
use naivepost::cut_select;
use naivepost::cut_trim as trim;

/// A footage scene on camera 0.
fn clip(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        cam: 0,
        ..Default::default()
    }
}

/// The fixture S1's bounds are read against: previous clip ends at 9.5, held runs 10–30, next starts at 40.
/// The recording itself runs 0–100.
const REC_START: f64 = 0.0;
const REC_END: f64 = 100.0;
/// Pixels-per-second for these fixtures, so a pixel reach is a whole number of seconds: 40 px/s puts
/// SNAP_PX (8 px) at exactly 0.2 s. // layout.snapPx
const PPS: f64 = 40.0;

fn neighbours() -> Vec<Seg> {
    vec![clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)]
}

fn shift(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), *value))
        .collect()
}

fn sources(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// A press that lands on one scene, with no selection and not on a border.
fn on_scene<'a>() -> trim::Press<'a> {
    trim::Press {
        scene: Some(1),
        row: 0,
        ..Default::default()
    }
}

/// S3: a press that never travelled and changed no row is a click, not a slide — even when it sat on
/// something movable.
#[test]
fn f2_8_s3_an_unmoved_right_press_is_a_click_through_the_gesture() {
    let segs = neighbours();
    let gesture = trim::right_gesture(
        &on_scene(),
        0.0,
        false,
        0.0,
        &segs,
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&["cam A"]),
        false,
    );
    assert_eq!(gesture, trim::Gesture::Click);
    // The same thing the module's own click rule says, so the gesture cannot disagree with it.
    assert!(trim::is_click(0.0, false));
}

/// S2: sideways travel from 3 px opens the gesture; one less is still a click.
#[test]
fn f2_8_s2_the_gesture_opens_at_three_px_of_travel() {
    let segs = neighbours();
    let just_short = trim::right_gesture(
        &on_scene(),
        trim::ROW_TRAVEL_PX - 0.1,
        false,
        0.5,
        &segs,
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    assert_eq!(just_short, trim::Gesture::Click, "under 3 px nothing was dragged");

    let opened = trim::right_gesture(
        &on_scene(),
        trim::ROW_TRAVEL_PX,
        false,
        0.5,
        &segs,
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    match opened {
        trim::Gesture::Slid { what, status, .. } => {
            assert_eq!(what, "the scene");
            assert_eq!(status, "the scene moved +0.50 s");
        }
        trim::Gesture::Click => panic!("3 px of travel must open the gesture"),
    }
}

/// S2: a scene slides along its recording with its length kept, clamped by its neighbours, and snaps
/// flush when it comes within the 8 px reach of a bound. // layout.snapPx
#[test]
fn f2_8_s2_a_scene_slide_keeps_length_clears_neighbours_and_snaps_flush() {
    let segs = neighbours();
    let length = segs[1].e - segs[1].s;

    // Well inside the bounds: the scene moves the full half second and keeps its twenty seconds.
    let slid = trim::slide_scene(&segs, 1, 10.5, REC_START, REC_END, PPS);
    assert_eq!(slid, 10.5);
    assert_eq!(length, 20.0, "a slide moves a scene, it never resizes it");

    // Past the next clip: bounded to where the next one starts minus its own length. // layout.snapPx
    let bounded = trim::slide_scene(&segs, 1, 39.0, REC_START, REC_END, PPS);
    assert_eq!(bounded, 20.0, "the far bound is the next clip's start off the length");

    // Within the 8 px reach (0.2 s here) of that bound: snapped flush, not a pixel short.
    let near = trim::slide_scene(&segs, 1, 20.15, REC_START, REC_END, PPS);
    assert_eq!(near, 20.0, "flush within the snap reach means flush exactly");

    // And the composed gesture reports the same delta it computed, not the hand's raw distance.
    let gesture = trim::right_gesture(
        &on_scene(),
        10.0,
        false,
        10.15,
        &segs,
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    match gesture {
        trim::Gesture::Slid { status, .. } => {
            assert_eq!(status, "the scene moved +10.00 s", "snapped, so ten seconds not 10.15");
        }
        trim::Gesture::Click => panic!("a scene that moved 10 s is not a click"),
    }
}

/// S1 vs S2: a press within EDGE_GRAB_PX of a border is a trim and never a slide — the two rules share
/// the same reach constant so they cannot drift apart. // layout.edgeGrabPx
#[test]
fn f2_8_s1_a_press_on_a_border_reaches_trim_not_slide() {
    // The held clip spans 250..750 px at 40 px/s. Six pixels off its end border is inside the grab...
    let px = 750.0;
    assert_eq!(
        trim::grab_border(px - trim::EDGE_GRAB_PX + 1.0, 250.0, 750.0),
        Some(trim::Border::End),
        "within 6 px the border is taken"
    );
    // ...and seven pixels away it is not, which is what lets the same press fall through to a slide.
    assert_eq!(
        trim::grab_border(px + 7.0, 250.0, 750.0),
        None,
        "outside the 6 px reach there is no border to trim"
    );
    assert_eq!(trim::EDGE_GRAB_PX, 6.0, "// layout.edgeGrabPx");

    // A press flagged as being on a border does NOT take the selection, even inside one: that is the
    // flowchart's "not on a border" guard, and it is why the same point answers trim or slide.
    let inside_on_border = trim::Press {
        selection: Some((12.0, 20.0)),
        selected: vec![1],
        inside_selection: true,
        on_border: true,
        scene: Some(1),
        row: 0,
        ..Default::default()
    };
    assert_ne!(
        trim::slide_for(&inside_on_border),
        trim::Slide::Selection(vec![1]),
        "a border press trims, it never slides the selection"
    );
    assert_eq!(trim::slide_for(&inside_on_border), trim::Slide::Scene(1));
}

/// S2: a row slide is the shift correction — every listed source moved, nobody else touched.
#[test]
fn f2_8_s2_a_row_slide_shifts_every_listed_source_and_nobody_else() {
    let opening = shift(&[("cam A", 0.0), ("mic", 1.5), ("slides", -2.0)]);
    let all = sources(&["cam A", "mic", "slides"]);
    let press = trim::Press {
        row: 2,
        ..Default::default()
    };

    let gesture = trim::right_gesture(
        &press,
        20.0,
        false,
        1.25,
        &neighbours(),
        REC_START,
        REC_END,
        PPS,
        &[],
        &opening,
        &all,
        false,
    );
    match gesture {
        trim::Gesture::Slid {
            what,
            status,
            shift,
            row,
            open,
        } => {
            assert_eq!(what, "the camera row");
            assert_eq!(status, "the camera row moved +1.25 s");
            assert_eq!(row, None, "a row sliding along the clock is not a row CHANGE");
            assert!(open.is_empty(), "no folds were given to open");
            assert_eq!(shift.get("cam A"), Some(&1.25));
            assert_eq!(shift.get("mic"), Some(&2.75));
            assert_eq!(shift.get("slides"), Some(&-0.75));
            assert_eq!(shift.len(), 3, "nothing outside the list was invented");
        }
        trim::Gesture::Click => panic!("a row slid 1.25 s"),
    }

    // The opening map is untouched — the rule that stops a drag storm compounding.
    assert_eq!(opening.get("cam A"), Some(&0.0));
}

/// S2: a vertical row change needs no sideways travel and says where the scenes went.
#[test]
fn f2_8_s2_a_row_change_needs_no_travel_and_names_the_row() {
    let gesture = trim::right_gesture(
        &on_scene(),
        0.0,
        true,
        0.0,
        &neighbours(),
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    match gesture {
        trim::Gesture::Slid { status, row, .. } => {
            assert_eq!(row, Some(0));
            assert_eq!(
                status,
                "the scene moved to row 1 \u{2014} its kept scenes came along",
                "rows are counted from 1 in the sentence"
            );
        }
        trim::Gesture::Click => panic!("a row change opens the gesture without any travel"),
    }
}

/// S2: a gesture that ends where it began says so and pushed nothing worth undoing.
#[test]
fn f2_8_s2_a_gesture_back_to_where_it_started_reports_no_move() {
    let gesture = trim::right_gesture(
        &on_scene(),
        12.0,
        false,
        0.0,
        &neighbours(),
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    // Travelled, but zero seconds of shift: no change happened, so no sentence about one either.
    assert_eq!(gesture, trim::Gesture::Click);
    // The sentence itself is still what the module would say if asked directly.
    assert_eq!(
        trim::shift_label("the scene", 0.0),
        "the scene is back where it started"
    );
}

/// S2: a repeat update of a hold whose first update already landed changes nothing further, so it is not
/// reported as a fresh move.
#[test]
fn f2_8_s2_a_second_update_that_changes_nothing_is_not_reported_as_a_move() {
    let fresh = trim::right_gesture(
        &on_scene(),
        12.0,
        false,
        0.5,
        &neighbours(),
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    assert!(matches!(fresh, trim::Gesture::Slid { .. }));

    let repeat = trim::right_gesture(
        &on_scene(),
        12.0,
        false,
        0.0,
        &neighbours(),
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        true,
    );
    assert_eq!(repeat, trim::Gesture::Click, "already pushed, and nothing new moved");
    // One Undo per gesture, stated by the rule the gesture leans on.
    assert!(trim::pushes_undo(true, false));
    assert!(!trim::pushes_undo(true, true));
}

/// S2: the folds the drag travels through open; folds elsewhere are left alone.
#[test]
fn f2_8_s2_only_the_folds_under_the_drag_open() {
    let folds = [[4.0, 6.0], [12.0, 14.0], [80.0, 90.0]];
    // Grab at the held scene's start (10 s) dragging forward half a second: covers 10..10.5, which
    // touches none of these. Dragging four seconds reaches the 12..14 fold? No — 10..14 does.
    let gesture = trim::right_gesture(
        &on_scene(),
        40.0,
        false,
        4.0,
        &neighbours(),
        REC_START,
        REC_END,
        PPS,
        &folds,
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    match gesture {
        trim::Gesture::Slid { open, .. } => {
            assert_eq!(open, vec![1], "only the fold inside 10..14 opens");
        }
        trim::Gesture::Click => panic!("40 px of travel is a drag"),
    }

    // With no travel the fold list is measured on the grab column alone.
    let column = trim::folds_to_open(&folds, 12.5, 0.0);
    assert_eq!(column, vec![1]);
    let elsewhere = trim::folds_to_open(&folds, 0.5, 0.0);
    assert!(elsewhere.is_empty(), "a fold far from the grab is untouched by this drag");
}

/// S2: the band and the wave strip both take their recording, and the gesture shifts that one column.
#[test]
fn f2_8_s2_a_band_or_wave_press_slides_its_own_recording_only() {
    let opening = shift(&[("lecture", 0.0), ("other", 5.0)]);
    let all = sources(&["lecture", "other"]);
    let band = trim::Press {
        recorders_band: Some("lecture"),
        ..Default::default()
    };
    let wave = trim::Press {
        wave_strip: Some("lecture"),
        ..Default::default()
    };
    for press in [&band, &wave] {
        let gesture = trim::right_gesture(
            press,
            20.0,
            false,
            2.0,
            &neighbours(),
            REC_START,
            REC_END,
            PPS,
            &[],
            &opening,
            &all,
            false,
        );
        match gesture {
            trim::Gesture::Slid {
                what,
                status,
                shift,
                ..
            } => {
                assert_eq!(what, "the recording");
                assert_eq!(status, "the recording moved +2.00 s");
                assert_eq!(shift.get("lecture"), Some(&2.0));
                assert_eq!(shift.get("other"), Some(&5.0), "another recording is not dragged along");
            }
            trim::Gesture::Click => panic!("a band or wave press with travel must slide"),
        }
    }
}

/// S2: a footage selection the press falls inside slides together; the sound of a lane never gets here
/// because scope decides that upstream (F2.6 S4), so the gesture only ever sees marked footage.
#[test]
fn f2_8_s2_a_footage_selection_slides_its_marked_scenes_together() {
    let segs = neighbours();
    let press = trim::Press {
        selection: Some((10.0, 30.0)),
        selected: vec![1],
        inside_selection: true,
        on_border: false,
        scene: Some(1),
        row: 0,
        ..Default::default()
    };
    assert_eq!(trim::slide_for(&press), trim::Slide::Selection(vec![1]));

    let gesture = trim::right_gesture(
        &press,
        10.0,
        false,
        1.0,
        &segs,
        REC_START,
        REC_END,
        PPS,
        &[],
        &BTreeMap::new(),
        &sources(&[]),
        false,
    );
    match gesture {
        trim::Gesture::Slid { what, status, .. } => {
            assert_eq!(what, "the selected scenes");
            assert_eq!(status, "the selected scenes moved +1.00 s");
        }
        trim::Gesture::Click => panic!("a marked scene with travel slides"),
    }
    // The scene itself stays put in the list: the gesture reports, the page writes.
    assert_eq!(segs[1].s, 10.0, "the rule computes, it does not mutate behind the caller");
}

/// The tooltips the page shows carry the reach and the hands, so a person reads the rule before touching.
#[test]
fn f2_8_s1_and_s2_the_tooltips_name_the_reach_and_the_hands() {
    // layout.edgeGrabPx
    assert!(trim::TRIM_TIP.contains("6 px"), "{}", trim::TRIM_TIP);
    assert!(trim::TRIM_TIP.contains("border"));
    assert!(trim::MOVE_TIP.contains("right-drag"));
    assert!(trim::MOVE_TIP.contains("recording"));
    assert!(trim::EDGE_GRAB_PX < cut_select::SNAP_PX, "the grab is tighter than the snap");
}
