// F2.13 (Undo, Redo, Revert, Clear) — THE WIRE. The rules and the four sentences live in
// `naivepost::cut` and are proven by tests/history_flow_rules.rs; this file proves the Cut page's real
// buttons reach them. Every check fires a named widget the way GTK does (`emit_by_name("clicked")`) or a
// key through the window's own controller, then asserts the SAME state the logic test asserts — segment
// counts, lane/shift/row survival, the exact message string — never a painted pixel.
//
// The point of the file: no check calls `ui::note_edit`. Each one makes a REAL edit (＋ Add, ✗ Clear) and
// expects Undo/Redo/Revert to light up because the app recorded it, which is what was missing before.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared
// state (this window's cut slot, its history and the status line), so they run once in sequence and
// report through flags the test asserts on at the end.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{self, Cut, Fx, Lane, Seg};
use naivepost::cut_select::Surface;
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_ADD_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_REDO: AtomicBool = AtomicBool::new(false);
static RAN_REVERT: AtomicBool = AtomicBool::new(false);
static RAN_CLEAR: AtomicBool = AtomicBool::new(false);
static RAN_KEYS: AtomicBool = AtomicBool::new(false);

/// A footage scene on camera 0.
fn clip(s: f64, e: f64) -> Seg {
    Seg { s, e, cam: 0, ..Default::default() }
}

/// Build a window sitting on the Cut tab with the given tape and cut, dropping the previous check's
/// window first.
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
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::set_session_recordings(tape);
    // Re-open the history on THIS cut before publishing it. `cut_history` opens lazily against whatever
    // is newest at the time, so seeding first would have opened the base on the previous check's cut and
    // left Revert live on a page that has changed nothing. This is the same order `build_window` uses:
    // open the history on what the page opens with, then show it (§2).
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    // Nothing left over from the previous check that would answer S1/S2 where this one wants "no".
    ui::set_held_clip(None);
    ui::clear_selection(&window);
    settle();
    window
}

/// The session filmed 0–120 as one take.
fn tape() -> Vec<Recording> {
    vec![Recording { base: "session-tape".to_string(), start: 0.0, end: 120.0 }]
}

/// Let the main context run what the widget emissions queued.

fn button(window: &adw::ApplicationWindow, name: &str) -> gtk::Button {
    ui::line_step_button(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

fn click(window: &adw::ApplicationWindow, name: &str) {
    button(window, name).emit_by_name::<()>("clicked", &[]);
    settle();
}

/// Draw a band the way a drag does, so ＋ Add has a region to keep.
fn seed_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(window, Surface::PictureRow(0), None, from, to);
    assert!(band.is_some(), "a picture-row drag must produce a footage band");
    settle();
}

/// S1: a real ＋ Add lights Undo with no help from the test, and a real ↶ says what is left.
fn f2_13_s1_a_real_add_makes_undo_live_and_a_real_undo_says_what_is_left(app: &adw::Application) {
    let seeded = Cut { segs: vec![clip(0.0, 30.0)], ..Default::default() };
    let window = cut_page(app, &tape(), &seeded);
    // At rest nothing is undoable, redoable, revertible or clearable.
    for name in ["undo-button", "redo-button", "revert-button", "clear-cut-button"] {
        assert!(
            !button(&window, name).is_sensitive(),
            "{name} must be greyed on an untouched page"
        );
    }

    // A real edit through the real verb button.
    seed_band(&window, 40.0, 50.0);
    click(&window, "add-button");
    let after_add = ui::review_cut_segs(&window);
    assert!(
        after_add.len() > 1,
        "＋ Add kept the band as a scene: {:?}",
        after_add.len()
    );
    assert!(
        button(&window, "undo-button").is_sensitive(),
        "Undo lit because the APP recorded the edit -- the test pushed nothing"
    );

    // And a real Undo walks back to the seeded state, printing the S1 sentence.
    click(&window, "undo-button");
    let back = ui::review_cut_segs(&window);
    assert_eq!(back.len(), 1, "back to the one scene the page opened with");
    assert_eq!(
        status_text(&window),
        cut::undone(back.len()),
        "the status is `cut::undone`'s own string"
    );
    assert!(status_text(&window).ends_with("segment(s) left"), "{}", status_text(&window));
    window.close();
}

/// S1: ↷ Redo puts the added scene back and shares the same walk sentence.
fn f2_13_s1_redo_puts_it_back(app: &adw::Application) {
    let seeded = Cut { segs: vec![clip(0.0, 30.0)], ..Default::default() };
    let window = cut_page(app, &tape(), &seeded);
    seed_band(&window, 40.0, 50.0);
    click(&window, "add-button");
    let added = ui::review_cut_segs_count(&window);
    click(&window, "undo-button");
    assert!(button(&window, "redo-button").is_sensitive(), "Redo is live after an Undo");

    click(&window, "redo-button");
    let forward = ui::review_cut_segs_count(&window);
    assert_eq!(forward, added, "Redo put the added scene back");
    assert_eq!(status_text(&window), cut::undone(forward), "one sentence for both directions");
    window.close();
}

/// S2: Revert refuses while nothing changed since the base, then goes back to the base, then refuses again.
fn f2_13_s2_revert_refuses_then_returns_to_the_base(app: &adw::Application) {
    let seeded = Cut { segs: vec![clip(0.0, 30.0)], ..Default::default() };
    let window = cut_page(app, &tape(), &seeded);
    // Greyed, and a press that got through anyway answers the refusal without writing.
    assert!(!button(&window, "revert-button").is_sensitive(), "nothing changed since the base");
    assert_eq!(ui::press_revert(&window), cut::NOTHING_TO_REVERT, "S2's refusal, verbatim");
    assert_eq!(ui::review_cut_segs_count(&window), 1, "and the cut is untouched");

    // A real edit, then the real Revert button.
    seed_band(&window, 60.0, 70.0);
    click(&window, "add-button");
    let edited = ui::review_cut_segs_count(&window);
    assert!(edited > 1, "the edit landed");
    assert!(button(&window, "revert-button").is_sensitive(), "Revert is live once there is a change");

    click(&window, "revert-button");
    let base = ui::review_cut_segs_count(&window);
    assert_eq!(base, 1, "back to what the page opened with, which is also the base");
    let said = status_text(&window);
    assert!(said.contains("reverted"), "{said}");
    assert!(said.contains("1 segment(s)"), "the base's count is said: {said}");
    assert!(!button(&window, "revert-button").is_sensitive(), "and now there is nothing left to revert");
    assert_eq!(ui::press_revert(&window), cut::NOTHING_TO_REVERT, "a second revert is refused again");
    window.close();
}

/// S3: ✗ Clear takes every scene and every effect off in one step and leaves shift, rows, lanes and the
/// row-count floor exactly where they were.
fn f2_13_s3_clear_takes_scenes_and_effects_and_keeps_the_recordings(app: &adw::Application) {
    // DECISION (spec silent): a cut that arrives from OUTSIDE — a suggestion, or a project reopened with
    // one saved — is not "an edit of this session", so it does not light Revert or Clear. Those two ask
    // whether *this page* has work to lose; the way to have three scenes on the page as your own work is
    // to keep them with ＋ Add, which is what the check below does before clearing.
    let seeded = Cut { segs: vec![clip(0.0, 20.0)], ..Default::default() };
    let window = cut_page(app, &tape(), &seeded);
    seed_band(&window, 30.0, 45.0);
    click(&window, "add-button");
    seed_band(&window, 50.0, 65.0);
    click(&window, "add-button");
    // Lay one effect and one lane under hand too, so there is something of each kind to lose.
    let mut with_fx = ui::review_cut_of(&window);
    with_fx.fx.push(Fx { kind: "zoom".into(), t: 12.0, dur: 3.0, ..Default::default() });
    with_fx.shift.insert("cam2".to_string(), -1.25);
    with_fx.rows.insert("cam2".to_string(), 1);
    with_fx.lanes.push(Lane {
        name: "cam-2".into(),
        src: "project:sources/cam.mkv".into(),
        at: 100.0,
        dur: 20.0,
        ..Default::default()
    });
    with_fx.nrows = 2;
    // Hand-laid, not handed in from outside: seed the state, then say it is an edit, the way a door that
    // changed one of the seven things does through `record_edit`.
    ui::seed_review_cut(&window, &with_fx);
    ui::note_edit(&window);
    settle();

    assert!(ui::review_cut_segs_count(&window) >= 3, "three scenes to lose");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 1, "one effect to lose");
    assert!(button(&window, "clear-cut-button").is_sensitive(), "Clear is live with a cut on it");

    click(&window, "clear-cut-button");
    let after = ui::review_cut_of(&window);
    assert!(after.segs.is_empty(), "every scene is off");
    assert!(after.fx.is_empty(), "and every effect");
    assert_eq!(
        ui::review_cut_shift(&window),
        BTreeMap::from([("cam2".to_string(), -1.25)]),
        "shifts survive: Clear is not an undo of where a file was parked"
    );
    assert_eq!(after.rows, with_fx.rows, "row pins survive");
    assert_eq!(ui::review_lanes(&window), with_fx.lanes, "lanes survive");
    assert_eq!(after.nrows, 2, "and the floor under the row count survives, or the tracks blank out");
    assert_eq!(after.aspect, with_fx.aspect, "aspect survives untouched as well");
    assert_eq!(
        status_text(&window),
        cut::cleared_message(3, 1),
        "S3's sentence, verbatim"
    );

    // With nothing left, Clear refuses with the timeline's own reason.
    assert_eq!(
        ui::press_clear_cut(&window),
        cut::NOTHING_TO_CLEAR,
        "a second Clear answers 'no cut yet'"
    );
    window.close();
}

/// S4/the chord: Ctrl+Z reaches the same rule the button does, so the keyboard and the toolbar cannot drift.
fn f2_13_s4_the_keyboard_reaches_the_same_rules(app: &adw::Application) {
    let seeded = Cut { segs: vec![clip(0.0, 30.0)], ..Default::default() };
    let window = cut_page(app, &tape(), &seeded);
    seed_band(&window, 40.0, 50.0);
    click(&window, "add-button");
    let added = ui::review_cut_segs_count(&window);

    let controller = ui::history_key_controller(&window).expect("the page has a history key controller");
    // Ctrl+Z: the same undo the button performs.
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&gtk::gdk::Key::z, &0u32, &gtk::gdk::ModifierType::CONTROL_MASK],
    );
    settle();
    let after_chord = ui::review_cut_segs_count(&window);
    assert_eq!(after_chord, 1, "Ctrl+Z undid the add the same way the button did");
    assert_eq!(status_text(&window), cut::undone(after_chord), "and printed the same sentence");

    // Ctrl+Shift+Z puts it back through the same pair of rules.
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::z,
            &0u32,
            &gtk::gdk::ModifierType::from_bits_truncate(
                gtk::gdk::ModifierType::CONTROL_MASK.bits() | gtk::gdk::ModifierType::SHIFT_MASK.bits()
            ),
        ],
    );
    settle();
    assert_eq!(
        ui::review_cut_segs_count(&window),
        added,
        "Ctrl+Shift+Z redid it -- one rule, two doors"
    );
    window.close();
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            f2_13_s1_a_real_add_makes_undo_live_and_a_real_undo_says_what_is_left(app);
            RAN_ADD_UNDO.store(true, Ordering::SeqCst);
            f2_13_s1_redo_puts_it_back(app);
            RAN_REDO.store(true, Ordering::SeqCst);
            f2_13_s2_revert_refuses_then_returns_to_the_base(app);
            RAN_REVERT.store(true, Ordering::SeqCst);
            f2_13_s3_clear_takes_scenes_and_effects_and_keeps_the_recordings(app);
            RAN_CLEAR.store(true, Ordering::SeqCst);
            f2_13_s4_the_keyboard_reaches_the_same_rules(app);
            RAN_KEYS.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

#[test]
fn f2_13_wire_undo_redo_revert_and_clear_reach_the_rules_through_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds: the presses resolve the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-history-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    window_round();
    assert!(RAN_ADD_UNDO.load(Ordering::SeqCst), "the add-then-undo check never ran");
    assert!(RAN_REDO.load(Ordering::SeqCst), "the redo check never ran");
    assert!(RAN_REVERT.load(Ordering::SeqCst), "the revert check never ran");
    assert!(RAN_CLEAR.load(Ordering::SeqCst), "the clear check never ran");
    assert!(RAN_KEYS.load(Ordering::SeqCst), "the keyboard-chord check never ran");
}
