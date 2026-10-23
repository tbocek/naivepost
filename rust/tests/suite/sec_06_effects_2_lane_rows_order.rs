//! §06-effects#2-the-lane-and-the-preview — the lane's ROWS as painted, not as counted.
//!
//! §2's Rows sentence says three things about where the bars go: the lane "sits between the selection band
//! above and the picture rows below", its rows are "stacked top to bottom inside the lane", and an effect goes
//! on "the topmost row where it fits". `cut_six_lane_preview.rs` already pins the packing itself (first-fit by
//! seconds, the 0.4 s floor, one row deep when empty) from `fx_lane`'s numbers alone. What nothing pinned was
//! that those row NUMBERS become ink in that order: row 0 must be the row nearest the top of the band, and the
//! whole lane must land under the selection band and above the first picture row.
//!
//! So both checks here paint a real `Scene` onto a cairo `ImageSurface` through the same door the Cut page
//! uses (`tracks::paint_at`) and read bytes back (§00-principles §5: measure the artifact, do not trust the
//! arithmetic). Nothing is written to disk and nothing waits on time.

use naivepost::{
    cut::{EffectKind, Fx, Seg},
    fx_lane,
    ui::cut_band_rows,
    ui::cut_tracks::{self as tracks, Scene},
};

const PPS: f64 = 20.0;
const WIDTH: i32 = 300;

/// The file's key for a kind — the record stores a string and §06#1 is what types it.
fn key(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Zoom => "zoom",
        EffectKind::Speed => "speed",
        EffectKind::Text => "text",
        EffectKind::Svg => "svg",
        EffectKind::Volume => "volume",
        EffectKind::Label => "label",
    }
}

/// An effect long enough to clear the packing floor and wide enough at this zoom to find pixels in.
fn fx(kind: EffectKind, from: f64, dur: f64) -> Fx {
    Fx {
        kind: key(kind).to_string(),
        t: from,
        dur,
        ..Default::default()
    }
}

fn scene(fx: Vec<Fx>) -> Scene {
    Scene {
        pps: PPS,
        origin: 0.0,
        segs: vec![Seg {
            s: 0.0,
            e: 20.0,
            ..Default::default()
        }],
        fx,
        recordings: vec![naivepost::timeline::Recording {
            base: "take-a".into(),
            start: 0.0,
            end: 50.0,
        }],
        folds: Vec::new(),
        selection: None,
        watched_row: None,
        row_count: 1,
        shift: Default::default(),
        lanes: Vec::new(),
        lane_plates: Vec::new(),
        thumbs: Vec::new(),
        line: 0.0,
    }
}

/// Paint `scene` and hand back its bytes, stride and height — the route `sec_05_cut_1_band_paints.rs` uses.
fn painted(scene: &Scene) -> (Vec<u8>, usize, i32) {
    let height = tracks::band_height_px(scene, tracks::THUMB_DEFAULT_PX).ceil() as i32;
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, WIDTH, height).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        tracks::paint_at(
            &cr,
            scene,
            WIDTH as f64,
            height as f64,
            tracks::THUMB_DEFAULT_PX,
        );
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

/// The lane's own top, spelled through the door the painter uses.
fn lane_top() -> f64 {
    cut_band_rows::lane_top_px(naivepost::cut_trim::RULER_H, tracks::SELECTION_BAND_PX)
}

/// The colour a bar actually leaves on the surface. `fill` paints it at alpha 0.6 with NOTHING under it but
/// the band's own ground (`GROUND`, painted over the whole surface first), so cairo's `OVER` gives
/// `alpha * bar + (1 - alpha) * ground` — the pure triple never appears in the bytes and must be blended the same
/// way here. The alpha is the lane's own wash (`fx_lane::bar_wash_alpha`), the number §A.2 spells for a speed bar
/// and the one the shared bar painter lays every kind on, so this cannot drift from what is drawn.
fn bar_ink(colour: (f64, f64, f64), alpha: f64) -> (u8, u8, u8) {
    let ground = naivepost::ui::cut_colours::GROUND;
    let mix = |c: f64| ((c * alpha + ground.0 * (1.0 - alpha)) * 255.0).round() as u8;
    (mix(colour.0), mix(colour.1), mix(colour.2))
}

/// Is any pixel of the bar's own ink present anywhere in rows `[from, to)`? Returns the first such y, if any.
/// `first` asks for the topmost such row; `last` for the bottommost, which is what compares two stacked rows.
fn row_with(
    painted: &(Vec<u8>, usize, i32),
    colour: (f64, f64, f64),
    kind: EffectKind,
    from: i32,
    to: i32,
    first: bool,
) -> Option<i32> {
    let (data, stride, _) = painted;
    // The wash alpha the painter lays this kind on, asked of the same door `cut_lane_paint::paint_bar` uses.
    let want = bar_ink(colour, fx_lane::bar_wash_alpha(kind));
    let range = if first {
        Box::new(from.max(0)..to) as Box<dyn Iterator<Item = i32>>
    } else {
        Box::new((from.max(0)..to).rev())
    };
    for y in range {
        for x in (10..WIDTH - 10).step_by(3) {
            let got = rgb(data, *stride, x, y);
            // cairo's blend rounds each channel, so allow a couple of levels rather than an exact match.
            let near = (got.0 as i32 - want.0 as i32).abs() <= 3
                && (got.1 as i32 - want.1 as i32).abs() <= 3
                && (got.2 as i32 - want.2 as i32).abs() <= 3;
            if near {
                return Some(y);
            }
        }
    }
    None
}

fn first_row_with(
    painted: &(Vec<u8>, usize, i32),
    colour: (f64, f64, f64),
    kind: EffectKind,
    from: i32,
    to: i32,
) -> Option<i32> {
    row_with(painted, colour, kind, from, to, true)
}

fn last_row_with(
    painted: &(Vec<u8>, usize, i32),
    colour: (f64, f64, f64),
    kind: EffectKind,
    from: i32,
    to: i32,
) -> Option<i32> {
    row_with(painted, colour, kind, from, to, false)
}

/// §2: the lane sits BETWEEN the selection band above and the picture rows below — in the arithmetic the
/// painter uses and in the ink it leaves.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s3_the_lane_sits_between_the_selection_band_and_the_pictures(
) {
    let single = scene(vec![fx(EffectKind::Zoom, 0.0, 5.0)]);
    let top = lane_top();
    let row_h = fx_lane::lane_height_px(1);
    println!("lane top {top}, one row {row_h}");
    assert_eq!(
        top,
        naivepost::cut_trim::RULER_H + tracks::SELECTION_BAND_PX,
        "the lane begins right under the selection band (§2)"
    );
    let pictures = cut_band_rows::pictures_top_px(top, fx_lane::row_count(&single.fx) as f64);
    assert!(
        pictures >= top + row_h,
        "the picture rows begin below the lane's last row: pictures {pictures}, lane bottom {}",
        top + row_h
    );

    let painted_band = painted(&single);
    let zoom = fx_lane::bar_colour(EffectKind::Zoom, false);
    let lane_bottom = (top + row_h) as i32;
    let in_lane = first_row_with(
        &painted_band,
        zoom,
        EffectKind::Zoom,
        top as i32,
        lane_bottom,
    );
    assert!(
        in_lane.is_some(),
        "the zoom bar is painted inside the lane's own rows [{top}, {lane_bottom})"
    );
    // Above the lane is the selection band; below it, the pictures. Neither may carry the bar's ink.
    let above = first_row_with(
        &painted_band,
        zoom,
        EffectKind::Zoom,
        naivepost::cut_trim::RULER_H as i32,
        top as i32,
    );
    assert_eq!(
        above, None,
        "no bar ink in the selection band above the lane (found at {above:?})"
    );
    let below = first_row_with(
        &painted_band,
        zoom,
        EffectKind::Zoom,
        lane_bottom,
        painted_band.2,
    );
    assert_eq!(
        below, None,
        "no bar ink in the picture rows below the lane (found at {below:?})"
    );
}

/// §2: a bar that sits EXACTLY on the span floor is packed like any other effect, so it gets inked on its own
/// row instead of being flattened onto row 0 over whatever shares it. The bug this pins was arithmetic, not
/// intent: the packing measured the floor on `(t + dur) - t`, which for `t = 1.0, dur = 0.4` answers
/// `0.3999999999999999` — under 0.4 — while the painted bar said it reserved a row.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s3_a_bar_at_the_floor_is_inked_on_its_own_row() {
    // The reproduction's own numbers: `dur` EXACTLY the floor, and `t` chosen so recomputing the span length
    // as `(t + dur) - t` lands below it (0.3999999999999999). At this scene's zoom those 0.4 s are only 8 px
    // wide, too narrow to sample as ink — so what is measured here is which ROW each kind's ink lands on.
    let pair = scene(vec![
        fx(EffectKind::Zoom, 1.0, fx_lane::PACK_MIN_SECONDS),
        fx(EffectKind::Text, 1.0, 5.0),
    ]);
    assert!(
        (pair.fx[0].t + pair.fx[0].dur) - pair.fx[0].t < fx_lane::PACK_MIN_SECONDS,
        "the floor test on a recomputed span would call this sub-floor: {:?}",
        (pair.fx[0].t + pair.fx[0].dur) - pair.fx[0].t
    );
    let top = lane_top();
    let row_h = fx_lane::lane_height_px(1);
    assert_eq!(
        fx_lane::row_count(&pair.fx),
        2,
        "the floor bar reserves a row, so the lane is two rows deep"
    );

    let painted_band = painted(&pair);
    let text = fx_lane::bar_colour(EffectKind::Text, false);
    let row_one = cut_band_rows::lane_row_top_px(top, 1) as i32;
    // Before the fix the floor bar reserved nothing, so the caption overlapping its seconds packed onto row 0
    // beside it and the lane's second row carried no ink at all.
    let text_y = first_row_with(
        &painted_band,
        text,
        EffectKind::Text,
        top as i32,
        (top + 2.0 * row_h) as i32,
    )
    .expect("the overlapping caption is painted somewhere in the lane");
    println!("floor bar holds row 0; caption ink first seen at y={text_y}, row 1 begins {row_one}");
    assert!(
        text_y >= row_one,
        "§2 gives an effect the topmost row where it FITS, and the 0.4 s zoom fits row 0 because it claims \
         it: the caption over those seconds belongs to the row below (row 1 begins {row_one}), found its \
         ink at y={text_y}"
    );
}

/// §2: THREE effects all over one another, one of them exactly on the span floor. `s10` pins the same floor in
/// the packing alone and the check above paints a pair; what nothing pinned was that the STACK reaches past a
/// pair — with three rows the lane must still be laid out row 0 nearest the top, row 1 under it and row 2 under
/// that, and the floor bar must take part in that stack instead of being flattened onto row 0.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s3_three_rows_stack_past_the_floor_bar() {
    // The zoom is EXACTLY the floor at `t = 1.0`, where recomputing its span as `(t + dur) - t` answers
    // 0.3999999999999999 — under it. That is the bug's own numbers, kept as a guard here too. The three spans
    // are written so they really do overlap in pairs (row 0 / row 1 / row 2), which first-fit needs to reach a
    // third row at all.
    let triple = scene(vec![
        fx(EffectKind::Zoom, 1.0, fx_lane::PACK_MIN_SECONDS),
        fx(EffectKind::Text, 1.0, 5.0),
        fx(EffectKind::Svg, 1.2, 4.0),
    ]);
    assert!(
        (triple.fx[0].t + triple.fx[0].dur) - triple.fx[0].t < fx_lane::PACK_MIN_SECONDS,
        "the floor test on a recomputed span would call the first bar sub-floor: {:?}",
        (triple.fx[0].t + triple.fx[0].dur) - triple.fx[0].t
    );
    // Pairwise overlap, spelled out rather than trusted: each pair shares seconds, so no two can share a row.
    for (a, b) in [(0, 1), (0, 2), (1, 2)] {
        let (sa, ea) = triple.fx[a].spans();
        let (sb, eb) = triple.fx[b].spans();
        assert!(
            sa < eb && sb < ea,
            "bars {a} and {b} must overlap ([{sa}, {ea}) vs [{sb}, {eb})) or the lane cannot be three deep"
        );
    }
    let top = lane_top();
    let row_h = fx_lane::lane_height_px(1);
    assert_eq!(
        fx_lane::row_count(&triple.fx),
        3,
        "three effects over one another make the lane three rows deep"
    );

    let painted_band = painted(&triple);
    // The floor bar's own 8 px are too narrow to sample as ink (see the note above), so the two wide bars carry
    // the measurement and the floor bar is read where its own painter puts it: through `lane_layout`, the very
    // door `draw_effects_lane` iterates to decide each bar's row.
    let layout = fx_lane::lane_layout(&triple.fx);
    let mut seen = Vec::new();
    for kind in [EffectKind::Text, EffectKind::Svg] {
        let y = first_row_with(
            &painted_band,
            fx_lane::bar_colour(kind, false),
            kind,
            top as i32,
            (top + 3.0 * row_h) as i32,
        )
        .unwrap_or_else(|| panic!("{} carries no ink anywhere in the lane", key(kind)));
        seen.push(y);
    }
    println!(
        "three-row stack: zoom row {} text y={} svg y={} (rows begin {}, {}, {})",
        layout[0].row,
        seen[0],
        seen[1],
        cut_band_rows::lane_row_top_px(top, 0),
        cut_band_rows::lane_row_top_px(top, 1),
        cut_band_rows::lane_row_top_px(top, 2)
    );
    // The floor bar owns the TOP row and claims its seconds — the bug gave it row 0 unclaimed, so the caption
    // below could not tell it was taken and packed beside it.
    assert_eq!(
        (layout[0].row, layout[0].reserves_a_row),
        (0, true),
        "the 0.4 s floor bar fits the topmost row and CLAIMS those seconds"
    );
    // One row each, in order: §2's "rows stacked top to bottom inside the lane", read off the pixels.
    for (index, kind) in [EffectKind::Text, EffectKind::Svg].iter().enumerate() {
        let from = cut_band_rows::lane_row_top_px(top, index + 1) as i32;
        let to = cut_band_rows::lane_row_top_px(top, index + 2) as i32;
        assert!(
            seen[index] >= from && seen[index] < to,
            "{} sits in row {} [{from}, {to}) — its ink first appears at y={}",
            key(*kind),
            index + 1,
            seen[index]
        );
    }
    assert!(
        seen[0] < seen[1],
        "row numbers grow DOWNWARD, so the second and third bars print at ascending y: {seen:?}"
    );
}

/// §2: rows stack TOP TO BOTTOM, so "the topmost row where it fits" is the SMALLEST painted y — and the
/// stacking is by seconds, so zooming cannot reorder it.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s3_the_topmost_row_is_the_smallest_y() {
    let pair = scene(vec![
        fx(EffectKind::Zoom, 0.0, 5.0),
        fx(EffectKind::Text, 2.0, 5.0),
    ]);
    let top = lane_top();
    let row_h = fx_lane::lane_height_px(1);
    assert_eq!(
        cut_band_rows::lane_row_top_px(top, 0),
        top,
        "row 0 IS the lane's top row"
    );
    assert!(
        cut_band_rows::lane_row_top_px(top, 0) < cut_band_rows::lane_row_top_px(top, 1),
        "a bigger row number is a lower row: {} vs {}",
        cut_band_rows::lane_row_top_px(top, 0),
        cut_band_rows::lane_row_top_px(top, 1)
    );
    assert!(
        (row_h - 26.0).abs() < 0.01,
        "one lane row is 26 px ({row_h})"
    );

    let painted_band = painted(&pair);
    let zoom = fx_lane::bar_colour(EffectKind::Zoom, false);
    let text = fx_lane::bar_colour(EffectKind::Text, false);
    let zoom_y = last_row_with(
        &painted_band,
        zoom,
        EffectKind::Zoom,
        top as i32,
        (top + 2.0 * row_h) as i32,
    )
    .expect("the zoom bar is painted");
    let text_y = first_row_with(
        &painted_band,
        text,
        EffectKind::Text,
        top as i32,
        (top + 2.0 * row_h) as i32,
    )
    .expect("the text bar is painted");
    println!(
        "lane top {top}: zoom row y={zoom_y}, text row y={text_y}, second row starts {}",
        cut_band_rows::lane_row_top_px(top, 1)
    );
    assert!(
        zoom_y < text_y,
        "the zoom took the TOP row and the overlapping text the one below it: zoom y={zoom_y}, text y={text_y}"
    );

    // Packed by SECONDS: the same two effects at a different zoom keep the same rows. `rows_for_effects` is
    // the packing the painter then draws, so this is the promise §2 makes ("first-fit BY SECONDS") and it does
    // not depend on what fits in the painted surface -- at 240 px/s the bars run off its right edge.
    let mut far = pair.clone();
    far.pps = 240.0;
    assert_eq!(
        fx_lane::rows_for_effects(&far.fx),
        fx_lane::rows_for_effects(&pair.fx),
        "zooming does not repack the lane (§2 packs by seconds)"
    );
    let mut small = pair.clone();
    small.pps = 2.0;
    assert_eq!(
        fx_lane::rows_for_effects(&small.fx),
        fx_lane::rows_for_effects(&pair.fx),
        "nor does zooming out: the row numbers are the same {small:?}"
    );
}
