//! §05-cut §1 / §A: the toolbar's zoom − / + and the form column's 🖼− / 🖼+ change what the tracks band
//! PAINTS. A real Cut page over real footage, the real buttons clicked, and after every press the band's own
//! scene (`cut_view::band_scene_for`, the painter's input) and its fitted height are read: zoom moves the
//! pixels a second the band is drawn at, the thumbnail size moves the height of every picture row. At either
//! end of a ladder the press says it can go no further. The first rewrite's 🖼 buttons only rewrote their own
//! label (the size they set was read by nothing), and its zoom buttons measured their floor over an old 480 px
//! placeholder strip instead of the band the person looks at.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::project::{Project, Source};
use naivepost::shell::Page;
use naivepost::ui;

/// A test video `seconds` long, tiny and at one frame a second so it is written in a moment.
fn video(at: &std::path::Path, seconds: u32) {
    let made = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("testsrc=size=160x90:rate=2:duration={seconds}"))
        .arg(at)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test video was written");
}

fn status(window: &adw::ApplicationWindow) -> String {
    ui::state(window).status
}

fn band_height(window: &adw::ApplicationWindow) -> i32 {
    common::widget_in(window, "track-band")
        .expect("the page draws its tracks on track-band")
        .size_request()
        .1
}

fn press(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the page carries {name}"))
        .emit_clicked();
    common::settle_workers();
}

/// The session's length: short enough that its fit on a desktop-wide band is ABOVE the 4 px/s open zoom, which
/// is the case where a stored zoom under the fit would paint the tape across a corner of the band.
const SECONDS: f64 = 120.0;

#[test]
fn sec_05_cut_1_screen_zoom_and_thumbnails_resize_the_band() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::one_app_round(|app| {
        let root = common::temp_dir_created("zoom-thumbs");
        let session = root.join("zoom.naivepost");
        std::fs::create_dir_all(session.join("sources")).expect("sources folder");
        video(
            &session.join("sources").join("2026-10-25 10-00-00.mkv"),
            SECONDS as u32,
        );
        let project = Project {
            sources: vec![Source {
                path: "project:sources/2026-10-25 10-00-00.mkv".to_string(),
                footage: true,
                ..Source::default()
            }],
            ..Project::default()
        };
        naivepost::project::save(&project, &session).expect("the project file");
        naivepost::startup::set_open_dir(&session);
        let window = ui::window::build_window_at(app, &project, "Prepare", &session);
        window.set_default_size(1400, 900);
        window.present();
        ui::tab_button(&window, Page::Cut)
            .expect("the Cut tab")
            .emit_clicked();
        common::settle_workers();
        common::settle();

        // At open a short session is drawn at the open zoom, under its fit: − cannot widen it further, and says so.
        let view = ui::cut_view::wheel_gestures_view(&window);
        let opened = ui::cut_view::band_scene_for(&window).pps;
        assert!(
            opened < view / SECONDS,
            "this session's fit is above the open zoom: {opened} px/s, fit {}",
            view / SECONDS
        );
        press(&window, "zoom-out-button");
        assert_eq!(
            ui::cut_view::band_scene_for(&window).pps,
            opened,
            "− never widens a band already showing the whole session"
        );
        assert!(
            status(&window).contains("the whole session is on screen"),
            "and says why it did nothing: \"{}\"",
            status(&window)
        );

        // Zoom +: every press draws the band at more pixels a second, until the ceiling says so.
        let mut painted = opened;
        let mut first = true;
        let mut at_limit = false;
        for _ in 0..40 {
            press(&window, "zoom-in-button");
            let now = ui::cut_view::band_scene_for(&window).pps;
            if now > painted + 1e-9 {
                // From under the fit, the first + goes straight to it: one press is always visible.
                if first {
                    assert!(
                        now >= view / SECONDS - 1e-6,
                        "the first + from under the fit lands on it: {now} px/s"
                    );
                    first = false;
                }
                painted = now;
                continue;
            }
            assert!(
                status(&window).contains("as close as it goes"),
                "a + that does not deepen the band says it is at the limit, not \"{}\" (band at {now} px/s)",
                status(&window)
            );
            at_limit = true;
            break;
        }
        assert!(at_limit, "the ladder reaches its ceiling in 40 presses");
        assert!(
            (painted - naivepost::cut_screen::ZOOM_MAX).abs() < 1e-6,
            "the ceiling is ZOOM_MAX, got {painted}"
        );

        // Zoom −: every press draws it at fewer, until the whole session is on the band.
        let mut at_floor = false;
        for _ in 0..60 {
            press(&window, "zoom-out-button");
            let now = ui::cut_view::band_scene_for(&window).pps;
            if now < painted - 1e-9 {
                painted = now;
                continue;
            }
            assert!(
                status(&window).contains("the whole session is on screen"),
                "a − that does not widen the view says so, not \"{}\" (band at {now} px/s)",
                status(&window)
            );
            at_floor = true;
            break;
        }
        assert!(at_floor, "the ladder reaches its floor in 60 presses");
        // The floor is the band's own fit: the session across the band the person sees, not across 480 px.
        assert!(
            (painted - view / SECONDS).abs() < 0.05,
            "the floor fits the session on the band: {painted} px/s over a {view} px view"
        );

        // 🖼+: every press makes each picture row taller, so the band grows; at 160 px it says so.
        let mut tall = band_height(&window);
        let mut thumb = ui::cut_tracks::thumb_px();
        assert_eq!(
            thumb,
            naivepost::cut_screen::THUMB_AT_OPEN,
            "the page opens at 64 px"
        );
        let mut topped = false;
        for _ in 0..10 {
            press(&window, "thumb-plus");
            let now = ui::cut_tracks::thumb_px();
            if now > thumb {
                let grown = band_height(&window);
                assert!(
                    grown > tall,
                    "a bigger thumbnail grows the band the page draws: {tall} -> {grown} px at {now} px"
                );
                tall = grown;
                thumb = now;
                continue;
            }
            assert_eq!(
                now,
                naivepost::cut_screen::THUMB_MAX,
                "the size stops at 160 px"
            );
            assert!(
                status(&window).contains("largest"),
                "a 🖼+ at the top says so, not \"{}\"",
                status(&window)
            );
            topped = true;
            break;
        }
        assert!(topped, "the thumbnail ladder reaches 160 px");
        let label =
            common::widget_in(&window, "cut-readout-thumbnails").expect("the Thumbnails readout");
        let mut child = label.first_child();
        let mut read = String::new();
        while let Some(node) = child {
            if let Some(text) = node.downcast_ref::<gtk::Label>() {
                read = text.text().to_string();
            }
            child = node.next_sibling();
        }
        assert_eq!(
            read, "160 px",
            "the readout says the size the band is drawn at"
        );

        // 🖼−: back down, the band shrinks each step, and 40 px is the bottom.
        let mut bottomed = false;
        for _ in 0..10 {
            press(&window, "thumb-minus");
            let now = ui::cut_tracks::thumb_px();
            if now < thumb {
                let shrunk = band_height(&window);
                assert!(
                    shrunk < tall,
                    "a smaller thumbnail shrinks the band: {tall} -> {shrunk} px"
                );
                tall = shrunk;
                thumb = now;
                continue;
            }
            assert_eq!(
                now,
                naivepost::cut_screen::THUMB_MIN,
                "the size stops at 40 px"
            );
            assert!(
                status(&window).contains("smallest"),
                "a 🖼− at the bottom says so, not \"{}\"",
                status(&window)
            );
            bottomed = true;
            break;
        }
        assert!(bottomed, "the thumbnail ladder reaches 40 px");
        RAN.store(true, Ordering::SeqCst);
        app.quit();
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
