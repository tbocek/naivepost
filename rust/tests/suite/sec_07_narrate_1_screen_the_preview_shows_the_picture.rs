//! §07-narrate#1-screen **2**, the preview: paused it shows the frame Prepare extracted for the second under the
//! line, and while ▶ plays it shows the built-in player's own frames. The first rewrite looked the still up once,
//! while the page's recordings were still the stand-ins the cut implies (they have no frames), held "nothing" as
//! its answer and never looked again, so the preview was black at rest; and it spelled "nothing found" the same way
//! as "the player's frames are up", so when ▶ started the player its frames were never put on the picture either.

use std::path::Path;
use std::process::Command;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// One 4 s recording with picture and sound, a cut keeping all of it, and Prepare's frames for it every 0.25 s
/// under their stamps (`‹YYYY-MM-DD_HH-MM-SS.mmm›.jpg`).
fn project(root: &Path) -> std::path::PathBuf {
    let dir = root.join("preview.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=8:duration=4",
        ])
        .args(["-f", "lavfi", "-i", "sine=duration=4", "-shortest"])
        .arg(dir.join("sources/2026-10-25 10-00-00.mkv"))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/2026-10-25 10-00-00.mkv", "footage": true, "narrator": 1,
            "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.0, "e": 4.0}]}"#,
    )
    .unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    let frames = tree.frames_dir("2026-10-25 10-00-00");
    std::fs::create_dir_all(&frames).unwrap();
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=4:duration=4",
        ])
        .arg(frames.join("%02d.jpg"))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());
    for n in 1..=16u32 {
        let at = f64::from(n - 1) * 0.25;
        let stamp = format!(
            "2026-10-25_10-00-{:02}.{:03}.jpg",
            at as u32,
            ((at.fract()) * 1000.0) as u32
        );
        std::fs::rename(frames.join(format!("{n:02}.jpg")), frames.join(stamp)).unwrap();
    }
    dir
}

#[test]
fn sec_07_narrate_1_screen_the_preview_shows_the_frame_at_rest_and_the_player_while_playing() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("narrate-preview");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("loads");
        naivepost::startup::set_open_dir(&dir);
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        let video = common::find_named(&window, "narrate-video")
            .and_then(|w| w.downcast::<gtk::Picture>().ok())
            .expect("the preview's picture");
        common::wait_until(
            "the preview shows the frame under the line, at rest",
            || {
                common::settle_workers();
                video.paintable().is_some()
            },
        );

        let said = ui::narrate_page::press_preview_picture(&window);
        common::wait_until(
            &format!("the player's frames are on the preview ({said})"),
            || {
                common::settle();
                naivepost::narrate_preview_leg::picture_built_in()
                    && video.paintable().map(|p| p.upcast::<glib::Object>())
                        == naivepost::play_built_in::paintable()
            },
        );
        ui::narrate_page::press_preview_stop(&window);
        common::settle();
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the round never ran"
    );
}
