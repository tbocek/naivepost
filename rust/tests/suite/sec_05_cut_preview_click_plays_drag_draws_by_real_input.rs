//! §06-effects F3.1, the rule after S5, and §05-cut F2.1 S2, by real input: on the Cut preview picture a click
//! (press and release without the pointer passing GTK's drag threshold) only plays and pauses, whatever is placed
//! under it, and a new zoom rectangle starts only once the pointer has travelled while pressed. The first rewrite
//! ran both on one click: the picture's toggle AND the zoom-framing press, so a click over a placed zoom started
//! playback and took the zoom (or opened a zoom form) in the same press.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::ui;

use crate::common;
use crate::common::real_input::{real_click, real_drag};

static RAN: AtomicBool = AtomicBool::new(false);

/// The recording the cut is made of: 24 s at 25 fps.
const A: &str = "2026-10-18 19-21-52";

fn project(root: &Path) -> PathBuf {
    let dir = root.join("preview-click.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let a = dir.join("sources").join(format!("{A}.mkv"));
    let made = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi"])
        .args(["-i", "testsrc=size=160x90:rate=25:duration=24"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=24",
            "-shortest",
        ])
        .arg(&a)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    // One zoom placed over the first five seconds, its box in the upper left of the picture, so the lower right
    // is clear of every box; past it the line stands where nothing is placed.
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 1.0, "e": 20.0}], "fx": [{"kind": "zoom", "t": 0.0, "dur": 5.0, "trans": 1.0, "tout": 1.0, "cx": 0.3, "cy": 0.3, "hf": 0.4}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("cut/line.json"), r#"{"t": 2.0}"#).unwrap();
    dir
}

fn playing(window: &adw::ApplicationWindow) -> bool {
    ui::preview_player(window).transport.playing
}

fn status(window: &adw::ApplicationWindow) -> String {
    ui::state(window).status
}

fn undo_lit(window: &adw::ApplicationWindow) -> bool {
    ui::line_step_button(window, "undo-button")
        .expect("the Cut page has ↶ Undo")
        .is_sensitive()
}

/// A click on the picture: plays or pauses as `playing_after` says, and frames nothing.
fn click_only_plays(
    window: &adw::ApplicationWindow,
    preview: &gtk4::Widget,
    at: (f64, f64),
    playing_after: bool,
    what: &str,
) {
    let fx = ui::review_cut_of(window).fx;
    real_click(preview, at.0, at.1, 1);
    common::settle();
    assert_eq!(
        playing(window),
        playing_after,
        "{what}: the click toggled play (status \"{}\")",
        status(window)
    );
    assert!(
        ui::zoom_form_open().is_none(),
        "{what}: the click opened no zoom form (status \"{}\")",
        status(window)
    );
    assert!(
        ui::held_effect().is_none(),
        "{what}: the click took no zoom in hand (status \"{}\")",
        status(window)
    );
    assert_eq!(
        ui::review_cut_of(window).fx,
        fx,
        "{what}: the click changed no effect"
    );
    assert!(!undo_lit(window), "{what}: the click added nothing to undo");
}

#[test]
fn sec_05_cut_a_click_on_the_preview_plays_and_a_drag_draws_by_real_input() {
    common::pin_private_cwd();
    std::env::remove_var("NAIVEPOST_PLAYER");
    let root = common::temp_dir("cut-preview-click");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.set_default_size(1240, 900);
        window.present();
        common::settle();
        common::settle_workers();
        let preview = common::widget_in(&window, "preview-panel").expect("the preview panel");
        let (w, h) = (f64::from(preview.width()), f64::from(preview.height()));
        assert!(!playing(&window), "a fresh preview is paused");
        assert!(!undo_lit(&window), "a fresh page has nothing to undo");

        // (3) Nothing placed under the line: a click plays, a second pauses.
        ui::set_playhead(&window, 10.0);
        common::settle();
        assert!(
            ui::cut_zoom_hand::framed_boxes(&ui::review_cut_of(&window), 10.0, (w, h)).is_empty(),
            "no zoom stands at 0:10"
        );
        click_only_plays(
            &window,
            &preview,
            (w * 0.8, h * 0.8),
            true,
            "no zoom, first click",
        );
        click_only_plays(
            &window,
            &preview,
            (w * 0.8, h * 0.8),
            false,
            "no zoom, second click",
        );

        // (1) A zoom under the line: a click clear of its box plays, a second pauses, and neither frames.
        ui::set_playhead(&window, 2.0);
        common::settle();
        assert!(
            !ui::cut_zoom_hand::framed_boxes(
                &ui::review_cut_of(&window),
                ui::cut_lane::paste_line(&window),
                (w, h)
            )
            .is_empty(),
            "the zoom stands under the line"
        );
        click_only_plays(
            &window,
            &preview,
            (w * 0.8, h * 0.8),
            true,
            "over a zoom, first click",
        );
        click_only_plays(
            &window,
            &preview,
            (w * 0.8, h * 0.8),
            false,
            "over a zoom, second click",
        );
        // ...and a click INSIDE the zoom's box is the same click: it plays and pauses, and takes nothing in hand.
        let fit =
            ui::cut_preview_frame::fit_size_of(&window, ui::PREVIEW_PANEL_W, ui::PREVIEW_PANEL_H);
        let line_at = ui::cut_lane::paste_line(&window);
        let (_, zoom_box) =
            ui::cut_zoom_hand::framed_boxes(&ui::review_cut_of(&window), line_at, fit)[0];
        let inside = (zoom_box.0 + zoom_box.2 / 2.0, zoom_box.1 + zoom_box.3 / 2.0);
        click_only_plays(
            &window,
            &preview,
            inside,
            true,
            "inside the zoom's box, first click",
        );
        click_only_plays(
            &window,
            &preview,
            inside,
            false,
            "inside the zoom's box, second click",
        );

        // (2) A drag clear of the box draws a new rectangle: its form opens, and play does not start.
        let fx = ui::review_cut_of(&window).fx;
        real_drag(&preview, (w * 0.6, h * 0.55), (w * 0.9, h * 0.9), 1);
        common::settle();
        assert!(
            ui::zoom_form_open().is_some(),
            "a drag clear of the box drew a new rectangle (status \"{}\")",
            status(&window)
        );
        assert!(!playing(&window), "the drag did not start play");
        assert!(ui::held_effect().is_none(), "the drag took no zoom in hand");
        assert_eq!(
            ui::review_cut_of(&window).fx,
            fx,
            "the drag wrote nothing until its form is applied"
        );
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the checks ran");
}
