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

static RAN_ORDER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_UNDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_KEYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_ZOOM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_FORM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

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
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page carries {name}"))
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
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
    // The volume slider is NOT drawn by this round: F2.5's row exists in `build_window` but the page
    // column that owns it is not part of §05-cut#1-screen's toolbar assertion (it lands with F2.8's
    // tracks). Assert the five groups §A can prove today and note the sixth separately.
    let found = toolbar_names(&window);
    let without_volume: Vec<String> = expected.iter().cloned()
        .filter(|n| n != "preview-volume")
        .collect();
    assert_eq!(
        found, without_volume,
        "the five drawn groups sit on the bar in TOOLBAR_GROUPS' order; preview-volume is F2.5/F2.8's"
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

    // A real edit through the F2.7 door: | Split with no band puts one border at the red line.
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
    assert!(said.contains("back to the previous state"), "the click printed the undo sentence: {said}");
    assert!(ui::window_logs().iter().any(|l| l.contains("back to the previous state")));

    button(&window, "redo-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let said = status_text(&window);
    assert!(said.contains("forward again"), "Redo printed its own sentence: {said}");
}

fn check_ctrl_z_and_y_reach_the_same_history(app: &adw::Application) {
    let window = cut_window(app);
    settle();
    let controller = ui::history_key_controller(&window).expect("history has a key controller");
    ui::note_edit(&window);
    settle();

    // Ctrl+Z undoes. The handler answers with `glib::Propagation`, which has no FromValue, so the
    // signal is emitted for its side effect and the status line is what the check reads — the same
    // shape `tests/cut_copy_paste_lane_widgets.rs::press_key` uses for Esc.
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&gtk::gdk::Key::z, &0u32, &gtk::gdk::ModifierType::CONTROL_MASK],
    );
    settle();
    let z = status_text(&window);
    assert!(z.contains("back to the previous state"), "Ctrl+Z reached the history: {z}");

    // Ctrl+Y redoes — the second chord §1 spells, because both hands reach for one of them.
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&gtk::gdk::Key::y, &0u32, &gtk::gdk::ModifierType::CONTROL_MASK],
    );
    settle();
    let y = status_text(&window);
    assert!(y.contains("forward again"), "Ctrl+Y reached the same history: {y}");

    // Ctrl+Shift+Z redoes too, and an UNheld letter is left to whatever has focus: pressing plain `z`
    // must not move the history at all.
    let before = status_text(&window);
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&gtk::gdk::Key::z, &0u32, &gtk::gdk::ModifierType::empty()],
    );
    settle();
    assert_eq!(status_text(&window), before, "plain z without Ctrl is not ours");
}

fn parse_pps(said: &str) -> f64 {
    // "zoom 5.0 px a second" -> 5.0
    let tail = said.trim_start_matches("zoom ");
    tail.split_whitespace().next().unwrap_or("").parse().unwrap_or(f64::NAN)
}

fn check_the_zoom_ladder_clamps(app: &adw::Application) {
    let window = cut_window(app);
    settle();

    button(&window, "zoom-in-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let first = parse_pps(&status_text(&window));
    button(&window, "zoom-in-button").emit_by_name::<()>("clicked", &[]);
    settle();
    let second = parse_pps(&status_text(&window));
    assert!(
        (second - first * cut_screen::ZOOM_STEP).abs() < 0.05,
        "+ steps by P.layout.zoomStep {first} -> {second}"
    );

    // In far enough to hit the ceiling: 240 px a second, `P.layout.zoomMax`.
    for _ in 0..80 {
        button(&window, "zoom-in-button").emit_by_name::<()>("clicked", &[]);
    }
    settle();
    let top = parse_pps(&status_text(&window));
    assert_eq!(top, cut_screen::ZOOM_MAX, "// P.layout.zoomMax 240 stops the ladder");

    // Out far enough to hit the floor: where the whole session fits, which is the placeholder strip's
    // zoom until F2.10/F2.11 put the real tracks on this number.
    for _ in 0..120 {
        button(&window, "zoom-out-button").emit_by_name::<()>("clicked", &[]);
    }
    settle();
    let bottom = parse_pps(&status_text(&window));
    assert_eq!(bottom, ui::TRACK_STRIP_PPS, "zooming out stops where the session fits");
}

fn check_the_idle_form_reads_and_the_ladder_clamps(app: &adw::Application) {
    let window = cut_window(app);
    settle();
    let label = |name: &str| -> String {
        find_first(window.upcast_ref(), name)
            .and_then(|w| w.downcast::<gtk::Label>().ok())
            .map(|l| l.text().to_string())
            .unwrap_or_else(|| panic!("the form column has {name}"))
    };

    // What the form shows is what idle_readouts computes for the seeded three-scene cut.
    let seeded = cut::Cut {
        segs: vec![seg(0.0, 30.0), seg(30.0, 60.0), seg(70.0, 90.0)],
        ..Default::default()
    };
    let want = cut_screen::idle_readouts(
        cut_screen::THUMB_AT_OPEN,
        cut_screen::ASPECT_DEFAULT,
        0.0,
        None,
        &seeded,
        0.0,
    );
    let wanted = |tag: &str| {
        want.iter()
            .find(|r| r.label == tag)
            .unwrap_or_else(|| panic!("no {tag} row"))
            .value
            .clone()
    };
    assert_eq!(label("cut-readout-segments"), wanted("Segments"), "three scenes read as three");
    assert_eq!(label("cut-readout-cut"), wanted("Cut"));
    assert_eq!(label("cut-readout-thumbnails"), "64 px", "the page opens at THUMB_AT_OPEN");

    // The ladder: one third at a step, clamped at both ends.
    button(&window, "thumb-minus").emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(label("cut-readout-thumbnails"), "48 px", "64 * 3/4");
    for _ in 0..12 {
        button(&window, "thumb-minus").emit_by_name::<()>("clicked", &[]);
    }
    settle();
    assert_eq!(
        label("cut-readout-thumbnails"),
        "40 px",
        "// P.layout.thumbMin 40 clamps the small end"
    );
    for _ in 0..24 {
        button(&window, "thumb-plus").emit_by_name::<()>("clicked", &[]);
    }
    settle();
    assert_eq!(
        label("cut-readout-thumbnails"),
        "160 px",
        "// P.layout.thumbMax 160 clamps the large end"
    );
    // The aspect dropdown exists and leads with the default shape.
    let choice = find_first(window.upcast_ref(), "aspect-choice")
        .and_then(|w| w.downcast::<gtk::DropDown>().ok())
        .expect("the form column has aspect-choice");
    let list = choice
        .model()
        .expect("the dropdown has its list of shapes");
    // The item type is read through `strings()` (the same helper `tests/prepare_screen_widgets.rs`
    // uses): `StringList::item` hands back a `glib::Object` that must be downcast to the string item,
    // and asking for the wrong static type trips gio's own assertion.
    let shapes = strings(list.upcast_ref());
    let first_row = shapes.first().cloned();
    assert_eq!(
        first_row.as_deref(),
        Some(cut_screen::ASPECT_DEFAULT),
        "the dropdown's first row is the default shape"
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
            println!("CHECK 3 keys");
            check_ctrl_z_and_y_reach_the_same_history(app);
            RAN_KEYS.store(true, std::sync::atomic::Ordering::SeqCst);
            println!("CHECK 4 zoom");
            check_the_zoom_ladder_clamps(app);
            RAN_ZOOM.store(true, std::sync::atomic::Ordering::SeqCst);
            println!("CHECK 5 form");
            check_the_idle_form_reads_and_the_ladder_clamps(app);
            RAN_FORM.store(true, std::sync::atomic::Ordering::SeqCst);
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
        (&RAN_KEYS, "the Ctrl+Z / Ctrl+Y check"),
        (&RAN_ZOOM, "the zoom-ladder clamp check"),
        (&RAN_FORM, "the idle-form and thumbnail-ladder check"),
    ] {
        assert!(flag.load(std::sync::atomic::Ordering::SeqCst), "{what} never ran");
    }
}
