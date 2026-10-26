//! §05-cut#1-screen "Pictures": the film strip's pictures are cut from the recording itself, on the fly, at the
//! seconds the zoom asks for, and kept in the temp folder (`naivepost-thumbs/`), so the band has pictures before
//! Prepare ever ran and at any zoom. The first rewrite drew only Prepare's frames: a recording Prepare had not
//! reached was blank film, and between sparse frames the same picture stretched across many cells.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use adw::prelude::*;
use naivepost::ui;

use crate::common;

const A: &str = "2026-10-18 19-21-52";

/// A 4 s recording and a cut over it, and NO `prepare/` frames at all.
fn project(root: &Path) -> PathBuf {
    let dir = root.join("film.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x180:rate=25:duration=4",
        ])
        .arg(dir.join("sources").join(format!("{A}.mkv")))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [{{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": []}}]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.5, "e": 3.5}]}"#,
    )
    .unwrap();
    dir
}

#[test]
fn sec_05_cut_1_screen_the_film_strip_is_cut_from_the_recording_on_the_fly() {
    common::pin_private_cwd();
    let root = common::temp_dir("film-on-the-fly");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        // The band draws, asks for its cells' pictures, and repaints as they land: wait on the fact itself.
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(20) {
            common::settle();
            let (present, asked) = ui::cut_film_cache::counts_for_test();
            if present > 0 && asked == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let (present, asked) = ui::cut_film_cache::counts_for_test();
        assert!(
            present > 0,
            "pictures were cut from the recording with no Prepare frames on disk"
        );
        assert_eq!(asked, 0, "and every one asked for landed");
        let cached: Vec<PathBuf> = std::fs::read_dir(ui::cut_film_cache::cache_root())
            .expect("the cache folder is in the temp folder")
            .flatten()
            .flat_map(|folder| {
                std::fs::read_dir(folder.path())
                    .into_iter()
                    .flatten()
                    .flatten()
            })
            .map(|entry| entry.path())
            .collect();
        assert!(
            cached
                .iter()
                .any(|path| path.extension().is_some_and(|e| e == "jpg")),
            "they are kept as pictures under {}: {cached:?}",
            ui::cut_film_cache::cache_root().display()
        );
        assert!(
            cached
                .iter()
                .all(|path| !path.to_string_lossy().contains(".part.")),
            "and no half-written file is left: {cached:?}"
        );
        // A repaint asks nothing again: what is cut is answered from the cache.
        let before = ui::cut_film_cache::counts_for_test();
        ui::cut_picture_rows::repaint_bands();
        common::settle();
        assert_eq!(
            ui::cut_film_cache::counts_for_test().1,
            0,
            "a repaint cut nothing new"
        );
        assert_eq!(ui::cut_film_cache::counts_for_test().0, before.0);
        window.close();
    });
}

#[test]
fn sec_05_cut_1_screen_the_film_grid_follows_the_zoom() {
    use ui::cut_film_cache::{snapped, step_for, FINEST_STEP_S};
    assert_eq!(step_for(0.1), FINEST_STEP_S, "deep in, the finest grid");
    assert_eq!(step_for(0.3), 0.5, "doubled until one step covers a cell");
    assert_eq!(step_for(5.0), 8.0);
    // Zooming in by two keeps every second picture: a coarse grid's seconds are on the finer one too.
    for k in 0..20 {
        let coarse = snapped(k as f64 * 1.0, 1.0);
        assert_eq!(
            snapped(coarse, 0.5),
            coarse,
            "{coarse} s is on the 0.5 s grid too"
        );
    }
    assert_eq!(snapped(1.37, 0.25), 1.25);
}
