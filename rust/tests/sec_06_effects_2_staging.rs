// §06-effects#2-the-lane-and-the-preview S2 — the door the Effect-menu screen stages through.
//
// `just snapshot 06-effect-menu` used to come out byte-identical to a bare Cut page: the screen name existed so
// the toolbar guard would accept it, but nothing popped the popover. This check calls the SAME function the
// snapshot entry point calls (`snapshot::run` → `snapshot_effects::paint` → `snapshot_effect_menu::stage`) and
// asserts the rows it reads back are the app's own, in `fx_lane::MENU_ORDER`'s order -- so a shot that silently
// stopped staging cannot go green. `sec_06_effects_2_effect_menu_widgets.rs` proves the popover is on screen;
// this proves the staging step is what puts it there.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::Cut;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_MENU_STAGE: AtomicBool = AtomicBool::new(false);

/// A window sitting on the Cut tab. Built here rather than through `common::cut_page` because the snapshot
/// staging steps expect a page nobody has seeded a cut into (the `06-text` shot places its own line).
fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, &Cut::default());
    settle();
    window
}

/// S2: the menu staging door returns the app's own rows, in `fx_lane::MENU_ORDER`'s order.
fn staged_menu_lists_the_app_s_rows(app: &adw::Application) {
    let window = cut_page(app);
    let rows = naivepost::snapshot_effect_menu::stage(&window);
    assert_eq!(
        rows.len(),
        fx_lane_len(),
        "the staging step read {} rows from the popover",
        rows.len()
    );
    // Each row is `"<widget name>: <label>"`, so both halves of the proof come off the widget tree.
    let names: Vec<&str> = rows.iter().map(|row| row.split(": ").next().unwrap()).collect();
    let wanted: Vec<&str> = naivepost::cut_screen::EFFECT_ITEMS.iter().map(|item| item.name).collect();
    assert_eq!(names, wanted, "the staged menu is not in the spec's row order");
    let labels: Vec<String> = rows
        .iter()
        .map(|row| row.split(": ").nth(1).unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        labels,
        naivepost::snapshot_effect_menu::labels_in_menu_order(),
        "the staged menu's labels are not the ones fx_lane::MENU_ORDER implies"
    );
    RAN_MENU_STAGE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// How many effects the menu is specced to list, asked of the module that owns the order.
fn fx_lane_len() -> usize {
    naivepost::fx_lane::MENU_ORDER.len()
}

/// S3 lives in its own binary (`sec_06_effects_2_text_staging.rs`): the caption form and its arm are
/// process-wide slots, so it cannot share a window-building scenario with this one.

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_staged_menu_lists_the_app_s_own_rows() {
    // One binary runs one staging scenario at a time: the caption form and the answered-endpoints table are both
    // process-wide, so `06-text` gets its own binary (`sec_06_effects_2_text_staging.rs`). Pin cwd to our own
    // temp root BEFORE anything builds, for the same reason the menu file does.
    let root = std::env::temp_dir().join(format!("np-s06e2stage-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(staged_menu_lists_the_app_s_rows);
        app.run();
    });

    assert!(RAN_MENU_STAGE.load(Ordering::SeqCst), "the menu-staging check never ran");
}
