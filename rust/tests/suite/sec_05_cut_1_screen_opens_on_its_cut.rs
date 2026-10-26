//! §05-cut#1-screen on a project that already has a cut: the page opens on it. The first port opened such a
//! project with an empty picture, `Segments: 0`, no thumbnails and one phantom `cam0` row, because the readings
//! and the picture were painted only after an edit, the recordings were named `….mkv` while Prepare files their
//! frames by stem, the picture indexed the recording list by row number, a frame was looked up by its second in
//! the file rather than its instant, and the live tape never decoded a thumbnail.

use std::path::{Path, PathBuf};
use std::process::Command;

use adw::prelude::*;
use naivepost::ui;

use crate::common;

/// Two stamped recordings, 55 s apart on the session clock, each 3 s long.
const A: &str = "2026-10-18 19-21-52";
const B: &str = "2026-10-18 19-22-47";

fn video(path: &Path) {
    let made = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=10:duration=3",
        ])
        .arg(path)
        .status()
        .expect("ffmpeg runs");
    assert!(made.success(), "the test clip was written");
}

fn frame(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::new(gtk4::gdk_pixbuf::Colorspace::Rgb, false, 8, 32, 18)
        .expect("a pixbuf");
    pixbuf.fill(0x3366ccff);
    pixbuf
        .savev(dir.join(name), "jpeg", &[])
        .expect("a frame written");
}

/// The project: both recordings as footage, A's frames under the old whole-second names, B's under today's
/// millisecond names, a cut of two scenes, and the red line saved inside B.
fn project(root: &Path) -> PathBuf {
    let dir = root.join("news.naivepost");
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    for name in [A, B] {
        video(&dir.join("sources").join(format!("{name}.mkv")));
    }
    let frames = dir.join("prepare/inputs/frames");
    for name in [
        "2026-10-18_19-21-52.jpg",
        "2026-10-18_19-21-53.jpg",
        "2026-10-18_19-21-54.jpg",
    ] {
        frame(&frames.join(A), name);
    }
    for name in ["47.000", "47.500", "48.000", "48.500", "49.000"] {
        frame(&frames.join(B), &format!("2026-10-18_19-22-{name}.jpg"));
    }
    std::fs::write(
        dir.join("naivepost.json"),
        format!(
            r#"{{"sources": [
                {{"path": "project:sources/{A}.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": []}},
                {{"path": "project:sources/{B}.mkv", "footage": true, "narrator": 0, "sepvoice": false, "tracks": []}}
            ]}}"#
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 0.5, "e": 2.5}, {"s": 55.5, "e": 57.5}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("cut/line.json"), r#"{"t": 56.6}"#).unwrap();
    dir
}

fn readout(window: &adw::ApplicationWindow, row: &str) -> String {
    ui::find_source_widget(window, &naivepost::cut_screen::readout_widget(row))
        .and_then(|w| w.downcast::<gtk4::Label>().ok())
        .map(|label| label.text().to_string())
        .unwrap_or_default()
}

fn shown_name() -> String {
    ui::cut_picture::frame_shown_for_test()
        .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

#[test]
fn sec_05_cut_1_screen_a_project_opens_on_its_cut() {
    common::pin_private_cwd();
    let root = common::temp_dir("opens-on-cut");
    let dir = project(&root);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Cut", &dir);
        window.present();
        common::settle();
        assert!(
            naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(60)),
            "the workers answered"
        );

        assert_eq!(
            readout(&window, "Segments"),
            "2",
            "the readings count the cut it opened on"
        );
        assert_ne!(readout(&window, "Cut"), "00:00", "and its length");

        let bases: Vec<String> = ui::page_recordings()
            .iter()
            .map(|r| r.base.clone())
            .collect();
        assert_eq!(
            bases,
            vec![A.to_string(), B.to_string()],
            "the recordings by the stem Prepare files frames under, and no `cam0` row beside them"
        );

        assert_eq!(
            shown_name(),
            "2026-10-18_19-22-48.500.jpg",
            "the picture at the restored line: B, 1.6 s into its file"
        );
        // §1's ONE red line: the restored second is the preview's playhead too, so ▶ plays from it and the band
        // paints the line where the picture is.
        assert_eq!(
            ui::preview_player(&window).playhead,
            Some(56.6),
            "the restored line is the preview's playhead"
        );
        ui::cut_picture::note_line_moved(&window, 1.4);
        // The frame decodes on a worker (§03-shell §1, §05-cut §1 Pictures): it is up once it lands.
        assert!(naivepost::ui::background::settle_for_test(
            std::time::Duration::from_secs(60)
        ));
        assert_eq!(
            shown_name(),
            "2026-10-18_19-21-53.jpg",
            "A's frame at 1.4 s into it, under its whole-second name"
        );
        assert_eq!(
            ui::preview_player(&window).playhead,
            Some(1.4),
            "a line moved by a press moves the playhead with it"
        );
        // And the other way: what ▶'s tick writes (`set_playhead`) is the line, and the picture follows it.
        ui::set_playhead(&window, 56.6);
        assert_eq!(
            ui::line_position(&window).t,
            56.6,
            "the tick moved the line"
        );
        assert_eq!(
            shown_name(),
            "2026-10-18_19-22-48.500.jpg",
            "and the picture at the line moved with it while playing"
        );

        let started = std::time::Instant::now();
        while ui::cut_picture_rows::thumbs_for_test().0 == 0
            && started.elapsed() < common::WAIT_LIMIT
        {
            common::settle();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            ui::cut_picture_rows::thumbs_for_test().0 > 0,
            "the live tape decodes its thumbnails in a worker"
        );
        window.close();
    });
}
