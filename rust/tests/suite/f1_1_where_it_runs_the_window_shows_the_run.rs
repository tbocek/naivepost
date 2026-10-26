//! F1.1 "Where it runs" (spec/04-prepare.md): the Prepare walk runs on a worker, and the window shows it as it
//! goes. A real press on a project with two real videos: while the walk runs, the log on screen fills with its
//! lines and the bar moves; when it ends, the run closes (▶ again) and the log on screen carries its last word.
//! The first worker version stored the walk's lines and queue but never painted them, so the window showed an
//! empty log and a still bar until the end, and a worker that died left the bar open forever; the last
//! check proves that is gone too.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::common;
use adw::prelude::*;
use naivepost::project::{Project, Source};
use naivepost::ui;

/// A two-second test video, picture and sound, written with ffmpeg.
fn video(at: &std::path::Path) {
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=10:duration=2",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-shortest",
        ])
        .arg(at)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test video was written");
}

/// The text the log view on screen shows right now.
fn shown_log(window: &adw::ApplicationWindow) -> String {
    let view = ui::log_paint::log_view(window).expect("the window has its log view");
    let buffer = view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}

/// Turn the main loop for `wait`, or until `done` says so.
fn turn_until(wait: Duration, done: impl Fn() -> bool) -> bool {
    let until = Instant::now() + wait;
    let context = glib::MainContext::default();
    while Instant::now() < until {
        if done() {
            return true;
        }
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
    done()
}

#[test]
fn f1_1_where_it_runs_the_window_shows_the_run_as_it_goes() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = common::temp_dir_created("f11-shows");
        let dir = naivepost::startup::session_dir(&root);
        std::fs::create_dir_all(dir.join("sources")).expect("sources folder");
        std::env::set_current_dir(&root).expect("cwd pinned");
        for name in ["2026-10-25 10-28-46.mkv", "2026-10-25 10-29-52.mkv"] {
            video(&dir.join("sources").join(name));
        }
        let config = root.join("config");
        let paths = naivepost::settings::Paths {
            config_dir: config.join("naivepost"),
            data_dir: config.join("data"),
        };
        let mut conf = naivepost::settings::Conf::default();
        conf.audio_server = common::fake_audio_server();
        naivepost::settings::save(&paths, &conf).expect("conf written");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &config) };
        let project = Project {
            sources: ["2026-10-25 10-28-46.mkv", "2026-10-25 10-29-52.mkv"]
                .iter()
                .map(|name| Source {
                    path: format!("project:sources/{name}"),
                    footage: true,
                    ..Source::default()
                })
                .collect(),
            ..Project::default()
        };
        let window = ui::build_window(app, &project, "Prepare");
        window.present();
        // The log open, as the person had it.
        ui::window::log_expander(&window)
            .expect("the log expander")
            .set_expanded(true);
        ui::play_button(&window).expect("▶").emit_clicked();
        assert_eq!(
            ui::running_step(&window),
            Some(naivepost::run::Step::Prepare),
            "the press opened the run"
        );
        // While it runs, the log on screen shows the walk's own lines: they are there before the run closes.
        assert!(
            turn_until(Duration::from_secs(60), || shown_log(&window)
                .contains("input file(s)")
                || ui::running_step(&window).is_none()),
            "the walk's lines or its end reached the screen"
        );
        let during = shown_log(&window);
        assert!(
            ui::running_step(&window).is_some() && during.contains("input file(s)"),
            "the log on screen showed the walk while it ran, not only when it ended: {during}"
        );
        // And when it ends, the run closes and its ending is on screen.
        assert!(
            turn_until(Duration::from_secs(120), || ui::running_step(&window)
                .is_none()),
            "the run closed; on screen: {}",
            shown_log(&window)
        );
        let after = shown_log(&window);
        assert!(
            !after.contains("ended without an answer"),
            "the worker answered: {after}"
        );
        // F1.1 S5: the audio.cpp ask is said before it goes and when it lands, in that order, so a slow
        // server reads as a wait rather than a hang.
        let asking = after.find("asking audio.cpp at").expect("the ask is said");
        let answered = after
            .find("audio.cpp answered in")
            .expect("the answer is said");
        assert!(asking < answered, "said in order: {after}");
        // F1.6: a video whose sound outlasts its picture names no frame past the last picture.
        assert!(
            !after.contains("frames failed"),
            "the frames stage ran to the picture's end: {after}"
        );
        assert!(
            after.contains("requests:"),
            "the run's last word is on screen: {after}"
        );
        // §03-shell §1: a worker that dies without an answer does not leave the run standing on ⏸ for ever.
        if let (Some(bar), Some(queue)) = (ui::run_bar(&window), ui::run_queue(&window)) {
            naivepost::runqueue::start_run(
                &mut bar.borrow_mut(),
                &mut queue.borrow_mut(),
                &mut naivepost::exchanges::RunLog::new(),
                naivepost::run::Step::Prepare,
                naivepost::run::snapshot_sources(&project),
            );
        }
        assert!(ui::running_step(&window).is_some(), "a run stands open");
        // Another worker still in flight when the run's one dies (a readouts recount, say): the run is closed
        // when the last of them lands, not left open because one was busy at the moment of the death.
        naivepost::ui::background::run(
            "busy",
            || std::thread::sleep(std::time::Duration::from_millis(400)),
            |()| {},
        );
        naivepost::ui::background::run("doomed", || panic!("a worker that dies"), |()| {});
        common::settle_workers();
        assert!(
            ui::running_step(&window).is_none(),
            "the run with no worker behind it was closed"
        );
        assert!(
            shown_log(&window).contains("doomed: the worker ended without an answer"),
            "and the log on screen says why: ...{}",
            shown_log(&window)
                .chars()
                .rev()
                .take(400)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>()
        );
        // §03-shell §1: a live paint finds the widgets it draws on once per window, not at every worker note.
        // It walked the whole window each time, and on a real project's Cut page one paint held the window thread
        // for over 3 s (hang dump, 2026-10-26). Asked here, in this file's one round, where no other window is
        // built in between.
        naivepost::ui::run_close::paint_live();
        let after_first = naivepost::ui::run_close::live_searches_for_test();
        for _ in 0..50 {
            naivepost::ui::run_close::paint_live();
        }
        assert_eq!(
            naivepost::ui::run_close::live_searches_for_test(),
            after_first,
            "fifty more paints, no more searches"
        );
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
