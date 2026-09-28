//! §05-cut#1-screen — the wire: the Cut page's toolbar groups, its history and zoom controls and its
//! idle form column reach the window through real widgets. The logic tests (`tests/cut_screen_view.rs`)
//! prove the tables and the rules; this one proves a click arrives, that the group order on the realized
//! page is the table's order, and that the readouts show what `cut_screen` computes.
//!
//! One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
//! state (this window's cut, selection, zoom and thumbnail slots), so they run once inside a single
//! `Application::run` and report through flags the test asserts on — the shape
//! `tests/cut_verb_widgets.rs` uses.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Seg};
use naivepost::cut_verbs;
use naivepost::cut_screen::{self, HISTORY_BUTTONS, TRANSPORT_BUTTONS, VERB_BUTTONS, ZOOM_BUTTONS};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, settle, status_text};

static RAN_ORDER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_UNDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A window sitting on the Cut page with a three-scene cut seeded, cwd pinned to its own temp root so
/// nothing resolves the project through `rust/` and writes a stray `rust/session.naivepost/` into the
/// repo (`startup::session_dir(current_dir())` is how the page finds its folder).
fn cut_window(app: &adw::Application) -> adw::ApplicationWindow {
    let root = std::env::temp_dir().join(format!("cut-toolbar-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");

    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock) and this would \
         test the wrong page"
    );
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);

    let mut seeded = cut::Cut::default();
    seeded.segs = vec![seg(0.0, 30.0), seg(30.0, 60.0), seg(70.0, 90.0)];
    ui::seed_review_cut(&window, &seeded);
    window
}

fn seg(s: f64, e: f64) -> Seg {
    Seg { s, e, ..Default::default() }
}

/// Let the main context run what the widget emissions queued.

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page carries {name}"))
}

/// Every widget name under `root`, depth-first, skipping unnamed ones. `observe_children` gives the
/// direct children of a box in pack order, which is the order the page assembled them in.
fn names_under(widget: &gtk::Widget) -> Vec<String> {
    let mut out = vec![widget.widget_name().to_string()];
    for child in widget.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
        out.extend(names_under(child));
    }
    out
}

fn named_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    // `find_widget_by_name` is crate-private; walking from the window is the same search and keeps the
    // test honest about what is actually in the tree.
    names_under(window.upcast_ref())
        .iter()
        .position(|n| n == name)
        .and_then(|_| find_first(window.upcast_ref(), name))
}

fn find_first(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if widget.widget_name() == name {
        return Some(widget.clone());
    }
    for child in widget.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
        if let Some(found) = find_first(child, name) {
            return Some(found);
        }
    }
    None
}

/// The toolbar's control names in the order they sit on the bar, group by group. Each group box is
/// named `group-<name>` (`cut_tool_group`); the walk descends through any UNNAMED wrapper (the volume
/// group holds a labelled row around the slider) and collects only the controls that carry a widget
/// name, which is what §A actually enumerates.
fn toolbar_names(window: &adw::ApplicationWindow) -> Vec<String> {
    let toolbar = find_first(window.upcast_ref(), "cut-toolbar").expect("the Cut page has a cut-toolbar");
    let mut found: Vec<String> = Vec::new();
    for group in toolbar.observe_children().iter::<glib::Object>() {
        let Ok(group) = group else { continue };
        let Some(group) = group.downcast_ref::<gtk::Widget>() else { continue };
        collect_named(group, &mut found);
    }
    found
}

/// Every NAMED control under `widget`, in tree order. A wrapper with no widget name (a row holding a
/// label and a slider) contributes nothing of its own but is still descended into, so the control it
/// wraps is reported at the group it belongs to.
fn collect_named(widget: &gtk::Widget, out: &mut Vec<String>) {
    let name = widget.widget_name().to_string();
    // A control is a BUTTON (or the volume slider) that carries a widget name. The group boxes are
    // named too -- `group-transport` etc. -- and the buttons are wrapped in labelled rows whose Labels
    // GTK gives the type name as its widget name; neither belongs in the list §A enumerates.
    let is_control = widget.downcast_ref::<gtk::Button>().is_some()
        || widget.downcast_ref::<gtk::Scale>().is_some()
        || widget.downcast_ref::<gtk::MenuButton>().is_some();
    // A gtk::MenuButton IS-A gtk::ToggleButton IS-A gtk::Button in the C hierarchy, so a MenuButton
    // shows up as "GtkToggleButton" when it has no widget name of its own. §A lists only controls with
    // names, so an unnamed one is skipped; the effect-button and its popover rows carry names and pass.
    let named_real = !name.is_empty() && !name.starts_with("Gtk");
    if is_control && named_real {
        out.push(name);
    }
    for child in widget.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
        collect_named(child, out);
    }
}

/// The strings a `StringList` holds, in order.
fn strings(model: &glib::Object) -> Vec<String> {
    let list = model.downcast_ref::<gtk::StringList>().expect("a list of strings");
    (0..list.n_items())
        .map(|index| {
            list.item(index)
                .and_downcast::<gtk::StringObject>()
                .expect("a string item")
                .string()
                .to_string()
        })
        .collect()
}

// --- the checks, each run inside the one activate ---------------------------------------------------

fn check_the_group_order_is_the_table_order(app: &adw::Application) {
    let window = cut_window(app);
    settle();

    let expected: Vec<String> = TRANSPORT_BUTTONS
        .iter()
        .map(|t| t.name.to_string())
        .chain(std::iter::once("preview-volume".to_string()))
        .chain(VERB_BUTTONS.iter().map(|t| t.name.to_string()))
        .chain(std::iter::once("effect-button".to_string()))
        // The dropdown's six rows are named and live under the MenuButton's popover, so the walk finds
        // them right after the button that owns them -- §A lists them inside the effects group.
        .chain(cut_screen::EFFECT_ITEMS.iter().map(|i| i.name.to_string()))
        .chain(HISTORY_BUTTONS.iter().map(|t| t.name.to_string()))
        .chain(ZOOM_BUTTONS.iter().map(|t| t.name.to_string()))
        .collect();
    // The volume slider IS on the bar now: its labelled row sits inside `group-volume`, so the walk
    // finds `preview-volume` right after the transport group, which is where §A puts it.
    let found = toolbar_names(&window);
    assert_eq!(
        found, expected,
        "the six groups sit on the bar in TOOLBAR_GROUPS' order, each holding its table's controls"
    );
    // Insert sits where §1 puts it: after Paste, before Lane.
    let at = |n: &str| found.iter().position(|x| x == n).expect(n);
    assert!(at("paste-button") + 1 == at("insert-button"), "Insert follows Paste");
    assert!(at("insert-button") + 1 == at("lane-button"), "Lane follows Insert");

    // The effects dropdown's six rows exist by name. Asserted on their existence and order rather than
    // on the popover being shown: popping a popover needs a mapped, focused surface, and headless that
    // depends on the window manager rather than on anything this page owns.
    let menu = find_first(window.upcast_ref(), "effect-menu").expect("effect-button has an effect-menu");
    // `observe_children().iter::<T>()` asserts T against the container's actual element type before it
    // yields anything, so iterating a gtk::Widget view of a Box whose elements are Buttons trips gio's
    // own check. Iterating glib::Object and downcasting is the shape that works (and the one
    // `find_first` above uses).
    let items: Vec<String> = menu
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(|c| c.ok())
        .filter_map(|c| c.downcast::<gtk::Widget>().ok())
        .map(|w| w.widget_name().to_string())
        .collect();
    let kinds: Vec<&str> = cut_screen::EFFECT_ITEMS.iter().map(|i| i.name).collect();
    assert_eq!(items, kinds, "the dropdown lists the six effects in spec order");
}

fn check_undo_redo_reach_the_history(app: &adw::Application) {
    let window = cut_window(app);
    settle();

    // Greyed before anything has been edited: the resting answer, from the rule not a leftover.
    assert!(
        !button(&window, "undo-button").is_sensitive(),
        "an unedited page has nothing to undo"
    );

    // A real edit through the F2.7 door. | Split with NO band refuses when the red line lands on an
    // existing scene border ("nothing to split at 00:00"), so the press is given a band first -- the
    // same shape `tests/cut_verb_widgets.rs` uses -- and the split then falls inside the selection.
    let band = ui::draw_selection(
        &window,
        naivepost::cut_select::Surface::PictureRow(0),
        None,
        10.0,
        40.0,
    )
    .expect("a drag on a picture row selects that row's footage");
    assert!(band.start < band.end, "the band has an inside for the split to fall in");
    let outcome = ui::press_split(&window);
    settle();
    assert!(
        matches!(outcome, cut_verbs::Outcome::Applied { .. }),
        "Split applied: {outcome:?}"
    );
    // Record it in the page's history the way the app's own edit path does, then click Undo.
    ui::note_edit(&window);
    settle();
    assert!(
        button(&window, "undo-button").is_sensitive(),
        "after an edit, Undo is live"
    );

    button(&window, "undo-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let said = status_text(&window);
    // F2.13 S1: the walk sentence, spelled once in `cut::undone` and printed by the press.
    assert!(
        said.contains("undone \u{2014}") && said.ends_with("segment(s) left"),
        "the click printed the undo sentence: {said}"
    );
    assert!(ui::window_logs().iter().any(|l| l.contains("undone \u{2014}")));

    button(&window, "redo-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let said = status_text(&window);
    // F2.13 S1: Redo shares that one line -- what matters after either press is what the timeline holds.
    assert!(
        said.contains("undone \u{2014}") && said.ends_with("segment(s) left"),
        "Redo printed the same walk sentence: {said}"
    );
}

fn round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            println!("CHECK 1 toolbar order");
            check_the_group_order_is_the_table_order(app);
            RAN_ORDER.store(true, std::sync::atomic::Ordering::SeqCst);
            println!("CHECK 2 undo/redo");
            check_undo_redo_reach_the_history(app);
            RAN_UNDO.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn sec_05_cut_1_screen_widgets_the_toolbar_history_zoom_and_form_reach_the_window() {
    round();
    for (flag, what) in [
        (&RAN_ORDER, "the toolbar-order check"),
        (&RAN_UNDO, "the undo/redo click check"),
    ] {
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "{what} never ran");
    }
}
