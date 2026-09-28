// §06-effects#1-record / F3.1 S1 — the wire: a click on the ✚ Effect dropdown reaches each kind's rule through
// the real buttons.
//
// `naivepost::fx_record` holds the record's rules (proven by tests/effect_record_creation.rs); this file proves
// the Cut page's six named dropdown rows reach them. Five kinds record straight through the shared closure; ⊕ Zoom
// does NOT -- §F3.1 S1 makes it ARM a drag instead, adding nothing to the cut until a box is drawn (see
// tests/zoom_drag_widgets.rs for that placement). Every check fires a widget the way GTK does
// (`emit_by_name("clicked")`) and asserts the SAME state the logic test asserts — the cut's `fx` list, the kind
// recorded, the whole-bed reading, the exact status sentence — never a painted pixel.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind};
use naivepost::fx_record as rec;
use naivepost::fx_volume as vol;
use naivepost::fx_zoom as zoom;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_VOLUME_ROW: AtomicBool = AtomicBool::new(false);
static RAN_ZOOM_ROW: AtomicBool = AtomicBool::new(false);
static RAN_UNDO: AtomicBool = AtomicBool::new(false);

/// Let the main context run what the widget emissions queued.

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

/// Build a window sitting on the Cut tab with an empty cut, dropping the previous check's window first.
fn cut_page(app: &adw::Application, tape: &[Recording], seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
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

    // --- S1: the volume row OPENS THE FORM; it records nothing until Apply ---------------------
    // §F3.6 supersedes what this block used to assert (a record appearing straight out of the dropdown,
    // `NEW_EFFECT_SECONDS` wide, printed with `rec::recorded_status`). A volume is placed over seconds and
    // its loudness is an answer someone has to give, so the press only opens "Volume a – b": the record
    // arrives at Apply, which is what the S3 undo below then takes back. The form's own fields, defaults
    // and refusals are proven in tests/volume_form_widgets.rs.
    //
    // A placed line first: with none ever placed and no band marked there is nothing for a volume to work
    // on, and §F3.6's refusal ("volume needs seconds to work on") is the right answer -- that branch is
    // checked in tests/volume_form_widgets.rs, not here.
    ui::note_place(true);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 81.0 });
    click(&window, "effect-item-volume");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "the press adds no record: until the percent is given there is no loudness to place"
    );
    let opened = ui::volume_form_open().expect("the volume row opened the form");
    assert_eq!(opened.dur, vol::LINE_SECONDS, "it starts two seconds wide, off the line");
    assert_eq!(
        opened.percent,
        vol::DEFAULT_GAIN * 100.0,
        "pre-filled at the default gain, in the field's own unit"
    );
    assert!(
        status_text(&window).starts_with(&vol::form_title(opened.t, opened.dur)),
        "and the status names the seconds the form is about, got {}",
        status_text(&window)
    );
    RAN_VOLUME_ROW.store(true, Ordering::SeqCst);

    // --- S2: ⊕ Zoom ARMS a drag, it does not record ------------------------------------------
    // §F3.1 S1: the box someone draws is the effect, so this click must add NOTHING to the cut. The
    // old version of this block asserted a second `zoom` record appeared here, which is exactly what
    // arming forbids; the placement itself is proven in tests/zoom_drag_widgets.rs.
    //
    // A line has to be placed first: with none ever placed, S1's own refusal is the right answer and that
    // branch is checked in tests/zoom_drag_widgets.rs. (S1 already placed one at 81 s; set again so this
    // block does not depend on what came before it.)
    ui::note_place(true);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 81.0 });
    click(&window, "effect-item-zoom");
    let cut_ = ui::review_cut_of(&window);
    assert_eq!(
        cut_.fx.len(),
        0,
        "arming added no record -- the zoom's box has not been drawn yet, and §F3.6 moved the volume to \
         Apply as well, so nothing is on the cut here"
    );
    assert!(ui::zoom_armed(), "the zoom entry left the page waiting for a box");
    let said = status_text(&window);
    assert!(
        said.starts_with(zoom::ARM_WORDS),
        "the status line opens with the module's own arm words, got {said}"
    );
    assert!(
        said.contains("It starts at the red line"),
        "and its tail names where the zoom will go and for how long, got {said}"
    );
    // Camera layer goes down while armed: whole source visible (§F3.1 S1).
    assert!(zoom::whole_source_shown(true), "while armed the framing is out of the way");
    RAN_ZOOM_ROW.store(true, Ordering::SeqCst);

    // --- S3: the hand-added record came in as an edit, so ↶ takes it back ----------------------
    // The cut reaches this point empty -- neither the volume press nor the zoom arm puts anything on it --
    // so the record that gets undone is put there deliberately, by pressing the form's own Apply rather
    // than by poking state. That is the record `record_edit` pushed, which is exactly what F2.13's ↶ covers.
    // The line from S1 is still placed, so the press finds seconds to work on again.
    click(&window, "effect-item-volume");
    click(&window, "volume-apply-button");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        1,
        "Apply, not the press, is what put one volume on the cut"
    );
    click(&window, "undo-button");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        0,
        "one Undo took the hand-added effect back -- a hand-added effect is never an unwinding surprise"
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
