//! F0.7 — the one door the derivation runs behind: a press of ▶. Typing asks no model.
//!
//! `tests/suite/policy_derive.rs` proves the rules (S1 defaults, S2 the request's shape, S3 validation,
//! S4 what gets stored). What it cannot pin is the header line of §F0.7: "Runs after the User Context
//! changes, on the next ▶". The ▶ door has a widget test here — a press derives a context nobody derived
//! from, once, and says the result in the log.
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

static RAN_PRESS_DOOR: AtomicBool = AtomicBool::new(false);

/// The context every check types: words the model can answer with one field, so an assertion about the
/// stored value says the derivation ran rather than that a default happened to match.
const CONTEXT: &str = "cut on the words, not the model";

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
    ui::set_session_context(CONTEXT);
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
        ui::policy_is_current(CONTEXT),
        "after the press the policy answers the context the step is about to use"
    );
    assert_eq!(
        ui::window_policy::live_session_policy().cut_mode.origin,
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
