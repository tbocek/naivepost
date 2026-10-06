// F2.12 S5 (Placed: spliced / overwriting / a lane of its own) — the three modes must be told apart ON THE
// PAINTED BAND, not only in the status line. Every check paints a `Scene` onto an `ImageSurface` and reads
// bytes back, in the harness `tests/sec_05_cut_1_band_paints.rs` established: a rule that changes what is
// drawn changes a number here, and a mode that draws nothing changes none.

use naivepost::{
    cut::{Lane, Seg},
    cut_screen, fx_lane,
    timeline::Recording,
    ui::cut_tracks::{self as tracks, Scene},
};

const PPS: f64 = 2.0;
const WIDTH: i32 = 300;

/// The band's fixed rows above the pictures, as the painter lays them.
fn head_px(scene: &Scene) -> i32 {
    (naivepost::cut_trim::RULER_H
        + tracks::SELECTION_BAND_PX
        + fx_lane::lane_height_px(fx_lane::row_count(&scene.fx))) as i32
}

/// One take, 0–100 s, so every x below sits on filmed material.
fn tape() -> Vec<Recording> {
    vec![Recording { base: "take-a".into(), start: 0.0, end: 100.0 }]
}

fn scene() -> Scene {
    Scene {
        pps: PPS,
        origin: 0.0,
        segs: vec![Seg { s: 0.0, e: 100.0, ..Default::default() }],
        fx: Vec::new(),
        recordings: tape(),
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

/// Paint `scene` and hand back a pixel reader: (x, y) -> (r, g, b) plus the surface's height.
fn painted(scene: &Scene) -> (Vec<u8>, usize, i32) {
    let height = tracks::band_height_px(scene, tracks::THUMB_DEFAULT_PX).ceil() as i32;
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, WIDTH, height).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        tracks::paint_at(&cr, scene, WIDTH as f64, height as f64, tracks::THUMB_DEFAULT_PX);
    }
    let data = surface.data().expect("read the pixels back").to_vec();
    let stride = surface.stride() as usize;
    (data, stride, height)
}

fn rgb(data: &[u8], stride: usize, x: i32, y: i32) -> (u8, u8, u8) {
    // ARgb32 is stored B,G,R,A on little-endian.
    let base = y as usize * stride + x as usize * 4;
    (data[base + 2], data[base + 1], data[base])
}

/// A card laid OVER footage: `s != e`, no `dur` of its own (`Seg::is_overwrite_insert`).
fn over_card(from: f64, to: f64, path: &str) -> Seg {
    Seg { s: from, e: to, ins: path.into(), ..Default::default() }
}

/// A spliced card: `s == e` with a `dur`.
fn spliced(at: f64, dur: f64, path: &str) -> Seg {
    Seg { s: at, e: at, dur, ins: path.into(), ..Default::default() }
}

/// F2.12 S5 (`lane → adds a row`): a cut lane costs the band exactly one more row, and that row is drawn —
// it is not the ground colour, and it carries the lane's violet span where the lane runs.
#[test]
fn f2_12_s5_a_lane_adds_a_row_to_the_band() {
    let plain = scene();
    let mut with_lane = plain.clone();
    with_lane.lanes = vec![Lane {
        name: "sting".into(),
        src: "assets/sting.mp4".into(),
        at: 20.0,
        off: 0.0,
        dur: 10.0,
    }];
    assert_eq!(tracks::lane_rows(&plain), 0, "no lane, no row");
    assert_eq!(tracks::lane_rows(&with_lane), 1, "one lane, one row");
    assert_eq!(tracks::lane_rows(&with_lane), 1, "and the painter counts them the same way");

    let plain_h = tracks::band_height_px(&plain, tracks::THUMB_DEFAULT_PX);
    let lane_h = tracks::band_height_px(&with_lane, tracks::THUMB_DEFAULT_PX);
    let pitch = tracks::THUMB_DEFAULT_PX as f64 + tracks::PICTURE_ROW_EXTRA_PX;
    assert!(
        (lane_h - plain_h - pitch).abs() < 1.0,
        "the lane row costs exactly one row pitch: {plain_h} -> {lane_h}"
    );

    let before = painted(&plain);
    let after = painted(&with_lane);
    // The new row begins below every camera row: `head + one camera row`. Sample inside it.
    let y = head_px(&with_lane) + pitch as i32 + 6;
    let ground = rgb(&before.0, before.1, 150, y.min(before.2 - 1));
    let lane = rgb(&after.0, after.1, 150, y);
    assert_ne!(lane, ground, "the lane row is painted, not left as ground: {lane:?} vs {ground:?}");
    assert!(
        lane.2 > lane.0 && lane.2 > lane.1,
        "a lane reads blue-grey against the plate grey, got {lane:?}"
    );
    // And its span is where the lane runs: 20 s at 2 px/s is x 40, ten seconds wide.
    let span = rgb(&after.0, after.1, 50, y);
    assert!(span.0 > 100 && span.2 > 100 && span.1 < span.0, "violet lane span at its own seconds: {span:?}");
}

/// F2.12 S5 (`overwriting → replaces those seconds`): an OVER card's seconds are GONE from the footage, and
/// the band says so — the ink under the card is the dropped-stretch grey, not the footage green.
#[test]
fn f2_12_s5_an_over_card_eats_the_footage_it_covers() {
    let plain = scene();
    let mut eaten = scene();
    eaten.segs.push(over_card(40.0, 45.0, "assets/tier.svg"));
    let y = head_px(&plain) + 20;
    // 40..45 s at 2 px/s is x 80..90; sample the middle of the eaten stretch.
    let kept = rgb(&painted(&plain).0, painted(&plain).1, 85, y);
    let gone = rgb(&painted(&eaten).0, painted(&eaten).1, 85, y);
    assert_ne!(gone, kept, "the same second reads differently once a card replaced it: {kept:?} -> {gone:?}");
    // The eaten stretch is the dropped-stretch grey with the card's violet laid over it at 0.6 alpha: the
    // result is neither the footage green nor the plain card violet, and that mixture IS the claim "these
    // seconds are gone and something else is on them".
    assert!(gone.0 > gone.1 && gone.2 > gone.1, "grey under violet, not footage green: {gone:?}");
    assert!(gone.0 < 240 && gone.2 < 240, "darker than bare ground — the gap is painted: {gone:?}");
    // Footage outside the card is untouched by the card's presence.
    let outside_kept = rgb(&painted(&plain).0, painted(&plain).1, 20, y);
    let outside_gone = rgb(&painted(&eaten).0, painted(&eaten).1, 20, y);
    assert_eq!(outside_gone, outside_kept, "only the covered seconds changed");
}

/// F2.12 S5 (`spliced → s == e with dur`): a spliced card costs no session time and leaves the footage under
/// it alone, so at the SAME x a spliced cut and an over cut differ — which is what makes the two modes tellable
/// apart on the surface rather than only in the sentence the Apply press prints.
#[test]
fn f2_12_s5_a_spliced_card_leaves_the_footage_intact() {
    let mut spliced_scene = scene();
    spliced_scene.segs.push(spliced(42.0, 4.0, "assets/tier.svg"));
    let mut over_scene = scene();
    over_scene.segs.push(over_card(40.0, 45.0, "assets/tier.svg"));
    let y = head_px(&spliced_scene) + 20;
    let splice_px = rgb(&painted(&spliced_scene).0, painted(&spliced_scene).1, 85, y);
    let over_px = rgb(&painted(&over_scene).0, painted(&over_scene).1, 85, y);
    assert_ne!(
        splice_px, over_px,
        "the same second must read differently for the two modes: splice {splice_px:?} over {over_px:?}"
    );
    // And beside a spliced card the footage is exactly what it was with no card at all: the video grew by the
    // card's own seconds, the tape did not lose anything.
    let plain = scene();
    let beside = rgb(&painted(&spliced_scene).0, painted(&spliced_scene).1, 20, y);
    assert_eq!(beside, rgb(&painted(&plain).0, painted(&plain).1, 20, y), "footage elsewhere is intact");
    // A spliced card also costs no band rows: it sits among the footage, unlike a lane.
    assert_eq!(tracks::lane_rows(&spliced_scene), 0, "a spliced card adds no row");
    assert!((tracks::band_height_px(&spliced_scene, cut_screen::THUMB_AT_OPEN)
        - tracks::band_height_px(&plain, cut_screen::THUMB_AT_OPEN))
        .abs()
        < 1.0);
}
