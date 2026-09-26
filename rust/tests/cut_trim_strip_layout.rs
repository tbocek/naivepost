// F2.8 (Trim and move) — the geometry the Cut page's strip paints, so the bar is visible instead of an
// empty box. Every number here is hand-computed at 4 px per second (`cut_screen::ZOOM_AT_OPEN`, which
// `ui::TRACK_STRIP_PPS` copies), the zoom the snapshot renders at: a clip from 10 s to 30 s is 40 px to
// 120 px wide 80 px. The draw callback in `src/ui/window.rs` paints exactly these boxes and ticks; it
// holds no arithmetic of its own, so what these tests pin is what appears on screen.

use naivepost::cut::Seg;
use naivepost::cut_trim as trim;

fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// An insert is `s == e` with a `dur` (`Seg::is_insert`): material spliced in beside the footage, which
/// rule 8 of `spec/05-cut.md` keeps out of every trim reach. The box it gets on the strip stands for its
/// own length so there is something to see and click.
fn insert(seconds: f64, asset: &str) -> Seg {
    Seg { s: 0.0, e: 0.0, dur: seconds, ins: asset.to_string(), ..Default::default() }
}

const PPS: f64 = 4.0;

/// F2.8 S1: seconds to pixels for the kept-bar band, one box per segment in slice order.
#[test]
fn f2_8_s1_clip_boxes_map_seconds_to_pixels_at_the_strips_zoom() {
    let segs = vec![clip(0.0, 9.5), clip(10.0, 30.0), clip(40.0, 70.0)];
    let boxes = trim::clip_boxes(&segs, PPS);

    assert_eq!(boxes.len(), 3, "one box per segment, none dropped");
    // 0 s -> 0 px, 9.5 s -> 38 px wide.
    assert_eq!((boxes[0].x, boxes[0].w, boxes[0].index), (0.0, 38.0, 0));
    // 10 s -> 40 px, 20 s long -> 80 px.
    assert_eq!((boxes[1].x, boxes[1].w, boxes[1].index), (40.0, 80.0, 1));
    // 40 s -> 160 px, 30 s long -> 120 px.
    assert_eq!((boxes[2].x, boxes[2].w, boxes[2].index), (160.0, 120.0, 2));
    assert!(
        boxes.iter().all(|b| !b.insert),
        "footage segments are drawn as trimmable clips"
    );
}

/// F2.8 S1 + rule 8: an insert gets its box (the band has to show something is there) but flagged, so the
/// painter draws it violet and the press never hands its borders to a trim.
#[test]
fn f2_8_s1_an_insert_is_boxed_and_flagged_never_trimmable() {
    let segs = vec![clip(0.0, 9.5), insert(10.0, "project:titles/intro.png")];
    let boxes = trim::clip_boxes(&segs, PPS);

    assert!(boxes[1].insert, "and carries the flag that keeps it out of every trim reach");
    // A spliced insert sits at one instant (`s == e`), so the box on the strip stands for `dur`: a card
    // has to be seen and clicked, not drawn as a hairline. 10 s * 4 px/s = 40 px.
    // `index` is the caller's to fill: this helper draws one card, so it leaves 0 and the painter sets it
    // from its own slice position.
    let laid = trim::insert_box(&segs[1], 30.0, PPS);
    assert_eq!((laid.x, laid.w, laid.insert), (120.0, 40.0, true));
    assert_eq!(boxes[0].x, 0.0, "the footage before it is unaffected");

    // Every press on an insert takes nothing at all; the only thing in reach is the footage's own end.
    assert_eq!(trim::box_at(&boxes, 0.0), Some((0, Some(trim::Border::Start))), "footage start still grabbable");
    assert_eq!(trim::box_at(&boxes, 38.0), Some((0, Some(trim::Border::End))), "// layout.edgeGrabPx: 2 px off the footage end is inside the reach");
    let only_insert = trim::clip_boxes(&[insert(10.0, "project:titles/intro.png")], PPS);
    assert_eq!(trim::box_at(&only_insert, 20.0), None, "rule 8: an insert offers no border");
}

/// F2.8 S1: a zero-length or backwards segment draws as nothing rather than a box running into its
/// neighbour — the width is clamped, so a bad cut cannot paint a negative rectangle.
#[test]
fn f2_8_s1_a_zero_or_backwards_segment_draws_no_width() {
    let boxes = trim::clip_boxes(&[clip(20.0, 20.0), clip(30.0, 25.0)], PPS);
    assert_eq!(boxes[0].w, 0.0);
    assert_eq!(boxes[1].w, 0.0, "backwards length clamps to zero, never negative");
    assert_eq!(boxes[1].x, 120.0, "its left edge still reads from its own start");
}

/// F2.8 S1: the press side asks the same mapping in reverse — within 6 px of a border that border is taken,
/// nearest wins, and the body of a clip answers with the clip and no border.
#[test]
fn f2_8_s1_a_press_px_resolves_to_a_border_or_the_bar_body() {
    let boxes = trim::clip_boxes(&[clip(10.0, 30.0), clip(40.0, 70.0)], PPS);

    // Border of clip 1 sits at 40 px and 120 px; clip 2's at 160 px and 280 px.
    assert_eq!(trim::box_at(&boxes, 40.0), Some((0, Some(trim::Border::Start))));
    assert_eq!(trim::box_at(&boxes, 124.0), Some((0, Some(trim::Border::End))), "4 px past the end is still inside the reach");
    assert_eq!(trim::box_at(&boxes, 156.0), Some((1, Some(trim::Border::Start))));

    // In the middle of the green bar: the box, no border — the answer that means "move me", not "trim me".
    assert_eq!(trim::box_at(&boxes, 80.0), Some((0, None)));
    assert_eq!(trim::box_at(&boxes, 200.0), Some((1, None)));

    // Outside every box, and further from a border than the reach: nothing under the pointer.
    assert_eq!(trim::box_at(&boxes, 140.0), None, "20 px from both neighbours' borders is no trim and no bar");
    assert_eq!(trim::box_at(&[], 40.0), None, "an empty cut offers nothing to grab");
}

/// F2.8 S1: two borders closer together than the reach — the nearer one wins, so a narrow clip is still
/// trimmed on the side the pointer actually points at.
#[test]
fn f2_8_s1_the_nearer_border_wins_when_two_are_in_reach() {
    // A 1.5 s clip: 40 px wide at 4 px/s? No — 1.5 s * 4 = 6 px, so its borders sit 6 px apart.
    let boxes = trim::clip_boxes(&[clip(10.0, 11.5)], PPS);
    assert_eq!((boxes[0].x, boxes[0].w), (40.0, 6.0));
    assert_eq!(
        trim::box_at(&boxes, 41.0),
        Some((0, Some(trim::Border::Start))),
        "1 px off the start beats 5 px off the end"
    );
    assert_eq!(
        trim::box_at(&boxes, 45.5),
        Some((0, Some(trim::Border::End))),
        "and the other way round"
    );
}

/// F2.8 S1: the ruler row, spelled like `spec/img/05-trim.png` — whole-second marks, `0:00`, `0:30`,
/// `1:00`, no tenth, laid out at the same zoom as the band under it.
#[test]
fn f2_8_s1_ruler_ticks_are_whole_second_marks_laid_out_at_the_same_zoom() {
    let ticks = trim::ruler_ticks(90.0, PPS, 30.0);
    let labels: Vec<&str> = ticks.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(labels, vec!["00:00", "00:30", "01:00", "01:30"], "the picture's spelling, minus the playhead's tenth");
    let xs: Vec<f64> = ticks.iter().map(|(x, _)| *x).collect();
    assert_eq!(xs, vec![0.0, 120.0, 240.0, 360.0], "a tick every 30 s is a tick every 120 px at 4 px/s");
}

/// F2.8 S1: the ruler stops at the recording's end — a mark with no footage under it asks a question
/// nobody can answer — and a nonsense span, zoom or spacing answers with no ticks instead of looping.
#[test]
fn f2_8_s1_ruler_ticks_stop_at_the_end_and_survive_nonsense() {
    let ticks = trim::ruler_ticks(65.0, PPS, 30.0);
    assert_eq!(ticks.len(), 3, "0, 30, 60 — 90 is past the end and does not appear");
    assert_eq!(ticks.last().unwrap().0, 240.0);

    assert_eq!(trim::ruler_ticks(0.0, PPS, 30.0).len(), 0, "no recording, no ruler");
    assert_eq!(trim::ruler_ticks(-5.0, PPS, 30.0).len(), 0);
    assert_eq!(trim::ruler_ticks(60.0, 0.0, 30.0).len(), 0, "zero zoom would loop forever");
    assert_eq!(trim::ruler_ticks(60.0, PPS, 0.0).len(), 0, "zero spacing likewise");
}
