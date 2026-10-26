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
        let model = naivepost::project::Project::default();
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
        RAN.store(true, Ordering::SeqCst);
        window.close();
        common::settle();
    });
    assert!(RAN.load(Ordering::SeqCst), "the plate check never ran");
}
