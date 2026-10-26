//! §05-cut through the real widgets: one press does what it says ONCE, and the line keys go where they point.
//!
//! - F3.8 S3: the effect ✕ removes exactly one effect, however many times the lane was refreshed before the
//!   click. The first rewrite connected the ✕'s handler again on every refresh, so after a few repaints one click
//!   removed several effects.
//! - F2.4 S2/S3: `,` steps the line one frame BACK and `.` one frame forward (the first rewrite stepped both
//!   forward); with a clip or an edge held, ← / → and the ‹f / f› buttons nudge THAT, one frame each (the first
//!   rewrite answered "nudged" and moved nothing, and the buttons ignored the hold).
//!
//! One GTK round per test process (`g_application_run` takes the default main context once).

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_line::LinePos;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_KILL: AtomicBool = AtomicBool::new(false);
static RAN_KEYS: AtomicBool = AtomicBool::new(false);
static RAN_NUDGE: AtomicBool = AtomicBool::new(false);

/// A Cut-page window with `seeded` as its cut and its history opened on it, the effects lane drawn.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// A zoom wide enough to carry its ✕ (10 s; `fx_band::KILL_MIN_PX` is 32 px).
fn wide_zoom(t: f64) -> Fx {
    Fx {
        kind: "zoom".to_string(),
        t,
        dur: 10.0,
        cx: Some(0.4),
        cy: Some(0.6),
        wf: Some(0.5),
        hf: Some(0.4),
        ..Default::default()
    }
}

/// Fire a key the way the toolkit does: the controller's own `key-pressed`.
fn fire_key(controller: &gtk::EventControllerKey, key: gtk::gdk::Key) {
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&key, &0u32, &gtk::gdk::ModifierType::empty()],
    );
    settle();
}

fn close(window: adw::ApplicationWindow) {
    window.close();
    settle();
}

/// F3.8 S3: refreshed four times, then one click on the first ✕: exactly one effect gone.
fn check_one_kill_removes_one_effect(app: &adw::Application) {
    let mut seeded = Cut::default();
    seeded.fx = vec![wide_zoom(20.0), wide_zoom(40.0), wide_zoom(60.0)];
    let window = cut_page(app, &seeded);
    for _ in 0..4 {
        ui::refresh_effects_lane(&window);
        settle();
    }
    common::click(&window, "fx-remove-0");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        2,
        "one click on the ✕ removes ONE effect, however often the lane was redrawn"
    );
    RAN_KILL.store(true, Ordering::SeqCst);
    close(window);
}

/// F2.4 S2: `,` steps one frame back, `.` one frame forward, through the window's own key controller.
fn check_comma_steps_back_and_period_forward(app: &adw::Application) {
    let mut seeded = Cut::default();
    seeded.segs = vec![Seg {
        s: 0.0,
        e: 90.0,
        ..Default::default()
    }];
    let window = cut_page(app, &seeded);
    ui::set_held_clip(None);
    ui::set_line_position(&window, LinePos { t: 10.0 });
    let frame = 1.0 / naivepost::cut_fps::session_fps(&window);
    let keys = ui::line_key_controller(&window).expect("the window wired its line keys");
    fire_key(&keys, gtk::gdk::Key::comma);
    let back = ui::line_position(&window).t;
    assert!(
        (back - (10.0 - frame)).abs() < 1e-6,
        "`,` steps one frame BACK: 10 -> {back} (a frame is {frame} s)"
    );
    fire_key(&keys, gtk::gdk::Key::period);
    let forth = ui::line_position(&window).t;
    assert!(
        (forth - 10.0).abs() < 1e-6,
        "`.` steps one frame forward again: {back} -> {forth}"
    );
    RAN_KEYS.store(true, Ordering::SeqCst);
    close(window);
}

/// F2.4 S2/S3: with a clip held, → nudges it a frame later and ← a frame back, the line staying put; with an
/// edge held, f› moves THAT border (the one nearest the line) a frame later. Each nudge is one ↶.
fn check_a_held_clip_or_edge_is_nudged(app: &adw::Application) {
    let clip = Seg {
        s: 20.0,
        e: 30.0,
        ..Default::default()
    };
    let mut seeded = Cut::default();
    seeded.segs = vec![
        Seg {
            s: 0.0,
            e: 10.0,
            ..Default::default()
        },
        clip.clone(),
        Seg {
            s: 50.0,
            e: 60.0,
            ..Default::default()
        },
    ];
    let window = cut_page(app, &seeded);
    let frame = 1.0 / naivepost::cut_fps::session_fps(&window);
    ui::set_line_position(&window, LinePos { t: 25.0 });
    ui::set_held_clip(Some(clip.clone()));
    let keys = ui::line_key_controller(&window).expect("the window wired its line keys");
    fire_key(&keys, gtk::gdk::Key::Right);
    let moved = ui::review_cut_of(&window).segs[1].clone();
    assert!(
        (moved.s - (20.0 + frame)).abs() < 1e-6 && (moved.e - (30.0 + frame)).abs() < 1e-6,
        "→ slid the held clip one frame later, its length kept: {moved:?}"
    );
    assert!(
        (ui::line_position(&window).t - 25.0).abs() < 1e-9,
        "and the line stayed where it was"
    );
    fire_key(&keys, gtk::gdk::Key::Left);
    let back = ui::review_cut_of(&window).segs[1].clone();
    assert!((back.s - 20.0).abs() < 1e-6, "← slid it back: {back:?}");
    common::click(&window, "undo-button");
    let undone = ui::review_cut_of(&window).segs[1].clone();
    assert!(
        (undone.s - (20.0 + frame)).abs() < 1e-6,
        "each nudge is its own ↶: {undone:?}"
    );
    close(window);

    // An edge: the line sits by the END border, so f› moves the end, not the start.
    let window = cut_page(app, &seeded);
    let frame = 1.0 / naivepost::cut_fps::session_fps(&window);
    ui::set_line_position(&window, LinePos { t: 29.9 });
    ui::set_held_edge(Some(clip.clone()));
    common::click(&window, "line-step-forward");
    let trimmed = ui::review_cut_of(&window).segs[1].clone();
    assert!(
        (trimmed.e - (30.0 + frame)).abs() < 1e-6 && (trimmed.s - 20.0).abs() < 1e-9,
        "f› with the end edge held moved that border one frame: {trimmed:?}"
    );
    RAN_NUDGE.store(true, Ordering::SeqCst);
    ui::set_held_clip(None);
    close(window);
}

#[test]
fn sec_05_cut_every_press_does_its_thing_once() {
    let root = common::temp_dir_created("cut-press-once");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    common::one_app_round(|app| {
        check_one_kill_removes_one_effect(app);
        check_comma_steps_back_and_period_forward(app);
        check_a_held_clip_or_edge_is_nudged(app);
        app.quit();
    });
    assert!(RAN_KILL.load(Ordering::SeqCst), "the ✕ check never ran");
    assert!(
        RAN_KEYS.load(Ordering::SeqCst),
        "the `,`/`.` check never ran"
    );
    assert!(
        RAN_NUDGE.load(Ordering::SeqCst),
        "the nudge check never ran"
    );
}
