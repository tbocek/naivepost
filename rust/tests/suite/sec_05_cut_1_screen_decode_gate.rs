//! §05-cut#1-screen / §7 — a draw never blocks on a decode: the gate that lets film pixels into a paint pass.
//!
//! "Nothing decodes inside a draw" is §5's rule for this page's tracks, so the check lives under that item's name.
//!
//! `cut_picture_rows::paint_frame_cell` fills every cell with the slate placeholder; on the live page it then paints
//! a frame only once the worker has decoded it, and asks the worker otherwise, so a pass with nothing decoded yet
//! draws placeholders and nothing else. Only a snapshot (`allow_frame_decoding`) decodes inside the draw. Nothing here needs a
//! display: cairo paints to an `ImageSurface` and the bytes are read back off the surface itself, so this is a
//! plain `#[test]` file like the other non-widget ones.

use naivepost::cut::Seg;
use naivepost::snapshot_lane;
use naivepost::ui::cut_tracks::{self, Scene};

fn near(a: (u32, u32, u32), b: (u32, u32, u32), within: u32) -> bool {
    let one = |t: (u32, u32, u32)| [t.0, t.1, t.2];
    let (x, y) = (one(a), one(b));
    (0..3).all(|i| x[i].abs_diff(y[i]) <= within)
}

/// A band scene wide enough to sample, with one recording so the picture row has a run to paint over.
fn probe_scene(width: i32) -> Scene {
    Scene {
        pps: snapshot_lane::PPS,
        origin: 0.0,
        segs: vec![Seg {
            s: 0.0,
            e: width as f64 / snapshot_lane::PPS,
            cam: 0,
            ..Default::default()
        }],
        fx: Vec::new(),
        recordings: vec![naivepost::timeline::Recording {
            base: "session-tape".into(),
            start: 0.0,
            end: 400.0,
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
    }
}

/// Frame names, spelled the only way that means anything here: `clock::frame_name`.
fn stamps(at: &[f64]) -> Vec<std::path::PathBuf> {
    at.iter()
        .map(|second| {
            std::path::PathBuf::from(format!("{}.jpg", naivepost::clock::frame_name(*second)))
        })
        .collect()
}

/// Paint `scene` and hand back a reader of its own pixels plus the y the picture row's film starts at.
fn painted(scene: &Scene) -> (Vec<u8>, usize, i32) {
    let height = cut_tracks::band_height_px(scene, cut_tracks::THUMB_DEFAULT_PX).ceil() as i32;
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, WIDTH, height).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        // No panic, however short or long the list: the batch walk terminates on every shape of it.
        cut_tracks::paint_at(
            &cr,
            scene,
            WIDTH as f64,
            height as f64,
            cut_tracks::THUMB_DEFAULT_PX,
        );
    }
    let data = surface.data().expect("read the pixels back").to_vec();
    let stride = surface.stride() as usize;
    let top = naivepost::ui::cut_band_rows::pictures_top_px(
        naivepost::ui::cut_band_rows::lane_top_px(
            naivepost::cut_trim::RULER_H,
            cut_tracks::SELECTION_BAND_PX,
        ),
        naivepost::fx_lane::row_count(&scene.fx) as f64,
    ) as i32;
    (data, stride, top)
}

const WIDTH: i32 = 600;

/// §7: a page that has NOT asked for film leaves the row exactly as an empty row looks — placeholder ink, no frame
/// decoded. This test never calls `allow_frame_decoding`; the sibling below does, and must be the one that changes.
#[test]
fn sec_05_cut_1_screen_a_pass_that_did_not_ask_decodes_no_frames() {
    let base = probe_scene(WIDTH);
    let fixture: Vec<f64> = (0..40).map(|s| s as f64).collect();
    for (tag, paths) in [
        ("none", Vec::new()),
        ("one", stamps(&[0.0])),
        ("fixture", stamps(&fixture)),
    ] {
        let scene = Scene {
            thumbs: vec![("session-tape".to_string(), paths.clone())],
            ..base.clone()
        };
        let (data, stride, top) = painted(&scene);
        let at = |x: i32| {
            let base = top as usize * stride + x as usize * 4;
            (
                data[base + 2] as u32,
                data[base + 1] as u32,
                data[base] as u32,
            )
        };
        // What the row looks like when NOTHING is on it: the same scene with no frames listed, painted and read at
        // the same y. Asking the painter keeps this honest through any palette change — a retyped triple would not.
        let mut bare = scene.clone();
        bare.thumbs.clear();
        let (bare_data, bare_stride, _) = painted(&bare);
        let bare_at = |x: i32| {
            let base = top as usize * bare_stride + x as usize * 4;
            (
                bare_data[base + 2] as u32,
                bare_data[base + 1] as u32,
                bare_data[base] as u32,
            )
        };
        // Right of the gutter (the rail and the row's name plate are other painters' ink) and short of the far
        // edge (the recording's amber end stripes are §1's own marks). Two grounds are legitimate here: a column a
        // cell covers shows that cell's slate placeholder (`paint_frame_cell` fills it first, and with nothing
        // decoded yet that is all a pass draws: the decode happens in the worker, never in the draw), and a column
        // no cell covers shows the empty row. What may NEVER appear is a decoded frame's own pixels.
        // The slate under the kept scene's green tint, which covers this whole fixture (one scene, edge to edge).
        let slate = {
            use naivepost::ui::cut_colours::{KEPT_TINT, KEPT_TINT_ALPHA, SLATE};
            let mix = |c: f64, g: f64| {
                ((c * KEPT_TINT_ALPHA + g * (1.0 - KEPT_TINT_ALPHA)) * 255.0).round() as u32
            };
            (
                mix(KEPT_TINT.0, SLATE.0),
                mix(KEPT_TINT.1, SLATE.1),
                mix(KEPT_TINT.2, SLATE.2),
            )
        };
        let samples: Vec<i32> = (60..540).step_by(7).collect();
        let flat: Vec<i32> = samples
            .iter()
            .copied()
            .filter(|x| !(near(at(*x), slate, 2) || near(at(*x), bare_at(*x), 2)))
            .collect();
        if !paths.is_empty() {
            assert!(
                samples.iter().any(|x| near(at(*x), slate, 2)),
                "{tag}: listed frames show their placeholder cells"
            );
        }
        assert!(
            flat.is_empty(),
            "{tag}: a page that asked for no decoding left {flat:?} samples that are neither the cell placeholder \
             nor the empty row -- i.e. decoded frame pixels"
        );
    }
}
