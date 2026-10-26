//! §07-narrate through the pointer: the picture's frame wheel and click, a line row, the slider and the take
//! band pressed, dragged and wheeled with REAL input (`common::real_input`: xdotool on the test's X display,
//! GTK's own event route), each effect asserted on the page's state.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;
use crate::common::real_input::{drain, real_click, real_click_centre, real_drag, real_scroll};

static RAN: AtomicBool = AtomicBool::new(false);

/// One stamped recording with picture and sound, 20 s, tagged as narrator 1, kept whole, with two lines.
fn project(root: &Path) -> PathBuf {
    let dir = root.join("narrate.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc=size=160x90:rate=25:duration=20")
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=20",
            "-shortest",
        ])
        .arg(dir.join("sources/2026-10-18 19-21-52.mkv"))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/2026-10-18 19-21-52.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.0, "e": 20.0}]}"#,
    )
    .unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    let line = |s: f64, e: f64, text: &str| naivepost::narration::Entry {
        s,
        e,
        text: text.into(),
        ..Default::default()
    };
    naivepost::narration::save(
        &naivepost::narration::Narration {
            entries: vec![
                line(2.0, 4.0, "First words"),
                line(12.0, 14.0, "Later words"),
            ],
            ..Default::default()
        },
        &tree,
    )
    .expect("the narration was written");
    dir
}

fn session() -> f64 {
    ui::narrate_page::read_state().session
}

/// §1 badge 5's clock, "session · cut/length", split into its three fields.
fn clock(window: &adw::ApplicationWindow) -> (String, String, String) {
    let text = common::find_named(window, "narrate-clock")
        .and_then(|w| w.downcast::<gtk4::Label>().ok())
        .map(|label| label.text().to_string())
        .expect("the clock");
    let (session, rest) = text.split_once(" \u{b7} ").expect("session · cut/length");
    let (cut, length) = rest.split_once('/').expect("cut/length");
    (session.into(), cut.into(), length.into())
}

/// The cut is kept whole from 0 s, so the clock's middle field (the finished video's second) is the session's.
fn assert_clock_follows(window: &adw::ApplicationWindow, after: &str) {
    let (at, cut, length) = clock(window);
    let want = naivepost::tools::mm_ss(session());
    assert!(
        at == want && cut == want && length == "00:20",
        "after {after} the clock reads {at} · {cut}/{length}, the line at {want} on a cut kept whole"
    );
}

#[test]
fn sec_07_narrate_every_surface_answers_real_input() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir_tagged("narrate-real-input");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        common::settle();
        common::settle_workers();

        // §1 a line row: a click on the second line cues the preview at its lead-in.
        let row = common::find_named(&window, "narrate-line-1").expect("the second line's row");
        real_click_centre(&row);
        let cued = session();
        assert!(
            cued > 8.0 && cued <= 12.0,
            "a click on the second line cued the preview before 12 s: {cued}"
        );
        assert_clock_follows(&window, "a line row's cue");

        // §1 the frame wheel over the paused picture steps one frame.
        let picture = common::find_named(&window, "narrate-picture").expect("the picture");
        let (pw, ph) = (f64::from(picture.width()), f64::from(picture.height()));
        drain(Duration::from_millis(300), Duration::from_secs(1));
        let before = session();
        real_scroll(&picture, pw / 2.0, ph / 2.0, 1, &[]);
        assert!(
            session() > before && session() - before < 0.2,
            "a wheel notch down over the paused picture stepped a frame forward: {before} -> {}",
            session()
        );
        assert_clock_follows(&window, "a wheel notch");

        // §1 the slider: a drag to the right scrubs the preview later.
        let slider = common::find_named(&window, "narrate-slider").expect("the slider");
        let (sw, sh) = (f64::from(slider.width()), f64::from(slider.height()));
        real_drag(&slider, (sw * 0.2, sh / 2.0), (sw * 0.8, sh / 2.0), 1);
        drain(Duration::from_millis(400), Duration::from_secs(2));
        assert!(
            session() > 14.0,
            "a slider drag to 80 % scrubbed the preview there: {}",
            session()
        );
        assert_clock_follows(&window, "a slider drag");

        // §1 the take band: a click puts the red bar there, a drag selects, a wheel notch zooms.
        let lane = common::find_named(&window, "take-band").expect("the take band");
        assert!(
            ui::narrate_band::recording_shown() && lane.is_mapped(),
            "the band shows the recording"
        );
        let (lw, lh) = (f64::from(lane.width()), f64::from(lane.height()));
        let bar = ui::narrate_band::red_bar();
        real_click(&lane, lw * 0.5, lh / 2.0, 1);
        assert!(
            (ui::narrate_band::red_bar() - bar).abs() > 0.5,
            "a click on the band moved the red bar ({bar} -> {})",
            ui::narrate_band::red_bar()
        );
        real_drag(&lane, (lw * 0.3, lh / 2.0), (lw * 0.45, lh / 2.0), 1);
        let picked = ui::narrate_band::live_selection();
        assert!(
            picked.is_some_and(|(a, b)| b - a > 1.0),
            "a drag on the band selected a stretch: {picked:?}"
        );
        let zoom = ui::narrate_band::zoom();
        real_scroll(&lane, lw * 0.5, lh / 2.0, -1, &[]);
        assert!(
            ui::narrate_band::zoom() != zoom,
            "a wheel notch over the band zoomed it ({zoom} -> {})",
            ui::narrate_band::zoom()
        );

        // §1 the picture: a click plays, a second click pauses.
        real_click(&picture, pw / 2.0, ph / 2.0, 1);
        assert!(
            ui::narrate_page::read_state().playing,
            "a click on the picture started the preview (status \"{}\")",
            ui::state(&window).status
        );
        real_click(&picture, pw / 2.0, ph / 2.0, 1);
        assert!(
            !ui::narrate_page::read_state().playing,
            "a second click paused it"
        );

        // §03-shell §2 on this page: with the preview paused, ▶ belongs to it (§11 §2, "the preview's transport
        // until ⏹") and plays on; ⏹ then ends the preview and hands ▶ back to the step.
        real_click_centre(&ui::play_button(&window).expect("the run bar's \u{25b6}"));
        assert!(
            ui::narrate_page::read_state().playing,
            "a real click on the bar's \u{25b6} played the paused preview on (status \"{}\")",
            ui::state(&window).status
        );
        let stop = ui::stop_button(&window).expect("the run bar's \u{23f9}");
        assert!(
            stop.is_sensitive(),
            "\u{23f9} is live while the preview plays"
        );
        real_click_centre(&stop);
        common::settle();
        let after = ui::narrate_page::read_state();
        assert!(
            !after.playing && !after.started,
            "a real click on \u{23f9} ended the preview (status \"{}\")",
            ui::state(&window).status
        );

        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
