// F2.6 (Select) — spec/05-cut.md F2.6.
// One test per step: what a drag was drawn on and what it therefore selects (S1), the band as an object
// with ends that snap, a middle that moves and a ✕ (S2), the marks and readout that follow it (S3), and
// what a sound selection does to the verbs (S4). Every rule is a plain function in naivepost::cut_select.

use naivepost::cut::{Cut, Fx, Lane, Seg};
use naivepost::cut_screen;
use naivepost::cut_select as sel;

/// The screenshot's separate recording: 45 s placed at 1:14–1:59 on the session clock.
const REC: &str = "2026-09-16 17-26-20";

fn footage(row: usize, start: f64, end: f64) -> sel::Selection {
    sel::Selection { start, end, scope: sel::Scope::Footage { row } }
}

fn sound(start: f64, end: f64) -> sel::Selection {
    sel::Selection { start, end, scope: sel::Scope::Sound { recording: REC.into() } }
}

/// F2.6 (Select) S1: a left-drag on a picture row selects that row's footage — the row it was drawn on,
/// not the timeline in general.
#[test]
fn f2_6_s1_a_drag_on_a_picture_row_selects_that_rows_footage() {
    let band = sel::draw(sel::Surface::PictureRow(1), None, 12.0, 20.0).expect("a picture row always selects");
    assert_eq!(band.scope, sel::Scope::Footage { row: 1 }, "that row's footage");
    assert_eq!(band.length(), 8.0, "the seconds the hand covered");

    let other = sel::draw(sel::Surface::PictureRow(2), None, 12.0, 20.0).expect("same seconds, another row");
    assert_ne!(other.scope, band.scope, "the same seconds on another row are a different selection");
    assert!(sel::scoped_to_row(&band.scope, 1) && !sel::scoped_to_row(&band.scope, 0), "row 1 is not row 0");

    let backwards = sel::draw(sel::Surface::PictureRow(1), None, 20.0, 12.0).expect("a right-to-left drag");
    assert_eq!(backwards.start, 12.0, "and it draws the same band as the other way round");
    assert_eq!(backwards.end, 20.0);
}

/// F2.6 (Select) S1: a drag on a wave strip or on a lane selects that one recording's sound — and with no
/// recording under the pointer there is nothing of that kind to select.
#[test]
fn f2_6_s1_a_drag_on_a_wave_strip_or_a_lane_selects_one_recordings_sound() {
    let strip = sel::draw(sel::Surface::WaveStrip, Some(REC), 74.0, 119.0).expect("the strip is a recording's");
    assert_eq!(strip.scope, sel::Scope::Sound { recording: REC.into() }, "the sound of the row it was drawn under");
    let lane = sel::draw(sel::Surface::Lane, Some(REC), 74.0, 119.0).expect("a lane is one too");
    assert_eq!(lane.scope, strip.scope, "a lane and the strip under a row answer the same way");

    assert_eq!(
        sel::draw(sel::Surface::Lane, None, 74.0, 119.0),
        None,
        "no recording there means no sound selection, and never a footage one by accident"
    );
    assert!(sel::scoped_to_row(&strip.scope, 0) == false, "a sound selection is nobody's footage");
}

/// F2.6 (Select) S1: the ruler and the empty part of the selection band are draggable ground that belongs
/// to no row, so what they select is the whole timeline's footage — the only thing the verbs can act on.
#[test]
fn f2_6_s1_the_ruler_and_the_empty_band_select_the_whole_timelines_footage() {
    let ruler = sel::draw(sel::Surface::Ruler, None, 5.0, 9.0).expect("the ruler takes a drag");
    let band = sel::draw(sel::Surface::SelectionBand, None, 5.0, 9.0).expect("so does the empty band");
    assert_eq!(ruler.scope, sel::Scope::Footage { row: sel::ANY_ROW }, "not row 0 of three cameras");
    assert_eq!(band.scope, sel::Scope::Footage { row: sel::ANY_ROW });

    assert!(sel::scoped_to_row(&ruler.scope, 0) && sel::scoped_to_row(&ruler.scope, 3), "any row's verbs may use it");
    let second = sel::draw(sel::Surface::PictureRow(2), None, 5.0, 9.0).expect("a row's own drag");
    assert!(sel::scoped_to_row(&second.scope, 2) && !sel::scoped_to_row(&second.scope, 0), "a row's scope stays on its row");
}

/// F2.6 (Select) S1: only the effects lane refuses a selection, and what it does instead depends on whether
/// an effect is in the hand.
#[test]
fn f2_6_s1_only_the_effects_lane_refuses() {
    assert_eq!(sel::draw(sel::Surface::FxLane, Some(REC), 5.0, 9.0), None, "no selection, whatever was under it");
    assert_eq!(sel::fx_lane_press(true), sel::FxLanePress::PutsHeldEffectDown, "a press on empty lane puts the held effect down");
    assert_eq!(sel::fx_lane_press(false), sel::FxLanePress::Nothing, "with nothing held, nothing happens");
}

// F2.6 S1: a drag on a picture row / a wave strip or a lane / the effects lane / the ruler -- which
// band the press's y fell in is what decides the scope, so the mapping is data plus a pure function and
// gets tested here rather than inside a gesture callback.
#[test]
fn f2_6_s1_surface_at_maps_each_y_band_to_its_surface() {
    // A two-row table so PictureRow(0) AND PictureRow(1) are both reachable: ruler 0-8, then per row
    // 20 px of picture + 12 px of wave, then the effects lane at the bottom.
    let bands = sel::SurfaceBands {
        ruler: (0.0, 8.0),
        selection: (8.0, 8.0),
        fx: (52.0, 60.0),
        first_picture: 8.0,
        row_height: 20.0,
        wave_height: 12.0,
    };
    assert_eq!(sel::surface_at(4.0, &bands, 2), sel::Surface::Ruler, "the ruler band");
    assert_eq!(
        sel::surface_at(10.0, &bands, 2),
        sel::Surface::PictureRow(0),
        "the first 20 px under `first_picture` is row 0's picture"
    );
    assert_eq!(
        sel::surface_at(28.0, &bands, 2),
        sel::Surface::WaveStrip,
        "the 12 px after it is that row's wave strip"
    );
    assert_eq!(
        sel::surface_at(42.0, &bands, 2),
        sel::Surface::PictureRow(1),
        "the next pitch down is row 1's picture"
    );
    assert_eq!(
        sel::surface_at(56.0, &bands, 2),
        sel::Surface::FxLane,
        "the bottom band is the effects lane"
    );
    // Past every drawn row -- but still above the effects lane -- the ground belongs to a separate
    // recording, not to a camera row. Rows are 16 px (12 picture + 4 wave) from y=8: rows 0 and 1 end
    // at 40, so y=40 is already past `row_count` = 2 while fx still sits above it.
    let bands_above_fx = sel::SurfaceBands {
        ruler: (0.0, 8.0),
        selection: (8.0, 8.0),
        fx: (60.0, 68.0),
        first_picture: 8.0,
        row_height: 12.0,
        wave_height: 4.0,
    };
    assert_eq!(
        sel::surface_at(39.0, &bands_above_fx, 2),
        sel::Surface::WaveStrip,
        "y=39 is still inside row 1's wave"
    );
    assert_eq!(
        sel::surface_at(40.0, &bands_above_fx, 2),
        sel::Surface::Lane,
        "past `row_count` rows comes a lane"
    );
    assert_eq!(
        sel::surface_at(40.0, &bands_above_fx, 3),
        sel::Surface::PictureRow(2),
        "the same y IS row 2's picture when three rows are drawn"
    );
    // And with no rows drawn at all, anything below `first_picture` is already past them.
    assert_eq!(
        sel::surface_at(9.0, &bands, 0),
        sel::Surface::Lane,
        "zero rows -> nothing but lanes below the ruler"
    );

    // The scope each surface produces, through the rule that already exists (`draw`).
    let row1 = sel::draw(sel::Surface::PictureRow(1), None, 0.0, 5.0).expect("a row selects footage");
    assert_eq!(row1.scope, sel::Scope::Footage { row: 1 }, "row 1's own footage");
    assert!(sel::scoped_to_row(&row1.scope, 1), "and it IS row 1");
    assert!(!sel::scoped_to_row(&row1.scope, 0), "not row 0's");

    for surface in [sel::Surface::WaveStrip, sel::Surface::Lane] {
        let sound = sel::draw(surface, Some("mic"), 0.0, 5.0).expect("a sound scope needs a name");
        assert_eq!(
            sound.scope,
            sel::Scope::Sound { recording: "mic".to_string() },
            "{surface:?} is one recording's sound, named by its base"
        );
        assert!(
            !sel::scoped_to_row(&sound.scope, 1),
            "a sound selection belongs to no camera row"
        );
    }
    // Ruler and empty band are ground that belongs to no row: the whole timeline's footage.
    for surface in [sel::Surface::Ruler, sel::Surface::SelectionBand] {
        let whole = sel::draw(surface, None, 0.0, 5.0).expect("timeline-wide footage");
        assert_eq!(
            whole.scope,
            sel::Scope::Footage { row: sel::ANY_ROW },
            "{surface:?} selects the whole timeline's footage"
        );
    }
}

/// F2.6 S1: only the effects lane refuses, and what it does instead is put the held effect DOWN. No other
/// ground answers a put-down -- `fx_lane_press` is asked ONLY for `Surface::FxLane`.
#[test]
fn f2_6_s1_only_the_effects_lane_refuses_a_selection_and_puts_a_held_effect_down() {
    assert_eq!(
        sel::fx_lane_press(true),
        sel::FxLanePress::PutsHeldEffectDown,
        "an effect in hand goes down on the lane"
    );
    assert_eq!(
        sel::fx_lane_press(false),
        sel::FxLanePress::Nothing,
        "with nothing held the lane still takes no selection"
    );
    // Nothing else puts an effect down: every non-fx surface still draws a band.
    for surface in [
        sel::Surface::Ruler,
        sel::Surface::SelectionBand,
        sel::Surface::PictureRow(0),
        sel::Surface::WaveStrip,
        sel::Surface::Lane,
    ] {
        let band = sel::draw(surface, Some("mic"), 1.0, 4.0);
        assert!(
            band.is_some(),
            "{surface:?} draws a band -- only FxLane refuses, so only FxLane has a put-down to answer"
        );
    }
    // And the lane itself never yields a band, whatever it does with the hand.
    assert!(
        sel::draw(sel::Surface::FxLane, Some("mic"), 1.0, 4.0).is_none(),
        "the effects lane draws no selection either way"
    );
}

/// F2.6 S1: a drag keeps the ground it STARTED on. Spec: a selection is scoped to what it was DRAWN ON,
/// so a drag begun on a wave strip stays that recording's sound even when the pointer crosses into a
/// picture row mid-drag -- re-scoping under the hand would make the readout lie about what was drawn.
#[test]
fn f2_6_s1_a_drag_keeps_the_surface_it_started_on() {
    assert_eq!(
        sel::keep_started_surface(
            Some(sel::Surface::WaveStrip),
            sel::Surface::PictureRow(0)
        ),
        sel::Surface::WaveStrip,
        "started on the wave strip -> crossing a picture row changes nothing"
    );
    assert_eq!(
        sel::keep_started_surface(
            Some(sel::Surface::PictureRow(1)),
            sel::Surface::Ruler
        ),
        sel::Surface::PictureRow(1),
        "same the other way: a row drag stays on its row"
    );
    assert_eq!(
        sel::keep_started_surface(None, sel::Surface::Lane),
        sel::Surface::Lane,
        "with nothing remembered yet the current surface is used"
    );
    // The kept surface keeps its scope too, which is the point of remembering it.
    let kept = sel::keep_started_surface(
        Some(sel::Surface::WaveStrip),
        sel::Surface::PictureRow(0),
    );
    let band = sel::draw(kept, Some("mic"), 0.0, 3.0).expect("the kept surface still draws");
    assert_eq!(
        band.scope,
        sel::Scope::Sound { recording: "mic".to_string() },
        "the band is still the sound it started as, not footage"
    );
}

/// F2.6 S1: a y outside every band answers a surface instead of panicking. The empty selection band is
/// legal drag ground per the spec, so falling back there keeps the press meaningful.
#[test]
fn f2_6_s1_an_unknown_y_still_answers_a_surface_instead_of_panicking() {
    let bands = sel::PLACEHOLDER_BANDS;
    assert_eq!(
        sel::surface_at(-500.0, &bands, 1),
        sel::Surface::SelectionBand,
        "far above everything -> the band, not a panic"
    );
    assert_eq!(
        sel::surface_at(100_000.0, &bands, 1),
        sel::Surface::SelectionBand,
        "far below everything: past the last row and past fx, so the band"
    );
    assert_eq!(
        sel::surface_at(f64::NAN, &bands, 1),
        sel::Surface::SelectionBand,
        "a NaN y is not a crash either"
    );
    // Degenerate geometry (no height per row) must not divide into nonsense either.
    let flat = sel::SurfaceBands {
        ruler: (0.0, 0.0),
        selection: (0.0, 0.0),
        fx: (0.0, 0.0),
        first_picture: 0.0,
        row_height: 0.0,
        wave_height: 0.0,
    };
    assert_eq!(
        sel::surface_at(5.0, &flat, 3),
        sel::Surface::SelectionBand,
        "zero-height rows fall back to the band rather than an index"
    );
}

/// F2.6 S1: below the drag slop a press is a click — it clears the selection and moves the line
/// (F2.4 S1) instead of drawing a band, so the slop is asked before anything is drawn.
#[test]
fn f2_6_s1_under_the_drag_slop_a_press_is_a_click() {
    assert_eq!(sel::DRAG_SLOP_PX, 4.0, "spec/10-parameters.md's \"drag slop 4\"");
    assert!(!sel::is_drag(4.0), "exactly the slop is still a click");
    assert!(sel::is_drag(4.5), "past it the hand meant a drag");
}

/// F2.6 (Select) S2: a press on the band knows which part it landed on — that decides whether the drag
/// resizes an end, moves the middle, or starts something new outside.
#[test]
fn f2_6_s2_a_press_on_the_band_knows_which_part_it_lands_on() {
    assert_eq!(sel::GRIP_PX, 6.0, "spec/10-parameters.md's shortest grab reach");
    // A band from 100 px to 200 px at the zoom in hand.
    assert_eq!(sel::part_at(94.0, 100.0, 200.0), sel::Part::Start, "the grip's outer edge");
    assert_eq!(sel::part_at(106.0, 100.0, 200.0), sel::Part::Start, "its inner edge");
    assert_eq!(sel::part_at(194.0, 100.0, 200.0), sel::Part::End, "the far grip's inner edge");
    assert_eq!(sel::part_at(206.0, 100.0, 200.0), sel::Part::End, "and its outer");
    assert_eq!(sel::part_at(150.0, 100.0, 200.0), sel::Part::Middle, "the middle moves the band");
    assert_eq!(sel::part_at(93.0, 100.0, 200.0), sel::Part::Outside, "clear left starts a new selection");
    assert_eq!(sel::part_at(207.0, 100.0, 200.0), sel::Part::Outside, "and clear right does too");

    // A band narrower than two grips: the press can only mean one thing, and it is not "move".
    assert_eq!(sel::part_at(103.0, 100.0, 104.0), sel::Part::Start, "an end wins over the middle");
}

/// F2.6 (Select) S2: the landmarks are every border the hand is aiming at — clip borders, recording ends,
/// effect ends and the red line — once each, in order.
#[test]
fn f2_6_s2_the_marks_are_every_border_the_hand_is_aiming_at() {
    let cut = Cut {
        segs: vec![Seg { s: 0.0, e: 30.0, ..Default::default() }, Seg { s: 40.0, e: 70.0, ..Default::default() }],
        fx: vec![Fx { kind: "text".into(), t: 20.0, dur: 10.0, ..Default::default() }],
        ..Default::default()
    };
    let lanes = [Lane { name: REC.into(), src: "project:audio/rec.wav".into(), at: 74.0, off: 0.0, dur: 45.0 }];
    let marks = sel::snap_marks(&cut, &lanes, 65.0);

    for wanted in [0.0, 30.0, 40.0, 70.0, 74.0, 119.0, 20.0, 65.0] {
        assert!(marks.contains(&wanted), "{wanted} is a border the hand aims at: {marks:?}");
    }
    let mut sorted = marks.clone();
    sorted.sort_by(f64::total_cmp);
    assert_eq!(marks, sorted, "sorted, so a caller can walk them once");
    assert_eq!(marks.iter().filter(|m| **m == 30.0).count(), 1, "a clip's end and an effect's end are one landmark");

    // A separate recording writes no length: it runs for its whole file, so it has no end to snap to.
    let open = [Lane { name: REC.into(), src: "project:audio/rec.wav".into(), at: 74.0, off: 0.0, dur: 0.0 }];
    let marks = sel::snap_marks(&Cut::default(), &open, 74.0);
    assert_eq!(marks, [74.0], "its start is the one landmark — a file with no written length has no end");
}

/// F2.6 (Select) S2: an end snaps within 8 px of a landmark and the band never closes under it.
#[test]
fn f2_6_s2_an_end_snaps_within_eight_px_and_never_closes_the_band() {
    // P.eng.minPieceSeconds is the floor the clamp holds the band to; 40 px/s makes the 8 px reach 0.2 s.
    assert_eq!(sel::MIN_SECONDS, 0.04);
    let marks = [30.0];
    assert_eq!(sel::snap(29.9, &marks, 40.0), 30.0, "a hair off the border is pulled onto it");
    assert_eq!(sel::snap(29.6, &marks, 40.0), 29.6, "outside the reach the hand keeps its second");
    assert_eq!(sel::SNAP_PX, 8.0, "S2's own number");

    let band = footage(0, 10.0, 29.5);
    let out = sel::resize(&band, true, 30.02, &marks, 40.0);
    assert_eq!(out.end, 30.0, "the end lands on the clip border");
    assert_eq!(out.start, 10.0, "and the other end stayed where it was");

    let crushed = sel::resize(&band, true, 0.0, &marks, 40.0);
    assert!(
        (crushed.length() - sel::MIN_SECONDS).abs() < 1e-12,
        "a hand that overshoots leaves the shortest band there is: {crushed:?}"
    );
    assert!(crushed.start <= crushed.end, "and an end never crosses the other");
    assert_eq!(crushed.start, 10.0, "the stationary end is the one that holds");

    let inverted = sel::resize(&band, false, 40.0, &marks, 40.0);
    assert!((inverted.length() - sel::MIN_SECONDS).abs() < 1e-12, "dragging the start past the end stops short: {inverted:?}");
    assert_eq!(inverted.end, 29.5);
}

/// F2.6 (Select) S2: the middle slides the band whole and the closer of its two ends wins the snap.
#[test]
fn f2_6_s2_the_middle_moves_the_band_whole_and_the_closer_end_snaps() {
    // Landmarks at 40 and 46: slid to 40.1, the start is 0.1 s off one and the end (44.1) 1.9 s off the
    // other, so the start wins and the band lands on the border.
    let marks = [40.0, 46.0];
    let band = footage(0, 10.0, 14.0);
    let slid = sel::move_band(&band, 40.1, &marks, 40.0);
    assert_eq!(slid.start, 40.0, "the nearer end snapped");
    assert_eq!(slid.length(), 4.0, "a slide never changes the length");

    // Now the far end is nearer a landmark. At 40 px/s the snap reach is 0.2 s, so both ends have to come
    // within that: slid to 39.95 the start is 0.05 s off 40 while the end (43.95) is 2.05 s off 46, and the
    // nearer end wins — the band lands on the border rather than a hair off it.
    let nearer = sel::move_band(&band, 39.95, &marks, 40.0);
    assert_eq!(nearer.start, 40.0, "the leading end lined up");

    // And with the trailing end nearer — slid to 41.85, its end is 45.85, 0.15 s off 46, while the start is
    // 1.85 s off 40 — the trailing one wins and the band lands behind the second the pointer was on.
    let towards = sel::move_band(&band, 41.85, &marks, 40.0);
    assert_eq!(towards.end, 46.0, "the trailing end lined up instead");
    assert_eq!(towards.start, 42.0, "so the start is not where the pointer was");

    let nowhere = sel::move_band(&band, 100.0, &marks, 40.0);
    assert_eq!(nowhere.start, 100.0, "with nothing near, exactly where the hand left it");

    let front = sel::move_band(&band, -3.0, &marks, 40.0);
    assert_eq!(front.start, 0.0, "and never off the front of the session");
    assert_eq!(front.length(), 4.0);
}

/// F2.6 (Select) S2: the ✕ clears the band, and the marks and the readout go with it — they are the band.
#[test]
fn f2_6_s2_the_cross_clears_the_band_and_the_marks_with_it() {
    let band = footage(0, 12.0, 20.0);
    assert_eq!(sel::marks(Some(&band)), Some((12.0, 20.0)), "while it is there the marks are it");
    assert_eq!(sel::clear(Some(band)), None, "the ✕ answers nothing");
    assert_eq!(sel::marks(None), None, "and the readout has nothing to read");
}

/// F2.6 (Select) S3: the in/out marks and the Selection readout follow the band live — one source, so they
/// cannot disagree with the handle being dragged.
#[test]
fn f2_6_s3_the_readout_follows_the_band_live() {
    let band = footage(0, 12.0, 20.0);
    assert_eq!(sel::marks(Some(&band)), Some((12.0, 20.0)));

    let rows = cut_screen::idle_readouts(64, "", 0.0, sel::marks(Some(&band)), &Cut::default(), 0.0);
    let value = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("{} is one of the idle rows", label))
            .value
            .clone()
    };
    assert_eq!(value("Selection"), "00:12 \u{2013} 00:20", "the reading follows the band");

    let moved = sel::move_band(&band, 40.0, &[40.0], 40.0);
    let rows = cut_screen::idle_readouts(64, "", 0.0, sel::marks(Some(&moved)), &Cut::default(), 0.0);
    assert_eq!(value("Selection"), "00:12 \u{2013} 00:20", "the rows read above are the ones asked about");
    let moved_value = rows.iter().find(|row| row.label == "Selection").expect("the row is there").value.clone();
    assert_eq!(moved_value, "00:40 \u{2013} 00:48", "a slide moves the reading with it");

    let resized = sel::resize(&band, false, 8.0, &[], 40.0);
    let rows = cut_screen::idle_readouts(64, "", 0.0, sel::marks(Some(&resized)), &Cut::default(), 0.0);
    let resized_value = rows.iter().find(|row| row.label == "Selection").expect("the row is there").value.clone();
    assert_eq!(resized_value, "00:08 \u{2013} 00:20", "and so does a resize");
}

/// F2.6 (Select) S4: a sound selection greys ＋ Add, | Split and － Remove — all three act on footage.
#[test]
fn f2_6_s4_a_sound_selection_greys_add_split_remove() {
    // P.policy.minSceneSeconds is the 1 s floor for keeping and copying; ten seconds clears it.
    let audio = sel::verbs(Some(&sound(74.0, 84.0)), true);
    assert!(!audio.add && !audio.split && !audio.remove, "a wave is seconds of sound, not footage: {audio:?}");

    let video = sel::verbs(Some(&footage(0, 74.0, 84.0)), true);
    assert!(video.add && video.split && video.remove, "the same seconds of footage keep all three live");

    // §0's rule 13: Remove takes exactly the selection; only keeping and copying ask for a scene's worth.
    let short = sel::verbs(Some(&footage(0, 74.0, 74.5)), true);
    assert!(!short.add, "under 1 s there is no scene to keep");
    assert!(short.split && short.remove, "but the seconds can still be cut and dropped");
}

/// F2.6 (Select) S4: a sound selection re-aims ⧉ Copy and ⧉ Insert at the sound instead of greying them,
/// and ＋ Add says why it is the wrong button.
#[test]
fn f2_6_s4_a_sound_selection_aims_copy_and_insert_at_the_sound() {
    let audio = sel::verbs(Some(&sound(74.0, 84.0)), true);
    assert!(audio.copy, "⧉ Copy takes the sound of those seconds");
    assert_eq!(audio.insert, sel::InsertAim::Sound, "and ⧉ Insert lays a sound over them");

    let video = sel::verbs(Some(&footage(0, 74.0, 84.0)), true);
    assert_eq!(video.insert, sel::InsertAim::Footage, "on footage it puts a card in the cut");

    let short = sel::verbs(Some(&sound(74.0, 74.5)), true);
    assert!(!short.copy, "and the same floor as the footage applies: too short to take");

    assert_eq!(
        sel::add_tip(Some(&sound(74.0, 84.0))).as_deref(),
        Some("\u{ff0b} Add keeps footage, and this selection is 2026-09-16 17-26-20's sound"),
        "spec/inventory/cut.md §A's wording, naming what was drawn on"
    );
    assert_eq!(sel::add_tip(Some(&footage(0, 74.0, 84.0))), None, "nothing to say about footage");
    assert_eq!(sel::add_tip(None), None, "or about an empty page");
}

/// F2.6 (Select) S4: with nothing selected only | Split survives, and only when there is footage at all —
/// it then cuts once, at the red line.
#[test]
fn f2_6_s4_with_nothing_selected_only_split_survives() {
    let idle = sel::verbs(None, true);
    assert!(!idle.add && !idle.remove && !idle.copy, "nothing is there to keep, drop or take");
    assert!(idle.split, "\"With nothing selected it cuts once, at the red line\"");
    assert_eq!(idle.insert, sel::InsertAim::Footage, "and a card still goes between the footage");

    assert!(!sel::verbs(None, false).split, "with no recording there is nothing to cut");
}

/// F2.6 (Select) S4: the two floors are the ones §10 names, spelled from this module's constants.
#[test]
fn f2_6_s4_the_two_floors_are_the_ones_the_catalogue_names() {
    // P.policy.minSceneSeconds — shortest stretch worth suggesting, keeping or copying.
    assert_eq!(sel::MIN_SCENE_SECONDS, 1.0);
    let scene = naivepost::params::find("P.policy.minSceneSeconds").expect("the floor is catalogued");
    assert_eq!(scene.spelled, "1", "§10 spells it as a whole number");
    assert_eq!(scene.from, "cut_select::MIN_SCENE_SECONDS");

    // P.eng.minPieceSeconds — the shortest band a resize may leave.
    let piece = naivepost::params::find("P.eng.minPieceSeconds").expect("the other floor is too");
    assert_eq!(piece.spelled, "0.04");
    assert_eq!(piece.from, "cut_select::MIN_SECONDS");
}
