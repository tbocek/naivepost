// §06-effects#1-record — the wire: a click on the ✚ Effect dropdown records the effect through the real buttons.
//
// `naivepost::fx_record` holds the record's rules (proven by tests/effect_record_creation.rs); this file proves
// the Cut page's six named dropdown rows reach them. Every check fires a widget the way GTK does
// (`emit_by_name("clicked")`) and asserts the SAME state the logic test asserts — the cut's `fx` list, the kind
// recorded, the whole-bed reading, the exact status sentence — never a painted pixel.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind};
use naivepost::fx_record as rec;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

static RAN_VOLUME_ROW: AtomicBool = AtomicBool::new(false);
static RAN_ZOOM_ROW: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);

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

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

/// The dropdown's trigger is a `MenuButton`, not a plain Button, so it needs its own lookup.
fn menu_button(window: &adw::ApplicationWindow, name: &str) -> gtk::MenuButton {
    let holder = std::cell::RefCell::new(None);
    // Walk the window's own tree for the name and downcast to MenuButton; `line_step_button` only
    // reaches plain Buttons.
    fn walk(node: &gtk::Widget, name: &str, out: &std::cell::RefCell<Option<gtk::MenuButton>>) -> bool {
        if node.widget_name() == name {
            if let Ok(found) = node.clone().downcast::<gtk::MenuButton>() {
                out.borrow_mut().replace(found);
                return true;
            }
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
    walk(window.upcast_ref(), name, &holder);
    holder.into_inner().unwrap_or_else(|| panic!("the Cut page drew no MenuButton named {name}"))
}

fn click(window: &adw::ApplicationWindow, name: &str) {
    button(window, name).emit_by_name::<()>("clicked", &[]);
    settle();
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

/// Build a window sitting on the Cut tab with an empty cut, dropping the previous check's window first.
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
    // Open the history on what the page opens with, then show it -- the order build_window uses, so a
    // recorded effect is one Undo away from THIS baseline rather than from a stale one.
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

/// §06-effects#1-record — clicking a dropdown row records that kind, says so, and lands as ONE Undo.
fn fx_menu_round(app: &adw::Application) {
    let window = cut_page(app, &tape(), &Cut::default());

    // The dropdown itself is drawn and labelled (§1 item 17).
    let trigger = menu_button(&window, "effect-button");
    assert_eq!(trigger.label().map(|l| l.to_string()), Some("\u{271a} Effect".to_string()));
    // And its contents resolve by name whether the popover popped or fell back to the flat Box: every
    // one of the six rows is reachable by its own name from this window's tree.
    for row in ["effect-item-zoom", "effect-item-speed", "effect-item-text", "effect-item-svg", "effect-item-volume", "effect-item-label"] {
        assert!(
            !button(&window, row).label().map(|l| l.is_empty()).unwrap_or(true),
            "{row} is drawn with a label"
        );
    }

    // --- S1: the volume row records a volume -----------------------------------------------
    click(&window, "effect-item-volume");
    let cut_ = ui::review_cut_of(&window);
    assert_eq!(cut_.fx.len(), 1, "one click, one record");
    assert_eq!(cut_.fx[0].kind, "volume");
    assert_eq!(cut_.fx[0].dur, rec::NEW_EFFECT_SECONDS, "it starts two seconds wide");
    // Empty lane = the whole bed (§1's `lane (new)`), straight out of the dropdown.
    assert!(rec::rides_whole_bed(&cut_.fx[0]), "a new volume rides the whole bed");
    assert_eq!(
        status_text(&window),
        rec::recorded_status(EffectKind::Volume),
        "the status line prints the module's own sentence"
    );
    RAN_VOLUME_ROW.store(true, Ordering::SeqCst);

    // --- S2: a different row reaches a different kind through the same closure ---------------
    click(&window, "effect-item-zoom");
    let cut_ = ui::review_cut_of(&window);
    assert_eq!(cut_.fx.len(), 2, "the second row added, it did not replace");
    assert_eq!(cut_.fx[1].kind, "zoom");
    // A zoom names the camera row it was framed on; nothing was framed yet, so row 0.
    assert_eq!(rec::camera_row(&cut_.fx[1]), Some(0));
    // ...and a volume has no such answer at all (§1's `cam` row is zoom-only).
    assert_eq!(rec::camera_row(&cut_.fx[0]), None);
    assert_eq!(status_text(&window), rec::recorded_status(EffectKind::Zoom));
    RAN_ZOOM_ROW.store(true, Ordering::SeqCst);

    // --- S3: the pair came in as edits, so ↶ takes them back one at a time ------------------
    click(&window, "undo-button");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        1,
        "one Undo removed the last record, not both"
    );
    click(&window, "undo-button");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "and the second Undo left the page as it opened -- a hand-added effect is never an unwinding surprise"
    );
    RAN_UNDO.store(true, Ordering::SeqCst);

    window.close();
}

#[test]
fn sec_06_effects_1_record_wire_a_real_click_on_an_effect_row_records_it() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-fxmenu-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(fx_menu_round);
        app.run();
    });

    assert!(RAN_VOLUME_ROW.load(Ordering::SeqCst), "the volume-row check never ran");
    assert!(RAN_ZOOM_ROW.load(Ordering::SeqCst), "the zoom-row check never ran");
    assert!(RAN_UNDO.load(Ordering::SeqCst), "the undo-after-recording check never ran");
}
