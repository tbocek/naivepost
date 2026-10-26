//! §07-narrate, every press through its REAL widget to its real effect, for the controls the button inventory
//! found tested only below the widget or not at all. One window round (one GTK main loop per test process):
//!
//! - F4.1 ▶ on the Narrate page asks the LIVE writer (a fake LLM over HTTP at the Settings address, no
//!   `set_narrate_script`) and the lines it wrote land in `narration.json` and on the page;
//! - F4.5 S5 selecting a line in `narrate-lines` jumps the preview to that line's lead-in;
//! - F4.5 S1 a click on the picture goes through its own gesture to the same press ▶ makes;
//! - §1 the wheel over the paused picture steps one frame, through the real scroll controller;
//! - F4.6 "Add file…" raises one chooser; its answer goes through the door the chooser's response runs.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::shell::Page;
use naivepost::ui;

#[path = "narrate_live_harness.rs"]
mod harness;

/// A one-second mono wav written with ffmpeg, to add as a voice.
fn wav(at: &std::path::Path) {
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:duration=1",
        ])
        .arg(at)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the wav was written");
}

/// F4.1: ▶ over the live writer.
fn check_play_asks_the_live_writer(window: &adw::ApplicationWindow, fixture: &harness::Fixture) {
    let llm = harness::FakeLlm::start(vec![harness::entries_answer(&[
        (0.0, 10.0, 1.0, "The chain moved on.", "calm"),
        (10.0, 20.0, 11.0, "And the energy use fell.", "calm"),
    ])]);
    harness::point_at(&llm.url());
    ui::play_button(window).expect("▶").emit_clicked();
    common::settle_workers();
    assert!(
        llm.drained() >= 1,
        "the press asked the writer over the wire"
    );
    let written = naivepost::narration::load(&fixture.tree).expect("narration.json was written");
    let texts: Vec<String> = written
        .entries
        .iter()
        .map(|entry| entry.text.clone())
        .collect();
    assert_eq!(
        texts,
        ["The chain moved on.", "And the energy use fell."],
        "the writer's lines are the narration"
    );
    assert_eq!(ui::read_state().entries.len(), 2, "and the page shows them");
}

/// F4.5 S5: a row selected jumps the preview to its line's lead-in.
fn check_row_select_jumps(window: &adw::ApplicationWindow) {
    ui::refresh(window);
    common::settle();
    let lines = common::widget_in(window, "narrate-lines")
        .and_then(|widget| widget.downcast::<gtk::ListBox>().ok())
        .expect("the line list");
    let second = lines.row_at_index(1).expect("a second row");
    let wanted = naivepost::narrate_preview_leg::lead_in_of(&ui::read_state().entries, 1)
        .expect("the second line has a lead-in");
    lines.select_row(Some(&second));
    common::settle();
    assert_eq!(
        ui::read_state().session,
        wanted,
        "the preview stands at the line's lead-in"
    );
}

/// §1: one notch over the paused picture steps one frame forward.
fn check_wheel_steps_a_frame(window: &adw::ApplicationWindow) {
    let picture = common::widget_in(window, "narrate-picture").expect("the picture");
    let wheel = picture
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|controller| controller.downcast::<gtk::EventControllerScroll>().ok())
        .find(|controller| controller.name().as_deref() == Some("wheel-narrate-picture"))
        .expect("the picture's wheel");
    let before = ui::read_state().session;
    let _: bool = wheel.emit_by_name("scroll", &[&0.0f64, &1.0f64]);
    common::settle();
    assert!(
        ui::read_state().session > before,
        "the notch walked the paused picture forward ({before} -> {})",
        ui::read_state().session
    );
}

/// F4.5 S1: the picture's click is the same press as ▶ (here: refused or started, but answered).
fn check_picture_click(window: &adw::ApplicationWindow) {
    let picture = common::widget_in(window, "narrate-picture").expect("the picture");
    let click = picture
        .observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .find_map(|controller| controller.downcast::<gtk::GestureClick>().ok())
        .expect("the picture answers clicks");
    common::status_text(window);
    let status = common::widget_in(window, "status").and_then(|w| w.downcast::<gtk::Label>().ok());
    if let Some(label) = &status {
        label.set_text("");
    }
    let before = ui::read_state().playing;
    click.emit_by_name::<()>("released", &[&1i32, &10.0f64, &10.0f64]);
    common::settle();
    let answered = ui::read_state().playing != before || !common::status_text(window).is_empty();
    assert!(answered, "the click reached the preview's press");
    if ui::read_state().playing {
        click.emit_by_name::<()>("released", &[&1i32, &10.0f64, &10.0f64]);
        common::settle();
        assert!(!ui::read_state().playing, "a second click pauses it");
    }
}

/// F4.6: "Add file…" raises one chooser; a pick is converted into the voices folder and offered, Cancel says so.
fn check_add_voice_file(window: &adw::ApplicationWindow, root: &std::path::Path) {
    let before = ui::narrate_voices::choosers_raised_for_test();
    common::narrate_button(window, "voice-add-file").emit_clicked();
    assert_eq!(
        ui::narrate_voices::choosers_raised_for_test(),
        before + 1,
        "one press, one chooser"
    );
    ui::narrate_voices::answer_chooser(window, None);
    assert_eq!(
        common::status_text(window),
        ui::narrate_voices::refused_no_file(),
        "Cancel copies nothing and says so"
    );
    // The voices folder this scenario owns, as a person sets it in Settings (never the machine's real one).
    let voices = root.join("voices");
    let paths = harness::settings_folder();
    let mut conf = naivepost::settings::read(&paths).expect("settings read");
    conf.voices = voices.display().to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");
    let picked = root.join("Ann Reads.wav");
    wav(&picked);
    ui::narrate_voices::answer_chooser(window, Some(picked));
    common::settle_workers();
    let folder = ui::narrate_voices::voices_folder();
    assert!(
        std::fs::read_dir(&folder)
            .map(|entries| entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".wav")))
            .unwrap_or(false),
        "the picked recording is in the voices folder {}",
        folder.display()
    );
    assert!(
        ui::read_state()
            .voice_files
            .iter()
            .any(|name| name.to_lowercase().contains("ann")),
        "and offered as a voice: {:?}",
        ui::read_state().voice_files
    );
}

#[test]
fn sec_07_narrate_every_press_reaches_its_effect() {
    static RAN: AtomicBool = AtomicBool::new(false);
    let fixture = harness::fixture("every-press");
    unsafe { std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/bin/true") };
    common::one_app_round(move |app| {
        naivepost::project::save(&fixture.project, fixture.tree.dir()).expect("naivepost.json");
        naivepost::startup::set_open_dir(fixture.tree.dir());
        let window = ui::build_window(app, &fixture.project, "Prepare");
        window.present();
        ui::tab_button(&window, Page::Narrate)
            .expect("the Narrate tab")
            .emit_clicked();
        common::settle_workers();
        check_play_asks_the_live_writer(&window, &fixture);
        check_row_select_jumps(&window);
        check_wheel_steps_a_frame(&window);
        check_picture_click(&window);
        check_add_voice_file(&window, &fixture.root);
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
