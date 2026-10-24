//! §12-decisions#cut-and-effects entry 21, on the wire: the target length's outcome reaches the log
//! through the door F0.7 names ("on the next ▶").
//!
//! `sec_12_decisions_cut_and_effects.rs` proves what the reader says; this file proves the window prints
//! it. Only the unread case has a line of its own (a read that landed is said by F0.7's S4 line, which
//! `f0_7_derive_doors.rs` already checks from a press), so this is the one fact worth a widget here: the
//! program calls `policy::target_length_log` and the sentence lands in the shell's log while
//! `P.policy.targetLengthSeconds` stays at none. The door mechanics — the ask count, the tracker, the
//! derived-field line — are F0.7's, not repeated here.
//!
//! One application, one `connect_activate`, one `#[test]`: the check mutates process-shared state (this
//! thread's live session policy and the sender slot), so it runs once in sequence and reports through a
//! flag its test asserts on.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::policy::{self, Proposal};
use naivepost::ui;

use crate::common;
use common::{fake_audio_server, fixture_dir, settle, wait_until};

static RAN_GAP_DOOR: AtomicBool = AtomicBool::new(false);

/// The context: a length mentioned in words this build cannot spell in seconds, so the read lands as a gap.
const CONTEXT: &str = "make it the length of a train journey";

/// Point the settings this process reads at the loopback audio fake, so a real ▶ reaches the step body
/// instead of refusing at F1.1's preflight. Copied from `f0_7_derive_doors.rs`: the program resolves the
/// address through `settings`, which is the seam the spec's target asks for.
fn serve_audio(url: &str) {
    unsafe {
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-s12gap-cfg-{}", std::process::id()));
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

/// Lines the shell's log has printed since `mark`.
fn log_lines_from(mark: usize) -> Vec<String> {
    ui::window_logs()[mark..].to_vec()
}

/// The unread-length line: found by what it says, not by position, because other lines land around it.
fn gap_line(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .find(|line| line.contains("could not be read"))
        .cloned()
}

fn check_the_gap_line_prints_from_the_press_door(app: &adw::Application) {
    serve_audio(&fake_audio_server());
    // A model that proposes nothing: the only thing this derivation can say is what the reader made of the
    // length the person wrote, so any target line on the log came from the program's own print site.
    ui::set_policy_sender_for_tests(
        |_: &policy::DerivationRequest, _: &[String]| -> Vec<Proposal> { Vec::new() },
    );

    let window = fresh_window(app);
    let log_mark = ui::window_logs().len();
    ui::set_session_context(&window, CONTEXT);

    ui::play_button(&window)
        .expect("the run bar has a \u{25b6}")
        .emit_by_name::<()>("clicked", &[]);
    // The derivation is queued ahead of the step body but lands on the main loop: wait for the line itself.
    wait_until("the press-derived gap line reached the log", || {
        gap_line(&log_lines_from(log_mark)).is_some()
    });
    let line = gap_line(&log_lines_from(log_mark)).expect("waited for it");
    assert!(
        line.contains("stays none"),
        "the line says what the number is left at: {line}"
    );
    assert_eq!(
        ui::session_policy(&window).target_length_seconds.value,
        0.0,
        "P.policy.targetLengthSeconds: a length mentioned and unread leaves §10's none"
    );
    // And nothing claims a reading happened — the row this round removed must stay gone.
    let lines = log_lines_from(log_mark);
    assert!(
        !lines.iter().any(|line| line.contains("read from")),
        "no line may claim a read that understood nothing: {lines:?}"
    );

    // Leave the bar idle the way a person would, then drop the window before the round ends. The step the
    // press started is still running against the fake server, so stop it twice: the first press pauses,
    // and only an idle bar lets `close` mean anything.
    for _ in 0..2 {
        ui::stop_button(&window)
            .expect("the run bar has a \u{23f9}")
            .emit_by_name::<()>("clicked", &[]);
        settle();
    }
    window.close();
    settle();
    RAN_GAP_DOOR.store(true, Ordering::SeqCst);
}

#[test]
fn sec_12_decisions_cut_and_effects_s21_the_gap_line_prints_from_the_press_door() {
    common::pin_private_cwd();
    common::one_app_round(check_the_gap_line_prints_from_the_press_door);
    assert!(
        RAN_GAP_DOOR.load(Ordering::SeqCst),
        "the press-door check never ran inside activate"
    );
}
