//! F2.5 badge **2** — the wire: the Cut page's own band scene carries the lane plate WITH its channel
//! layout, so what `cut_tracks::draw_lane_rows` paints is the state a check can read (the rule itself is
//! pinned in tests/suite/cut_lane_plate.rs).
//!
//! One application, one `connect_activate`, exactly one `#[test]`: the checks move process-shared state
//! (this window's review-cut slot and the page's recording list), so they run once and report through the
//! flags the test asserts on — the shape tests/suite/cut_hear_mix_widgets.rs uses.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use common::{fixture_dir, settle};
use naivepost::cut::{Cut, Lane, Seg};
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::{cut_lanes, ui};

static RAN_PLATE: AtomicBool = AtomicBool::new(false);

/// A window sitting on the Cut page over a session whose cwd is its own temp root, so nothing resolves
/// the project under `rust/`.
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!("np-laneplate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(
        ui::state(&window).page,
        Page::Cut,
        "the Cut page is showing"
    );
    window
}

/// Badge 2 through the page's own door: seed a cut with one recorder lane, ask the SAME scene the band
/// painter asks (`cut_view::band_scene_for`), and read the plate off it. The file is not on disk, so the
/// probe cannot answer and the plate falls back to the name plus the mono default of an unknown count —
/// which is the specific-and-local half of the rule, seen live.
fn check_the_live_band_scene_carries_the_lane_plate(app: &adw::Application) {
    let window = cut_window(app);
    ui::set_session_recordings(&[Recording {
        base: "lecture".into(),
        start: 0.0,
        end: 120.0,
    }]);
    let mut seeded = Cut::default();
    seeded.segs.push(Seg {
        s: 0.0,
        e: 60.0,
        ..Default::default()
    });
    seeded.lanes.push(Lane {
        name: "2026-09-16 17-26-20".into(),
        src: "project:audio/2026-09-16 17-26-20.wav".into(),
        at: 30.0,
        off: 0.0,
        dur: 45.0,
    });
    ui::seed_review_cut(&window, &seeded);
    settle();

    let scene = ui::cut_view::band_scene_for(&window);
    assert_eq!(
        scene.lanes.len(),
        1,
        "the page's scene carries the lane that was seeded"
    );
    assert_eq!(
        scene.lane_plates.len(),
        scene.lanes.len(),
        "one plate per lane, in the same order — the painter indexes them together"
    );
    let plate = &scene.lane_plates[0];
    assert!(
        plate.starts_with("2026-09-16 17-26-20"),
        "the plate begins with the lane's name: {plate}"
    );
    // Whatever the probe could or could not say, the plate says a layout the rule allows: badge 2 wants
    // the channel layout spelled on the plate, never a bare nameless bar.
    let tail = plate
        .strip_prefix("2026-09-16 17-26-20")
        .unwrap_or_default()
        .trim()
        .to_string();
    assert!(
        matches!(tail.as_str(), "" | "mono" | "L=R" | "L" | "R"),
        "the plate ends in the channel layout the rule answers: {plate}"
    );
    // And the scene agrees with the rule asked directly, so the painter cannot drift from it.
    let expected = cut_lanes::plates(&scene.lanes);
    assert_eq!(
        scene.lane_plates, expected,
        "the band reads the same plates `cut_lanes::plates` builds"
    );
    RAN_PLATE.store(true, Ordering::SeqCst);
    window.close();
}

fn round() {
    common::one_app_round(|app| {
        check_the_live_band_scene_carries_the_lane_plate(app);
    });
}

#[test]
fn f2_5_s2_the_lane_plate_on_the_live_band_carries_the_layout() {
    round();
    assert!(
        RAN_PLATE.load(Ordering::SeqCst),
        "the live-band lane-plate check never ran"
    );
}
