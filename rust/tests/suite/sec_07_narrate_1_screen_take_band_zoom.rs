//! §07-narrate#1-screen **10**, the take band: the wheel zooms INSIDE the band (the window keeps its width and
//! the band gets its own scrollbar), and the band is measured in the RECORDING's seconds even when a voice
//! reference was built. The first rewrite made the zoomed width the scroller's minimum, so every notch widened the
//! window, and took the ruler from the built reference, so the red bar and ▶ meant different seconds.

use std::path::Path;
use std::process::Command;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::ui;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn project(root: &Path) -> std::path::PathBuf {
    let dir = root.join("band.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=60",
        ])
        .arg(dir.join("sources/voice.mkv"))
        .status()
        .expect("ffmpeg runs");
    assert!(made.success());
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/voice.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.0, "e": 60.0}]}"#,
    )
    .unwrap();
    // A built reference of 3 s: the band must not take its seconds from this.
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    let reference = naivepost::narrate_data::base_reference(&tree);
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    let wav = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=3",
            "-ac",
            "1",
            "-ar",
            "24000",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&reference)
        .status()
        .expect("ffmpeg runs");
    assert!(wav.success());
    dir
}

#[test]
fn sec_07_narrate_1_screen_the_take_band_zooms_inside_itself_in_recording_seconds() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("band-zoom");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("loads");
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &dir);
        window.set_default_size(1280, 900);
        window.present();
        common::wait_for_stable_layout(&window);
        common::settle();
        let seconds = ui::narrate_band::painted_seconds().expect("the band has seconds");
        assert!(
            (seconds - 60.0).abs() < 0.5,
            "the RECORDING's 60 s, not the reference's 3 s: {seconds}"
        );

        // The ruler is on the Cut page's tick ladder: at fit (60 s across the band, under 10 px a second) a tick
        // every 10 s, each one marked. The first rewrite ticked every 30 s at any zoom, so a recording under half
        // a minute had a bare ruler. Counted on the row just above the ruler's line, where only the ticks reach.
        let lane_w = common::find_named(&window, "take-band")
            .expect("the lane")
            .width();
        let (rgb, w, _, _, pps) =
            ui::narrate_band::painted_pixels_for_test(lane_w, 56).expect("the lane paints");
        let row = (naivepost::narrate_screen::BAND_RULER_PX as usize) - 2;
        let inked = |x: usize| {
            let at = (row * w as usize + x) * 3;
            rgb[at] > 100 && rgb[at + 1] > 100 && rgb[at + 2] > 100
        };
        let ticks = (1..w as usize)
            .filter(|&x| inked(x) && !inked(x - 1))
            .count();
        assert!(
            ticks >= 5,
            "a tick every 10 s across 60 s at {pps:.1} px/s, not every 30 s: {ticks} ticks"
        );

        let scroller = common::find_named(&window, "take-band-scroller")
            .and_then(|w| w.downcast::<gtk::ScrolledWindow>().ok())
            .expect("the band's scroller");
        let lane = common::find_named(&window, "take-band").expect("the lane");
        let (window_width, scroller_width) = (window.width(), scroller.width());
        let wheel = lane
            .observe_controllers()
            .iter::<glib::Object>()
            .flatten()
            .filter_map(|c| c.downcast::<gtk::EventControllerScroll>().ok())
            .find(|c| c.name().as_deref() == Some("wheel-take-band"))
            .expect("the band's wheel");
        for _ in 0..6 {
            // A mouse notch, as GTK hands it over: dy = -1 (the first rewrite's tests fired -50, as if dy were
            // pixels, which hid that a real notch zoomed by 0.4 %).
            let _: bool = wheel.emit_by_name("scroll", &[&0.0f64, &-1.0f64]);
            common::settle();
        }
        common::wait_for_stable_layout(&window);
        common::settle();
        assert!(
            ui::narrate_band::zoom() > 2.0,
            "the wheel zoomed: {}",
            ui::narrate_band::zoom()
        );
        assert_eq!(window.width(), window_width, "the window keeps its width");
        assert_eq!(scroller.width(), scroller_width, "and so does the band");
        let adjustment = scroller.hadjustment();
        assert!(
            lane.width() > scroller.width() && adjustment.upper() > adjustment.page_size() + 1.0,
            "the lane is wider than the band and scrolls: lane {} in {}, {} of {}",
            lane.width(),
            scroller.width(),
            adjustment.page_size(),
            adjustment.upper()
        );
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the round never ran"
    );
}
