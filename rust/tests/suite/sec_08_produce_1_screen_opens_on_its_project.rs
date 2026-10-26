//! §08-produce#1-screen on a project that already states its encoder settings and its upload text: the page
//! opens on them, a changed setting is kept in the project (§01 §3's `produce`), and the next project opened does
//! not inherit them. The first port filled the page only from tests and the snapshot bench: a live page showed the
//! defaults, rendered with them, and carried one project's title into the next.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use naivepost::ui;

use crate::common;

fn project(root: &Path, name: &str, stated: bool) -> PathBuf {
    let dir = root.join(format!("{name}.naivepost"));
    std::fs::create_dir_all(&dir).unwrap();
    let body = if stated {
        r#"{"sources": [], "produce": {"container": "mkv", "codec": "h264", "preset": "veryslow",
            "resolution": "720p", "frame_rate": "24", "audio_kbps": 192, "subtitles": "none",
            "translate": [], "game_volume": 0.4, "crf": 20, "vfr": true, "mono": true, "blurred_edges": false},
            "publish": {"frames": [], "own": false, "thumb_title": "Episode 6", "title_seeded": true,
            "texts": [], "title": "Episode 6", "prompt": "the speaker smiling", "negative": "",
            "description": "What this week covers."}}"#
    } else {
        r#"{"sources": []}"#
    };
    std::fs::write(dir.join("naivepost.json"), body).unwrap();
    dir
}

#[test]
fn sec_08_produce_1_screen_the_page_opens_on_the_projects_settings_and_keeps_a_change() {
    common::pin_private_cwd();
    let root = common::temp_dir("produce-opens");
    let first = project(&root, "first", true);
    let second = project(&root, "second", false);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&first).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Produce", &first);
        common::settle();
        let state = ui::produce_page::read_state();
        assert_eq!(
            (
                state.container.as_str(),
                state.preset.as_str(),
                state.resolution.as_str(),
                state.frame_rate.as_str(),
                state.audio.as_str()
            ),
            ("mkv", "veryslow", "720p", "24", "192"),
            "the encoder rows are the project's"
        );
        assert!(
            state.vfr && state.mono && !state.blurred_edges,
            "and its ticks"
        );
        assert_eq!(
            state.title, "Episode 6",
            "the upload title is the project's"
        );
        assert_eq!(state.instruction, "the speaker smiling");

        // A change is kept in the project, every row of it: the render reads what the page shows.
        ui::produce_page::set_setting(&window, "Preset", "fast");
        let kept = naivepost::project::load(&first).expect("reloads");
        assert_eq!(
            naivepost::produce_render::preset_name(kept.produce.preset),
            "fast",
            "the changed preset is in naivepost.json"
        );
        assert_eq!(
            kept.produce.audio_kbps, 192,
            "and the rows it did not touch are as they were"
        );
        window.close();
        common::settle();

        let loaded = naivepost::project::load(&second).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Produce", &second);
        common::settle();
        let state = ui::produce_page::read_state();
        assert!(
            state.title.is_empty() && state.preset == "slow",
            "the second project opens on its own (default) page, not the first one's: {:?} {:?}",
            state.title,
            state.preset
        );
        window.close();
    });
}
