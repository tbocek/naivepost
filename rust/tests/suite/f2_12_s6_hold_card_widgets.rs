// F2.12 S6 (`right-click or double left-click a card → holds it and Insert becomes Edit`) — THE WIRE. The
// rule lives in `src/ui/cut_card_hold.rs`; this file proves the painted band's own gesture reaches it: a
// press is emitted on the gesture the page attached, converted by the SAME span the bars were painted from,
// and what comes back is the held card plus the button's word — the two things S6 says change.
//
// F2.12 S6 also names the other doors: a double right click answers `holds_card` twice and holds the same
// card twice (harmless, and the rule test pins it), a lone left click belongs to F2.4's line and must NOT be
// a hold, a miss leaves a held card and the button's word alone, and a card picked up by this hold walks
// through the real `insert-button` so the `Edit` word is the button's and not a string this test compares.
//
// One application, one `connect_activate`, exactly one `#[test]`, like `insert_form_widgets.rs`: these checks
// mutate process-shared state (this window's cut slot, its held clip, the status line), so they run once in
// sequence and report through flags the test asserts on at the end.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{self, Seg};
use naivepost::shell::Page;
use naivepost::timeline::Recording;
use naivepost::ui;
use naivepost::ui::cut_card_hold as hold;

use crate::common;
use common::{
    clip, cut_button as button, fixture_dir, hold_last_window, release_last_window, settle,
    status_text, tape,
};

static RAN_DOUBLE: AtomicBool = AtomicBool::new(false);
static RAN_RIGHT: AtomicBool = AtomicBool::new(false);
static RAN_FOOTAGE: AtomicBool = AtomicBool::new(false);
static RAN_PRESSED: AtomicBool = AtomicBool::new(false);
static RAN_RULE: AtomicBool = AtomicBool::new(false);

/// A spliced card: `s == e` with a `dur`, which is what makes it an insert (`Seg::is_insert`).
fn card(at: f64, dur: f64, path: &str) -> Seg {
    Seg {
        s: at,
        e: at,
        dur,
        ins: path.to_string(),
        ..Default::default()
    }
}

/// A button's word, as the page's own accessor reads it (`label` is an Option on a button built from icon
/// name, so an unwrap names the widget rather than hiding that it went missing).
fn word(window: &adw::ApplicationWindow, name: &str) -> String {
    button(window, name).label().unwrap_or_default().to_string()
}

/// A window sitting on the Cut tab with this tape and cut, the way `insert_form_widgets.rs` builds one.
fn cut_page(
    app: &adw::Application,
    tape: &[Recording],
    seeded: &cut::Cut,
) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    hold_last_window(window.clone());
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    ui::set_session_recordings(tape);
    ui::seed_review_cut(&window, seeded);
    ui::set_held_clip(None);
    ui::clear_selection(&window);
    settle();
    window
}

/// Fire the band's own hold gesture the toolkit's way.
///
/// `released` carries `(n_press, x, y)` — the BUTTON IS NOT AN ARGUMENT: GTK fills `current_button` from a
/// real button event, and gtk4-rs 0.11 exposes no `set_current_button` (`GestureSingleExt` has `set_button`
/// only), so a synthetic emit can never say "this was the right button" — it always reports the gesture's own
/// `button`, which a `set_button(0)` gesture answers as 0. The LEFT half is therefore emitted on the page's
/// own gesture (that is a real press reaching a real handler); the right-button half goes through the same
/// `press` the handler forwards to and the gate it gates on, which is the honest limit of a headless test.
fn press_left(window: &adw::ApplicationWindow, x: f64, n_press: i32) {
    let gesture = ui::cut_card_hold::card_hold_gesture_for_test(window)
        .expect("the page wired a card-hold press onto the painted band");
    gesture.emit_by_name::<()>("released", &[&n_press, &x, &0f64]);
    settle();
}

/// The x of a second, through the span the band paints its bars with — so the press lands where the ink is.
fn x_of(window: &adw::ApplicationWindow, t: f64) -> f64 {
    ui::cut_card_hold::span_of_for_test(window).x_of(t)
}

/// S6: a double left click on a card holds it, and `insert-button` says `Edit` under the hand.
fn f2_12_s6_a_double_click_on_a_card_holds_it(app: &adw::Application) {
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 90.0), card(30.0, 4.0, "assets/tier.svg")],
        ..Default::default()
    };
    let window = cut_page(app, &tape(), &seeded);
    assert_eq!(
        ui::held_clip(),
        None,
        "the page opened with nothing in hand"
    );
    assert_eq!(word(&window, "insert-button"), "Insert");

    // 31 s is one second into the card, which runs 30..34 (`s == e`, so its seconds are `s .. s + dur`).
    press_left(&window, x_of(&window, 31.0), 2);

    let held = ui::held_clip().expect("the double click took the card up");
    assert_eq!(
        held.ins, "assets/tier.svg",
        "the card under the ink came into the hand"
    );
    assert_eq!(
        held.s, 30.0,
        "and it is that card, not its neighbour: {:?}",
        held
    );
    assert_eq!(
        word(&window, "insert-button"),
        naivepost::cut_insert::edit_verb(true),
        "S6: Insert becomes Edit under a held card"
    );
    assert!(
        status_text(&window).contains("tier.svg"),
        "{}",
        status_text(&window)
    );
    let held_now = ui::held_clip().expect("the gesture left the card in hand");
    assert!(
        naivepost::cut_insert::hold(Some(&held_now)),
        "a card held by the double click is a card to the button's own predicate: {held_now:?}"
    );
    assert!(
        status_text(&window).contains("Edit"),
        "the line names the new word: {}",
        status_text(&window)
    );
    window.close();
}

/// S6: the right button holds a card on its FIRST press — the half the line gesture can never see, because
/// that one is `set_button(1)`. A synthetic `released` cannot name a button (see [`press_left`]), so this
/// drives the SAME conversion the widget uses and then asks the gate the handler asks.
fn f2_12_s6_a_right_click_holds_a_card_too(app: &adw::Application) {
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 90.0), card(30.0, 4.0, "assets/tier.svg")],
        ..Default::default()
    };
    let window = cut_page(app, &tape(), &seeded);
    assert!(hold::holds_card(3, 1), "one right press is a hold");
    // Same second, same span, same `press` the gesture forwards to: a first press is a hold, so it holds.
    let said = hold::press(
        &window,
        ui::cut_card_hold::seconds_at_for_test(&window, x_of(&window, 32.0)),
    );
    let held = ui::held_clip().expect("a first right press takes the card up");
    assert_eq!(held.ins, "assets/tier.svg");
    assert!(
        said.contains("Edit"),
        "the sentence names the new word: {said}"
    );
    assert_eq!(word(&window, "insert-button"), "Edit");
    assert!(hold::holds_card(1, 2), "a second left click is a hold");
    assert!(
        !hold::holds_card(1, 1),
        "a lone left click is the line's, not this rule's"
    );
    window.close();
}

/// S6's other half: a press on footage holds NOTHING and changes nothing — a card nobody aimed at must not
/// drop a card someone already holds.
fn f2_12_s6_a_press_on_footage_holds_nothing(app: &adw::Application) {
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 90.0), card(30.0, 4.0, "assets/tier.svg")],
        ..Default::default()
    };
    let window = cut_page(app, &tape(), &seeded);
    // Take a card up first: the refusal must leave THAT hand alone, not merely hold nothing new.
    press_left(&window, x_of(&window, 31.0), 2);
    let before = ui::held_clip().expect("the card is in hand before the miss");

    press_left(&window, x_of(&window, 10.0), 2);

    assert_eq!(
        ui::held_clip().map(|seg| seg.ins).as_deref(),
        Some("assets/tier.svg"),
        "a miss keeps the hand"
    );
    assert_eq!(ui::held_clip().unwrap().s, before.s);
    assert_eq!(
        status_text(&window),
        hold::NOTHING,
        "the refusal is the module's sentence, verbatim"
    );
    assert_eq!(
        word(&window, "insert-button"),
        "Edit",
        "and a miss does not un-word the button either"
    );
    window.close();
}

/// S6 through the BUTTON's own predicate: a card held by the gesture is a card to `cut_insert::hold`, which
/// is the one door `refresh_insert_button` reads the `Edit` word from. The word is not free text on the
/// button, so proving the pair here is what makes the gesture's work load-bearing rather than cosmetic.
fn f2_12_s6_a_held_card_presses_as_edit(app: &adw::Application) {
    let seeded = cut::Cut {
        segs: vec![clip(0.0, 90.0), card(30.0, 4.0, "assets/tier.svg")],
        ..Default::default()
    };
    let window = cut_page(app, &tape(), &seeded);
    press_left(&window, x_of(&window, 31.0), 2);
    let held = ui::held_clip().expect("the double click held the card");
    assert!(
        naivepost::cut_insert::hold(Some(&held)),
        "the press's own predicate must call a card held by the gesture a card: {held:?}"
    );
    assert_eq!(
        word(&window, "insert-button"),
        naivepost::cut_insert::edit_verb(true)
    );
    // And the hand is the card the band was aimed at, not the footage around it: same `s`, same file.
    // The hand is the card the band was aimed at, not the footage around it: same `s`, same file.
    assert_eq!(
        (held.s, held.e, held.dur),
        (30.0, 30.0, 4.0),
        "the spliced card, whole: {held:?}"
    );
    // With the card gone from the hand the same predicate stops answering yes, so the word follows the hand
    // back -- which is the other side of the same door the gesture pressed.
    ui::set_held_clip(None);
    ui::refresh_insert_button(&window);
    assert!(
        !naivepost::cut_insert::hold(ui::held_clip().as_ref()),
        "no hand, no card"
    );
    assert_eq!(
        word(&window, "insert-button"),
        "Insert",
        "the word follows the hand back"
    );
    window.close();
}

/// The gate the handler asks, stated once: `button == 3 || clicks >= 2`. A double RIGHT click answers it on
/// both releases and therefore holds the same card twice, which is what S6's silence about the right button's
/// click count allows; a LONE left click answers false, because that press belongs to F2.4's line.
fn f2_12_s6_the_gate_is_the_rule(app: &adw::Application) {
    assert!(hold::holds_card(3, 1), "one right press is a hold");
    assert!(
        hold::holds_card(3, 2),
        "a double right press is still a hold — holds the same card twice"
    );
    assert!(hold::holds_card(1, 2), "a second left click is a hold");
    assert!(hold::holds_card(1, 3), "and a third is");
    assert!(
        !hold::holds_card(1, 1),
        "a lone left click is the line's, not this rule's"
    );
    assert!(
        !hold::holds_card(0, 1),
        "no button and one click is nothing (a synthetic emit's answer)"
    );
    // The refusal sentence, once, with the word for what it is not aimed at.
    assert!(hold::NOTHING.contains("footage"), "{}", hold::NOTHING);
    let _ = app;
}

fn window_round() {
    common::one_app_round(|app| {
        f2_12_s6_a_double_click_on_a_card_holds_it(app);
        RAN_DOUBLE.store(true, Ordering::SeqCst);
        f2_12_s6_a_right_click_holds_a_card_too(app);
        RAN_RIGHT.store(true, Ordering::SeqCst);
        f2_12_s6_a_press_on_footage_holds_nothing(app);
        RAN_FOOTAGE.store(true, Ordering::SeqCst);
        f2_12_s6_a_held_card_presses_as_edit(app);
        RAN_PRESSED.store(true, Ordering::SeqCst);
        f2_12_s6_the_gate_is_the_rule(app);
        RAN_RULE.store(true, Ordering::SeqCst);
    });
}

#[test]
fn f2_12_s6_hold_card_wire() {
    // Pin cwd to our own temp root BEFORE anything builds: the presses resolve the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-cardhold-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    window_round();
    assert!(
        RAN_DOUBLE.load(Ordering::SeqCst),
        "the double-click check never ran"
    );
    assert!(
        RAN_RIGHT.load(Ordering::SeqCst),
        "the right-click check never ran"
    );
    assert!(
        RAN_FOOTAGE.load(Ordering::SeqCst),
        "the footage-miss check never ran"
    );
    assert!(
        RAN_PRESSED.load(Ordering::SeqCst),
        "the held-card Edit check never ran"
    );
    assert!(RAN_RULE.load(Ordering::SeqCst), "the gate check never ran");
}
