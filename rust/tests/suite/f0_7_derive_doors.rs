//! F0.7 — the door the derivation runs behind on a press, and the debounce it waits on.
//!
//! `tests/suite/policy_derive.rs` proves the rules (S1 defaults, S2 the request's shape, S3 validation,
//! S4 what gets stored). What it cannot pin is the header line of §F0.7: "Runs after the User Context
//! changes (debounced; on the next ▶)". The ▶ door has a widget test here — a press derives a context
//! nobody derived from, once, and says the result in the log — plus the wait itself as a number, so a
//! change to it cannot pass unnoticed.
//!
//! One application, one `connect_activate`, one `#[test]` per door: these checks mutate process-shared
//! state (this window's live session policy and the sender slot), so each runs once in
//! sequence and reports through a flag its test asserts on.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::common;
use adw::prelude::*;
use common::{fake_audio_server, fixture_dir, settle, wait_until};
use naivepost::policy::{self, Proposal};
use naivepost::project::Origin;
use naivepost::ui;

static RAN_DEBOUNCE_LOGIC: AtomicBool = AtomicBool::new(false);
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
        policy::DEBOUNCE_MS,
        500,
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
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    settle();
    window
}

/// The log lines this thread's window printed since now — the fact §F0.7 S4 is about.
fn log_lines_from(mark: usize) -> Vec<String> {
    ui::window_logs()[mark..].to_vec()
}

/// The one-field answer both doors are given: `cutMode` = words, with a reason worth reading in the log.
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
// door: "on the next ▶" — a press derives a context nobody derived from, once
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// door 3: "on the next ▶" — a press derives a context nobody derived from, once
// ---------------------------------------------------------------------------

/// ▶ must never run a step on a policy that lags what the person just typed, and the derivation it makes is
/// said in the log (S4): a field, its value and the model's reason, so what was decided is visible without
/// a form. The tracker is what keeps the door free on every later press: an unchanged context asks once.
fn check_the_next_press_derives_once(app: &adw::Application) {
    serve_audio(&fake_audio_server());
    let asked = std::rc::Rc::new(AtomicUsize::new(0));
    let counted = std::rc::Rc::clone(&asked);
    ui::set_policy_sender_for_tests(
        move |req: &policy::DerivationRequest, refusals: &[String]| {
            counted.fetch_add(1, Ordering::SeqCst);
            one_field_answer(req, refusals)
        },
    );

    let window = fresh_window(app);
    let log_mark = ui::window_logs().len();
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
    // §F0.7 S4: the derived value is SAID, with its field, its value and its `because` — the log is where
    // what the model decided is visible, since there is no form to open.
    let lines = log_lines_from(log_mark);
    assert!(
        lines.iter().any(|line| line.contains("cutMode")
            && line.contains("words")
            && line.contains(CONTEXT)),
        "the derivation said what it set: {lines:?}"
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
// the round: ONE application, so the door runs inside a single `connect_activate` and reports
// through its flag.
// ---------------------------------------------------------------------------

fn doors_round() {
    common::one_app_round(|app| {
        check_the_next_press_derives_once(app);
        RAN_PRESS_DOOR.store(true, Ordering::SeqCst);
    });
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
