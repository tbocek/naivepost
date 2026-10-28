//! §07-narrate#1-screen **2**, the preview of a recording Prepare extracted no frames for: dark at rest (nothing to
//! show), and the built-in player's frames once ▶ plays. The first rewrite held "no still here" in the same spelling
//! as "the player's frames are up", so on such a recording ▶ played the sound over a black preview.

use std::process::Command;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn sec_07_narrate_1_screen_the_preview_plays_the_player_where_no_still_was_extracted() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("narrate-no-stills");
    let dir = root.join("nostills.naivepost");
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
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("loads");
        naivepost::startup::set_open_dir(&dir);
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        common::settle_workers();
        common::settle();
        let video = common::find_named(&window, "narrate-video")
            .and_then(|w| w.downcast::<gtk::Picture>().ok())
            .expect("the preview's picture");
        assert!(
            video.paintable().is_none(),
            "no frame extracted, nothing at rest"
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
