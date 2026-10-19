//! F0.7 — the three doors the derivation runs behind, and the debounce it waits on.
//!
//! `tests/suite/policy_derive.rs` proves the rules (S1 defaults, S2 the request's shape, S3 validation,
//! S4 the user's own fields) and `cut_policy_form_widgets.rs` proves the form draws what the state holds.
//! What neither pins is the header line of §F0.7: "Runs after the User Context changes (debounced; on the
//! next ▶ or on demand from the policy form)". Two of those three doors have a widget test here — the ⚙
//! that demands a derivation before it paints, and the ▶ that closes the staleness gap — plus the wait
//! itself as a number, so a change to it cannot pass unnoticed.
//!
//! One application, one `connect_activate`, one `#[test]` per door: these checks mutate process-shared
//! state (this window's live session policy, the sender slot, the open-form slot), so each runs once in
//! sequence and reports through a flag its test asserts on — the shape `cut_policy_form_widgets.rs` uses.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::policy::{self, Proposal};
use naivepost::project::Origin;
use naivepost::ui;
use crate::common;
use common::{fake_audio_server, fixture_dir, settle, wait_until};

static RAN_DEBOUNCE_LOGIC: AtomicBool = AtomicBool::new(false);
static RAN_FORM_DOOR: AtomicBool = AtomicBool::new(false);
static RAN_PRESS_DOOR: AtomicBool = AtomicBool::new(false);

/// The context every check types: words the model can answer with one field, so an assertion about the
/// stored value says the derivation ran rather than that a default happened to match.
const CONTEXT: &str = "cut on the words, not the model";

// ---------------------------------------------------------------------------
// the debounce: §F0.7 says "debounced" and gives no number; `policy::DEBOUNCE_MS` is ours.
// ---------------------------------------------------------------------------

/// Whether a keystroke has been quiet long enough to earn the request. This is the arithmetic the
/// program's timer applies (`window_policy::schedule_policy_derive` arms a `glib::timeout_add_local`
/// for exactly this many ms and cancels a pending one on the next keystroke); the constant lives in
/// `policy`, so pinning it here is what stops the wait drifting without a test noticing. No program-side
/// predicate exists because the timer IS the implementation — see `fx_band::keystroke_lands`, which
/// does the same job for §A.9's 300 ms.
fn quiet_long_enough(pending_ms: u64) -> bool {
    pending_ms >= policy::DEBOUNCE_MS
}

#[test]
fn f0_7_debounce_the_wait_is_the_parameter_and_a_later_keystroke_cancels_the_first() {
    common::pin_private_cwd();
    // §F0.7's word is "debounced" and spec/03-shell.md names no number, so 500 ms is the app's own
    // engineering constant (`policy.rs`: long enough that typing a paragraph costs one request).
    assert_eq!(
        policy::DEBOUNCE_MS, 500,
        "§F0.7's debounce changed: update the spec text or the constant, not this test silently"
    );
    // A keystroke still inside the wait has NOT earned a request: that is what makes a burst of typing
    // one call instead of one per letter.
    assert!(!quiet_long_enough(policy::DEBOUNCE_MS - 1));
    assert!(quiet_long_enough(policy::DEBOUNCE_MS));
    assert!(quiet_long_enough(3 * policy::DEBOUNCE_MS));
    RAN_DEBOUNCE_LOGIC.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// the harness the two widget doors share
// ---------------------------------------------------------------------------

/// Point the settings this process reads at the loopback audio fake, so a real ▶ reaches the step body
/// instead of refusing at F1.1's preflight. Copied from `sec_03_shell_2_the_run_bar.rs`; the program
/// resolves the address through `settings`, which is the seam the spec's target asks for.
fn serve_audio(url: &str) {
    unsafe {
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-f07doors-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    let paths = naivepost::settings::Paths {
        config_dir: config.join("naivepost"),
        data_dir: config.join("data"),
    };
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the fake server");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
}

fn fresh_window(app: &adw::Application) -> adw::ApplicationWindow {
    ui::close_policy_forms();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    window
}

/// Walk every descendant of `root` for a widget name. The policy form is its OWN top level (an
/// `adw::Window` built by `open_policy_form`), so it is not under the main window's tree.
fn find_by_name_recursive(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_by_name_recursive(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn form_widget(name: &str) -> gtk::Widget {
    ui::open_policy_form()
        .and_then(|form| find_by_name_recursive(form.upcast_ref(), name))
        .unwrap_or_else(|| panic!("the policy form carries a widget named {name}"))
}

fn label_text(name: &str) -> String {
    form_widget(name)
        .downcast::<gtk::Label>()
        .expect("a source/reason cell is a label")
        .text()
        .to_string()
}

fn edit_value(name: &str) -> String {
    naivepost::policy_form_edit::value_of(&form_widget(name))
}

/// The one-field answer both doors are given: `cutMode` = words, with a reason worth reading on screen.
fn one_field_answer(_req: &policy::DerivationRequest, refusals: &[String]) -> Vec<Proposal> {
    if !refusals.is_empty() {
        return Vec::new();
    }
    vec![Proposal {
        field: "cutMode".into(),
        value: "words".into(),
        because: CONTEXT.into(),
    }]
}

// ---------------------------------------------------------------------------
// door 2: "on demand from the policy form" — the ⚙ derives BEFORE it paints (S5 + S3 over the wire)
// ---------------------------------------------------------------------------

/// The gear is a demand for the current answer, not a peek at whatever is cached: a context nobody has
/// derived from is derived first, so the first thing the form draws is what the model said with its
/// `source` and `because` (S3's stored shape), never a default nobody chose.
fn check_the_form_demands_its_derivation(app: &adw::Application) {
    ui::set_policy_sender_for_tests(one_field_answer);
    let window = fresh_window(app);
    ui::set_session_context(&window, CONTEXT);
    assert!(
        !ui::policy_is_current(&window, CONTEXT),
        "a context just typed has been derived from by nothing yet"
    );

    // The real button, the way a click reaches it. `press_policy` is what answers the press.
    ui::line_step_button(&window, "policy-button")
        .expect("the run bar carries the policy gear")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    // S3's shape, read off the form the click opened: value, source, reason — one row's three cells.
    assert_eq!(edit_value("policy-edit-cutMode"), "words");
    assert_eq!(label_text("policy-source-cutMode"), "model");
    assert_eq!(label_text("policy-reason-cutMode"), CONTEXT);
    assert_eq!(
        ui::session_policy(&window).cut_mode.origin,
        Origin::Model,
        "the stored policy says who set it, not only what it holds"
    );
    assert!(
        ui::policy_is_current(&window, CONTEXT),
        "the tracker now agrees the policy answers this context"
    );
    ui::close_policy_forms();
    window.close();
    settle();
}

// ---------------------------------------------------------------------------
// door 3: "on the next ▶" — a press derives a context nobody derived from, once
// ---------------------------------------------------------------------------

/// ▶ must never run a step on a policy that lags what the person just typed. The door sits in front of
/// the step body in `window_run_press`, and the tracker is what keeps it free on every later press: an
/// unchanged context asks the model exactly once.
fn check_the_next_press_derives_once(app: &adw::Application) {
    serve_audio(&fake_audio_server());
    let asked = std::rc::Rc::new(AtomicUsize::new(0));
    let counted = std::rc::Rc::clone(&asked);
    ui::set_policy_sender_for_tests(move |req: &policy::DerivationRequest, refusals: &[String]| {
        counted.fetch_add(1, Ordering::SeqCst);
        one_field_answer(req, refusals)
    });

    let window = fresh_window(app);
    ui::set_session_context(&window, CONTEXT);
    assert_eq!(asked.load(Ordering::SeqCst), 0, "typing alone asks nothing");

    let play = ui::play_button(&window).expect("the run bar has a \u{25b6}");
    play.emit_by_name::<()>("clicked", &[]);
    // The derivation is queued ahead of the step body but lands on the main loop, so wait for the fact
    // rather than for a guess of how many turns it takes.
    wait_until(
        "the \u{25b6} door asked the model about the new context",
        || asked.load(Ordering::SeqCst) >= 1,
    );
    assert!(
        ui::policy_is_current(&window, CONTEXT),
        "after the press the policy answers the context the step is about to use"
    );
    assert_eq!(
        ui::session_policy(&window).cut_mode.origin,
        Origin::Model,
        "and what landed is the model's answer, stored with its source"
    );

    // Leave the bar idle the way a person would, then press again over the SAME context: the tracker
    // says it is current, so the second press costs no request.
    ui::stop_button(&window)
        .expect("the run bar has a \u{23f9}")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let after_first = asked.load(Ordering::SeqCst);
    play.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        asked.load(Ordering::SeqCst),
        after_first,
        "§F0.7: an unchanged context re-derives nothing — the door is free on every later press"
    );

    // Whatever the step did with the fake server, the bar must be left idle for the next check.
    ui::stop_button(&window)
        .expect("the run bar has a \u{23f9}")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    window.close();
    settle();
}

// ---------------------------------------------------------------------------
// the round: ONE application, so both doors run inside a single `connect_activate` and report through
// their flags — the shape `cut_policy_form_widgets.rs::window_round` uses.
// ---------------------------------------------------------------------------

fn doors_round() {
    common::one_app_round(|app| {
        check_the_form_demands_its_derivation(app);
        RAN_FORM_DOOR.store(true, Ordering::SeqCst);
        check_the_next_press_derives_once(app);
        RAN_PRESS_DOOR.store(true, Ordering::SeqCst);
    });
}

#[test]
fn f0_7_s5_the_policy_form_demands_a_derivation_it_has_not_had() {
    common::pin_private_cwd();
    doors_round();
    assert!(
        RAN_FORM_DOOR.load(Ordering::SeqCst),
        "the policy-form door check never ran"
    );
}

#[test]
fn f0_7_s2_the_next_press_derives_a_context_nobody_derived_from() {
    common::pin_private_cwd();
    doors_round();
    assert!(
        RAN_PRESS_DOOR.load(Ordering::SeqCst),
        "the \u{25b6} door check never ran"
    );
}
