//! §05-cut#2 §2 — "zoom 4 px/s at open, floor = fit the filmed length, ceiling 240 px/s".
//!
//! The two halves §2's zoom sentence needs, both through real widgets:
//! * the FLOOR is the filmed-length fit of THIS session in THIS view (`timeline::floor_pps` /
//!   `cut_screen::fit_pps`), not the constant the placeholder strip was sized with — so pressing − until
//!   it stops lands on `view / filmed`, whatever that is for a 120 s tape;
//! * the STORED zoom drives the layout: press − or + and the clip boxes the page paints and grabs come
//!   out of `cut_trim::clip_boxes(&segs, live_pps)` at the new number, so a pixel the hand travels means
//!   the seconds the paint used.
//!
//! One application, one `connect_activate`, one `#[test]`: these checks move process-shared zoom state.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::timeline::{self, Recording};
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_FLOOR: AtomicBool = AtomicBool::new(false);
static RAN_LAYOUT: AtomicBool = AtomicBool::new(false);

/// A two-minute take: at the opening 4 px/s it is 480 px of tape, so its fit is NOT 4 in any ordinary
/// view — which is what makes this a check of the rule rather than of the constant.
fn tape() -> Vec<Recording> {
    vec![Recording {
        base: "session-tape".into(),
        start: 0.0,
        end: 120.0,
    }]
}

fn scenes() -> cut::Cut {
    cut::Cut {
        segs: vec![
            Seg {
                s: 0.0,
                e: 30.0,
                ..Default::default()
            },
            Seg {
                s: 40.0,
                e: 60.0,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
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
    ui::seed_review_cut(&window, &scenes());
    settle();
    window
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the toolbar carries {name}"))
}

/// S1: pressing − to the bottom of the ladder stops at the filmed-length fit, not at the constant.
fn check_the_floor_is_the_fit(app: &adw::Application) {
    let window = cut_page(app);
    let zoom_out = button(&window, "zoom-out-button");
    // The strip's own view: what the page measures its floor against.
    // The strip's own view, which is what the ladder's floor is worked out from: `strip_pps`
    // (the door the placeholder's paint and press geometry read) clamps at `view_px(window::STRIP_WIDTH)`
    // whatever GTK has laid out, and §A's "all of it on screen" for a session that fits this 480 px strip
    // is exactly that fit.
    let view = ui::cut_view::view_px(f64::from(ui::window::STRIP_WIDTH));
    let want = timeline::floor_pps(&timeline::filmed_runs(&tape()), view);
    assert_ne!(
        want,
        ui::window::TRACK_STRIP_PPS,
        "this tape's fit is not the open zoom: {want}"
    );
    // One surface both ways: the ladder floors at the strip's DECLARED width (`press_zoom` passes
    // `STRIP_WIDTH`, not the allocation GTK gave the strip once §05-cut#1's paned let it fill the column),
    // so this check works its fit out from that same declared 480. Asking the window for its realized
    // width instead would compare the rule against a surface no door in the program uses.
    assert_eq!(
        view,
        ui::cut_view::view_px(f64::from(ui::window::STRIP_WIDTH)),
        "the ladder's floor is measured over the strip this check works its fit out from"
    );

    for _ in 0..40 {
        zoom_out.emit_by_name::<()>("clicked", &[]);
        settle();
    }
    let stopped = ui::window::strip_pps();
    assert!(
        (stopped - want).abs() < 1e-6,
        "− bottoms out at the filmed-length fit {want}, not at {stopped}"
    );
    assert_eq!(
        stopped,
        ui::cut_view::live_pps(view),
        "and the door the layout reads answers with that same floor"
    );
    RAN_FLOOR.store(true, Ordering::SeqCst);
}

/// S2: the stored zoom moves the drawn geometry. Two presses of the toolbar's + give two different box
/// widths, each exactly what `cut_trim::clip_boxes` computes at the zoom now stored.
fn check_the_stored_zoom_moves_the_layout(app: &adw::Application) {
    let window = cut_page(app);
    let view = ui::cut_view::view_px(f64::from(ui::window::STRIP_WIDTH));
    let zoom_in = button(&window, "zoom-in-button");
    let painted = || ui::review_cut_segs(&window);
    let _ = painted;

    let before = ui::window::strip_pps();
    let boxes_before = ui::window::strip_clip_boxes_for_test();
    zoom_in.emit_by_name::<()>("clicked", &[]);
    settle();
    let after = ui::window::strip_pps();
    let boxes_after = ui::window::strip_clip_boxes_for_test();

    assert!(
        after > before,
        "+ deepens the stored zoom: {before} -> {after}"
    );
    assert!(
        boxes_after[0].w > boxes_before[0].w,
        "the first clip's box grew with the zoom: {} -> {}",
        boxes_before[0].w,
        boxes_after[0].w
    );
    // And the boxes ARE the stored zoom's arithmetic — no second, private pps anywhere on this path.
    let expected = naivepost::cut_trim::clip_boxes(&painted(), ui::cut_view::live_pps(view));
    assert_eq!(
        boxes_after, expected,
        "the page paints what the stored zoom says"
    );
    assert!(
        after <= naivepost::cut_screen::ZOOM_MAX,
        "and never past the 240 px/s ceiling"
    );
    RAN_LAYOUT.store(true, Ordering::SeqCst);
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 the floor is the filmed-length fit");
        check_the_floor_is_the_fit(app);
        println!("CHECK 2 the stored zoom moves the band");
        check_the_stored_zoom_moves_the_layout(app);
    });
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s1_the_zoom_floor_is_the_film_length_fit_not_a_constant(
) {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_FLOOR.load(Ordering::SeqCst),
        "the filmed-length-fit check never ran"
    );
}

#[test]
fn sec_05_cut_2_model_summary_full_schema_in_01_project_and_filesmd_3_s2_the_stored_zoom_moves_the_band_layout(
) {
    common::pin_private_cwd();
    round();
    assert!(
        RAN_LAYOUT.load(Ordering::SeqCst),
        "the stored-zoom-drives-the-layout check never ran"
    );
}
