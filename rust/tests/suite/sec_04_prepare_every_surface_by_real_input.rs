//! §04-prepare through the pointer: the sources list's row controls (🎥 footage, 🎤 voice, 🗣 split, the track menu
//! and its ticks, 🗑) and the run bar's ▶ pressed with REAL input (`common::real_input`: xdotool on the test's X
//! display, GTK's own event route), each effect asserted on the session. §03-shell §4 specifies the row; the list
//! lives on this page. The servers are pointed at a port nothing listens on, so ▶ ends on its own refusal.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::project::{Project, Source};
use naivepost::ui;

use crate::common;
use crate::common::real_input::{drain, real_click_centre};

static RAN: AtomicBool = AtomicBool::new(false);

/// No server answers here.
const NOBODY: &str = "http://127.0.0.1:1";

/// A 2 s video with `voices` audio streams.
fn video(at: &Path, voices: usize) {
    let mut command = std::process::Command::new("ffmpeg");
    command.args([
        "-v",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=160x120:rate=5:duration=2",
    ]);
    for n in 0..voices {
        command.args([
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={}:duration=2", 440 + 220 * n),
        ]);
    }
    for n in 0..=voices {
        command.args(["-map", &n.to_string()]);
    }
    let made = command
        .arg("-shortest")
        .arg(at)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test video was written");
}

fn point_settings_nowhere() {
    let paths = naivepost::settings::from_environment().expect("settings folder");
    let mut conf = naivepost::settings::Conf::default();
    conf.model = "nobody-model".to_string();
    conf.server = NOBODY.to_string();
    conf.audio_server = NOBODY.to_string();
    conf.sd_server = NOBODY.to_string();
    naivepost::settings::save(&paths, &conf).expect("settings written");
}

fn status(window: &adw::ApplicationWindow) -> String {
    ui::state(window).status
}

fn named(window: &adw::ApplicationWindow, name: &str) -> gtk4::Widget {
    common::find_named(window, name).unwrap_or_else(|| panic!("the page has {name}"))
}

fn source(window: &adw::ApplicationWindow, index: usize) -> Source {
    ui::session_handle(window).borrow().sources[index].clone()
}

#[test]
fn sec_04_prepare_every_surface_answers_real_input() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("prepare-real-input");
    let session = root.join("prepare.naivepost");
    std::fs::create_dir_all(session.join("sources")).expect("sources folder");
    video(&session.join("sources/2026-10-25 10-00-00.mkv"), 2);
    video(&session.join("sources/2026-10-25 10-10-00.mkv"), 1);
    point_settings_nowhere();
    common::one_app_round(move |app| {
        let model = Project {
            sources: ["2026-10-25 10-00-00.mkv", "2026-10-25 10-10-00.mkv"]
                .iter()
                .map(|name| Source {
                    path: format!("project:sources/{name}"),
                    footage: true,
                    narrator: 1,
                    ..Source::default()
                })
                .collect(),
            ..Project::default()
        };
        naivepost::project::save(&model, &session).expect("the project file");
        naivepost::startup::set_open_dir(&session);
        let window = ui::window::build_window_at(app, &model, "Prepare", &session);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        common::settle_workers();
        common::settle();

        // Each row's stream count is probed on a worker and the list is redrawn when it lands: pressed before
        // that, a click can land on a row the redraw is about to replace.
        common::wait_until("the two-stream row grows its track menu", || {
            common::find_named(&window, "track-0").is_some()
        });
        common::settle_workers();
        common::settle();

        // 🎥: a click takes the recording off footage, a second puts it back.
        let footage = named(&window, "footage-0");
        real_click_centre(&footage);
        assert!(
            !source(&window, 0).footage && status(&window).ends_with("not footage"),
            "a real click on 🎥 took footage off: \"{}\"",
            status(&window)
        );
        real_click_centre(&footage);
        assert!(source(&window, 0).footage, "a second click put it back");

        // 🎤: a click moves the row to the next voice and the face shows its number.
        real_click_centre(&named(&window, "narrator-0"));
        let voice = source(&window, 0).narrator;
        let face = named(&window, "narrator-face-0")
            .downcast::<gtk4::Label>()
            .expect("the face's number");
        assert!(
            voice != 1 && face.text() == voice.to_string(),
            "a real click on 🎤 moved the row to voice {voice}, the face reads {}",
            face.text()
        );

        // 🗣: a click asks for the voice to be split off on ▶, a second takes the wish back.
        let split = named(&window, "separate-0");
        real_click_centre(&split);
        assert!(
            source(&window, 0).sepvoice,
            "a real click on 🗣 asked for the split: \"{}\"",
            status(&window)
        );
        real_click_centre(&split);
        assert!(!source(&window, 0).sepvoice, "a second click took it back");

        // The track menu of the two-stream file: a click opens it, a click on the second tick hears both.
        let menu = named(&window, "track-0");
        real_click_centre(&menu);
        drain(Duration::from_millis(50), Duration::from_millis(400));
        // The ticks are named after ffprobe's file-wide index, so the second one is found by place.
        let list = menu
            .downcast_ref::<gtk4::MenuButton>()
            .and_then(|m| m.popover())
            .and_then(|p| common::find_in(p.upcast_ref(), "track-popover-0"))
            .expect("the track menu's list");
        let mut ticks: Vec<gtk4::Widget> = Vec::new();
        let mut child = list.first_child();
        while let Some(widget) = child {
            if widget.is::<gtk4::CheckButton>() {
                ticks.push(widget.clone());
            }
            child = widget.next_sibling();
        }
        let tick = ticks.get(1).cloned().expect("the second stream's tick");
        assert!(tick.is_mapped(), "a real click on the track menu opened it");
        real_click_centre(&tick);
        assert_eq!(
            source(&window, 0).tracks,
            vec![0, 1],
            "a real click on the second tick added its stream"
        );
        let menu = named(&window, "track-0");
        if let Some(popover) = menu
            .downcast_ref::<gtk4::MenuButton>()
            .and_then(|m| m.popover())
            .filter(|p| p.is_visible())
        {
            common::real_input::real_key(&window, "Escape");
            drain(Duration::from_millis(50), Duration::from_millis(400));
            assert!(!popover.is_visible(), "Escape closed the track menu");
        }

        // 🗑: a click takes the second recording out of the session.
        real_click_centre(&named(&window, "remove-1"));
        assert_eq!(
            ui::session_handle(&window).borrow().sources.len(),
            1,
            "a real click on 🗑 removed the row: \"{}\"",
            status(&window)
        );

        // F1.1 and §03-shell §2: ▶ reaches the Prepare run, which ends on its own refusal with nobody serving.
        let before = status(&window);
        real_click_centre(&ui::play_button(&window).expect("the run bar's \u{25b6}"));
        assert!(
            ui::background::settle_for_test(Duration::from_secs(60)),
            "the run's worker answered"
        );
        common::settle();
        assert!(
            status(&window) != before && ui::running_step(&window).is_none(),
            "a real click on \u{25b6} reached Prepare and its refusal closed the bar: \"{}\"",
            status(&window)
        );

        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
