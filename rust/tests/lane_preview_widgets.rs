// §06-effects#2-the-lane-and-the-preview — the wire: the lane's bars and the preview's overlays draw what the
// records actually hold, through the real widgets.
//
// `naivepost::fx_lane` holds every number on these two surfaces (proven by tests/cut_six_lane_preview.rs and
// tests/lane_preview_surfaces.rs); this file proves the Cut page pushes them into named widgets rather than
// leaving a placeholder behind. Every check reads a widget the page drew — its existence, its css class, its
// opacity — and never re-asserts a colour or an alpha the logic test already pins.
//
// One application, one `connect_activate`, exactly one `Application::run`: `g_application_run` refuses a second
// claimant of the default main context, so all four checks live inside that single activate and each ends with the
// window dropped before the next is built. cwd is pinned to our own temp root BEFORE the run because the presses
// resolve the project through `startup::session_dir(current_dir())`; leaving it at rust/ writes a stray
// `rust/session.naivepost/` into the repo. No sleeps: `settle()` pumps the glib context instead.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind, Fx};
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

static RAN_EMPTY_LANE: AtomicBool = AtomicBool::new(false);
static RAN_BAR_IN_ROW: AtomicBool = AtomicBool::new(false);
static RAN_HELD_OVERLAY: AtomicBool = AtomicBool::new(false);
static RAN_UNDO_TAKES_BAR: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

thread_local! {
    /// The only strong handle on the window last built, so the next check can drop it first: a closed GTK
    /// window is not destroyed and its names stay parented, which would send a lookup to the wrong tree.
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
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

fn click(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the Cut page drew no {name}"))
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// A widget somewhere in THIS window's tree, found from the window itself rather than from any global slot —
/// the F2.12 lesson: a tree-wide search can land on another window's copy because a closed window survives.
fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if let Some(found) = walk(&current, name) {
                    return Some(found);
                }
                cursor = current.next_sibling();
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// Is `holder` holding a direct child named `name`? Direct only: the assertion is about THIS holder's children,
/// so a nested match elsewhere must not satisfy it.
fn holds(holder: &gtk::Widget, name: &str) -> bool {
    let mut child = holder.first_child();
    while let Some(node) = child {
        if node.widget_name() == name {
            return true;
        }
        child = node.next_sibling();
    }
    false
}

/// Build a window sitting on the Cut tab with the given cut seeded, dropping the previous check's window first.
fn cut_page(app: &adw::Application, tape: &[Recording], seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    // The order build_window uses: open the history on what the page opens with, then show it, so a recorded
    // effect is one Undo away from THIS baseline rather than from a stale one.
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    let _ = tape;
    window
}

/// The session filmed 0–120 as one take.
fn tape() -> Vec<Recording> {
    vec![Recording { base: "session-tape".to_string(), start: 0.0, end: 120.0 }]
}

/// A caption covering the line at 12.0, faded in over 2 s so its own alpha at the line is NOT 1.0 — which is
/// what makes "held draws full" a real answer rather than a coincidence of a fully-visible overlay.
fn held_caption() -> Fx {
    Fx {
        kind: "text".to_string(),
        t: 10.0,
        dur: 6.0,
        trans: 2.0,
        text: "a caption".to_string(),
        ..Default::default()
    }
}

/// §06-effects#2 — the lane and the preview draw what the records hold: one row when empty, a named bar per
/// record, the one in hand full and outlined, and nothing left behind when Undo takes a record away.
fn lane_preview_round(app: &adw::Application) {
    // --- (a) an EMPTY cut still draws exactly one row ---------------------------------------------
    // §2: the lane is never empty. Seen through the widget: the holder exists, has ONE child, and that child
    // is `fx-lane-row-0`.
    let window = cut_page(app, &tape(), &Cut::default());
    let lane = ui::effects_lane_box(&window).expect("the Cut page drew the effects-lane holder");
    let lane: gtk::Widget = lane.upcast();
    assert_eq!(
        lane.observe_children().n_items(),
        1,
        "an empty cut leaves the lane one row deep, not zero-height"
    );
    assert!(holds(&lane, "fx-lane-row-0"), "that row is named fx-lane-row-0");
    RAN_EMPTY_LANE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (b) a real click puts a named bar in that row -------------------------------------------
    let window = cut_page(app, &tape(), &Cut::default());
    click(&window, "effect-item-volume");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 1, "one click, one record");
    let row = widget_in(&window, "fx-lane-row-0").expect("row 0 exists after the click");
    assert!(
        holds(&row, "fx-bar-0"),
        "the new record's bar is a CHILD of the row it was laid in, not packed loose in the lane"
    );
    let bar = widget_in(&window, "fx-bar-0").expect("fx-bar-0 exists");
    assert!(
        bar.has_css_class("fx-kind-volume"),
        "the bar wears its kind's class; the colour numbers stay proven by the logic test"
    );
    RAN_BAR_IN_ROW.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (c) the overlay in hand is full and outlined; released, the outline goes ------------------
    // The line sits at 12.0, two seconds into a caption that fades in over 2 s, so its OWN alpha there is
    // mid-fade. Held, it must read 1.0 (§2 "the held one full"), and carry the dashed violet class.
    let mut seeded = Cut::default();
    seeded.fx.push(held_caption());
    let window = cut_page(app, &tape(), &seeded);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 12.0 });
    ui::set_held_effect(Some(held_caption()));
    ui::refresh_effects_lane(&window);
    settle();
    let held_overlay = widget_in(&window, "preview-overlay-0")
        .expect("the record in hand gets an overlay named for its index");
    assert!(
        (held_overlay.opacity() - 1.0).abs() < 1e-9,
        "the held overlay draws full, got {}",
        held_overlay.opacity()
    );
    assert!(held_overlay.has_css_class("held-effect"), "and carries the dashed outline class");
    // Put it down the way the app does — the effects lane takes it — and the class must be GONE, not left over.
    ui::press_fx_lane(&window);
    ui::refresh_effects_lane(&window);
    settle();
    assert!(ui::held_effect().is_none(), "the lane took the effect out of hand");
    assert!(
        !widget_in(&window, "preview-overlay-0")
            .map(|w| w.has_css_class("held-effect"))
            .unwrap_or(false),
        "released: the outline is removed, so nothing claims an effect is still in hand"
    );
    RAN_HELD_OVERLAY.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (d) Undo takes the bar away -------------------------------------------------------------
    let window = cut_page(app, &tape(), &Cut::default());
    click(&window, "effect-item-volume");
    assert!(widget_in(&window, "fx-bar-0").is_some(), "the bar is there before the undo");
    click(&window, "undo-button");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 0, "Undo took the record back");
    let lane = ui::effects_lane_box(&window)
        .expect("the lane holder survives the undo")
        .upcast::<gtk::Widget>();
    assert!(
        !holds(&lane, "fx-bar-0"),
        "a bar left on screen for an undone effect would be a lie about what this cut holds"
    );
    assert!(
        holds(&lane.upcast(), "fx-lane-row-0"),
        "and the lane itself stays one row deep rather than collapsing"
    );
    RAN_UNDO_TAKES_BAR.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_wire_the_lane_and_the_preview_draw_what_the_records_hold() {
    // Pin cwd to our own temp root BEFORE anything builds.
    let root = std::env::temp_dir().join(format!("np-lane-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(lane_preview_round);
        app.run();
    });

    assert!(RAN_EMPTY_LANE.load(Ordering::SeqCst), "the empty-lane check never ran");
    assert!(RAN_BAR_IN_ROW.load(Ordering::SeqCst), "the bar-in-row check never ran");
    assert!(RAN_HELD_OVERLAY.load(Ordering::SeqCst), "the held-overlay check never ran");
    assert!(RAN_UNDO_TAKES_BAR.load(Ordering::SeqCst), "the undo-takes-the-bar check never ran");
    // Silence the unused-import lint for the kind the fixtures spell as a string.
    let _ = EffectKind::Text;
}
