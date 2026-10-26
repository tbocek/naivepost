//! F2.4 S1 through the pointer: a REAL click (xdotool on the test's X display, GTK's own event route) on the
//! Cut band puts the red line at the clicked second, on the picture row, on the ruler and on the selection
//! band, zoomed in as a person works. The earlier tests emitted the click gesture's signal straight, which
//! skips the route a real press takes through every gesture on the band; the person's click did nothing.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use crate::common::real_input;
use adw::prelude::*;
use naivepost::project::{Project, Source};
use naivepost::shell::Page;
use naivepost::ui;

/// A 20 s test video, picture and sound.
fn video(at: &std::path::Path) {
    let made = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=4:duration=20",
        ])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=20",
            "-shortest",
        ])
        .arg(at)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test video was written");
}

fn line(window: &adw::ApplicationWindow) -> f64 {
    ui::preview_player(window).playhead.unwrap_or(-1.0)
}

/// Click the band at second `t` and height `y`, and say where the line went.
fn click_at(window: &adw::ApplicationWindow, band: &gtk4::Widget, t: f64, y: f64) -> f64 {
    let scene = ui::cut_view::band_scene_for(window);
    let x = ui::cut_tracks::band_x(&scene, t) - ui::cut_view::origin();
    real_input::real_click(band, x, y, 1);
    common::settle_workers();
    line(window)
}

#[test]
fn f2_4_s1_a_real_click_on_the_band_places_the_line() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = common::temp_dir_created("real-click-line");
        let session = root.join("click.naivepost");
        std::fs::create_dir_all(session.join("sources")).expect("sources folder");
        video(&session.join("sources").join("2026-10-25 10-28-46.mkv"));
        let project = Project {
            sources: vec![Source {
                path: "project:sources/2026-10-25 10-28-46.mkv".to_string(),
                footage: true,
                ..Source::default()
            }],
            ..Project::default()
        };
        naivepost::project::save(&project, &session).expect("the project file");
        naivepost::startup::set_open_dir(&session);
        let window = ui::window::build_window_at(app, &project, "Prepare", &session);
        window.set_default_size(1240, 900);
        window.present();
        ui::tab_button(&window, Page::Cut)
            .expect("the Cut tab")
            .emit_clicked();
        common::settle_workers();
        common::settle();
        // Zoomed in, as the person had it.
        for _ in 0..4 {
            ui::line_step_button(&window, "zoom-in-button")
                .expect("zoom +")
                .emit_clicked();
            common::settle_workers();
        }
        let band = common::widget_in(&window, "track-band").expect("the tracks band");
        // The rows as the page lays them out: an effects lane is always drawn, even an empty one.
        let bands = ui::cut_tracks::bands_for(naivepost::fx_lane::row_count(
            &ui::review_cut_of(&window).fx,
        ));
        let picture_y = bands.first_picture + bands.row_height / 2.0;
        let ruler_y = (bands.ruler.0 + bands.ruler.1) / 2.0;
        let selection_y = (bands.selection.0 + bands.selection.1) / 2.0;
        // + zooms about the view's centre, so the band is scrolled now, as a zoomed-in band usually is: the
        // seconds are aimed at from the first one on screen.
        let first = ui::cut_view::origin() / ui::cut_view::band_scene_for(&window).pps;
        assert!(
            first > 1.0,
            "four steps in, the band is scrolled (first second on screen {first})"
        );
        for (what, y, t) in [
            ("the picture row", picture_y, first + 1.5),
            ("the ruler", ruler_y, first + 2.5),
            ("the selection band", selection_y, first + 3.5),
        ] {
            let landed = click_at(&window, &band, t, y);
            assert!(
                (landed - t).abs() < 0.3,
                "a real click on {what} at {t} s puts the red line there, got {landed} (status \"{}\")",
                ui::state(&window).status
            );
        }
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
