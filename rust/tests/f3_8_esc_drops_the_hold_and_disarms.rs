// F3.8 S3 (`Esc drops the hold and disarms ("cancelled" for a bare disarm)`) — the half `band_hold_widgets.rs`
// could not reach: it proves Esc takes the HOLD down, but an armed hand-drag was still up when that press
// returned, because the door claimed the key and no other Esc handler ran after it. Both halves live in ONE
// call now (`ui::press_band_esc`), so both are asserted here, and one test fires the real Escape key through
// the page's own controller to prove the wiring reaches it (window.rs wires the band's controller FIRST among
// the Esc controllers; GTK stops at the first handler that claims the key).
//
// One application, one `connect_activate`, exactly one `Application::run`: `g_application_run` refuses a second
// claimant of the default main context. The arm slots (`ZOOM_ARMED`, `TEXT_ARMED`, `SVG_ARMED`) and the held
// effect are process-wide, so the three scenarios run IN SEQUENCE inside that single activate, each closing its
// window and taking back down whatever it raised. No sleeps: `settle()` pumps the glib context, and anything a
// repaint timer writes is awaited with `common::wait_until`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind, Fx};
use naivepost::cut_line::LinePos;
use naivepost::fx_band::{self, Press};
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, widget_in};

static RAN_DISARM: AtomicBool = AtomicBool::new(false);
static RAN_REAL_KEY: AtomicBool = AtomicBool::new(false);
static RAN_BARE_HOLD: AtomicBool = AtomicBool::new(false);

/// The second the checks stand their red line on, and the start of the wide record they pick up: kept equal so
/// arming a hand-drag at the line and holding a band at its start are the same moment on the tape.
const LINE_SECOND: f64 = 20.0;

/// A window on the Cut tab with one wide zoom record at `LINE_SECOND` — wide enough (10 s at the lane's px/s)
/// that a middle press is a hold rather than a click.
fn cut_page_with_a_bar(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    let mut seeded = Cut::default();
    seeded.fx.push(Fx {
        kind: "zoom".to_string(),
        t: LINE_SECOND,
        dur: 10.0,
        ..Default::default()
    });
    ui::reopen_history_on(&window, &seeded);
    ui::seed_review_cut(&window, &seeded);
    // `seed_review_cut` fills the slot only; the bar exists after the refresh every real press path does.
    ui::refresh_effects_lane(&window);
    settle();
    window
}

/// Pick up bar 0 through the seam the bar's own gesture hands to: a middle press with travel is a hold.
fn pick_up_bar_zero(window: &adw::ApplicationWindow) {
    let width = ui::cut_bar_width_px(10.0);
    let answer = ui::press_effect_bar(window, 0, width / 2.0, width, 8.0, false);
    assert_eq!(answer, Press::Hold { t: LINE_SECOND, from_end: None });
    assert!(ui::held_effect().is_some(), "the pick-up put the record in hand");
    settle();
}

/// Arm ⊕ Zoom through the dropdown's own door, with a line standing where the spec says a zoom needs one.
fn arm_zoom(window: &adw::ApplicationWindow) {
    ui::note_place(true);
    ui::note_zoom_place(true);
    ui::set_line_position(window, LinePos { t: LINE_SECOND });
    let said = ui::press_zoom_item(window);
    assert!(ui::zoom_armed(), "the dropdown's ⊕ Zoom did not arm -- {said}");
    settle();
}

/// Fire a key through a controller the way the toolkit does: emit its own `key-pressed` signal, which is what
/// GTK's event delivery runs (`band_hold_widgets.rs` fires keys the same way).
fn fire_key(controller: &gtk::EventControllerKey, key: gtk::gdk::Key) {
    let _ = controller.emit_by_name::<bool>(
        "key-pressed",
        &[&key, &0u32, &gtk::gdk::ModifierType::empty()],
    );
    settle();
}

/// S3, both halves at the logic seam: a hold plus an armed drag, and one Esc answering for both.
fn esc_drops_hold_and_disarms(app: &adw::Application) {
    let window = cut_page_with_a_bar(app);

    // First: with nothing in hand the door does NOT claim the key — that `None` is what leaves the per-kind
    // controllers their job on a page that holds nothing (and `band_hold_widgets.rs` pins it too).
    assert_eq!(
        ui::press_band_esc(&window),
        None,
        "with no hold Esc travels on to the form/arm controllers"
    );

    // Hold + armed zoom: the state §F3.8 S3 names. Esc must drop the hold AND take the arm down.
    pick_up_bar_zero(&window);
    arm_zoom(&window);
    assert_eq!(
        ui::press_band_esc(&window).as_deref(),
        Some(fx_band::CANCELLED),
        "S3's bare-disarm word covers both halves"
    );
    assert!(ui::held_effect().is_none(), "S3: the hold dropped");
    assert!(!ui::zoom_armed(), "S3: and the armed drag disarmed -- this is what used to stay up");

    // Repeatable: a second hold + arm, answered the same way (no slot left sticky from the first press).
    pick_up_bar_zero(&window);
    arm_zoom(&window);
    assert_eq!(ui::press_band_esc(&window).as_deref(), Some(fx_band::CANCELLED));
    assert!(ui::held_effect().is_none(), "the second Esc dropped its own hold");
    assert!(!ui::zoom_armed(), "and its own arm");
    assert_eq!(ui::press_band_esc(&window), None, "and then has nothing left to claim");

    RAN_DISARM.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// S3 through the real widget: the page's own band-Esc controller answers a fired Escape, prints on the status
/// row, and drops BOTH the hold and every arm that was up (zoom and caption together).
fn real_escape_key_drops_hold_and_disarms(app: &adw::Application) {
    let window = cut_page_with_a_bar(app);
    let controller = ui::band_esc_controller(&window).expect("the page wired a band Esc controller");

    pick_up_bar_zero(&window);
    arm_zoom(&window);
    // ❝ Text armed too, through its own dropdown door: one Esc must clear every hand waiting on the picture,
    // not just the kind whose controller happens to be wired first.
    ui::note_place(true);
    let said = ui::press_effect_item(&window, EffectKind::Text);
    assert!(ui::text_armed(), "the dropdown's ❝ Text did not arm -- {said}");
    settle();

    fire_key(&controller, gtk::gdk::Key::Escape);
    // The row is written by the controller's own handler, i.e. on the main context this emission returns into;
    // wait for the word rather than reading it once.
    common::wait_until("the status row to read cancelled", || status_text(&window) == fx_band::CANCELLED);
    assert_eq!(status_text(&window), fx_band::CANCELLED, "the key reached the seam and printed");
    assert!(ui::held_effect().is_none(), "and dropped the hold");
    assert!(!ui::zoom_armed(), "and disarmed ⊕ Zoom");
    assert!(!ui::text_armed(), "and disarmed ❝ Text");

    RAN_REAL_KEY.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The half that must not regress: a hold with NOTHING armed still answers `cancelled` and drops the hold, and
/// the heading's ✕ (which forwards to this same door) does too.
fn bare_hold_still_answers_cancelled(app: &adw::Application) {
    let window = cut_page_with_a_bar(app);
    assert!(!ui::zoom_armed() && !ui::text_armed() && !ui::svg_armed(), "nothing armed to start with");

    pick_up_bar_zero(&window);
    assert_eq!(ui::press_band_esc(&window).as_deref(), Some(fx_band::CANCELLED));
    assert!(ui::held_effect().is_none(), "a bare hold still drops, with no arm to take down");

    // And through the cross the pinned heading draws: `cut_form_frame::press_close` forwards a held band to
    // this same door, so the button has to keep working after the reorder.
    pick_up_bar_zero(&window);
    let close = widget_in(&window, "cut-form-close").expect("the form column drew its cross");
    close.emit_by_name::<()>("clicked", &[]);
    settle();
    common::wait_until("the cross's press to drop the hold", || ui::held_effect().is_none());
    assert!(ui::held_effect().is_none(), "the heading's cross went through the same door");

    RAN_BARE_HOLD.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn round(app: &adw::Application) {
    esc_drops_hold_and_disarms(app);
    real_escape_key_drops_hold_and_disarms(app);
    bare_hold_still_answers_cancelled(app);
    app.quit();
}

/// ONE application, ONE `connect_activate`, ONE scenario: every check lives inside it in sequence, which is what
/// `band_hold_widgets.rs` and `f0_5_bookkeeping_live.rs` do for the same reason -- `app.run()` may only be
/// claimed once per process, and a second claimant aborts rather than failing politely. That rules out three
/// `#[test]` functions each driving a round: libtest runs them on threads in an order this file cannot control
/// (`--test-threads=1` runs them one after another, so a "whoever claims first" scheme leaves the other two
/// waiting on work that will never come). So there is a single `#[test]` below, and it asserts all three flags.
fn run_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let root = std::env::temp_dir().join(format!("np-f38esc-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp root created");
        std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run();
    });
}

#[test]
fn f3_8_s3_esc_drops_the_hold_and_disarms() {
    run_round();
    assert!(RAN_DISARM.load(Ordering::SeqCst), "the hold-and-disarm check never ran");
    assert!(RAN_REAL_KEY.load(Ordering::SeqCst), "the real-Escape wire check never ran");
    assert!(RAN_BARE_HOLD.load(Ordering::SeqCst), "the bare-hold / cross check never ran");
}
