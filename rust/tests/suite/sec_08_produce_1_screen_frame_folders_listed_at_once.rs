//! §08-produce#1-screen: a slot's frame, and the listings it takes to find it. In a file of its own: a window test
//! pins the process's working directory.

use std::time::Duration;

use adw::prelude::*;
use naivepost::ui;

use crate::common;

/// A slot's frame needs every recording's listing. They are all asked for in one pass: one at a time, each landing
/// repainted the page and asked for the next, 9 s on a 125-recording lecture before the picture could show.
#[test]
fn sec_08_produce_1_screen_every_frame_folder_is_listed_in_one_pass() {
    let root = common::temp_dir_created("sec08-listing");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    common::one_app_round(move |app| {
        let stems = [
            "2026-10-25 10-39-43",
            "2026-10-25 10-40-43",
            "2026-10-25 10-41-43",
        ];
        let model = naivepost::project::Project {
            sources: stems
                .iter()
                .map(|stem| naivepost::project::Source {
                    path: format!("project:sources/{stem}.mkv"),
                    footage: true,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let window = ui::build_window(app, &model, naivepost::shell::Page::Produce.label());
        window.present();
        common::settle();
        let tree = naivepost::layout::Tree::new(naivepost::startup::open_dir())
            .expect("the session folder");
        for stem in stems {
            std::fs::create_dir_all(tree.frames_dir(stem)).expect("frames folder");
        }
        let mut page = ui::produce_page::read_state();
        page.frames = vec!["frame:90.00".to_string()];
        ui::produce_page::set_state(page);
        ui::produce_page::refresh(&window);
        let _ = ui::produce_paint::paint_slot_for_test(&window, 0);
        assert_eq!(
            ui::cut_picture_cache::folders_listing_for_test(),
            stems.len(),
            "every folder asked for at once"
        );
        ui::background::settle_for_test(Duration::from_secs(60));
    });
}
