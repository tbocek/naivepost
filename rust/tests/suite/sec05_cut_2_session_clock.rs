//! §05-cut#2 §2 — the session clock, and the hand correction it carries.
//!
//! §05 §B spells the conversion (`at(t) = t − start + off`, `sessionAt(local) = start + local − off`) and
//! says a slid recording moves every lane of its base; §2 names the session clock as part of the model.
//! The two halves here:
//! * PURE — [`timeline::local_at`] / [`timeline::session_at`] round-trip around a file's own offset, with
//!   the shift from [`timeline::shift_of`] folded in;
//! * THE WIRE — seed a cut whose camera row carries `shift = -19 s`, press nothing, and read the boxes the
//!   band paints: they sit 19 s to the left of the same cut unshifted, because the page applies the
//!   correction rather than storing it.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::timeline::{self, Recording};
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_ROUNDTRIP: AtomicBool = AtomicBool::new(false);
static RAN_SHIFTED: AtomicBool = AtomicBool::new(false);
static RAN_WIRE: AtomicBool = AtomicBool::new(false);

const PPS: f64 = 4.0;

/// A scene on the camera row named `cam<N>` — the naming `timeline::kept_footage_recordings` uses, which is
/// what makes a shift entry find the scenes it belongs to.
fn scene(s: f64, e: f64, cam: i32) -> Seg {
    Seg { s, e, cam, ..Default::default() }
}

fn tape() -> Vec<Recording> {
    vec![Recording { base: "cam0".into(), start: 0.0, end: 120.0 }]
}

fn cut_with(shift: Option<f64>) -> cut::Cut {
    let mut out = cut::Cut {
        segs: vec![scene(10.0, 30.0, 0), scene(40.0, 60.0, 0)],
        ..Default::default()
    };
    if let Some(seconds) = shift {
        out.shift.insert("cam0".to_string(), seconds);
    }
    out
}

/// S2 (pure): the two directions are exact inverses around a file's own start and offset, for real values
/// including a source that starts off the band and one already corrected by hand.
fn check_the_conversion_round_trips() {
    // (start, off, shift, local) — a lecture from 0, a lane windowing a file at 20 s, a slid take, and a
    // negative-shifted second camera.
    let cases = [
        (0.0f64, 0.0, 0.0, 0.0),
        (0.0, 0.0, 0.0, 137.5),
        (20.0, 3.25, 0.0, 7.0),
        (0.0, 0.0, -19.0, 44.0),
        (12.0, -4.0, 2.5, 91.25),
    ];
    for (start, off, shift, local) in cases {
        let session = timeline::session_at(local, start, off, shift);
        let back = timeline::local_at(session, start, off, shift);
        assert!(
            (back - local).abs() < 1e-9,
            "sessionAt({local}) = {session} must come back to {local}, got {back}"
        );
        // And §B's shape holds literally for the plain case: sessionAt(local) = start + local − off.
        if shift == 0.0 {
            assert!(
                (session - (start + local - off)).abs() < 1e-9,
                "§B's sessionAt for start {start} off {off}: {session}"
            );
        }
    }
    // shift_of answers 0.0 for a source nobody slid, and the stored number for one that was.
    let shifted = cut_with(Some(-19.0));
    assert_eq!(timeline::shift_of(&shifted, "cam0"), -19.0);
    assert_eq!(timeline::shift_of(&shifted, "cam1"), 0.0, "no entry means no correction");
    RAN_ROUNDTRIP.store(true, Ordering::SeqCst);
}

/// S1: the same cut with and without a hand correction puts its clips 19 s apart, converted through
/// `session_at` — the map is not decoration, it moves coordinates.
fn check_a_shift_moves_the_coordinates() {
    let flat = naivepost::cut_trim::clip_boxes(&cut_with(None).segs, PPS);
    let shifted = naivepost::cut_trim::clip_boxes(&cut_with(Some(-19.0)).segs, PPS);
    assert_eq!(flat, shifted, "the raw box rule knows nothing about shifts \u{2014} that is the point");

    // The page's own answer: the same list with the corrections applied, as `cut_tracks` paints it.
    let moved = ui::cut_tracks::clip_boxes_shifted(
        &cut_with(Some(-19.0)).segs,
        &cut_with(Some(-19.0)).shift,
        PPS,
    );
    let want = cut_with(Some(-19.0))
        .segs
        .iter()
        .map(|seg| timeline::session_at(seg.s, 0.0, 0.0, timeline::shift_of(&cut_with(Some(-19.0)), "cam0")) * PPS)
        .collect::<Vec<_>>();
    let got = moved.iter().map(|b| b.x).collect::<Vec<_>>();
    assert_eq!(got, want, "each clip sits where session_at put it");
    assert!(
        (moved[0].x - flat[0].x - 19.0 * PPS).abs() < 1e-9,
        "a shift of -19 s puts the material 19 s later on the session clock, so the clip moves 76 px right: {} -> {}",
        flat[0].x,
        moved[0].x
    );
    // Widths survive a slide: the clip is the same length wherever it sits.
    assert_eq!(flat[0].w, moved[0].w, "a slide changes place, not length");
    RAN_SHIFTED.store(true, Ordering::SeqCst);
}

fn cut_page(app: &adw::Application, seeded: &cut::Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::set_session_recordings(&tape());
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// S3 wire: seed a cut whose row carries a -19 s correction and read the geometry the band paints from.
/// Nothing else was pressed: the correction alone must move the clips.
fn check_a_slided_recording_shows_at_its_new_place(app: &adw::Application) {
    let plain = cut_page(app, &cut_with(None));
    let before = ui::window::band_clip_boxes_for_test(&plain);
    assert_eq!(before.len(), 2, "both kept scenes drew boxes");

    let slid = cut_page(app, &cut_with(Some(-19.0)));
    let after = ui::window::band_clip_boxes_for_test(&slid);
    assert_eq!(after.len(), 2, "and still both after the slide");
    assert!(
        (after[0].x - before[0].x - 19.0 * ui::cut_view::pps()).abs() < 1e-6,
        "the slid row's first clip sits 19 s later on the clock: {} vs {}",
        before[0].x,
        after[0].x
    );
    assert_eq!(before[0].w, after[0].w, "same clip, same length");
    RAN_WIRE.store(true, Ordering::SeqCst);
}

fn round() {
    check_the_conversion_round_trips();
    check_a_shift_moves_the_coordinates();
    common::one_app_round(|app| {
        println!("CHECK a slid recording draws at its new place");
        check_a_slided_recording_shows_at_its_new_place(app);
    });
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s1_a_shifted_recording_moves_its_coordinates() {
    common::pin_private_cwd();
    round();
    assert!(RAN_SHIFTED.load(Ordering::SeqCst), "the shift-moves-coordinates check never ran");
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s2_the_conversion_round_trips_around_the_offset() {
    common::pin_private_cwd();
    round();
    assert!(RAN_ROUNDTRIP.load(Ordering::SeqCst), "the round-trip check never ran");
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s3_a_slided_recording_shows_at_its_new_place() {
    common::pin_private_cwd();
    round();
    assert!(RAN_WIRE.load(Ordering::SeqCst), "the seeded-shift widget check never ran");
}
