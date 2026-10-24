// §06-effects#2-the-lane-and-the-preview S2 — the Effect-menu SHOT is the page with the menu open over it.
//
// `spec/QUESTIONS.md`, §06 answer 3(b): "06-effect-menu is the Cut page with the '+ Effect' menu open under its
// button, the toolbar and the tracks around it: the render shows the menu alone, stretched over the whole window.
// The menu is a surface of its own, so the snapshot draws the window first and the open menu on top of it, where
// it opens and at its own size; that is a fix in the snapshot." These checks pin each half of that sentence as a
// NUMBER off the artifact or off the doors that place it:
//   * the asked file is the WINDOW's shape, not the menu box's (a subject-stretched shot cannot be this);
//   * the panel's painted size is its OWN measured size — compact, a fraction of the frame;
//   * its top-left sits below the app's own ✚ Effect button's rect, clamped inside the window.
// Nothing here re-derives a colour or a row list: the rows are proven by `sec_06_effects_2_staging.rs`.

use std::path::PathBuf;

use adw::prelude::*;
use naivepost::{snapshot_effect_menu, snapshot_face, ui};

use crate::common;
use common::settle;

/// This test binary's own folder — never a fixed path, since the suite runs files in parallel.
fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("np-s06e2menu-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp root created");
    dir
}

/// One application, one activation: `gio` refuses a second `run_with_args` in a process, so every window-building
/// check here is a step of ONE run (the same shape `sec_06_effects_2_staging.rs` uses).
static RAN_SHOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where the spec's picture puts the menu: a COMPACT popup, a fraction of the frame. `spec/img/06-effect-menu.png`
/// is 1098 px wide at its scaled height and its panel covers ~175 px of that, i.e. under a fifth of the width;
/// anything wider than this third of the window is the sheet the answer rejects.
const MAX_PANEL_SHARE_OF_WIDTH: f64 = 1.0 / 3.0;
const MAX_PANEL_SHARE_OF_HEIGHT: f64 = 2.0 / 3.0;

/// Paint the screen's own shot into a file and read the numbers back off the ARTIFACT.
fn paint_the_shot(app: &adw::Application) {
    let root = temp_root("shot");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    // The Cut page WITH material, as the spec's picture draws it (bands and thumbnail rows behind the menu),
    // built through the same door `06-effect-menu` stages on.
    let seeded = naivepost::cut::Cut {
        segs: vec![naivepost::cut::Seg {
            s: 10.0,
            e: 30.0,
            cam: 0,
            ..Default::default()
        }],
        ..Default::default()
    };
    let window = common::cut_page(app, &seeded);
    // §1 sizes the window the way the snapshot does, so the ask the painter reads is a real one.
    window.set_default_size(
        naivepost::shell::WINDOW_MIN_W,
        naivepost::shell::WINDOW_MIN_H,
    );
    window.present();
    // Drive the frame clock until the layout has actually been PAINTED. A widget only gains a render node once GTK
    // has drawn it, and under X with no compositor nothing paints unless something asks for frames — which is what
    // `snapshot::run` gets from its own drain and what a bare `present()` + `settle()` does not. Without this the
    // window answers "widget produced no render node" and the shot cannot be taken at all.
    common::wait_for_stable_layout(&window);
    settle();
    // The staging door first: it pops the dropdown through the widget's own toggle, which is what gives the
    // painter the app's popover to copy. Skipping it would paint a menu no press produced.
    let rows = snapshot_effect_menu::stage(&window);
    let out = root.join("06-effect-menu.png");
    // The snapshot asks for the PNG at the window's size before taking it (`snapshot::run` → `set_shot_size`).
    snapshot_face::set_shot_size(window.width().max(1), window.height().max(1));
    // The window only yields a render node once GTK has actually drawn it, and under X with no compositor that
    // can take more frames than one pass (the same shot succeeded on its own and failed inside the wider
    // `sec_06_effects_2` run). Retry until the paint works, failing by name after the shared deadline rather
    // than guessing a number of loop turns.
    let mut painted = None;
    common::wait_until(
        "the Cut-page window produced a render node to paint the menu shot from",
        || match snapshot_effect_menu::paint_open_over_window(&window, &out) {
            Ok(result) => {
                painted = Some(result);
                true
            }
            Err(_) => false,
        },
    );
    let (entries, panel, at) = painted.expect("the menu paints over the page");
    assert_eq!(
        entries + 1,
        rows.len(),
        "the painted panel holds a different set of rows than the popover does"
    );
    let (file_w, file_h) = snapshot_face::png_size(&out);
    eprintln!("MENU shot file {file_w}x{file_h}, panel {panel:?} at {at:?}");
    // (a) THE FILE IS THE PAGE: the asked PNG carries the window's shape, so the toolbar, ruler, bands and
    // thumbnail rows the spec draws around the menu are in it. A menu-alone shot could not be this size.
    assert_eq!(
        (file_w, file_h),
        (window.width().max(1), window.height().max(1)),
        "the asked file must be the window shot with the menu over it, not the menu box on its own"
    );
    // (b) AT ITS OWN SIZE: compact, never stretched over the frame.
    assert!(
        panel.0 as f64 <= file_w as f64 * MAX_PANEL_SHARE_OF_WIDTH,
        "the menu panel is {} px of a {file_w} px frame — the spec draws a compact popup, not a sheet",
        panel.0
    );
    assert!(
        panel.1 as f64 <= file_h as f64 * MAX_PANEL_SHARE_OF_HEIGHT,
        "the menu panel is {} px of a {file_h} px frame",
        panel.1
    );
    // (c) WHERE IT OPENS: below the app's own ✚ Effect button, inside the window.
    let button =
        ui::find_source_widget(&window, "effect-button").expect("the Cut page drew effect-button");
    let bounds = button
        .compute_bounds(window.upcast_ref::<gtk4::Widget>())
        .expect("the button has a box");
    assert!(
        at.1 >= bounds.y() as i32,
        "the menu hangs at y={} but its button's top edge is at {}",
        at.1,
        bounds.y() as i32
    );
    assert!(
        at.0 >= 0 && at.0 + panel.0 <= file_w,
        "the menu's x={}..{} leaves the {file_w} px frame",
        at.0,
        at.0 + panel.0
    );
    assert!(
        at.1 + panel.1 <= file_h,
        "the menu's y={}..{} leaves the {file_h} px frame",
        at.1,
        at.1 + panel.1
    );
    RAN_SHOT.store(true, std::sync::atomic::Ordering::SeqCst);
    window.close();
    settle();
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_menu_shot_is_the_page_with_the_menu_over_it() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(paint_the_shot);
        app.run_with_args::<&str>(&[]);
    });
    assert!(
        RAN_SHOT.load(std::sync::atomic::Ordering::SeqCst),
        "the menu-over-page shot check never ran"
    );
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_panel_is_placed_under_its_button() {
    // The placement rule itself, as arithmetic a person can check against the picture: GTK points a menu's left
    // edge at the button's CENTRE, and the panel's top sits one gap below the button — both clamped inside the
    // window so a menu near an edge still lands wholly on screen.
    let (x, y) = (600.0_f32, 400.0_f32);
    let (bw, bh) = (120.0_f32, 34.0_f32);
    let (pw, ph) = (180_i32, 160_i32);
    let placed_x = snapshot_effect_menu::panel_x(x, bw, pw, 1280);
    let placed_y = snapshot_effect_menu::panel_y(y, bh, ph, 900);
    assert!(
        (placed_x - (x + bw / 2.0)).abs() < 1.0,
        "the menu's left edge should point at the button's centre: {placed_x} vs {}",
        x + bw / 2.0
    );
    assert!(
        placed_y > y + bh,
        "the menu hangs BELOW its button: y={placed_y}, button bottom {}",
        y + bh
    );
    // Clamped: a button at the right edge cannot push the panel off the frame.
    assert!(
        snapshot_effect_menu::panel_x(1270.0, 40.0, pw, 1280) + pw as f32 <= 1280.0,
        "a menu by the right edge must stay inside the window"
    );
    assert!(
        snapshot_effect_menu::panel_y(895.0, 34.0, ph, 900) + ph as f32 <= 900.0,
        "a menu by the bottom edge must stay inside the window"
    );
}
