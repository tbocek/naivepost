// F3.8 Hold, move, resize, edit, remove — the wire: a press on a band of the effects lane reaches every rule
// in `naivepost::fx_band` through real widgets, and each answer lands on the cut, the status line and the
// buttons rather than staying inside a callback.
//
// `rust/tests/cut_band_hold.rs` proves the rules themselves (and that `fx_band` delegates instead of
// re-deriving); this file proves the Cut page can actually reach them. Before F3.8 there was no ✕ in a bar,
// no band press at all, and ⌦ could not remove an effect someone was holding.
//
// One application, one `connect_activate`, exactly one `Application::run`: `g_application_run` refuses a
// second claimant of the default main context, so all eleven checks live inside that single activate and each
// drops its window before the next is built. cwd is pinned to our own temp root BEFORE the run because the
// presses resolve the project through `startup::session_dir(current_dir())`. No sleeps: `settle()` pumps the
// glib context instead.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx};
use naivepost::cut_line::LinePos;
use naivepost::fx_band::{self, Press};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text};

static RAN_CLICK_FORM: AtomicBool = AtomicBool::new(false);
static RAN_PICK_UP: AtomicBool = AtomicBool::new(false);
static RAN_GRIP: AtomicBool = AtomicBool::new(false);
static RAN_KILL_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_KILL_ABSENT: AtomicBool = AtomicBool::new(false);
static RAN_DELETE_KEY: AtomicBool = AtomicBool::new(false);
static RAN_DRAG_BAND: AtomicBool = AtomicBool::new(false);
static RAN_NUDGE: AtomicBool = AtomicBool::new(false);
static RAN_EDIT_REFIND: AtomicBool = AtomicBool::new(false);
static RAN_ESC: AtomicBool = AtomicBool::new(false);
static RAN_LIVE_FORM: AtomicBool = AtomicBool::new(false);
static RAN_DRAWN_WIDTH: AtomicBool = AtomicBool::new(false);

/// Let the main context run what the widget emissions queued.

fn click(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the Cut page drew no {name}"))
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// A widget somewhere in THIS window's tree, found from the window itself rather than from any global slot:
/// a tree-wide search can land on another window's copy because a closed window survives.
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

/// Build a window sitting on the Cut tab with the given cut seeded, dropping the previous check's window first.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
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
    // The order build_window uses: open the history on what the page opens with, then show it, so a recorded
    // effect is one Undo away from THIS baseline rather than from a stale one.
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    // `seed_review_cut` only fills the cut slot — it draws nothing. The lane's bars and their ✕ exist only
    // after a refresh, which is what every real press path does; without this call no `fx-bar-*` would be in
    // the tree for any check to find.
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// The wide record: 10 s at 4 px/s is 40 px, past BOTH `fx_band::KILL_MIN_PX` (32) and `GRIP_MIN_PX` (30).
/// Its box fractions are set so S4's live-vs-snapshot comparison has four real numbers to disagree about.
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

/// The narrow record: 5 s is 20 px, under both thresholds, so it offers no grips and no ✕.
fn narrow_label(t: f64) -> Fx {
    Fx {
        kind: "label".to_string(),
        t,
        dur: 5.0,
        text: "quiet".to_string(),
        ..Default::default()
    }
}

/// The two-record tape these checks work on: index 0 wide, index 1 narrow.
fn two_records() -> Cut {
    let mut cut_ = Cut::default();
    cut_.fx.push(wide_zoom(20.0));
    cut_.fx.push(narrow_label(40.0));
    cut_
}

/// Put the red line where a band starts and say out loud that a line exists, so Insert believes it too.
fn place_line_at(window: &adw::ApplicationWindow, t: f64) {
    ui::note_place(true);
    ui::set_line_position(window, LinePos { t });
    settle();
}

/// The seam-driven pick-up every later check needs: a real drag on bar 0, middle grip, enough travel to hold.
fn pick_up_bar_zero(window: &adw::ApplicationWindow) {
    let width = ui::cut_bar_width_px(10.0);
    let answer = ui::press_effect_bar(window, 0, width / 2.0, width, 8.0, false);
    assert_eq!(answer, Press::Hold { t: 20.0, from_end: None });
    settle();
}

/// Fire a key through a controller the way the toolkit does: emit its own `key-pressed` signal, which is
/// what GTK's event delivery runs (`cut_line_place_step_widgets.rs` fires keys the same way).
fn fire_key(controller: &gtk::EventControllerKey, key: gtk::gdk::Key) {
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&key, &0u32, &gtk::gdk::ModifierType::empty()],
    );
    settle();
}

/// This window's ⌦ controller, so a test can fire a real Delete through it. `None` on a page that drew none.
fn delete_controller(window: &adw::ApplicationWindow) -> gtk::EventControllerKey {
    ui::delete_key_controller(window).expect("the Cut page wired its delete keys")
}

// --- the round ----------------------------------------------------------------------------------------

fn band_hold_round(app: &adw::Application) {
    // --- (1) S1: a click on a bar opens THE FORM OF THAT KIND ------------------------------------
    let seeded = two_records();
    let window = cut_page(app, &seeded);
    let clicked = ui::press_effect_bar(&window, 0, 20.0, 40.0, 2.0, false);
    assert_eq!(clicked, Press::ClickForm, "2 px of travel on a band is a click");
    assert!(
        ui::zoom_form_open().is_some(),
        "the click opened the ZOOM form: `open_effect_form` dispatched on the record's kind"
    );
    // The other bar's click opens ITS form, not the first one again.
    let clicked_other = ui::press_effect_bar(&window, 1, 10.0, 20.0, 2.0, false);
    assert_eq!(clicked_other, Press::ClickForm);
    assert!(
        ui::label_form_open().is_some(),
        "bar 1 is a label, so the LABEL form opened \u{2014} one door, six kinds, no hard-coded panel"
    );
    RAN_CLICK_FORM.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (2) S1: a drag picks the band up, moves the line, relabels Insert ------------------------
    let window = cut_page(app, &two_records());
    let width = ui::cut_bar_width_px(10.0);
    let answer = ui::press_effect_bar(&window, 0, width / 2.0, width, 8.0, false);
    assert_eq!(answer, Press::Hold { t: 20.0, from_end: None }, "past the slop the band is picked up");
    let held = ui::held_effect().expect("the press put the record in hand");
    assert_eq!(held.t, 20.0, "the thing in hand is bar 0's record");
    assert_eq!(
        status_text(&window),
        fx_band::picked_up("Zoom"),
        "the status reads §A.9's sentence byte-exact"
    );
    // §A.9: "Insert becomes ✎ Edit" while something is held. Read off the real button.
    let insert = ui::line_step_button(&window, "insert-button").expect("the page has an Insert button");
    assert_eq!(
        insert.label().unwrap_or_default().as_str(),
        naivepost::cut_insert::edit_verb(true),
        "the held band turned ⧉ Insert into Edit"
    );
    // And the line landed on the band's start, which is what S1 says picking something up means.
    assert_eq!(ui::line_position(&window).t, 20.0, "the line moved to the band's start");
    RAN_PICK_UP.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (3) S1: where the hand grabbed decides grip or middle -----------------------------------
    let window = cut_page(app, &two_records());
    let left_edge = ui::press_effect_bar(&window, 0, 0.0, 40.0, 8.0, false);
    assert_eq!(left_edge, Press::Hold { t: 20.0, from_end: Some(true) }, "x=0 grabs the START");
    let right_edge = ui::press_effect_bar(&window, 0, 39.0, 40.0, 8.0, false);
    assert_eq!(right_edge, Press::Hold { t: 20.0, from_end: Some(false) }, "x=39 grabs the END");
    let middle = ui::press_effect_bar(&window, 0, 20.0, 40.0, 8.0, false);
    assert_eq!(middle, Press::Hold { t: 20.0, from_end: None }, "the middle slides");
    // Below GRIP_MIN_PX there are no grips at all: every x on the 20 px bar moves it.
    for x in [0.0, 6.0, 10.0, 17.0, 19.0] {
        let answer = ui::press_effect_bar(&window, 1, x, 20.0, 8.0, false);
        assert_eq!(
            answer,
            Press::Hold { t: 40.0, from_end: None },
            "{x} px on a 20 px bar is NOT a grip"
        );
    }
    RAN_GRIP.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (4) S3: the ✕ removes the record, and ONE ↶ puts it back --------------------------------
    let window = cut_page(app, &two_records());
    assert_eq!(ui::review_cut_of(&window).fx.len(), 2, "two records before the kill");
    click(&window, "fx-remove-0");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 1, "the ✕ took one record out");
    assert_eq!(
        status_text(&window),
        fx_band::removed("Zoom"),
        "the kill sentence, whole \u{2014} it promises ↶ and this test collects that promise"
    );
    assert!(ui::held_effect().is_none(), "nothing stays in hand after its record is gone");
    // The promise: the SAME undo seam F2.13's tests use. This only works because removal went through
    // `record_edit`; a `seed_review_cut` would leave nothing to take back.
    click(&window, "undo-button");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 2, "one ↶ restored the killed record");
    assert_eq!(ui::review_cut_of(&window).fx[0].kind, "zoom", "and it is the zoom, not the label");
    RAN_KILL_UNDO.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (5) S3: a bar under 32 px offers no ✕ ---------------------------------------------------
    let window = cut_page(app, &two_records());
    assert!(
        widget_in(&window, "fx-remove-0").is_some(),
        "the 40 px bar carries its ✕"
    );
    assert!(
        widget_in(&window, "fx-remove-1").is_none(),
        "the 20 px bar draws NO ✕ \u{2014} below KILL_MIN_PX there is no room for a kill clear of the grips"
    );
    RAN_KILL_ABSENT.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (6) S3: ⌦ with an effect held removes the EFFECT; with none it keeps deleting ------------
    let window = cut_page(app, &two_records());
    pick_up_bar_zero(&window);
    assert_eq!(ui::review_cut_of(&window).fx.len(), 2, "holding changes nothing yet");
    fire_key(&delete_controller(&window), gtk::gdk::Key::Delete);
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        1,
        "\u{2326} with an effect in hand removed THAT effect"
    );
    assert!(ui::held_effect().is_none(), "and the hand is empty afterwards");
    // With NOTHING held the same key must still reach the selection-delete path: the effect count cannot
    // drop again (there is no effect selected), so the branch that ran is the old one.
    let before = ui::review_cut_of(&window).fx.len();
    fire_key(&delete_controller(&window), gtk::gdk::Key::Delete);
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "with nothing held the key takes no effect \u{2014} the selection path is untouched"
    );
    RAN_DELETE_KEY.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (7) S2: drag moves the whole band; an end resizes it, floored at 0.1 s ------------------
    let window = cut_page(app, &two_records());
    pick_up_bar_zero(&window);
    let pps = ui::SELECT_SURFACE_PPS;
    let said = ui::drag_held_band(&window, 30.0, &[], pps, 100.0, None);
    assert_eq!(said, fx_band::moved("Zoom"), "a slide reports 'moved'");
    let moved = &ui::review_cut_of(&window).fx[0];
    assert_eq!(moved.t, 30.0, "the whole band slid to where the hand left it");
    assert_eq!(moved.dur, 10.0, "a slide never changes the length");
    let resized_said = ui::drag_held_band(&window, 36.0, &[], pps, 100.0, Some(false));
    assert_eq!(resized_said, fx_band::resized("Zoom"), "an end reports 'resized'");
    let resized = &ui::review_cut_of(&window).fx[0];
    // `end = false` is the LEADING end, so the start is what moves and the stop is what holds: 30 + 10 =
    // 40 stays 40, and the band is 36 → 40. The rule lives in `fx_lane::drag_end`, clamped from the
    // stationary end so a crossing stop cannot invert the band.
    assert_eq!(resized.t, 36.0, "the dragged end moved");
    assert_eq!(resized.t + resized.dur, 40.0, "the untouched end stayed at its old stop");
    // Floor: dragging the end far past the start stops AT 0.1 s rather than through it.
    ui::drag_held_band(&window, 5.0, &[], pps, 100.0, Some(true));
    let floored = &ui::review_cut_of(&window).fx[0];
    assert!(
        floored.dur >= naivepost::fx_lane::MIN_BAND_SECONDS,
        "length stopped at the 0.1 s floor, got {}",
        floored.dur
    );
    assert!(floored.dur > 0.0, "no inverted band");
    RAN_DRAG_BAND.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (8) S2: the keyboard nudges a held effect UNSNAPPED -------------------------------------
    let window = cut_page(app, &two_records());
    pick_up_bar_zero(&window);
    // A mark sits 0.05 s away \u{2014} well inside the snap reach at this zoom \u{2014} and the nudge ignores it.
    let said = ui::nudge_held_effect(&window, -1.0 / 25.0);
    assert_eq!(said, fx_band::moved("Zoom"));
    let nudged = &ui::review_cut_of(&window).fx[0];
    assert_eq!(nudged.t, 20.0 - 1.0 / 25.0, "exactly one frame off, unsnapped");
    assert_eq!(nudged.dur, 10.0, "the keyboard moves a band, it does not stretch one");
    RAN_NUDGE.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (9) S4: ✎ Edit re-finds the LIVE record, or says it is gone -----------------------------
    let window = cut_page(app, &two_records());
    // Prime the hold with a STALE copy of the box, then change the live record underneath it.
    let mut stale = wide_zoom(20.0);
    stale.cx = Some(0.11);
    stale.cy = Some(0.22);
    stale.wf = Some(0.33);
    stale.hf = Some(0.44);
    ui::set_held_effect(Some(stale));
    settle();
    let said = ui::press_edit_effect(&window);
    assert!(
        said.contains(fx_band::KEPT_AS_YOU_TYPE),
        "the edit line carries the live-form note: {said}"
    );
    let shown = ui::held_effect().expect("Edit kept the record in hand");
    assert_eq!(shown.cx, Some(0.4), "the box came off the LIVE record, not the snapshot");
    assert_eq!(shown.cy, Some(0.6));
    assert_eq!(shown.wf, Some(0.5));
    assert_eq!(shown.hf, Some(0.4));
    assert!(
        ui::zoom_form_open().is_some(),
        "and the form of that kind is open on it"
    );
    // Now remove the record and ask again: the miss sentence, and no write at all.
    let before = ui::review_cut_of(&window);
    ui::remove_effect_at(&window, 0);
    ui::set_held_effect(Some(wide_zoom(20.0)));
    let gone = ui::press_edit_effect(&window);
    assert_eq!(gone, fx_band::GONE, "a record that is not there says so, whole");
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before.fx.len() - 1,
        "the miss wrote nothing further to the cut"
    );
    RAN_EDIT_REFIND.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (10) S3: Esc drops the hold and says "cancelled" ----------------------------------------
    let window = cut_page(app, &two_records());
    pick_up_bar_zero(&window);
    let dropped = ui::press_band_esc(&window).expect("with a hold, Esc answers");
    assert_eq!(dropped, "cancelled", "§A.9's bare-disarm word");
    assert!(ui::held_effect().is_none(), "the hold is gone");
    assert!(
        ui::press_band_esc(&window).is_none(),
        "with nothing held Esc is NOT claimed \u{2014} it travels on to the form/arm controllers"
    );
    // Through the real controller too: every Esc listener on this window runs, and with no form open and no
    // arm the band's own controller is the one left to answer (§A.9: Esc drops the hold).
    pick_up_bar_zero(&window);
    for controller in [ui::band_esc_controller(&window).expect("the page wired a band Esc controller")] {
        fire_key(&controller, gtk::gdk::Key::Escape);
    }
    settle();
    assert_eq!(status_text(&window), "cancelled", "the key reached the seam and printed");
    assert!(ui::held_effect().is_none(), "and dropped the hold");
    RAN_ESC.store(true, Ordering::SeqCst);
    window.close();
    settle();

    // --- (11) S5: the live note and the visit's one Undo entry -----------------------------------
    let window = cut_page(app, &two_records());
    place_line_at(&window, 21.0);
    click(&window, "effect-item-volume");
    let footer = widget_in(&window, "volume-form-footer").expect("the volume form drew its footer");
    let footer = footer.downcast::<gtk::Label>().expect("the footer is a Label");
    assert_eq!(
        footer.text().as_str(),
        fx_band::KEPT_AS_YOU_TYPE,
        "the note the forms print IS the module's live-form sentence"
    );
    // The visit's single entry: true once, false after, and given back by a refusal.
    assert!(ui::live_form_answer_is_undo(&window), "the first accepted answer owns the entry");
    assert!(!ui::live_form_answer_is_undo(&window), "later changes ride on that same entry");
    ui::live_form_refused();
    assert!(ui::live_form_answer_is_undo(&window), "a refusal gave the entry back");
    RAN_LIVE_FORM.store(true, Ordering::SeqCst);

    // --- (10) S1/S3: the bar's DRAWN width is the width the thresholds measure -----------------
    // Read off `width_request()` of the real widget, so this pins the fact rather than a picture of it:
    // F3.8 decides grips (`GRIP_MIN_PX` 30) and the ✕ (`KILL_MIN_PX` 32) against the bar's on-screen
    // width, so that width must come from the record's own length (`cut_bar_width_px`) and not be a fixed
    // number that disagrees with the press rule.
    let drawn_cut = {
        let mut c = Cut::default();
        c.fx.push(wide_zoom(5.0)); // dur 10 s -> 40 px at SELECT_SURFACE_PPS
        c.fx.push(narrow_label(40.0)); // dur 5 s  -> 20 px
        c
    };
    let window = cut_page(app, &drawn_cut);
    let pps = ui::SELECT_SURFACE_PPS;
    for (index, dur) in [(0usize, 10.0f64), (1usize, 5.0f64)] {
        let bar = widget_in(&window, &format!("fx-bar-{index}")).expect("the bar is in the tree");
        let drawn = f64::from(bar.width_request());
        let rules_width = ui::cut_bar_width_px(dur);
        assert_eq!(
            drawn,
            rules_width.max(48.0).round(),
            "bar {index} draws at its duration's width floored at 48 px, not a fixed size"
        );
        // The ✕ follows the SAME number the bar was sized with: present only past KILL_MIN_PX.
        let has_kill = widget_in(&window, &format!("fx-remove-{index}")).is_some();
        assert_eq!(
            has_kill,
            fx_band::kill_open(rules_width),
            "bar {index} ({drawn} px drawn) offers a \u{2715} exactly when the kill threshold is met"
        );
        // And the grips are read against that same drawn width: left 6 px / right 6 px are edges, the middle
        // is a handle for moving. Below GRIP_MIN_PX nothing is an edge at all.
        if fx_band::grips_open(drawn) {
            assert_eq!(fx_band::grab_at(0.0, drawn), Some(true), "left end is a grip");
            assert_eq!(fx_band::grab_at(drawn, drawn), Some(false), "right end is a grip");
            assert_eq!(fx_band::grab_at(drawn / 2.0, drawn), None, "the middle moves, never resizes");
        } else {
            // A bar under `GRIP_MIN_PX` offers no edge ANYWHERE in it, not even at its own ends: below the
            // threshold every pixel is a handle for moving, which is what stops a 20 px label from being
            // resized by a grab that meant to move it (`fx_band::grab_at`'s own reason).
            assert_eq!(fx_band::grab_at(0.0, drawn), None, "no left edge below the grip floor");
            assert_eq!(fx_band::grab_at(drawn, drawn), None, "no right edge below the grip floor");
            assert_eq!(fx_band::grab_at(drawn / 2.0, drawn), None, "the middle moves");
        }
        assert!(rules_width > 0.0 && pps > 0.0, "the pixel rate is real");
    }
    RAN_DRAWN_WIDTH.store(true, Ordering::SeqCst);
    window.close();
    settle();
    window.close();
    settle();
}

#[test]
fn f3_8_widget_the_lane_wires_hold_move_resize_edit_and_remove() {
    let root = std::env::temp_dir().join(format!("np-band-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(band_hold_round);
        app.run();
    });

    assert!(RAN_CLICK_FORM.load(Ordering::SeqCst), "S1 click-opens-form never ran");
    assert!(RAN_PICK_UP.load(Ordering::SeqCst), "S1 pick-up never ran");
    assert!(RAN_GRIP.load(Ordering::SeqCst), "S1 grip position never ran");
    assert!(RAN_KILL_UNDO.load(Ordering::SeqCst), "S3 kill-and-undo never ran");
    assert!(RAN_KILL_ABSENT.load(Ordering::SeqCst), "S3 narrow-bar-no-cross never ran");
    assert!(RAN_DELETE_KEY.load(Ordering::SeqCst), "S3 delete-key never ran");
    assert!(RAN_DRAG_BAND.load(Ordering::SeqCst), "S2 drag/resize never ran");
    assert!(RAN_NUDGE.load(Ordering::SeqCst), "S2 unsnapped nudge never ran");
    assert!(RAN_EDIT_REFIND.load(Ordering::SeqCst), "S4 edit-refind never ran");
    assert!(RAN_ESC.load(Ordering::SeqCst), "S3 esc-drops-hold never ran");
    assert!(RAN_LIVE_FORM.load(Ordering::SeqCst), "S5 live-form note/entry never ran");
    assert!(RAN_DRAWN_WIDTH.load(Ordering::SeqCst), "S1/S3 drawn-bar-width never ran");
}
