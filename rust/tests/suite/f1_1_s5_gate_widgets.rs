//! F1.1 S5 — the audio.cpp preflight reached through the real ▶ button.
//!
//! `tests/suite/f1_1_preflight.rs` proves the rule by calling `prepare_run::preflight` over a socket; this
//! binary proves the WIRE: that a press on a Prepare page is refused at the screen, before the run
//! bar opens, when the server the conf names does not answer. That path had no button test because
//! every other ▶ binary (`press_prepare_widgets`, `describe_stage_widgets`, `joins_widgets`,
//! `mark_retakes_widgets`) stands up a HEALTHY fake so its own stage lines can land — and a healthy
//! fake never reaches the refusal.
//!
//! A binary of its own for the same reason those files each have one: `XDG_CONFIG_HOME` belongs to the
//! whole process and the conf here names a DEAD port, the opposite of theirs. One application drives
//! the check: GTK's main loop may only be run once per thread.

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::layout::Tree;
use naivepost::project::Project;
use crate::common;
use common::footage;
use naivepost::{prepare_run, run, ui};

/// A root with `<root>/session.naivepost/` made and cwd pinned to the root — the arrangement
/// `tests/suite/press_prepare_widgets.rs::pin_session` uses: the ▶ handler resolves the project through
/// `startup::session_dir(current_dir())` at press time, not at build time.
fn pin_session(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!("naivepost-f11s5w-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = naivepost::startup::session_dir(&root);
    std::fs::create_dir_all(&dir).expect("session folder");
    std::env::set_current_dir(&root).expect("cwd pinned to the session root");
    let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
    (root, tree)
}

/// A port nothing listens on: every dial to it is refused, which is §S5's "not answering".
fn dead_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// Point the settings this process reads at `url`, and keep audio.cpp's own override variables out of
/// the way: [`services::audio_endpoint`] takes the conf first, but a box already carrying
/// `AUDIOCPP_SERVER` would answer for any press that never reached the conf. Cleared here so the refusal
/// below is provably about the address the conf names.
fn point_audio_at(url: &str) {
    unsafe {
        std::env::remove_var("AUDIOCPP_SERVER");
        std::env::remove_var("NAIVEPOST_TTS_URL");
    }
    let config = std::env::temp_dir().join(format!("np-f11s5w-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    let paths = naivepost::settings::Paths {
        config_dir: config.join("naivepost"),
        data_dir: config.join("data"),
    };
    let mut conf = naivepost::settings::Conf::default();
    conf.audio_server = url.to_string();
    naivepost::settings::save(&paths, &conf).expect("the conf points at the dead port");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
}

#[test]
fn f1_1_s5_a_press_with_no_audio_server_refuses_through_the_button() {
    static RAN: AtomicBool = AtomicBool::new(false);

    // The conf is process-wide, so it goes up before the window exists and stays up for the press.
    // `move` because GTK's `connect_activate` needs a `'static` closure; it copies the `String`.
    let dead = dead_port();
    point_audio_at(&dead);

    let app = adw::Application::builder()
        .application_id(ui::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        check_the_press_is_refused_at_the_screen(app, &dead);
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    app.run_with_args::<String>(&[]);

    assert!(RAN.load(Ordering::SeqCst), "the refusal check never ran");
}

/// One footage source: enough for S1 to pass, so any refusal below can only be S5's.
fn check_the_press_is_refused_at_the_screen(app: &adw::Application, dead: &str) {
    let (_keep, tree) = pin_session("dead");
    // S3's marker planted so the clear runs and consumes it: an uncleared `state.txt` arms the next
    // press in this process (F0.3), whose restart notice would land inside another check's log tail.
    std::fs::create_dir_all(tree.describe_dir("lecture")).expect("describe dir");
    std::fs::write(tree.describe_state("lecture"), b"halfway\n").expect("state.txt");
    let model = Project {
        sources: vec![footage("project:sources/lecture.mkv")],
        ..Project::default()
    };
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let play = ui::play_button(&window).expect("the run bar has a ▶");
    let before_tooltip = ui::play_tooltip(&window);
    assert_eq!(
        before_tooltip.as_deref(),
        Some(run::PLAY_TOOLTIP),
        "an idle ▶ says what §2 makes it say"
    );
    play.emit_clicked();

    // The sentence is `preflight_error`'s, aimed at the same facts the logic test asserts — not a
    // second wording invented for the row.
    let why = prepare_run::preflight_error(false, false, false, false, false).expect("a sentence");
    assert_eq!(
        ui::state(&window).status,
        why,
        "the status line carries §S5's refusal, from the port {dead}"
    );
    let logs = ui::window_logs();
    assert!(
        logs.contains(&format!("!!! {why}")),
        "the log says which server was missing: {logs:?}"
    );
    // §S5 sits between the opening lines and the first phase: S4 has already spoken, so the refusal
    // lands AFTER them rather than replacing them.
    let opening = logs
        .iter()
        .position(|line| line == ">>> prepare: 1 input file(s)")
        .unwrap_or_else(|| panic!("S4's opening line is missing from this press: {logs:?}"));
    let refused_at = logs
        .iter()
        .rposition(|line| line == &format!("!!! {why}"))
        .expect("the refusal line is in the log");
    assert!(
        refused_at > opening,
        "the preflight answers after the inputs are logged, before any phase runs: {logs:?}"
    );
    assert_eq!(
        naivepost::ui::run_close::last_run_step_for_test(),
        None,
        "a refused preflight never opened a run, so ⏸ was never shown"
    );
    assert_eq!(
        ui::state(&window).status,
        why,
        "and the sentence is still the row's last word after every repaint the press made"
    );
}
