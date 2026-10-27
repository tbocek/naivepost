//! §08-produce#1-screen's plates, painted without decoding (§03-shell §1): a plate's draw func paints the
//! picture a worker decoded and kept, and a picture nobody decoded yet is asked for, not decoded there. The
//! first rewrite decoded every full-size JPEG on every paint, the thumbnail and up to eight slots each time
//! the page repainted, on the window thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;

static RAN: AtomicBool = AtomicBool::new(false);

#[test]
fn sec_08_produce_1_screen_a_plate_paint_never_decodes() {
    let root = common::temp_dir_created("sec08-plate");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    common::one_app_round(move |app| {
        // One footage recording, so the page has a frames folder to read `frame:` entries from.
        let model = naivepost::project::Project {
            sources: vec![naivepost::project::Source {
                path: "project:sources/2026-10-25 10-39-43.mkv".to_string(),
                footage: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let window = ui::build_window(app, &model, naivepost::shell::Page::Produce.label());
        window.present();
        common::settle();
        // The slot names a picture that is not on disk yet: the page's own paint finds nothing to ask for.
        let picture = root.join("frame-one.png");
        let mut page = ui::produce_page::read_state();
        page.frames = vec![picture.display().to_string()];
        ui::produce_page::set_state(page);
        ui::produce_page::refresh(&window);
        common::settle();
        naivepost::produce_upload_print::flat_png_for_test(&picture, 640, 360, 0.3)
            .expect("a picture for the slot");

        assert!(
            !ui::produce_paint::paint_slot_for_test(&window, 0),
            "the first paint of a picture only asks a worker for it: it decoded nothing"
        );
        assert!(
            !ui::produce_paint::picture_held_for_test(&picture),
            "and nothing is held until the decode lands"
        );
        assert!(
            ui::background::settle_for_test(Duration::from_secs(60)),
            "the decode's worker answered"
        );
        assert!(
            ui::produce_paint::picture_held_for_test(&picture),
            "the decoded picture is held for every later paint"
        );
        assert!(
            ui::produce_paint::paint_slot_for_test(&window, 0),
            "and the next paint draws it from what is held"
        );

        // A `frame:<seconds>` entry (what the spread and the model's pick write) shows the extracted frame
        // nearest that second, out of the recording's own frames folder, named by its real instant. The first
        // rewrite read the entry as a file name and painted the missing marker in every such slot.
        let tree = naivepost::layout::Tree::new(naivepost::startup::open_dir())
            .expect("the session folder");
        let zero = naivepost::clock::clock(&["2026-10-25 10-39-43.mkv"]).zero;
        let frames = tree.frames_dir("2026-10-25 10-39-43");
        std::fs::create_dir_all(&frames).expect("frames folder");
        for second in [90.0, 92.0] {
            naivepost::produce_upload_print::flat_png_for_test(
                &frames.join(format!(
                    "{}.jpg",
                    naivepost::clock::frame_name(zero + second)
                )),
                640,
                360,
                0.4,
            )
            .expect("an extracted frame");
        }
        let mut page = ui::produce_page::read_state();
        page.frames = vec!["frame:91.7".to_string()];
        ui::produce_page::set_state(page);
        ui::produce_page::refresh(&window);
        // The window's own draws only: the folder is listed on a worker and its landing repaints the plate,
        // whose decode lands and repaints it again. Waited for on the clock, since a busy machine draws late;
        // the picture is the frame at 92 s, the nearest to 91.7.
        let nearest = frames.join(format!("{}.jpg", naivepost::clock::frame_name(zero + 92.0)));
        common::wait_until(
            "a frame: slot painting the extracted frame nearest its second, not the missing marker",
            || ui::produce_paint::slot_shown_for_test(0).as_deref() == Some(nearest.as_path()),
        );
        RAN.store(true, Ordering::SeqCst);
        window.close();
        common::settle();
    });
    assert!(RAN.load(Ordering::SeqCst), "the plate check never ran");
}
