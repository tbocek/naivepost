//! §03-shell.md F0.13 Tests — through the widgets. The pass rules themselves are checked in
//! `tests/settings_tests.rs` against [`naivepost::checks`]; what these three assert is that a real
//! click on a named button reads the box beside it, paints its badge with the verdict as tooltip,
//! and mirrors the line into both logs (spec/03-shell.md §5).
//!
//! No server, binary or file is touched: each check installs a canned answer provider before it
//! clicks, so the dialog's probe is whatever this test says it was. That is also how S1 proves the
//! press tested the *typed* value rather than the file behind it, which lags by
//! [`naivepost::checks::CONF_SAVE_WAIT`].

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::checks;
use naivepost::ui;
use naivepost::ui::settings::Provider;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

/// One running GTK application for this test binary — the same single-main-loop arrangement
/// `tests/add_sources_widgets.rs` needs for the same reason: all three checks run in one
/// `connect_activate`, and each records that it got that far, so a check that never happened is a
/// failure rather than a quiet pass. GTK's main loop may only be started by the thread that will
/// run it, which is why the checks share one application.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            check_press_reads_the_typed_value(app);
            RAN_TYPED.store(true, Ordering::SeqCst);
            check_failure_keeps_its_reason(app);
            RAN_FAILED.store(true, Ordering::SeqCst);
            check_test_all_reports_every_kind(app);
            RAN_ALL.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN_TYPED: AtomicBool = AtomicBool::new(false);
static RAN_FAILED: AtomicBool = AtomicBool::new(false);
static RAN_ALL: AtomicBool = AtomicBool::new(false);

/// A provider whose every answer echoes what was typed, so a press that tested anything other than
/// the box's own text shows up as a wrong verdict string.
fn echoing() -> Provider {
    Rc::new(|_key, typed| Ok(format!("{typed} answered")))
}

/// A provider that fails one chosen key and passes the rest.
fn one_failure(fails: &'static str, reason: &'static str) -> Provider {
    Rc::new(move |key, _| {
        if key == fails {
            Err(reason.to_string())
        } else {
            Ok(format!("{key} is fine"))
        }
    })
}

/// Open the dialog over `window` answering from `provider`.
///
/// The gear's own press already opened one dialog wired to `no_probe`; opening again pushes a second
/// onto the thread-local stack, and every accessor resolves against the LAST one — so the canned
/// dialog supersedes the unwired one rather than replacing it. Deliberate: the gear has to stay
/// wired to something when nothing else is available, and a headless test cannot answer a chooser.
fn open_canned(window: &adw::ApplicationWindow, provider: Provider) {
    ui::open_settings(window, provider);
    assert!(ui::dialog_open(window), "the settings dialog is open");
}

/// S1 through the widgets: the press takes the value in the box, not the value in the file.
fn check_press_reads_the_typed_value(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    // The header bar carries the gear at all — without it there is no way to reach the dialog.
    let gear = ui::settings_button(&window).expect("the header bar has a Settings button");
    gear.emit_clicked();
    assert!(ui::dialog_open(&window), "clicking the gear opens the dialog");

    // A fresh dialog per check: the accessors resolve against the LAST dialog opened, so each check
    // installs its own provider this way rather than sharing one across all three.
    // The dialog the gear itself opened answers nothing, so a press on it says so rather than
    // showing a check mark nobody probed for. Read here, before the canned dialog supersedes it.
    ui::test_button(&window, "ffmpeg")
        .expect("the ffmpeg row has a Test button")
        .emit_clicked();
    let unwired = last_log();
    assert!(
        unwired.contains("nothing wired"),
        "S1: an unwired probe fails loudly, got {unwired:?}"
    );

    open_canned(&window, echoing());
    let typed = "/opt/ffmpeg/ffmpeg";
    // One key throughout: `wire_buttons` pairs `entry-ffmpeg` with `badge-ffmpeg` and
    // `test-ffmpeg-button`, so typing into that box is what the press reads back.
    ui::entry(&window, "ffmpeg")
        .expect("the ffmpeg row has its value box")
        .set_text(typed);
    ui::test_button(&window, "ffmpeg")
        .expect("the ffmpeg row has a Test button")
        .emit_clicked();

    let badge = ui::badge(&window, "ffmpeg").expect("the ffmpeg row has a badge");
    assert_eq!(badge.text().as_str(), "\u{2713}", "a passing probe marks \u{2713}");
    assert_eq!(
        badge.tooltip_text().as_deref(),
        Some(format!("{typed} answered").as_str()),
        "S1: the tooltip is the verdict, and the verdict names the TYPED path"
    );    assert_eq!(
        last_log(),
        format!("{} ffmpeg: {typed} answered", checks::LOG_PREFIX),
        "S1: the verdict is mirrored into the main log under settings'"
    );
}

/// S2 through the widgets: ✗ keeps its reason, and the first failure opens the dialog's log.
fn check_failure_keeps_its_reason(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    open_canned(&window, one_failure("ffmpeg", "not on PATH"));

    ui::test_button(&window, "ffmpeg")
        .expect("the ffmpeg row has a Test button")
        .emit_clicked();

    let badge = ui::badge(&window, "ffmpeg").expect("the ffmpeg row has a badge");
    assert_eq!(badge.text().as_str(), "\u{2717}", "a failing probe marks \u{2717}");
    assert_eq!(
        badge.tooltip_text().as_deref(),
        Some("not on PATH"),
        "S2: the mark says the row failed, the tooltip says why"
    );
    // Compared against checks' own rendering so the `--` shape of a failure is pinned by `checks`,
    // not copied into this file where it could drift.
    assert_eq!(
        last_log(),
        checks::log_line("ffmpeg", &Err("not on PATH".to_string())),
        "S2: the failure reaches the main log with its reason intact"
    );
    assert!(
        ui::dialog_log_open(&window),
        "S2: the first failure opens the dialog's own log and it stays open"
    );
    assert!(
        ui::dialog_log(&window).contains("ffmpeg"),
        "S2: the dialog's log holds the row that failed"
    );
}

/// "Test All" through the widgets: six kinds, six lines, in order, and the rows after the failing
/// one still carry their own \u{2713}.
fn check_test_all_reports_every_kind(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    open_canned(&window, one_failure("llm-vision", "it is not seeing the image"));

    let before = ui::window_logs().len();
    ui::test_button(&window, "all")
        .expect("the dialog has a Test All button")
        .emit_clicked();

    let logs = ui::window_logs();
    let mine: Vec<&str> = logs[before..].iter().map(String::as_str).collect();
    assert_eq!(mine.len(), 6, "S13: Test All reports all six kinds, got {mine:?}");
    for line in &mine {
        assert!(
            line.starts_with(checks::LOG_PREFIX),
            "S13: every Test All line is mirrored under settings', got {line:?}"
        );
    }
    let joined = mine.join("\n");
    let order = ["LLM:", "LLM vision --", "ffmpeg:", "firefox:", "audio.cpp:", "sd.cpp:"];
    let mut at = 0usize;
    for marker in order {
        let found = joined[at..]
            .find(marker)
            .unwrap_or_else(|| panic!("S13: {} missing or out of order in {joined:?}", marker));
        at += found + marker.len();
    }
    // The kinds AFTER the failing one carry their own marks, which is what shows nothing short-
    // circuited. Read as (key, badge text) pairs so a missing badge names its own row rather than
    // aborting inside a shared `expect`.
    let marks: Vec<(String, String)> = ["llm", "ffmpeg", "firefox", "audio", "sd"]
        .iter()
        .map(|key| {
            let text = ui::badge(&window, key)
                .unwrap_or_else(|| panic!("S13: no badge-{key} on the dialog"))
                .text()
                .to_string();
            ((*key).to_string(), text)
        })
        .collect();
    let not_ok: Vec<&(String, String)> = marks
        .iter()
        .filter(|(_, text)| text != "\u{2713}")
        .collect();
    assert!(
        not_ok.is_empty(),
        "S13: rows after the failure were still tested and passed; off: {not_ok:?} (all: {marks:?})"
    );
    assert_eq!(
        ui::badge(&window, "llm-vision")
            .expect("badge")
            .text()
            .as_str(),
        "\u{2717}",
        "S13: the failing kind keeps its own cross"
    );
}

/// The most recent line the window logged. Shared thread-local, so callers take the tail rather than
/// scanning everything earlier checks put there.
fn last_log() -> String {
    ui::window_logs()
        .last()
        .cloned()
        .expect("something was logged")
}

#[test]
fn f0_13_s11_a_press_reads_the_typed_value_and_marks_the_badge_through_the_widgets() {
    window_round();
    assert!(RAN_TYPED.load(Ordering::SeqCst), "the typed-value check never ran");
}

#[test]
fn f0_13_s12_a_failure_marks_the_cross_keeps_the_reason_and_opens_the_log() {
    window_round();
    assert!(RAN_FAILED.load(Ordering::SeqCst), "the failure check never ran");
}

#[test]
fn f0_13_s13_test_all_reports_every_kind_without_short_circuiting() {
    window_round();
    assert!(RAN_ALL.load(Ordering::SeqCst), "the Test All check never ran");
}
