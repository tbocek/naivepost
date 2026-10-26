//! §07-narrate#1-screen on a project that already holds a cut and a line: the page opens on THEM. The first port
//! filled the page's state only from tests and the snapshot bench, so a live window's Narrate page read
//! "00:00/00:00", offered no voice and said "the playhead is between clips" over a project with a cut; and the
//! state, one per process, carried the last project's lines into the next one opened.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use naivepost::ui;

use crate::common;

/// A project: one footage recording tagged as narrator 1, a cut of two clips (4 s of finished video), one line.
fn project(root: &Path, name: &str, with_line: bool) -> PathBuf {
    let dir = root.join(format!("{name}.naivepost"));
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    std::fs::write(dir.join("sources/take.mkv"), b"stub").unwrap();
    std::fs::write(
        dir.join("naivepost.json"),
        r#"{"sources": [{"path": "project:sources/take.mkv", "footage": true, "narrator": 1, "sepvoice": false, "tracks": []}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("cut")).unwrap();
    std::fs::write(
        dir.join("cut/cut.json"),
        r#"{"segs": [{"s": 1.0, "e": 3.0}, {"s": 5.0, "e": 7.0}]}"#,
    )
    .unwrap();
    let tree = naivepost::layout::Tree::new(&dir).unwrap();
    let narration = naivepost::narration::Narration {
        entries: if with_line {
            vec![naivepost::narration::Entry {
                s: 1.0,
                e: 3.0,
                text: "Welcome back".into(),
                ..Default::default()
            }]
        } else {
            Vec::new()
        },
        ..Default::default()
    };
    naivepost::narration::save(&narration, &tree).unwrap();
    dir
}

#[test]
fn sec_07_narrate_1_screen_the_page_opens_on_the_projects_cut_lines_and_voice() {
    common::pin_private_cwd();
    let root = common::temp_dir("narrate-opens");
    let first = project(&root, "first", true);
    let second = project(&root, "second", false);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&first).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &first);
        window.present();
        common::settle();
        let state = ui::narrate_page::read_state();
        assert_eq!(state.segs.len(), 2, "the page knows the cut's two clips");
        assert!(
            (state.length - 4.0).abs() < 1e-9,
            "and the finished video's length: {}",
            state.length
        );
        assert_eq!(state.entries.len(), 1, "and the project's line");
        assert_eq!(state.narrators, 1, "Prepare tagged narrator 1");
        let clock = common::find_named(&window, "narrate-clock")
            .and_then(|w| w.downcast::<gtk4::Label>().ok())
            .expect("the clock is on the page");
        assert!(
            clock.text().ends_with("/00:04"),
            "the clock reads the cut's length: {}",
            clock.text()
        );
        let picker = common::find_named(&window, "voice-picker")
            .and_then(|w| w.downcast::<gtk4::DropDown>().ok())
            .expect("the picker is on the page");
        assert_ne!(
            picker.selected(),
            gtk4::INVALID_LIST_POSITION,
            "the picker shows the project's own narrator"
        );
        // §1 **10**: the take band, where the sample's seconds are picked, shows for the project's own narrator.
        // With no `voice.txt` the voice is `own`, narrator 1's id; the band asked a copy of the slot rule that did
        // not know that, found no recording and hid itself.
        let band =
            common::find_named(&window, "take-band-scroller").expect("the band is on the page");
        assert!(
            band.is_visible(),
            "the take band shows for narrator 1's recording"
        );
        // §1 **13**: the pitch slider has a track a hand can drag along.
        let pitch = common::find_named(&window, "pitch-slider").expect("the pitch slider");
        // The widget's whole box (`width()` is its content, inside the scale's own padding).
        let wide = pitch.compute_bounds(&pitch).map_or(0.0, |b| b.width());
        assert!(
            wide >= ui::narrate_pitch::PITCH_SLIDER_MIN_PX as f32,
            "the pitch slider is {wide} px wide, under its {} px floor",
            ui::narrate_pitch::PITCH_SLIDER_MIN_PX
        );
        // Opening is not a pick: the stored "own" voice IS narrator 1's row, so the picker settling on it must
        // neither write `voice.txt` nor report a switch nobody made.
        let tree = naivepost::layout::Tree::new(&first).unwrap();
        assert!(
            !tree.voice_txt().exists(),
            "opening the page chose no voice for the person"
        );
        assert!(
            !ui::state(&window).status.starts_with("voice:"),
            "and said no switch: {}",
            ui::state(&window).status
        );
        window.close();
        common::settle();

        // Another project opened after it: its own (no) lines, not the first one's.
        let loaded = naivepost::project::load(&second).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Narrate", &second);
        window.present();
        common::settle();
        assert!(
            ui::narrate_page::read_state().entries.is_empty(),
            "the second project opens on its own lines, not the first one's"
        );
        window.close();
    });
}
