// §06-effects#2-the-lane-and-the-preview S1 — the wire: the ✚ Effect dropdown really opens, and the rows the
// spec's picture shows are the rows the app drew.
//
// `spec/img/06-effect-menu.png` shows the popover popped, with ⊕ Zoom, ❝ Text, ▨ SVG, ⏩ Speed, 🔊 Volume and
// 🏷 Label in that order. `tests/effect_menu_record_widgets.rs` proves a click on each row reaches its rule;
// nothing proved the MENU itself is on screen — the snapshot of this screen was byte-identical to a bare Cut
// page because nothing ever popped it. This file pops it through the toolkit's own toggle and reads the rows
// back off the popover's own children: their order, their labels and whether they are actually mapped.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::Cut;
use naivepost::cut_screen::EFFECT_ITEMS;
use naivepost::fx_lane;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_MENU: AtomicBool = AtomicBool::new(false);

/// The dropdown's trigger is a `MenuButton`, not a plain Button, so it needs its own lookup (the same walk
/// `tests/effect_menu_record_widgets.rs` uses — `ui::find_widget_by_name` is crate-private).
fn menu_button(window: &adw::ApplicationWindow, name: &str) -> gtk::MenuButton {
    fn walk(node: &gtk::Widget, name: &str, out: &mut Option<gtk::MenuButton>) -> bool {
        if node.widget_name() == name && node.downcast_ref::<gtk::MenuButton>().is_some() {
            out.replace(node.clone().downcast().expect("checked above"));
            return true;
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if walk(&current, name, out) {
                    return true;
                }
                cursor = current.next_sibling();
            }
        }
        false
    }
    let mut found = None;
    walk(window.upcast_ref(), name, &mut found);
    found.expect("the Cut page drew a MenuButton named effect-button")
}

/// A window sitting on the Cut tab, built the way `tests/common::cut_page` builds one (that one also repaints
/// the lane, which this check does not need; this one keeps the tab order and the window pinning).
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

fn menu_shows_the_rows(app: &adw::Application) {
    let window = cut_page(app);
    let button = menu_button(&window, "effect-button");

    // Closed to begin with: a popover that was already open would make "the toggle opened it" unfalsifiable.
    assert!(!button.is_active(), "the Effect dropdown starts closed");
    // The toolkit's own toggle -- what a click on the button produces, not a peek at the child box.
    button.set_active(true);
    settle();
    assert!(button.is_active(), "the Effect dropdown opened when the toolkit toggled it");
    assert!(
        button.popover().is_some_and(|popover| popover.is_visible()),
        "an open dropdown shows its popover"
    );

    let popover = button.popover().expect("the Effect dropdown has a popover");
    let list = popover
        .child()
        .and_then(|child| child.downcast::<gtk::Box>().ok())
        .expect("the Effect popover holds a Box of rows");
    let rows: Vec<gtk::Button> = list
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|row| row.downcast::<gtk::Button>().ok())
        .collect();

    // One row per effect the menu is specced to list, in that order, each labelled as the table spells it.
    let wanted: Vec<(&str, &str)> = EFFECT_ITEMS.iter().map(|item| (item.name, item.label)).collect();
    assert_eq!(
        wanted.len(),
        fx_lane::MENU_ORDER.len(),
        "the dropdown table and `fx_lane::MENU_ORDER` disagree about how many effects there are"
    );
    assert_eq!(rows.len(), wanted.len(), "the popover drew the six effect rows");
    for (index, (name, label)) in wanted.iter().enumerate() {
        let row = &rows[index];
        assert_eq!(
            row.widget_name(),
            *name,
            "row {} of the open menu is not `{name}` -- the order on screen is not the spec's",
            index + 1
        );
        assert_eq!(
            row.label().unwrap_or_default().as_str(),
            *label,
            "row `{name}` does not read as the label the spec's picture gives it"
        );
        assert!(row.is_visible(), "`{name}` is hidden inside an open menu");
    }

    // Mapped, not just visible: a row that never got laid out is not on the screen anyone can see. The popover
    // has its own surface, so the walk asks each widget for itself rather than the window for the lot.
    for (name, _) in wanted.iter() {
        let row = rows.iter().find(|row| row.widget_name() == *name).expect("row found above");
        assert!(row.is_mapped(), "`{name}` is visible but never drawn -- a popover that did not map");
    }

    // The spec's order is `fx_lane::MENU_ORDER`, so the labels the widget tree gives must match the order the
    // module that owns the menu spells -- read back, not restated.
    let spec_order: Vec<String> = rows.iter().map(|row| row.label().unwrap_or_default().to_string()).collect();
    assert_eq!(
        spec_order,
        naivepost::snapshot_effect_menu::labels_in_menu_order(),
        "the open menu's rows are not in fx_lane::MENU_ORDER"
    );

    button.set_active(false);
    settle();
    assert!(!button.is_active(), "and it closes again, so the check proved a toggle both ways");

    RAN_MENU.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s1_the_effect_menu_shows_the_six_rows_in_its_order() {
    // Pin cwd to our own temp root BEFORE anything builds: the page resolves the session through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray folder into the repo.
    let root = std::env::temp_dir().join(format!("np-s06e2menu-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(menu_shows_the_rows);
        app.run();
    });

    assert!(RAN_MENU.load(Ordering::SeqCst), "the open-the-menu check never ran");
}
