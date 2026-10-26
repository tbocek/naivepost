//! §05-cut#1-screen "Tracks": "each row's first audio track as a strip below it". The strip is the row's own sound,
//! read off `cache/waves/<base>.wave` (§01 §6) as the cache was stamped: against the RECORDING it was measured from,
//! its size and mtime. The first port asked with the cache file's own length and no mtime, so every read missed and
//! the strip was a flat line on the live page. Drawn on the prototype's meter curve (`go/cut_audio.go` `iecScale`):
//! the quietest byte sits on the floor, full scale at the top.

use std::path::Path;

use naivepost::cut::Seg;
use naivepost::timeline::Recording;
use naivepost::ui::cut_tracks::{self as tracks, Scene};
use naivepost::ui::cut_wave_rows;

use crate::common;

#[test]
fn sec_05_cut_1_screen_the_wave_strip_reads_on_the_meter_curve() {
    assert_eq!(cut_wave_rows::meter(0.0), 0.0, "silence is the floor");
    assert!(
        cut_wave_rows::meter(1.0 / 255.0).abs() < 1e-9,
        "and so is the quietest byte the envelope can hold"
    );
    assert!(
        (cut_wave_rows::meter(1.0) - 1.0).abs() < 1e-9,
        "full scale reaches the top"
    );
    let mut last = 0.0;
    for byte in 1..=255u32 {
        let level = cut_wave_rows::meter(f64::from(byte) / 255.0);
        assert!(level >= last, "the curve never falls: {byte} -> {level}");
        last = level;
    }
    // A column is the LOUDEST bucket it covers, so zooming out does not fade the wave.
    let peaks = [0u8, 10, 200, 30, 0];
    assert_eq!(
        cut_wave_rows::peak(&peaks, 1.0, 0.0, 5.0),
        200.0 / 255.0,
        "the loudest of the buckets a column covers"
    );
    assert_eq!(
        cut_wave_rows::peak(&peaks, 1.0, 3.0, 4.0),
        30.0 / 255.0,
        "and the one it lands in when it covers less than one"
    );
}

/// The project: one recording on disk, its envelope stamped against it the way `wave_build::ensure` stamps it,
/// loud for the first 5 s and silent after.
fn project(root: &Path) -> std::path::PathBuf {
    let dir = root.join("strip.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let source = dir.join("sources/take-a.mkv");
    std::fs::write(&source, vec![7u8; 4096]).unwrap();
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/take-a.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    let meta = std::fs::metadata(&source).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut peaks = vec![230u8; 1000];
    peaks.extend(vec![0u8; 1000]);
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    naivepost::wave::write(
        &tree,
        "take-a",
        &naivepost::wave::Wave {
            hz: 200.0,
            chans: vec![peaks],
        },
        meta.len() as i64,
        mtime,
    )
    .unwrap();
    dir
}

#[test]
fn sec_05_cut_1_screen_the_wave_strip_draws_the_rows_own_sound() {
    common::pin_private_cwd();
    let root = common::temp_dir("wave-strip");
    let dir = project(&root);
    naivepost::startup::set_open_dir(&dir);
    let pps = 20.0;
    let scene = Scene {
        pps,
        origin: 0.0,
        segs: vec![Seg {
            s: 0.0,
            e: 0.5,
            ..Default::default()
        }],
        fx: Vec::new(),
        recordings: vec![Recording {
            base: "take-a".into(),
            start: 0.0,
            end: 10.0,
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
    };
    let (w, h) = (
        300,
        tracks::band_height_px(&scene, tracks::THUMB_DEFAULT_PX).ceil() as i32,
    );
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).expect("a surface");
    {
        let cr = cairo::Context::new(&surface).expect("a context");
        tracks::paint_at(&cr, &scene, w as f64, h as f64, tracks::THUMB_DEFAULT_PX);
    }
    let stride = surface.stride() as usize;
    let data = surface.data().expect("pixels").to_vec();
    let blue = |x: i32, y: i32| {
        let at = y as usize * stride + x as usize * 4;
        let (r, b) = (data[at + 2], data[at]);
        b > r + 40 && b > 120
    };
    // The strip is the row's lower part: count the blue rows of one column at a loud second and a silent one.
    let strip_top = tracks::band_height_px(&scene, tracks::THUMB_DEFAULT_PX) - tracks::ROW_WAVE_PX;
    let column = |t: f64| {
        let x = tracks::band_x(&scene, t) as i32;
        (strip_top as i32..h).filter(|y| blue(x, *y)).count()
    };
    let (loud, quiet) = (column(2.0), column(8.0));
    println!("WAVE loud column {loud} px, quiet column {quiet} px");
    assert!(
        loud as f64 > tracks::ROW_WAVE_PX * 0.6,
        "a loud second stands most of the strip tall: {loud} px"
    );
    assert_eq!(quiet, 0, "a silent second leaves only the baseline");
}
