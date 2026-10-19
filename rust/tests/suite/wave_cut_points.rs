//! §02-services.md §4.1 — no aligner, so the Cut page's cut points come off the waveform.
//!
//! The rules themselves (`silence_spans`, `cut_points`, `cut_source`) belong to [`crate::degraded`] and
//! `tests/suite/degraded_modes.rs` pins them; this file pins the PROGRAM side — the envelope this build can
//! actually read off `cache/waves`, the row a band belongs to, and the settings question that decides
//! which mode the Cut page is in.

use naivepost::cut_wave_edges as wave_edges;
use naivepost::wave::{self, Wave};

/// A recording whose file exists, so its size and mtime can be stat-ed — the stamps a cache is checked
/// against must come from the recording, never from the cache file being checked.
fn recording(tree: &naivepost::layout::Tree, name: &str) -> std::path::PathBuf {
    let path = tree.sources_dir().join(name);
    std::fs::create_dir_all(tree.sources_dir()).unwrap();
    std::fs::write(&path, [7u8; 4_096]).unwrap();
    path
}

fn source(path: &str, footage: bool) -> naivepost::project::Source {
    naivepost::project::Source {
        path: path.to_string(),
        footage,
        ..Default::default()
    }
}

/// A peak list: `loud` buckets between two long silences, with every second derived from the constants
/// so a change to them moves the fixture instead of silently breaking it.
///
/// [`crate::degraded::dbfs`] reads a peak byte as its own amplitude (`20·log10(peak)`), so a byte of 1
/// is 0 dB — never quiet — and the silence has to be a FRACTION of full scale: 0.02 amplitude is −34 dB,
/// inside [`cut_wave_edges::SILENCE_THRESHOLD_DB`] of −35 dB, while [`LOUD`] at 200 is +46 dB.
const HZ: f64 = 5.0;
/// Two buckets of silence: `2 / HZ` = 0.4 s, which is exactly `SILENCE_MIN_SECONDS`, so each run is a
/// silence one may cut in and not the gap between two words.
const RUN_BUCKETS: usize = 3;
/// The noise's length: 20 s, so the envelope's centre is 10 s from either silence and past the 5 s
/// reach of both. Kept well over `TOLERANCE_SECONDS` so the refusal case cannot land in reach by accident.
const NOISE_SECONDS: f64 = 20.0;
/// A bucket that is sound by a wide margin, so rounding cannot make it quiet.
const LOUD: u8 = 200;
/// A bucket quiet enough to cut in: `dbfs(0)` is minus infinity (silence, not `log(0)`), which is the
/// only byte under the −35 dB floor — a byte of 1 is already 0 dB on this scale.
const QUIET: u8 = 0;

/// The dBFS the mode itself would read for a peak byte (`dbfs` is private, so this is its formula):
/// `20·log10(peak)`, which treats the byte as an amplitude of itself — so a quiet bucket is a small
/// NUMBER of bytes, not a small fraction of 255. Kept here so the fixture cannot drift from the rule.
fn dbfs(peak: u8) -> f64 {
    if peak == 0 {
        return f64::NEG_INFINITY;
    }
    20.0 * f64::from(peak).ln() / 10.0_f64.ln()
}

/// A quiet run of 4× the floor, 2 s of noise, another quiet run. Starts at the lane's `at`.
fn envelope_wave() -> Wave {
    let mut peaks = vec![QUIET; RUN_BUCKETS];
    peaks.extend(std::iter::repeat(LOUD).take((NOISE_SECONDS * HZ) as usize));
    peaks.extend(vec![QUIET; RUN_BUCKETS]);
    Wave { hz: HZ, chans: vec![peaks] }
}

/// Seconds of one bucket, from the same constants the wave is built from.
const BUCKET: f64 = 1.0 / HZ;
/// The noise starts here, and its end is one bucket later than that plus its own length.
fn noise_start(at: f64) -> f64 {
    at + RUN_BUCKETS as f64 * BUCKET
}
/// The last second of the second silent run, where the wave still has peaks to answer from.
fn near_tail(at: f64) -> f64 {
    noise_end(at) + RUN_BUCKETS as f64 * BUCKET / 2.0
}
fn noise_end(at: f64) -> f64 {
    noise_start(at) + NOISE_SECONDS
}

fn tree_with_cache(tag: &str, lane: &str, at: f64) -> (std::path::PathBuf, naivepost::layout::Tree) {
    let root = std::path::PathBuf::from(format!(
        "/tmp/naivepost-wave-edges-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    let file = recording(&tree, "lecture.mkv");
    let meta = std::fs::metadata(&file).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // The cache is stamped against THIS file; `at` is what the session clock says for it, and an
    // unstamped file like this one sits at the session's start, so a test that wants a later `at`
    // shifts the expected seconds rather than the file.
    let _ = at;
    wave::write(&tree, lane, &envelope_wave(), meta.len() as i64, mtime).unwrap();
    (root, tree)
}

fn sources() -> Vec<naivepost::project::Source> {
    vec![source("project:sources/lecture.mkv", true)]
}

#[test]
fn sec_02_services_4_degraded_modes_s15_an_envelope_off_the_cache_gives_the_band_its_silences() {
    // The fixture proves itself first: the quiet really is under the threshold the module reads and the
    // loud really is not, or this would be a test of a wave that is all silence.
    assert!(dbfs(QUIET) <= wave_edges::SILENCE_THRESHOLD_DB, "{} dB", dbfs(QUIET));
    assert!(dbfs(LOUD) > wave_edges::SILENCE_THRESHOLD_DB);
    assert!(
        RUN_BUCKETS as f64 / HZ >= wave_edges::SILENCE_MIN_SECONDS,
        "each quiet run must reach the minimum"
    );
    assert!(NOISE_SECONDS > wave_edges::TOLERANCE_SECONDS, "the noise outstretches the reach");

    let (_root, tree) = tree_with_cache("read", "lecture", 0.0);
    let envs = wave_edges::read_lanes(&tree, &sources());
    assert_eq!(envs.len(), 1, "the footage lane is read");
    assert_eq!(envs[0].lane, "lecture", "the key drops the extension");
    assert_eq!(envs[0].hz, HZ);

    let silences = wave_edges::silences_of(&envs[0]);
    assert_eq!(silences.len(), 2, "{silences:?}: a quiet run before and after the noise");
    // Both runs are 4 × the minimum, so neither is the gap between two words. One bucket is a fifth of
    // a second at this rate, so a span's ends are a multiple of that — hence the half-bucket slack.
    // Each run is three buckets at 5 Hz = 0.6 s, over the 0.4 s floor; a span's ends are multiples of a
    // bucket, so the assertion carries that slack rather than a round second.
    for (from, to) in &silences {
        assert!(
            (to - from - RUN_BUCKETS as f64 / HZ).abs() < BUCKET,
            "{from}..{to} is not a silence one may cut in"
        );
        assert!(to - from >= wave_edges::SILENCE_MIN_SECONDS, "{from}..{to} is a gap, not a silence");
    }
    // An edge inside a silence stays exactly where it was asked for: the mode does not move a cut that
    // is already clean. The far end sits past the last peak, where there is no audio to check at all,
    // so it is the refusal §4.1 asks for rather than a cut into nothing.
    let inside = silences[0].0 + 0.05;
    let past = noise_end(0.0) + RUN_BUCKETS as f64 * BUCKET + 60.0;
    let (start, end) = wave_edges::band_edges(&envs, "lecture", inside, past);
    assert_eq!(start, Some(inside), "inside a silence: stays put");
    assert_eq!(end, None, "beyond the recording there is no silence to cut in: refused");

    // An edge outside a silence is pulled onto its NEAR END, and only from within the reach.
    // `noise_start` is `RUN_BUCKETS / HZ` exactly, so it is the first bucket of the noise and its
    // distance to the first silence's end is one bucket — inside the reach — and the clamp to that
    // silence moves it back onto the silence. `near` lies inside the second silence, so it stays put.
    let near = near_tail(0.0);
    let (start, end) = wave_edges::band_edges(&envs, "lecture", noise_start(0.0), near);
    assert_eq!(start, Some(silences[0].1), "one bucket off: pulled back onto the silence's end");
    assert_eq!(end, Some(near), "inside the second silence, so it stays where it was drawn");

    // The reach, measured from outside in. An edge just OUTSIDE the second silence's start is one bucket
    // from it, inside TOLERANCE_SECONDS, and moves back onto that start; an edge just INSIDE a silence
    // keeps the second it asked for, because a point inside a silence scores a distance of zero and beats
    // every nearby silence.
    let near_second = silences[1].0 - BUCKET;
    let (one, _) = wave_edges::band_edges(&envs, "lecture", near_second, near);
    assert_eq!(one, Some(silences[1].0), "{near_second} s is one bucket out: onto the silence's start");
    let inside_second = silences[1].0 + BUCKET;
    let (stays, _) = wave_edges::band_edges(&envs, "lecture", inside_second, near);
    assert_eq!(stays, Some(inside_second), "inside a silence: stays where it was drawn");
    // The wave is 23 buckets at 5 Hz = 4.6 s of centres, and the reach is 5 s, so NOTHING outside a
    // silence can be out of reach here; the refusal this mode can give comes from the far end instead, in
    // the `past` case above: past the recording's own audio, where there is no envelope left to check.
}

#[test]
fn sec_02_services_4_degraded_modes_s15_no_envelope_means_no_cut_points_rather_than_invented_ones() {
    let root = std::path::PathBuf::from(format!("/tmp/naivepost-wave-edges-none-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    recording(&tree, "lecture.mkv");
    // No cache file at all: not an error, and not a guess either.
    assert!(wave_edges::read_lanes(&tree, &sources()).is_empty());
    let (start, end) = wave_edges::band_edges(&[], "lecture", 12.0, 30.0);
    assert_eq!((start, end), (None, None), "\"unable to cut between two words of one breath\"");

    // A stale cache — made against a DIFFERENT recording — is a miss for the same reason.
    let wave = Wave { hz: HZ, chans: vec![vec![QUIET; 8]] };
    wave::write(&tree, "lecture", &wave, 1, 1).unwrap();
    assert!(
        wave_edges::read_lanes(&tree, &sources()).is_empty(),
        "a cache stamped against another file is not this recording's envelope"
    );
}

#[test]
fn sec_02_services_4_degraded_modes_s15_a_bands_lane_is_the_row_it_was_dragged_on() {
    let all = vec![
        source("project:sources/cam-a.mkv", true),
        source("project:sources/mic.wav", false),
        source("project:sources/cam-b.mkv", true),
    ];
    assert_eq!(wave_edges::lane_for_row(&all, 0).as_deref(), Some("cam-a"));
    assert_eq!(
        wave_edges::lane_for_row(&all, 1).as_deref(),
        Some("cam-b"),
        "the sound row is not a footage row, so row 1 is the second camera"
    );
    assert_eq!(wave_edges::lane_for_row(&all, 2), None, "no third footage row");
}

#[test]
fn sec_02_services_4_degraded_modes_s16_the_settings_box_decides_where_cut_points_come_from() {
    // §4.1's condition is the empty box, which is also its default: no aligner named, waveform mode.
    // The mode answers the same question the aligner path asks, so the two routes cannot disagree:
    // a name that survives trimming is word timings, blank or whitespace is the waveform.
    assert!(wave_edges::waveform_mode(""));
    assert!(wave_edges::waveform_mode("   "));
    assert!(!wave_edges::waveform_mode("qwen3-aligner"));

    // Read the way the app reads it: a config home with nothing written, and one that names an aligner.
    let root = std::path::PathBuf::from(format!("/tmp/naivepost-wave-edges-conf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = naivepost::settings::Paths {
        config_dir: root.join("config"),
        data_dir: root.join("data"),
    };
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    // No conf file: the default, which names no aligner.
    let conf = naivepost::settings::read(&paths).unwrap();
    assert!(conf.align_model.is_empty(), "a fresh machine names no aligner");
    assert!(wave_edges::waveform_mode(&conf.align_model));

    let mut named = conf.clone();
    named.align_model = "qwen3-aligner".to_string();
    naivepost::settings::save(&paths, &named).unwrap();
    let read = naivepost::settings::read(&paths).unwrap();
    assert_eq!(read.align_model, "qwen3-aligner");
    assert!(!wave_edges::waveform_mode(&read.align_model), "with word timings, word edges");
}

#[test]
fn sec_02_services_4_degraded_modes_s15_a_band_higher_than_the_cameras_offers_keeps_its_ends() {
    // Row 3 with two footage rows: no lane, so no points, so `snap_end` leaves both ends where they were
    // drawn rather than the press inventing a second. The window seam answers the same way with no
    // project open at all: an empty list is "nothing this mode can offer".
    let all = vec![source("project:sources/cam-a.mkv", true)];
    assert_eq!(wave_edges::lane_for_row(&all, 3), None);
    assert_eq!(
        cut_verbs_snap(10.0, &[60.0, 90.0]),
        (10.0, 10.0),
        "points too far away move nothing: the reach is one snap's worth"
    );
}

/// What the window hands `cut_verbs::add`'s `snap_points`: nothing, when this mode has no envelope.
/// `snap_end` then answers the asked second back, which is the "ends stay where they were drawn" half
/// of §4.1 — the mode refuses, it does not guess.
fn cut_verbs_snap(band_end: f64, points: &[f64]) -> (f64, f64) {
    use naivepost::cut_verbs;
    (cut_verbs::snap_end(band_end, &[]), cut_verbs::snap_end(band_end, points))
}
