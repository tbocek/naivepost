// §06-effects#2-the-lane-and-the-preview S1 — the wire: the ✚ Effect dropdown really opens, and the rows the
// spec's picture shows are the rows the app drew.
//
// `spec/img/06-effect-menu.png` shows the popover popped, with ⊕ Zoom, ❝ Text, ▨ SVG, ⏩ Speed, 🔊 Volume and
// 🏷 Label in that order. `tests/suite/effect_menu_record_widgets.rs` proves a click on each row reaches its rule;
// nothing proved the MENU itself is on screen — the snapshot of this screen was byte-identical to a bare Cut
// page because nothing ever popped it. This file pops it through the toolkit's own toggle and reads the rows
// back off the popover's own children: their order, their labels and whether they are actually mapped.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut_screen::EFFECT_ITEMS;
use naivepost::fx_lane;
use naivepost::ui;

use crate::common;
use common::settle;

static RAN_MENU: AtomicBool = AtomicBool::new(false);

/// The dropdown's trigger is a `MenuButton`, not a plain Button, so it needs its own lookup (the same walk
/// `tests/suite/effect_menu_record_widgets.rs` uses — `ui::find_widget_by_name` is crate-private).
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

// The shared builder, under the name these checks already call: `cut_page_empty` is that same page build
// (the other one also repaints the lane, which this check does not need), and what matters here — the tab
// order and the window pinning — it keeps.
use common::cut_page_empty as cut_page;

fn menu_shows_the_rows(app: &adw::Application) {
    let window = cut_page(app);
    let button = menu_button(&window, "effect-button");

    // Closed to begin with: a popover that was already open would make "the toggle opened it" unfalsifiable.
    assert!(!button.is_active(), "the Effect dropdown starts closed");
    // The toolkit's own toggle -- what a click on the button produces, not a peek at the child box.
    //
    // GTK 4.22 (what this container links) does not open a `MenuButton` synchronously: `set_active(true)` queues
    // the popup, which is mapped on a LATER main-context iteration, and with no pointer grab in play -- headless
    // never has one -- its seatbelt/autohide path can hand it back a few iterations after that. So there are TWO
    // races here and one `settle()` loses both: reading too early sees `is_active()`/`popover.is_visible()` still
    // false, and reading just after the map can see them true a moment before GTK closes the popup again.
    // Measured over 20 runs of this binary alone, ~15 % hit one of the two asserts below -- and a panic inside
    // GTK's `activate` is an exit-134 abort, not a test failure, which is how it surfaced as a whole-file failure
    // in the suite. So each side is waited for as a CONDITION with a deadline naming what never happened
    // (`common::wait_until`: pumps the context, no fixed sleep, fails after `WAIT_LIMIT`). Nothing forces a
    // widget open -- a build that cannot keep this dropdown open now fails by name instead of racing.
    button.set_active(true);
    common::wait_until(
        "the Effect dropdown opened when the toolkit toggled it",
        || button.is_active() && button.popover().is_some_and(|popover| popover.is_visible()),
    );
    // GTK hands an ungrabbed popup back a few iterations after it maps (headless there is never a pointer grab
    // to hold it), and pumping the context lets that land -- so a second wait would be waiting for something that
    // legitimately stops being true. What the check actually needs is the state a person sees WHILE the dropdown
    // is open, so the rows are read now, inside the window the wait above just proved open, and only then closed.
    assert!(
        button.is_active(),
        "the Effect dropdown opened when the toolkit toggled it"
    );
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
fn sec_06_effects_2_the_lane_and_the_preview_s2_the_menu_shot_takes_the_pictures_own_name() {
    // `spec/img/06-effect-menu.png` is a picture of the POPOVER, not of the page behind it, so this screen's
    // `<name>.png` must be the popover too and the staged page has to go beside it (`snapshot::run`'s
    // `menu_subject` branch). The door is asked by screen name, so the check is exactly that: the one screen whose
    // spec picture is the menu is the one whose PNG is the menu -- every other screen keeps the page at its own
    // name. No window needed: this is the wiring between the screen list and the frame-taking step.
    assert!(
        naivepost::snapshot_effect_menu::is_menu_subject("06-effect-menu"),
        "the screen whose spec picture is the popover must paint the popover under its own name"
    );
    for other in ["06-lane", "05-cut"] {
        assert!(
            !naivepost::snapshot_effect_menu::is_menu_subject(other),
            "{other}'s picture is a page, so its `<name>.png` must stay the page"
        );
    }
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
        app.run_with_args::<&str>(&[]);
    });

    assert!(RAN_MENU.load(Ordering::SeqCst), "the open-the-menu check never ran");
}
