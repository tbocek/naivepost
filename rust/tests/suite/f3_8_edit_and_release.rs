// F3.8 Hold, move, resize, edit, remove — the two doors the first pass left unwired.
//
// `band_hold_widgets.rs` proves a press on a band reaches every rule in `naivepost::fx_band`. What it could not
// catch is that the two ENDPOINTS of §S1's "Insert becomes ✎ Edit" and §S2's "hold drops when the line walks off"
// had no door: the button relabelled to ✎ Edit still opened the insert chooser, and `HOLD_RELEASE_SECONDS` was a
// number nothing read. Both are wired now, so both are checked here through the widgets that reach them.
//
// One application, one `connect_activate`, exactly one `Application::run` (the `g_application_run` rule the other
// widget rounds follow): all checks live inside that single activate and each drops its window before the next.
// cwd is pinned to our own temp root before the run because the presses resolve the project through
// `startup::session_dir(current_dir())`. No sleeps: `settle()` pumps the glib context.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx};
use naivepost::cut_line::LinePos;
use naivepost::fx_band;
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_EDIT_DOOR: AtomicBool = AtomicBool::new(false);
static RAN_EDIT_GONE: AtomicBool = AtomicBool::new(false);
static RAN_HOLD_DROPS: AtomicBool = AtomicBool::new(false);
static RAN_ZOOM_MOVED: AtomicBool = AtomicBool::new(false);
static RAN_ZOOM_RESIZED: AtomicBool = AtomicBool::new(false);
static RAN_ZOOM_REFUSALS: AtomicBool = AtomicBool::new(false);
static RAN_DEBOUNCE: AtomicBool = AtomicBool::new(false);

/// A placed zoom's rectangle in the preview's px, through the same door the panel paints with (`camera_at`'s
/// reading of the record), so a test presses where the frame actually is rather than where it guesses it is.
fn camera_box(record: &Fx, fit: (f64, f64)) -> (f64, f64, f64, f64) {
    naivepost::ui::cut_preview_frame::camera_rect_px(
        &naivepost::fx_zoom::rect_of(record),
        fit,
    )
}

/// A cut with ONE placed zoom at 20 s, wide enough that its rectangle covers a useful part of the preview and the
/// centre starts well away from both the frame centre and the clamp bounds.
fn one_zoom() -> Cut {
    let mut cut_ = Cut::default();
    cut_.fx.push(Fx {
        kind: "zoom".to_string(),
        t: 20.0,
        dur: 10.0,
        trans: 0.0,
        tout: 0.0,
        cx: Some(0.4),
        cy: Some(0.6),
        hf: Some(0.5),
        ..Default::default()
    });
    cut_
}

/// The wide record, copied from `band_hold_widgets`: 10 s at this zoom clears both pixel floors, so nothing in
/// these checks trips over a bar too narrow to hold.
fn wide_zoom(t: f64) -> Fx {
    Fx {
        kind: "zoom".to_string(),
        t,
        dur: 10.0,
        cx: Some(0.4),
        cy: Some(0.6),
        wf: Some(0.5),
        hf: Some(0.4),
        ..Default::default()
    }
}

fn tape() -> Cut {
    let mut cut_ = Cut::default();
    cut_.fx.push(wide_zoom(20.0));
    cut_.fx.push(Fx {
        kind: "label".to_string(),
        t: 40.0,
        dur: 5.0,
        text: "quiet".to_string(),
        ..Default::default()
    });
    cut_
}

/// A window on the Cut tab with `seeded` as its cut — the same staging `band_hold_widgets` uses, including the
/// `reopen_history_on` + `seed_review_cut` order and the refresh that actually draws the lane's bars.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// Press the page's ⧉ Insert / ✎ Edit button the way the toolkit does, and hand back what landed on the status
/// line — which is the only place the returned sentence can be read, because the wire prints rather than returns.
fn click_insert(window: &adw::ApplicationWindow) -> String {
    ui::line_step_button(window, "insert-button")
        .expect("the Cut page drew an insert-button")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    status_text(window)
}

fn round(app: &adw::Application) {
    // --- S4/S1: the button labelled ✎ Edit opens the HELD EFFECT'S FORM, not the chooser ----------------
    let window = cut_page(app, &tape());
    ui::note_place(true);
    ui::set_line_position(&window, LinePos { t: 20.0 });
    ui::set_held_effect(Some(wide_zoom(20.0)));
    ui::refresh_insert_button(&window);
    settle();
    let button = ui::line_step_button(&window, "insert-button").expect("insert-button exists");
    assert_eq!(
        button.label().unwrap_or_default().as_str(),
        naivepost::cut_insert::edit_verb(true),
        "a held effect turns the button into Edit \u{2014} the label half of \u{a7}S1"
    );
    let said = click_insert(&window);
    assert!(
        said.contains(fx_band::KEPT_AS_YOU_TYPE),
        "pressing Edit says the live-form line, not a chooser prompt: {said}"
    );
    assert!(
        !said.contains("choose the file"),
        "Edit must NOT open the insert chooser: {said}"
    );
    assert!(
        ui::zoom_form_open().is_some(),
        "and the held record's own form is on screen"
    );
    assert!(
        ui::held_effect().is_some(),
        "Edit keeps the record in hand rather than dropping it"
    );
    RAN_EDIT_DOOR.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- S4: a record that went away says the miss sentence and writes nothing -------------------------
    let seeded = tape();
    let window = cut_page(app, &seeded);
    ui::remove_effect_at(&window, 0);
    settle();
    let before = ui::review_cut_of(&window).fx.len();
    ui::set_held_effect(Some(wide_zoom(20.0)));
    let said = click_insert(&window);
    assert_eq!(said, fx_band::GONE, "the miss sentence, whole, printed by the real button");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "the miss wrote nothing: \u{201c}nothing was changed\u{201d} has to be true of the cut"
    );
    RAN_EDIT_GONE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- S2: walking the line off the band drops the hold through the page's own line door --------------
    let window = cut_page(app, &tape());
    ui::note_place(true);
    ui::set_held_effect(Some(wide_zoom(20.0)));
    ui::set_line_position(&window, LinePos { t: 20.0 });
    assert!(
        ui::held_effect().is_some(),
        "a line moved to the band's own start keeps the hold \u{2014} pick-up depends on this"
    );
    ui::set_line_position(&window, LinePos { t: 20.0 + 10.0 + 1.0 });
    assert!(
        ui::held_effect().is_none(),
        "a line a full second past the band's end must have dropped the hold"
    );
    // And the near miss: a frame outside keeps it, so a mid-drag crossing does not snatch the band away.
    ui::set_held_effect(Some(wide_zoom(20.0)));
    ui::set_line_position(&window, LinePos { t: 30.0 + fx_band::HOLD_RELEASE_SECONDS * 0.5 });
    assert!(
        ui::held_effect().is_some(),
        "half a frame past the end is inside the slack \u{2014} the hand still holds it"
    );
    RAN_HOLD_DROPS.store(true, Ordering::SeqCst);
    window.close();
    settle();
    // --- S3: a placed camera rect dragged on the picture MOVES, and one ↶ takes it back ----------------
    let window = cut_page(app, &one_zoom());
    let fit = (ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H);
    let before = ui::review_cut_of(&window).fx[0].clone();
    let box_px = camera_box(&before, fit);
    // A press in the interior, dragged both axes: the centre travels, `hf` does not.
    let inside = (box_px.0 + box_px.2 / 2.0, box_px.1 + box_px.3 / 2.0);
    let said = naivepost::ui::cut_zoom_hand::zoom_box_edit_on_release(
        &window,
        20.0,
        fit,
        inside,
        (inside.0 + 40.0, inside.1 + 30.0),
    )
    .expect("a press inside a placed zoom's box is a camera drag");
    assert_eq!(said, fx_band::moved("Zoom"), "§S3's move sentence, whole");
    let moved_rec = ui::review_cut_of(&window).fx[0].clone();
    assert!(
        moved_rec.cx.unwrap() > before.cx.unwrap(),
        "the centre followed the hand: {:?} -> {:?}",
        before.cx,
        moved_rec.cx
    );
    assert!(
        moved_rec.cy.unwrap() > before.cy.unwrap(),
        "and the other axis moved with it"
    );
    assert_eq!(moved_rec.hf, before.hf, "a slide never sizes the camera");
    // One ↶ takes this gesture back alone, because the write went through `record_edit`.
    ui::press_undo(&window);
    settle();
    let back = ui::review_cut_of(&window).fx[0].clone();
    assert_eq!(back.cx, before.cx, "one undo restores the centre");
    assert_eq!(back.hf, before.hf, "and the height, which never moved");
    RAN_ZOOM_MOVED.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- S3: an edge drag RESIZES, and the other axis stays put ---------------------------------------
    let window = cut_page(app, &one_zoom());
    let fit = (ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H);
    let before = ui::review_cut_of(&window).fx[0].clone();
    let box_px = camera_box(&before, fit);
    // Just inside the bottom edge -- within `fx_text::EDGE_GRIP_PX`, the reach this door uses.
    let on_edge = (box_px.0 + box_px.2 / 2.0, box_px.1 + box_px.3 - 2.0);
    let said = naivepost::ui::cut_zoom_hand::zoom_box_edit_on_release(
        &window,
        20.0,
        fit,
        on_edge,
        (on_edge.0, on_edge.1 - 40.0),
    )
    .expect("a press on the bottom line sizes the camera");
    assert_eq!(said, fx_band::resized("Zoom"), "§S3's resize sentence, whole");
    let sized = ui::review_cut_of(&window).fx[0].clone();
    assert!(
        sized.hf.unwrap() < before.hf.unwrap(),
        "dragging the bottom up shrinks the slice: {:?} -> {:?}",
        before.hf,
        sized.hf
    );
    assert!(
        (sized.cx.unwrap() - before.cx.unwrap()).abs() < 1e-9,
        "an edge drag leaves the other axis alone"
    );
    RAN_ZOOM_RESIZED.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- S3: an armed or held press is not a camera drag at all ----------------------------------------
    let window = cut_page(app, &one_zoom());
    let fit = (ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H);
    let box_px = camera_box(&ui::review_cut_of(&window).fx[0], fit);
    let inside = (box_px.0 + box_px.2 / 2.0, box_px.1 + box_px.3 / 2.0);
    let before_cx = ui::review_cut_of(&window).fx[0].cx;
    ui::set_zoom_armed(true);
    assert_eq!(
        naivepost::ui::cut_zoom_hand::zoom_box_edit_on_release(
            &window,
            20.0,
            fit,
            inside,
            (inside.0 + 60.0, inside.1)
        ),
        None,
        "while the zoom arm is up the arm owns the press"
    );
    ui::set_zoom_armed(false);
    ui::set_held_effect(Some(ui::review_cut_of(&window).fx[0].clone()));
    assert_eq!(
        naivepost::ui::cut_zoom_hand::zoom_box_edit_on_release(
            &window,
            20.0,
            fit,
            inside,
            (inside.0 + 60.0, inside.1)
        ),
        None,
        "with a lane effect in hand, F3.8's hold owns the press"
    );
    ui::set_held_effect(None);
    // And no travel is no drag: that press is §F3.1's framing question, not this door's.
    assert_eq!(
        naivepost::ui::cut_zoom_hand::zoom_box_edit_on_release(&window, 20.0, fit, inside, inside),
        None,
        "a press without travel is a click"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx[0].cx,
        before_cx,
        "none of the three wrote anything"
    );
    RAN_ZOOM_REFUSALS.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- S5: a burst of keystrokes commits ONCE, after the pause ---------------------------------------
    let window = cut_page(app, &tape());
    // Nothing lands before the pause runs out: that is the whole rule, stated as the gate's own answer.
    assert!(
        !fx_band::keystroke_lands(0.0, fx_band::DEBOUNCE_MS),
        "a keystroke that just happened does not land"
    );
    assert!(
        fx_band::keystroke_lands(fx_band::DEBOUNCE_MS, fx_band::DEBOUNCE_MS),
        "the same one lands once the pause has run out"
    );
    // Open the caption form through the door the widget fires, then land a box so the form has a real record.
    ui::note_place(true);
    ui::set_line_position(&window, LinePos { t: 20.0 });
    ui::press_text_item(&window);
    settle();
    ui::text_drag_ended_with_source(&window, (240.0, 200.0), (240.0, 200.0), (300.0, 600.0));
    settle();
    assert!(ui::text_form_open().is_some(), "the caption form is open to type into");
    // Type twice, quickly, through the real entry: each keystroke restamps, one timer waits.
    let length = entry_in(&window, "text-field-length");
    length.set_text("4");
    settle();
    length.set_text("45");
    settle();
    // Wait ON A CONDITION: the debounced commit re-reads the entry into the form's stored state, so the form saying
    // 45 is the observable fact. No fixed sleep -- on a loaded machine the timer can take longer than one guess.
    common::wait_until(
        "the debounced commit moved the typed length into the caption form",
        || ui::text_form_open().is_some_and(|form| (form.dur - 45.0).abs() < 1e-9),
    );
    // The cut is untouched: the form keeps the edit, Apply writes it (§S5's one-Undo-for-the-whole-edit).
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        2,
        "a keystroke never places a record \u{2014} Apply does"
    );
    assert!(
        status_text(&window).contains("45"),
        "the status row shows the kept reading: {}",
        status_text(&window)
    );
    // The footer and the two buttons, in the order §S5 spells, on the same form the commit just kept.
    let footer = widget_in(&window, "text-form-footer").expect("the form drew its note");
    assert_eq!(
        footer
            .downcast_ref::<gtk::Label>()
            .expect("the note is a label")
            .text()
            .as_str(),
        fx_band::KEPT_AS_YOU_TYPE
    );
    let row = widget_in(&window, "form-buttons-text").expect("the shared button row");
    let names: Vec<String> = row
        .downcast_ref::<gtk::Box>()
        .expect("the row is a box")
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .map(|w| w.widget_name().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["text-apply-button", "text-cancel-button"],
        "Apply then Cancel, left to right, in one row"
    );
    RAN_DEBOUNCE.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// One of this window's widgets by name, found from the window itself: a tree-wide search can land on another
/// window's copy, because a closed window survives.
fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    common::widget_in(window, name)
}

/// One of a form's entries by name, fired through the widget rather than a form-local reader.
fn entry_in(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    widget_in(window, name)
        .unwrap_or_else(|| panic!("the form drew no {name}"))
        .downcast::<gtk::Entry>()
        .expect("the field is an entry")
}

#[test]
fn f3_8_s4_edit_door_reopens_the_held_effect_form() {
    run_round();
    assert!(RAN_EDIT_DOOR.load(Ordering::SeqCst), "S4 edit-door check never ran");
}

#[test]
fn f3_8_s4_edit_says_gone_and_writes_nothing() {
    run_round();
    assert!(RAN_EDIT_GONE.load(Ordering::SeqCst), "S4 gone-check never ran");
}

#[test]
fn f3_8_s2_hold_survives_inside_the_frame_slack() {
    // The pure rule, no widgets: `fx_band::hold_survives` over the band and its one-frame slack.
    let frame = fx_band::HOLD_RELEASE_SECONDS;
    assert!(fx_band::hold_survives(25.0, 20.0, 10.0), "inside the band the hold stands");
    assert!(
        fx_band::hold_survives(20.0, 20.0, 10.0),
        "pick-up puts the line ON the band's start, so the rule must keep that hold"
    );
    assert!(fx_band::hold_survives(20.0 - frame, 20.0, 10.0), "one frame before the start survives");
    assert!(fx_band::hold_survives(30.0 + frame, 20.0, 10.0), "one frame past the end survives");
    assert!(
        !fx_band::hold_survives(20.0 - frame - 0.01, 20.0, 10.0),
        "past a frame outside, the hold is behind the viewer and drops"
    );
    assert!(!fx_band::hold_survives(31.0, 20.0, 10.0), "a second past the end drops");
}

#[test]
fn f3_8_s2_line_walking_off_the_band_drops_the_hold() {
    run_round();
    assert!(RAN_HOLD_DROPS.load(Ordering::SeqCst), "S2 line-walks-off check never ran");
}

#[test]
fn f3_8_s3_dragging_a_placed_zoom_box_records_and_says_moved() {
    run_round();
    assert!(RAN_ZOOM_MOVED.load(Ordering::SeqCst), "S3 camera-move check never ran");
}

#[test]
fn f3_8_s3_dragging_a_zoom_edge_says_resized() {
    run_round();
    assert!(RAN_ZOOM_RESIZED.load(Ordering::SeqCst), "S3 camera-resize check never ran");
}

#[test]
fn f3_8_s3_armed_or_held_press_is_not_a_camera_drag() {
    run_round();
    assert!(RAN_ZOOM_REFUSALS.load(Ordering::SeqCst), "S3 camera-drag refusals never ran");
}

#[test]
fn f3_8_s5_a_burst_of_keystrokes_commits_once_after_the_pause() {
    run_round();
    assert!(RAN_DEBOUNCE.load(Ordering::SeqCst), "S5 debounced-keystroke check never ran");
}

/// The checks all live in ONE application activation: `g_application_run` refuses a second claimant of the default
/// main context, and the round's checks depend on each other's ordering (cwd, the last window). Each `f3_8_s*`
/// entry point runs the round and asserts only that its own check reported; the round is idempotent because
/// `ONCE` guards the activation.
fn run_round() {
    let root = std::env::temp_dir().join(format!("np-f38-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run_with_args::<&str>(&[]);
    });
}

