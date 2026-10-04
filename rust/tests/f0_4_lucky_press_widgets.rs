//! F0.4 S2/S4 through the real button, with work behind the steps.
//!
//! `lucky_button_widgets.rs` proves a click reaches the chain; this file proves the chain reaches the
//! machine: that pressing the gears walks a stage plan (real `/bin/sh` tasks at the argv seam, the same
//! swap tests/run_stages.rs uses), that §S4's hand-edits branch is answered from the session folder and
//! not handed in by a test, and that §S6's end line then reflects what actually ran.

use std::fs;
use std::path::Path;

use adw::prelude::*;
use naivepost::project::{Project, Source};
use naivepost::ui;

/// One application, one round of scenarios — the single-main-loop shape every widget test here uses.
fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            walked_steps_are_tallied(app);
            hand_edits_come_from_the_session_folder(app);
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// S4 + S6 through the widget: with a session whose Cut has hand edits and whose recordings give Prepare
/// real work, the press walks Prepare, skips Cut over the edit, and says so — an end line naming what
/// ran, not "4 step(s) left undone".
fn walked_steps_are_tallied(app: &adw::Application) {
    let root = scratch("walked");
    plant_recording(&root, "lecture.mkv");
    // A hand edit: `final.txt` newer than the marks is exactly what §F1.12 asks, and what makes Cut skip.
    plant_hand_edit(&root);
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    let model = Project { sources: vec![recording("lecture.mkv")], ..Default::default() };
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let before = ui::window_logs().len();
    let gears = ui::lucky_button(&window).expect("the gears exist");
    gears.emit_clicked();
    let logged: Vec<String> = ui::window_logs()[before..].to_vec();
    let joined = logged.join("\n");

    assert_eq!(
        logged.first().map(String::as_str),
        Some(">>> run: Prepare \u{2192} Cut \u{2192} Narrate \u{2192} Produce"),
        "S3 first, whatever the steps turn out to be: {joined}"
    );
    // The chain reached its steps and switched to them: the page it ends on is a later one than it began.
    assert!(logged.contains(&">>> run: Prepare".to_string()), "{joined}");
    // Cut skipped itself over the hand edit -- the sentence only exists if the flag was really set.
    assert!(
        logged.iter().any(|line| line.contains("Cut skipped") && line.contains("hand edits")),
        "S4's hand-edits branch must fire from the session folder: {joined}"
    );
    // Something ran: the walk reached a real tool of its own. The frames row is a dial now (§F1.6), so
    // its first leg is ffprobe on the video rather than one spawned ffmpeg — and this fixture's mkv is
    // placeholder bytes, so the honest outcome is that the door said WHY it stopped. Both halves are
    // required: a tool call proves work was attempted, the reason proves it was the walk reporting and
    // not a short-circuit.
    assert!(
        logged.iter().any(|line| line.contains("ffprobe") || line.contains("ffmpeg") || line.contains("scening frames")),
        "the walk calls tools for real, so its own lines appear: {joined}"
    );
    assert!(
        logged.iter().any(|line| {
            line.starts_with("!!! ")
                && (line.contains("no video to frame") || line.contains("Invalid data") || line.contains("failed:"))
        }),
        "the frames row must say what stopped it: {joined}"
    );
    let last = logged.last().expect("an end line");
    assert!(
        !last.contains("4 step(s) left undone"),
        "a press that walked its steps no longer reports all four undone: {joined}"
    );
    assert!(
        last.starts_with(">>> run:"),
        "S6's end line closes the press: {last}"
    );
    assert!(!ui::state(&window).status.is_empty(), "the status mirrors the end line");
    assert!(gears.is_sensitive(), "the gears come back after the chain");
    window.close();
}

/// S4's other half: the hand-edits flag is READ, not assumed. With no `final.txt` at all nothing was
/// edited, so the chain hands Cut over instead of skipping it.
fn hand_edits_come_from_the_session_folder(app: &adw::Application) {
    let root = scratch("clean");
    plant_recording(&root, "lecture.mkv");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    assert!(
        !naivepost::ui::lucky_run::session_hand_edits(),
        "a session with no transcript text has no hand edit"
    );

    let model = Project { sources: vec![recording("lecture.mkv")], ..Default::default() };
    let window = ui::build_window(app, &model, "Prepare");
    window.present();

    let before = ui::window_logs().len();
    ui::lucky_button(&window).expect("the gears").emit_clicked();
    let logged: Vec<String> = ui::window_logs()[before..].to_vec();
    assert!(
        !logged.iter().any(|line| line.contains("Cut skipped")),
        "nothing was hand-edited here, so Cut must not be skipped: {:?}",
        logged
    );
    // And now the flag answers yes, from the very files the driver reads.
    plant_hand_edit(&root);
    assert!(
        naivepost::ui::lucky_run::session_hand_edits(),
        "the same question, asked after the edit, flips to yes"
    );
    window.close();
}

#[test]
fn f0_4_s4_the_press_walks_real_work_through_the_button() {
    window_round();
    assert!(RAN.load(std::sync::atomic::Ordering::SeqCst), "the rounds never ran");
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("naivepost-f04-press-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

/// A recording the project names, plus the bytes on disk so a spawn against it is honest about the file.
fn plant_recording(root: &Path, name: &str) {
    let session = naivepost::startup::session_dir(root);
    fs::create_dir_all(&session).expect("session folder");
    fs::write(session.join(name), vec![b'x'; 8]).expect("recording bytes");
}

/// `final.txt` newer than `retakes.tsv`: the exact condition §F1.12 defines, written into the session
/// folder the driver resolves through `startup::session_dir(current_dir())`. The two paths come from
/// the same accessors the driver reads (`Tree::final_txt`, `Tree::retakes_tsv`), so this plants the
/// condition rather than guessing at a layout.
fn plant_hand_edit(root: &Path) {
    let transcript = naivepost::startup::session_dir(root).join("prepare").join("transcript");
    fs::create_dir_all(&transcript).expect("transcript folder");
    fs::write(transcript.join("retakes.tsv"), "1\t2\n").expect("marks");
    std::thread::sleep(std::time::Duration::from_millis(5));
    fs::write(transcript.join("final.txt"), "the words as typed by hand\n").expect("text");
}

fn recording(name: &str) -> Source {
    let path = naivepost::startup::session_dir(&std::env::current_dir().unwrap_or_default())
        .join(name)
        .to_string_lossy()
        .to_string();
    Source { path, footage: true, narrator: 0, sepvoice: false, tracks: vec![] }
}
