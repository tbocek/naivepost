// F2.8 (Trim and move) — spec/05-cut.md F2.8.
// One test per step: dragging a clip's border and what the drop joins back to (S1), what the right button
// picks up and how far it may go (S2), and what an unmoved press is (S3). Every rule is a plain function in
// naivepost::cut_trim, so nothing here needs a widget or a display.

use std::collections::BTreeMap;

use naivepost::cut::Seg;
use naivepost::cut_select;
use naivepost::cut_trim as trim;

/// The clip under the hand throughout: 10–30 s of camera 0, in a recording that runs 0–100.
fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// The same fixtures S1's bounds are read against: a previous clip ending at 9.5 and a next one starting at 40.
fn neighbours() -> Vec<Seg> {
    vec![clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)]
}

/// F2.8 (Trim and move) S1: a border is taken within six pixels of it, and the nearer of two in reach wins.
#[test]
fn f2_8_s1_a_border_is_grabbed_within_six_px() {
    assert_eq!(trim::EDGE_GRAB_PX, 6.0, "spec/inventory/cut.md's \"edges within 6 px\"");
    // A clip drawn from 100 px to 300 px.
    assert_eq!(trim::grab_border(94.0, 100.0, 300.0), Some(trim::Border::Start), "exactly on the reach");
    assert_eq!(trim::grab_border(106.0, 100.0, 300.0), Some(trim::Border::Start), "the other side of it");
    assert_eq!(trim::grab_border(294.0, 100.0, 300.0), Some(trim::Border::End), "the far border's reach too");
    assert_eq!(trim::grab_border(93.0, 100.0, 300.0), None, "a pixel further is not a border");
    assert_eq!(trim::grab_border(200.0, 100.0, 300.0), None, "the middle of the clip is not either");

    // A clip narrower than two reaches, where both borders are in reach at once: the pointer is nearer one of
    // them, and that is the arrow it saw. (A 4 px clip drawn at 100–104 has its middle at 102, so 101 sits
    // nearer the start and 103 nearer the end.)
    assert_eq!(trim::grab_border(101.0, 100.0, 104.0), Some(trim::Border::Start), "nearer the start");
    assert_eq!(trim::grab_border(103.0, 100.0, 104.0), Some(trim::Border::End), "and nearer the end");
}

/// F2.8 (Trim and move) S1: an end will not go below a scene's worth of length, past the next clip, or past
/// the recording's end.
#[test]
fn f2_8_s1_an_end_stops_at_a_seconds_worth_the_next_clip_and_the_recordings_end() {
    let segs = neighbours();
    // P.policy.minSceneSeconds is that 1 s, and it is catalogued once — from cut_select.
    assert_eq!(cut_select::MIN_SCENE_SECONDS, 1.0);

    let free = trim::clamp_edge(&segs, 1, trim::Border::End, 30.2, 0.0, 100.0);
    assert_eq!(free, 30.2, "a border with room follows the pointer exactly");

    // The wall is at 40 and not a third of the way to it: an end dragged into the next clip stops exactly on
    // that clip's start, which is also where the merge on release will find a pair touching.
    let next = trim::clamp_edge(&segs, 1, trim::Border::End, 42.0, 0.0, 100.0);
    assert_eq!(next, 40.0, "the next clip's start is the bound");

    let short_of_it = trim::clamp_edge(&segs, 1, trim::Border::End, 39.0, 0.0, 100.0);
    assert_eq!(short_of_it, 39.0, "one second short of the wall is not an edit against it");

    let tail = trim::clamp_edge(&vec![clip(10.0, 30.0)], 0, trim::Border::End, 140.0, 0.0, 100.0);
    assert_eq!(tail, 100.0, "and the recording's end is the last second there is");

    let floor = trim::clamp_edge(&segs, 1, trim::Border::End, 10.0, 0.0, 100.0);
    assert_eq!(floor, 11.0, "an end never comes below start + 1 s");
}

/// F2.8 (Trim and move) S1: a start will not go above a scene's worth of length, below the previous clip, or
/// below the recording's start.
#[test]
fn f2_8_s1_a_start_stops_at_a_seconds_worth_the_previous_clip_and_the_recordings_start() {
    let segs = neighbours();

    let previous = trim::clamp_edge(&segs, 1, trim::Border::Start, 5.0, 0.0, 100.0);
    assert_eq!(previous, 9.5, "the previous clip's end bounds it, not the recording's start");

    let free = trim::clamp_edge(&segs, 1, trim::Border::Start, 9.4, 0.0, 100.0);
    assert_eq!(free, 9.5, "and never below it");

    let alone = vec![clip(10.0, 30.0)];
    let head = trim::clamp_edge(&alone, 0, trim::Border::Start, -8.0, 0.0, 100.0);
    assert_eq!(head, 0.0, "with nothing filmed before it the recording's start is the bound");

    let floor = trim::clamp_edge(&segs, 1, trim::Border::Start, 31.0, 0.0, 100.0);
    assert_eq!(floor, 29.0, "a start never goes above end − 1 s");
}

/// F2.8 (Trim and move) S1: a drag that lands back on the border is not an edit, so it costs no Undo step.
#[test]
fn f2_8_s1_a_drag_back_to_the_border_is_not_an_edit() {
    let segs = neighbours();
    assert!(!trim::trims(&segs, 1, trim::Border::End, 30.0, 0.0, 100.0), "the value it already has");
    // Past the next clip's start the wall holds at 40, which is a different second from this clip's own end —
    // an edit that costs an Undo step even though the pointer went further than the cut allowed.
    assert!(trim::trims(&segs, 1, trim::Border::End, 42.0, 0.0, 100.0), "a drag stopped by the wall still moved the border");
    // The floor is start + 1 s = 11, and a border already sitting on its own floor cannot be trimmed down:
    // the clamp answers the value it holds, so nothing is written and nothing is undone.
    let on_the_floor = vec![clip(0.0, 9.5), clip(10.0, 11.0), clip(40.0, 70.0)];
    assert!(!trim::trims(&on_the_floor, 1, trim::Border::End, 5.0, 0.0, 100.0), "a drag onto the floor that is already there");
    assert!(trim::trims(&segs, 1, trim::Border::End, 32.0, 0.0, 100.0), "one that moves is an edit");
    assert!(!trim::trims(&[], 4, trim::Border::Start, 12.0, 0.0, 100.0), "and a clip that is not there trims nothing");
}

/// F2.8 (Trim and move) S1: the picture scrubs live, throttled — a flushing seek per mouse-move event is a
/// pipeline that never stops flushing.
#[test]
fn f2_8_s1_the_picture_scrubs_at_most_every_90_ms() {
    assert_eq!(trim::SCRUB_MS, 90, "spec/10-parameters.md's \"scrub throttle 90 ms\"");
    assert!(trim::scrubs(None, 0), "the first tick always scrubs");
    assert!(!trim::scrubs(Some(0), 89), "the next 89 ms are not owed a seek");
    assert!(trim::scrubs(Some(0), 90), "a second's worth of ninety milliseconds on, it does");
}

/// F2.8 (Trim and move) S1: the picture lands on the edge — and an end shows the frame before the boundary,
/// because the frame at the boundary is already outside the clip.
#[test]
fn f2_8_s1_the_picture_lands_on_the_edge() {
    let frame = 1.0 / 30.0;
    assert_eq!(trim::scrub_second(trim::Border::Start, 10.0, 30.0, frame), 10.0, "a start shows itself");
    let end = trim::scrub_second(trim::Border::End, 10.0, 30.0, frame);
    assert!((end - (30.0 - frame)).abs() < 1e-12, "the last frame kept, not the first removed: {end}");
}

/// F2.8 (Trim and move) S1: what the page says as the border is picked up and as it is let go — clips counted
/// from 1, on the clock beside the red line.
#[test]
fn f2_8_s1_the_status_reads_the_clip_and_the_span() {
    assert_eq!(
        trim::trim_status(2, 12.0, 20.5),
        "clip 2: 00:12 \u{2013} 00:20 (8.5 s)",
        "the clip, the span it now covers, its length"
    );
    assert_eq!(trim::trim_status(1, 0.0, 20.0), "clip 1: 00:00 \u{2013} 00:20 (20.0 s)", "tenths at both ends");

    assert_eq!(
        trim::picked_up_status(2, trim::Border::End, 12.34),
        "clip 2's end picked up at 00:12.3 \u{2014} right-drag to trim",
        "spec/inventory/cut.md's wording, on the tenth clock"
    );
    assert_eq!(
        trim::picked_up_status(1, trim::Border::Start, 0.0),
        "clip 1's start picked up at 00:00.0 \u{2014} right-drag to trim"
    );
}

/// F2.8 (Trim and move) S1: a neighbour landed against within 0.04 s joins back into one scene, and the Split
/// border between them is gone with it.
#[test]
fn f2_8_s1_a_neighbour_within_004_s_joins_and_clears_the_split_border() {
    // P.eng.minPieceSeconds is that tolerance — the same 0.04 s F2.6 floors a selection at.
    assert_eq!(cut_select::MIN_SECONDS, 0.04);

    let mut segs = vec![
        clip(10.0, 20.0),
        Seg { split: true, ..clip(20.02, 30.0) },
    ];
    assert_eq!(trim::merge_pair(&segs, 0), Some(1), "a fifth of a frame apart is touching");

    let joined = trim::merge(&mut segs, 0).expect("the pair joins");
    assert_eq!(joined, (0, 1), "one clip left the list, and the earlier one is still clip 1");
    assert_eq!(segs.len(), 1);
    assert_eq!((segs[0].s, segs[0].e), (10.0, 30.0), "widened over its neighbour");
    assert!(!segs[0].split, "the drag clears a Split border between them");

    assert_eq!(
        trim::joined_status(10.0, 30.0),
        "joined into one scene, 00:10 \u{2013} 00:30 (20.0 s) \u{2014} \u{21b6} Undo puts the border back",
        "the sentence that offers the border back"
    );

    // And joining from the later clip's side works too, keeping the earlier piece's seconds.
    let mut backwards = vec![clip(10.0, 20.0), clip(20.01, 30.0)];
    assert_eq!(trim::merge(&mut backwards, 1), Some((1, 1)));
    assert_eq!((backwards[0].s, backwards[0].e), (10.0, 30.0));
}

/// F2.8 (Trim and move) S1: a join never crosses a camera or takes an insert, and a gap a hand can see is not
/// a pair that failed to meet.
#[test]
fn f2_8_s1_a_join_never_crosses_a_camera_or_takes_an_insert() {
    let other_cam = vec![clip(10.0, 20.0), Seg { cam: 1, ..clip(20.01, 30.0) }];
    assert_eq!(trim::merge_pair(&other_cam, 0), None, "two cameras never become one clip");

    let card = vec![clip(10.0, 20.0), Seg { ins: "sting.mp4".into(), ..clip(20.01, 30.0) }];
    assert_eq!(trim::merge_pair(&card, 0), None, "a card brings its own edges");

    let held_card = vec![clip(10.0, 20.0), Seg { ins: "sting.mp4".into(), s: 20.01, e: 30.01, ..Default::default() }, clip(30.02, 40.0)];
    assert_eq!(trim::merge_pair(&held_card, 1), None, "and none merges a card away");

    let gap = vec![clip(10.0, 20.0), clip(20.05, 30.0)];
    assert_eq!(trim::merge_pair(&gap, 0), None, "0.05 s apart is a hole somebody left on purpose");
}

/// F2.8 (Trim and move) S2: the right button takes the first thing the flowchart finds — recording, then wave
/// strip, then the marked seconds, then one scene, then the row.
#[test]
fn f2_8_s2_the_right_press_takes_the_first_thing_it_finds() {
    let everything = trim::Press {
        recorders_band: Some("2026-09-16 17-26-20"),
        wave_strip: Some("own"),
        selection: Some((12.0, 20.0)),
        selected: vec![3],
        inside_selection: true,
        on_border: false,
        scene: Some(3),
        row: 1,
    };
    assert_eq!(trim::slide_for(&everything), trim::Slide::Recording("2026-09-16 17-26-20".into()), "the recorders' band outranks everything");

    let strip = trim::Press { recorders_band: None, ..everything.clone() };
    assert_eq!(trim::slide_for(&strip), trim::Slide::Recording("own".into()), "then the wave strip under the pointer");

    let marked = trim::Press { recorders_band: None, wave_strip: None, ..everything.clone() };
    assert_eq!(trim::slide_for(&marked), trim::Slide::Selection(vec![3]), "the marked seconds, even over a green bar");

    let on_border = trim::Press { on_border: true, ..marked.clone() };
    assert_eq!(trim::slide_for(&on_border), trim::Slide::Scene(3), "unless the press is on a border, which trims");

    let one = trim::Press { selection: None, selected: vec![], inside_selection: false, ..marked.clone() };
    assert_eq!(trim::slide_for(&one), trim::Slide::Scene(3), "then the scene under the pointer");

    let row = trim::Press { scene: None, ..one.clone() };
    assert_eq!(trim::slide_for(&row), trim::Slide::Row(1), "else the whole camera row along the clock");

    // A selection with nothing in it is not a press inside one: falling through to the row would move
    // footage nobody marked.
    let empty = trim::Press { selected: vec![], scene: None, ..marked.clone() };
    assert_eq!(trim::slide_for(&empty), trim::Slide::Row(1));
}

/// F2.8 (Trim and move) S2: a scene slides along its recording — length kept, clear of neighbours, flush within
/// eight pixels.
#[test]
fn f2_8_s2_a_scene_slides_along_its_keeping_its_length_and_flushes() {
    // P.eng.minPieceSeconds' cousin SNAP_PX is the 8 px reach; at 40 px/s that is 0.2 s.
    let segs = neighbours();
    let reach = cut_select::SNAP_PX / 40.0;
    assert!((reach - 0.2).abs() < 1e-12);

    let free = trim::slide_scene(&segs, 1, 35.0, 0.0, 100.0, 40.0);
    assert_eq!(free, 20.0, "the next clip's start bounds a twenty-second scene at 20 s, not at 35");

    // Room for this fixture means room inside [9.5, 20]: the free slide has to be asked where nothing bounds
    // it, so the same question is put with no neighbours in reach.
    let alone = vec![clip(10.0, 30.0)];
    assert_eq!(trim::slide_scene(&alone, 0, 35.0, 0.0, 100.0, 40.0), 35.0, "with room the slide lands where the hand left it");

    let flush = trim::slide_scene(&segs, 1, 9.6, 0.0, 100.0, 40.0);
    assert_eq!(flush, 9.5, "0.1 s past the previous clip's end is flush with it");

    let near_miss = trim::slide_scene(&segs, 1, 9.3, 0.0, 100.0, 40.0);
    assert_eq!(near_miss, 9.5, "and further back the bound holds anyway — no overlap either way");

    let far = trim::slide_scene(&alone, 0, 62.0, 0.0, 100.0, 40.0);
    assert_eq!(far, 62.0, "a second and a half short of the recording's end is nowhere near it: {far}");

    // The length never changes: what moves is where the twenty seconds sit. Asked of a clip with no
    // neighbours, so it is the recording's end that bounds it and not the next clip's start.
    let tail = trim::slide_scene(&alone, 0, 80.0, 0.0, 100.0, 40.0);
    assert_eq!(tail + 20.0, 100.0, "the recording's end bounds the end, not just the start");

    // The far bound snaps too: slid to within reach of the next clip's implied start, it meets it exactly.
    let before_next = trim::slide_scene(&segs, 1, 19.9, 0.0, 100.0, 40.0);
    assert_eq!(before_next, 20.0, "flush against the next clip as against the previous");
}

/// F2.8 (Trim and move) S2: a gesture opens at three pixels of sideways travel, and a row change needs none.
#[test]
fn f2_8_s2_a_gesture_opens_at_3_px_and_a_row_change_needs_none() {
    assert_eq!(trim::ROW_TRAVEL_PX, 3.0);
    assert!(!trim::opens_gesture(2.9, false), "a hair of sideways drift is still a press");
    assert!(trim::opens_gesture(3.0, false), "three pixels is the ask");
    assert!(trim::opens_gesture(0.0, true), "pointing at another row that has room needs no travel at all");

    assert!(trim::is_click(2.9, false), "and short of both it is a click");
    assert!(!trim::is_click(3.0, false));
    assert!(!trim::is_click(0.0, true));
}

/// F2.8 (Trim and move) S2: one Undo per gesture, and a shift that reads its opening state so a storm of drag
/// updates is still one edit.
#[test]
fn f2_8_s2_one_undo_per_gesture_and_no_compounding_shift() {
    assert!(trim::pushes_undo(true, false), "the first real move opens the step");
    assert!(!trim::pushes_undo(true, true), "every update after it is the same act of the hand");
    assert!(!trim::pushes_undo(false, false), "and a press that has not moved pushes nothing");

    let mut opening = BTreeMap::new();
    opening.insert("cam A".to_string(), 1.0);
    opening.insert("cam B".to_string(), -0.5);

    let slid = trim::slide_shift(&opening, &["cam A".to_string()], 1.23);
    assert_eq!(slid.get("cam A"), Some(&2.23), "the row's clock corrected by what the hand travelled");
    assert_eq!(slid.get("cam B"), Some(&-0.5), "every other source is untouched");

    // Ten drag updates of one gesture, each asked of the OPENING map, are one correction — not ten.
    let twice = trim::slide_shift(&opening, &["cam A".to_string()], 1.23);
    assert_eq!(slid, twice, "the same travel from the same opening gives the same map");

    let fresh = trim::slide_shift(&opening, &["recorder mic".to_string()], -0.25);
    assert_eq!(fresh.get("recorder mic"), Some(&-0.25), "a source not in the map was at 0");
}

/// F2.8 (Trim and move) S2: the folds this gesture travels through open for the drag, and no others.
#[test]
fn f2_8_s2_folds_near_the_grab_open_for_the_drag() {
    let folds = [[0.0, 5.0], [10.0, 20.0], [30.0, 40.0]];
    assert_eq!(trim::folds_to_open(&folds, 12.0, 0.0), vec![1], "the fold the press landed in");
    assert_eq!(trim::folds_to_open(&folds, 12.0, 19.0), vec![1, 2], "everything the drag travelled through");
    assert_eq!(trim::folds_to_open(&folds, 25.0, 0.0), Vec::<usize>::new(), "a fold elsewhere is untouched");
    // A slide leftwards asks the same question about the stretch it travelled: from 12 back to 4 that is the
    // fold at 10–20 and the one at 0–5, and never the one at 30–40.
    assert_eq!(trim::folds_to_open(&folds, 12.0, -8.0), vec![0, 1], "the stretch travelled, whichever way");
}

/// F2.8 (Trim and move) S2: the status says which way the clock moved — signed, to two decimals — or says
/// nothing changed.
#[test]
fn f2_8_s2_the_status_says_which_way_the_clock_moved() {
    assert_eq!(trim::shift_label("the recording", 1.234), "the recording moved +1.23 s");
    assert_eq!(trim::shift_label("the recording", -1.234), "the recording moved -1.23 s");
    assert_eq!(trim::shift_label("the recording", 0.0), "the recording is back where it started");

    assert_eq!(
        trim::row_status("the recording", 2),
        "the recording moved to row 3 \u{2014} its kept scenes came along",
        "rows are numbered from 0 in the map and counted from 1 in the sentence"
    );
}

/// F2.8 (Trim and move) S3: an unmoved press is a click, and a right click never moves the red line — the left
/// button's line is F2.4's, so the right hand owns moves only.
#[test]
fn f2_8_s3_an_unmoved_press_is_a_click_and_a_right_click_never_moves_the_line() {
    assert!(trim::is_click(0.0, false), "nothing moved, nothing asked");
    assert!(!trim::right_press_moves_line(), "\"a right click never moves the line\"");
}
